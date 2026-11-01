# Syousetu Translate — Rust migration

**Development status: Rust CLI prototype; engine/GUI feature parity is not complete.**

This repository contains a Rust implementation of web-novel parsing, dictionary protection, text translation flow and native Windows DLL compatibility. It was based on the source-visible Java code included in `ezTrans XP.zip` (39 .java files) and `translator7.zip` (45 .java files).

## Build and test

```shell
cargo test --all-targets
cargo clippy --all-targets
cargo build --release
```

On Windows, build the **i686-pc-windows-msvc** target if you are using a 32-bit original ezTrans engine. GitHub Actions checks Linux x64 and Windows x86 on commits.

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

## Rust-native J2K replacement (experimental)

The CLI now uses a **Rust-only, DLL-free** Japanese → Korean glossary engine by
default. The package also compiles a native DLL exposing the original Java
integration's **three** C exports: J2K_InitializeEx, J2K_TranslateMMNTW
(UTF-16), and J2K_FreeMem. The Windows x86 artifact includes the executable and
syousetu_translate.dll. If you need the Java application's historical DLL
filename, rename a copy of the Rust DLL to J2KEngineH.dll and place it on
jna.library.path; do not mix pointers from different DLL builds.

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
J2K_FreeMem for each returned pointer. Only the three exports used by the
archived Java client are currently provided.

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
.\\target\\i686-pc-windows-msvc\\release\\syousetu-translate.exe file --input original.txt --out translated.txt
```

The installer extracts `J2KEngine.dll` / `J2KEngineH.dll`, relevant
support DLLs, `Dat/`, and optional `Ehnd/` into the Git-ignored `engine/`
folder. It never overwrites an existing destination. For portable deployment,
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
- The original JavaFX GUI, config editor, asynchronous job pipeline and multiprocessing DLL workers are not yet ported.
- All production-site HTML variants, full work completeness, and original-vs-Rust translations have not been exhaustively tested.
- Aho–Corasick dictionary matching runs in approximately O(input length + matches) after preprocessing; network waits and native model translation are not guaranteed O(n).

**Third-party binaries, DLLs, dictionaries, license files and Java runtimes are intentionally excluded. Do not redistribute them without permission.**
