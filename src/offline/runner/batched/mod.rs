//! Iteration-level scheduling and selective linear batching (Orca-style).
//! Logical contexts share a bounded Rayon pool, weights and tokenizer. Each
//! context owns its decoder KV cache, token history and original source index.
mod plan;
use super::{generation::repeated_trigram_tokens, Runner};
use crate::offline::{model::Model, ordered};
use anyhow::{bail, Result};
use plan::{Cursor, State};

pub(in crate::offline) fn translate(
    runners: &mut [Runner],
    input: &[String],
    pool: &rayon::ThreadPool,
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
    let result = run(runners, input, pool, &mut emit);
    for runner in runners {
        runner.model.reset();
    }
    result
}
fn run(
    runners: &mut [Runner],
    input: &[String],
    pool: &rayon::ThreadPool,
    emit: &mut dyn FnMut(usize, String) -> Result<()>,
) -> Result<()> {
    let count = runners.len().min(input.len());
    let mut cursors: Vec<Option<Cursor>> = (0..count).map(|_| None).collect();
    let mut pending = 0;
    loop {
        // Refill immediately after completion. Input-order admission lets the
        // streaming sink expose the contiguous translated prefix promptly.
        for slot in 0..count {
            loop {
                if cursors[slot].is_none() {
                    if pending == input.len() {
                        break;
                    }
                    runners[slot].model.reset();
                    cursors[slot] = Some(Cursor::new(pending, &input[pending], &runners[slot])?);
                    pending += 1;
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
        let missing = cursors
            .iter()
            .enumerate()
            .filter_map(|(index, cursor)| {
                cursor
                    .as_ref()?
                    .part
                    .as_ref()
                    .filter(|part| part.state.is_none())
                    .map(|_| index)
            })
            .collect::<Vec<_>>();
        let ready = cursors
            .iter()
            .flatten()
            .filter(|cursor| {
                cursor
                    .part
                    .as_ref()
                    .is_some_and(|part| part.state.is_some())
            })
            .count();
        let prefill_threshold = (count / 4).clamp(1, 8);
        if !missing.is_empty() && (missing.len() >= prefill_threshold || ready <= count / 2) {
            let sources = missing
                .iter()
                .map(|&slot| {
                    let part = cursors[slot].as_ref().unwrap().part.as_ref().unwrap();
                    let mut source = Vec::with_capacity(part.encoded.tokens.len() + 2);
                    source.push(runners[slot].tokenizer.ko);
                    source.extend_from_slice(&part.encoded.tokens);
                    source.push(runners[slot].config.eos_token_id);
                    source
                })
                .collect::<Vec<_>>();
            let encoded = pool.install(|| runners[0].model.encode_many(&sources))?;
            for (slot, encoder) in missing.into_iter().zip(encoded) {
                let part = cursors[slot].as_mut().unwrap().part.as_mut().unwrap();
                let limit = (part.encoded.tokens.len() * 4 + 32)
                    .clamp(64, 512)
                    .min(runners[slot].config.max_position_embeddings - 2);
                part.state = Some(State {
                    encoder,
                    input: runners[slot].config.decoder_start_token_id,
                    past: 0,
                    output: Vec::new(),
                    limit,
                });
            }
        }
        let active = cursors
            .iter()
            .enumerate()
            .filter_map(|(slot, cursor)| {
                cursor.as_ref()?.part.as_ref()?.state.as_ref().map(|_| slot)
            })
            .collect::<Vec<_>>();
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
            pool.install(|| {
                Model::decode_many(&mut models, &inputs, &past, &encoders, vocab, &blocked)
            })?
        };
        for (row, &slot) in active.iter().enumerate() {
            let runner = &mut runners[slot];
            let cursor = cursors[slot].as_mut().unwrap();
            let part = cursor.part.as_mut().unwrap();
            let state = part.state.as_mut().unwrap();
            let next = scores[row];
            state.past += 1;
            let completed =
                if next == runner.config.eos_token_id {
                    let decoded = runner.tokenizer.decode(&state.output)?;
                    if decoded.is_empty() && part.encoded.text.chars().any(char::is_alphabetic) {
                        bail!("모델이 빈 번역을 생성했습니다");
                    }
                    Some(crate::translation::prose::finish(
                        &part.encoded.text,
                        &decoded,
                    ))
                } else {
                    state.output.push(next);
                    state.input = next;
                    if state.output.len() == state.limit {
                        Some(pool.install(|| {
                            runner.recover_after_limit(&part.encoded.text, state.limit)
                        })?)
                    } else {
                        None
                    }
                };
            if let Some(korean) = completed {
                cursor
                    .output
                    .push_str(&runner.memory.post_edit(&part.encoded.text, &korean)?);
                cursor.part = None;
                runner.model.reset();
            }
        }
    }
    Ok(())
}
