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
}
impl Weights {
    pub fn open(directory: &Path) -> Result<Self> {
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
        let mut names = content.tensor_infos.keys().cloned().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let tensor = content.tensor(&mut file, &name, &Device::Cpu)?;
            if tensor.rank() == 2 {
                anyhow::ensure!(
                    tensor.dtype() == GgmlDType::Q8_0,
                    "INT8이 아닌 행렬: {name}"
                );
                quantized.insert(name, Arc::new(tensor));
            } else {
                floats.insert(name, tensor.dequantize(&Device::Cpu)?);
            }
        }
        Ok(Self {
            prefix: String::new(),
            quantized: Arc::new(quantized),
            floats: Arc::new(floats),
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
    fn name(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.into()
        } else {
            format!("{}.{key}", self.prefix)
        }
    }
    pub fn matrix(&self, key: &str, shape: (usize, usize)) -> candle_core::Result<QMatMul> {
        let name = self.name(key);
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
