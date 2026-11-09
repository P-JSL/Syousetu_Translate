//! Greedy decoding over borrowed CPU logits. No full-vocabulary copy is needed.
use anyhow::{Context, Result};
use candle_core::{CpuStorage, Storage, Tensor};
use std::collections::HashSet;

pub(super) fn select_logits(
    scores: &Tensor,
    vocab: usize,
    eos: u32,
    blocked: &HashSet<u32>,
) -> Result<u32> {
    anyhow::ensure!(scores.rank() == 1, "출력 점수는 1차원이어야 합니다");
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

pub(super) fn select(
    scores: &[f32],
    vocab: usize,
    eos: u32,
    blocked: &HashSet<u32>,
) -> Option<u32> {
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
    fn tensor_view_respects_storage_offsets_and_rejects_wrong_shapes() {
        let t = Tensor::new(&[100f32, 0., 0., 1., 0., 2., 3.], &candle_core::Device::Cpu).unwrap();
        let view = t.narrow(0, 1, 6).unwrap();
        assert_eq!(select_logits(&view, 6, 2, &HashSet::new()).unwrap(), 5);
        assert!(select_logits(&view.unsqueeze(0).unwrap(), 6, 2, &HashSet::new()).is_err());
    }
}
