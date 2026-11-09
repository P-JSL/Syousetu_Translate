// SPDX-License-Identifier: Apache-2.0
// Architecture adapted from Transformers v4.46.3 modeling_m2m_100.py.
// Copyright 2021 The Fairseq Authors and The HuggingFace Inc. team.
// See licenses/Apache-2.0.txt. Modified for Rust, quantization and isolated caches.
//! CPU M2M100 inference, following Transformers' Apache-2.0 reference architecture.
//! Pre-normalization, sinusoidal position offset, source language prefix and
//! forced Korean decoder prefix differ from Marian. Weights are shared Q8_0
//! tensors; each runner owns its attention caches.
use crate::int8::Weights;
use candle_core::{quantized::QMatMul, DType, Device, IndexOp, Module, Result, Tensor};
use candle_nn::{layer_norm, LayerNorm};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub vocab_size: usize,
    pub d_model: usize,
    pub encoder_layers: usize,
    pub decoder_layers: usize,
    pub encoder_attention_heads: usize,
    pub decoder_attention_heads: usize,
    pub encoder_ffn_dim: usize,
    pub decoder_ffn_dim: usize,
    pub max_position_embeddings: usize,
    pub pad_token_id: u32,
    pub eos_token_id: u32,
    pub decoder_start_token_id: u32,
    pub scale_embedding: bool,
}
#[derive(Clone)]
struct Linear {
    weight: QMatMul,
    bias: Tensor,
}
impl Linear {
    fn new(input: usize, output: usize, vb: Weights) -> Result<Self> {
        Ok(Self {
            weight: vb.matrix("weight", (output, input))?,
            bias: vb.floats().get(output, "bias")?,
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
        let q = (q * (self.head_dim as f64).powf(-0.5))?;
        if cache {
            self.cache = Some((k.clone(), v.clone()));
        }
        let scores = q.matmul(&k.transpose(1, 2)?)?;
        let scores = match mask {
            Some(mask) => scores.broadcast_add(mask)?,
            None => scores,
        };
        let context = candle_nn::ops::softmax_last_dim(&scores)?
            .matmul(&v)?
            .reshape((1, self.heads, length, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((1, length, self.heads * self.head_dim))?;
        self.out.forward(&context)
    }
}
#[derive(Clone)]
struct Layer {
    attention: Attention,
    attention_norm: LayerNorm,
    cross: Option<Attention>,
    cross_norm: Option<LayerNorm>,
    ff1: Linear,
    ff2: Linear,
    ff_norm: LayerNorm,
}
impl Layer {
    fn new(cfg: &Config, decoder: bool, vb: Weights) -> Result<Self> {
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
            attention_norm: layer_norm(dim, 1e-5, vb.pp("self_attn_layer_norm").floats())?,
            cross: if decoder {
                Some(Attention::new(dim, heads, true, vb.pp("encoder_attn"))?)
            } else {
                None
            },
            cross_norm: if decoder {
                Some(layer_norm(
                    dim,
                    1e-5,
                    vb.pp("encoder_attn_layer_norm").floats(),
                )?)
            } else {
                None
            },
            ff1: Linear::new(dim, ff, vb.pp("fc1"))?,
            ff2: Linear::new(ff, dim, vb.pp("fc2"))?,
            ff_norm: layer_norm(dim, 1e-5, vb.pp("final_layer_norm").floats())?,
        })
    }
    fn forward(
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
    fn reset(&mut self) {
        self.attention.cache = None;
        if let Some(cross) = &mut self.cross {
            cross.cache = None;
        }
    }
}
#[derive(Clone)]
pub struct Model {
    config: Config,
    shared: QMatMul,
    positions: Tensor,
    encoder: Vec<Layer>,
    encoder_norm: LayerNorm,
    decoder: Vec<Layer>,
    decoder_norm: LayerNorm,
}
impl Model {
    pub fn new(cfg: &Config, vb: Weights) -> Result<Self> {
        let vb = vb.pp("model");
        let shared = vb.matrix("shared.weight", (cfg.vocab_size, cfg.d_model))?;
        let half = cfg.d_model / 2;
        let frequencies = (0..half)
            .map(|i| (-(i as f32) * (10000f32.ln() / (half - 1) as f32)).exp())
            .collect::<Vec<_>>();
        let mut positions = Vec::with_capacity((cfg.max_position_embeddings + 2) * cfg.d_model);
        for position in 0..cfg.max_position_embeddings + 2 {
            for cosine in [false, true] {
                positions.extend(frequencies.iter().map(|frequency| {
                    if position == cfg.pad_token_id as usize {
                        0f32
                    } else if cosine {
                        (position as f32 * frequency).cos()
                    } else {
                        (position as f32 * frequency).sin()
                    }
                }));
            }
        }
        let positions = Tensor::from_vec(
            positions,
            (cfg.max_position_embeddings + 2, cfg.d_model),
            &Device::Cpu,
        )?;
        let encoder = (0..cfg.encoder_layers)
            .map(|i| Layer::new(cfg, false, vb.pp(format!("encoder.layers.{i}"))))
            .collect::<Result<Vec<_>>>()?;
        let decoder = (0..cfg.decoder_layers)
            .map(|i| Layer::new(cfg, true, vb.pp(format!("decoder.layers.{i}"))))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            config: cfg.clone(),
            shared,
            positions,
            encoder,
            decoder,
            encoder_norm: layer_norm(cfg.d_model, 1e-5, vb.pp("encoder.layer_norm").floats())?,
            decoder_norm: layer_norm(cfg.d_model, 1e-5, vb.pp("decoder.layer_norm").floats())?,
        })
    }
    fn embed(&self, ids: &[u32], past: usize) -> Result<Tensor> {
        // Inputs have no padding; source and target language prefixes are real tokens.
        let x = self
            .shared
            .embedding(&Tensor::new(ids, &Device::Cpu)?)?
            .to_dtype(DType::F32)?
            .unsqueeze(0)?;
        let x = if self.config.scale_embedding {
            (x * (self.config.d_model as f64).sqrt())?
        } else {
            x
        };
        x.broadcast_add(
            &self
                .positions
                .narrow(0, past + 2, ids.len())?
                .unsqueeze(0)?,
        )
    }
    pub fn encode(&mut self, ids: &[u32]) -> Result<Tensor> {
        let mut x = self.embed(ids, 0)?;
        for layer in &mut self.encoder {
            x = layer.forward(&x, None, None, false)?;
        }
        x.apply(&self.encoder_norm)
    }
    pub fn decode(&mut self, ids: &[u32], encoder: &Tensor, past: usize) -> Result<Vec<f32>> {
        let mut x = self.embed(ids, past)?;
        let length = ids.len();
        let mask = if length > 1 {
            let data = (0..length)
                .flat_map(|i| {
                    (0..past + length).map(move |j| {
                        if j > past + i {
                            f32::NEG_INFINITY
                        } else {
                            0f32
                        }
                    })
                })
                .collect::<Vec<_>>();
            Some(Tensor::from_vec(
                data,
                (length, past + length),
                &Device::Cpu,
            )?)
        } else {
            None
        };
        for layer in &mut self.decoder {
            x = layer.forward(&x, Some(encoder), mask.as_ref(), true)?;
        }
        let last = x
            .apply(&self.decoder_norm)?
            .i((0, length - 1))?
            .unsqueeze(0)?;
        self.shared.forward(&last)?.squeeze(0)?.to_vec1::<f32>()
    }
    pub fn reset(&mut self) {
        for layer in self.encoder.iter_mut().chain(self.decoder.iter_mut()) {
            layer.reset();
        }
    }
}
