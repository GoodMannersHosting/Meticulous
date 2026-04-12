#!/usr/bin/env bash
# Create a fresh join token on the target control plane and emit it as a met-output var.
# Required env: MET_E2E_API_TOKEN, MET_E2E_SERVER_URL, MET_E2E_ORG
set -euo pipefail

WS="${METICULOUS_WORKSPACE:?}"
SERVER="${MET_E2E_SERVER_URL:?MET_E2E_SERVER_URL is required}"
TOKEN="${MET_E2E_API_TOKEN:?MET_E2E_API_TOKEN is required}"

# Use the met CLI if available, otherwise use curl
MET_BIN="${WS}/meticulous/target/release/met"
if ! [ -x "${MET_BIN}" ]; then
  MET_BIN="${WS}/meticulous/target/debug/met"
fi
if ! [ -x "${MET_BIN}" ]; then
  MET_BIN="$(command -v met 2>/dev/null || true)"
fi

if [ -z "${MET_BIN}" ] || ! [ -x "${MET_BIN}" ]; then
  # Fall back to curl directly against the admin API
  echo "met binary not found, using curl..." >&2
  RESP=$(curl -sf -X POST "${SERVER}/api/v1/agents/join-tokens" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"name":"e2e-test-token","expires_in_hours":2}')
  JOIN_TOKEN=$(echo "${RESP}" | python3 -c "import json,sys; print(json.load(sys.stdin)['token'])")
  TOKEN_ID=$(echo "${RESP}" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
else
  RESP=$("${MET_BIN}" \
    --server "${SERVER}" \
    --token "${TOKEN}" \
    --format json \
    agents join-tokens create \
      --name "e2e-test-token" \
      --expires-in 2)
  JOIN_TOKEN=$(echo "${RESP}" | python3 -c "import json,sys; print(json.load(sys.stdin)['token'])")
  TOKEN_ID=$(echo "${RESP}" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
fi

echo "Join token created (id: ${TOKEN_ID})"
echo "${TOKEN_ID}" > "${WS}/.e2e-join-token-id"

met-output var MET_E2E_JOIN_TOKEN="${JOIN_TOKEN}" 2>/dev/null || echo "MET_E2E_JOIN_TOKEN=${JOIN_TOKEN}"
met-output var MET_E2E_JOIN_TOKEN_ID="${TOKEN_ID}" 2>/dev/null || echo "MET_E2E_JOIN_TOKEN_ID=${TOKEN_ID}"
