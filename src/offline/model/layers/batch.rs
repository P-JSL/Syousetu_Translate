//! Selective batching: shared projections/FFN, private attention and KV caches.
//! No padding tokens or cross-sentence attention are introduced.
use super::Layer;
use candle_core::{Result, Tensor};
use rayon::prelude::*;

impl Layer {
    pub(in crate::offline::model) fn encode_segments(
        &self,
        x: &Tensor,
        segments: &[(usize, usize)],
    ) -> Result<Tensor> {
        let normalized = x.apply(&self.attention_norm)?;
        let qkv = profile_op!(BatchLinear, self.attention.qkv.forward(&normalized))?;
        let dim = self.attention.heads * self.attention.head_dim;
        let contexts = profile_op!(
            BatchAttention,
            segments
                .par_iter()
                .map(|&(start, length)| {
                    let projected = qkv.narrow(1, start, length)?;
                    self.attention.context(
                        self.attention.shape(projected.narrow(2, 0, dim)?)?,
                        self.attention.shape(projected.narrow(2, dim, dim)?)?,
                        self.attention.shape(projected.narrow(2, 2 * dim, dim)?)?,
                        length,
                        None,
                    )
                })
                .collect::<Result<Vec<_>>>()
        )?;
        let context = Tensor::cat(&contexts, 1)?;
        let x = profile_op!(
            BatchLinear,
            self.attention.out.forward_residual(&context, x)
        )?;
        profile_op!(
            BatchFfn,
            self.ff2
                .forward_residual(&self.ff1.forward_relu(&x.apply(&self.ff_norm)?)?, &x)
        )
    }

    pub(in crate::offline::model) fn decode_rows(
        layers: &mut [&mut Self],
        x: &Tensor,
        encoders: &[Tensor],
    ) -> Result<Tensor> {
        let normalized = x.apply(&layers[0].attention_norm)?;
        let qkv = profile_op!(BatchLinear, layers[0].attention.qkv.forward(&normalized))?;
        let dim = layers[0].attention.heads * layers[0].attention.head_dim;
        let contexts = profile_op!(
            BatchAttention,
            layers
                .par_iter_mut()
                .enumerate()
                .map(|(index, layer)| {
                    let projected = qkv.narrow(1, index, 1)?;
                    let attention = &mut layer.attention;
                    let q = attention.shape(projected.narrow(2, 0, dim)?)?;
                    let k = attention.shape(projected.narrow(2, dim, dim)?)?;
                    let v = attention.shape(projected.narrow(2, 2 * dim, dim)?)?;
                    let (k, v) = if let Some((old_k, old_v)) = &attention.cache {
                        (Tensor::cat(&[old_k, &k], 1)?, Tensor::cat(&[old_v, &v], 1)?)
                    } else {
                        (k, v)
                    };
                    attention.cache = Some((k.clone(), v.clone()));
                    attention.context(q, k, v, 1, None)
                })
                .collect::<Result<Vec<_>>>()
        )?;
        let context = Tensor::cat(&contexts, 1)?;
        let mut x = profile_op!(
            BatchLinear,
            layers[0].attention.out.forward_residual(&context, x)
        )?;

        // Project new source K/V together; completed contexts are replaced on
        // the next iteration without waiting for the longest sentence.
        let missing = layers
            .iter()
            .enumerate()
            .filter_map(|(index, layer)| {
                layer
                    .cross
                    .as_ref()
                    .filter(|cross| cross.cache.is_none())
                    .map(|_| index)
            })
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            let sources = missing.iter().map(|&i| &encoders[i]).collect::<Vec<_>>();
            let joined = Tensor::cat(&sources, 1)?;
            let kv = profile_op!(
                BatchLinear,
                layers[0]
                    .cross
                    .as_ref()
                    .expect("decoder cross attention")
                    .qkv
                    .forward(&joined)
            )?;
            let mut start = 0;
            for &index in &missing {
                let length = encoders[index].dim(1)?;
                let projected = kv.narrow(1, start, length)?;
                let cross = layers[index]
                    .cross
                    .as_mut()
                    .expect("decoder cross attention");
                cross.cache = Some((
                    cross.shape(projected.narrow(2, 0, dim)?)?,
                    cross.shape(projected.narrow(2, dim, dim)?)?,
                ));
                start += length;
            }
        }
        let normalized = x.apply(layers[0].cross_norm.as_ref().expect("cross norm"))?;
        let q = profile_op!(
            BatchLinear,
            layers[0]
                .cross
                .as_ref()
                .expect("decoder cross attention")
                .cross_q
                .as_ref()
                .expect("cross query")
                .forward(&normalized)
        )?;
        let contexts = profile_op!(
            BatchAttention,
            layers
                .par_iter()
                .enumerate()
                .map(|(index, layer)| {
                    let cross = layer.cross.as_ref().expect("decoder cross attention");
                    let (k, v) = cross.cache.as_ref().expect("initialized cross cache");
                    cross.context(
                        cross.shape(q.narrow(1, index, 1)?)?,
                        k.clone(),
                        v.clone(),
                        1,
                        None,
                    )
                })
                .collect::<Result<Vec<_>>>()
        )?;
        let context = Tensor::cat(&contexts, 1)?;
        x = profile_op!(
            BatchLinear,
            layers[0]
                .cross
                .as_ref()
                .expect("decoder cross attention")
                .out
                .forward_residual(&context, &x)
        )?;
        profile_op!(
            BatchFfn,
            layers[0].ff2.forward_residual(
                &layers[0].ff1.forward_relu(&x.apply(&layers[0].ff_norm)?)?,
                &x
            )
        )
    }
}
