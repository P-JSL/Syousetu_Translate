use super::{join_rows, row_index, storage, Heads, KeyMask, GROUP_SIZE, PAD_ROW};
use candle_core::{Device, Error, Result, Tensor, TensorId};
use std::ops::Range;

#[derive(Clone, Copy)]
struct Cursor {
    past: usize,
    encoder: TensorId,
}

/// Owned by one continuous batch run. A slot can be reused as soon as its
/// sentence finishes. The cursor and indices restrict it to the new prefix.
pub(in crate::offline) struct Decoder {
    heads: Heads,
    limits: (usize, usize),
    step: usize,
    cursors: Vec<Option<Cursor>>,
    groups: Vec<Vec<storage::Kv>>,
}

struct Lookup {
    rows: Tensor,
    mask: KeyMask,
    capacity: usize,
}
impl Lookup {
    fn new(
        slots: &[usize],
        lengths: &[usize],
        capacity: usize,
        device: &Device,
        index: impl Fn(usize, usize, usize) -> usize,
    ) -> Result<Self> {
        let mask = KeyMask::new(lengths, device)?;
        let size = mask.batch * mask.width;
        let mut rows = vec![PAD_ROW; size];
        for ((row, &slot), &length) in rows.chunks_exact_mut(mask.width).zip(slots).zip(lengths) {
            for (position, id) in row[..length].iter_mut().enumerate() {
                *id = row_index(index(slot, position, length))?;
            }
        }
        Ok(Self {
            rows: Tensor::from_vec(rows, size, device)?,
            mask,
            capacity,
        })
    }

    fn context(&self, heads: Heads, query: &Tensor, bank: &Tensor) -> Result<Tensor> {
        let batch = self.mask.batch;
        let hidden = heads.hidden();
        let q = query.reshape((batch * heads.count, 1, heads.width))?;
        let rows = bank
            .reshape(((), 2 * hidden))?
            .index_select(&self.rows, 0)?;
        let kv = heads.split(&rows, batch, self.mask.width, 2)?;
        self.mask.context(heads, &q, &kv.get(0)?, &kv.get(1)?)
    }
}

struct Write {
    slot: usize,
    rows: Range<usize>,
}

struct Group {
    index: usize,
    rows: Range<usize>,
    slots: usize,
    history: Lookup,
    sources: Lookup,
    append: Vec<Write>,
    refresh: Vec<Write>,
}

/// One set of slot addresses and masks shared by all decoder layers.
pub(in crate::offline::model) struct Step<'a> {
    decoder: &'a mut Decoder,
    next: Vec<Option<Cursor>>,
    groups: Vec<Group>,
    new_sources: Vec<usize>,
}

impl Decoder {
    pub(in crate::offline::model) fn new(
        heads: Heads,
        slots: usize,
        layers: usize,
        max_history: usize,
        max_source: usize,
    ) -> Self {
        Self {
            heads,
            limits: (max_history, max_source),
            step: 0,
            cursors: vec![None; slots],
            groups: (0..slots.div_ceil(GROUP_SIZE))
                .map(|_| (0..layers).map(|_| storage::Kv::default()).collect())
                .collect(),
        }
    }

    pub(in crate::offline::model) fn prepare(
        &mut self,
        slots: &[usize],
        past: &[usize],
        encoders: &[Tensor],
    ) -> Result<Step<'_>> {
        if slots.is_empty()
            || slots.len() != past.len()
            || slots.len() != encoders.len()
            || slots.windows(2).any(|pair| pair[0] >= pair[1])
            || slots.last().is_some_and(|&slot| slot >= self.cursors.len())
        {
            candle_core::bail!("invalid GPU decoder slot list");
        }
        let device = encoders[0].device();
        let mut next = vec![None; self.cursors.len()];
        let mut source_lengths = Vec::with_capacity(slots.len());
        for ((&slot, &tokens), encoder) in slots.iter().zip(past).zip(encoders) {
            let cursor = Cursor {
                past: tokens,
                encoder: encoder.id(),
            };
            if tokens > 0
                && !self.cursors[slot].is_some_and(|previous| {
                    previous.past + 1 == tokens && previous.encoder == cursor.encoder
                })
            {
                candle_core::bail!("GPU decoder history is missing or belongs to another source");
            }
            next[slot] = Some(cursor);
            source_lengths.push(encoder.dim(1)?);
        }

