#!/usr/bin/env bash

set -Eeuo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
readonly BASELINE="$REPO_ROOT/contracts/grainhack-escrow/resource_baseline.json"

command -v stellar >/dev/null 2>&1 || {
    printf 'update-resource-baseline: stellar CLI is required\n' >&2
    exit 1
}
command -v cargo >/dev/null 2>&1 || {
    printf 'update-resource-baseline: cargo is required\n' >&2
    exit 1
}

build_dir="$(mktemp -d "${TMPDIR:-/tmp}/grainhack-resource-update.XXXXXX")"
trap 'rm -rf "$build_dir"' EXIT

(cd "$REPO_ROOT" && stellar contract build --locked --package grainhack-escrow --out-dir "$build_dir")
wasm_path="$(find "$build_dir" -type f -name '*.wasm' -print -quit)"
[[ -n "$wasm_path" ]] || {
    printf 'update-resource-baseline: contract build produced no wasm file\n' >&2
    exit 1
}

printf 'Regenerating %s; review the diff before committing.\n' "$BASELINE"
(cd "$REPO_ROOT" && RESOURCE_BASELINE_OUTPUT="$BASELINE" WASM_PATH="$wasm_path" \
    cargo test --locked -- --ignored write_resource_baseline)
