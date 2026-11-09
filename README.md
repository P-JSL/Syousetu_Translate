# Syousetu Translate — Rust migration

**v0.3.1: Bundled SMaLL-100 INT8 for immediate offline Japanese→Korean translation. Windows x64 and Linux x64.**

This repository contains a Rust implementation of web-novel parsing, dictionary protection, text translation flow and native Windows DLL compatibility. It was based on the source-visible Java code included in `ezTrans XP.zip` (39 .java files) and `translator7.zip` (45 .java files).

The default `internal` engine runs the public **SMaLL-100 INT8** model directly from
Japanese to Korean in Rust/Candle. No English pivot, proprietary DLL, Python,
GPU, or translation API is required. The portable Windows release includes the
ready-to-run **354MB Q8_0 GGUF** plus tokenizer/configuration. Unzip the entire
package and launch the BAT: the adjacent `models` folder is selected automatically,
without initial model preparation or downloading. The model payload is
**360,332,983 bytes**, about 73% less than the former FP32 source download.
Optional repairs fetch only these prepared INT8 assets with pinned SHA-256 and
resumable downloads. Inference shares weights and runs offline. A live novel URL
still requires website access; use a 64-bit system.

User-approved sentence corrections and source-qualified terminology edits are
saved in a small local translation memory. This is lightweight adaptation,
not model-weight fine-tuning. Generated translations are never automatically
accepted as training data. See [offline engine and validation](docs/offline-engine.md)
and [third-party notices](THIRD_PARTY_NOTICES.md). This small public model can
mistranslate fiction, names and slang; it does not reproduce ezTrans output.

Both user-provided ZIPs were downloaded and checked against their SHA-256
records on 2026-11-03. Their client differences, Rust module mapping, and
remaining compatibility limits are recorded in [docs/porting-status.md](docs/porting-status.md).

## Build and test

```shell
cargo test --all-targets
cargo clippy --all-targets
cargo build --release
```

Production features exclude the tokenizer/archive adapters, Candle NN,
model-export tools and legacy backends. A std-only Rust dependency/package
audit prevents them entering the shipping build. See
[direct INT8 download and release scope](docs/direct-int8-download.md).

The optional Rust desktop client ports the `translator7.zip` web launcher:
**BAT → GUI → URL → Run → translation and save**, together with its settings
dialog and ordered task queue. Build both executables together, then start it:

```sh
cargo build --locked --features gui --bins
./target/debug/syousetu-gui
```

Keep `syousetu-gui` and `syousetu-translate` in the same directory (on Windows,
both use `.exe`). On Windows, launch **`웹소설 번역기.bat`**. The GUI defaults
to `internal`. Old GUI `native`/`eztrans` settings switch to the internal engine
in memory; existing settings files change only when saved. Output selection,
chapter ranges, settings save/load,
queue reordering, per-task settings snapshots, errors and progress are supported.
Linux needs a graphical desktop with X11/OpenGL runtime libraries. A Korean system font is loaded automatically; `SYOUSETU_FONT`
can select a local font. The CLI does not require the `gui` feature or a display.

Java/JRE/JavaFX and ezTrans XP are not needed for the default internal engine.
The portable GUI is ready after extraction; enter a work URL and run translation.
For a source/nonbundled build, **모델 준비 / 복구** downloads only prepared INT8.
Use **교정 기억** to save, edit or delete corrections.

The `SyousetuTranslate-Web-Windows-x64` CI artifact contains both executables,
the BAT launcher, notices and a [Windows usage guide](docs/windows-web-client.md).

GitHub Actions checks Linux x64 and Windows x64. The old CLI-only DLL adapter
requires a separate i686 build with `--no-default-features --features legacy`.
Recovery inspection/glossary tools are also behind `legacy`; the production
package contains no compatibility DLL or vendor/runtime/model bundles.
[INT8, TLS, performance research and validation](docs/int8-performance.md) covers
the current implementation. Sections below describe the optional legacy scope.

## Verified legacy binary file readers (not a full engine)

The Rust CLI can validate the on-disk layout of user-owned JisJK.Inf,
RuleKJ.Dat, CRuleKJ.Dat, and YongUn.Dat without loading the commercial DLL:

```sh
cargo run -- inspect-dat --dir "C:\path\to\ezTrans XP\Dat" --out inspection.json
```

The Rust library exposes lossless borrowed record readers for the aligned
JisJK table (128-byte rows), RuleKJ (1024-byte pages), compact length-prefixed
CRuleKJ, and YongUn signature/marker. This validates the **container format
only**. The proprietary grammar bytecode, cross-file indices and dictionary
codec are still unknown. The independent native translator does not use these
binary models as grammar rules. Do not infer semantic equivalence or translation
quality from successful inspection. See docs/legacy-binary-analysis.md.

