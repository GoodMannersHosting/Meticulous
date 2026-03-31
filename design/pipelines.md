# General Notes

- Secrets must be checked before pipeline run to make sure they've been loaded in; don't run if the secrets don't exist
- Secret fields MUST ALWAYS be hashed (***) when printing to stdout or logs
- Secret fields which have been base64 encoded MUST ALWAYS be hashed (***) when printing to stdout or logs

## Basic Docker Build / Push Workflow

1. `git clone https://${scm_url}/${path}.git --depth 1 `
2. `echo "${registry_password}" | docker login --username ${registry_username} --password-stdin ${registry_url}`
3. `docker buildx build --sbom=true --provenance=max,version=v1 ${build_params} -t ${container_tag} ${container_file:=.}`
  3a. <https://docs.docker.com/build/metadata/attestations/>
4. `docker push ${container_tag}`

## Example Yaml

```yaml
# .stable/workflow-1.yaml
name: workflow demo
triggers:
  manual: {}
  release:
    tag:
      - "semver bump"
      - "v*"
runs-on:
  tags:
    - amd64: true
    - gpu: false
secrets:
  DOCKER_USERNAME:
    aws:
      arn: arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/docker_username-AbCdEf
  DOCKER_PASSWORD:
    aws:
      arn: arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/docker_password-gHiJkL
vars:
  GIT_REPO: https://github.com/example-org/smol-repo.git
  REGISTRY: ghcr.io/example-org
  CONTAINER: project
workflows:
  # Docker Build Reusable Workflow
  - name: Docker Build
    id: dbap
    workflow: global/docker-build
    version: v0.1
    inputs:
      image_tag: "${REGISTRY}/${CONTAINER}"
      repo: $GIT_REPO
    depends-on: []
  # Docker Push Reusable Workflow
  - name: Docker Push
    id: dp
    workflow: global/docker-push
    version: v0.2
    inputs:
      docker_password: "${DOCKER_PASSWORD}"
      docker_username: "${DOCKER_USERNAME}"
      image_tag: "${REGISTRY}/${CONTAINER}"
    depends-on: [dbap]
```
