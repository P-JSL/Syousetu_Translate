//! Apply bias and ReLU in the CPU projection's own allocation.
use candle_core::{CpuStorage, DType, InplaceOp2, Layout, Result, Tensor};
use rayon::prelude::*;
struct BiasRelu;
impl InplaceOp2 for BiasRelu {
    fn name(&self) -> &'static str {
        "bias-relu"
    }
    fn cpu_fwd(
        &self,
        x: &mut CpuStorage,
        xl: &Layout,
        bias: &CpuStorage,
        bl: &Layout,
    ) -> Result<()> {
        let output = super::slice_mut(x, xl)?;
        let bias = super::slice(bias, bl)?;
        let Some(&width) = xl.shape().dims().last() else {
            candle_core::bail!("bias ReLU needs rows");
        };
        if width == 0 || bl.shape().dims() != [width] {
            candle_core::bail!("bias ReLU shape mismatch");
        }
        let run = |row: &mut [f32]| {
            for (value, &bias) in row.iter_mut().zip(bias) {
                *value = (*value + bias).max(0f32);
            }
        };
        if output.len() >= 65_536 && rayon::current_num_threads() > 1 {
            output.par_chunks_mut(width).for_each(run);
        } else {
            output.chunks_mut(width).for_each(run);
        }
        #[cfg(feature = "inference-profile")]
        crate::offline::profiling::observe_relu(output, width);
        Ok(())
    }
}
/// This tensor is the freshly computed matrix projection and may be modified.
pub(in crate::offline::model) fn forward(projected: Tensor, bias: &Tensor) -> Result<Tensor> {
    if !projected.device().is_cpu() {
        return tensor_forward(&projected, bias);
    }
    if !projected.is_contiguous() || !bias.is_contiguous() {
        return projected.broadcast_add(bias)?.relu();
    }
    projected.inplace_op2(bias, &BiasRelu)?;
    Ok(projected)
}

fn tensor_forward(x: &Tensor, bias: &Tensor) -> Result<Tensor> {
    x.to_dtype(DType::F32)?
        .broadcast_add(&bias.to_dtype(DType::F32)?)?
        .relu()
}
#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;
    #[test]
    fn fusion_matches_tensor_operations_including_zero_nan_and_layout_offsets() {
        for (rows, width) in [(1, 16), (17, 1024), (129, 4096)] {
            let mut values = (0..(rows + 1) * width)
                .map(|i| ((i * 17 % 997) as f32 - 498.0) / 127.0)
                .collect::<Vec<_>>();
            values[width..width + 5].copy_from_slice(&[
                0.,
                -0.,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ]);
            let x = Tensor::from_vec(values, (rows + 1, width), &Device::Cpu)
                .unwrap()
                .narrow(0, 1, rows)
                .unwrap();
            let bias = Tensor::from_vec(
                (0..width)
                    .map(|i| (i % 7) as f32 * -0.25)
                    .collect::<Vec<_>>(),
                width,
                &Device::Cpu,
            )
            .unwrap();
            let expected = x.broadcast_add(&bias).unwrap().relu().unwrap();
            let actual = forward(x, &bias).unwrap();
            assert_eq!(
                actual
                    .flatten_all()
                    .unwrap()
                    .to_vec1::<f32>()
                    .unwrap()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .flatten_all()
                    .unwrap()
                    .to_vec1::<f32>()
                    .unwrap()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn portable_bias_addition_promotes_before_relu() {
        let x = Tensor::new(&[[-0.5f32, 0.5, 65_504.]], &Device::Cpu)
            .unwrap()
            .to_dtype(DType::F16)
            .unwrap();
        let bias = Tensor::new(&[0.25f32, -1., 65_504.], &Device::Cpu).unwrap();
        let result = tensor_forward(&x, &bias).unwrap();
        assert_eq!(result.dtype(), DType::F32);
        assert_eq!(result.to_vec2::<f32>().unwrap()[0], [0., 0., 131_008.]);
    }
}
