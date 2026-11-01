//! Optional Windows ezTrans ABI-compatible interface.
//! Requires a licensed, user-provided 32-bit DLL and its own dictionary data.
use anyhow::{bail, Context, Result};
use std::{path::{Path, PathBuf}, ffi::CString};

pub trait Translator {
    fn translate(&mut self, text: &str) -> Result<String>;
}
pub struct Passthrough;
impl Translator for Passthrough {
    fn translate(&mut self, text: &str) -> Result<String> { Ok(text.to_owned()) }
}
#[cfg(windows)]
pub struct EzTrans {
    _lib: libloading::Library,
    translate: unsafe extern "C" fn(i32, *const u16) -> *mut u16,
    free: unsafe extern "C" fn(*mut u16) -> i32,
}
#[cfg(windows)]
impl EzTrans {
    pub fn open(dir: &Path) -> Result<Self> {
        use libloading::Library;
        let names = ["J2KEngineH.dll", "J2KEngine.dll"];
        let file = names.iter().map(|x| dir.join(x)).find(|p| p.is_file())
            .context("J2KEngineH.dll 또는 J2KEngine.dll을 찾지 못했습니다")?;
        if !dir.join("Dat").is_dir() {bail!("ezTrans Dat 디렉터리가 없습니다");}
        unsafe {
            let lib = Library::new(&file).with_context(||format!("DLL 로드 실패: {} (32-bit 프로세스인지 확인)",file.display()))?;
            let init: unsafe extern "C" fn(*const i8, *const i8) -> i32 =
                *lib.get(b"J2K_InitializeEx\0")?;
            let translate = *lib.get::<unsafe extern "C" fn(i32,*const u16)->*mut u16>(b"J2K_TranslateMMNTW\0")?;
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
#[cfg(windows)]
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
#[cfg(not(windows))]
pub struct EzTrans;
#[cfg(not(windows))]
impl EzTrans {pub fn open(_: &Path)->Result<Self>{bail!("ezTrans DLL은 Windows에서만 실행됩니다")}}
#[cfg(not(windows))]
impl Translator for EzTrans {fn translate(&mut self, _: &str)->Result<String>{bail!("Windows ezTrans DLL 필요")}}
/// Resolve a user-provided installation or a local, Git-ignored engine bundle.
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

#[derive(Debug, Clone)]
pub struct EngineConfig {pub eztrans_dir: Option<PathBuf>, pub passthrough: bool}
pub fn create_engine(c:&EngineConfig)->Result<Box<dyn Translator>>{
    if c.passthrough {return Ok(Box::new(Passthrough));}
    let path=resolve_engine_dir(c.eztrans_dir.as_deref())?;
    Ok(Box::new(EzTrans::open(&path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_engine_dir_is_not_overridden() {
        let path = Path::new("custom-eztrans-path");
        assert_eq!(resolve_engine_dir(Some(path)).unwrap(), path);
    }

    #[test]
    fn pass_through_never_requires_a_dll() {
        let mut engine = create_engine(&EngineConfig {
            eztrans_dir: Some(PathBuf::from("intentionally-nonexistent")),
            passthrough: true,
        }).unwrap();
        assert_eq!(engine.translate("原文").unwrap(), "原文");
    }
}