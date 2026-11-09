// SPDX-License-Identifier: Apache-2.0
// Architecture adapted from Transformers v4.46.3 modeling_m2m_100.py.
// Copyright 2021 The Fairseq Authors and The HuggingFace Inc. team.
// See licenses/Apache-2.0.txt. Modified for Rust, quantization and isolated caches.
//! CPU M2M100 inference, following Transformers' Apache-2.0 reference architecture.
//! Pre-normalization, sinusoidal position offset, source language prefix and
//! forced Korean decoder prefix differ from Marian. Weights are shared Q8_0
//! tensors; each runner owns its attention caches.
mod config;
mod layers;
#[allow(unsafe_code)]
mod ops;
use self::ops::{layer_norm, LayerNorm};
use crate::int8::Weights;
use candle_core::{quantized::QMatMul, DType, Device, IndexOp, Module, Result, Tensor};
pub(super) use config::Config;
use layers::Layer;

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
            encoder_norm: layer_norm(cfg.d_model, 1e-5, vb.pp("encoder.layer_norm"))?,
            decoder_norm: layer_norm(cfg.d_model, 1e-5, vb.pp("decoder.layer_norm"))?,
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
    pub fn decode(&mut self, ids: &[u32], encoder: &Tensor, past: usize) -> Result<Tensor> {
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
        self.shared.forward(&last)?.squeeze(0)
    }
    pub fn reset(&mut self) {
        for layer in self.encoder.iter_mut().chain(self.decoder.iter_mut()) {
            layer.reset();
        }
    }
}
