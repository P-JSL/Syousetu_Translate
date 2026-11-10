// SPDX-License-Identifier: Apache-2.0
// Architecture adapted from Transformers v4.46.3 modeling_m2m_100.py.
// Copyright 2021 The Fairseq Authors and The HuggingFace Inc. team.
// See licenses/Apache-2.0.txt. Modified for Rust, quantization and isolated caches.
//! CPU M2M100 inference, following Transformers' Apache-2.0 reference architecture.
//! Pre-normalization, sinusoidal position offset, source language prefix and
//! forced Korean decoder prefix differ from Marian. Weights are shared Q8_0
//! tensors; each runner owns its attention caches.
macro_rules! profile_scope {
    ($phase:ident) => {
        #[cfg(feature = "inference-profile")]
        let _profile_scope =
            crate::offline::profiling::Scope::start(crate::offline::profiling::Stage::$phase);
    };
}
macro_rules! profile_op {
    ($phase:ident, $operation:expr) => {{
        #[cfg(feature = "inference-profile")]
        {
            crate::offline::profiling::measure(crate::offline::profiling::Stage::$phase, || {
                $operation
            })
        }
        #[cfg(not(feature = "inference-profile"))]
        {
            $operation
        }
    }};
}
mod config;
mod layers;
#[allow(unsafe_code)]
mod ops;
mod projection;
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
    greedy: projection::Projection,
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
            greedy: projection::Projection::default(),
        })
    }
    fn embed(&self, ids: &[u32], past: usize) -> Result<Tensor> {
        profile_scope!(Embedding);
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
    fn embed_rows(&self, ids: &[u32], positions: &[u32]) -> Result<Tensor> {
        profile_scope!(Embedding);
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
                .index_select(&Tensor::new(positions, &Device::Cpu)?, 0)?
                .unsqueeze(0)?,
        )
    }
    pub fn encode(&mut self, ids: &[u32]) -> Result<Tensor> {
        profile_scope!(Encoder);
        let mut x = self.embed(ids, 0)?;
        for layer in &mut self.encoder {
            x = layer.forward(&x, None, None, false)?;
        }
        x.apply(&self.encoder_norm)
    }
    pub fn decode(&mut self, ids: &[u32], encoder: &Tensor, past: usize) -> Result<Tensor> {
        profile_scope!(Decoder);
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
        profile_op!(Projection, self.shared.forward(&last))?.squeeze(0)
    }
    pub fn reset(&mut self) {
        for layer in self.encoder.iter_mut().chain(self.decoder.iter_mut()) {
            layer.reset();
        }
    }

    pub fn encode_many(&self, sources: &[Vec<u32>]) -> Result<Vec<Tensor>> {
        profile_scope!(Encoder);
        let mut start = 0;
        let segments = sources
            .iter()
            .map(|source| {
                let segment = (start, source.len());
                start += source.len();
                segment
            })
            .collect::<Vec<_>>();
        let ids = sources.iter().flatten().copied().collect::<Vec<_>>();
        let positions = sources
            .iter()
            .flat_map(|source| (2..source.len() + 2).map(|position| position as u32))
            .collect::<Vec<_>>();
        let mut x = self.embed_rows(&ids, &positions)?;
        for layer in &self.encoder {
            x = layer.encode_segments(&x, &segments)?;
        }
        let x = x.apply(&self.encoder_norm)?;
        segments
            .iter()
            .map(|&(start, length)| x.narrow(1, start, length))
            .collect()
    }

    pub fn decode_many(
        models: &mut [&mut Self],
        inputs: &[u32],
        past: &[usize],
        encoders: &[Tensor],
        vocab: usize,
        blocked: &[std::collections::HashSet<u32>],
    ) -> Result<Vec<u32>> {
        profile_scope!(Decoder);
        let positions = past
            .iter()
            .map(|position| (position + 2) as u32)
            .collect::<Vec<_>>();
        let mut x = models[0].embed_rows(inputs, &positions)?;
        for index in 0..models[0].decoder.len() {
            let mut layers = models
                .iter_mut()
                .map(|model| &mut model.decoder[index])
                .collect::<Vec<_>>();
            x = Layer::decode_rows(&mut layers, &x, encoders)?;
        }
        let last = x.apply(&models[0].decoder_norm)?.squeeze(0)?;
        profile_op!(
            Projection,
            models[0].greedy.forward(
                &models[0].shared,
                &last,
                vocab,
                models[0].config.eos_token_id,
                blocked,
            )
        )
    }
}
