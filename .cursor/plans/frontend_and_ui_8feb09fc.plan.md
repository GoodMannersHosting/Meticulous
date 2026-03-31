---
name: Frontend and UI
overview: "Detailed plan for the Meticulous frontend: SvelteKit + TypeScript app covering the design system, application shell, project/pipeline/run pages, live log streaming, DAG visualization, SBOM diff viewer, blast radius explorer, agent monitoring, user/group management, and release management UI."
todos:
  - id: fe-scaffold
    content: Scaffold SvelteKit + TypeScript project with Tailwind CSS 4, Bits UI, Superforms, Svelte Flow, and env config
    status: pending
  - id: fe-design-system
    content: "Build design system: UI primitives (Button, Input, Dialog, Select, Badge, Tooltip, Toast) on Bits UI, plus layout (Shell, Sidebar, TopBar, Breadcrumbs) and data components (DataTable, Pagination, EmptyState, Skeleton)"
    status: pending
  - id: fe-api-client
    content: "Implement API client layer: fetch wrapper with auth headers, OIDC login/callback/refresh flow, WebSocket manager with reconnection/heartbeat"
    status: pending
  - id: fe-auth-pages
    content: Build auth pages (login, OIDC callback) and route guards via hooks.server.ts and root +layout.ts
    status: pending
  - id: fe-dashboard
    content: "Build dashboard page: run heatmap, active runs, recent failures, agent health cards, flaky step highlights, quick actions"
    status: pending
  - id: fe-projects-pipelines
    content: Build project list/detail and pipeline list/detail pages with CRUD, tabs, DAG preview, sparkline, trigger dialog, Monaco YAML viewer
    status: pending
  - id: fe-secrets-variables
    content: Build secrets and variables management pages with provider config, external secrets warning, inline editing, blast radius check on delete
    status: pending
  - id: fe-dag-viewer
    content: "Build DAG viewer component: Svelte Flow (@xyflow/svelte) + dagre layout, custom nodes with status variants, animated edges, minimap, context menu, critical path highlighting"
    status: pending
  - id: fe-log-viewer
    content: "Build log viewer component: virtual scrolling (@tanstack/svelte-virtual), ANSI parser, line numbers/selection, search, follow mode, section folding, log diff overlay, download"
    status: pending
  - id: fe-run-pages
    content: "Build run list and run detail pages: split-pane layout (DAG + job panel), WebSocket integration via Svelte stores for live updates, step list, artifacts, environment tab"
    status: pending
  - id: fe-sbom
    content: Build SBOM browser (list, tree view, search, download) and SBOM diff viewer (side-by-side/unified, change annotations, Monaco raw diff)
    status: pending
  - id: fe-blast-radius-tools
    content: "Build blast radius explorer and tool database pages: tool lookup, affected entity listing, timeline, version history, usage trends (LayerChart)"
    status: pending
  - id: fe-agents
    content: "Build agent pools page: pool list/detail, agent detail with metadata/job history, admin actions (revoke, drain, join tokens)"
    status: pending
  - id: fe-admin
    content: "Build admin pages: user/group management, API token CRUD, webhook management with GitHub App provisioning, notification channel config"
    status: pending
  - id: fe-releases
    content: "Build release management pages: list/detail, scheduling calendar, comms generator, rollback trigger"
    status: pending
  - id: fe-polish
    content: "Polish: accessibility audit, performance profiling, +error.svelte pages, responsive design (1024px+), E2E tests (Playwright) for critical flows"
    status: pending
isProject: false
---

# Meticulous -- Frontend and UI Plan

**Phase 5 of the Meticulous build order.** This plan depends on Phase 4 (API and CLI) being substantially complete -- the frontend consumes the REST and WebSocket endpoints defined there. It also references domain models from `met-core` (Phase 0) for shared type alignment.

---

## 1. Overview

The frontend is a SvelteKit + TypeScript application. It gives developers and security engineers end-to-end visibility from `git push` to production release. The UI surfaces the platform's security-first nature (SBOM browsing, blast radius, tool tracking) alongside everyday CI/CD workflows (pipeline runs, live logs, DAG views).

SvelteKit provides file-based routing, server-side rendering, streaming, form actions, and automatic code splitting out of the box -- eliminating the need for separate routing, state management, and code-splitting libraries.

Reference: [design/plans/06-frontend-and-ui.md](design/plans/06-frontend-and-ui.md) (detailed design document), [design/notes/user-interface.md](design/notes/user-interface.md) (original feature notes).

