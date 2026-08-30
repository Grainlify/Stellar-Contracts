#!/usr/bin/env bash
set -euo pipefail

# Inspect metadata in the final release wasm. A source-level macro is not
# evidence that the intended artifact was rebuilt, so this helper always asks
# the Stellar CLI to read the bytes that would be released or deployed.

usage() {
  cat <<'EOF'
Usage: scripts/inspect-contract-metadata.sh [OPTIONS]

Build or inspect grainhack-escrow and verify its embedded metadata.

Options:
  --wasm PATH       Inspect an existing wasm without rebuilding it
  --no-build        Use the standard release artifact already on disk
  --json            Print a machine-readable validation report
  --help            Show this help

The default path builds the contract with Cargo and asks the Stellar CLI to
display its metadata. The script never deploys or submits a transaction.
EOF
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE="grainhack-escrow"
TARGET="wasm32-unknown-unknown"
WASM_PATH="${ROOT_DIR}/target/${TARGET}/release/${PACKAGE//-/_}.wasm"
DO_BUILD=1
JSON_OUTPUT=0

die() {
  printf 'metadata inspection failed: %s\n' "$*" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --wasm)
      [[ $# -ge 2 ]] || die "--wasm requires a path"
      WASM_PATH="$2"
      DO_BUILD=0
      shift 2
      ;;
    --no-build)
      DO_BUILD=0
      shift
      ;;
    --json)
      JSON_OUTPUT=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

cd "$ROOT_DIR"
command -v cargo >/dev/null 2>&1 || die "cargo is not installed"
command -v stellar >/dev/null 2>&1 || die "stellar CLI is not installed"

if [[ "$DO_BUILD" -eq 1 ]]; then
  cargo build --package "$PACKAGE" --target "$TARGET" --release
fi

[[ -f "$WASM_PATH" ]] || die "wasm file does not exist: $WASM_PATH"

# Keep the expected values beside the inspection code. The package version is
# read from Cargo metadata and is compared with the literal in lib.rs before
# the wasm is inspected; all other public fields are intentionally stable.
EXPECTED_NAME="GrainHack escrow"
EXPECTED_VERSION="$(cargo metadata --no-deps --format-version 1 \
  | sed -n 's/.*"name":"grainhack-escrow".*"version":"\([^"]*\)".*/\1/p' \
  | head -1)"
EXPECTED_DESCRIPTION="Holds GrainHack prize pools, publishes commitments, and honours pull-based Merkle claims."
EXPECTED_LICENSE="MIT"
EXPECTED_REPOSITORY="https://github.com/Grainlify/Stellar-Contracts"
EXPECTED_CONTRACT="grainhack-escrow"
[[ -n "$EXPECTED_VERSION" ]] || die "could not read package version"
SOURCE_VERSION="$(sed -n 's/.*contractmeta!(key = "version", val = "\([^"]*\)").*/\1/p' \
  contracts/grainhack-escrow/src/lib.rs | head -1)"
[[ "$SOURCE_VERSION" == "$EXPECTED_VERSION" ]] || die \
  "metadata version $SOURCE_VERSION does not match Cargo version $EXPECTED_VERSION"

RAW_OUTPUT="$(stellar contract info meta --wasm="$WASM_PATH" 2>&1)" \
  || die "stellar CLI could not inspect $WASM_PATH: $RAW_OUTPUT"

contains() {
  grep -Fq "$1" <<<"$RAW_OUTPUT"
}

missing=()
contains "$EXPECTED_NAME" || missing+=("name=$EXPECTED_NAME")
contains "$EXPECTED_VERSION" || missing+=("version=$EXPECTED_VERSION")
contains "$EXPECTED_DESCRIPTION" || missing+=("description=$EXPECTED_DESCRIPTION")
contains "$EXPECTED_LICENSE" || missing+=("license=$EXPECTED_LICENSE")
contains "$EXPECTED_REPOSITORY" || missing+=("repository=$EXPECTED_REPOSITORY")
contains "$EXPECTED_CONTRACT" || missing+=("contract=$EXPECTED_CONTRACT")

size_bytes="$(wc -c < "$WASM_PATH" | tr -d ' ')"
sha256="$(shasum -a 256 "$WASM_PATH" | awk '{print $1}')"

if [[ "${#missing[@]}" -ne 0 ]]; then
  if [[ "$JSON_OUTPUT" -eq 1 ]]; then
    printf '{"ok":false,"wasm":"%s","size_bytes":%s,"sha256":"%s","missing":[' \
      "$WASM_PATH" "$size_bytes" "$sha256"
    first=1
    for item in "${missing[@]}"; do
      [[ "$first" -eq 1 ]] || printf ','
      printf '"%s"' "$item"
      first=0
    done
    printf ']}\n'
  else
    printf 'missing metadata entries:\n' >&2
    printf '  - %s\n' "${missing[@]}" >&2
    printf '\nRaw CLI output:\n%s\n' "$RAW_OUTPUT" >&2
  fi
  exit 1
fi

if [[ "$JSON_OUTPUT" -eq 1 ]]; then
  printf '{"ok":true,"wasm":"%s","size_bytes":%s,"sha256":"%s","name":"%s","version":"%s"}\n' \
    "$WASM_PATH" "$size_bytes" "$sha256" "$EXPECTED_NAME" "$EXPECTED_VERSION"
else
  printf 'Embedded contract metadata verified\n'
  printf '  wasm:        %s\n' "$WASM_PATH"
  printf '  size bytes:  %s\n' "$size_bytes"
  printf '  sha256:      %s\n' "$sha256"
  printf '  name:        %s\n' "$EXPECTED_NAME"
  printf '  version:     %s\n' "$EXPECTED_VERSION"
  printf '  description: %s\n' "$EXPECTED_DESCRIPTION"
  printf '  license:     %s\n' "$EXPECTED_LICENSE"
  printf '  repository:  %s\n' "$EXPECTED_REPOSITORY"
  printf '  contract:    %s\n' "$EXPECTED_CONTRACT"
fi
