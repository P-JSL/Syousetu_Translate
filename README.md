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

## Known gaps and constraints

- **The proprietary J2K translation engine itself is NOT ported into native Rust.** The original source package contains DLLs and data, not the underlying translation-engine implementation. It needs a user-provided licensed Windows DLL and Dat files, and its ABI and translation fidelity must be validated on a real machine.
- The original JavaFX GUI, config editor, asynchronous job pipeline and multiprocessing DLL workers are not yet ported.
- All production-site HTML variants, full work completeness, and original-vs-Rust translations have not been exhaustively tested.
- Aho–Corasick dictionary matching runs in approximately O(input length + matches) after preprocessing; network waits and native model translation are not guaranteed O(n).

**Third-party binaries, DLLs, dictionaries, license files and Java runtimes are intentionally excluded. Do not redistribute them without permission.**
