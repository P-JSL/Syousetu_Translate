// SPDX-License-Identifier: Apache-2.0
//! CPU self-attention on the encoder's combined Q/K/V projection.
use super::{attend_head, storage, KeyValue, Scratch, HEAD_PARALLEL_WORK, SCRATCH};
use candle_core::{Device, Result, Tensor};
use rayon::prelude::*;

pub(in crate::offline::model) fn prefer_encoder_context(
    segments: &[(usize, usize)],
    width: usize,
) -> bool {
    let mut total: usize = 0;
    let mut longest = 0;
    for &(_, rows) in segments {
        let work = rows.saturating_mul(rows);
        total = total.saturating_add(work);
        longest = longest.max(work);
    }
    // Direct writes let sentences run independently. If one long sentence
    // exceeds the ideal per-worker share, keep parallelism across its heads.
    longest.saturating_mul(width) < HEAD_PARALLEL_WORK
        || longest <= total.div_ceil(rayon::current_num_threads())
}

pub(in crate::offline::model) fn encoder_context(
    qkv: &Tensor,
    segments: &[(usize, usize)],
    heads: usize,
    head_dim: usize,
) -> Result<Tensor> {
    let (batch, total, projected_width) = qkv.dims3()?;
    if batch != 1
        || total == 0
        || heads == 0
        || head_dim == 0
        || heads
            .checked_mul(head_dim)
            .and_then(|width| width.checked_mul(3))
            != Some(projected_width)
    {
        candle_core::bail!("encoder attention shape mismatch");
    }
    let width = projected_width / 3;
    let (data, layout) = qkv.storage_and_layout();
    let data = storage(&data, layout)?;
    let mut output = vec![0f32; total * width];
    let mut remaining = output.as_mut_slice();
    let mut next = 0;
    let mut work = Vec::with_capacity(segments.len());
    for &(start, rows) in segments {
        if start != next || rows == 0 || rows > total - next || rows.checked_mul(rows).is_none() {
            candle_core::bail!("invalid encoder attention segment");
        }
        next += rows;
        let source = &data[start * projected_width..next * projected_width];
        let (destination, rest) = remaining.split_at_mut(rows * width);
        remaining = rest;
        work.push((source, destination, rows));
    }
    if next != total {
        candle_core::bail!("encoder attention segments do not cover projection");
    }

    let scale = (head_dim as f64).powf(-0.5) as f32;
    work.into_par_iter()
        .for_each(|(source, destination, rows)| {
            SCRATCH.with(|cell| {
                let mut scratch = cell.borrow_mut();
                let Scratch { query, scores } = &mut *scratch;
                query.resize(rows * head_dim, 0.0);
                for head in 0..heads {
                    let column = head * head_dim;
                    for (input, scaled) in source
                        .chunks_exact(projected_width)
                        .zip(query.chunks_exact_mut(head_dim))
                    {
                        for (dst, &value) in scaled.iter_mut().zip(&input[column..][..head_dim]) {
                            *dst = value * scale;
                        }
                    }
                    let kv = KeyValue {
                        key: &source[width + column..],
                        value: &source[2 * width + column..],
                        length: rows,
                        stride: projected_width,
                    };
                    attend_head(
                        query,
                        kv,
                        head_dim,
                        scores,
                        &mut destination[column..],
                        width,
                    );
                }
            });
        });
    Tensor::from_vec(output, (1, total, width), &Device::Cpu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_context_matches_separate_tensors_for_segment_tails() {
        let heads = 16;
        let dim = 64;
        let width = heads * dim;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        pool.install(|| {
            assert!(prefer_encoder_context(&[(0, 24), (24, 24)], width));
            assert!(!prefer_encoder_context(&[(0, 8), (8, 192)], width));
            assert!(prefer_encoder_context(
                &[(0, 64), (64, 64), (128, 64), (192, 64)],
                width,
            ));
        });
        for lengths in [&[1, 3, 17, 32, 63, 128][..], &[192]] {
            let total: usize = lengths.iter().sum();
            let qkv = Tensor::from_vec(
                (0..total * width * 3)
                    .map(|i| ((i * 1723 % 1009) as f32 - 504.0) / 137.0)
                    .collect(),
                (1, total, width * 3),
                &Device::Cpu,
            )
            .unwrap();
            let mut start = 0;
            let mut segments = Vec::new();
            let mut expected = Vec::new();
            for &rows in lengths {
                let projection = qkv.narrow(1, start, rows).unwrap();
                let head_rows = |column| {
                    projection
                        .narrow(2, column, width)?
                        .reshape((1, rows, heads, dim))?
                        .transpose(1, 2)?
                        .contiguous()?
                        .reshape((heads, rows, dim))
                };
                expected.push(
                    super::super::context(
                        &head_rows(0).unwrap(),
                        &head_rows(width).unwrap(),
                        &head_rows(2 * width).unwrap(),
                        0.125,
                    )
                    .unwrap(),
                );
                segments.push((start, rows));
                start += rows;
            }
            let expected = Tensor::cat(&expected, 1)
                .unwrap()
                .flatten_all()
                .unwrap()
                .to_vec1::<f32>()
                .unwrap();
            let actual = pool
                .install(|| encoder_context(&qkv, &segments, heads, dim))
                .unwrap()
                .flatten_all()
                .unwrap()
                .to_vec1::<f32>()
                .unwrap();
            assert_eq!(actual.len(), expected.len());
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_eq!(actual.to_bits(), expected.to_bits(), "element {index}");
            }
        }
    }
}
