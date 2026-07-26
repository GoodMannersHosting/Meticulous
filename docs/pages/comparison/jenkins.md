# Meticulous vs Jenkins

Jenkins is the granddaddy of self-hosted CI. It's extremely flexible, has a vast plugin ecosystem, and runs everywhere. That flexibility comes at a cost: operational overhead, security surface area, and a Groovy DSL that can be difficult to audit.

---

## At a glance

| Dimension | Jenkins | Meticulous |
|-----------|---------|------------|
| **Language** | Groovy (Declarative or Scripted pipeline) | YAML pipelines + versioned workflow catalog |
| **Plugin ecosystem** | 1,800+ plugins; any capability imaginable | Focused built-ins; external via workflow catalog |
| **Execution model** | Controller + agents (Java, inbound or outbound) | Control plane + agents (egress-only gRPC) |
| **Agent networking** | JNLP (outbound) or SSH (inbound) | Outbound gRPC only |
| **Pipeline-as-code security** | `Jenkinsfile` in repo; any Groovy runs on controller or agent | YAML DAG; steps are defined in catalog workflows, not arbitrary code |
| **Secrets** | Credentials plugin (env vars) | Per-job encrypted keypair; external store preferred |
| **Multi-tenancy** | Role Strategy plugin, Folders | Native RBAC with org/project/pipeline scopes |
| **Operational overhead** | High (plugin updates, Java tuning, HA setup) | Lower; single control plane with standard Kubernetes deployment |
| **UI** | Classic (aging); BlueOcean (unmaintained) | Modern SvelteKit UI |

---

## Pipeline definition

### Jenkins

```groovy
// Jenkinsfile
pipeline {
  agent any
  environment {
    REGISTRY = 'ghcr.io/my-org'
  }
  stages {
    stage('Build') {
      steps {
        sh 'docker build -t $REGISTRY/my-app .'
      }
    }
    stage('Push') {
      steps {
        withCredentials([usernamePassword(credentialsId: 'docker-creds', ...)]) {
          sh 'docker push $REGISTRY/my-app'
        }
      }
    }
  }
}
```

Jenkinsfiles are Groovy. Scripted pipelines (`node {}` blocks) can execute **arbitrary Groovy code on the Jenkins controller**, which is a significant security risk if pipeline authors are not fully trusted. Declarative pipelines are safer but still execute in the JVM.

### Meticulous

```yaml
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    version: v0.2
    inputs:
      image_tag: "${REGISTRY}/my-app"

  - name: Push
    id: push
    workflow: global/docker-push
    version: v0.2
    inputs:
      image_tag: "${REGISTRY}/my-app"
      docker_password: "${DOCKER_PASSWORD}"
    depends-on: [build]
```

Pipeline authors compose versioned workflows; they cannot inject arbitrary code. Workflow authors (platform admins) define what steps actually run.

---

## Security posture

### Jenkins controller risk

Jenkins has been the target of numerous CVEs. The controller runs on a long-lived JVM process, often with broad filesystem and network access. Plugins are third-party Java JARs — updating any plugin can introduce regressions or vulnerabilities.

The `Jenkinsfile` in-process execution model means a malicious or compromised pipeline file can exfiltrate the Jenkins controller's credentials store.

### Meticulous

- The control plane is stateless HTTP + gRPC processes; no JVM, no plugin JARs.
- Pipeline YAML is declarative; there is no general-purpose code execution in the control plane.
- Secrets are delivered per-job via asymmetric encryption; the control plane never holds decrypted secret values.
- Agent compromise is contained to the job's key material, which expires at job completion.

---

## Operational overhead

Jenkins requires:

- Java JVM sizing and tuning.
- Regular plugin updates (each with potential incompatibilities).
- HA setup (Jenkins controller HA is non-trivial).
- Log management, build artifact storage, and workspace cleanup.

Meticulous runs as standard containerized services (API, controller, NATS, optional SeaweedFS) on Kubernetes, managed with Helm. Upgrades are image tag bumps. There are no plugins to manage.
