//! Optional legacy C ABI, excluded from the normal app and release package.
use crate::native::NativeTranslator;
use std::{
    collections::HashMap,
    ffi::{c_char, c_void},
    path::Path,
    ptr,
    sync::{Arc, Mutex, OnceLock},
};
use encoding_rs::{EUC_KR, SHIFT_JIS};

const MAX_TEXT_UNITS: usize = 8_000_000;

#[derive(Default)]
struct Session { engine: Option<Arc<NativeTranslator>> }
static SESSION: OnceLock<Mutex<Session>> = OnceLock::new();
static WIDE_ALLOCATIONS: OnceLock<Mutex<HashMap<usize, Box<[u16]>>>> = OnceLock::new();
static NARROW_ALLOCATIONS: OnceLock<Mutex<HashMap<usize, Box<[u8]>>>> = OnceLock::new();

fn session() -> &'static Mutex<Session> {
    SESSION.get_or_init(|| Mutex::new(Session::default()))
}
fn wide_allocations() -> &'static Mutex<HashMap<usize,Box<[u16]>>> {
    WIDE_ALLOCATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn narrow_allocations() -> &'static Mutex<HashMap<usize,Box<[u8]>>> {
    NARROW_ALLOCATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn build_translator() -> Result<Arc<NativeTranslator>, String> {
    let path = std::env::var_os("SYOUSETU_NATIVE_DICTIONARY");
    NativeTranslator::new(path.as_deref().map(Path::new), false)
        .map(Arc::new).map_err(|err| err.to_string())
}
fn current_translator() -> Option<Arc<NativeTranslator>> {
    session().lock().unwrap_or_else(|e|e.into_inner()).engine.clone()
}

/// Initialize from the independent Rust glossary; ignores legacy vendor
/// path/license arguments. Returns 1 on success, 0 on failure.
/// # Safety
/// Argument pointers are never dereferenced.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn J2K_InitializeEx(_id: *const c_char,_dir: *const c_char) -> i32 {
    std::panic::catch_unwind(||{
        match build_translator() {
            Ok(engine) => {
                session().lock().unwrap_or_else(|e|e.into_inner()).engine=Some(engine);
                1
            }
            Err(_) => 0,
        }
    }).unwrap_or(0)
}

/// Compatibility shortcut for callers which pass just one legacy path.
/// # Safety
/// Input pointer is ignored.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_Initialize(path: *const c_char) -> i32 {
    unsafe { J2K_InitializeEx(path, ptr::null()) }
}

/// Discard the active dictionary. Previously returned translation allocations
/// remain valid until the caller invokes J2K_FreeMem.
/// # Safety
/// No raw pointers are accessed.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_Terminate() -> i32 {
    session().lock().unwrap_or_else(|e|e.into_inner()).engine=None;
    1
}

/// Reload user-owned glossary. Old dictionary stays active on failure.
/// Existing translations continue with the previous Arc snapshot.
/// # Safety
/// No raw pointers are accessed.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_ReloadUserDict() -> i32 {
    let Some(_) = current_translator() else { return 0; };
    unsafe { J2K_InitializeEx(ptr::null(),ptr::null()) }
}

/// Legacy stop hook. Translator is synchronous; returning success does not
/// imply an already running call can be interrupted.
/// # Safety
/// No raw pointers are accessed.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_StopTranslation(_handle: i32) -> i32 { 1 }

fn alloc_wide(value: &str) -> *mut u16 {
    let mut units: Vec<u16> = value.encode_utf16().collect();
    units.push(0);
    let mut owned=units.into_boxed_slice();
    let pointer=owned.as_mut_ptr();
    wide_allocations().lock().unwrap_or_else(|e|e.into_inner())
        .insert(pointer as usize,owned);
    pointer
}
fn alloc_narrow(value: &[u8]) -> *mut c_char {
    let mut bytes=value.to_vec();
    bytes.push(0);
    let mut owned=bytes.into_boxed_slice();
    let pointer=owned.as_mut_ptr();
    narrow_allocations().lock().unwrap_or_else(|e|e.into_inner())
        .insert(pointer as usize,owned);
    pointer.cast()
}

/// UTF-16 source/result implementation. Returned pointer is DLL-owned.
/// # Safety
/// The caller must supply a readable, NUL-terminated UTF-16 pointer no longer
/// than MAX_TEXT_UNITS, and release any result using J2K_FreeMem.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn J2K_TranslateMMNTW(_flags:i32,source:*const u16)->*mut u16 {
    if source.is_null(){return ptr::null_mut();}
    std::panic::catch_unwind(|| {
        let engine=current_translator()?;
        let mut count=0usize;
        while count < MAX_TEXT_UNITS && unsafe{*source.add(count)}!=0 {
            count+=1;
        }
        if count==MAX_TEXT_UNITS {return None;}
        let input=String::from_utf16(unsafe{std::slice::from_raw_parts(source,count)}).ok()?;
        let text=engine.translate(&input).ok()?;
        Some(alloc_wide(&text))
    }).ok().flatten().unwrap_or(ptr::null_mut())
}

