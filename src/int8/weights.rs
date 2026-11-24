use anyhow::Result;
use candle_core::{
    quantized::{gguf_file::Content, GgmlDType, QMatMul, QTensor},
    Device, Tensor,
};
use std::{collections::HashMap, fs::File, io::BufReader, path::Path, sync::Arc};

use super::manifest::{source_hash, spec, verify};
#[derive(Clone)]
pub struct Weights {
    prefix: String,
    quantized: Arc<HashMap<String, Arc<QTensor>>>,
    floats: Arc<HashMap<String, Tensor>>,
    gpu_matrices: Arc<HashMap<String, Tensor>>,
    device: Device,
}
impl Weights {
    pub fn open(directory: &Path) -> Result<Self> {
        Self::open_on_device(directory, &Device::Cpu)
    }
    /// The pinned on-disk Q8 model is expanded once for CUDA Tensor Core GEMM.
    /// CPU keeps its original Q8 arithmetic; GPU normalization/residuals are F32.
    pub fn open_on_device(directory: &Path, device: &Device) -> Result<Self> {
        verify(directory)?;
        let mut file = BufReader::new(File::open(directory.join(spec().file))?);
        let content = Content::read(&mut file)?;
        anyhow::ensure!(
            content
                .metadata
                .get("syousetu.source_sha256")
                .and_then(|v| v.to_string().ok())
                .is_some_and(|s| s == &source_hash()),
            "INT8 모델 출처 메타데이터 불일치"
        );
        let mut floats = HashMap::new();
        let mut quantized = HashMap::new();
        let mut gpu_matrices = HashMap::new();
        let mut names = content.tensor_infos.keys().cloned().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let info = &content.tensor_infos[&name];
            if info.shape.rank() == 2 {
                anyhow::ensure!(
                    info.ggml_dtype == GgmlDType::Q8_0,
                    "INT8이 아닌 행렬: {name}"
                );
                if device.is_cpu() {
                    let tensor = content.tensor(&mut file, &name, device)?;
                    quantized.insert(name, Arc::new(tensor));
                } else {
                    // Transfer the verified Q8 blocks and expand them once on CUDA.
                    let source = if device.is_cuda() {
                        device
                    } else {
                        &Device::Cpu
                    };
                    let weights = content
                        .tensor(&mut file, &name, source)?
                        .dequantize_f16(source)?
                        .to_device(device)?;
                    gpu_matrices.insert(name, weights);
                }
            } else {
                let tensor = content.tensor(&mut file, &name, &Device::Cpu)?;
                floats.insert(name, tensor.dequantize(&Device::Cpu)?.to_device(device)?);
            }
        }
        Ok(Self {
            prefix: String::new(),
            quantized: Arc::new(quantized),
            floats: Arc::new(floats),
            gpu_matrices: Arc::new(gpu_matrices),
            device: device.clone(),
        })
    }
    pub fn pp(&self, path: impl std::fmt::Display) -> Self {
        let prefix = if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}.{path}", self.prefix)
        };
        Self {
            prefix,
            ..self.clone()
        }
    }
    pub fn device(&self) -> &Device {
        &self.device
    }
    fn name(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.into()
        } else {
            format!("{}.{key}", self.prefix)
        }
    }
    pub fn matrix(&self, key: &str, shape: (usize, usize)) -> candle_core::Result<QMatMul> {
        let name = self.name(key);
        if let Some(tensor) = self.gpu_matrices.get(&name) {
            if tensor.dims() != [shape.0, shape.1] {
                candle_core::bail!("GPU 행렬 크기 불일치: {name}");
            }
            return Ok(QMatMul::TensorF16(tensor.clone()));
        }
        let tensor = self
            .quantized
            .get(&name)
            .ok_or_else(|| candle_core::Error::Msg(format!("INT8 행렬 없음: {name}")))?;
        if tensor.shape().dims() != [shape.0, shape.1] {
            candle_core::bail!("INT8 행렬 크기 불일치: {name}");
        }
        QMatMul::from_arc(Arc::clone(tensor))
    }
    pub fn get(&self, size: usize, key: &str) -> candle_core::Result<Tensor> {
        let name = self.name(key);
        let tensor = self
            .floats
            .get(&name)
            .ok_or_else(|| candle_core::Error::Msg(format!("모델 텐서 없음: {name}")))?;
        if tensor.dims() != [size] {
            candle_core::bail!("모델 텐서 크기 불일치: {name}");
        }
        Ok(tensor.clone())
    }
}
