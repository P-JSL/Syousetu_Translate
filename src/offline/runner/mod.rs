pub(super) mod batched;
pub(super) mod generation;
use crate::learning::Memory;
use anyhow::Result;
use std::sync::Arc;

use super::{model, text::japanese, tokenizer::Tokenizer};
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
        if let Some(text) = self.memory.resolve(source) {
            return Ok(text.into_owned());
        }
        if !japanese(source) {
            return Ok(source.to_owned());
        }
        let mut lines = Vec::new();
        for line in source.split('\n') {
            if let Some(text) = self.memory.resolve(line) {
                lines.push(text.into_owned());
                continue;
            }
            if !japanese(line) {
                lines.push(line.to_owned());
                continue;
            }
            let mut translated = Vec::new();
            for part in self.tokenizer.split(line)? {
                if let Some(text) = self.memory.resolve(&part.text) {
                    translated.push(text.into_owned());
                    continue;
                }
                let korean = self.generate(&part.text, &part.tokens)?;
                translated.push(self.memory.post_edit(&part.text, &korean)?);
            }
            lines.push(translated.join(" "));
        }
        Ok(lines.join("\n"))
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
