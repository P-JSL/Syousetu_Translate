use anyhow::{Context, Result};
use sentencepiece_rs::SentencePieceProcessor;
use std::{collections::HashMap, fs, path::Path};

use super::text::split_boundary;
pub(super) const MAX_SOURCE_TOKENS: usize = 192;
pub(super) struct Tokenizer {
    sp: SentencePieceProcessor,
    pub(super) vocab: HashMap<String, u32>,
    inverse: Vec<Option<String>>,
    pub(super) ja: u32,
    pub(super) ko: u32,
}
impl Tokenizer {
    pub(super) fn open(root: &Path) -> Result<Self> {
        let sp = SentencePieceProcessor::open(root.join("sentencepiece.bpe.model"))?;
        let vocab: HashMap<String, u32> =
            serde_json::from_slice(&fs::read(root.join("vocab.json"))?)?;
        let mut inverse = vec![None; vocab.len()];
        for (piece, &id) in &vocab {
            let slot = inverse
                .get_mut(id as usize)
                .context("토크나이저 ID 범위 초과")?;
            *slot = Some(piece.clone());
        }
        // Official M2M100 language order. Shared vocab ids differ from SPM ids.
        let size = vocab.len() as u32;
        Ok(Self {
            sp,
            vocab,
            inverse,
            ja: size + 42,
            ko: size + 48,
        })
    }
    pub(super) fn encode(&self, text: &str) -> Result<Vec<u32>> {
        Ok(self
            .sp
            .encode(text)?
            .iter()
            .map(|p| self.vocab.get(p).copied().unwrap_or(3))
            .collect())
    }
    pub(super) fn decode(&self, ids: &[u32]) -> Result<String> {
        let pieces = ids
            .iter()
            .filter(|&&id| id >= 4)
            .filter_map(|id| self.inverse.get(*id as usize).and_then(Option::as_ref))
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut decoded = self.sp.decode(&pieces)?;
        decoded.truncate(decoded.trim_end().len());
        let leading = decoded.len() - decoded.trim_start().len();
        if leading != 0 {
            decoded.drain(..leading);
        }
        Ok(decoded)
    }
    pub(super) fn encode_prose(&self, text: &str) -> Result<Vec<u32>> {
        use crate::translation::prose::{game_terms, japanese};
        let expanded = japanese::expand(text);
        self.encode(&game_terms::expand(&expanded))
    }
    pub(super) fn encode_part(&self, text: String) -> Result<Option<EncodedText>> {
        let tokens = self.encode_prose(&text)?;
        Ok((tokens.len() <= MAX_SOURCE_TOKENS).then_some(EncodedText { text, tokens }))
    }
    pub(super) fn split(&self, text: &str) -> Result<Vec<EncodedText>> {
        if let Some(encoded) = self.encode_part(text.to_owned())? {
            return Ok(vec![encoded]);
        }
        let boundary =
            split_boundary(text).context("모델 입력 한도에 맞게 원문을 분할할 수 없습니다")?;
        let mut parts = self.split(&text[..boundary])?;
        parts.extend(self.split(&text[boundary..])?);
        Ok(parts)
    }
}

pub(super) struct EncodedText {
    pub(super) text: String,
    pub(super) tokens: Vec<u32>,
}
