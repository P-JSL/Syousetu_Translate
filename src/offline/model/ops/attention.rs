// SPDX-License-Identifier: Apache-2.0
//! Fused CPU attention with the same GEMM parameters/reductions as Candle.
//! Avoids intermediate Tensor/layout objects and reuses per-thread scratch.
use candle_core::{cpu::kernels::VecOps, CpuStorage, Device, Result, Storage, Tensor};
use rayon::prelude::*;
use std::cell::RefCell;

#[derive(Default)]
struct Scratch {
    query: Vec<f32>,
    scores: Vec<f32>,
}
thread_local! { static SCRATCH: RefCell<Scratch> = RefCell::default(); }

fn storage<'a>(storage: &'a Storage, layout: &candle_core::Layout) -> Result<&'a [f32]> {
    let Storage::Cpu(CpuStorage::F32(_)) = storage else {
        candle_core::bail!("attention requires CPU F32");
    };
    let Storage::Cpu(cpu) = storage else {
        unreachable!()
    };
    super::slice(cpu, layout)
}

pub(in crate::offline::model) fn context(
    q: &Tensor,
    k: &Tensor,
    v: &Tensor,
    scale: f64,
) -> Result<Tensor> {
    let (heads, rows, dim) = q.dims3()?;
    let (kh, length, kd) = k.dims3()?;
    if heads == 0
        || rows == 0
        || dim == 0
        || length == 0
        || heads != kh
        || dim != kd
        || v.dims() != [heads, length, dim]
    {
        candle_core::bail!("fused attention shape mismatch");
    }
    let score_len = rows
        .checked_mul(length)
        .ok_or_else(|| candle_core::Error::Msg("attention size overflow".into()))?;
    let query_len = rows * dim;
    let key_len = length * dim;
    let (qs, ql) = q.storage_and_layout();
    let (ks, kl) = k.storage_and_layout();
    let (vs, vl) = v.storage_and_layout();
    let qs = storage(&qs, ql)?;
    let ks = storage(&ks, kl)?;
    let vs = storage(&vs, vl)?;
    let mut output = vec![0f32; q.elem_count()];
    let run = |head: usize, dst: &mut [f32]| -> Result<()> {
        SCRATCH.with(|cell| {
            let mut scratch = cell.borrow_mut();
            let Scratch { query, scores } = &mut *scratch;
            query.resize(query_len, 0.0);
            scores.resize(score_len, 0.0);
            for (out, &input) in query
                .iter_mut()
                .zip(&qs[head * query_len..(head + 1) * query_len])
            {
                *out = input * scale as f32;
            }
            let key = &ks[head * key_len..(head + 1) * key_len];
            let value = &vs[head * key_len..(head + 1) * key_len];
            // SAFETY: shapes/contiguous storage were checked above. These are
            // exactly Candle CPU matmul's row/column strides, with a disjoint
            // initialized destination. Parallelism stays with the outer pool.
            unsafe {
                gemm::gemm(
                    rows,
                    length,
                    dim,
                    scores.as_mut_ptr(),
                    1,
                    length as isize,
                    false,
                    query.as_ptr(),
                    1,
                    dim as isize,
                    key.as_ptr(),
                    dim as isize,
                    1,
                    0f32,
                    1f32,
                    false,
                    false,
                    false,
                    gemm::Parallelism::None,
                );
            }
            for row in scores.chunks_mut(length) {
                let mut max = f32::NEG_INFINITY;
                // SAFETY: full initialized rows of exactly `length` elements.
                unsafe { f32::vec_reduce_max(row.as_ptr(), &mut max, length) };
                for item in row.iter_mut() {
                    *item = (*item - max).exp();
                }
                let mut sum = 0f32;
                unsafe { f32::vec_reduce_sum(row.as_ptr(), &mut sum, length) };
                for item in row.iter_mut() {
                    *item /= sum;
                }
            }
            // SAFETY: same checked shapes/strides as the second Candle matmul.
            unsafe {
                gemm::gemm(
                    rows,
                    dim,
                    length,
                    dst.as_mut_ptr(),
                    1,
                    dim as isize,
                    false,
                    scores.as_ptr(),
                    1,
                    length as isize,
                    value.as_ptr(),
                    1,
                    dim as isize,
                    0f32,
                    1f32,
                    false,
                    false,
                    false,
                    gemm::Parallelism::None,
                );
            }
            Ok(())
        })
    };
    if heads.saturating_mul(score_len).saturating_mul(dim) >= 1_000_000
        && rayon::current_num_threads() > 1
    {
        output
            .par_chunks_mut(query_len)
            .enumerate()
            .try_for_each(|(head, dst)| run(head, dst))?;
    } else {
        for (head, dst) in output.chunks_mut(query_len).enumerate() {
            run(head, dst)?;
        }
    }
    Tensor::from_vec(output, (1, heads, rows, dim), &Device::Cpu)?
        .transpose(1, 2)?
        .contiguous()?
        .reshape((1, rows, heads * dim))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fused_attention_matches_independent_tensor_reference_with_real_head_width() {
        let mut seed = 11u64;
        for (heads, rows, length) in [
            (1, 1, 1),
            (16, 1, 3),
            (16, 1, 17),
            (16, 1, 63),
            (2, 7, 17),
            (16, 23, 23),
            (2, 33, 67),
            (2, 128, 192),
        ] {
            let mut make = |a, b, c| {
                Tensor::from_vec(
                    (0..a * b * c)
                        .map(|_| {
                            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                            ((seed >> 32) as i32 as f32) / (i32::MAX as f32)
                        })
                        .collect::<Vec<_>>(),
                    (a, b, c),
                    &Device::Cpu,
                )
                .unwrap()
            };
            let q = make(heads, rows, 64);
            let k = make(heads, length, 64);
            let v = make(heads, length, 64);
            let scores = (&q * 0.125)
                .unwrap()
                .matmul(&k.transpose(1, 2).unwrap())
                .unwrap();
            let expected = super::super::softmax_last_dim(&scores)
                .unwrap()
                .matmul(&v)
                .unwrap()
                .reshape((1, heads, rows, 64))
                .unwrap()
                .transpose(1, 2)
                .unwrap()
                .contiguous()
                .unwrap()
                .reshape((1, rows, heads * 64))
                .unwrap();
            let actual = context(&q, &k, &v, 0.125).unwrap();
            assert_eq!(
                actual.flatten_all().unwrap().to_vec1::<f32>().unwrap(),
                expected.flatten_all().unwrap().to_vec1::<f32>().unwrap(),
                "shape {heads}/{rows}/{length}"
            );
        }
    }
}
