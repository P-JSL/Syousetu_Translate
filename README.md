# Syousetu_Translate — Rust migration

**Status: early prototype / unverified.** The original attached archives were audited locally (39 and 45 Java source files, respectively). A Rust source ZIP with parsing, filter, file I/O, HTTP and Windows ezTrans DLL compatibility code was produced separately for review. The ZIP has **not yet been imported into this GitHub repository**. This README is not a claim of a completed migration.

## Test URLs

- Kakuyomu: https://kakuyomu.jp/works/2912051603596422650
- Alphapolis: https://www.alphapolis.co.jp/novel/767156331/284095783

## Important notes

- The original Alphapolis parser destroys the HTML DOM by calling `Jsoup.parse(tbody.text())` and can mistakenly accept an empty article. The new Rust prototype attempts DOM-based extraction and explicit error handling.
- The ezTrans application wrapper has available Java source. The proprietary **J2K translation engine itself** is only available as a Windows DLL and dictionary files; no independent model source was present. The Rust prototype includes a Windows DLL compatibility layer, not a reimplementation of the commercial engine.
- Vendor DLLs, JRE, dictionaries and licensed binaries will not be published here without redistribution rights.
- Rust compilation, Windows x86 DLL execution, and live end-to-end verification have not been completed in the current environment.
- O(n + matches) dictionary matching is a design objective; end-to-end web downloading and model translation are not guaranteed to be O(n).

Implementation must be checked in and compiled in a subsequent commit before this repository can be treated as a buildable project.
