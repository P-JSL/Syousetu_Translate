//! Full-vocabulary greedy selection using device-side reductions. A second
//! pass is required only for rows in which signed zero can affect ordering.
use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use std::collections::HashSet;

#[derive(Clone)]
pub(in crate::offline) struct TokenIds(Tensor);
impl TokenIds {
    pub(in crate::offline) fn new(width: usize, device: &Device) -> candle_core::Result<Self> {
        let end = u32::try_from(width)
            .map_err(|_| candle_core::Error::Msg("selection vocabulary is too large".into()))?;
        Ok(Self(Tensor::arange(0u32, end, device)?))
    }
}

pub(in crate::offline) fn select_rows(
    scores: &Tensor,
    vocab: usize,
    eos: u32,
    blocked: &[HashSet<u32>],
) -> Result<Vec<u32>> {
    let width = scores.dim(1)?.min(vocab);
    let ids = TokenIds::new(width, scores.device())?;
    select_with_ids(scores, vocab, eos, blocked, &ids)
}

pub(in crate::offline) fn select_with_ids(
    scores: &Tensor,
    vocab: usize,
    eos: u32,
    blocked: &[HashSet<u32>],
    ids: &TokenIds,
) -> Result<Vec<u32>> {
    let (rows, columns) = scores.dims2()?;
    let width = columns.min(vocab);
    anyhow::ensure!(rows > 0 && rows == blocked.len(), "projection row mismatch");
    anyhow::ensure!(
        matches!(scores.dtype(), DType::F16 | DType::F32),
        "GPU selection requires F16 or F32 logits"
    );
    anyhow::ensure!(width > 0, "모델이 유효한 출력 토큰을 생성하지 못했습니다");
    let sentinel = u32::try_from(width).context("selection vocabulary is too large")?;
    let status_limit = sentinel
        .checked_mul(2)
        .context("selection vocabulary is too large")?;
    let count = rows.checked_mul(width).context("selection size overflow")?;
    u32::try_from(count).context("selection batch is too large")?;
    let device = scores.device();
    let ids = ids.0.narrow(0, 0, width)?;
    let excluded = Tensor::new(f32::NEG_INFINITY, device)?.to_dtype(scores.dtype())?;
    let scores = scores.narrow(1, 0, width)?;
    // +inf and NaN fail this comparison. -inf is already the exclusion value.
    let flat = scores
        .lt(f32::INFINITY)?
        .where_cond(&scores, &excluded.broadcast_as((rows, width))?)?
        .flatten_all()?;

    let mut indices = Vec::new();
    for (row, banned) in blocked.iter().enumerate() {
        let offset = (row * width) as u32;
        indices.extend(
            banned
                .iter()
                .filter(|&&id| id < sentinel)
                .map(|&id| offset + id),
        );
        indices.extend(
            (0..sentinel.min(4))
                .filter(|id| *id != eos && !banned.contains(id))
                .map(|id| offset + id),
        );
    }
    if !indices.is_empty() {
        let length = indices.len();
        let indices = Tensor::from_vec(indices, length, device)?;
        let values = excluded.broadcast_as(length)?.contiguous()?;
        // Special and banned IDs are disjoint and rows have distinct spans.
        // flat owns a fresh allocation, so input logits remain unchanged.
        flat.scatter_set(&indices, &values, 0)?;
    }
    let masked = flat.reshape((rows, width))?;
    drop(flat);
    let absent = Tensor::new(sentinel, device)?;
    let maximum = masked.max_keepdim(1)?;
    let first = lowest_id(&masked.broadcast_eq(&maximum)?, &ids, &absent)?;
    // A width offset marks zero maxima. One small readback also reports
    // exhausted rows, without any scores leaving the device.
    let maximum = maximum.squeeze(1)?;
    let valid = maximum.gt(f32::NEG_INFINITY)?.mul(&first.lt(sentinel)?)?;
    let first = maximum
        .eq(0f32)?
        .where_cond(&first.broadcast_add(&absent)?, &first)?;
    let mut result = valid
        .where_cond(&first, &Tensor::new(u32::MAX, device)?.broadcast_as(rows)?)?
        .to_vec1::<u32>()?;
    anyhow::ensure!(
        result.iter().all(|&id| id < status_limit),
        "모델이 유효한 출력 토큰을 생성하지 못했습니다"
    );

    let zero_rows = result
        .iter_mut()
        .enumerate()
        .filter_map(|(row, id)| {
            if *id < sentinel {
                return None;
            }
            *id -= sentinel;
            Some(row as u32)
        })
        .collect::<Vec<_>>();
    if !zero_rows.is_empty() {
        // Numerical equality conflates the two zeros. For marked rows only,
        // look for the first +0. If there is none, the first -0 already won.
        // The equality check also excludes nonzero reciprocal overflow.
        let indexes = Tensor::from_slice(&zero_rows, zero_rows.len(), device)?;
        let scores = masked.index_select(&indexes, 0)?.to_dtype(DType::F32)?;
        let positive_zero = scores.eq(0f32)?.mul(&scores.recip()?.eq(f32::INFINITY)?)?;
        let plus_ids = lowest_id(&positive_zero, &ids, &absent)?.to_vec1::<u32>()?;
        for (row, positive) in zero_rows.into_iter().zip(plus_ids) {
            if positive < sentinel {
                result[row as usize] = positive;
            }
        }
    }
    Ok(result)
}