---

## 2. Tech Stack

- **Framework**: SvelteKit (Svelte 5 with runes) + TypeScript
- **Routing**: SvelteKit file-based routing (built-in, type-safe via generated `$types`)
- **Data Loading**: SvelteKit `load` functions (SSR/CSR); optionally `@tanstack/svelte-query` for complex cache/mutation patterns
- **Client State**: Svelte stores (`writable`/`readable`/`derived` -- built-in, no library needed)
- **Styling**: Tailwind CSS 4 + Bits UI (accessible unstyled Svelte components for dialogs, dropdowns, tooltips, selects)
- **DAG Rendering**: Svelte Flow (`@xyflow/svelte`) + dagre layout
- **Log Viewer**: Custom virtualized component (`@tanstack/svelte-virtual`) with ANSI color parser
- **SBOM/Diff**: Custom dependency tree view + Monaco diff editor for raw JSON comparison
- **Charts**: LayerChart (Svelte-native, D3-based, composable)
- **Forms**: Superforms + Zod (SvelteKit-native form handling with progressive enhancement)
- **Icons**: Lucide Svelte (`lucide-svelte`)
- **Date/Time**: date-fns
- **WebSocket**: Native WebSocket with reconnecting wrapper
- **Testing**: Vitest + @testing-library/svelte (unit/integration), Playwright (E2E)

---

## 3. Project Structure

SvelteKit uses `src/routes/` for file-based routing and `src/lib/` for shared code (importable as `$lib`).

```
frontend/
├── package.json, tsconfig.json, svelte.config.js, vite.config.ts
├── tailwind.config.ts
├── .env.example                          # PUBLIC_API_BASE_URL, PUBLIC_WS_BASE_URL
├── static/
│   └── favicon.svg
├── src/
│   ├── app.html                          # HTML shell template
│   ├── app.css                           # Tailwind directives, CSS custom properties, design tokens
│   ├── hooks.server.ts                   # Server hooks: auth token validation, request enrichment
│   ├── hooks.client.ts                   # Client hooks: global error handling
│   ├── lib/
│   │   ├── api/
│   │   │   ├── client.ts                 # Fetch wrapper (base URL, auth headers, error handling)
│   │   │   ├── ws.ts                     # WebSocket manager (reconnection, heartbeat, multiplexing)
│   │   │   └── types.ts                  # Shared API response/request TypeScript types
│   │   ├── components/
│   │   │   ├── ui/                       # Design system primitives (Button, Input, Dialog, Select, Badge, Tooltip, Toast) on Bits UI
│   │   │   ├── layout/                   # Shell.svelte, Sidebar.svelte, TopBar.svelte, Breadcrumbs.svelte
│   │   │   ├── data/                     # DataTable.svelte, Pagination.svelte, FilterBar.svelte, EmptyState.svelte, Skeleton.svelte
│   │   │   ├── feedback/                 # Toast.svelte, AlertBanner.svelte, Spinner.svelte
│   │   │   └── domain/
│   │   │       ├── log-viewer/           # LogViewer.svelte, LogLine.svelte, LogToolbar.svelte, LogSearch.svelte
│   │   │       ├── dag-viewer/           # DagViewer.svelte, DagNode.svelte, DagEdge.svelte
│   │   │       ├── sbom-diff/            # SbomDiff.svelte, DependencyTree.svelte
│   │   │       ├── run-status/           # StatusBadge.svelte, TimingBar.svelte
│   │   │       └── secret-field/         # SecretField.svelte
│   │   ├── stores/
│   │   │   ├── theme.ts                  # writable store, persisted to localStorage
│   │   │   ├── sidebar.ts               # writable store (collapsed state)
│   │   │   ├── log-viewer.ts            # writable store (filters, search, follow, wrap)
│   │   │   └── auth.ts                  # writable store (JWT, user info, OIDC state)
│   │   ├── utils/
│   │   │   ├── ansi.ts                  # ANSI escape code parser
│   │   │   ├── dag-layout.ts            # dagre layout helpers
│   │   │   ├── format.ts               # Date, duration, byte formatting
│   │   │   ├── permissions.ts           # RBAC helper utilities
│   │   │   └── search.ts               # Client-side log/text search
│   │   └── index.ts                     # Barrel exports for $lib
│   └── routes/
│       ├── +layout.svelte               # Root layout: Shell (sidebar + topbar + <slot>), theme init, global WS
│       ├── +layout.ts                   # Root load: auth check, user data
│       ├── +error.svelte                # Global error fallback page
│       ├── auth/
│       │   ├── login/+page.svelte
│       │   └── callback/+page.ts        # OIDC callback (load function exchanges code for tokens)
│       ├── dashboard/
│       │   ├── +page.svelte
│       │   └── +page.ts                 # load: GET /api/v1/dashboard/summary
│       ├── projects/
│       │   ├── +page.svelte             # Project list
│       │   ├── +page.ts
│       │   └── [projectId]/
│       │       ├── +layout.svelte       # Project detail layout (tabs)
│       │       ├── +layout.ts           # load: project data
│       │       ├── +page.svelte         # Overview tab
│       │       ├── pipelines/
│       │       │   ├── +page.svelte     # Pipeline list for project
│       │       │   └── [pipelineId]/
│       │       │       ├── +page.svelte # Pipeline detail (DAG preview, sparkline, trigger)
│       │       │       ├── +page.ts
│       │       │       └── editor/+page.svelte
│       │       ├── runs/+page.svelte
│       │       ├── secrets/+page.svelte
│       │       └── settings/+page.svelte
│       ├── runs/
│       │   └── [runId]/
│       │       ├── +page.svelte         # Run detail (DAG progress + job panel, WebSocket)
│       │       ├── +page.ts
│       │       └── jobs/[jobId]/+page.svelte
│       ├── agents/
│       │   ├── +page.svelte             # Pool list
│       │   └── [agentId]/+page.svelte
│       ├── security/
│       │   ├── sboms/
│       │   │   ├── +page.svelte         # SBOM browser
│       │   │   └── diff/+page.svelte    # SBOM diff viewer
│       │   ├── blast-radius/+page.svelte
│       │   └── tools/
│       │       ├── +page.svelte         # Tool database
│       │       └── [toolId]/+page.svelte
│       ├── releases/
│       │   ├── +page.svelte
│       │   ├── [releaseId]/+page.svelte
│       │   └── schedule/+page.svelte
│       └── settings/
│           ├── +layout.svelte           # Settings layout (settings sidebar nav)
│           ├── users/+page.svelte
│           ├── groups/+page.svelte
│           ├── tokens/+page.svelte
│           ├── secrets/+page.svelte     # Global secrets
│           ├── variables/+page.svelte   # Global variables
│           ├── webhooks/+page.svelte
│           └── profile/+page.svelte
├── tests/
│   ├── unit/                             # Vitest + @testing-library/svelte
│   ├── integration/
│   └── e2e/                              # Playwright
```

