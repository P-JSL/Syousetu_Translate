#!/usr/bin/env bash
# Source this file to activate the repository's cloud Rust environment.
export SYOUSETU_PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_HOME="${CARGO_HOME:-"$SYOUSETU_PROJECT_ROOT/.local/cargo"}"
export RUSTUP_HOME="${RUSTUP_HOME:-"$SYOUSETU_PROJECT_ROOT/.local/rustup"}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-"$SYOUSETU_PROJECT_ROOT/target"}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
export PATH="$CARGO_HOME/bin:$PATH"
