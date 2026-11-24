//! Device attention packs token rows in bounded groups and keeps K/V on the device.
mod decode;
mod encode;
mod sources;
mod storage;

pub(in crate::offline) use decode::Decoder;
pub(in crate::offline::model) use decode::Step;
pub(in crate::offline::model) use encode::EncoderPlan;

use crate::offline::model::ops::softmax_last_dim;
use candle_core::{Device, Error, Result, Tensor};

const GROUP_SIZE: usize = 32;
// Per F32 score tensor (64 MiB), not a global GPU memory limit.
const MAX_SCORE_ELEMENTS: usize = 16 * 1024 * 1024;
// Candle index_select maps this ID to a zero row on CUDA and CPU.
const PAD_ROW: u32 = u32::MAX;

fn row_index(index: usize) -> Result<u32> {
    u32::try_from(index)
        .ok()
        .filter(|&id| id != PAD_ROW)
        .ok_or_else(|| Error::Msg("attention row index exceeds device range".into()))
}

#[derive(Clone, Copy)]
pub(in crate::offline::model) struct Heads {
    count: usize,
    width: usize,
}
impl Heads {
    pub(in crate::offline::model) fn new(count: usize, width: usize) -> Self {
        Self { count, width }
    }

    fn hidden(self) -> usize {
        self.count * self.width
    }

    // [context * position, projections * hidden] ->
    // [projections, context * head, position, head_width].
    fn split(self, rows: &Tensor, batch: usize, length: usize, parts: usize) -> Result<Tensor> {
        rows.reshape((batch, length, parts, self.count, self.width))?
            .permute((2, 0, 3, 1, 4))?
            .reshape((parts, batch * self.count, length, self.width))
    }
}

struct KeyMask {
    batch: usize,
    width: usize,
    mask: Option<Tensor>,
}
impl KeyMask {
    fn new(lengths: &[usize], device: &Device) -> Result<Self> {
        let width = lengths.iter().copied().max().unwrap_or(0);
        if width == 0 || lengths.contains(&0) {
            candle_core::bail!("attention needs a nonempty key sequence per context");
        }
        let batch = lengths.len();
        let mask = if lengths.iter().all(|&length| length == width) {
            None
        } else {
            let mut mask = vec![0f32; batch * width];
            for (row, &length) in mask.chunks_exact_mut(width).zip(lengths) {
                row[length..].fill(f32::NEG_INFINITY);
            }
            Some(Tensor::from_vec(mask, (batch, 1, 1, width), device)?)
        };
        Ok(Self { batch, width, mask })
    }

    fn context(&self, heads: Heads, q: &Tensor, k: &Tensor, v: &Tensor) -> Result<Tensor> {
        let queries = q.dim(1)?;
        let batch_heads = self.batch * heads.count;
        // cuBLAS can consume this last-two-axis transpose as a view.
        let scaled = (q * (heads.width as f64).powf(-0.5))?;
        let scores = scaled.matmul(&k.transpose(1, 2)?)?;
        let scores = match &self.mask {
            Some(mask) => scores
                .reshape((self.batch, heads.count, queries, self.width))?
                .broadcast_add(mask)?
                .reshape((batch_heads, queries, self.width))?,
            None => scores,
        };
        softmax_last_dim(&scores)?
            .matmul(v)?
            .reshape((self.batch, heads.count, queries, heads.width))?
            .transpose(1, 2)?
            .reshape((self.batch * queries, heads.hidden()))
    }
}

fn join_rows(rows: Vec<Tensor>) -> Result<Tensor> {
    Tensor::cat(&rows, 0)?.unsqueeze(0)
}