To run the tests on locally installed, personally authorized original files:

```sh
SYOUSETU_LEGACY_DAT_DIR=/path/to/Dat cargo test --lib original_binary_assets_if_available
```

CI uses synthetic fixtures and excludes proprietary assets from releases.

## Original DLL scope: ZIP-root EHND wrapper

The sole *original DLL reference target* for this migration is the
364,032-byte **J2KEngine.dll at the root of ezTrans XP.zip**. The
same-byte J2KEngineH.dll is accepted as an alias. This wrapper advertises
the PE export name EHND.dll and 25 named exports. A separate
J2KEngine.dlx dependency supplies the original proprietary engine;
the 2,998,272-byte translator/J2KEngine.dll is identical to that
dependency, not to the root wrapper. Do not confuse the two.

Inspect your licensed local copy, without executing it:

```sh
cargo run -- inspect-root-dll --dll "C:\ezTrans XP\J2KEngine.dll" --out root-wrapper.json
```

Legacy --engine eztrans now checks that it is loading the **root
EHND DLL** and that J2KEngine.dlx and Dat/ exist before initializing it.
The independent Rust native translator does not need any of these files.
An export-name-compatible Rust DLL is NOT equivalent to reproducing all
the commercial engine's grammar and dictionary algorithms.

Full PE evidence, export ordinals, and known ABI gaps:
docs/root-dll-scope.md.

## Experimental EHND regex and control-flow audit

Original EHND filter files include 8,668 ordinary literal rules,
8,597 ordinary regex rules, 3,173 conditional directives, and 7
unclassified definitions. The structural parser recognizes the exact `_TGL`, `_SkipLayer`, and
`_skip_sub` control keywords before numeric columns. Other patterns starting
with an underscore remain ordinary rules.

An explicitly enabled *experimental* Rust filter executor supports
lookaround, backreferences, numeric capture replacements, and
literal transformations in priority order. A per-pattern backtrack
limit and input/output limits are enforced; regex failures return
an error with the source file and line number. The original EHND
conditional state machine is not implemented.

Run against user-owned local files:

```shell
cargo run -- audit-ehnd-regex --dir ./engine/Ehnd --out audit.json
cargo run -- audit-ehnd-control --dir ./engine/Ehnd --out controls.json
cargo run -- file --input source.txt --out experimental.txt --engine native --ehnd-dir ./engine/Ehnd --ehnd-regex-experimental
```

The --ehnd-regex-strict switch explicitly rejects incomplete execution.
Avoid this experimental filter mode for production content until results
are compared with the licensed original executable. Ordinary native
mode does not enable EHND filters. See docs/ehnd-parity-status.md.

## Rust-native J2K replacement (experimental)

The optional `legacy` feature preserves the old glossary and **four** C exports:
J2K_InitializeEx, J2K_TranslateMMNTW, J2K_TranslateMMNT and J2K_FreeMem.
These are excluded from the default CLI/GUI and release files. To explicitly
build the compatibility DLL, use `cargo rustc --release --lib --no-default-features
--features legacy --crate-type cdylib` (and the appropriate i686 Windows target).
The default engine is SMaLL-100 INT8, not this glossary.

**Important limitation:** This is NOT the ezTrans language engine recreated
bit-for-bit. It does not implement its proprietary parser, morphological
dictionary, grammar model, full transfer rules, or exact Japanese/Korean output.
It is a limited curated glossary and Korean particle postprocessor; unsupported
Japanese stays untranslated unless strict mode is enabled. It can be improved
with user-supplied translation dictionaries, but identical novel translations
are not currently possible. The commercial binary/dictionary is not needed for
the native mode and is not redistributed.

```sh
cargo run -- file --input original.txt --out translated.txt --engine native
cargo run -- file --input original.txt --out translated.txt --engine native --native-strict
cargo run -- file --input original.txt --out translated.txt --native-dictionary myterms.tsv
cargo build --release --target i686-pc-windows-msvc
```

The optional UTF-8 or UTF-16-BOM glossary format is one Japanese/Korean pair
per line, separated by TAB. Additional columns are ignored, so user-owned
Ehnd UserDict-style text files may be used. Last matching dictionary entry
overrides a built-in translation; longest Japanese match wins for overlapping
terms. For the DLL interface, set SYOUSETU_NATIVE_DICTIONARY to an absolute
glossary path **before the first DLL initialization**. The DLL does not read
the legacy Dat/ binary dictionary, and its InitializeEx path/ID parameters are
ignored. All calls are UTF-16, return a DLL-owned allocation, and require
J2K_FreeMem for each returned pointer. The four client-used exports and limited
lifecycle aliases do not reproduce all 25 original wrapper exports.

