use anyhow::Result;
use candle_core::{
    quantized::{GgmlDType, QTensor},
    Device, Tensor,
};
use std::{
    fs::{self, File},
    path::Path,
};
#[cfg(feature = "model-tools")]
use {
    candle_core::{
        quantized::gguf_file::{self, Value},
        DType,
    },
    std::{
        collections::BTreeMap,
        io::{BufWriter, Write},
    },
};

use super::manifest::{hash, source_hash, Spec};
/// Writes only tensors used by inference. Q/K/V weights are fused before
/// quantization along full rows, preserving the original block quantization.
/// Maintainer-only tooling: the shipping app downloads prepared INT8 directly.
#[cfg(feature = "model-tools")]
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
