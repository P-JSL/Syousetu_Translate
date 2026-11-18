use super::{coverage, Runner};
use crate::offline::{text::japanese, tokenizer::EncodedText};
use anyhow::Result;
use std::collections::VecDeque;

pub(super) enum Piece {
    Literal(String),
    Generate(Prepared),
}

pub(super) struct Prepared {
    pub(super) encoded: EncodedText,
    boundary: Option<SentenceBoundary>,
}

struct SentenceBoundary {
    framing: Option<coverage::Framing>,
    full_stop: bool,
}

impl Prepared {
    pub(super) fn render(&self, mut korean: String) -> String {
        if let Some(boundary) = &self.boundary {
            if let Some(framing) = boundary.framing {
                if korean.starts_with(framing.target.0) && korean.ends_with(framing.target.1) {
                    korean.truncate(korean.len() - framing.target.1.len_utf8());
                    korean.drain(..framing.target.0.len_utf8());
                }
            }
            if boundary.full_stop && !korean.ends_with(['.', '!', '?', '。', '…']) {
                korean.push('.');
            }
        }
        korean
    }
}

impl Runner {
    pub(super) fn prepare(&self, source: &str) -> Result<VecDeque<Piece>> {
        let mut pieces = VecDeque::new();
        if let Some(corrected) = self.memory.resolve(source) {
            pieces.push_back(Piece::Literal(corrected.into_owned()));
            return Ok(pieces);
        }
        for (index, line) in source.split('\n').enumerate() {
            if index != 0 {
                pieces.push_back(Piece::Literal("\n".into()));
            }
            if let Some(corrected) = self.memory.resolve(line) {
                pieces.push_back(Piece::Literal(corrected.into_owned()));
            } else if !japanese(line) {
                pieces.push_back(Piece::Literal(line.to_owned()));
            } else {
                for (index, encoded) in self.tokenizer.split(line)?.into_iter().enumerate() {
                    if index != 0 {
                        pieces.push_back(Piece::Literal(" ".into()));
                    }
                    pieces.push_back(self.prepare_piece(encoded, None));
                }
            }
        }
        Ok(pieces)
    }

    pub(super) fn sentence_retry(
        &self,
        part: &Prepared,
        result: &Result<String>,
    ) -> Result<Option<VecDeque<Piece>>> {
        // Segmented results are accepted once per fragment, with no retry loop.
        if part.boundary.is_some() {
            return Ok(None);
        }
        let source = &part.encoded.text;
        let Some(segments) = coverage::segments(source) else {
            return Ok(None);
        };
        let retry = match result {
            Ok(korean) => {
                coverage::may_have_missing_sentence(&segments, korean)
                    && self.memory.matching_terms(source, korean).next().is_none()
            }
            Err(error) => super::generation::incomplete(error),
        };
        if !retry {
            return Ok(None);
        }
        self.prepare_sentences(segments).map(Some)
    }

    fn prepare_sentences(&self, segments: coverage::Segments<'_>) -> Result<VecDeque<Piece>> {
        let mut prepared = VecDeque::new();
        if let Some(framing) = segments.framing {
            prepared.push_back(Piece::Literal(framing.target.0.to_string()));
        }
        for (index, sentence) in segments.sentences.into_iter().enumerate() {
            let text = match segments.framing {
                Some(framing) => format!("{}{sentence}{}", framing.source.0, framing.source.1),
                None => sentence.to_owned(),
            };
            if index != 0 {
                prepared.push_back(Piece::Literal(" ".into()));
            }
            if let Some(encoded) = self.tokenizer.encode_part(text)? {
                prepared.push_back(self.prepare_piece(
                    encoded,
                    Some(SentenceBoundary {
                        framing: segments.framing,
                        full_stop: sentence.ends_with('。'),
                    }),
                ));
            } else {
                // Keep the sentence boundary even when that sentence needs
                // token-limit splits of its own. The paragraph has one frame.
                let fragments = self.tokenizer.split(sentence)?;
                let count = fragments.len();
                for (index, encoded) in fragments.into_iter().enumerate() {
                    if index != 0 {
                        prepared.push_back(Piece::Literal(" ".into()));
                    }
                    prepared.push_back(self.prepare_piece(
                        encoded,
                        Some(SentenceBoundary {
                            framing: None,
                            full_stop: index + 1 == count && sentence.ends_with('。'),
                        }),
                    ));
                }
            }
        }
        if let Some(framing) = segments.framing {
            prepared.push_back(Piece::Literal(framing.target.1.to_string()));
        }
        Ok(prepared)
    }

    fn prepare_piece(&self, encoded: EncodedText, boundary: Option<SentenceBoundary>) -> Piece {
        let prepared = Prepared { encoded, boundary };
        if let Some(corrected) = self.memory.resolve(&prepared.encoded.text) {
            return Piece::Literal(prepared.render(corrected.into_owned()));
        }
        if !japanese(&prepared.encoded.text) {
            return Piece::Literal(prepared.encoded.text);
        }
        Piece::Generate(prepared)
    }
}
