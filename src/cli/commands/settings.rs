use super::super::{
    args::{flag, require, value},
    options,
    progress::emit_progress,
};
use crate::{config::Config, translation::output};
use anyhow::{bail, Context, Result};
use std::{fs, path::Path};
pub(crate) fn write_config(args: &[String]) -> Result<()> {
    let config: Config = if let Some(path) = value(args, "--json") {
        serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("설정 JSON 읽기 실패: {path}"))?,
        )?
    } else {
        options::settings(args)?
    };
    let text = config.to_properties()?;
    output::atomic_write(Path::new(&require(args, "--out")?), &text)?;
    Ok(())
}
pub(crate) fn model_command(args: &[String]) -> Result<()> {
    let cfg = options::settings(args)?;
    let dir = cfg.model_directory();
    if flag(args, "--prepare") && flag(args, "--verify") {
        bail!("--prepare와 --verify는 함께 사용할 수 없습니다");
    }
    if flag(args, "--prepare") {
        let mut last = std::time::Instant::now() - std::time::Duration::from_secs(2);
        crate::models::prepare_with_ca(
            &dir,
            cfg.ca_bundle.as_deref().map(Path::new),
            |done, total, label| {
                if done == total || last.elapsed() >= std::time::Duration::from_secs(1) {
                    if flag(args, "--progress-json") {
                        emit_progress(args, done as usize, total as usize, "bytes");
                    }
                    eprintln!("모델 준비 {done}/{total}: {label}");
                    last = std::time::Instant::now();
                }
            },
        )?;
    } else if flag(args, "--verify") {
        crate::models::verify(&dir)?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&crate::models::status(&dir))?
    );
    Ok(())
}
pub(crate) fn memory_command(args: &[String]) -> Result<()> {
    use crate::learning::Memory;
    let path = options::settings(args)?.memory_file();
    let learn = flag(args, "--learn");
    let forget = flag(args, "--forget");
    let import = value(args, "--import-ehnd");
    let dictionary = value(args, "--import-dictionary");
    if usize::from(learn)
        + usize::from(forget)
        + usize::from(import.is_some())
        + usize::from(dictionary.is_some())
        > 1
    {
        bail!("교정 편집과 사전 가져오기는 한 번에 하나만 지정해 주세요");
    }
    let memory = if let Some(directory) = import {
        Memory::edit(&path, |m| {
            let report = crate::learning::import::import_ehnd(Path::new(&directory), m)?;
            eprintln!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        })?
    } else if let Some(file) = dictionary {
        Memory::edit(&path, |m| {
            let report = crate::learning::import::import_dictionary(Path::new(&file), m)?;
            eprintln!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        })?
    } else if learn || forget {
        let source = require(args, "--source")?;
        Memory::edit(&path, |m| {
            if forget {
                if let Some(previous) = value(args, "--previous") {
                    m.terms
                        .retain(|t| t.source != source.trim() || t.previous != previous.trim());
                } else {
                    m.sentences.remove(source.trim());
                }
                Ok(())
            } else if let Some(previous) = value(args, "--previous") {
                m.learn_term(&source, &previous, &require(args, "--corrected")?)
            } else {
                m.learn(&source, &require(args, "--corrected")?)
            }
        })?
    } else {
        Memory::load(&path)?
    };
    println!("{}", serde_json::to_string_pretty(&memory)?);
    Ok(())
}
