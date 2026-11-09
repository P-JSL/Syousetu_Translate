// SPDX-License-Identifier: Apache-2.0
//! Only the F32 CPU layer norm and softmax used by SMaLL-100.
//! Adapted from Hugging Face Candle NN 0.11.1 ops.rs/layer_norm.rs. The
//! accumulation order and vector reductions match the upstream CPU kernels.
use crate::int8::Weights;
use candle_core::{cpu::kernels::VecOps, CpuStorage, Layout, Module, Result, Shape, Tensor, D};
use rayon::prelude::*;

fn slice<'a>(storage: &'a CpuStorage, layout: &Layout) -> Result<&'a [f32]> {
    let CpuStorage::F32(values) = storage else {
        candle_core::bail!("SMaLL-100 CPU op requires F32");
    };
    let Some((start, end)) = layout.contiguous_offsets() else {
        candle_core::bail!("CPU op requires contiguous input");
    };
    Ok(&values[start..end])
}
struct Softmax;
impl candle_core::CustomOp1 for Softmax {
    fn name(&self) -> &'static str {
        "softmax-last-dim"
    }
    fn cpu_fwd(&self, storage: &CpuStorage, layout: &Layout) -> Result<(CpuStorage, Shape)> {
        let source = slice(storage, layout)?;
        let dims = layout.shape().dims();
        let width = *dims
            .last()
            .ok_or_else(|| candle_core::Error::Msg("softmax needs a dimension".into()))?;
        if width == 0 {
            candle_core::bail!("softmax width is zero");
        }
        let mut output = vec![0f32; source.len()];
        source
            .par_chunks(width)
            .zip(output.par_chunks_mut(width))
            .for_each(|(src, dst)| {
                let mut max = f32::NEG_INFINITY;
                // SAFETY: src and dst are full contiguous rows of exactly `width`.
                unsafe { f32::vec_reduce_max(src.as_ptr(), &mut max, width) };
                for (s, d) in src.iter().zip(dst.iter_mut()) {
                    *d = (*s - max).exp();
                }
                let mut sum = 0f32;
                unsafe { f32::vec_reduce_sum(dst.as_ptr(), &mut sum, width) };
                for d in dst.iter_mut() {
                    *d /= sum;
                }
            });
        Ok((CpuStorage::F32(output), layout.shape().clone()))
    }
}
pub fn softmax_last_dim(x: &Tensor) -> Result<Tensor> {
    x.apply_op1_no_bwd(&Softmax)
}

struct NormOp {
    eps: f32,
}
impl candle_core::CustomOp3 for NormOp {
    fn name(&self) -> &'static str {
        "layer-norm"
    }
    fn cpu_fwd(
        &self,
        s: &CpuStorage,
        l: &Layout,
        a: &CpuStorage,
        al: &Layout,
        b: &CpuStorage,
        bl: &Layout,
    ) -> Result<(CpuStorage, Shape)> {
        let source = slice(s, l)?;
        let alpha = slice(a, al)?;
        let beta = slice(b, bl)?;
        let width = *l
            .shape()
            .dims()
            .last()
            .ok_or_else(|| candle_core::Error::Msg("layer norm needs a dimension".into()))?;
        if width == 0 || alpha.len() != width || beta.len() != width {
            candle_core::bail!("layer norm shape mismatch");
        }
        let mut output = vec![0f32; source.len()];
        source
            .par_chunks(width)
            .zip(output.par_chunks_mut(width))
            .for_each(|(src, dst)| {
                let (mut sum, mut sum2) = (0f32, 0f32);
                for &v in src {
                    sum += v;
                    sum2 += v * v;
                }
                let mean = sum / width as f32;
                let var = sum2 / width as f32 - mean * mean;
                let inv_std = (var + self.eps).sqrt().recip();
                for ((d, s), (a, b)) in dst.iter_mut().zip(src).zip(alpha.iter().zip(beta)) {
                    *d = (*s - mean) * inv_std * a + b;
                }
            });
        Ok((CpuStorage::F32(output), l.shape().clone()))
    }
}
#[derive(Clone)]
pub struct LayerNorm {
    weight: Tensor,
    bias: Tensor,
    eps: f64,
}
pub fn layer_norm(size: usize, eps: f64, weights: Weights) -> Result<LayerNorm> {
    Ok(LayerNorm {
        weight: weights.get(size, "weight")?,
        bias: weights.get(size, "bias")?,
        eps,
    })
}
impl Module for LayerNorm {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let width = x.dim(D::Minus1)?;
        if width != self.weight.dims1()? || width != self.bias.dims1()? {
            candle_core::bail!("layer norm shape mismatch");
        }
        if x.is_contiguous() {
            return x.apply_op3_no_bwd(
                &self.weight,
                &self.bias,
                &NormOp {
                    eps: self.eps as f32,
                },
            );
        }
        // Same noncontiguous fallback as upstream, without changing reductions.
        let centered = x.broadcast_sub(&(x.sum_keepdim(D::Minus1)? / width as f64)?)?;
        let variance = (centered.sqr()?.sum_keepdim(D::Minus1)? / width as f64)?;
        centered
            .broadcast_div(&(variance + self.eps)?.sqrt()?)?
            .broadcast_mul(&self.weight)?
            .broadcast_add(&self.bias)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;
    #[test]
    fn stable_softmax_preserves_probabilities_with_large_logits() {
        let x = Tensor::new(
            &[[1000f32, 1001., 1000., 1001.], [-2., 2., 3., -3.]],
            &Device::Cpu,
        )
        .unwrap();
        let y = softmax_last_dim(&x).unwrap().to_vec2::<f32>().unwrap();
        for row in &y {
            assert!((row.iter().sum::<f32>() - 1.).abs() < 1e-6);
            assert!(row.iter().all(|v| v.is_finite() && *v >= 0.));
        }
        assert!((y[0][0] - 0.13447072).abs() < 1e-6);
        assert!((y[1][2] - 0.7261657).abs() < 1e-6);
    }
    #[test]
    fn norm_matches_analytical_rows_and_checks_shapes() {
        let x = Tensor::new(&[[1f32, 2., 3.], [2., 2., 2.]], &Device::Cpu).unwrap();
        let layer = LayerNorm {
            weight: Tensor::new(&[1f32; 3], &Device::Cpu).unwrap(),
            bias: Tensor::new(&[0f32; 3], &Device::Cpu).unwrap(),
            eps: 1e-5,
        };
        let y = layer.forward(&x).unwrap().to_vec2::<f32>().unwrap();
        assert!((y[0][0] + 1.2247356).abs() < 1e-5);
        assert_eq!(y[1], [0., 0., 0.]);
        assert!(layer.forward(&x.narrow(1, 0, 2).unwrap()).is_err());
        let transposed = x.transpose(0, 1).unwrap();
        let two = LayerNorm {
            weight: Tensor::new(&[1f32; 2], &Device::Cpu).unwrap(),
            bias: Tensor::new(&[0f32; 2], &Device::Cpu).unwrap(),
            eps: 1e-5,
        };
        let result = two.forward(&transposed).unwrap().to_vec2::<f32>().unwrap();
        assert!(result[0][0] < -0.99);
        assert_eq!(result[1], [0., 0.]);
    }
}
