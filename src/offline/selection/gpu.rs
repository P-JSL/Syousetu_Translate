//! Device-resident full-vocabulary greedy selection using standard Tensor ops.
//! Token-index metadata is uploaded and one winner ID per row is
//! downloaded. No dense logits are copied to the CPU. CPU execution of these
//! same operations provides a test oracle before hardware integration tests.
use anyhow::{Context, Result};
use candle_core::{DType, Tensor};
use std::collections::HashSet;

pub(in crate::offline) fn select_rows(
    scores: &Tensor,
    vocab: usize,
    eos: u32,
    blocked: &[HashSet<u32>],
) -> Result<Vec<u32>> {
    let (rows, columns) = scores.dims2()?;
    let width = columns.min(vocab);
    anyhow::ensure!(rows > 0 && rows == blocked.len(), "projection row mismatch");
    anyhow::ensure!(
        scores.dtype() == DType::F32,
        "GPU selection requires F32 logits"
    );
    anyhow::ensure!(width > 0, "모델이 유효한 출력 토큰을 생성하지 못했습니다");
    let sentinel = u32::try_from(width).context("selection vocabulary is too large")?;
    let count = rows.checked_mul(width).context("selection size overflow")?;
    u32::try_from(count).context("selection batch is too large")?;
    let device = scores.device();
    let ids = Tensor::arange(0u32, sentinel, device)?;
    let flat = {
        let scores = scores.narrow(1, 0, width)?;
        // Values 1 and 2 are both true to where_cond; eos may also be >= 4.
        let special_allowed = ids.ge(4f64)?.add(&ids.eq(eos as f64)?)?;
        let finite = scores.abs()?.lt(f32::INFINITY as f64)?;
        let allowed = finite.broadcast_mul(&special_allowed)?;
        let excluded = Tensor::full(f32::NEG_INFINITY, (rows, width), device)?;
        allowed.where_cond(&scores, &excluded)?.flatten_all()?
    };

    let indices = blocked
        .iter()
        .enumerate()
        .flat_map(|(row, ids)| {
            ids.iter()
                .copied()
                .filter(move |&id| (id as usize) < width)
                .map(move |id| (row * width + id as usize) as u32)
        })
        .collect::<Vec<_>>();
    if !indices.is_empty() {
        let length = indices.len();
        let indices = Tensor::from_vec(indices, length, device)?;
        let values = Tensor::full(f32::NEG_INFINITY, length, device)?;
        // Indices are unique within each HashSet and rows have disjoint spans.
        // flat owns a fresh allocation, so input logits remain unchanged.
        flat.scatter_set(&indices, &values, 0)?;
    }
    let masked = flat.reshape((rows, width))?;
    drop(flat);
    let winners = {
        let maximum = masked.max_keepdim(1)?;
        let valid_row = maximum.abs()?.lt(f32::INFINITY as f64)?;
        let candidates = masked.broadcast_eq(&maximum)?.broadcast_mul(&valid_row)?;

        // Numeric max/equality treats -0 and +0 as equal. The CPU contract uses
        // total_cmp, where +0 ranks above -0. Reciprocal detects positive zero
        // without assuming how a backend's max/argmax kernel resolves signed ties.
        // Exclude tiny nonzero inputs whose reciprocal overflows to infinity.
        let positive_zero = candidates
            .mul(&masked.eq(0f64)?)?
            .mul(&masked.recip()?.eq(f32::INFINITY as f64)?)?;
        let prefer_positive_zero = positive_zero
            .to_dtype(DType::F32)?
            .max_keepdim(1)?
            .gt(0f64)?;
        prefer_positive_zero
            .broadcast_as((rows, width))?
            .where_cond(&positive_zero, &candidates)?
    };
    // Release the dense F32 mask before allocating U32 tie-break buffers.
    drop(masked);

    // Explicit ID minimum gives stable lower-ID tie breaking on every backend.
    // For all-masked/non-finite rows, no candidate remains and sentinel survives.
    let ids = ids.broadcast_as((rows, width))?;
    let absent = Tensor::full(sentinel, (rows, width), device)?;
    let result = winners
        .where_cond(&ids, &absent)?
        .min(1)?
        .to_vec1::<u32>()?;
    anyhow::ensure!(
        result.iter().all(|&id| id < sentinel),
        "모델이 유효한 출력 토큰을 생성하지 못했습니다"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;

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
}
