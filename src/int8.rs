//! Persistent, deterministic SMaLL-100 INT8 weights. Q8_0 stores signed int8
//! values with block scales; normalization/biases remain floating point.
use anyhow::Result;
use candle_core::{
    quantized::{
        gguf_file::{self, Content, Value},
        GgmlDType, QMatMul, QTensor,
    },
    DType, Device, Tensor,
};
use candle_nn::VarBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    sync::Arc,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct Spec {
    pub format_version: u32,
    pub file: String,
    pub size: u64,
    pub sha256: String,
}
pub fn spec() -> Spec {
    serde_json::from_str(include_str!("../models/int8.json")).expect("embedded INT8 spec")
}
fn source_hash() -> String {
    crate::models::manifest()
        .into_iter()
        .find(|a| a.file == "model.safetensors")
        .expect("source model")
        .sha256
}
pub fn hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 128 * 1024];
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn exists(directory: &Path) -> bool {
    let spec = spec();
    spec.size > 0
        && fs::metadata(directory.join(spec.file))
            .is_ok_and(|m| m.is_file() && m.len() == spec.size)
}
pub fn verify(directory: &Path) -> Result<()> {
    let spec = spec();
    let path = directory.join(&spec.file);
    anyhow::ensure!(spec.format_version == 1, "지원하지 않는 INT8 모델 형식");
    anyhow::ensure!(
        exists(directory),
        "SMaLL-100 INT8 파일 준비가 필요합니다: {}",
        path.display()
    );
    anyhow::ensure!(
        hash(&path)? == spec.sha256,
        "INT8 모델 체크섬 불일치: {}",
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
pub fn ensure(directory: &Path) -> Result<()> {
    if verify(directory).is_ok() {
        return Ok(());
    }
    let destination = directory.join(spec().file);
    eprintln!("SMaLL-100 INT8 모델을 한 번 변환해 저장합니다...");
    convert(&directory.join("model.safetensors"), &destination)?;
    verify(directory)
}
/// Writes only tensors used by inference. Q/K/V weights are fused before
/// quantization along full rows, preserving the original block quantization.
pub fn convert(source: &Path, destination: &Path) -> Result<Spec> {
    anyhow::ensure!(hash(source)? == source_hash(), "원본 모델 체크섬 불일치");
    let data = candle_core::safetensors::BufferedSafetensors::new(fs::read(source)?)?;
    let mut tensors = BTreeMap::<String, QTensor>::new();
    let mut names = data
        .tensors()
        .into_iter()
        .map(|(n, _)| n)
        .collect::<Vec<_>>();
    names.sort();
    for name in names {
        if name.contains(".self_attn.k_proj.")
            || name.contains(".self_attn.v_proj.")
            || name.contains(".encoder_attn.v_proj.")
        {
            continue;
        }
        let (out, tensor) = if name.contains(".self_attn.q_proj.") {
            let q = data.load(&name, &Device::Cpu)?;
            let k = data.load(&name.replace("q_proj.", "k_proj."), &Device::Cpu)?;
            let v = data.load(&name.replace("q_proj.", "v_proj."), &Device::Cpu)?;
            (
                name.replace("q_proj.", "qkv_proj."),
                Tensor::cat(&[q, k, v], 0)?,
            )
        } else if name.contains(".encoder_attn.k_proj.") {
            let k = data.load(&name, &Device::Cpu)?;
            let v = data.load(&name.replace("k_proj.", "v_proj."), &Device::Cpu)?;
            (
                name.replace("k_proj.", "kv_proj."),
                Tensor::cat(&[k, v], 0)?,
            )
        } else {
            (name.clone(), data.load(&name, &Device::Cpu)?)
        };
        let dtype = if tensor.rank() == 2 {
            GgmlDType::Q8_0
        } else {
            GgmlDType::F32
        };
        tensors.insert(
            out,
            QTensor::quantize(&tensor.to_dtype(DType::F32)?, dtype)?,
        );
    }
    let metadata = [
        ("general.name", Value::String("SMaLL-100 INT8".into())),
        ("general.quantization_version", Value::U32(1)),
        ("syousetu.source_sha256", Value::String(source_hash())),
    ];
    let metadata = metadata.iter().map(|(k, v)| (*k, v)).collect::<Vec<_>>();
    let tensors = tensors
        .iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect::<Vec<_>>();
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_extension("gguf.part");
    let result = (|| -> Result<Spec> {
        let mut writer = BufWriter::new(File::create(&temporary)?);
        gguf_file::write(&mut writer, &metadata, &tensors)?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        let report = Spec {
            format_version: 1,
            file: "model-int8.gguf".into(),
            size: fs::metadata(&temporary)?.len(),
            sha256: hash(&temporary)?,
        };
        if destination.exists() {
            fs::remove_file(destination)?;
        }
        fs::rename(&temporary, destination)?;
        Ok(report)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[derive(Clone)]
pub struct Weights {
    prefix: String,
    quantized: Arc<HashMap<String, Arc<QTensor>>>,
    floats: VarBuilder<'static>,
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
            floats: VarBuilder::from_tensors(floats, DType::F32, &Device::Cpu),
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
    pub fn floats(&self) -> VarBuilder<'static> {
        if self.prefix.is_empty() {
            self.floats.clone()
        } else {
            self.floats.pp(&self.prefix)
        }
    }
}
