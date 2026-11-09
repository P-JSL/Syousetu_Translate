# 0.2 offline engine update

The default GUI/CLI now uses direct Japanese→Korean SMaLL-100 inference in
Rust/Candle on 64-bit systems, with local user-correction memory. The proprietary
engine has not been reconstructed. The earlier original-DLL and glossary findings
below describe the legacy compatibility work. See [offline-engine.md](offline-engine.md)
for the current engine, CPU budget, ordered merge, validation and quality limits.

---

# translator7 web-client port

## Requested scope

The selected workflow is the `translator7.zip` web launcher running beside an
existing ezTrans installation:

**BAT → Rust GUI → novel URL → Run → translation task → saved text.**

Settings, chapter selection, output selection, dictionaries, progress and the
ordered task queue support this workflow. Recreating the whole ezTrans XP
application, its commercial language engine, the older client's ANSI API, and
the file/directory GUI is outside this change. Existing CLI functionality is
preserved.

## Reference files actually inspected

Both user-provided Google Drive downloads were inspected on 2026-11-03. The
second link names its file `ezTrans XP.zip`, rather than `ezTrans EX.zip`.

| Archive | Bytes | SHA-256 |
|---|---:|---|
| translator7.zip | 102042123 | `23ef00e3053dfcd061599139a2c1776ff11d4b5bc527e3f76834cb5cb0ca17b7` |
| ezTrans XP.zip | 287405116 | `cbd954b128235d805278171c5dd6bc3f187492a2393e4acc9d541399bc4b96a4` |

The `translator7.zip/src.zip` source archive contains 45 Java files; its SHA-256
is `c9b013b79102a4d8471890b48cdfad32d236b93ca69523b45f4d17c2575f71f3`.
The XP root source archive contains 39 Java files and was inspected to establish
the client/DLL boundary. Those files are client code, not the implementation of
the commercial translation engine. The downloaded vendor archives, extracted
Java sources, DLLs, dictionaries and JREs remain outside Git in `.local/`.

The original web BAT launches `javaw.exe -Dentry.target=web` and
`SyosetuReader.jar`. The Rust BAT launches `syousetu-gui.exe`; the desktop
client dispatches tasks to the sibling `syousetu-translate.exe`. No Java runtime
is used by this replacement.

## Source-to-Rust mapping

| translator7 source responsibility | Rust implementation |
|---|---|
| Web BAT, `ApplicationEntry`, `SyosetuApplication`, `SyosetuMainController` | `웹소설 번역기.bat`, `src/bin/syousetu-gui.rs` |
| `PageDialog`, output selection and automatic filenames | GUI chapter/output controls; existing `src/main.rs` and `src/translation/output.rs` |
| `ConfigHolder`, `ConfigDialog` | Public `src/config/mod.rs`, GUI settings, validated Java Properties save/load |
| `TranslateTask`, `SyosetuTranslateTask`, `ProgressController` | `src/jobs/mod.rs`: ordered tasks, status, progress, reordering and removal of inactive tasks |
| `SyosetuReader`, site parsers and information classes | Existing `src/novel/site/mod.rs`, `src/main.rs` and `src/translation/output.rs` |
| Ordinary HTTP retrieval and retries | `src/novel/network.rs`; saved HTML remains supported |
| `Eztrans`, `EztransWrapper`, `EztransProcess` | Existing `src/translation/engine/mod.rs`, `src/translation/session/mod.rs`: isolated workers and UTF-16 `J2K_TranslateMMNTW` calls |
| Special-character, bracket and dictionary filters | `src/translation/filter/`; full-width punctuation is normalized before bracket protection, as in translator7 |
| Safe saved output and settings | Shared `src/storage/atomic.rs` transaction writer |

The GUI uses `eztrans` by default when no saved configuration exists; it does
not silently substitute the limited native glossary engine. Each queued task
gets an immutable settings snapshot. Task readers are drained before a task is
marked complete, and worker responses are drained after an error to prevent
results crossing task boundaries. Windows subprocesses use `CREATE_NO_WINDOW`
so background translation does not open extra console windows.

## Windows use

1. Build/package both executables for Windows x86 (`i686-pc-windows-msvc`) or
   obtain the `SyousetuTranslate-Web-Windows-x86` CI artifact. The web package
   contains only the two executables, BAT launcher and usage guide.
2. Keep `syousetu-gui.exe` and `syousetu-translate.exe` together. The checked-in
   `웹소설 번역기.bat` finds either that flat layout or the build target layout.
3. Run the BAT, open Settings and select the existing working ezTrans folder
   containing the supported root `J2KEngineH.dll`/`J2KEngine.dll`, its
   `J2KEngine.dlx` dependency and `Dat/`. Alternatively use a private `engine/`
   bundle prepared with the existing installer. The full Java/XP application
   need not be copied.
4. Enter the work URL, optionally set the chapter range and output filename,
   and press **번역 실행**. If no output path is set and automatic filenames are
   disabled, a save dialog opens. Progress and errors appear in the task queue.

The original engine's dictionaries and dependencies are required for actual
ezTrans translation. The Rust client does not include or replace them.

## Verification and remaining limits

Current checks of the changed workflow passed:

- Linux CLI, GUI and library build with the optional `gui` feature.
- Windows x86 GNU cross-build of both executables and the Rust library.
- Eight CLI workflow tests, including edited settings round-trips and safe
  rejection of an invalid configuration without replacing the existing file.
- A desktop queue integration test: novel URL plus saved index/chapter HTML →
  queued translation → text output; task reordering, error recovery, settings
  isolation, output capture and progress are checked. This test uses the
  deterministic native test backend, not the proprietary Windows DLL.
- Both desktop tabs render in a headless egui frame.
- The native Linux GUI started in Xvfb; the Korean web and settings screens,
  URL input, chapter/output controls and Run button were visually checked.
- The affected dictionary/bracket filter tests passed, including full-width
  parentheses around protected dictionary text.

Clippy completed with existing warnings in older modules. The Windows GNU
library build emitted existing stdcall export-name fix-up warnings; its required
client entrypoints were verified in the PE export table. These are compile and
display checks, not a Windows runtime translation test.

Actual licensed-DLL initialization and novel translation still need validation
on Windows x86. Cross-compilation is not proof of DLL/ABI runtime parity.
Selenium/browser fallback and driver patching are not included; HTTP access
restrictions are reported, and saved accessible HTML can be used offline.
Live complete-work downloads and every site's current page variants have not
been revalidated by this change. Unchanged proprietary-asset and full suites
were not rerun locally; CI includes the normal platform checks.

Recovery was already applied in `9d0406f`, and cloud scripts in `ed8580b`.
Commit `fbe2eeb` merged upstream `main`'s verified `bakcup/` archive while
preserving those sources and was pushed to `main`. The recovery patch was not
applied again.
