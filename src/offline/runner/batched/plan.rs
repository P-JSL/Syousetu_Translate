use super::super::Runner;
use crate::offline::tokenizer::EncodedText;
use anyhow::Result;
use candle_core::Tensor;
use std::collections::VecDeque;

enum Piece {
    Literal(String),
    Generate(EncodedText),
}
pub(super) struct State {
    pub(super) encoder: Tensor,
    pub(super) input: u32,
    pub(super) past: usize,
    pub(super) output: Vec<u32>,
    pub(super) limit: usize,
}
pub(super) struct Part {
    pub(super) encoded: EncodedText,
    pub(super) state: Option<State>,
}
pub(super) struct Cursor {
    pub(super) index: usize,
    pieces: VecDeque<Piece>,
    pub(super) part: Option<Part>,
    pub(super) output: String,
}
impl Cursor {
    pub(super) fn new(index: usize, source: &str, runner: &Runner) -> Result<Self> {
        let mut pieces = VecDeque::new();
        if let Some(corrected) = runner.memory.resolve(source) {
            pieces.push_back(Piece::Literal(corrected.into_owned()));
        } else {
            for (line_index, line) in source.split('\n').enumerate() {
                if line_index > 0 {
                    pieces.push_back(Piece::Literal("\n".into()));
                }
                if let Some(corrected) = runner.memory.resolve(line) {
                    pieces.push_back(Piece::Literal(corrected.into_owned()));
                } else if !crate::offline::text::japanese(line) {
                    pieces.push_back(Piece::Literal(line.to_owned()));
                } else {
                    for (part_index, part) in runner.tokenizer.split(line)?.into_iter().enumerate()
                    {
                        if part_index > 0 {
                            pieces.push_back(Piece::Literal(" ".into()));
                        }
                        if let Some(corrected) = runner.memory.resolve(&part.text) {
                            pieces.push_back(Piece::Literal(corrected.into_owned()));
                        } else {
                            pieces.push_back(Piece::Generate(part));
                        }
                    }
                }
            }
        }
        Ok(Self {
            index,
            pieces,
            part: None,
            output: String::new(),
        })
    }
    pub(super) fn advance(&mut self) {
        while self.part.is_none() {
            match self.pieces.pop_front() {
                Some(Piece::Literal(text)) => self.output.push_str(&text),
                Some(Piece::Generate(encoded)) => {
                    self.part = Some(Part {
                        encoded,
                        state: None,
                    })
                }
                None => break,
            }
        }
    }
}
