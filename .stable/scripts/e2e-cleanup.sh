#!/usr/bin/env bash
# Revoke the E2E join token and stop the agent container.
# Always exits 0 — cleanup failures should not fail the pipeline.
set -uo pipefail

WS="${METICULOUS_WORKSPACE:?}"
SERVER="${MET_E2E_SERVER_URL:-}"
API_TOKEN="${MET_E2E_API_TOKEN:-}"

# Stop agent container
CONTAINER_NAME="$(cat "${WS}/.e2e-agent-container" 2>/dev/null || true)"
if [ -n "${CONTAINER_NAME}" ]; then
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    ENG="docker"
  elif command -v podman >/dev/null 2>&1; then
    ENG="podman"
  else
    ENG=""
  fi
  if [ -n "${ENG}" ]; then
    echo "Stopping agent container ${CONTAINER_NAME}..."
    "${ENG}" stop "${CONTAINER_NAME}" 2>/dev/null || true
    "${ENG}" rm -f "${CONTAINER_NAME}" 2>/dev/null || true
  fi
fi

# Revoke join token
TOKEN_ID="$(cat "${WS}/.e2e-join-token-id" 2>/dev/null || true)"
if [ -n "${TOKEN_ID}" ] && [ -n "${SERVER}" ] && [ -n "${API_TOKEN}" ]; then
  echo "Revoking join token ${TOKEN_ID}..."
  curl -sf -X DELETE "${SERVER}/api/v1/agents/join-tokens/${TOKEN_ID}" \
    -H "Authorization: Bearer ${API_TOKEN}" >/dev/null 2>&1 || \
  curl -sf -X POST "${SERVER}/api/v1/agents/join-tokens/${TOKEN_ID}/revoke" \
    -H "Authorization: Bearer ${API_TOKEN}" >/dev/null 2>&1 || \
    echo "WARNING: could not revoke join token ${TOKEN_ID}" >&2
fi

echo "E2E cleanup complete."
exit 0
