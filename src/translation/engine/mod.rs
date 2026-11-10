//! Optional Windows ezTrans ABI-compatible interface.
//! Requires a licensed, user-provided 32-bit DLL and its own dictionary data.
#[cfg(not(feature = "legacy"))]
use anyhow::bail;
#[cfg(feature = "legacy")]
use anyhow::Context;
use anyhow::Result;
#[cfg(feature = "legacy")]
use std::path::Path;
use std::path::PathBuf;

pub trait Translator {
    /// Opt in only when changing batch/chapter boundaries cannot change a
    /// row's content through shared translation context.
    fn independent_rows(&self) -> bool {
        false
    }
    fn translate(&mut self, text: &str) -> Result<String>;
    fn translate_lines(&mut self, lines: &[String]) -> Result<Vec<String>> {
        lines.iter().map(|text| self.translate(text)).collect()
    }
    fn translate_lines_into(
        &mut self,
        lines: &[String],
        emit: &mut dyn FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        let translated = self.translate_lines(lines)?;
        anyhow::ensure!(
            translated.len() == lines.len(),
            "원문과 번역 줄 수가 다릅니다"
        );
        for (index, text) in translated.iter().enumerate() {
            emit(index, text)?;
        }
        Ok(())
    }
}
pub struct Passthrough;
impl Translator for Passthrough {
    fn independent_rows(&self) -> bool {
        true
    }
    fn translate(&mut self, text: &str) -> Result<String> {
        Ok(text.to_owned())
    }
}
#[cfg(feature = "legacy")]
#[allow(unsafe_code)]
mod eztrans;
#[cfg(feature = "legacy")]
pub use eztrans::EzTrans;
#[cfg(feature = "legacy")]
fn resolve_engine_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = explicit {
        return Ok(dir.to_path_buf());
    }
    if let Some(path) = std::env::var_os("SYOUSETU_EZTRANS_DIR") {
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("engine"));
            candidates.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("engine"));
        candidates.push(cwd);
    }
    candidates.into_iter().find(|dir| {
        dir.join("Dat").is_dir()
            && (dir.join("J2KEngineH.dll").is_file()
                || dir.join("J2KEngine.dll").is_file())
    }).context("ezTrans DLL 및 Dat 경로가 없습니다. scripts/install_local_engine.py로 설치하거나 --eztrans-dir을 지정하세요")
}

/// Translation backend selector. Native mode never loads the vendor DLL.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EngineMode {
    Native,
    EzTrans,
    Passthrough,
}
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub eztrans_dir: Option<PathBuf>,
    pub native_dictionary: Option<PathBuf>,
    pub strict_native: bool,
    pub mode: EngineMode,
}
#[cfg(feature = "legacy")]
pub struct Native {
    inner: crate::native::NativeTranslator,
}
#[cfg(feature = "legacy")]
impl Translator for Native {
    fn translate(&mut self, text: &str) -> Result<String> {
        self.inner.translate(text)
    }
}
pub fn create_engine(c: &EngineConfig) -> Result<Box<dyn Translator>> {
    #[cfg(not(feature = "legacy"))]
    let _ = (&c.eztrans_dir, &c.native_dictionary, c.strict_native);
    match c.mode {
        #[cfg(not(feature = "legacy"))]
        EngineMode::Native | EngineMode::EzTrans => {
            bail!("기존 엔진은 legacy 기능 빌드가 필요합니다. 기본 내부 INT8 엔진을 사용해 주세요")
        }
        EngineMode::Passthrough => Ok(Box::new(Passthrough)),
        #[cfg(feature = "legacy")]
        EngineMode::Native => Ok(Box::new(Native {
            inner: crate::native::NativeTranslator::new(
                c.native_dictionary.as_deref(),
                c.strict_native,
            )?,
        })),
        #[cfg(feature = "legacy")]
        EngineMode::EzTrans => {
            let path = resolve_engine_dir(c.eztrans_dir.as_deref())?;
            Ok(Box::new(EzTrans::open(&path)?))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "legacy")]
    fn explicit_engine_dir_is_not_overridden() {
        let path = Path::new("custom-eztrans-path");
        assert_eq!(resolve_engine_dir(Some(path)).unwrap(), path);
    }
    #[test]
    fn pass_through_never_requires_a_dll() {
        let mut engine = create_engine(&EngineConfig {
            eztrans_dir: Some(PathBuf::from("intentionally-nonexistent")),
            native_dictionary: None,
            strict_native: false,
            mode: EngineMode::Passthrough,
        })
        .unwrap();
        assert_eq!(engine.translate("原文").unwrap(), "原文");
    }
    #[test]
    #[cfg(feature = "legacy")]
    fn native_engine_does_not_require_vendor_dll_or_dat() {
        let mut engine = create_engine(&EngineConfig {
            eztrans_dir: Some(PathBuf::from("intentionally-nonexistent")),
            native_dictionary: None,
            strict_native: false,
            mode: EngineMode::Native,
        })
        .unwrap();
        assert_eq!(
            engine.translate("私は学生です。").unwrap(),
            "나는 학생입니다。"
        );
    }
}