CLI compatibility: --engine eztrans and --eztrans-dir PATH retain the old
licensed ezTrans DLL adapter; --passthrough retains raw Japanese for parser
testing. Explicit --eztrans-dir without --engine still selects the legacy
adapter. Default native mode cannot be treated as a production-grade
Japanese-to-Korean translator.

## Commands

```shell
cargo run -- inspect --url https://kakuyomu.jp/works/2912051603596422650
cargo run -- inspect --url https://www.alphapolis.co.jp/novel/767156331/284095783
cargo run -- download --url https://kakuyomu.jp/works/2912051603596422650 --out novel.txt --start 1 --end 2 --eztrans-dir "C:\\ezTrans"
cargo run -- file --input original.txt --out translated.txt --eztrans-dir "C:\\ezTrans"
```

When automated access is restricted, legally accessible pages saved by the user can be parsed offline with `inspect --url ... --html saved-work.html` or `download --html work.html --episode-html-dir folder` (chapter files `1.html`, `2.html`, etc.). No CAPTCHA, premium, age verification or sign-in bypass is implemented.

Use `--passthrough` only for parser tests. It retains the Japanese source text and is **not** a translation engine.

## Test targets

- Kakuyomu: https://kakuyomu.jp/works/2912051603596422650
- Alphapolis: https://www.alphapolis.co.jp/novel/767156331/284095783

The original Alphapolis Java parser loses DOM markup by calling `Jsoup.parse(tbody.text())`. The Rust version extracts from the original DOM with alternative selectors and rejects missing article bodies. Live Alphapolis access was unavailable in the assistant environment. Real complete-work downloads and live site-specific tests remain unverified.

## What is implemented

- Six original website URL families (Kakuyomu, Alphapolis, Syosetu, Syosetu18, Hameln, Novelup)
- HTML parsing and legacy Kakuyomu `__NEXT_DATA__` JSON fallback
- DOM-based chapter extraction and ruby pronunciation handling
- HTTP timeouts, bounded retries, clear restriction errors, basic request throttling
- UTF-8 and UTF-16(BOM) file input, separate atomic output file
- Aho–Corasick dictionary replacement, hangul particle selection and bracket/emoji preservation
- Dynamic loading of a user-provided Windows ezTrans DLL (original J2K ABI), in a 32-bit compatible process
- CI unit tests and builds on Linux/Windows

## Local ezTrans installation (Windows x86)

If you own a usable ezTrans XP installation, you can create a private local
engine bundle from its ZIP without manually selecting DLLs.

```powershell
py -3 scripts/install_local_engine.py --archive "C:\\Downloads\\ezTrans XP.zip"
cargo build --release --target i686-pc-windows-msvc
.\\target\\i686-pc-windows-msvc\\release\\syousetu-translate.exe file --input original.txt --out translated.txt --engine eztrans
```

The installer extracts `J2KEngine.dll` / `J2KEngineH.dll`, the required root
`J2KEngine.dlx` dependency, relevant support DLLs, `Dat/`, and optional `Ehnd/`
into the Git-ignored `engine/` folder. Archives missing the root dependency
are rejected before extraction; `translator/J2KEngine.dll` is not substituted
for it. It never overwrites an existing destination. For portable deployment,
copy your own `engine/` beside the 32-bit executable. By default the program
searches `engine/` beside the executable and in the current working directory.
Use `--eztrans-dir PATH` or `SYOUSETU_EZTRANS_DIR` to override that location.

The installer rejects ZIP path traversal and duplicate filenames, then
verifies extracted byte counts. Synthetic fixture tests run with:

```sh
python -m unittest discover -s scripts -p 'test_*.py'
```

No vendor files are stored in Git. The supplied PE32 DLL exports the native
`J2K_TranslateMMNTW` interface, but actual engine initialization/translation
on Windows has not yet been smoke-tested.

## Known gaps and constraints

- **The proprietary J2K translation engine itself is NOT ported into native Rust.** The original source package contains DLLs and data, not the underlying translation-engine implementation. It needs a user-provided licensed Windows DLL and Dat files, and its ABI and translation fidelity must be validated on a real machine.
- The `translator7` web GUI, settings editor, task queue and isolated DLL workers
  have Rust counterparts. The file/directory GUI and the older client's ANSI API
  are outside the selected web-workflow scope; existing CLI file commands remain.
- Browser/Selenium fallback and driver patching are not implemented. Use ordinary
  HTTP access or saved HTML; login, paid-content and CAPTCHA barriers are not bypassed.
- All production-site HTML variants, full work completeness, and original-vs-Rust translations have not been exhaustively tested.
- Aho–Corasick dictionary matching runs in approximately O(input length + matches) after preprocessing; network waits and native model translation are not guaranteed O(n).

**Third-party binaries, DLLs, dictionaries, license files and Java runtimes are intentionally excluded. Do not redistribute them without permission.**
