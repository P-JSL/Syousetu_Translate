#!/usr/bin/env bash
# Start the CLI from the existing checkout, using retained dependencies offline.
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/cloud_env.sh"
cd "$SYOUSETU_PROJECT_ROOT"

if ! command -v cargo >/dev/null 2>&1 || [[ ! -f Cargo.lock ]]; then
    printf 'Run bash scripts/install_cloud.sh before starting this environment.\n' >&2
    exit 1
fi

# Incremental compilation prevents accidentally running an older source build.
cargo build --locked --offline --bin syousetu-translate
exec "$CARGO_TARGET_DIR/debug/syousetu-translate" "$@"
