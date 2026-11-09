use anyhow::{Context, Result};
use sentencepiece_rs::SentencePieceProcessor;
use std::{collections::HashMap, fs, path::Path, sync::OnceLock};

use super::text::split_boundary;
pub(super) struct Tokenizer {
    sp: SentencePieceProcessor,
    pub(super) vocab: HashMap<String, u32>,
    inverse: Vec<Option<String>>,
    pub(super) ja: u32,
    pub(super) ko: u32,
    fast_base: OnceLock<Vec<u32>>,
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
            fast_base: OnceLock::new(),
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
    pub(super) fn shortlist(&self, sources: &[String]) -> Result<Vec<u32>> {
        let mut ids = self
            .fast_base
            .get_or_init(|| {
                self.vocab
                    .iter()
                    .filter(|(piece, _)| super::vocabulary::base_piece(piece))
                    .map(|(_, &id)| id)
                    .collect()
            })
            .clone();
        ids.push(2);
        for source in sources {
            ids.extend(self.encode(source)?);
            extend_source_ids(&self.vocab, source, &mut ids);
        }
        ids.sort_unstable();
        ids.dedup();
        // Keep the existing packed Q8 arithmetic path. Padding uses BOS rows,
        // which greedy selection excludes after mapping back to original IDs.
        while !ids.len().is_multiple_of(16) {
            ids.push(0);
        }
        Ok(ids)
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

// Preserve alternate source-name subwords, including long Latin names and
// Japanese copy fragments. Encoded IDs above cover pieces longer than 16 chars.
fn extend_source_ids(vocab: &HashMap<String, u32>, source: &str, ids: &mut Vec<u32>) {
    let mut boundaries = source
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    boundaries.push(source.len());
    let mut prefixed = String::with_capacity(80);
    for start in 0..boundaries.len().saturating_sub(1) {
        for end in start + 1..=(start + 16).min(boundaries.len() - 1) {
            let part = &source[boundaries[start]..boundaries[end]];
            if let Some(&id) = vocab.get(part) {
                ids.push(id);
            }
            prefixed.clear();
            prefixed.push('▁');
            prefixed.push_str(part);
            if let Some(&id) = vocab.get(&prefixed) {
                ids.push(id);
            }
        }
    }
}

pub(super) struct EncodedText {
    pub(super) text: String,
    pub(super) tokens: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternate_source_pieces_preserve_names_without_adding_unrelated_words() {
        let vocab = [
            ("▁Alexandra", 7),
            ("Alex", 8),
            ("andra", 9),
            ("▁図書館", 10),
            ("図書", 11),
            ("無関係", 12),
            ("😀", 13),
        ]
        .map(|(piece, id)| (piece.to_owned(), id))
        .into();
        let mut ids = Vec::new();
        extend_source_ids(&vocab, "Alexandraは図書館にいます😀", &mut ids);
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids, [7, 8, 9, 10, 11, 13]);
    }
}
