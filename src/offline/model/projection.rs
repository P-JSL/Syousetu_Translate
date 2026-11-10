//! Autotune two exact full-vocabulary reductions on the actual CPU.
//! Discard warmup; compare three paired samples for each row-count bucket.
use candle_core::{quantized::QMatMul, Module, Result, Tensor};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Samples {
    dense: [Duration; 3],
    fused: [Duration; 3],
    count: usize,
}
#[derive(Clone, Copy, Default)]
enum Choice {
    #[default]
    Warmup,
    Probe(Samples),
    Dense,
    Fused,
}
impl Samples {
    fn add(mut self, dense: Duration, fused: Duration) -> Choice {
        self.dense[self.count] = dense;
        self.fused[self.count] = fused;
        self.count += 1;
        if self.count != 3 {
            return Choice::Probe(self);
        }
        self.dense.sort_unstable();
        self.fused.sort_unstable();
        if self.fused[1].as_secs_f64() < self.dense[1].as_secs_f64() * 0.95 {
            Choice::Fused
        } else {
            Choice::Dense
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
            Choice::Warmup | Choice::Probe(_) => {
                let samples = match choice {
                    Choice::Probe(samples) => samples,
                    _ => Samples {
                        dense: [Duration::ZERO; 3],
                        fused: [Duration::ZERO; 3],
                        count: 0,
                    },
                };
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
                let ((expected, td), (candidate, tf)) = if samples.count.is_multiple_of(2) {
                    let d = dense()?;
                    (d, fused()?)
                } else {
                    let f = fused()?;
                    (dense()?, f)
                };
                let next = if candidate.as_ref() != Some(&expected) {
                    Choice::Dense
                } else if matches!(choice, Choice::Warmup) {
                    Choice::Probe(samples)
                } else {
                    samples.add(td, tf)
                };
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
    fn median_and_hysteresis_ignore_an_outlier_and_small_timing_noise() {
        let mut choice = Choice::Probe(Samples {
            dense: [Duration::ZERO; 3],
            fused: [Duration::ZERO; 3],
            count: 0,
        });
        for (dense, fused) in [(100, 80), (100, 999), (100, 80)] {
            let Choice::Probe(samples) = choice else {
                panic!("premature decision")
            };
            choice = samples.add(Duration::from_micros(dense), Duration::from_micros(fused));
        }
        assert!(matches!(choice, Choice::Fused));
        let samples = Samples {
            dense: [Duration::from_micros(100); 3],
            fused: [Duration::from_micros(97); 3],
            count: 2,
        };
        assert!(matches!(
            samples.add(Duration::from_micros(100), Duration::from_micros(97)),
            Choice::Dense
        ));
    }
}
