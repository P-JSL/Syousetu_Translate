use super::super::{Piece, Prepared, Runner};
use anyhow::Result;
use candle_core::Tensor;
use std::collections::VecDeque;

pub(super) struct State {
    pub(super) encoder: Tensor,
    pub(super) input: u32,
    pub(super) past: usize,
    pub(super) output: Vec<u32>,
    pub(super) limit: usize,
}
pub(super) struct Part {
    pub(super) prepared: Prepared,
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
        Ok(Self {
            index,
            pieces: runner.prepare(source)?,
            part: None,
            output: String::new(),
        })
    }
    pub(super) fn advance(&mut self) {
        while self.part.is_none() {
            match self.pieces.pop_front() {
                Some(Piece::Literal(text)) => self.output.push_str(&text),
                Some(Piece::Generate(prepared)) => {
                    self.part = Some(Part {
                        prepared,
                        state: None,
                    })
                }
                None => break,
            }
        }
    }

    pub(super) fn prepend(&mut self, mut pieces: VecDeque<Piece>) {
        pieces.append(&mut self.pieces);
        self.pieces = pieces;
    }
}
