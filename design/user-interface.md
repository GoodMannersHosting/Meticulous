# UI Features/Functions

- Group and User Management
- API Token Creation and management (per-user, per group, and administratively for the platform)
- Build Logs in Browser
- Easy view to see diff from previous runs in log output
- CRUD variables for runs in browser
- Grouping of Job/Workflow runs
- Run scheduling (i.e during release window)
- DAG viewer of workflows/pipelines/jobs dependencies
- SBOM change/diff viewer
- "tool" search (i.e which tools were used, which versions) to view/track/trend
- use of tool database and tracking to generate "blast radius" (i.e specific tool SHA was compromised, how many workflows used it, Who/What/Where/When/Why/How)
- Highlighting of "Flakey" steps in reusable workflows (i.e "Step 6 matching ~these~ parameters in a 9-step workflow fails intermittently more often than not" - not the why, just that it _is_ more flakey)
- User Profiles
- One-Click GitHub Webhook provisioning using GitHub App Auth (global/project scoped)
