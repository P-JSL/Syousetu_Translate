use super::*;
#[test]
fn pinned_int8_has_complete_integrity_metadata() {
    let spec = spec();
    assert_eq!(spec.format_version, 1);
    assert_eq!(spec.file, "model-int8.gguf");
    assert!(spec.size > 300_000_000 && spec.size < 400_000_000);
    assert_eq!(spec.sha256.len(), 64);
    assert!(spec.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(!spec.file.contains(['/', '\\']));
}

#[cfg(feature = "cuda")]
#[test]
#[ignore = "requires a real CUDA device and driver"]
fn cuda_q8_expansion_matches_reference_on_device() -> candle_core::Result<()> {
    use candle_core::{
        quantized::ggml_file::qtensor_from_ggml, quantized::*, DType, Device, Tensor,
    };

    let device = Device::new_cuda(0)?;
    // Partial final CUDA thread block and a mix of subnormal, small and large scales.
    let shape = (3, 96);
    let values = (0..shape.0 * shape.1)
        .map(|i| (i as f32 % 31. - 15.) * [1e-6, 0.03, 13.][i / 96])
        .collect();
    let dtype = GgmlDType::Q8_0;
    let cpu = QTensor::quantize(&Tensor::from_vec(values, shape, &Device::Cpu)?, dtype)?;
    let gpu = qtensor_from_ggml(dtype, &cpu.data()?, vec![shape.0, shape.1], &device)?;
    let bits = |tensor: Tensor| -> candle_core::Result<Vec<u32>> {
        Ok(tensor
            .flatten_all()?
            .to_dtype(DType::F32)?
            .to_vec1::<f32>()?
            .into_iter()
            .map(f32::to_bits)
            .collect())
    };
    assert_eq!(
        bits(cpu.dequantize_f16(&Device::Cpu)?)?,
        bits(gpu.dequantize_f16(&device)?)?
    );
    Ok(())
}
