#!/usr/bin/env bash

set -Eeuo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

command -v stellar >/dev/null 2>&1 || {
    printf 'check-resource-regressions: stellar CLI is required\n' >&2
    exit 1
}
command -v cargo >/dev/null 2>&1 || {
    printf 'check-resource-regressions: cargo is required\n' >&2
    exit 1
}

build_dir="$(mktemp -d "${TMPDIR:-/tmp}/grainhack-resource-check.XXXXXX")"
trap 'rm -rf "$build_dir"' EXIT

(cd "$REPO_ROOT" && stellar contract build --locked --package grainhack-escrow --out-dir "$build_dir")
wasm_path="$(find "$build_dir" -type f -name '*.wasm' -print -quit)"
[[ -n "$wasm_path" ]] || {
    printf 'check-resource-regressions: contract build produced no wasm file\n' >&2
    exit 1
}

(cd "$REPO_ROOT" && WASM_PATH="$wasm_path" cargo test --locked)
