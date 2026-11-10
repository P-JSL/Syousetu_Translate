use super::Runner;
use crate::offline::text::split_boundary;
use anyhow::{bail, Result};
impl Runner {
    pub(super) fn generate(&mut self, text: &str, tokens: &[u32]) -> Result<String> {
        self.generate_at(text, tokens, 0)
    }
    pub(super) fn recover_after_limit(&mut self, text: &str, limit: usize) -> Result<String> {
        self.model.reset();
        if text.chars().count() >= 16 {
            if let Some(boundary) = split_boundary(text) {
                let first_tokens = self.tokenizer.encode(&text[..boundary])?;
                let first = self.generate_at(&text[..boundary], &first_tokens, 1)?;
                let last_tokens = self.tokenizer.encode(&text[boundary..])?;
                let last = self.generate_at(&text[boundary..], &last_tokens, 1)?;
                return Ok(format!("{} {}", first.trim(), last.trim()));
            }
        }
        Err(GenerationLimit(limit).into())
    }
    fn generate_at(&mut self, text: &str, tokens: &[u32], depth: usize) -> Result<String> {
        self.model.reset();
        let result = self.generate_inner(text, tokens);
        self.model.reset();
        if result.as_ref().is_err_and(|e| e.is::<GenerationLimit>())
            && depth < 3
            && text.chars().count() >= 16
        {
            if let Some(boundary) = split_boundary(text) {
                let first_tokens = self.tokenizer.encode(&text[..boundary])?;
                let first = self.generate_at(&text[..boundary], &first_tokens, depth + 1)?;
                let last_tokens = self.tokenizer.encode(&text[boundary..])?;
                let last = self.generate_at(&text[boundary..], &last_tokens, depth + 1)?;
                return Ok(format!("{} {}", first.trim(), last.trim()));
            }
        }
        result
    }
    fn generate_inner(&mut self, text: &str, tokens: &[u32]) -> Result<String> {
        let limit = (tokens.len() * 4 + 32)
            .clamp(64, 512)
            .min(self.config.max_position_embeddings - 2);
        let mut source = vec![if self.small {
            self.tokenizer.ko
        } else {
            self.tokenizer.ja
        }];
        source.extend_from_slice(tokens);
        source.push(self.config.eos_token_id);
        let encoder = self.model.encode(&source)?;
        let mut input = vec![self.config.decoder_start_token_id];
        if !self.small {
            input.push(self.tokenizer.ko);
        }
        let mut output = Vec::new();
        let mut past = 0;
        for _ in 0..limit {
            let scores = self.model.decode(&input, &encoder, past)?;
            past += input.len();
            let blocked = repeated_trigram_tokens(&output);
            let next = crate::offline::selection::select_logits(
                &scores,
                self.tokenizer.vocab.len(),
                self.config.eos_token_id,
                &blocked,
            )?;
            if next == self.config.eos_token_id {
                let decoded = self.tokenizer.decode(&output)?;
                if decoded.is_empty() && text.chars().any(char::is_alphabetic) {
                    bail!("모델이 빈 번역을 생성했습니다");
                }
                return Ok(crate::translation::prose::finish(text, &decoded));
            }
            output.push(next);
            input.clear();
            input.push(next);
        }
        Err(GenerationLimit(limit).into())
    }
}
#[derive(Debug)]
struct GenerationLimit(usize);
impl std::fmt::Display for GenerationLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "모델 출력이 {}토큰 한도에 도달했습니다. 불완전한 번역은 저장하지 않습니다",
            self.0
        )
    }
}
impl std::error::Error for GenerationLimit {}
pub(in crate::offline) fn repeated_trigram_tokens(
    output: &[u32],
) -> std::collections::HashSet<u32> {
    if output.len() < 2 {
        return std::collections::HashSet::new();
    }
    let prefix = &output[output.len() - 2..];
    output
        .windows(3)
        .filter(|triple| triple[..2] == *prefix)
        .map(|triple| triple[2])
        .collect()
}
