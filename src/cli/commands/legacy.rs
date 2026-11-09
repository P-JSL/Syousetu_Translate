use super::super::args::{flag, require, value};
use crate::{
    ehnd::Ehnd,
    ehnd_regex::Executor as EhndRegexExecutor,
    translation::{
        engine::{create_engine, EngineConfig, EngineMode},
        output,
    },
};
use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[cfg(feature = "legacy")]
pub(crate) fn inspect_dat(args: &[String]) -> Result<()> {
    let directory = PathBuf::from(require(args, "--dir")?);
    let report = crate::legacy_dat::inspect(&directory)?;
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = value(args, "--out") {
        output::atomic_write(Path::new(&path), &json)?;
    } else {
        println!("{json}");
    }
    Ok(())
}
#[cfg(feature = "legacy")]
pub(crate) fn inspect_root_dll(args: &[String]) -> Result<()> {
    let path = PathBuf::from(require(args, "--dll")?);
    let bytes =
        fs::read(&path).with_context(|| format!("DLL 파일 읽기 실패: {}", path.display()))?;
    let report = crate::root_dll::inspect(&bytes)?;
    if !report.matching_known_root_exports {
        bail!("지정된 파일은 최상위 EHND/J2KEngine.dll의 검증된 25개 export와 일치하지 않습니다");
    }
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(output) = value(args, "--out") {
        output::atomic_write(Path::new(&output), &json)?;
    } else {
        println!("{json}");
    }
    Ok(())
}
#[cfg(feature = "legacy")]
pub(crate) fn inspect_ehnd(args: &[String]) -> Result<()> {
    let directory = PathBuf::from(require(args, "--dir")?);
    let rules = Ehnd::load(&directory)?;
    let json = serde_json::to_string_pretty(&rules.report())?;
    if let Some(path) = value(args, "--out") {
        output::atomic_write(Path::new(&path), &json)?;
    } else {
        println!("{json}");
    }
    Ok(())
}
/// Local differential harness: compare user-installed original EHND+DLX
/// against native Rust. No source sentences are uploaded to GitHub.
#[cfg(feature = "legacy")]
pub(crate) fn compare_engines(args: &[String]) -> Result<()> {
    let install = PathBuf::from(require(args, "--eztrans-dir")?);
    let input = PathBuf::from(require(args, "--input")?);
    let text = fs::read_to_string(&input).context("비교 문장 파일은 UTF-8이어야 합니다")?;
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|x| !x.is_empty() && !x.starts_with('#'))
        .take(2000)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        bail!("비교할 문장이 없습니다");
    }
    // The real library is only loadable on 32-bit Windows.
    let mut reference = create_engine(&EngineConfig {
        mode: EngineMode::EzTrans,
        eztrans_dir: Some(install),
        native_dictionary: None,
        strict_native: false,
    })?;
    let mut native = create_engine(&EngineConfig {
        mode: EngineMode::Native,
        eztrans_dir: None,
        native_dictionary: value(args, "--native-dictionary").map(PathBuf::from),
        strict_native: false,
    })?;
    let mut examples = Vec::new();
    let mut matched = 0usize;
    for (index, &source) in lines.iter().enumerate() {
        let reference_text = reference
            .translate(source)
            .with_context(|| format!("원본 DLL 비교 번역 실패: {}번 문장", index + 1))?;
        let native_text = native
            .translate(source)
            .with_context(|| format!("Rust 비교 번역 실패: {}번 문장", index + 1))?;
        if reference_text == native_text {
            matched += 1;
        } else if examples.len() < 40 {
            examples.push(serde_json::json!({
                "line":index+1,
                "japanese":source,
                "reference":reference_text,
                "native":native_text
            }));
        }
    }
    let report = serde_json::json!({
        "reference":"user-installed top-level EHND J2KEngine.dll + J2KEngine.dlx",
        "native":"independent Rust glossary engine",
        "compared":lines.len(),
        "exact_matches":matched,
        "exact_match_rate":matched as f64/lines.len() as f64,
        "all_semantics_verified":false,
        "mismatch_examples":examples,
    });
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(output) = value(args, "--out") {
        output::atomic_write(Path::new(&output), &serialized)?;
    } else {
        println!("{serialized}");
    }
    Ok(())
}
#[cfg(feature = "legacy")]
pub(crate) fn audit_ehnd_regex(args: &[String]) -> Result<()> {
    let filters = Ehnd::load(Path::new(&require(args, "--dir")?))?;
    let executor = EhndRegexExecutor::compile(&filters, flag(args, "--strict"))?;
    let serialized = serde_json::to_string_pretty(&executor.audit)?;
    if let Some(target) = value(args, "--out") {
        output::atomic_write(Path::new(&target), &serialized)?;
    } else {
        println!("{serialized}");
    }
    Ok(())
}
#[cfg(feature = "legacy")]
pub(crate) fn audit_ehnd_control(args: &[String]) -> Result<()> {
    let filters = Ehnd::load(Path::new(&require(args, "--dir")?))?;
    let result = crate::ehnd_control::audit(&filters);
    let serialized = serde_json::to_string_pretty(&result)?;
    if let Some(path) = value(args, "--out") {
        output::atomic_write(Path::new(&path), &serialized)?;
    } else {
        println!("{serialized}");
    }
    Ok(())
}