fn lowest_id(eligible: &Tensor, ids: &Tensor, absent: &Tensor) -> candle_core::Result<Tensor> {
    eligible
        .where_cond(
            &ids.broadcast_as(eligible.shape())?,
            &absent.broadcast_as(eligible.shape())?,
        )?
        .min(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(
        scores: &[Vec<f32>],
        vocab: usize,
        eos: u32,
        blocked: &[HashSet<u32>],
    ) -> Option<Vec<u32>> {
        scores
            .iter()
            .zip(blocked)
            .map(|(row, blocked)| super::super::select_scalar(row, vocab, eos, blocked))
            .collect()
    }

    #[test]
    fn tensor_selection_matches_scalar_for_nonfinite_scores_special_ids_and_bans() {
        let values = vec![
            vec![
                100.,
                f32::NAN,
                2.,
                f32::INFINITY,
                2.,
                2.,
                f32::NEG_INFINITY,
                1.,
            ],
            vec![100., 100., 1., 100., 5., 5., 4., f32::NAN],
        ];
        let scores = Tensor::new(values.clone(), &Device::Cpu).unwrap();
        for eos in [0, 2, 4, 99] {
            for blocked in [
                vec![HashSet::new(); 2],
                vec![[0, 2, 4, 7, 100].into(), [0, 4, 5].into()],
            ] {
                for vocab in [4, 7, 8, 10] {
                    let expected = reference(&values, vocab, eos, &blocked);
                    let actual = select_rows(&scores, vocab, eos, &blocked);
                    match expected {
                        Some(expected) => assert_eq!(actual.unwrap(), expected),
                        None => assert!(actual.is_err()),
                    }
                }
            }
        }
        // In-place sparse masking must not mutate the caller's logits.
        assert!(scores.to_vec2::<f32>().unwrap()[0][1].is_nan());
        assert_eq!(scores.to_vec2::<f32>().unwrap()[1][4], 5.);
    }

    #[test]
    fn signed_zero_and_equal_scores_have_explicit_total_order_and_lower_id_ties() {
        let values: Vec<Vec<f32>> = vec![
            vec![-1., -1., -0., -1., 0., 0., -0., -1.],
            vec![-1., -1., 0., -1., -0., 0., -0., -1.],
            vec![-1., -1., -0., -1., -0., -0., -0., -1.],
        ];
        let scores = Tensor::new(values, &Device::Cpu).unwrap();
        let unblocked: [HashSet<u32>; 3] = std::array::from_fn(|_| HashSet::new());
        assert_eq!(select_rows(&scores, 8, 2, &unblocked).unwrap(), [4, 2, 2]);
        let blocked = vec![[4, 5].into(), [2, 5].into(), [2].into()];
        assert_eq!(select_rows(&scores, 8, 2, &blocked).unwrap(), [2, 4, 4]);
    }

    #[test]
    fn reciprocal_overflow_does_not_treat_small_nonzero_scores_as_zero() {
        let values = vec![
            vec![-1., -1., -1., -1., f32::from_bits(1), f32::from_bits(2)],
            vec![-1., -1., -1., -1., 0., 1.],
        ];
        let scores = Tensor::new(values, &Device::Cpu).unwrap();
        assert_eq!(
            select_rows(&scores, 6, 2, &[HashSet::new(), HashSet::new()]).unwrap(),
            [5, 5]
        );
    }

    #[test]
    fn offset_and_noncontiguous_views_match_each_original_scalar_row() {
        let mut seed = 3u64;
        for width in [7usize, 64, 65, 257] {
            let source = (0..5 * (width + 2))
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    f32::from_bits((seed >> 32) as u32)
                })
                .collect::<Vec<_>>();
            let scores = Tensor::from_vec(source, (5, width + 2), &Device::Cpu)
                .unwrap()
                .narrow(0, 1, 3)
                .unwrap()
                .narrow(1, 1, width)
                .unwrap();
            let blocked = vec![[2, 4, 6, 10].into(), [5, 7, 11].into(), HashSet::new()];
            let values = scores.to_vec2::<f32>().unwrap();
            let expected = reference(&values, width - 1, 2, &blocked).unwrap();
            assert_eq!(
                select_rows(&scores, width - 1, 2, &blocked).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn empty_invalid_and_fully_blocked_rows_are_errors() {
        let scores = Tensor::new(
            &[[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0., 1.]],
            &Device::Cpu,
        )
        .unwrap();
        assert!(select_rows(&scores, 5, 2, &[[4].into()]).is_err());
        assert!(select_rows(&scores, 0, 2, &[HashSet::new()]).is_err());
        assert!(select_rows(&scores, 5, 2, &[]).is_err());
        let integer = Tensor::new(&[[1u32; 5]], &Device::Cpu).unwrap();
        assert!(select_rows(&integer, 5, 2, &[HashSet::new()]).is_err());
    }

    fn compare_half_logits(device: &Device) {
        let min_half = 2f32.powi(-24);
        let scores = Tensor::new(
            &[
                [99., f32::NAN, -0., 99., 0., 0., -2., f32::NEG_INFINITY],
                [
                    99.,
                    99.,
                    f32::INFINITY,
                    99.,
                    min_half,
                    2. * min_half,
                    -min_half,
                    0.,
                ],
                [99., 99., 1., 99., 1., 1., 2., -1.],
            ],
            device,
        )
        .unwrap()
        .to_dtype(DType::F16)
        .unwrap();
        let banned = [[4].into(), HashSet::new(), [6].into()];
        let ids = TokenIds::new(16, device).unwrap();
        let selected = select_with_ids(&scores, 8, 2, &banned, &ids).unwrap();
        assert_eq!(selected, [5, 5, 2]);
    }

    #[test]
    fn retained_f16_logits_preserve_subnormals_ties_and_masked_zeros() {
        compare_half_logits(&Device::Cpu);
    }

    #[cfg(feature = "cuda")]
    #[test]
    #[ignore = "requires a real CUDA device and driver"]
    fn cuda_half_greedy_selection_matches_reference_on_device() {
        compare_half_logits(&Device::new_cuda(0).unwrap());
    }
}