---

## 4. Application Shell and Auth

**Layout**: Fixed collapsible sidebar + top bar + scrollable content area. Implemented as the root `+layout.svelte`.

Sidebar navigation:

- Dashboard
- Projects > [Project] > Pipelines / Runs / Secrets and Variables / Settings
- Agents
- Security > SBOMs / Blast Radius / Tool Database
- Releases
- Settings (org-level) > Users and Groups / API Tokens / Webhooks / Global Workflows

Top bar: Cmd+K global search, notification bell, user avatar menu, theme toggle. Breadcrumbs reflect current hierarchy.

**Auth**: OIDC with PKCE. JWT stored in memory (Svelte `writable` store in `$lib/stores/auth.ts`), refresh token via HttpOnly cookie. Auth guard implemented in `hooks.server.ts` (validates token on every server request) and the root `+layout.ts` `load` function (redirects unauthenticated users to `/auth/login`). API client attaches `Authorization: Bearer <token>` to all requests. On auth state change, SvelteKit's `invalidateAll()` re-runs all load functions.

---

## 5. Core Pages

### 5.1 Dashboard

Aggregated org-level view. Data loaded via `+page.ts` calling `GET /api/v1/dashboard/summary`:

- Run activity heatmap (GitHub-contribution-style grid)
- Active runs (live-updating list with progress indicators via global WebSocket store)
- Recent failures (last N, one-click to failed job)
- Agent pool health cards (online/offline/busy per pool)
- Top N flaky reusable workflow steps
- Quick actions (trigger pipeline, create project, view SBOMs)

### 5.2 Projects and Pipelines

