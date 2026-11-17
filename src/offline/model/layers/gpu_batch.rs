//! Bounded device attention over independent, variable-length sentences.
//! Padding exists only inside each attention batch: masked keys cannot be
//! attended to, and padded query rows are discarded before residual/FFN work.
use crate::offline::model::ops::softmax_last_dim;
use candle_core::{DType, Result, Tensor};

const MAX_CONTEXTS: usize = 32;
// At most 64 MiB in each F32 score tensor, not a global GPU memory limit.
const MAX_SCORE_ELEMENTS: usize = 16 * 1024 * 1024;

fn score_elements(batch: usize, heads: usize, queries: usize, keys: usize) -> Option<usize> {
    batch
        .checked_mul(heads)?
        .checked_mul(queries)?
        .checked_mul(keys)
}

pub(super) fn contexts(
    queries: &[Tensor],
    keys: &[Tensor],
    values: &[Tensor],
    heads: usize,
    head_dim: usize,
) -> Result<Vec<Tensor>> {
    if queries.is_empty()
        || queries.len() != keys.len()
        || queries.len() != values.len()
        || heads == 0
        || head_dim == 0
    {
        candle_core::bail!("batched attention shape mismatch");
    }
    let device = queries[0].device();
    let lengths = queries
        .iter()
        .zip(keys)
        .zip(values)
        .map(|((q, k), v)| {
            let (qh, qlen, qd) = q.dims3()?;
            let (kh, klen, kd) = k.dims3()?;
            if qh != heads
                || kh != heads
                || qd != head_dim
                || kd != head_dim
                || qlen == 0
                || klen == 0
                || v.dims() != [heads, klen, head_dim]
                || !device.same_device(q.device())
                || !device.same_device(k.device())
                || !device.same_device(v.device())
            {
                candle_core::bail!("batched attention shape or device mismatch");
            }
            Ok((qlen, klen))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut output = Vec::with_capacity(queries.len());
    let mut start = 0;
    while start < queries.len() {
        let (mut end, mut max_q, mut max_k) = (start, 0, 0);
        while end < queries.len() && end - start < MAX_CONTEXTS {
            let qlen = max_q.max(lengths[end].0);
            let klen = max_k.max(lengths[end].1);
            if score_elements(end - start + 1, heads, qlen, klen)
                .is_none_or(|count| count > MAX_SCORE_ELEMENTS)
            {
                if end == start {
                    candle_core::bail!("one attention context exceeds the score memory limit");
                }
                break;
            }
            max_q = qlen;
            max_k = klen;
            end += 1;
        }

        let pad = |inputs: &[Tensor], width: usize| {
            let padded = inputs
                .iter()
                .map(|input| {
                    input
                        .to_dtype(DType::F32)?
                        .pad_with_zeros(1, 0, width - input.dim(1)?)
                })
                .collect::<Result<Vec<_>>>()?;
            Tensor::cat(&padded, 0)
        };
        let q = pad(&queries[start..end], max_q)?.contiguous()?;
        let k = pad(&keys[start..end], max_k)?
            .transpose(1, 2)?
            .contiguous()?;
        let v = pad(&values[start..end], max_k)?.contiguous()?;
        let batch_heads = (end - start) * heads;
        let mut mask = vec![0f32; batch_heads * max_k];
        for (index, &(_, valid_keys)) in lengths[start..end].iter().enumerate() {
            for head in 0..heads {
                let row = (index * heads + head) * max_k;
                mask[row + valid_keys..row + max_k].fill(f32::NEG_INFINITY);
            }
        }
        let mask = Tensor::from_vec(mask, (batch_heads, 1, max_k), device)?;
        let scaled = (q * (head_dim as f64).powf(-0.5))?;
        let scores = scaled.matmul(&k)?.broadcast_add(&mask)?;
        let joined = softmax_last_dim(&scores)?.matmul(&v)?.reshape((
            end - start,
            heads,
            max_q,
            head_dim,
        ))?;
        for (index, &(valid_queries, _)) in lengths[start..end].iter().enumerate() {
            output.push(
                joined
                    .narrow(0, index, 1)?
                    .narrow(2, 0, valid_queries)?
                    .transpose(1, 2)?
                    .contiguous()?
                    .reshape((1, valid_queries, heads * head_dim))?,
            );
        }
        start = end;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline::model::ops::attention;
    use candle_core::Device;

    fn flattened(tensor: &Tensor) -> Vec<f32> {
        tensor.flatten_all().unwrap().to_vec1::<f32>().unwrap()
    }

    #[test]
    fn variable_lengths_match_independent_attention_and_isolate_sources() {
        let device = Device::Cpu;
        let (heads, dim) = (2, 4);
        let mut seed = 37u64;
        let mut make = |length| {
            Tensor::from_vec(
                (0..heads * length * dim)
                    .map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        (seed >> 32) as i32 as f32 / i32::MAX as f32
                    })
                    .collect::<Vec<_>>(),
                (heads, length, dim),
                &device,
            )
            .unwrap()
        };
        let queries = [1, 3, 2].map(&mut make);
        let keys = [4, 1, 5].map(&mut make);
        let mut values = [4, 1, 5].map(&mut make);
        let actual = contexts(&queries, &keys, &values, heads, dim).unwrap();
        for (index, result) in actual.iter().enumerate() {
            let expected = attention::context(
                &queries[index],
                &keys[index],
                &values[index],
                (dim as f64).powf(-0.5),
            )
            .unwrap();
            assert_eq!(result.dims(), expected.dims());
            for (actual, expected) in flattened(result).into_iter().zip(flattened(&expected)) {
                assert!((actual - expected).abs() < 2e-6);
            }
        }
        values[1] = (&values[1] * 100_000.).unwrap();
        let changed = contexts(&queries, &keys, &values, heads, dim).unwrap();
        assert_eq!(flattened(&actual[0]), flattened(&changed[0]));
        assert_eq!(flattened(&actual[2]), flattened(&changed[2]));
        assert_ne!(flattened(&actual[1]), flattened(&changed[1]));
    }

    #[test]
    fn chunking_preserves_order_and_rejects_oversized_single_contexts() {
        let device = Device::Cpu;
        let queries = (0..40)
            .map(|_| Tensor::zeros((1, 1, 1), DType::F32, &device).unwrap())
            .collect::<Vec<_>>();
        let values = (0..40)
            .map(|index| Tensor::new(&[[[index as f32 + 1.]]], &device).unwrap())
            .collect::<Vec<_>>();
        let actual = contexts(&queries, &queries, &values, 1, 1).unwrap();
        assert_eq!(actual.len(), 40);
        for (index, result) in actual.iter().enumerate() {
            assert_eq!(flattened(result), [index as f32 + 1.]);
        }
        let too_large = Tensor::zeros((16, 1025, 1), DType::F32, &device).unwrap();
        assert!(contexts(
            std::slice::from_ref(&too_large),
            std::slice::from_ref(&too_large),
            std::slice::from_ref(&too_large),
            16,
            1,
        )
        .is_err());
    }
}
