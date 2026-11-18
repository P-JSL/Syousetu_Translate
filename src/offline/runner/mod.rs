pub(super) mod batched;
mod coverage;
pub(super) mod generation;
mod preparation;
use crate::learning::Memory;
use anyhow::Result;
use std::sync::Arc;

use super::{model, tokenizer::Tokenizer};
use preparation::{Piece, Prepared};
pub(super) struct Runner {
    model: model::Model,
    config: model::Config,
    tokenizer: Arc<Tokenizer>,
    memory: Arc<Memory>,
    pool: Arc<rayon::ThreadPool>,
    small: bool,
}
impl Runner {
    pub(super) fn translate(&mut self, source: &str) -> Result<String> {
        let pool = Arc::clone(&self.pool);
        pool.install(|| self.translate_inner(source))
    }
    pub(super) fn translate_inner(&mut self, source: &str) -> Result<String> {
        let mut output = String::new();
        let mut pieces = self.prepare(source)?;
        while let Some(piece) = pieces.pop_front() {
            match piece {
                Piece::Literal(text) => output.push_str(&text),
                Piece::Generate(part) => {
                    let result = self.generate(&part.encoded.text, &part.encoded.tokens);
                    if let Some(mut retry) = self.sentence_retry(&part, &result)? {
                        retry.append(&mut pieces);
                        pieces = retry;
                    } else {
                        output.push_str(&self.complete(&part, result)?);
                    }
                }
            }
        }
        Ok(output)
    }

    fn complete(&self, part: &Prepared, result: Result<String>) -> Result<String> {
        let source = &part.encoded.text;
        let korean = match result {
            Ok(korean) => {
                let user_edit = self.memory.matching_terms(source, &korean).next().is_some();
                if user_edit {
                    self.memory.post_edit(source, &korean)?
                } else {
                    crate::translation::prose::restore_facts(source, korean)
                }
            }
            Err(error) if generation::incomplete(&error) => {
                let utterance = crate::translation::prose::Utterance::parse(source);
                coverage::recover_known_phrase(utterance).unwrap_or_else(|| {
                    utterance.frame(&coverage::preserve_for_review(utterance.inner))
                })
            }
            Err(error) => return Err(error),
        };
        Ok(part.render(korean))
    }

    pub(super) fn new(
        model: model::Model,
        config: model::Config,
        tokenizer: Arc<Tokenizer>,
        memory: Arc<Memory>,
        pool: Arc<rayon::ThreadPool>,
    ) -> Self {
        let small = config.decoder_layers == 3;
        Self {
            model,
            config,
            tokenizer,
            memory,
            pool,
            small,
        }
    }
    pub(super) fn correction(&self, text: &str) -> Option<std::borrow::Cow<'_, str>> {
        self.memory.resolve(text)
    }
    pub(super) fn has_correction(&self, text: &str) -> bool {
        self.memory.resolve(text).is_some()
    }
}