- **Project list**: Filterable table (name, owner, pipeline count, last run status). Card/table view toggle. Data via `projects/+page.ts` load function.
- **Project detail**: Nested layout (`projects/[projectId]/+layout.svelte`) with tabs -- Overview (description, SCM), Pipelines, Runs, Secrets/Variables, Settings. Shared project data loaded once in `+layout.ts`.
- **Pipeline detail**: Header with trigger config, static DAG preview (Svelte Flow), recent runs sparkline (LayerChart), "Run now" button with variable overrides dialog (Superforms), YAML source (read-only Monaco).
- **Pipeline editor** (stretch): Monaco with schema-aware autocomplete for `.stable/*.yaml`.

### 5.3 Secrets and Variables

- **Secrets**: List with name, provider (built-in / Vault / AWS SM / K8s), scope, last rotated. Built-in secrets show a warning banner encouraging external providers. Create/edit with provider-specific config via Superforms + form actions. Values never displayed. Delete confirmation with "used in N pipelines" blast radius check.
- **Variables**: Editable inline table. Bulk edit mode. Variable groups.

---

## 6. Run Visualization and Logs

### 6.1 Run Detail Page

The operational heart of the UI. Split-pane layout: DAG at top, job detail panel at bottom (resizable).

**Live updates** via WebSocket (`/api/v1/runs/{id}/stream`). The page's `onMount` opens the WebSocket and writes events to a Svelte `writable` store. All child components (`DagViewer`, job panel, `LogViewer`) subscribe reactively to this store. Server pushes typed events:

- `job_status`, `step_status`, `log_line`, `run_complete`, `artifact_ready`

Initial run data loaded via `+page.ts` load function. WebSocket updates merge into the same reactive data.

### 6.2 DAG Viewer

Built on **Svelte Flow** (`@xyflow/svelte`). Two modes:

1. **Static** (pipeline definition): shows declared job/workflow dependencies
2. **Live** (run progress): real-time status per node during execution

Custom node (`DagNode.svelte`): shows job name, workflow reference, status icon, duration. Color-coded borders (gray=pending, blue=running, green=success, red=failed, dashed=skipped). Running nodes pulse via CSS animation. Edges animate for in-progress, solid for complete, red for failed paths.

Layout via dagre (top-to-bottom or left-to-right, user-togglable). Minimap. Zoom/pan. Click node opens job detail. Context menu via Bits UI `ContextMenu`. Critical path highlighting.

### 6.3 Log Viewer

Custom virtualized Svelte component handling 100k+ lines:

- **Virtual scrolling**: `@tanstack/svelte-virtual` for the virtualized list
- **ANSI parsing**: Custom parser in `$lib/utils/ansi.ts` renders colors, bold, underline as HTML spans
- **Line numbers**: Click to select, shift-click for ranges, URL updates with `#L42-L67` via `$app/stores` page store
- **Timestamps**: Per-line, togglable
- **Search**: Ctrl+F override with match highlighting and jump-to-match
- **Follow mode**: Auto-scroll to bottom on new lines, disengage on manual scroll-up
- **Log diff overlay**: Compare against previous run of same pipeline (fetch previous, compute diff ranges, show gutter markers)
- **Section folding**: Collapsible `::group::`/`::endgroup::` sections
- **Download**: Raw `.log` file export

Component hierarchy:

```
LogViewer.svelte (container)
├── LogToolbar.svelte (search, filters, wrap, follow, download)
├── LogVirtualList.svelte (@tanstack/svelte-virtual)
│   └── LogLine.svelte (line number + timestamp + ANSI spans)
└── LogSearch.svelte (match count, prev/next)
```

Log viewer state (filters, search term, follow mode, wrap) lives in a Svelte `writable` store (`$lib/stores/log-viewer.ts`) so it persists across tab switches within the same run.

WebSocket log lines buffered and flushed via requestAnimationFrame to prevent excessive reactivity during high-throughput output.

---

## 7. Security and Supply Chain Pages

### 7.1 SBOM Browser

- Filterable list by project/pipeline/run/date
- Each entry: format (SPDX/CycloneDX), component count, creation date, associated run
- Expandable dependency tree view with search by package name/version
- Download as JSON/SPDX

### 7.2 SBOM Diff Viewer

- Pick any two SBOMs. Side-by-side or unified view:
  - Added (green), removed (red), version changes (yellow with old->new), license changes flagged
  - Summary header: "+N added, -N removed, N changed"
- Diff computed server-side (`GET /api/v1/sboms/diff?left={id}&right={id}`), rendered as annotated tree. Raw JSON diff available via Monaco diff editor.

