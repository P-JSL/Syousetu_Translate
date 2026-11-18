use super::{join_rows, row_index, Heads, KeyMask, GROUP_SIZE, MAX_SCORE_ELEMENTS, PAD_ROW};
use candle_core::{Device, Result, Tensor};

struct Chunk {
    rows: Option<Tensor>,
    keep: Option<Tensor>,
    start: usize,
    keys: KeyMask,
}

/// The same gather indices and key masks are used by every encoder layer.
pub(in crate::offline::model) struct EncoderPlan {
    heads: Heads,
    chunks: Vec<Chunk>,
}
impl EncoderPlan {
    pub(in crate::offline::model) fn new(
        segments: &[(usize, usize)],
        heads: Heads,
        device: &Device,
    ) -> Result<Self> {
        let mut chunks = Vec::new();
        let mut remaining = segments;
        while !remaining.is_empty() {
            let mut count = 0usize;
            let mut width = 0;
            for &(_, length) in remaining.iter().take(GROUP_SIZE) {
                let next_width = width.max(length);
                if (count + 1)
                    .checked_mul(heads.count)
                    .and_then(|size| size.checked_mul(next_width))
                    .and_then(|size| size.checked_mul(next_width))
                    .is_none_or(|size| size > MAX_SCORE_ELEMENTS)
                {
                    break;
                }
                count += 1;
                width = next_width;
            }
            if count == 0 {
                candle_core::bail!("one attention context exceeds the score memory limit");
            }
            let (group, tail) = remaining.split_at(count);
            let lengths = group.iter().map(|&(_, length)| length).collect::<Vec<_>>();
            let keys = KeyMask::new(&lengths, device)?;
            let (rows, keep) = if keys.mask.is_some() {
                let mut rows = vec![PAD_ROW; count * width];
                let mut keep = Vec::new();
                for (i, &(start, length)) in group.iter().enumerate() {
                    for position in 0..length {
                        let index = i * width + position;
                        rows[index] = row_index(start + position)?;
                        keep.push(row_index(index)?);
                    }
                }
                let rows = Tensor::from_vec(rows, count * width, device)?;
                let keeps = keep.len();
                (Some(rows), Some(Tensor::from_vec(keep, keeps, device)?))
            } else {
                (None, None)
            };
            chunks.push(Chunk {
                rows,
                keep,
                start: group[0].0,
                keys,
            });
            remaining = tail;
        }
        Ok(Self { heads, chunks })
    }

    pub(in crate::offline::model) fn context(&self, qkv: &Tensor) -> Result<Tensor> {
        let qkv = qkv.squeeze(0)?;
        let mut output = Vec::with_capacity(self.chunks.len());
        for chunk in &self.chunks {
            let KeyMask { batch, width, .. } = chunk.keys;
            let rows = match &chunk.rows {
                Some(ids) => qkv.index_select(ids, 0)?,
                None => qkv.narrow(0, chunk.start, batch * width)?,
            };
            let packed = self.heads.split(&rows, batch, width, 3)?;
            let context = chunk.keys.context(
                self.heads,
                &packed.get(0)?,
                &packed.get(1)?,
                &packed.get(2)?,
            )?;
            let context = match &chunk.keep {
                Some(ids) => context.index_select(ids, 0)?,
                None => context,
            };
            output.push(context);
        }
        join_rows(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_encoder_matches_individual_attention_and_restores_row_order() {
        let heads = Heads::new(2, 4);
        // A full group followed by a same-length tail group exercises both
        // indexed padding and contiguous no-padding views.
        let mut total = 0;
        let segments = (0..34)
            .map(|i| {
                let length = if i < 32 { 1 + i % 5 } else { 3 };
                let segment = (total, length);
                total += length;
                segment
            })
            .collect::<Vec<_>>();
        let width = 3 * heads.hidden();
        let input = Tensor::from_vec(
            (0..total * width)
                .map(|i| ((i * 13) % 199) as f32 / 98. - 1.)
                .collect::<Vec<_>>(),
            (1, total, width),
            &Device::Cpu,
        )
        .unwrap();
        let plan = EncoderPlan::new(&segments, heads, &Device::Cpu).unwrap();
        assert_eq!(plan.chunks.len(), 2);
        assert!(plan.chunks[0].rows.is_some() && plan.chunks[1].rows.is_none());
        let actual = plan.context(&input).unwrap();
        for (start, length) in segments {
            let separate = input.narrow(1, start, length).unwrap();
            let project = |part| {
                separate
                    .narrow(2, part * heads.hidden(), heads.hidden())
                    .unwrap()
                    .reshape((length, heads.count, heads.width))
                    .unwrap()
                    .transpose(0, 1)
                    .unwrap()
                    .contiguous()
                    .unwrap()
            };
            let expected = crate::offline::model::ops::attention::context(
                &project(0),
                &project(1),
                &project(2),
                (heads.width as f64).powf(-0.5),
            )
            .unwrap();
            let values = |x: Tensor| x.flatten_all().unwrap().to_vec1::<f32>().unwrap();
            for (actual, expected) in values(actual.narrow(1, start, length).unwrap())
                .into_iter()
                .zip(values(expected))
            {
                assert!((actual - expected).abs() < 2e-6);
            }
        }
    }
}
