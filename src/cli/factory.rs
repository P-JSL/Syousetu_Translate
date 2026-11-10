#[cfg(feature = "legacy")]
use super::args::require;
use super::args::{flag, value};
#[cfg(feature = "legacy")]
use crate::ehnd::{Ehnd, Phase};
#[cfg(feature = "legacy")]
use crate::ehnd_regex::Executor as EhndRegexExecutor;
use crate::{
    config::Config,
    translation::{
        engine::{self, create_engine, EngineConfig, EngineMode},
        filter::{Dictionary, Pipeline},
        session::Session,
    },
};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
#[cfg(feature = "legacy")]
struct EhndRegexComparison {
    inner: Box<dyn engine::Translator>,
    executor: EhndRegexExecutor,
}
#[cfg(feature = "legacy")]
impl engine::Translator for EhndRegexComparison {
    fn translate(&mut self, text: &str) -> Result<String> {
        let pre = self.executor.apply(Phase::Pre, text)?;
        let middle = self.inner.translate(&pre)?;
        self.executor.apply(Phase::Post, &middle)
    }
}

#[cfg(feature = "legacy")]
struct EhndLiteralComparison {
    inner: Box<dyn engine::Translator>,
    filters: Ehnd,
}
#[cfg(feature = "legacy")]
impl engine::Translator for EhndLiteralComparison {
    fn translate(&mut self, text: &str) -> Result<String> {
        let pre = self.filters.apply_literal(Phase::Pre, text)?;
        let translated = self.inner.translate(&pre)?;
        self.filters.apply_literal(Phase::Post, &translated)
    }
}