### 7.3 Blast Radius Explorer

- Input: tool name + version/SHA (or select from tool database)
- Output from `GET /api/v1/tools/{id}/blast-radius`:
  - Summary card (N projects, N pipelines, N runs, date range)
  - Affected projects list with links
  - Timeline of affected runs
  - Optional graph visualization (tool at center, edges to affected items via Svelte Flow)

### 7.4 Tool Database

- Searchable/filterable table: tool name, versions seen, SHA256, first/last seen, usage count
- Drill down: version history, usage trend chart (LayerChart), list of pipelines using it
- Vulnerability indicators (if integrated with vuln database)

### 7.5 Flaky Step Dashboard

- Top N flakiest reusable workflow steps ranked by intermittent failure rate
- Per step: failure rate %, affected workflows, trend over time (LayerChart)

---

## 8. Administration Pages

### 8.1 Agent Pools

- Pool list: name/tags, agent count (online/offline/busy), platform breakdown
- Pool detail: agent table (hostname, platform, status, current job, last heartbeat, uptime)
- Agent detail: full metadata (OS, arch, IPs, container runtime), job history
- Admin actions: revoke agent, drain pool, create join token (via Superforms + form actions)

### 8.2 Users, Groups, Tokens

- User list (username, email, groups, role, last login). User profile with notification prefs.
- Group CRUD with membership management and project ownership assignment.
- API token management: create scoped to user/group/platform, set expiry, revoke.
- RBAC roles: platform admin, project owner, project member, viewer.

### 8.3 Webhooks and Integrations

- Webhook CRUD: URL, events subscribed, last delivery status, delivery log
- GitHub App: one-click webhook provisioning (global/project scoped)
- Notification channels: Slack/Teams/Webex/Discord webhook config

### 8.4 Release Management

- Release list: version, status (draft/scheduled/in-progress/complete/rolled-back), pipeline, scheduled time
- Release detail: associated run, approval status, comms draft, rollback history
- Scheduling: calendar picker for release windows, timezone-aware
- Comms generator: template-based release notes from run metadata
- Rollback: one-click trigger

---

## 9. Cross-Cutting Concerns

### Real-Time

Two WebSocket channels:

1. **Run stream** (`/api/v1/runs/{id}/stream`): Per-run logs and status. Opened `onMount` in run detail page, closed `onDestroy` on navigation away. Events written to a page-scoped Svelte `writable` store.
2. **Global events** (`/api/v1/events`): Org-wide notifications (run started/completed, agent offline). Opened once in root `+layout.svelte` `onMount`, kept alive. Events written to a global Svelte store.

WebSocket manager (`$lib/api/ws.ts`): exponential backoff reconnection (1s-30s), heartbeat, message deduplication, batched flush at 60fps via requestAnimationFrame.

### Error Handling

- `+error.svelte` at route root for global error fallback, plus per-section `+error.svelte` files for isolated failures
- `hooks.client.ts` `handleError` for unhandled client errors
- `hooks.server.ts` `handleError` for unhandled server errors
- API client wrapper: retry 3x with backoff for 5xx/network, immediate fail for 4xx, toast notifications on failure
- Offline detection: banner when network drops, queue mutations for retry

### Performance

- SvelteKit automatic code splitting per route (no manual lazy loading needed)
- Svelte compiles to minimal vanilla JS (smaller runtime than virtual-DOM frameworks)
- Virtual scrolling via `@tanstack/svelte-virtual` for log viewer and large lists
- No memoization boilerplate -- Svelte 5's compiler + runes handle fine-grained DOM updates
- Prefetching: SvelteKit's `data-sveltekit-preload-data` on links for instant navigation
- Bundle analysis via `vite-plugin-visualizer` in CI

### Accessibility

- Keyboard navigation via Bits UI primitives (same a11y guarantees as Radix -- focus trapping, ARIA attributes, keyboard interactions)
- ARIA labels on all icon-only buttons
- Focus management on route change and dialog lifecycle (Bits UI handles dialog focus)
- WCAG 2.1 AA color contrast
- `prefers-reduced-motion` respected via CSS media queries and Svelte `$effect`
- ARIA live regions for run status updates

### Theming

- Light/dark via CSS custom properties on `:root`
- System preference detection with manual override in a Svelte `writable` store persisted to localStorage
- Tailwind `dark:` variant on `.dark` class on `<html>` (set via inline script in `app.html` to prevent flash)
- Design tokens: color palette, spacing scale, typography scale, border radii, shadows

