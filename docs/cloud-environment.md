# Cloud development environment

Use the existing checkout at `/workspace/Syousetu_Translate`. Cloud tasks are
already isolated; a separate Git worktree is unnecessary.

## Install or refresh

```bash
cd /workspace/Syousetu_Translate
bash scripts/install_cloud.sh
```

Requires a Linux C compiler/linker, Python 3, curl, and system CA certificates.
The installer bootstraps the official Rust stable toolchain with Clippy and
rustfmt when missing, fetches Cargo dependencies, and builds the debug CLI and
shared library. Existing Rust installations and Cargo lockfiles are reused;
the first installation creates `Cargo.lock` if it is absent. TLS and Rustup/Cargo
artifact verification stay enabled. No vendor assets or credentials are installed.

With no environment overrides, Rust tools live under `.local/cargo` and
`.local/rustup`; build output lives under `target/`. Build parallelism defaults
to four jobs and can be overridden with `CARGO_BUILD_JOBS`. Activate these tools
in an interactive shell with:

```bash
source scripts/cloud_env.sh
```

## Start after restoring the environment

```bash
cd /workspace/Syousetu_Translate
bash scripts/start_cloud.sh --help
bash scripts/start_cloud.sh inspect-config
bash scripts/start_cloud.sh file --input source.txt --out translated.txt --engine native
```

Paths passed to the wrapper are relative to the checkout. Startup uses the
retained lockfile and dependencies for an incremental offline build, then runs
the CLI with the supplied arguments. Run installation again if tools or cached
dependencies are missing. This project is a CLI/library; no background server,
port, database, or persistent process needs to be restarted.

After initial setup or a relevant environment change, an existing offline
workflow test checks translation, dictionary replacement, bilingual output,
and isolated parallel workers without external services:

```bash
source scripts/cloud_env.sh
cargo test --locked --offline --test cli parallel_translation_matches_sequential_with_dictionary_and_bilingual_lines -- --exact
```

Do not rerun unchanged suites on every startup. Use targeted checks for the
work being done. Original-engine and live-site checks remain optional and
require their own assets/platform/network access. Linux native mode is a
limited glossary engine; the original ezTrans DLL requires Windows x86.

## Recovery already applied

The verified recovery ZIP from the upstream `bakcup/` directory was applied
against commit `7969c4360b820597a8ebe0c9673e9b28bfd06d4e`. Its SHA-256 is
`abc76a6993943100d5d5f4531670ad538c7940815805d24e5d000c90fbd0a7fd`;
all 15 source files matched the archive manifest immediately after application.
See `docs/task-status.md` for the historical validation record and remaining
engine gaps. That record describes previous instances, not fresh test results.

Installation and startup do not apply recovery patches, switch Git branches,
reset source files, commit, push, or publish. Preserve working changes when
switching environments. Saving `install_script` and `start_skill` in environment
settings stores a draft; it does not publish a new environment snapshot.

## Validation in this instance (2026-11-03)

- Installed Rust/Cargo 1.99.1, Clippy, and rustfmt; Python 3.12.14 was available.
- Built the Linux debug CLI and `libsyousetu_translate.so`.
- Ran startup `--help` and `inspect-config`; the default engine was `native`.
- Ran the single offline CLI workflow test above: 1 passed, 6 filtered out.
- Reran installation with cached tools/dependencies: source files and
  `Cargo.lock` stayed byte-for-byte unchanged.
- Windows x86, proprietary assets, live-site downloads, and the remaining
  test suites were not rerun. A fresh-task snapshot restoration was not tested.
