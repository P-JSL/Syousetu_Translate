// SPDX-License-Identifier: Apache-2.0
// Adapted from Transformers v4.46.3; numerical operations unchanged.
mod batch;
use super::ops::{layer_norm, LayerNorm};
use crate::int8::Weights;
use candle_core::{quantized::QMatMul, Module, Result, Tensor};

use super::config::Config;
#[derive(Clone)]
struct Linear {
    weight: QMatMul,
    bias: Tensor,
}
impl Linear {
    fn new(input: usize, output: usize, vb: Weights) -> Result<Self> {
        Ok(Self {
            weight: vb.matrix("weight", (output, input))?,
            bias: vb.get(output, "bias")?,
        })
    }
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        self.weight.forward(x)?.broadcast_add(&self.bias)
    }
}
#[derive(Clone)]
struct Attention {
    qkv: Linear,
    cross_q: Option<Linear>,
    out: Linear,
    heads: usize,
    head_dim: usize,
    cache: Option<(Tensor, Tensor)>,
}
impl Attention {
    fn new(dim: usize, heads: usize, cross: bool, vb: Weights) -> Result<Self> {
        Ok(Self {
            qkv: Linear::new(
                dim,
                dim * if cross { 2 } else { 3 },
                vb.pp(if cross { "kv_proj" } else { "qkv_proj" }),
            )?,
            cross_q: if cross {
                Some(Linear::new(dim, dim, vb.pp("q_proj"))?)
            } else {
                None
            },
            out: Linear::new(dim, dim, vb.pp("out_proj"))?,
            heads,
            head_dim: dim / heads,
            cache: None,
        })
    }
    fn shape(&self, x: Tensor) -> Result<Tensor> {
        x.reshape((1, (), self.heads, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((self.heads, (), self.head_dim))
    }
    fn forward(
        &mut self,
        x: &Tensor,
        encoder: Option<&Tensor>,
        mask: Option<&Tensor>,
        cache: bool,
    ) -> Result<Tensor> {
        let length = x.dim(1)?;
        let dim = self.heads * self.head_dim;
        // A single projection shares input reads and scheduling across Q/K/V.
        // Quantization is row-wise, so concatenation preserves each weight block.
        let (q, k, v) = if let Some(source) = encoder {
            let q = self.shape(self.cross_q.as_ref().expect("cross attention").forward(x)?)?;
            let (k, v) = if let Some((k, v)) = &self.cache {
                (k.clone(), v.clone())
            } else {
                let kv = self.qkv.forward(source)?;
                (
                    self.shape(kv.narrow(2, 0, dim)?)?,
                    self.shape(kv.narrow(2, dim, dim)?)?,
                )
            };
            (q, k, v)
        } else {
            let qkv = self.qkv.forward(x)?;
            let q = self.shape(qkv.narrow(2, 0, dim)?)?;
            let k = self.shape(qkv.narrow(2, dim, dim)?)?;
            let v = self.shape(qkv.narrow(2, 2 * dim, dim)?)?;
            let (k, v) = match &self.cache {
                Some((old_k, old_v)) if cache => {
                    (Tensor::cat(&[old_k, &k], 1)?, Tensor::cat(&[old_v, &v], 1)?)
                }
                _ => (k, v),
            };
            (q, k, v)
        };
        if cache {
            self.cache = Some((k.clone(), v.clone()));
        }
        self.out.forward(&self.context(q, k, v, length, mask)?)
    }
    fn context(
        &self,
        q: Tensor,
        k: Tensor,
        v: Tensor,
        length: usize,
        mask: Option<&Tensor>,
    ) -> Result<Tensor> {
        let q = (q * (self.head_dim as f64).powf(-0.5))?;
        let scores = q.matmul(&k.transpose(1, 2)?)?;
        let scores = match mask {
            Some(mask) => scores.broadcast_add(mask)?,
            None => scores,
        };
        let context = super::ops::softmax_last_dim(&scores)?
            .matmul(&v)?
            .reshape((1, self.heads, length, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((1, length, self.heads * self.head_dim))?;
        Ok(context)
    }
}
#[derive(Clone)]
pub(super) struct Layer {
    attention: Attention,
    attention_norm: LayerNorm,
    cross: Option<Attention>,
    cross_norm: Option<LayerNorm>,
    ff1: Linear,
    ff2: Linear,
    ff_norm: LayerNorm,
}
impl Layer {
    pub(super) fn new(cfg: &Config, decoder: bool, vb: Weights) -> Result<Self> {
        let dim = cfg.d_model;
        let heads = if decoder {
            cfg.decoder_attention_heads
        } else {
            cfg.encoder_attention_heads
        };
        let ff = if decoder {
            cfg.decoder_ffn_dim
        } else {
            cfg.encoder_ffn_dim
        };
        Ok(Self {
            attention: Attention::new(dim, heads, false, vb.pp("self_attn"))?,
            attention_norm: layer_norm(dim, 1e-5, vb.pp("self_attn_layer_norm"))?,
            cross: if decoder {
                Some(Attention::new(dim, heads, true, vb.pp("encoder_attn"))?)
            } else {
                None
            },
            cross_norm: if decoder {
                Some(layer_norm(dim, 1e-5, vb.pp("encoder_attn_layer_norm"))?)
            } else {
                None
            },
            ff1: Linear::new(dim, ff, vb.pp("fc1"))?,
            ff2: Linear::new(ff, dim, vb.pp("fc2"))?,
            ff_norm: layer_norm(dim, 1e-5, vb.pp("final_layer_norm"))?,
        })
    }
    pub(super) fn forward(
        &mut self,
        x: &Tensor,
        encoder: Option<&Tensor>,
        mask: Option<&Tensor>,
        cache: bool,
    ) -> Result<Tensor> {
        let normalized = x.apply(&self.attention_norm)?;
        let mut x = (x + self.attention.forward(&normalized, None, mask, cache)?)?;
        if let (Some(cross), Some(norm), Some(encoder)) =
            (&mut self.cross, &self.cross_norm, encoder)
        {
            let normalized = x.apply(norm)?;
            x = (&x + cross.forward(&normalized, Some(encoder), None, true)?)?;
        }
        let ff = self
            .ff2
            .forward(&self.ff1.forward(&x.apply(&self.ff_norm)?)?.relu()?)?;
        x + ff
    }
    pub(super) fn reset(&mut self) {
        self.attention.cache = None;
        if let Some(cross) = &mut self.cross {
            cross.cache = None;
        }
    }
}