/// CP932 source, CP949 output. Locale-independent code pages.
/// # Safety
/// Input pointer must be readable until NUL within MAX_TEXT_UNITS.
/// Output must be released with J2K_FreeMem.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn J2K_TranslateMMNT(_flags:i32,source:*const c_char)->*mut c_char {
    if source.is_null() {return ptr::null_mut();}
    std::panic::catch_unwind(|| {
        let engine=current_translator()?;
        let mut count=0usize;
        while count < MAX_TEXT_UNITS && unsafe{*source.add(count)}!=0 {count+=1;}
        if count==MAX_TEXT_UNITS{return None;}
        let input_bytes=unsafe{std::slice::from_raw_parts(source.cast::<u8>(),count)};
        let (input,_,input_bad)=SHIFT_JIS.decode(input_bytes);
        if input_bad{return None;}
        let translated=engine.translate(&input).ok()?;
        let (bytes,_,output_bad)=EUC_KR.encode(&translated);
        if output_bad{return None;}
        Some(alloc_narrow(&bytes))
    }).ok().flatten().unwrap_or(ptr::null_mut())
}

/// Classic narrow-string aliases; original ezTrans may differ in options.
/// # Safety
/// Same preconditions as J2K_TranslateMMNT.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_TranslateMM(flags:i32,source:*const c_char)->*mut c_char {
    unsafe {J2K_TranslateMMNT(flags,source)}
}
/// # Safety
/// Same preconditions as J2K_TranslateMMNT.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_TranslateMMEx(flags:i32,source:*const c_char)->*mut c_char {
    unsafe {J2K_TranslateMMNT(flags,source)}
}
/// # Safety
/// Same preconditions as J2K_TranslateMMNT.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_TranslateChat(flags:i32,source:*const c_char)->*mut c_char {
    unsafe {J2K_TranslateMMNT(flags,source)}
}

/// Unsupported file translator API: null instead of a misleading success.
/// # Safety
/// The raw filename is ignored and never dereferenced.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_TranslateFM(_flags:i32,_filename:*const c_char)->*mut c_char {
    ptr::null_mut()
}

/// Release wide or narrow allocation from this Rust DLL.
/// # Safety
/// This function compares pointer addresses but does not dereference them.
/// Foreign pointers are rejected; same pointer must not be freed elsewhere.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_FreeMem(pointer:*mut c_void)->i32 {
    if pointer.is_null(){return 0;}
    if wide_allocations().lock().unwrap_or_else(|e|e.into_inner())
        .remove(&(pointer as usize)).is_some() {return 1;}
    if narrow_allocations().lock().unwrap_or_else(|e|e.into_inner())
        .remove(&(pointer as usize)).is_some() {return 1;}
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_both_encodings_and_reject_double_free() {
        unsafe {
            assert_eq!(J2K_InitializeEx(ptr::null(),ptr::null()),1);
            let input:Vec<u16>="私は学生です。".encode_utf16().chain([0]).collect();
            let wide=J2K_TranslateMMNTW(0,input.as_ptr());
            assert!(!wide.is_null());
            let expected:Vec<u16>="나는 학생입니다。".encode_utf16().collect();
            let slice=std::slice::from_raw_parts(wide,expected.len());
            assert_eq!(slice,expected.as_slice());
            assert_eq!(J2K_FreeMem(wide.cast()),1);
            assert_eq!(J2K_FreeMem(wide.cast()),0);

            let (bytes,_,bad)=SHIFT_JIS.encode("私は学生です。");
            assert!(!bad);
            let mut input=bytes.into_owned(); input.push(0);
            let narrow=J2K_TranslateMMNT(0,input.as_ptr().cast());
            assert!(!narrow.is_null());
            let out=std::ffi::CStr::from_ptr(narrow).to_bytes();
            let (decoded,_,decode_bad)=EUC_KR.decode(out);
            assert!(!decode_bad);
            assert_eq!(decoded,"나는 학생입니다。");
            assert_eq!(J2K_FreeMem(narrow.cast()),1);

            assert!(J2K_TranslateMMNTW(0,ptr::null()).is_null());
            assert!(J2K_TranslateMMNT(0,ptr::null()).is_null());
            assert_eq!(J2K_FreeMem(ptr::null_mut()),0);
        }
    }
}
