Possibly use Tekton CI as a backend for execution????

Layout/Design:
    Project Owner could be user/group
    Project Name
    Secrets for a Project vs Secrets for global (or lower level scoping)
    Description of a project

Secrets Management - support built-in secrets but disuade users from doing it?

Layout: Project > Pipelines > Jobs > Steps
- where do reusable pipelines exist? GLobal? Project?
- How and what do we want to reference as an upstream SCM repo?

Can we have a *REASONABLY SECURE* Debug CLI where developers could have a good user experience WITHOUT the ability to easily exfiltrate secrets via CLI?

Capture a list and sha of all binaries executed in a pipeline/workflow?
Capture all network src/dst IPs (metadata only, not the network traffic itself) during pipeline? Want to know where our traffic is going.

Scan pipeline for recommendations? (AI Integration???)
- Assess pipelines for recommendations to do things like adding binary/object scanning *BEFORE* pushing objects to container registries, package managers, etc.

How can we limit the capabilities of a bad actor to exploit memory vulnerability to do core dumps and capture secrets in plaintext?

What "Write-Once" filesystems are there for persistent build tool as well as short-term log aggregation to minimize the risk of bad actors deleting or changing logs?

Performance for short-term log volume caching?
Same volume for artifact attestation volume?
