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
| Literal replacement definitions | 8,668 | Parsed; explicit comparison modes execute them |
| Regex replacement definitions | 8,597 | Parsed; optional bounded regex executor added |
| Stage-control directives (_TGL, _skip_sub, _SkipLayer) | 3,173 | Audited structurally; execution semantics unknown |
| Unknown non-comment definitions | 7 | Preserved as raw fields; not interpreted |

These are **rule definitions**, not independently established executable
functions. The original conditional sequencing, skip-layer routing,
backreference implementation, and custom .NET-pattern behavior are unknown.
Naively applying all literals in numeric-priority order is useful only as a
diagnostic approximation; it is not an EHND-equivalent execution model.

Supporting files additionally audited for presence and line counts:
SkipLayer_@Hdor.txt, PreFilter_@1Hdor_Set.txt, and the three
UserDict_@Hdor#*.txt dictionaries. The INI is UTF-16LE and the filter
files are BOM-bearing UTF-8 or UTF-16; all are read from user-owned files.

## EHND deep audit, November 2, 2026

Exact input inspection found a parser-classification error: some control
directives starting with underscore had numeric priority/type fields and
were accidentally classified as ordinary rules. The parser now recognizes only the exact `_TGL`, `_SkipLayer`, and
`_skip_sub` keywords before numeric metadata. The former underscore-prefix
test also misclassified ordinary patterns such as `_ㄹㅡㄴ∬해`. The table
above reflects the corrected classification recorded in the shared analysis;
the original corpus was not re-downloaded or rechecked after environment recovery.

On the locally supplied EHND files, the independent Python regex compiler
accepted all 8,595 active regex patterns in a *syntax-only* check.
This does not establish Rust fancy-regex or commercial EHND equivalence.
Pattern features include 6,759 positive lookahead, 1,617 positive
lookbehind, 848 negative lookbehind, 512 negative lookahead and
30 numeric backreference (group 1 or 2) occurrences.

The new opt-in src/ehnd_regex.rs uses fancy-regex 0.16.2 with bounded
backtracking, size limits, numeric capture replacement normalization
and explicit error locations. It compiles original ordinary literal and
regex rules in priority/source order. Missing conditional-rule semantics
are not guessed or silently enabled; strict mode rejects unsupported
directives or compile failures.

src/ehnd_control.rs extracts TGL family and control-id relationships.
Original data contains 704 _CONN, 262 _ON and 145 _DECR family
references, 144 unique control IDs, and 136 IDs shared between
_ON and _CONN. These are *structural relationships*, not established
runtime activation semantics.

On a personally owned local installation:

    cargo run -- audit-ehnd-regex --dir "C:\ezTrans XP\Ehnd" --out regex-audit.json
    cargo run -- audit-ehnd-control --dir "C:\ezTrans XP\Ehnd" --out control-audit.json
    cargo run -- file --input original.txt --out filtered.txt --engine native --ehnd-dir "C:\ezTrans XP\Ehnd" --ehnd-regex-experimental

Unverified: all EHND directive behavior, fully equivalent .NET regex
semantics, original J2KEngine.dlx morphology and dictionary codecs,
25-export ABI parity and bitwise-equivalent translations.

## Rust changes

* src/legacy/ehnd.rs — read-only filter corpus parser, configuration and
  supporting-file audit; explicit literal-only comparison mode.
* src/legacy/native.rs — opt-in loading of the original EHND user-owned text
  dictionaries as **lexical glossary entries**; proprietary markers
  containing # or 二 are deliberately not interpreted.
* src/lib.rs — native Rust glossary translator and selected C ABI functions.
* src/translation/engine/mod.rs — identifies only the root EHND wrapper and checks the
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
