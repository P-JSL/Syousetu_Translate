# Top-level J2KEngine.dll (EHND wrapper): migration scope

This page records the original adapter scope. The later request to consider
full core reconstruction is reviewed separately in
[eztrans-rust-feasibility.md](eztrans-rust-feasibility.md); the adapter identity
and the unverified compatibility claims below remain unchanged.

Scope decision: **use the ZIP-root J2KEngine.dll** as the only supported
proprietary *adapter identity*. Do not silently substitute
translator/J2KEngine.dll, which is another binary with different exports.

## Verified artifacts in ezTrans XP.zip

| Archive path | Bytes | SHA-256 | PE export identity |
|---|---:|---|---|
| J2KEngine.dll | 364032 | 751c7dad2fa62c9a62c7fa6eec09d691a897256bd5f308d76dd51810e12790b0 | EHND.dll / 25 exports |
| J2KEngineH.dll | 364032 | 751c7dad2fa62c9a62c7fa6eec09d691a897256bd5f308d76dd51810e12790b0 | byte-identical to the root DLL |
| J2KEngine.dlx | 2998272 | 141a53735eddfd950b9939af00606d8869f5665b9b1ca63be4adfbd3648f5ca0 | J2KEngine.dll / 466 exports |
| translator/J2KEngine.dll | 2998272 | 141a53735eddfd950b9939af00606d8869f5665b9b1ca63be4adfbd3648f5ca0 | byte-identical to J2KEngine.dlx |

All entries above are PE32 x86 binaries. Root DLL export directory calls
itself EHND.dll, and its UTF-16 text includes J2KEngine.dlx Load Failed.
Its loader uses Windows LoadLibrary/GetProcAddress imports. The commercial
translation function therefore cannot be faithfully reproduced merely by
copying the root DLL's exported function names into Rust.

## Verified root export table

| Ordinal | Export |
|---:|---|
| 1 | J2K_TranslateMMNT |
| 2 | J2K_TranslateMMNTW |
| 3 | ?GetJ2KMainDir |
| 4 | J2K_FreeMem |
| 5 | J2K_GetJ2KMainDir |
| 6 | J2K_GetPriorDict |
| 7 | J2K_GetProperty |
| 8 | J2K_Initialize |
| 9 | J2K_InitializeEx |
| 10 | J2K_ReloadUserDict |
| 11 | J2K_SetDelJPN |
| 12 | J2K_SetField |
| 13 | J2K_SetHnj2han |
| 14 | J2K_SetJWin |
| 15 | J2K_SetPriorDict |
| 16 | J2K_SetProperty |
| 17 | J2K_StopTranslation |
| 18 | J2K_Terminate |
| 19 | J2K_TranslateChat |
| 20 | J2K_TranslateFM |
| 21 | J2K_TranslateMM |
| 22 | J2K_TranslateMMEx |
| 23 | msvcrt_fopen |
| 24 | msvcrt_free |
| 25 | msvcrt_malloc |

Direct inspection of the root DLL disassembly shows J2K_InitializeEx
(at RVA 0x2870) saves/restores the stack and returns with ret 8, which
suggests callee-cleanup for 2 parameters in the wrapper. The UTF-16
translation entry at RVA 0x29f0 is implemented inside the wrapper. By
contrast many setter/getter exports are short indirect jumps to pointers
populated at runtime. The exported function prototypes and exact return
conventions for those indirect methods are **not fully recovered**.
Do not invent Rust ABI signatures for them based on the export names alone.

The two supplied Java versions use com.sun.jna.Library for these exports:
* ezTrans variant: InitializeEx(String,String), TranslateMMNT(int,Pointer)
  and FreeMem(Pointer), with Shift-JIS input / CP949 result.
* translator7 variant: InitializeEx(String,String), TranslateMMNTW(int,WString)
  and FreeMem(Pointer), with UTF-16 input and result.

Important: a PE entrypoint's stack cleanup and the Java-side JNA
Library calling convention must be resolved by actual Windows x86
call testing before changing the Rust FFI signatures. C-ABI function
signatures currently exposed by the independent Rust shim are **not
guaranteed bit-for-bit compatible with all 25 original exports**.

## Changes to this repository

- src/legacy/root_dll.rs validates PE32 x86 headers, reads every named export
  with bounds checking, and detects the J2KEngine.dlx dependency.
- src/main.rs exposes inspect-root-dll, and rejects a file whose
  25 exports and EHND name differ from the verified top-level wrapper.
- src/translation/engine/mod.rs rejects non-EHND DLLs in --engine eztrans mode and
  checks that the companion J2KEngine.dlx and Dat/ exist.
- Synthetic Rust tests cover the reader's PE offset handling.
- Real-file testing can use SYOUSETU_ROOT_DLL_PATH locally; no commercial
  binary is uploaded to GitHub or built into the Rust DLL.
- This validation pertains to **legacy adapter mode**. The independent
  Rust glossary translator remains a separate implementation and does
  not require EHND or a DLL.

Example:
```powershell
cargo run -- inspect-root-dll --dll "C:\ezTrans XP\J2KEngine.dll" --out root-wrapper.json
$env:SYOUSETU_ROOT_DLL_PATH = "C:\ezTrans XP\J2KEngine.dll"
cargo test --lib authentic_root_wrapper_if_available
```

## Outstanding and explicitly unverified

1. Runtime parity of the wrapper's EHND pre-/post-filters, dictionary
   priorities, and switch settings.
2. Full original ABI of all setter/getter exports including memory and
   thread ownership.
3. Elimination of the J2KEngine.dlx dependency while preserving the
   Japanese→Korean translation semantics (currently **not achieved**).
4. Bit-for-bit translation comparisons on the Kakuyomu/Alphapolis
   test works and full Windows x86 call-level end-to-end tests.
5. Verification that external redistributable licensing permits bundling
   the proprietary DLL and data (currently **not established**).

For grammar reconstruction and any use of proprietary data layouts,
inspect the *top-level wrapper's own calls* first. References to the
historically bundled J2KEngine.dlx are allowed only to establish the
dependency boundary, not as a substitute scope target.
