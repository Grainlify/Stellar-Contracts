#!/usr/bin/env bash

set -Eeuo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
readonly CONTRACT_PACKAGE="grainhack-escrow"
readonly DEFAULT_OUTPUT="$REPO_ROOT/deployments/testnet/grainhack-escrow.json"

network="${STELLAR_NETWORK:-}"
source_account="${STELLAR_ACCOUNT:-}"
admin="${ESCROW_ADMIN:-}"
token="${ESCROW_TOKEN:-}"
sweep_dest="${ESCROW_SWEEP_DEST:-}"
sweep_delay="${ESCROW_SWEEP_DELAY:-}"
output_file="${DEPLOYMENT_OUTPUT:-$DEFAULT_OUTPUT}"
replace=false

usage() {
    cat <<'EOF'
Usage: scripts/deploy-testnet.sh --network testnet --source-account IDENTITY \
    --admin ADDRESS --token ADDRESS --sweep-dest ADDRESS --sweep-delay SECONDS \
    [--output FILE] [--replace]

Required values may also be supplied through STELLAR_NETWORK, STELLAR_ACCOUNT,
ESCROW_ADMIN, ESCROW_TOKEN, ESCROW_SWEEP_DEST, and ESCROW_SWEEP_DELAY.

The command is intentionally testnet-only. --replace is required when the
output file already exists; it deploys a new instance and never overwrites an
existing on-chain contract.
EOF
}

fail() {
    printf 'deploy-testnet: %s\n' "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command '$1' is not installed"
}

while (($# > 0)); do
    case "$1" in
        --network)
            (($# >= 2)) || fail "--network requires a value"
            network="$2"
            shift 2
            ;;
        --source-account|--source)
            (($# >= 2)) || fail "$1 requires a value"
            source_account="$2"
            shift 2
            ;;
        --admin)
            (($# >= 2)) || fail "--admin requires a value"
            admin="$2"
            shift 2
            ;;
        --token)
            (($# >= 2)) || fail "--token requires a value"
            token="$2"
            shift 2
            ;;
        --sweep-dest)
            (($# >= 2)) || fail "--sweep-dest requires a value"
            sweep_dest="$2"
            shift 2
            ;;
        --sweep-delay)
            (($# >= 2)) || fail "--sweep-delay requires a value"
            sweep_delay="$2"
            shift 2
            ;;
        --output)
            (($# >= 2)) || fail "--output requires a value"
            output_file="$2"
            shift 2
            ;;
        --replace)
            replace=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument '$1' (use --help for usage)"
            ;;
    esac
done

[[ "$network" == "testnet" ]] || fail "--network testnet is required; refusing every other network"
[[ -n "$source_account" ]] || fail "a funded --source-account identity is required"
[[ -n "$admin" ]] || fail "--admin is required"
[[ -n "$token" ]] || fail "--token is required"
[[ -n "$sweep_dest" ]] || fail "--sweep-dest is required"
[[ "$sweep_delay" =~ ^[0-9]+$ ]] || fail "--sweep-delay must be a non-negative integer"

require_command stellar
require_command cargo
require_command curl
require_command grep
require_command mktemp

if [[ -e "$output_file" && "$replace" != true ]]; then
    fail "deployment record already exists at $output_file; pass --replace only when a new deployment is intentional"
fi

source_address="$(stellar keys public-key "$source_account" 2>/dev/null)" \
    || fail "identity '$source_account' is not configured in Stellar CLI"
[[ "$source_address" =~ ^G[A-Z2-7]{55}$ ]] \
    || fail "Stellar CLI did not return a valid public key for the source identity"

account_response=""
if ! account_response="$(curl --fail --silent --show-error --retry 2 \
    "https://horizon-testnet.stellar.org/accounts/$source_address" 2>/dev/null)"; then
    fail "source account is not funded on Stellar testnet, or testnet Horizon is unavailable"
fi
printf '%s' "$account_response" | grep -Fq '"id"' \
    || fail "source account could not be verified as funded on Stellar testnet"

build_dir="$(mktemp -d "${TMPDIR:-/tmp}/grainhack-deploy.XXXXXX")"
trap 'rm -rf "$build_dir"' EXIT

build_log=""
if ! build_log="$(cd "$REPO_ROOT" && stellar contract build --locked \
    --package "$CONTRACT_PACKAGE" --out-dir "$build_dir" 2>&1)"; then
    printf '%s\n' "$build_log" >&2
    fail "contract build failed"
fi

wasm_path="$(find "$build_dir" -type f -name '*.wasm' -print -quit)"
[[ -n "$wasm_path" ]] || fail "contract build produced no wasm file"
wasm_size="$(wc -c < "$wasm_path" | tr -d '[:space:]')"

upload_log=""
if ! upload_log="$(cd "$REPO_ROOT" && stellar contract upload \
    --wasm "$wasm_path" --source-account "$source_account" --network testnet 2>&1)"; then
    printf '%s\n' "$upload_log" >&2
    fail "wasm upload failed; verify the source identity is funded and configured for testnet"
fi
wasm_hash="$(printf '%s\n' "$upload_log" | grep -Eo '[0-9a-f]{64}' | tail -n 1 || true)"
[[ "$wasm_hash" =~ ^[0-9a-f]{64}$ ]] || fail "upload succeeded but did not return a wasm hash"

deploy_log=""
if ! deploy_log="$(cd "$REPO_ROOT" && stellar contract deploy \
    --wasm-hash "$wasm_hash" --source-account "$source_account" \
    --network testnet 2>&1)"; then
    printf '%s\n' "$deploy_log" >&2
    fail "contract deployment failed"
fi
contract_id="$(printf '%s\n' "$deploy_log" | grep -Eo 'C[A-Z2-7]{55}' | tail -n 1 || true)"
[[ "$contract_id" =~ ^C[A-Z2-7]{55}$ ]] || fail "deployment succeeded but did not return a contract ID"

invoke_log=""
if ! invoke_log="$(cd "$REPO_ROOT" && stellar contract invoke \
    --id "$contract_id" --source-account "$source_account" --network testnet \
    --send yes -- initialise --admin "$admin" --token "$token" \
    --sweep-dest "$sweep_dest" --sweep-delay "$sweep_delay" 2>&1)"; then
    printf '%s\n' "$invoke_log" >&2
    fail "contract deployment succeeded, but initialise failed"
fi

mkdir -p "$(dirname -- "$output_file")"
record_tmp="$(mktemp "${output_file}.tmp.XXXXXX")"
trap 'rm -rf "$build_dir"; rm -f "$record_tmp"' EXIT
umask 077
{
    printf '{\n'
    printf '  "network": "testnet",\n'
    printf '  "contract_id": "%s",\n' "$contract_id"
    printf '  "wasm_hash": "%s",\n' "$wasm_hash"
    printf '  "wasm_size_bytes": %s,\n' "$wasm_size"
    printf '  "admin": "%s",\n' "$admin"
    printf '  "token": "%s",\n' "$token"
    printf '  "sweep_dest": "%s",\n' "$sweep_dest"
    printf '  "sweep_delay": %s,\n' "$sweep_delay"
    printf '  "deployed_at_utc": "%s"\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf '}\n'
} > "$record_tmp"
mv -- "$record_tmp" "$output_file"

printf 'Testnet deployment complete.\n'
printf '  contract_id: %s\n' "$contract_id"
printf '  wasm_hash:   %s\n' "$wasm_hash"
printf '  wasm_size:   %s bytes\n' "$wasm_size"
printf '  record:      %s\n' "$output_file"
