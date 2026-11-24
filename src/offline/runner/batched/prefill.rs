//! Stage a small number of encoded sources while decoder contexts are full.
//! The ready queue contains no decoded output or self-attention history.
use super::{plan::Part, Cursor, Runner, State, MAX_GENERATED_TOKENS};
use anyhow::Result;
use candle_core::{Device, Tensor};
use std::collections::VecDeque;

const AHEAD_ROWS: usize = 32;
const ENCODER_BATCH_TOKENS: usize = 2048;

#[derive(Clone, Copy)]
pub(in crate::offline) enum PrefillBudget {
    Default,
    Cpu(usize),
    Cuda,
}

impl PrefillBudget {
    fn retain_encoder(self, encoder: Tensor) -> candle_core::Result<Tensor> {
        // A narrow view otherwise keeps the entire packed encoder batch alive
        // until its last context finishes. Copy only the source's own rows.
        match self {
            Self::Default => Ok(encoder),
            Self::Cpu(_) => {
                let values = encoder.flatten_all()?.to_vec1::<f32>()?;
                Tensor::from_vec(values, encoder.shape(), &Device::Cpu)
            }
            Self::Cuda => encoder.force_contiguous(),
        }
    }
}

pub(super) struct Intake<'a> {
    input: &'a [String],
    next: usize,
    ready: VecDeque<Result<Cursor>>,
}

impl<'a> Intake<'a> {
    pub(super) fn new(input: &'a [String]) -> Self {
        Self {
            input,
            next: 0,
            ready: VecDeque::new(),
        }
    }

    fn next_source(&mut self, runner: &Runner) -> Option<Result<Cursor>> {
        let source = self.input.get(self.next)?;
        let cursor = Cursor::new(self.next, source, runner);
        self.next += 1;
        Some(cursor)
    }

    pub(super) fn take(&mut self, runner: &Runner) -> Result<Option<Cursor>> {
        self.ready
            .pop_front()
            .or_else(|| self.next_source(runner))
            .transpose()
    }

    pub(super) fn can_stage(&self, minimum: usize) -> bool {
        self.ready.is_empty() && self.input.len().saturating_sub(self.next) >= minimum
    }

    pub(super) fn encode(
        &mut self,
        active: &mut [Option<Cursor>],
        runner: &Runner,
        pool: &rayon::ThreadPool,
        budget: PrefillBudget,
        stage_ahead: bool,
    ) -> Result<()> {
        let reserved_now: usize = active
            .iter()
            .flatten()
            .chain(self.ready.iter().filter_map(|cursor| cursor.as_ref().ok()))
            .filter_map(|cursor| cursor.part.as_ref())
            .filter_map(|part| {
                part.state
                    .as_ref()
                    .map(|state| source_tokens(part) + state.limit)
            })
            .sum();
        let mut available = match budget {
            PrefillBudget::Cpu(limit) => limit.saturating_sub(reserved_now),
            _ => usize::MAX,
        };
        let mut parts = Vec::new();
        for part in active
            .iter_mut()
            .flatten()
            .filter_map(|cursor| cursor.part.as_mut())
        {
            if part.state.is_none() && reserve(&mut available, part, runner) {
                parts.push(part);
            }
        }

        if stage_ahead && !parts.is_empty() {
            let mut tokens = 0;
            let mut staged = 0;
            for _ in 0..AHEAD_ROWS {
                let Some(cursor) = self.next_source(runner) else {
                    break;
                };
                let cursor = cursor.map(|mut cursor| {
                    cursor.advance();
                    cursor
                });
                let stop = match &cursor {
                    Ok(cursor) => cursor.part.as_ref().is_some_and(|part| {
                        tokens += source_tokens(part);
                        tokens > ENCODER_BATCH_TOKENS || !reserve(&mut available, part, runner)
                    }),
                    Err(_) => true,
                };
                // A boundary row (or preparation failure) is consumed when its
                // place is reached. Previously accepted rows remain ahead of it.
                self.ready.push_back(cursor);
                if stop {
                    break;
                }
                staged += 1;
            }
            parts.extend(
                self.ready
                    .iter_mut()
                    .take(staged)
                    .filter_map(|cursor| cursor.as_mut().ok())
                    .filter_map(|cursor| cursor.part.as_mut()),
            );
        }

        let width = match budget {
            PrefillBudget::Default => usize::MAX,
            _ => ENCODER_BATCH_TOKENS,
        };
        let mut offset = 0;
        while offset < parts.len() {
            let mut end = offset + 1;
            let mut tokens = source_tokens(parts[offset]);
            while end < parts.len() && tokens + source_tokens(parts[end]) <= width {
                tokens += source_tokens(parts[end]);
                end += 1;
            }
            let group = &mut parts[offset..end];
            let sources = group
                .iter()
                .map(|part| {
                    let mut source = Vec::with_capacity(source_tokens(part));
                    source.push(runner.tokenizer.ko);
                    source.extend_from_slice(&part.prepared.encoded.tokens);
                    source.push(runner.config.eos_token_id);
                    source
                })
                .collect::<Vec<_>>();
            let encoders = pool.install(|| runner.model.encode_many(&sources))?;
            for (part, encoder) in group.iter_mut().zip(encoders) {
                part.state = Some(State {
                    encoder: budget.retain_encoder(encoder)?,
                    input: runner.config.decoder_start_token_id,
                    past: 0,
                    output: Vec::new(),
                    limit: generation_limit(part, runner),
                });
            }
            offset = end;
        }
        Ok(())
    }
}

fn source_tokens(part: &Part) -> usize {
    part.prepared.encoded.tokens.len() + 2
}

fn generation_limit(part: &Part, runner: &Runner) -> usize {
    (part.prepared.encoded.tokens.len() * 4 + 32)
        .clamp(64, MAX_GENERATED_TOKENS)
        .min(runner.config.max_position_embeddings - 2)
}

fn reserve(available: &mut usize, part: &Part, runner: &Runner) -> bool {
    let cost = source_tokens(part) + generation_limit(part, runner);
    if let Some(remaining) = available.checked_sub(cost) {
        *available = remaining;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::{CpuStorage, Storage};

    #[test]
    fn cuda_prefill_keeps_only_the_selected_encoder_rows() {
        let packed = Tensor::new(&[[[1f32, 2.], [-0., 4.], [5., 6.]]], &Device::Cpu).unwrap();
        let source = packed.narrow(1, 1, 1).unwrap();
        let compact = PrefillBudget::Cuda.retain_encoder(source).unwrap();
        assert_eq!(compact.dims(), &[1, 1, 2]);
        let (storage, layout) = compact.storage_and_layout();
        assert_eq!(layout.start_offset(), 0);
        let Storage::Cpu(CpuStorage::F32(values)) = &*storage else {
            panic!("expected F32 backing storage");
        };
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].to_bits(), (-0.0f32).to_bits());
        assert_eq!(values[1], 4.);
    }
}
