//! Fuse `residual + (projection + bias)`, retaining both F32 additions and
//! their order while avoiding the intermediate biased projection buffer.
use candle_core::{CpuStorage, CustomOp3, Layout, Result, Shape, Tensor};
use rayon::prelude::*;
struct BiasResidual;
impl CustomOp3 for BiasResidual {
    fn name(&self) -> &'static str {
        "bias-residual"
    }
    fn cpu_fwd(
        &self,
        x: &CpuStorage,
        xl: &Layout,
        bias: &CpuStorage,
        bl: &Layout,
        residual: &CpuStorage,
        rl: &Layout,
    ) -> Result<(CpuStorage, Shape)> {
        let source = super::slice(x, xl)?;
        let bias = super::slice(bias, bl)?;
        let residual = super::slice(residual, rl)?;
        let Some(&width) = xl.shape().dims().last() else {
            candle_core::bail!("bias residual needs rows");
        };
        if width == 0 || bl.shape().dims() != [width] || xl.shape() != rl.shape() {
            candle_core::bail!("bias residual shape mismatch");
        }
        let mut result = vec![0f32; source.len()];
        let run = |((input, residual), output): ((&[f32], &[f32]), &mut [f32])| {
            for (((&value, &bias), &residual), output) in
                input.iter().zip(bias).zip(residual).zip(output)
            {
                *output = residual + (value + bias);
            }
        };
        if source.len() >= 65_536 && rayon::current_num_threads() > 1 {
            source
                .par_chunks(width)
                .zip(residual.par_chunks(width))
                .zip(result.par_chunks_mut(width))
                .for_each(run);
        } else {
            source
                .chunks(width)
                .zip(residual.chunks(width))
                .zip(result.chunks_mut(width))
                .for_each(run);
        }
        Ok((CpuStorage::F32(result), xl.shape().clone()))
    }
}
pub(in crate::offline::model) fn forward(
    x: &Tensor,
    bias: &Tensor,
    residual: &Tensor,
) -> Result<Tensor> {
    if !x.is_contiguous() || !bias.is_contiguous() || !residual.is_contiguous() {
        return residual + x.broadcast_add(bias)?;
    }
    x.apply_op3_no_bwd(bias, residual, &BiasResidual)
}
#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;
    fn compare(x: &Tensor, bias: &Tensor, residual: &Tensor) {
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
            compare(&x, &bias, &residual);
            let transposed = x.transpose(0, 1).unwrap();
            let residual = residual.transpose(0, 1).unwrap();
            let bias = Tensor::zeros(rows, candle_core::DType::F32, &Device::Cpu).unwrap();
            compare(&transposed, &bias, &residual);
        }
    }
}
