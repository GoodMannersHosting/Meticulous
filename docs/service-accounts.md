# Service accounts

Service accounts are organization users flagged with `users.service_account = true`. They are intended for **machine and API access**, not interactive sign-in.

## Behavior

- **Creation:** Platform administrators use `POST /api/v1/admin/users` with the service-account payload (see OpenAPI / `AdminCreateServiceAccountRequest`). The Admin UI exposes this as **Users → Service account**.
- **Password / SSO login:** The API rejects password login and OAuth completion paths for service accounts (`/auth/login` and related), with a clear error.
- **API access:** Use normal **user API tokens** (`Authorization: Token met_…`) scoped to the service account user, subject to organization policy (TTL cap, two active tokens per owner, etc.).

## Operations

- Treat service accounts like any other user for **admin user listing**, locking, and deletion policies.
- Prefer **dedicated emails** (e.g. `ci-bot@internal.example`) and descriptive display names so operators can distinguish them in the admin token inventory.
