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
            let (encoded,_,bad)=encoding_rs::EUC_KR.encode(&dir.join("Dat").to_string_lossy());
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
#[derive(Debug, Clone)]
pub struct EngineConfig {pub eztrans_dir: Option<PathBuf>, pub passthrough: bool}
pub fn create_engine(c:&EngineConfig)->Result<Box<dyn Translator>>{
    if c.passthrough {return Ok(Box::new(Passthrough));}
    let path=c.eztrans_dir.as_deref().context("--eztrans-dir 또는 --passthrough를 지정하세요")?;
    Ok(Box::new(EzTrans::open(path)?))
}