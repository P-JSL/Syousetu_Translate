//! Fuse bias addition and ReLU without changing either arithmetic operation.
use candle_core::{CpuStorage, CustomOp2, Layout, Result, Shape, Tensor};
use rayon::prelude::*;
struct BiasRelu;
impl CustomOp2 for BiasRelu {
    fn name(&self) -> &'static str {
        "bias-relu"
    }
    fn cpu_fwd(
        &self,
        x: &CpuStorage,
        xl: &Layout,
        bias: &CpuStorage,
        bl: &Layout,
    ) -> Result<(CpuStorage, Shape)> {
        let source = super::slice(x, xl)?;
        let bias = super::slice(bias, bl)?;
        let Some(&width) = xl.shape().dims().last() else {
            candle_core::bail!("bias ReLU needs rows");
        };
        if width == 0 || bl.shape().dims() != [width] {
            candle_core::bail!("bias ReLU shape mismatch");
        }
        let mut result = vec![0f32; source.len()];
        let run = |(input, output): (&[f32], &mut [f32])| {
            for ((&value, &bias), output) in input.iter().zip(bias).zip(output) {
                *output = (value + bias).max(0f32);
            }
        };
        if source.len() >= 65_536 && rayon::current_num_threads() > 1 {
            source
                .par_chunks(width)
                .zip(result.par_chunks_mut(width))
                .for_each(run);
        } else {
            source
                .chunks(width)
                .zip(result.chunks_mut(width))
                .for_each(run);
        }
        Ok((CpuStorage::F32(result), xl.shape().clone()))
    }
}
pub(in crate::offline::model) fn forward(x: &Tensor, bias: &Tensor) -> Result<Tensor> {
    if !x.is_contiguous() || !bias.is_contiguous() {
        return x.broadcast_add(bias)?.relu();
    }
    x.apply_op2_no_bwd(bias, &BiasRelu)
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
            let actual = forward(&x, &bias).unwrap();
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
}
