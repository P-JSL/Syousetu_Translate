//! Apply both ordered F32 additions in the CPU projection's own allocation.
use candle_core::{CpuStorage, DType, InplaceOp3, Layout, Result, Tensor};
use rayon::prelude::*;
struct BiasResidual;
impl InplaceOp3 for BiasResidual {
    fn name(&self) -> &'static str {
        "bias-residual"
    }
    fn cpu_fwd(
        &self,
        x: &mut CpuStorage,
        xl: &Layout,
        bias: &CpuStorage,
        bl: &Layout,
        residual: &CpuStorage,
        rl: &Layout,
    ) -> Result<()> {
        let output = super::slice_mut(x, xl)?;
        let bias = super::slice(bias, bl)?;
        let residual = super::slice(residual, rl)?;
        let Some(&width) = xl.shape().dims().last() else {
            candle_core::bail!("bias residual needs rows");
        };
        if width == 0 || bl.shape().dims() != [width] || xl.shape() != rl.shape() {
            candle_core::bail!("bias residual shape mismatch");
        }
        let run = |(row, residual): (&mut [f32], &[f32])| {
            for ((value, &bias), &residual) in row.iter_mut().zip(bias).zip(residual) {
                *value = residual + (*value + bias);
            }
        };
        if output.len() >= 65_536 && rayon::current_num_threads() > 1 {
            output
                .par_chunks_mut(width)
                .zip(residual.par_chunks(width))
                .for_each(run);
        } else {
            output
                .chunks_mut(width)
                .zip(residual.chunks(width))
                .for_each(run);
        }
        Ok(())
    }
}
/// The fresh projection may be overwritten. Bias and residual are read-only.
pub(in crate::offline::model) fn forward(
    projected: Tensor,
    bias: &Tensor,
    residual: &Tensor,
) -> Result<Tensor> {
    if !projected.device().is_cpu() {
        return tensor_forward(&projected, bias, residual);
    }
    if !projected.is_contiguous() || !bias.is_contiguous() || !residual.is_contiguous() {
        return residual + projected.broadcast_add(bias)?;
    }
    projected.inplace_op3(bias, residual, &BiasResidual)?;
    Ok(projected)
}

fn tensor_forward(x: &Tensor, bias: &Tensor, residual: &Tensor) -> Result<Tensor> {
    let biased = x
        .to_dtype(DType::F32)?
        .broadcast_add(&bias.to_dtype(DType::F32)?)?;
    residual.to_dtype(DType::F32)? + biased
}
#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;
    fn compare(x: Tensor, bias: &Tensor, residual: &Tensor) {
        let expected = (residual + x.broadcast_add(bias).unwrap()).unwrap();
        let actual = forward(x, bias, residual).unwrap();
        assert_eq!(actual.dims(), expected.dims());
        for (actual, expected) in actual
            .flatten_all()
            .unwrap()
            .to_vec1::<f32>()
            .unwrap()
            .into_iter()
            .zip(expected.flatten_all().unwrap().to_vec1::<f32>().unwrap())
        {
            if expected.is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), expected.to_bits());
            }
        }
    }
    #[test]
    fn fusion_matches_independent_additions_with_offsets_special_values_and_strides() {
        for (rows, width) in [(1, 16), (17, 1024), (129, 1024)] {
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
            let residual = (&x * -0.25).unwrap();
            let bias = Tensor::from_vec(
                (0..width)
                    .map(|i| (i % 7) as f32 * -0.25)
                    .collect::<Vec<_>>(),
                width,
                &Device::Cpu,
            )
            .unwrap();
            let transposed = x.transpose(0, 1).unwrap();
            let transposed_residual = residual.transpose(0, 1).unwrap();
            let transposed_bias = Tensor::zeros(rows, DType::F32, &Device::Cpu).unwrap();
            compare(transposed, &transposed_bias, &transposed_residual);
            compare(x, &bias, &residual);
        }
    }

    #[test]
    fn portable_residual_retains_f32_addition_order() {
        let x = Tensor::new(&[[-1e10f32, 65_504.]], &Device::Cpu).unwrap();
        let bias = Tensor::new(&[1e10f32, 65_504.], &Device::Cpu).unwrap();
        let residual = Tensor::new(&[[1f32, 2.]], &Device::Cpu)
            .unwrap()
            .to_dtype(DType::F16)
            .unwrap();
        let result = tensor_forward(&x, &bias, &residual).unwrap();
        assert_eq!(result.dtype(), DType::F32);
        assert_eq!(result.to_vec2::<f32>().unwrap()[0], [1., 131_010.]);
    }
}
