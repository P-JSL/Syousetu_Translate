//! Optional Windows ezTrans ABI-compatible interface.
//! Requires a licensed, user-provided 32-bit DLL and its own dictionary data.
use anyhow::{bail, Result};
#[cfg(feature="legacy")]
use anyhow::Context;
use std::path::PathBuf;
#[cfg(feature="legacy")]
use std::path::Path;
#[cfg(all(windows,feature="legacy"))]
use std::ffi::CString;

pub trait Translator {
    fn translate(&mut self, text: &str) -> Result<String>;
    fn translate_lines(&mut self, lines: &[String]) -> Result<Vec<String>> {lines.iter().map(|text|self.translate(text)).collect()}
}
pub struct Passthrough;
impl Translator for Passthrough {
    fn translate(&mut self, text: &str) -> Result<String> { Ok(text.to_owned()) }
}
#[cfg(all(windows,feature="legacy"))]
pub struct EzTrans {
    _lib: libloading::Library,
    translate: unsafe extern "system" fn(i32, *const u16) -> *mut u16,
    free: unsafe extern "C" fn(*mut u16) -> i32,
}
#[cfg(all(windows,feature="legacy"))]
impl EzTrans {
    pub fn open(dir: &Path) -> Result<Self> {
        if usize::BITS!=32 {bail!("기존 ezTrans DLL은 32비트 전용입니다. 내부 번역기는 --engine internal로 실행해 주세요");}
        use libloading::Library;
        let names = ["J2KEngineH.dll", "J2KEngine.dll"];
        let file = names.iter().map(|x| dir.join(x)).find(|p| p.is_file())
            .context("J2KEngineH.dll 또는 J2KEngine.dll을 찾지 못했습니다")?;
        if !dir.join("Dat").is_dir() {bail!("ezTrans Dat 디렉터리가 없습니다");}
        // This legacy mode is intentionally scoped to the top-level EHND wrapper,
        // not the different 466-export translator/J2KEngine.dll binary.
        let library_bytes=std::fs::read(&file).with_context(||format!("DLL 검사 실패: {}",file.display()))?;
        let report=syousetu_translate::root_dll::inspect(&library_bytes)
            .with_context(||format!("지원되지 않는 DLL 형식: {}",file.display()))?;
        if !report.matching_known_root_exports {
            bail!("최상위 EHND/J2KEngine.dll의 검증된 25개 export와 일치하지 않습니다: {}",file.display());
        }
        if report.references_j2kengine_dlx && !dir.join("J2KEngine.dlx").is_file() {
            bail!("최상위 DLL이 사용하는 J2KEngine.dlx가 설치 디렉터리에 없습니다");
        }
        unsafe {
            let lib = Library::new(&file).with_context(||format!("DLL 로드 실패: {} (32-bit 프로세스인지 확인)",file.display()))?;
            let init: unsafe extern "system" fn(*const i8, *const i8) -> i32 =
                *lib.get(b"J2K_InitializeEx\0")?;
            let translate = *lib.get::<unsafe extern "system" fn(i32,*const u16)->*mut u16>(b"J2K_TranslateMMNTW\0")?;
            let free = *lib.get::<unsafe extern "C" fn(*mut u16)->i32>(b"J2K_FreeMem\0")?;
            let id = CString::new("CSUSER123455")?;
            // JNA uses the Windows default ANSI codepage; CP949 is common on Korean Windows.
            let data_dir=dir.join("Dat");
            let path_text=data_dir.to_string_lossy().into_owned();
            let (encoded,_,bad)=encoding_rs::EUC_KR.encode(&path_text);
            if bad {bail!("ezTrans 데이터 경로를 CP949로 변환할 수 없습니다");}
            let directory=CString::new(encoded.into_owned()).context("DLL 데이터 경로에 NUL 문자가 있습니다")?;
            let _status=init(id.as_ptr(), directory.as_ptr());
            Ok(Self{_lib:lib,translate,free})
        }
    }
}
#[cfg(all(windows,feature="legacy"))]
impl Translator for EzTrans {
    fn translate(&mut self, text: &str) -> Result<String> {
        let wide:Vec<u16>=text.encode_utf16().chain(std::iter::once(0)).collect();
        let ptr=unsafe{(self.translate)(0,wide.as_ptr())};
        if ptr.is_null() {bail!("J2K_TranslateMMNTW returned null");}
        let result = unsafe {
            let mut n=0usize;
            while n<8_000_000 && *ptr.add(n)!=0 { n+=1; }
            let owned=if n==8_000_000 { Err(anyhow::anyhow!("DLL 결과 길이가 제한을 초과함")) }
                else { String::from_utf16(std::slice::from_raw_parts(ptr,n)).context("DLL UTF-16 결과 오류") };
            (self.free)(ptr);
            owned
        }?;
        Ok(result.trim().to_owned())
    }
}
#[cfg(all(not(windows),feature="legacy"))]
pub struct EzTrans;
#[cfg(all(not(windows),feature="legacy"))]
impl EzTrans {pub fn open(_: &Path)->Result<Self>{bail!("ezTrans DLL은 Windows에서만 실행됩니다")}}
#[cfg(all(not(windows),feature="legacy"))]
impl Translator for EzTrans {fn translate(&mut self, _: &str)->Result<String>{bail!("Windows ezTrans DLL 필요")}}
/// Resolve a user-provided installation or a local, Git-ignored engine bundle.
#[cfg(feature="legacy")]
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
pub enum EngineMode { Native, EzTrans, Passthrough }
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub eztrans_dir: Option<PathBuf>,
    pub native_dictionary: Option<PathBuf>,
    pub strict_native: bool,
    pub mode: EngineMode,
}
#[cfg(feature="legacy")]
pub struct Native {
    inner: syousetu_translate::native::NativeTranslator,
}
#[cfg(feature="legacy")]
impl Translator for Native {
    fn translate(&mut self, text: &str) -> Result<String> {
        self.inner.translate(text)
    }
}
pub fn create_engine(c: &EngineConfig) -> Result<Box<dyn Translator>> {
    #[cfg(not(feature="legacy"))]
    let _ = (&c.eztrans_dir,&c.native_dictionary,c.strict_native);
    match c.mode {
        #[cfg(not(feature="legacy"))]
        EngineMode::Native | EngineMode::EzTrans => bail!("기존 엔진은 legacy 기능 빌드가 필요합니다. 기본 내부 INT8 엔진을 사용해 주세요"),
        EngineMode::Passthrough => Ok(Box::new(Passthrough)),
        #[cfg(feature="legacy")]
        EngineMode::Native => Ok(Box::new(Native {
            inner: syousetu_translate::native::NativeTranslator::new(
                c.native_dictionary.as_deref(), c.strict_native)?,
        })),
        #[cfg(feature="legacy")]
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
    #[cfg(feature="legacy")]
    fn explicit_engine_dir_is_not_overridden() {
        let path = Path::new("custom-eztrans-path");
        assert_eq!(resolve_engine_dir(Some(path)).unwrap(), path);
    }
    #[test]
    fn pass_through_never_requires_a_dll() {
        let mut engine = create_engine(&EngineConfig {
            eztrans_dir: Some(PathBuf::from("intentionally-nonexistent")),
            native_dictionary: None, strict_native: false,
            mode: EngineMode::Passthrough,
        }).unwrap();
        assert_eq!(engine.translate("原文").unwrap(), "原文");
    }
    #[test]
    #[cfg(feature="legacy")]
    fn native_engine_does_not_require_vendor_dll_or_dat() {
        let mut engine = create_engine(&EngineConfig {
            eztrans_dir: Some(PathBuf::from("intentionally-nonexistent")),
            native_dictionary: None, strict_native: false,
            mode: EngineMode::Native,
        }).unwrap();
        assert_eq!(engine.translate("私は学生です。").unwrap(), "나는 학생입니다。");
    }
}
