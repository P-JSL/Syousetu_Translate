//! Iteration-level scheduling and selective linear batching (Orca-style).
//! Logical contexts share a bounded Rayon pool, weights and tokenizer. Each
//! context owns its decoder KV cache, token history and original source index.
mod plan;
mod prefill;
use super::{
    generation::{repeated_trigram_tokens, MAX_GENERATED_TOKENS},
    Runner,
};
use crate::offline::{
    model::{DecodeRows, Model},
    ordered,
};
use anyhow::Result;
use plan::{Cursor, State};
use prefill::Intake;
pub(in crate::offline) use prefill::PrefillBudget;

pub(in crate::offline) fn translate(
    runners: &mut [Runner],
    input: &[String],
    pool: &rayon::ThreadPool,
    prefill_budget: PrefillBudget,
    mut emit: impl FnMut(usize, String) -> Result<()>,
) -> Result<()> {
    if input.is_empty() {
        return Ok(());
    }
    anyhow::ensure!(!runners.is_empty(), "번역 작업자가 없습니다");
    if !runners[0].small {
        for (index, text) in ordered::map(runners, input, Runner::translate)?
            .into_iter()
            .enumerate()
        {
            emit(index, text)?;
        }
        return Ok(());
    }
    let result = run(runners, input, pool, prefill_budget, &mut emit);
    for runner in runners {
        runner.model.reset();
    }
    result
}
fn run(
    runners: &mut [Runner],
    input: &[String],
    pool: &rayon::ThreadPool,
    prefill_budget: PrefillBudget,
    emit: &mut dyn FnMut(usize, String) -> Result<()>,
) -> Result<()> {
    let count = runners.len().min(input.len());
    let mut cursors: Vec<Option<Cursor>> = (0..count).map(|_| None).collect();
    let mut device_decoder = runners[0].model.device_decoder(count);
    let mut intake = Intake::new(input);
    loop {
        // Refill immediately after completion. Input-order admission lets the
        // streaming sink expose the contiguous translated prefix promptly.
        for slot in 0..count {
            loop {
                if cursors[slot].is_none() {
                    let Some(cursor) = intake.take(&runners[slot])? else {
                        break;
                    };
                    runners[slot].model.reset();
                    cursors[slot] = Some(cursor);
                }
                let cursor = cursors[slot].as_mut().expect("admitted cursor");
                cursor.advance();
                if cursor.part.is_some() {
                    break;
                }
                emit(cursor.index, std::mem::take(&mut cursor.output))?;
                cursors[slot] = None;
            }
        }
        if cursors.iter().all(Option::is_none) {
            break;
        }
        let (ready, missing) = cursors
            .iter()
            .flatten()
            .filter_map(|cursor| cursor.part.as_ref())
            .fold((0, 0), |(ready, missing), part| {
                (
                    ready + usize::from(part.state.is_some()),
                    missing + usize::from(part.state.is_none()),
                )
            });
        let prefill_threshold = (count / 4).clamp(1, 32);
        let stage_ahead = matches!(prefill_budget, PrefillBudget::Cpu(_))
            && ready != 0
            && intake.can_stage(prefill_threshold);
        if missing != 0 && (missing >= prefill_threshold || ready <= count / 2 || stage_ahead) {
            intake.encode(&mut cursors, &runners[0], pool, prefill_budget, stage_ahead)?;
        }
        let active = cursors
            .iter()
            .enumerate()
            .filter_map(|(slot, cursor)| {
                cursor.as_ref()?.part.as_ref()?.state.as_ref().map(|_| slot)
            })
            .collect::<Vec<_>>();
        anyhow::ensure!(!active.is_empty(), "번역 배치의 토큰 예산이 너무 작습니다");
        let states = active
            .iter()
            .map(|&slot| {
                cursors[slot]
                    .as_ref()
                    .unwrap()
                    .part
                    .as_ref()
                    .unwrap()
                    .state
                    .as_ref()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let inputs = states.iter().map(|state| state.input).collect::<Vec<_>>();
        let past = states.iter().map(|state| state.past).collect::<Vec<_>>();
        let encoders = states
            .iter()
            .map(|state| state.encoder.clone())
            .collect::<Vec<_>>();
        let blocked = states
            .iter()
            .map(|state| repeated_trigram_tokens(&state.output))
            .collect::<Vec<_>>();
        let vocab = runners[0].tokenizer.vocab.len();
        let scores = {
            let mut models = runners
                .iter_mut()
                .take(count)
                .zip(&cursors)
                .filter_map(|(runner, cursor)| {
                    cursor
                        .as_ref()?
                        .part
                        .as_ref()?
                        .state
                        .as_ref()
                        .map(|_| &mut runner.model)
                })
                .collect::<Vec<_>>();
            let rows = DecodeRows {
                slots: &active,
                inputs: &inputs,
                past: &past,
                encoders: &encoders,
                vocab,
                blocked: &blocked,
            };
            pool.install(|| Model::decode_many(&mut models, rows, device_decoder.as_mut()))?
        };
        for (row, &slot) in active.iter().enumerate() {
            let runner = &mut runners[slot];
            let cursor = cursors[slot].as_mut().unwrap();
            let part = cursor.part.as_mut().unwrap();
            let state = part.state.as_mut().unwrap();
            let next = scores[row];
            state.past += 1;
            let source = &part.prepared.encoded.text;
            let completed = if next == runner.config.eos_token_id {
                Some(runner.finish_decoded(source, &state.output))
            } else {
                state.output.push(next);
                state.input = next;
                if state.output.len() == state.limit {
                    Some(pool.install(|| runner.recover_after_limit(source, state.limit)))
                } else {
                    None
                }
            };
            if let Some(result) = completed {
                if let Some(retry) = runner.sentence_retry(&part.prepared, &result)? {
                    cursor.prepend(retry);
                } else {
                    cursor
                        .output
                        .push_str(&runner.complete(&part.prepared, result)?);
                }
                cursor.part = None;
                runner.model.reset();
            }
        }
    }
    Ok(())
}
