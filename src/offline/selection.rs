//! Shared greedy decoding over borrowed CPU logits; no full-vocabulary copy.
use anyhow::{Context, Result};
use candle_core::{CpuStorage, Storage, Tensor};
use rayon::prelude::*;
use std::collections::HashSet;
pub(super) mod gpu;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[allow(unsafe_code)]
mod simd;

pub(super) fn select_logits(
    scores: &Tensor,
    vocab: usize,
    eos: u32,
    blocked: &HashSet<u32>,
) -> Result<u32> {
    anyhow::ensure!(scores.rank() == 1, "출력 점수는 1차원이어야 합니다");
    if !scores.device().is_cpu() {
        return gpu::select_rows(
            &scores.unsqueeze(0)?,
            vocab,
            eos,
            std::slice::from_ref(blocked),
        )?
        .into_iter()
        .next()
        .context("모델이 유효한 출력 토큰을 생성하지 못했습니다");
    }
    let (storage, layout) = scores.storage_and_layout();
    let Storage::Cpu(CpuStorage::F32(values)) = &*storage else {
        anyhow::bail!("출력 점수는 CPU F32여야 합니다");
    };
    let (start, end) = layout
        .contiguous_offsets()
        .context("출력 점수가 연속 메모리가 아닙니다")?;
    select(&values[start..end], vocab, eos, blocked)
        .context("모델이 유효한 출력 토큰을 생성하지 못했습니다")
}

pub(super) fn select_cpu_rows(
    scores: &Tensor,
    vocab: usize,
    eos: u32,
    blocked: &[HashSet<u32>],
) -> Result<Vec<u32>> {
    let (rows, width) = scores.dims2()?;
    anyhow::ensure!(
        width != 0 && rows == blocked.len(),
        "출력 점수 행의 크기가 다릅니다"
    );
    let (storage, layout) = scores.storage_and_layout();
    let Storage::Cpu(CpuStorage::F32(values)) = &*storage else {
        anyhow::bail!("출력 점수는 CPU F32여야 합니다");
    };
    let (start, end) = layout
        .contiguous_offsets()
        .context("출력 점수가 연속 메모리가 아닙니다")?;
    let matrix = &values[start..end];
    let choose = |(scores, blocked): (&[f32], &HashSet<u32>)| {
        select(scores, vocab, eos, blocked).context("모델이 유효한 출력 토큰을 생성하지 못했습니다")
    };
    if rows > 1 && matrix.len() >= 262_144 && rayon::current_num_threads() > 1 {
        matrix
            .par_chunks_exact(width)
            .zip(blocked)
            .map(choose)
            .collect()
    } else {
        matrix
            .chunks_exact(width)
            .zip(blocked)
            .map(choose)
            .collect()
    }
}

pub(super) fn select(
    scores: &[f32],
    vocab: usize,
    eos: u32,
    blocked: &HashSet<u32>,
) -> Option<u32> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if scores.len().min(vocab) >= 64 {
        if let Some(id) = simd::maximum(scores, vocab, eos) {
            if !blocked.contains(&id) {
                return Some(id);
            }
        }
    }
    select_scalar(scores, vocab, eos, blocked)
}

fn select_scalar(scores: &[f32], vocab: usize, eos: u32, blocked: &HashSet<u32>) -> Option<u32> {
    let mut best: Option<(u32, f32)> = None;
    for (id, &score) in scores.iter().take(vocab).enumerate() {
        if (id < 4 && id != eos as usize) || !score.is_finite() {
            continue;
        }
        // Scan in token order: equal scores retain the lower token ID. Test
        // membership only for a possible winner, rather than every vocab item.
        if best.is_none_or(|(_, current)| score.total_cmp(&current).is_gt())
            && !blocked.contains(&(id as u32))
        {
            best = Some((id as u32, score));
        }
    }
    best.map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(scores: &[f32], vocab: usize, eos: u32, blocked: &HashSet<u32>) -> Option<u32> {
        scores
            .iter()
            .enumerate()
            .filter(|(id, s)| {
                (*id >= 4 || *id == eos as usize)
                    && *id < vocab
                    && s.is_finite()
                    && !blocked.contains(&(*id as u32))
            })
            .max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(id, _)| id as u32)
    }
    #[test]
    fn matches_reference_for_ties_special_values_vocabulary_bounds_and_blocking() {
        let values = [
            f32::NAN,
            f32::NEG_INFINITY,
            -0.0,
            0.0,
            f32::INFINITY,
            -2.,
            2.,
            2.,
            3.,
        ];
        for offset in 0..values.len() {
            let scores = values
                .iter()
                .cycle()
                .skip(offset)
                .take(23)
                .copied()
                .collect::<Vec<_>>();
            for vocab in 0..=24 {
                for eos in [0, 2, 4, 30] {
                    for blocked in [
                        HashSet::new(),
                        [2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22].into(),
                    ] {
                        assert_eq!(
                            select(&scores, vocab, eos, &blocked),
                            reference(&scores, vocab, eos, &blocked)
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn vector_selection_matches_reference_at_boundaries_and_with_blocked_maxima() {
        let mut seed = 3u64;
        for length in [63usize, 64, 65, 127, 128, 1025, 128100] {
            let mut scores = (0..length)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    f32::from_bits((seed >> 32) as u32)
                })
                .collect::<Vec<_>>();
            scores[2] = f32::MAX;
            scores[4] = -0.0;
            scores[5] = 0.0;
            scores[17] = f32::MAX;
            scores[33] = f32::MAX;
            for vocab in [
                0,
                3,
                63,
                64,
                65,
                length.saturating_sub(1),
                length,
                length + 1,
            ] {
                for eos in [0, 2, 4, 30] {
                    for blocked in [HashSet::new(), [2, 17].into(), [2, 17, 33].into()] {
                        assert_eq!(
                            select(&scores, vocab, eos, &blocked),
                            reference(&scores, vocab, eos, &blocked)
                        );
                    }
                }
            }
            for value in [-0.0, 0.0, f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
                scores.fill(value);
                assert_eq!(
                    select(&scores, length, 2, &HashSet::new()),
                    reference(&scores, length, 2, &HashSet::new())
                );
            }
        }
    }
    #[test]
    fn tensor_view_respects_storage_offsets_and_rejects_wrong_shapes() {
        let t = Tensor::new(&[100f32, 0., 0., 1., 0., 2., 3.], &candle_core::Device::Cpu).unwrap();
        let view = t.narrow(0, 1, 6).unwrap();
        assert_eq!(select_logits(&view, 6, 2, &HashSet::new()).unwrap(), 5);
        assert!(select_logits(&view.unsqueeze(0).unwrap(), 6, 2, &HashSet::new()).is_err());
    }

    #[test]
    fn batched_selection_obeys_per_row_masks_in_original_order() -> Result<()> {
        let scores = Tensor::from_vec(
            vec![
                1f32, 2., 3., 4., 5., 6., 0., 0., 9., 5., 10., 10., 0., 0., 8., 2., 3., 9.,
            ],
            (3, 6),
            &candle_core::Device::Cpu,
        )?;
        let rows = scores.narrow(0, 1, 2)?;
        let blocked = [[4, 5].into(), [5].into()];
        assert_eq!(select_cpu_rows(&rows, 6, 2, &blocked)?, [2, 2]);
        assert!(select_cpu_rows(&rows, 6, 2, &blocked[..1]).is_err());
        Ok(())
    }
}