pub(super) fn internal_mode(args: &[String], config: &Config) -> bool {
    !flag(args, "--passthrough")
        && value(args, "--eztrans-dir").is_none()
        && value(args, "--engine")
            .as_deref()
            .unwrap_or(&config.translator_name)
            == "internal"
}
pub(super) fn engine_from_args(
    args: &[String],
    config: &Config,
) -> Result<Box<dyn engine::Translator>> {
    #[cfg(not(feature = "legacy"))]
    for option in [
        "--ehnd-dir",
        "--ehnd-regex-experimental",
        "--ehnd-literal-only",
        "--ehnd-regex-strict",
    ] {
        if flag(args, option) {
            bail!(
                "{option}은 legacy 기능 빌드가 필요합니다. 내부 엔진에는 교정 기억을 사용해 주세요"
            );
        }
    }
    if internal_mode(args, config) {
        for option in [
            "--dictionary",
            "--native-dictionary",
            "--native-strict",
            "--ehnd-dir",
            "--ehnd-regex-experimental",
            "--ehnd-literal-only",
            "--ehnd-regex-strict",
        ] {
            if flag(args, option) {
                bail!("{option}은 내부 신경망 엔진에서 지원하지 않습니다. 교정 기억의 문장·용어 기능을 사용해 주세요");
            }
        }
        #[cfg(feature = "offline-model")]
        {
            let memory = if config.learning_enabled {
                crate::learning::Memory::load(&config.memory_file())?
            } else {
                crate::learning::Memory::default()
            };
            return Ok(Box::new(Internal {
                engine: crate::offline::OfflineTranslator::builder(&config.model_directory())
                    .memory(memory)
                    .workers(config.workers())
                    .cpu_budget(crate::offline::CpuBudget::from_requested(config.workers()))
                    .parallel_policy(config.parallel_mode.parse()?)
                    .build()?,
                progress: flag(args, "--progress-json"),
            }));
        }
        #[cfg(not(feature = "offline-model"))]
        bail!("이 실행 파일에는 내부 모델 기능이 없습니다. 64비트 기본 빌드를 사용해 주세요");
    }
    let dll_path = value(args, "--eztrans-dir")
        .or_else(|| config.eztrans_path.clone())
        .map(PathBuf::from);
    let engine = match value(args, "--engine").as_deref() {
        Some("native") => EngineMode::Native,
        Some("eztrans") => EngineMode::EzTrans,
        Some("passthrough") => EngineMode::Passthrough,
        Some(name) => bail!("잘못된 --engine: {name}; internal|native|eztrans|passthrough"),
        None if flag(args, "--passthrough") => EngineMode::Passthrough,
        None if value(args, "--eztrans-dir").is_some() => EngineMode::EzTrans,
        None => match config.translator_name.as_str() {
            "eztrans" => EngineMode::EzTrans,
            "passthrough" => EngineMode::Passthrough,
            _ => EngineMode::Native,
        },
    };
    if flag(args, "--passthrough") && engine != EngineMode::Passthrough {
        bail!("--passthrough와 --engine 값이 충돌합니다");
    }
    if engine != EngineMode::Native
        && (flag(args, "--native-strict") || value(args, "--native-dictionary").is_some())
    {
        bail!("--native-strict와 --native-dictionary는 --engine native에서만 사용할 수 있습니다");
    }
    if flag(args, "--ehnd-regex-strict") && !flag(args, "--ehnd-regex-experimental") {
        bail!("--ehnd-regex-strict는 --ehnd-regex-experimental과 함께 사용해야 합니다");
    }
    let translator = create_engine(&EngineConfig {
        mode: engine,
        eztrans_dir: dll_path,
        native_dictionary: value(args, "--native-dictionary").map(PathBuf::from),
        strict_native: flag(args, "--native-strict"),
    })?;
    #[cfg(feature = "legacy")]
    if flag(args, "--ehnd-regex-experimental") {
        if flag(args, "--ehnd-literal-only") {
            bail!("--ehnd-regex-experimental과 --ehnd-literal-only는 동시 지정 불가");
        }
        if engine != EngineMode::Native {
            bail!("EHND 실험용 필터는 --engine native에서만 사용 가능합니다");
        }
        let filters = Ehnd::load(Path::new(&require(args, "--ehnd-dir")?))?;
        let executor = EhndRegexExecutor::compile(&filters, flag(args, "--ehnd-regex-strict"))?;
        eprintln!("WARNING: 원본 EHND와 다르게 동작할 수 있는 실험 모드입니다. literal {}, regex {}, 컴파일 불가 {}, 미지원 directive {}, 미분류 {}",
            executor.audit.literals,executor.audit.regexes,
            executor.audit.rejected,executor.audit.directives,executor.audit.unknown);
        return Ok(Box::new(EhndRegexComparison {
            inner: translator,
            executor,
        }));
    }
    #[cfg(feature = "legacy")]
    if flag(args, "--ehnd-literal-only") {
        if engine != EngineMode::Native {
            bail!("EHND 리터럴 비교는 --engine native에서만 허용됩니다");
        }
        let dir = require(args, "--ehnd-dir")?;
        let filters = Ehnd::load(Path::new(&dir))?;
        eprintln!("WARNING: EHND 리터럴 {}개만 적용하며 regex·지시문 {}개는 실행하지 않습니다. 원본 번역 재현이 아닙니다",
            filters.counts.literals,filters.counts.regexes+filters.counts.directives+filters.counts.unknown);
        return Ok(Box::new(EhndLiteralComparison {
            inner: translator,
            filters,
        }));
    }
    if flag(args, "--ehnd-dir") {
        bail!("--ehnd-dir에는 명시적인 --ehnd-literal-only가 필요합니다");
    }
    Ok(translator)
}
pub(super) fn pipeline_from_args(args: &[String], config: &Config) -> Result<Pipeline> {
    let default = Path::new("config/dictionary.txt");
    let passthrough = flag(args, "--passthrough")
        || value(args, "--engine").as_deref() == Some("passthrough")
        || (value(args, "--engine").is_none()
            && value(args, "--eztrans-dir").is_none()
            && config.translator_name == "passthrough");
    let dict = match value(args, "--dictionary") {
        Some(file) => Dictionary::from_file(Path::new(&file))?,
        None if default.is_file() && !passthrough => Dictionary::from_file(default)?,
        None => Dictionary::empty(),
    };
    let legacy = value(args, "--engine").as_deref() == Some("eztrans")
        || (value(args, "--engine").is_none()
            && !flag(args, "--passthrough")
            && (value(args, "--eztrans-dir").is_some() || config.translator_name == "eztrans"));
    Ok(if legacy {
        Pipeline::legacy(dict)
    } else {
        Pipeline::new(dict)
    })
}
#[cfg(feature = "offline-model")]
struct Internal {
    engine: crate::offline::OfflineTranslator,
    progress: bool,
}
#[cfg(feature = "offline-model")]
impl engine::Translator for Internal {
    fn translate(&mut self, text: &str) -> Result<String> {
        self.engine.translate(text)
    }
    fn translate_lines(&mut self, lines: &[String]) -> Result<Vec<String>> {
        let progress = self.progress;
        self.engine.translate_lines_with_progress(lines,|done,total| {
            if progress {eprintln!("{}",serde_json::json!({"event":"progress","done":done,"total":total,"unit":"lines"}));}
        })
    }
    fn translate_lines_into(
        &mut self,
        lines: &[String],
        emit: &mut dyn FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        let progress = self.progress;
        self.engine.translate_lines_to(lines, |done,total| {
            if progress {eprintln!("{}", serde_json::json!({"event":"progress","done":done,"total":total,"unit":"lines"}));}
        }, emit)
    }
}
pub(super) fn local_session(args: &[String], config: &Config) -> Result<Session> {
    if internal_mode(args, config) {
        Ok(Session::direct(engine_from_args(args, config)?))
    } else {
        Ok(Session::local(
            pipeline_from_args(args, config)?,
            engine_from_args(args, config)?,
        ))
    }
}
pub(super) fn translation_session(args: &[String], config: &Config) -> Result<Session> {
    if !cfg!(feature = "legacy") || internal_mode(args, config) || config.workers() == 1 {
        local_session(args, config)
    } else {
        Session::parallel(args, config.workers())
    }
}
