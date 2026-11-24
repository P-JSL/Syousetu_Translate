//! Keep cross-attention K/V in head order across decoder steps.
use super::{row_index, Heads, KeyMask, PAD_ROW};
use candle_core::{DType, Device, Result, Tensor};
use std::ops::Range;

enum Rows {
    Contiguous(Range<usize>),
    Indexed(Tensor),
}

impl Rows {
    fn select(&self, tensor: &Tensor, axis: usize) -> Result<Tensor> {
        match self {
            Self::Contiguous(range) => tensor.narrow(axis, range.start, range.len()),
            Self::Indexed(ids) => tensor.index_select(ids, axis),
        }
    }
}

struct Write {
    head_start: usize,
    slots: usize,
    input: Rows,
}

pub(super) struct Plan {
    mask: KeyMask,
    capacity: usize,
    read: Rows,
    writes: Vec<Write>,
}

impl Plan {
    pub(super) fn new(
        heads: Heads,
        slots: &[usize],
        lengths: &[usize],
        capacity: usize,
        refresh: &[(usize, Range<usize>)],
        device: &Device,
    ) -> Result<Self> {
        let mask = KeyMask::new(lengths, device)?;
        let first = slots[0];
        let last = slots[slots.len() - 1];
        let read = if last - first + 1 == slots.len() {
            Rows::Contiguous(first * heads.count..(last + 1) * heads.count)
        } else {
            let ids = slots
                .iter()
                .flat_map(|&slot| slot * heads.count..(slot + 1) * heads.count)
                .map(row_index)
                .collect::<Result<Vec<_>>>()?;
            let count = ids.len();
            Rows::Indexed(Tensor::from_vec(ids, count, device)?)
        };

        let mut writes = Vec::new();
        for run in refresh.chunk_by(|a, b| a.0 + 1 == b.0) {
            let input = if run.iter().all(|(_, rows)| rows.len() == capacity) {
                Rows::Contiguous(run[0].1.start..run[run.len() - 1].1.end)
            } else {
                // Overwrite padding on slot reuse so stale K/V cannot enter
                // the GEMMs for a shorter replacement source.
                let count = run.len() * capacity;
                let mut ids = vec![PAD_ROW; count];
                for (dst, (_, rows)) in ids.chunks_exact_mut(capacity).zip(run) {
                    for (id, source) in dst.iter_mut().zip(rows.clone()) {
                        *id = row_index(source)?;
                    }
                }
                Rows::Indexed(Tensor::from_vec(ids, count, device)?)
            };
            writes.push(Write {
                head_start: run[0].0 * heads.count,
                slots: run.len(),
                input,
            });
        }
        Ok(Self {
            mask,
            capacity,
            read,
            writes,
        })
    }

    pub(super) fn store(
        &self,
        bank: &mut Option<Tensor>,
        kv: &Tensor,
        heads: Heads,
        slots: usize,
    ) -> Result<()> {
        if self.writes.is_empty() {
            return Ok(());
        }
        if bank
            .as_ref()
            .is_none_or(|current| current.dims()[2] != self.capacity)
        {
            let next = Tensor::zeros(
                (2, slots * heads.count, self.capacity, heads.width),
                DType::F32,
                kv.device(),
            )?;
            if let Some(current) = bank.as_ref() {
                next.slice_set(current, 2, 0)?;
            }
            *bank = Some(next);
        }
        let bank = bank.as_ref().expect("source bank was allocated");
        for write in &self.writes {
            let rows = write.input.select(kv, 0)?;
            let packed = heads.split(&rows, write.slots, self.capacity, 2)?;
            bank.slice_set(&packed, 1, write.head_start)?;
        }
        Ok(())
    }

    pub(super) fn context(&self, heads: Heads, query: &Tensor, bank: &Tensor) -> Result<Tensor> {
        let q = query.reshape((self.mask.batch * heads.count, 1, heads.width))?;
        let kv = self.read.select(bank, 1)?;
        let k = kv.get(0)?.narrow(1, 0, self.mask.width)?;
        let v = kv.get(1)?.narrow(1, 0, self.mask.width)?;
        self.mask.context(heads, &q, &k, &v)
    }
}
