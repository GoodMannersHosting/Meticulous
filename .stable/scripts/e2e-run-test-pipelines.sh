#!/usr/bin/env bash
# Trigger several test pipelines via the API and assert they complete successfully.
# Uses the Meticulous self-build pipelines from .stable/ as the test subjects — lightweight
# "echo" variants are preferred so the E2E test doesn't rebuild everything.
#
# Required env: MET_E2E_API_TOKEN, MET_E2E_SERVER_URL, MET_E2E_ORG
# Optional:     RUN_TIMEOUT_SECS (default: 120)
set -euo pipefail

WS="${METICULOUS_WORKSPACE:?}"
SERVER="${MET_E2E_SERVER_URL:?}"
API_TOKEN="${MET_E2E_API_TOKEN:?}"
ORG="${MET_E2E_ORG:-meticulous-ci}"
TIMEOUT="${RUN_TIMEOUT_SECS:-120}"

# Helper: trigger a pipeline and return the run ID
trigger_pipeline() {
  local PIPELINE_ID="$1"
  curl -sf -X POST "${SERVER}/api/v1/pipelines/${PIPELINE_ID}/trigger" \
    -H "Authorization: Bearer ${API_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{}' \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])"
}

# Helper: poll run status until terminal state or timeout
wait_for_run() {
  local RUN_ID="$1"
  local TIMEOUT_SECS="$2"
  for i in $(seq 1 "${TIMEOUT_SECS}"); do
    STATUS=$(curl -sf "${SERVER}/api/v1/runs/${RUN_ID}" \
      -H "Authorization: Bearer ${API_TOKEN}" \
      | python3 -c "import json,sys; print(json.load(sys.stdin).get('status','unknown'))")
    case "${STATUS}" in
      success|completed)
        echo "Run ${RUN_ID} completed: ${STATUS} (${i}s)"
        return 0
        ;;
      failed|error|cancelled)
        echo "Run ${RUN_ID} ended with: ${STATUS} (${i}s)" >&2
        return 1
        ;;
    esac
    sleep 2
  done
  echo "Run ${RUN_ID} timed out after ${TIMEOUT_SECS}s (last status: ${STATUS})" >&2
  return 1
}

FAILED=0

# Discover pipelines in the e2e test project (named "e2e-*" by convention, or all pipelines if none)
echo "==> Listing pipelines to test..."
PIPELINES=$(curl -sf "${SERVER}/api/v1/pipelines" \
  -H "Authorization: Bearer ${API_TOKEN}" \
  | python3 -c "
import json, sys
d = json.load(sys.stdin)
# Prefer pipelines tagged for e2e; fall back to any enabled pipeline
pipelines = [p for p in d.get('data', []) if p.get('enabled', True)]
e2e = [p for p in pipelines if 'e2e' in p.get('name','').lower() or 'e2e' in p.get('slug','').lower()]
use = e2e if e2e else pipelines[:3]  # at most 3 pipelines
for p in use:
    print(p['id'])
" 2>/dev/null || true)

if [ -z "${PIPELINES}" ]; then
  echo "No pipelines found to test — skipping run assertions" >&2
  exit 0
fi

echo "Triggering pipelines: $(echo "${PIPELINES}" | tr '\n' ' ')"
RUN_IDS=()
for PID in ${PIPELINES}; do
  echo "==> Triggering pipeline ${PID}"
  RUN_ID=$(trigger_pipeline "${PID}" 2>/dev/null || true)
  if [ -n "${RUN_ID}" ]; then
    echo "    run id: ${RUN_ID}"
    RUN_IDS+=("${RUN_ID}")
  else
    echo "    WARNING: failed to trigger pipeline ${PID}" >&2
  fi
done

echo "==> Waiting for ${#RUN_IDS[@]} run(s) to complete (timeout: ${TIMEOUT}s each)..."
for RUN_ID in "${RUN_IDS[@]}"; do
  if ! wait_for_run "${RUN_ID}" "${TIMEOUT}"; then
    FAILED=$((FAILED + 1))
  fi
done

if [ "${FAILED}" -gt 0 ]; then
  echo "${FAILED} run(s) failed or timed out." >&2
  exit 1
fi

echo "All E2E test runs completed successfully."
