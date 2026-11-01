# EHND & original ezTrans Rust parity: implementation status

Date: 2026-11-02. Scope: **ZIP-root J2KEngine.dll (PE export name EHND.dll)**.

**This is NOT a complete replacement.** Do not claim grammar fidelity,
translation accuracy parity, or 25/25 API compatibility based on a successful
build. The original wrapper delegates translation to J2KEngine.dlx.

## Binary evidence

* User-supplied root J2KEngine.dll: 364,032 bytes, 25 exports, PE32 x86.
* J2KEngineH.dll is byte-for-byte identical.
* J2KEngine.dlx is **another** 2,998,272-byte PE32 binary with 466
  exports; it is required for the original wrapper's translation behavior.
* Assembly at PE RVA 0x2870 (J2K_InitializeEx) ends with ret 8.
* Assembly at PE RVA 0x29f0 (J2K_TranslateMMNTW) has ret 8 paths;
  the narrow wrapper at 0x2f60 invokes it.
* These concrete two-argument entrypoints now use Rust extern "system"
  for Windows x86. Unverified function prototypes are not automatically
  assigned guessed calling conventions.

## EHND rule corpus in the supplied archive

Rust reads both main and _C variants of PreFilter/PostFilter. Independent
analysis of the user's original, local EHND files found:

| Category | Count | Current status |
|---|---:|---|
| All physical lines | 37,870 | Counted |
| Comments / blank lines | 17,425 | Ignored intentionally |
| Literal replacement definitions | 8,667 | Parsed; explicit comparison mode executes them |
| Regex replacement definitions | 8,597 | Parsed, not executed |
| Stage-control directives (_TGL, _skip_sub, _SkipLayer, etc.) | 3,141 | Preserved as raw fields; not interpreted |
| Unknown non-comment definitions | 40 | Preserved as raw fields; not interpreted |

These are **rule definitions**, not independently established executable
functions. The original conditional sequencing, skip-layer routing,
backreference implementation, and custom .NET-pattern behavior are unknown.
Naively applying all literals in numeric-priority order is useful only as a
diagnostic approximation; it is not an EHND-equivalent execution model.

Supporting files additionally audited for presence and line counts:
SkipLayer_@Hdor.txt, PreFilter_@1Hdor_Set.txt, and the three
UserDict_@Hdor#*.txt dictionaries. The INI is UTF-16LE and the filter
files are BOM-bearing UTF-8 or UTF-16; all are read from user-owned files.

## Rust changes

* src/ehnd.rs — read-only filter corpus parser, configuration and
  supporting-file audit; explicit literal-only comparison mode.
* src/native.rs — opt-in loading of the original EHND user-owned text
  dictionaries as **lexical glossary entries**; proprietary markers
  containing # or 二 are deliberately not interpreted.
* src/lib.rs — native Rust glossary translator and selected C ABI functions.
* src/engine.rs — identifies only the root EHND wrapper and checks the
  companion J2KEngine.dlx and Dat directory.
* scripts/verify_pe_exports.py and CI — inspect the built Rust DLL's actual
  Windows x86 symbol names; this is necessary but not sufficient for ABI
  compatibility.
* main CLI — inspect-root-dll, inspect-dat, inspect-ehnd,
  compare-engines, and explicit --ehnd-literal-only experiment.

## Try on owned original files

Inspect EHND without invoking DLL:

```powershell
cargo run -- inspect-ehnd --dir "C:\ezTrans XP" --out ehnd-report.json
cargo run -- inspect-root-dll --dll "C:\ezTrans XP\J2KEngine.dll"
```

Native translation with supplementary EHND user vocabulary only
(**does not reproduce original grammar**):

```powershell
cargo run -- file --input original.txt --out native.txt --engine native --native-dictionary "C:\ezTrans XP\Ehnd"
```

Experimental literal-filter comparison (requires full EHND folder; regex
and control directives are skipped and results may be incorrect):

```powershell
cargo run -- file --input original.txt --out ehnd-experiment.txt --engine native --ehnd-dir "C:\ezTrans XP\Ehnd" --ehnd-literal-only
```

On a 32-bit Windows executable, compare original commercial DLL versus
independent Rust translation with a user-owned local installation:

```powershell
cargo run --release --target i686-pc-windows-msvc -- compare-engines --eztrans-dir "C:\ezTrans XP" --input tests\fixtures\original_engine_probe.txt --out comparison.json
```

Original commercial DLL and dictionary files must remain local; the CI
workflow does not have them. A green live-site smoke-test job is
non-blocking and does NOT mean all sites or original translation passed.

## Blocking gaps to exact parity

1. **EHND state machine:** config switches, stages, skip rules,
   user dictionary priority, named captures and custom regex semantics.
2. **Original engine:** J2KEngine.dlx interpreter, binary dictionary
   indices, word segmentation, Japanese grammar parse and Korean output.
3. **API surface:** exact signatures, side effects and memory lifetime for
   all 25 original exports; current shim exposes only a subset and
   some aliases have intentionally approximate semantics.
4. **Reference tests:** execute the licensed original DLL on Windows x86
   against an established sentence corpus, compare output, fix each
   deviation. This is not executable in public CI without authorized files.
5. **Quality and load:** sentence accuracy, throughput and compatibility
   must be measured against the same baseline. The current Rust lexical
   engine's quality does not equal ezTrans.

The remaining unknown algorithms cannot be inferred merely from the DLL
export table. Reimplementation should follow verified dataflow and differential
tests; unknown rules must not be silently treated as correct.
