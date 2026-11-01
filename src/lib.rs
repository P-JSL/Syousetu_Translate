//! Native Rust replacement for the three ezTrans J2K functions consumed by
//! the archived Java application. This preserves the *calling interface*,
//! not the proprietary ezTrans translation quality or binary dictionary format.
//! Native engine is independent of J2KEngine*.dll and Dat/ files.
pub mod dictionary;
pub mod morph;
pub mod native;

use native::NativeTranslator;
use std::{
    collections::HashMap,
    ffi::{c_char, c_void},
    path::Path,
    ptr,
    sync::{Mutex, OnceLock},
};

const MAX_UTF16: usize = 8_000_000;
static ENGINE: OnceLock<Result<NativeTranslator, String>> = OnceLock::new();
static ALLOCATIONS: OnceLock<Mutex<HashMap<usize, Box<[u16]>>>> = OnceLock::new();

fn translator() -> Option<&'static NativeTranslator> {
    ENGINE.get_or_init(|| {
        let dictionary = std::env::var_os("SYOUSETU_NATIVE_DICTIONARY");
        NativeTranslator::new(dictionary.as_deref().map(Path::new), false)
            .map_err(|err| err.to_string())
    }).as_ref().ok()
}

fn allocations() -> &'static Mutex<HashMap<usize, Box<[u16]>>> {
    ALLOCATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Legacy ezTrans interface. The ID and Dat location are intentionally
/// ignored: this is an independent Rust glossary engine.
/// Returns 1 on initialization success, 0 on failure.
///
/// # Safety
/// Accepts raw C pointers following the legacy API; these values are not read.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_InitializeEx(_id: *const c_char, _dir: *const c_char) -> i32 {
    if translator().is_some() { 1 } else { 0 }
}

/// Translate a null-terminated UTF-16 string. A returned pointer is owned by
/// the DLL and must be released with J2K_FreeMem, never with libc free.
///
/// # Safety
/// Caller must supply a valid readable UTF-16 pointer terminated by NUL
/// within MAX_UTF16 code units. Passing arbitrary pointers is undefined.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_TranslateMMNTW(
    _flags: i32, source: *const u16,
) -> *mut u16 {
    if source.is_null() { return ptr::null_mut(); }
    std::panic::catch_unwind(|| {
        let translator = translator()?;
        let mut len = 0usize;
        while len < MAX_UTF16 && unsafe { *source.add(len) } != 0 {
            len += 1;
        }
        if len == MAX_UTF16 { return None; }
        let units = unsafe { std::slice::from_raw_parts(source, len) };
        let input = String::from_utf16(units).ok()?;
        let translated = translator.translate(&input).ok()?;
        let mut units: Vec<u16> = translated.encode_utf16().collect();
        units.push(0);
        let mut owned = units.into_boxed_slice();
        let pointer = owned.as_mut_ptr();
        allocations().lock().unwrap_or_else(|p| p.into_inner())
            .insert(pointer as usize, owned);
        Some(pointer)
    }).ok().flatten().unwrap_or(ptr::null_mut())
}

/// Free only a pointer previously produced by J2K_TranslateMMNTW.
/// Double free or foreign pointers return 0 without dereferencing.
///
/// # Safety
/// The pointer must be a value returned by this DLL, or null. Never pass
/// pointers owned by the original proprietary DLL.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn J2K_FreeMem(pointer: *mut c_void) -> i32 {
    if pointer.is_null() { return 0; }
    if allocations().lock().unwrap_or_else(|p| p.into_inner())
        .remove(&(pointer as usize)).is_some() { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_ffi_round_trip_and_double_free() {
        unsafe {
            assert_eq!(J2K_InitializeEx(ptr::null(), ptr::null()), 1);
            let input: Vec<u16> = "私は学生です。".encode_utf16().chain([0]).collect();
            let result = J2K_TranslateMMNTW(0, input.as_ptr());
            assert!(!result.is_null());
            let len = "나는 학생입니다。".encode_utf16().count();
            assert_eq!(String::from_utf16(std::slice::from_raw_parts(result, len)).unwrap(),
                "나는 학생입니다。");
            assert_eq!(J2K_FreeMem(result.cast()), 1);
            assert_eq!(J2K_FreeMem(result.cast()), 0);
            assert!(J2K_TranslateMMNTW(0, ptr::null()).is_null());
            assert_eq!(J2K_FreeMem(ptr::null_mut()), 0);
        }
    }
}