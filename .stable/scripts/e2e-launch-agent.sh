#!/usr/bin/env bash
# Pull and launch the met-agent container for E2E testing.
# Required env: MET_E2E_JOIN_TOKEN, MET_E2E_SERVER_URL, AGENT_IMAGE, REGISTRY_HOST,
#               HARBOR_USERNAME, HARBOR_PASSWORD
set -euo pipefail

WS="${METICULOUS_WORKSPACE:?}"
JOIN_TOKEN="${MET_E2E_JOIN_TOKEN:?MET_E2E_JOIN_TOKEN is required}"
SERVER="${MET_E2E_SERVER_URL:?MET_E2E_SERVER_URL is required}"
IMAGE="${AGENT_IMAGE:?AGENT_IMAGE is required}"
REGISTRY="${REGISTRY_HOST:?REGISTRY_HOST is required}"

# Detect container engine
{
  echo 'if [ -z "${DOCKER_HOST-}" ]; then'
  echo '  for sock in /var/run/meticulous-dind/docker.sock /var/run/docker.sock; do'
  echo '    [ -S "${sock}" ] && export DOCKER_HOST="unix://${sock}" && break'
  echo '  done'
  echo 'fi'
} > "${WS}/.meticulous_probe_docker_host.sh"
# shellcheck disable=SC1091
. "${WS}/.meticulous_probe_docker_host.sh"

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  ENG="docker"
elif command -v podman >/dev/null 2>&1 && podman info >/dev/null 2>&1; then
  ENG="podman"
else
  echo "No container engine found" >&2; exit 1
fi

# Login to registry
echo "${HARBOR_PASSWORD:?}" | "${ENG}" login "${REGISTRY}" \
  --username "${HARBOR_USERNAME:?}" --password-stdin

# Pull the agent image
"${ENG}" pull "${IMAGE}"

# Launch agent container
CONTAINER_NAME="meticulous-e2e-agent-$$"
echo "${CONTAINER_NAME}" > "${WS}/.e2e-agent-container"

"${ENG}" run -d \
  --name "${CONTAINER_NAME}" \
  --network host \
  -e "MET_JOIN_TOKEN=${JOIN_TOKEN}" \
  -e "MET_CONTROLLER_URL=${SERVER}" \
  "${IMAGE}"

echo "Agent container ${CONTAINER_NAME} started"

# Wait for the agent to register (poll the API)
SERVER_API="${SERVER}"
API_TOKEN="${MET_E2E_API_TOKEN:?}"

echo "Waiting for agent to appear in the control plane..."
for i in $(seq 1 30); do
  COUNT=$(curl -sf "${SERVER_API}/api/v1/agents" \
    -H "Authorization: Bearer ${API_TOKEN}" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print(len([a for a in d.get('data',[]) if a.get('status')=='online']))" 2>/dev/null || echo 0)
  if [ "${COUNT}" -gt 0 ]; then
    echo "Agent online after ${i}s (${COUNT} online agents)"
    break
  fi
  sleep 2
done

COUNT=$(curl -sf "${SERVER_API}/api/v1/agents" \
  -H "Authorization: Bearer ${API_TOKEN}" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print(len([a for a in d.get('data',[]) if a.get('status')=='online']))" 2>/dev/null || echo 0)
if [ "${COUNT}" -eq 0 ]; then
  echo "No online agents found after 60s. Container logs:" >&2
  "${ENG}" logs "${CONTAINER_NAME}" >&2 || true
  exit 1
fi
