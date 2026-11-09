use anyhow::{Context, Result};
use sentencepiece_rs::SentencePieceProcessor;
use std::{collections::HashMap, fs, path::Path};

use super::text::split_boundary;
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
        Ok(self.sp.decode(&pieces)?.trim().to_owned())
    }
    pub(super) fn split(&self, text: &str) -> Result<Vec<EncodedText>> {
        let tokens = self.encode(text)?;
        if tokens.len() <= 192 {
            return Ok(vec![EncodedText {
                text: text.to_owned(),
                tokens,
            }]);
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