---

## 10. API Contract Summary

The frontend assumes endpoints defined in the API and CLI plan. Key patterns:

- REST CRUD for projects, pipelines, runs, agents, users, groups, tokens, secrets, variables, releases, webhooks, SBOMs, tools
- WebSocket at `/api/v1/runs/{id}/stream` and `/api/v1/events`
- All list endpoints support pagination (`?page=N&per_page=25` or cursor-based for runs), filtering, and sorting
- Auth endpoints: `/api/v1/auth/{login,callback,refresh,logout}`
- Full endpoint listing in [design/plans/06-frontend-and-ui.md](design/plans/06-frontend-and-ui.md) section 6

---

## 11. Key Technical Decisions and Rationale

**Why SvelteKit over plain Svelte + Vite?**
SvelteKit provides file-based routing, SSR, streaming, form actions, and automatic code splitting. Building these from scratch on plain Svelte + Vite would replicate what SvelteKit already does.

**Why Svelte stores instead of a state management library?**
Svelte's built-in `writable`/`readable`/`derived` stores cover all client state needs (theme, sidebar, auth, log viewer filters). Server-fetched data flows through SvelteKit `load` functions. No external state library is necessary.

**Why Bits UI?**
Bits UI is the Svelte equivalent of Radix UI -- accessible, unstyled component primitives (dialog, dropdown, tooltip, select, etc.) that handle keyboard interaction, focus trapping, and ARIA attributes. Styling with Tailwind gives full design control without reimplementing a11y.

**Why Svelte Flow for the DAG?**
Svelte Flow (`@xyflow/svelte`) is the official Svelte port of ReactFlow, maintained by the same xyflow team. It supports custom nodes, edges, layout plugins, minimap, and is Svelte-native. No React dependency.

**Why custom log viewer instead of xterm.js?**
xterm.js is a terminal emulator -- it handles input, cursor positioning, and escape sequences we don't need. A CI log viewer needs virtual scrolling over a static (but growing) list of lines with search, line selection, and diff support. A purpose-built Svelte component with `@tanstack/svelte-virtual` is lighter and more controllable.

**Why Superforms + Zod for forms?**
Superforms is purpose-built for SvelteKit. It integrates with SvelteKit form actions for progressive enhancement (forms work without JS), provides client-side validation via Zod, and handles loading/error states. No React dependency.

**Why LayerChart for charts?**
LayerChart is a Svelte-native charting library built on D3, with a composable component API. It avoids pulling in React-dependent charting libraries.

---

## 12. Open Questions

1. **Pagination style**: Offset-based or cursor-based? Recommend cursor for runs/logs (items inserted frequently), offset for stable lists (projects, users).
2. **Log lazy loading**: Fetch all lines or paginate? Recommend: last N lines initially + WebSocket for new + backward pagination for history.
3. **SBOM format**: Both SPDX and CycloneDX or normalize server-side? Recommend: canonical internal format, both raw formats for download.
4. **Mock API for frontend dev**: Should the frontend support a mock API mode? Recommend: MSW (Mock Service Worker) with realistic fixtures, or SvelteKit's server hooks to intercept and mock during dev.
5. **Mobile**: Desktop-first (1024px+), mobile as later enhancement.
6. **SSR vs SPA mode**: SvelteKit supports both. Recommend SSR for initial loads (better perceived performance, SEO for public pages) with client-side navigation after hydration. Can set `export const ssr = false` per route if needed.

---

## 13. Risk Mitigation

- **Log viewer perf at scale**: Virtual scrolling via `@tanstack/svelte-virtual` + batched rendering + web worker for ANSI parsing if needed
- **Large DAGs (50+ nodes)**: Svelte Flow + dagre handles this well; add zoom/pan/minimap; collapse sub-graphs for very large DAGs
- **WebSocket on flaky networks**: Exponential backoff reconnection, message buffering, "last seen" cursor for resumption
- **Bundle size creep**: CI analysis with `vite-plugin-visualizer`, SvelteKit auto code splitting, monitor with `size-limit`
- **API contract drift**: Generate TypeScript types from OpenAPI spec, CI check for spec compatibility
- **Svelte Flow maturity**: `@xyflow/svelte` is newer than ReactFlow but maintained by the same team and reaching stable API. Pin version and track releases.
