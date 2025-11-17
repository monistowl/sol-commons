#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PIPELINE_PAYLOAD=""
EVENTS_FILE=""

if ! command -v solana-test-validator >/dev/null 2>&1; then
  echo "solana-test-validator required but not found. Install via the Solana installer." >&2
  exit 1
fi

LOG_FILE="$(mktemp -t stv-log.XXXXXX)"

validator_cleanup() {
  if [[ -n "${STV_PID:-}" ]]; then
    kill "$STV_PID" >/dev/null 2>&1 || true
    wait "$STV_PID" >/dev/null 2>&1 || true
  fi
  rm -f "$LOG_FILE"
  [[ -n "$PIPELINE_PAYLOAD" ]] && rm -f "$PIPELINE_PAYLOAD"
  [[ -n "$EVENTS_FILE" ]] && rm -f "$EVENTS_FILE"
}
trap validator_cleanup EXIT

solana-test-validator --reset --quiet >"$LOG_FILE" 2>&1 &
STV_PID=$!

sleep 2

PIPELINE_PAYLOAD="$(mktemp -p "$REPO_ROOT" offchain-payload.XXXXXX.json)"
EVENTS_FILE="$(mktemp -p "$REPO_ROOT" praise-events.XXXXXX.json)"
readarray -t KP_INFO < <(
  cd "$REPO_ROOT/sol-commons-workspace"
  node - <<'NODE'
const { Keypair } = require('@solana/web3.js');
const kp = Keypair.generate();
console.log(JSON.stringify(Array.from(kp.secretKey)));
console.log(kp.publicKey.toBase58());
NODE
)
VALIDATOR_SECRET="${KP_INFO[0]}"
VALIDATOR_PUBKEY="${KP_INFO[1]}"
cat <<PAYLOAD >"$EVENTS_FILE"
[
  {
    "address": "$VALIDATOR_PUBKEY",
    "amount": 1234,
    "event": "validator-claimer"
  }
]
PAYLOAD

SOL_COMMONS_PIPELINE_SILENT=1 node "$REPO_ROOT/offchain/pipeline/index.js" --praise-events-file "$EVENTS_FILE" >"$PIPELINE_PAYLOAD"

(
  cd "$REPO_ROOT/sol-commons-workspace"
  OFFCHAIN_PIPELINE_PAYLOAD="$PIPELINE_PAYLOAD" OFFCHAIN_VALIDATOR_SECRET="$VALIDATOR_SECRET" ANCHOR_PROVIDER_URL="http://127.0.0.1:8899" yarn test:offchain
)
