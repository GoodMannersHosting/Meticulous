# GitHub Webhook Setup for Meticulous CI

This document explains how to wire a GitHub repository push/pull_request webhook to the
`meticulous-ci` pipeline so that every push triggers the full CI run automatically.

## Prerequisites

- A working Meticulous instance with the `meticulous-ci` pipeline imported and enabled.
- The `meticulous-ci` pipeline's **trigger ID** (obtained after importing the pipeline — visible
  in the pipeline detail page or via `met pipelines get`).
- The organization ID of your Meticulous org.
- A webhook secret (choose a random string; store it in Meticulous as a pipeline secret
  if you want the API to verify the HMAC signature).

## Step 1: Get the Trigger ID

After importing `meticulous-ci.pipeline.yaml` into Meticulous:

```bash
met --server https://meticulous.cloud.danmanners.com \
    --token "${MET_API_TOKEN}" \
    pipelines get --slug meticulous-ci --format json \
  | python3 -c "import json,sys; p=json.load(sys.stdin); print(p['id'])"
```

Get the webhook trigger URL from the pipeline triggers list:

```bash
# List triggers for the pipeline
curl -sf https://meticulous.cloud.danmanners.com/api/v1/pipelines/{PIPELINE_ID}/triggers \
  -H "Authorization: Bearer ${MET_API_TOKEN}" | python3 -m json.tool
```

The webhook endpoint has the form:
```
https://meticulous.cloud.danmanners.com/webhooks/{ORG_ID}/{TRIGGER_ID}
```
For GitHub specifically:
```
https://meticulous.cloud.danmanners.com/webhooks/github/{ORG_ID}/{TRIGGER_ID}
```

## Step 2: Register the Webhook on GitHub

### Via the GitHub UI

1. Go to `https://github.com/GoodMannersHosting/Meticulous/settings/hooks/new`
2. **Payload URL**: `https://meticulous.cloud.danmanners.com/webhooks/github/{ORG_ID}/{TRIGGER_ID}`
3. **Content type**: `application/json`
4. **Secret**: your webhook HMAC secret (store the same value in Meticulous under
   `Settings → Webhooks` for the pipeline trigger)
5. **Events**: select **"Let me select individual events"**, then check:
   - Pushes
   - Pull requests
6. Click **Add webhook**

### Via the GitHub CLI

```bash
gh api repos/GoodMannersHosting/Meticulous/hooks \
  --method POST \
  --field "name=web" \
  --field "active=true" \
  --field "config[url]=https://meticulous.cloud.danmanners.com/webhooks/github/${ORG_ID}/${TRIGGER_ID}" \
  --field "config[content_type]=json" \
  --field "config[secret]=${WEBHOOK_SECRET}" \
  --field "events[]=push" \
  --field "events[]=pull_request"
```

### Via the Meticulous API (automated SCM setup)

Meticulous has a built-in SCM setup endpoint that registers the webhook on GitHub automatically
using a stored `GITHUB_TOKEN` secret:

```bash
curl -sf -X POST \
  "https://meticulous.cloud.danmanners.com/api/v1/projects/{PROJECT_ID}/scm/setup" \
  -H "Authorization: Bearer ${MET_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "github",
    "repository": "GoodMannersHosting/Meticulous",
    "github_token_secret": "meticulous-ci"
  }' | python3 -m json.tool
```

## Step 3: Verify

Push a test commit to a branch and confirm:

1. The GitHub webhook shows a green ✓ delivery in `Settings → Webhooks → {your hook} → Recent Deliveries`
2. A new run appears in the `meticulous-ci` pipeline in the Meticulous UI

## Pipeline Secrets Required Before First Run

Ensure these stored secrets exist in Meticulous before the first webhook fires:

| Secret name | Description |
|---|---|
| `meticulous-ci` | GitHub PAT with `repo` scope (checkout + SCM setup) |
| `harbor-mar-operator-password` | Harbor robot account password |
| `harbor-mar-operator-username` | Harbor robot account username |
| `kubeconfig-meticulous` | Base64-encoded kubeconfig for deploy step |
| `ci-bootstrap-password` | Password for the CI bootstrap admin user |

Store each secret:

```bash
met --server https://meticulous.cloud.danmanners.com \
    --token "${MET_API_TOKEN}" \
    secrets set meticulous-ci --value "${GITHUB_PAT}"
```