        let mut row = 0;
        let mut source_offset = 0;
        let mut new_sources = Vec::new();
        let mut groups = Vec::new();
        for group in slots.chunk_by(|a, b| a / GROUP_SIZE == b / GROUP_SIZE) {
            let index = group[0] / GROUP_SIZE;
            let start = index * GROUP_SIZE;
            let group_slots = (self.cursors.len() - start).min(GROUP_SIZE);
            let local = group.iter().map(|&slot| slot - start).collect::<Vec<_>>();
            let end = row + group.len();
            let key_lengths = past[row..end]
                .iter()
                .map(|length| length + 1)
                .collect::<Vec<_>>();
            let source_lengths = &source_lengths[row..end];
            let layer = &self.groups[index][0];
            let history_capacity =
                capacity(layer.history.as_ref(), 0, &key_lengths, self.limits.0)?;
            let source_capacity =
                capacity(layer.sources.as_ref(), 1, source_lengths, self.limits.1)?;

            let history = Lookup::new(
                &local,
                &key_lengths,
                history_capacity,
                device,
                |slot, t, length| {
                    ((self.step + 1 - length + t) % history_capacity) * group_slots + slot
                },
            )?;
            let sources = Lookup::new(
                &local,
                source_lengths,
                source_capacity,
                device,
                |slot, t, _| slot * source_capacity + t,
            )?;

            let mut append = Vec::new();
            let mut begin = row;
            for run in local.chunk_by(|a, b| *a + 1 == *b) {
                append.push(Write {
                    slot: run[0],
                    rows: begin..begin + run.len(),
                });
                begin += run.len();
            }
            let mut refresh = Vec::new();
            for (offset, (&slot, &length)) in local.iter().zip(source_lengths).enumerate() {
                if past[row + offset] == 0 {
                    new_sources.push(row + offset);
                    refresh.push(Write {
                        slot,
                        rows: source_offset..source_offset + length,
                    });
                    source_offset += length;
                }
            }
            groups.push(Group {
                index,
                rows: row..end,
                slots: group_slots,
                history,
                sources,
                append,
                refresh,
            });
            row = end;
        }
        Ok(Step {
            decoder: self,
            next,
            groups,
            new_sources,
        })
    }
}

fn capacity(bank: Option<&Tensor>, axis: usize, lengths: &[usize], limit: usize) -> Result<usize> {
    let needed = lengths.iter().copied().max().unwrap_or(0);
    if needed == 0 || needed > limit {
        candle_core::bail!("GPU attention length {needed} exceeds the model limit {limit}");
    }
    let rounded = needed
        .checked_next_power_of_two()
        .unwrap_or(limit)
        .min(limit);
    Ok(bank.map_or(rounded, |bank| rounded.max(bank.dims()[axis])))
}

impl Step<'_> {
    pub(in crate::offline::model) fn new_sources(&self) -> &[usize] {
        &self.new_sources
    }

    pub(in crate::offline::model) fn self_context(
        &mut self,
        layer: usize,
        qkv: &Tensor,
    ) -> Result<Tensor> {
        let heads = self.decoder.heads;
        let hidden = heads.hidden();
        let qkv = qkv.squeeze(0)?;
        let q = qkv.narrow(1, 0, hidden)?;
        let kv = qkv.narrow(1, hidden, 2 * hidden)?.contiguous()?;
        let step = self.decoder.step;
        let mut output = Vec::with_capacity(self.groups.len());
        for group in &self.groups {
            let bank = storage::history(
                &mut self.decoder.groups[group.index][layer].history,
                step,
                group.history.capacity,
                group.slots,
                2 * hidden,
                kv.device(),
            )?;
            let current = bank.get(step % group.history.capacity)?;
            for write in &group.append {
                current.slice_set(
                    &kv.narrow(0, write.rows.start, write.rows.len())?,
                    0,
                    write.slot,
                )?;
            }
            let query = q.narrow(0, group.rows.start, group.rows.len())?;
            output.push(group.history.context(heads, &query, bank)?);
        }
        join_rows(output)
    }

    pub(in crate::offline::model) fn store_sources(
        &mut self,
        layer: usize,
        kv: &Tensor,
    ) -> Result<()> {
        let width = 2 * self.decoder.heads.hidden();
        let kv = kv.squeeze(0)?;
        for group in &self.groups {
            if group.refresh.is_empty() {
                continue;
            }
            let bank = storage::sources(
                &mut self.decoder.groups[group.index][layer].sources,
                group.sources.capacity,
                group.slots,
                width,
                kv.device(),
            )?;
            for write in &group.refresh {
                bank.get(write.slot)?.slice_set(
                    &kv.narrow(0, write.rows.start, write.rows.len())?,
                    0,
                    0,
                )?;
            }
        }
        Ok(())
    }

    pub(in crate::offline::model) fn cross_context(
        &self,
        layer: usize,
        query: &Tensor,
    ) -> Result<Tensor> {
        let query = query.squeeze(0)?;
        let mut output = Vec::with_capacity(self.groups.len());
        for group in &self.groups {
            let bank = self.decoder.groups[group.index][layer]
                .sources
                .as_ref()
                .ok_or_else(|| Error::Msg("GPU source cache has not been initialized".into()))?;
            let query = query.narrow(0, group.rows.start, group.rows.len())?;
            output.push(group.sources.context(self.decoder.heads, &query, bank)?);
        }
        join_rows(output)
    }

    pub(in crate::offline::model) fn finish(self) {
        self.decoder.cursors = self.next;
        self.decoder.step += 1;
    }
}

#[cfg(test)]
mod tests;
