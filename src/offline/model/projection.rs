//! Autotune two exact full-vocabulary reductions on the actual CPU.
//! Compare at most two pairs per row-count bucket: discard the first pair's
//! timing, then decide from the second pair with conservative hysteresis.
use candle_core::{quantized::QMatMul, Module, Result, Tensor};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default)]
enum Choice {
    #[default]
    Warmup,
    Probe,
    Dense,
    Fused,
}
impl Choice {
    fn observe(self, matches: bool, dense: Duration, fused: Duration) -> Self {
        if !matches {
            return Self::Dense;
        }
        match self {
            Self::Warmup => Self::Probe,
            Self::Probe if fused.as_secs_f64() < dense.as_secs_f64() * 0.95 => Self::Fused,
            Self::Probe => Self::Dense,
            Self::Dense | Self::Fused => self,
        }
    }
}
#[derive(Clone, Default)]
pub(super) struct Projection {
    choices: Arc<Mutex<[Choice; 8]>>,
}
impl Projection {
    fn dense(
        weight: &QMatMul,
        input: &Tensor,
        vocab: usize,
        eos: u32,
        blocked: &[HashSet<u32>],
    ) -> Result<Vec<u32>> {
        let scores = weight.forward(input)?;
        blocked
            .iter()
            .enumerate()
            .map(|(row, banned)| {
                super::super::selection::select_logits(&scores.get(row)?, vocab, eos, banned)
                    .map_err(|error| candle_core::Error::Msg(error.to_string()))
            })
            .collect()
    }
    fn record(&self, bucket: usize, choice: Choice) -> Result<()> {
        self.choices
            .lock()
            .map_err(|_| candle_core::Error::Msg("projection policy lock poisoned".into()))?
            [bucket] = choice;
        Ok(())
    }
    pub(super) fn forward(
        &self,
        weight: &QMatMul,
        input: &Tensor,
        vocab: usize,
        eos: u32,
        blocked: &[HashSet<u32>],
    ) -> Result<Vec<u32>> {
        let (rows, _) = input.dims2()?;
        if rows == 0 || rows != blocked.len() {
            candle_core::bail!("projection row mismatch");
        }
        if rows == 1 {
            return Self::dense(weight, input, vocab, eos, blocked);
        }
        let bucket = ((rows - 1).ilog2() as usize).min(7);
        let choice = self
            .choices
            .lock()
            .map_err(|_| candle_core::Error::Msg("projection policy lock poisoned".into()))?
            [bucket];
        match choice {
            Choice::Dense => Self::dense(weight, input, vocab, eos, blocked),
            Choice::Fused => match weight.forward_top1(input, vocab, eos, blocked)? {
                Some(tokens) => Ok(tokens),
                None => Self::dense(weight, input, vocab, eos, blocked),
            },
            Choice::Warmup | Choice::Probe => {
                let dense = || {
                    let start = Instant::now();
                    Self::dense(weight, input, vocab, eos, blocked)
                        .map(|result| (result, start.elapsed()))
                };
                let fused = || {
                    let start = Instant::now();
                    weight
                        .forward_top1(input, vocab, eos, blocked)
                        .map(|result| (result, start.elapsed()))
                };
                let ((expected, td), (candidate, tf)) = if matches!(choice, Choice::Warmup) {
                    let d = dense()?;
                    (d, fused()?)
                } else {
                    let f = fused()?;
                    (dense()?, f)
                };
                let next = choice.observe(candidate.as_ref() == Some(&expected), td, tf);
                self.record(bucket, next)?;
                // Return the reference throughout calibration. No approximation
                // is accepted if an unexpected backend disagreement occurs.
                Ok(expected)
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warmup_timing_is_discarded_and_second_pair_finishes_calibration() {
        let micros = Duration::from_micros;
        for (warmup_dense, warmup_fused) in [(100, 1), (100, 999)] {
            let choice = Choice::Warmup.observe(true, micros(warmup_dense), micros(warmup_fused));
            assert!(matches!(choice, Choice::Probe));
            assert!(matches!(
                choice.observe(true, micros(100), micros(80)),
                Choice::Fused
            ));
            assert!(matches!(
                choice.observe(true, micros(100), micros(97)),
                Choice::Dense
            ));
        }
    }
    #[test]
    fn fused_requires_more_than_five_percent_gain() {
        let micros = Duration::from_micros;
        for fused in [95, 97, 100, 200] {
            assert!(matches!(
                Choice::Probe.observe(true, micros(100), micros(fused)),
                Choice::Dense
            ));
        }
        assert!(matches!(
            Choice::Probe.observe(true, micros(100), micros(94)),
            Choice::Fused
        ));
    }
    #[test]
    fn any_disagreement_selects_dense_even_when_fused_is_faster() {
        for choice in [Choice::Warmup, Choice::Probe] {
            assert!(matches!(
                choice.observe(false, Duration::from_secs(1), Duration::ZERO),
                Choice::Dense
            ));
        }
    }
}
