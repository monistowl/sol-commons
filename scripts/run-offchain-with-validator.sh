#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

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
}
trap validator_cleanup EXIT

solana-test-validator --reset --quiet >"$LOG_FILE" 2>&1 &
STV_PID=$!

sleep 2

(
  cd "$REPO_ROOT/sol-commons-workspace"
  ANCHOR_PROVIDER_URL="http://127.0.0.1:8899" yarn test:offchain
)
