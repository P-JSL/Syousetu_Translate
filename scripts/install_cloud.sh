#!/usr/bin/env bash
# Install development tools and build the Linux CLI/library; safe to rerun.
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/cloud_env.sh"
cd "$SYOUSETU_PROJECT_ROOT"

for tool in cc python3 curl; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'Missing prerequisite: %s (install build-essential, python3, curl and CA certificates).\n' "$tool" >&2
        exit 1
    fi
done

mkdir -p "$CARGO_HOME" "$RUSTUP_HOME"
if ! command -v rustup >/dev/null 2>&1; then
    installer=$(mktemp)
    trap 'rm -f -- "$installer"' EXIT
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
        --retry 3 https://sh.rustup.rs --output "$installer"
    sh "$installer" -y --no-modify-path --profile minimal \
        --default-toolchain "$RUSTUP_TOOLCHAIN" --component clippy --component rustfmt
fi

# Reuse a retained toolchain rather than updating stable on every setup run.
if ! rustup run "$RUSTUP_TOOLCHAIN" rustc --version >/dev/null 2>&1; then
    rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal
fi
rustup component add --toolchain "$RUSTUP_TOOLCHAIN" clippy rustfmt
rustc --version
cargo --version
python3 --version

# Preserve an existing lockfile. Cargo creates one on the first build if absent.
if [[ -f Cargo.lock ]]; then
    cargo fetch --locked
else
    cargo fetch
fi
cargo build --locked --bins --lib
"$CARGO_TARGET_DIR/debug/syousetu-translate" --help
