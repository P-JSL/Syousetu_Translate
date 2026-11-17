//! Selective batching: shared projections/FFN, private attention and KV caches.
//! CPU tensors are unpadded. Device attention uses bounded masked padding;
//! no padding tokens enter the model or cross-sentence attention is introduced.
use super::Layer;
use candle_core::{Result, Tensor};
use rayon::prelude::*;
#[path = "gpu_batch.rs"]
mod gpu_batch;

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
            if !x.device().is_cpu() {
                let q = self.attention.shape(qkv.narrow(2, 0, dim)?)?;
                let k = self.attention.shape(qkv.narrow(2, dim, dim)?)?;
                let v = self.attention.shape(qkv.narrow(2, 2 * dim, dim)?)?;
                let segments_of = |tensor: &Tensor| {
                    segments
                        .iter()
                        .map(|&(start, length)| tensor.narrow(1, start, length))
                        .collect::<Result<Vec<_>>>()
                };
                gpu_batch::contexts(
                    &segments_of(&q)?,
                    &segments_of(&k)?,
                    &segments_of(&v)?,
                    self.attention.heads,
                    self.attention.head_dim,
                )
            } else {
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
            }
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
            if !x.device().is_cpu() {
                gpu_self_contexts(layers, &qkv, dim)
            } else {
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
                            #[cfg(feature = "inference-profile")]
                            let _append =
                                crate::offline::profiling::kv_append(&[old_k, old_v, &k, &v]);
                            (Tensor::cat(&[old_k, &k], 1)?, Tensor::cat(&[old_v, &v], 1)?)
                        } else {
                            (k, v)
                        };
                        attention.cache = Some((k.clone(), v.clone()));
                        attention.context(q, k, v, 1, None)
                    })
                    .collect::<Result<Vec<_>>>()
            }
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
            if !x.device().is_cpu() {
                gpu_cross_contexts(layers, &q)
            } else {
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
            }
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

fn gpu_self_contexts(layers: &mut [&mut Layer], qkv: &Tensor, dim: usize) -> Result<Vec<Tensor>> {
    let heads = layers[0].attention.heads;
    let head_dim = layers[0].attention.head_dim;
    let q = layers[0].attention.shape(qkv.narrow(2, 0, dim)?)?;
    let k = layers[0].attention.shape(qkv.narrow(2, dim, dim)?)?;
    let v = layers[0].attention.shape(qkv.narrow(2, 2 * dim, dim)?)?;
    let mut queries = Vec::with_capacity(layers.len());
    let mut keys = Vec::with_capacity(layers.len());
    let mut values = Vec::with_capacity(layers.len());
    for (index, layer) in layers.iter_mut().enumerate() {
        let attention = &mut layer.attention;
        let q = q.narrow(1, index, 1)?;
        let k = k.narrow(1, index, 1)?;
        let v = v.narrow(1, index, 1)?;
        let (k, v) = if let Some((old_k, old_v)) = &attention.cache {
            #[cfg(feature = "inference-profile")]
            let _append = crate::offline::profiling::kv_append(&[old_k, old_v, &k, &v]);
            (Tensor::cat(&[old_k, &k], 1)?, Tensor::cat(&[old_v, &v], 1)?)
        } else {
            (k, v)
        };
        attention.cache = Some((k.clone(), v.clone()));
        queries.push(q);
        keys.push(k);
        values.push(v);
    }
    gpu_batch::contexts(&queries, &keys, &values, heads, head_dim)
}

fn gpu_cross_contexts(layers: &[&mut Layer], q: &Tensor) -> Result<Vec<Tensor>> {
    let first = layers[0].cross.as_ref().expect("decoder cross attention");
    let q = first.shape(q.clone())?;
    let mut queries = Vec::with_capacity(layers.len());
    let mut keys = Vec::with_capacity(layers.len());
    let mut values = Vec::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        let cross = layer.cross.as_ref().expect("decoder cross attention");
        let (k, v) = cross.cache.as_ref().expect("initialized cross cache");
        queries.push(q.narrow(1, index, 1)?);
        keys.push(k.clone());
        values.push(v.clone());
    }
    gpu_batch::contexts(&queries, &keys, &values, first.heads, first.head_dim)
}
