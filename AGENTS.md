# ICEcube: agent guide

ICEcube is a Kubernetes RBAC manager. Admins create **users**, put them in
**groups** and grant **templates** (ClusterRoles) per namespace or
cluster-wide. ICEcube turns that into RoleBindings/ClusterRoleBindings and
hands out kubeconfigs (client certificate or ServiceAccount token).

There is one binary: a Rust backend (axum + kube-rs) that serves the JSON
API and the Vue UI, which is embedded at compile time. It has no database;
all state lives in the Kubernetes API.

## Repository layout

```
backend/            Rust crate `icecube` (edition 2024)
  build.rs          release builds fail without frontend/dist (override: ICECUBE_ALLOW_NO_UI=1)
  src/main.rs       startup: CRDs, seeding, migration, controller task, HTTP server, shutdown
  src/route/        path -> handler mapping only
  src/handler/      axum handlers: extract, call usecase, shape response (+ auth, ui)
  src/usecase/      business logic on Kubernetes objects, returns DTOs, no axum
  src/library/      config, errors, model/CRDs, state, storage helpers, leader election
frontend/           Vue 3 + TS + Pinia + reka-ui + Tailwind 4, built with bun/Vite
charts/icecube/     Helm chart; crds/icecube-crds.yaml is GENERATED from the code
deploy/icecube.yaml plain manifests (keep RBAC in sync with the chart)
docs/development.md developer docs (config table, controller, credentials)
Makefile            dev/build/lint tasks (`make help`)
.github/workflows/  ci.yaml (fmt, clippy -D warnings, test, CRD drift, UI build, helm lint), release.yaml
```

## Backend layering (follow it)

`route` -> `handler` -> `usecase` -> `library`. Keep each layer to its job:

- **route/mod.rs**: only `Router` wiring. Everything under `/api` goes through
  `auth::basic_auth`. `/healthz` is public. The UI is the fallback.
- **handler/**: thin. Example: `Json(user::create(&s, input).await?)`. Never
  call the Kubernetes API directly from here.
- **usecase/**: one module per domain (`user`, `group`, `template`,
  `kubeconfig`, `controller`, `discovery`). Takes `&Client` or `&AppState` (use
  `&AppState` when it needs config or the namespace). Returns
  `AppResult<Dto>`.
- **library/**:
  - `model.rs`: CRDs, DTOs, constants, `validate_name`, naming helpers.
  - `error.rs`: `AppError` and how it maps to HTTP.
  - `config.rs`: env vars.
  - `state.rs`: `AppState` (Arc; holds the client, config, server URL, CA PEM
    and the namespace ICEcube runs in).
  - `kubeconfig_store.rs`: Secret per user.
  - `tombstone.rs`: blocked deleted usernames.
  - `leader.rs`: Lease election.

## Error handling

- Use `AppResult<T>` / `AppError` everywhere in handlers and usecases.
  Variants and their statuses: `BadRequest` -> 400, `NotFound` -> 404,
  `Conflict` -> 409, `Kube` (an API status is passed through as-is),
  `Internal` -> 500.
- For non-kube errors use `AppError::internal(e)`.
- `is_not_found(&e)` makes deletes idempotent ("missing is fine").
- Map a 409 on create to `AppError::Conflict` with a readable message.
- Startup code uses `anyhow` with `.context(...)`.

## Data model (source of truth: `backend/src/library/model.rs`)

### CRDs (`icecube.io/v1alpha1`, cluster scoped)

- **IcecubeUser** (`icu`)
  - `spec`:
    - `description?`
    - `bindings?: [{template, namespace?}]`. `None` only on legacy users;
      `user::migrate_legacy` fills it on startup.
    - `groups: [name]`
    - `credential: certificate | serviceAccount` (default `certificate`)
  - `status` (status subresource, never written through spec patches):
    - `certificates` (history, newest last, max 10 entries; each with
      `kind` and `revokedAt?`)
    - `certificatesValidUntil?` (latest cert expiry ever; it survives history
      pruning)
    - `blocked?` (why the controller grants nothing)
- **IcecubeGroup** (`icg`): `spec.description?`, `spec.bindings`,
  `spec.defaultForNewUsers`. New users join the default groups when the
  request has no `groups`.
- **Templates are not a CRD.** They are ClusterRoles named
  `icecube-template-<name>`, labelled `icecube.io/template=<name>`, with scope
  and description in annotations `icecube.io/scope` and
  `icecube.io/description`. Built-ins carry `icecube.io/builtin=true` and are
  seeded by `template::seed_builtins`.
- **Scope:**
  - A `namespaced` template can be bound per namespace (RoleBinding) or
    cluster-wide (ClusterRoleBinding).
  - A `cluster` template is only ever a ClusterRoleBinding; a namespace is
    rejected.

### Derived objects (all labelled `app.kubernetes.io/managed-by=icecube` + `icecube.io/user=<user>`)

- **Binding:** name `icecube-<user>-<template>`, label
  `icecube.io/template-ref`, ownerReference to the IcecubeUser (no
  `controller: true`, only cascading delete).
- **Subject:** `User <name>` for certificate users, or `ServiceAccount
  <ns>/icecube-user-<name>` for token users.
- **ServiceAccount** `icecube-user-<name>`: in ICEcube's namespace, owned by
  the user, `automountServiceAccountToken: false`.
- **Secret** `icecube-kubeconfig-<user>`: the latest kubeconfig, written when
  `ICECUBE_STORE_KUBECONFIGS=true`.
- **ConfigMap** `icecube-deleted-users`: key = username, value = RFC 3339
  time until which the name is blocked.
- **Lease** `icecube-controller`: leader election.
- **CSRs** `icecube-<user>-<ms>`: temporary, deleted after signing.

Effective access is `user.spec.bindings ∪ bindings of every group in
user.spec.groups` (`user::effective_bindings`). Unknown groups are ignored.
The frontend computes the same thing in `stores/groups.ts` (`effective`), so
keep the two in sync.

## Controller (`usecase/controller.rs`)

- **The API only writes CRs. It never creates bindings.** The kube-runtime
  controller reconciles each IcecubeUser.
- **Triggers:** the user itself; owned RoleBinding/ClusterRoleBinding (cluster
  wide) and ServiceAccount (own namespace), filtered by the managed-by label;
  IcecubeGroup changes (members only, via the reflector store); any Namespace
  change (all users); a resync every 5 minutes. Errors requeue after 15 s.
- **Apply** (`apply`), in order:
  1. Check the tombstone, and block if needed: revoke everything and set
     `status.blocked`.
  2. Ensure or delete the ServiceAccount according to `credential`.
  3. Call `user::reconcile`.
- **`user::apply` is idempotent:**
  - It server-side applies every desired binding, so drift is reverted.
  - It deletes current bindings that are not desired.
  - It skips bindings whose namespace does not exist (logged at debug level);
    the Namespace watch retries them.
- **Cleanup:** finalizer `icecube.io/cleanup` -> `user::revoke_all`. If
  certificates are still valid, `tombstone::record`. Then delete the stored
  kubeconfig. If ICEcube is down, deleted users hang on the finalizer.
- **Leader election** (`library/leader.rs`, written by hand; kube-runtime 5.1
  has none):
  - Every replica serves the API; only the Lease holder runs the controller.
  - Timings: lease 15 s, retry 2 s, renew deadline 10 s. Writes use
    `replace` with resourceVersion.
  - `ICECUBE_LEADER_ELECTION=false` runs the controller unconditionally (use
    with a single replica only).
- **Shutdown** goes through a `watch::channel<bool>`. `main::either` combines
  "lost leadership" with "process shutdown".

## Credentials (`usecase/kubeconfig.rs`)

- **Certificate:**
  1. rcgen key + CSR with CN=`<user>`.
  2. `CertificateSigningRequest` with signer
     `kubernetes.io/kube-apiserver-client` and `expirationSeconds`.
  3. Approve, poll `get` (not `/status`), delete the CSR.
  4. Read serial and expiry from the signed cert (`describe`).

  Certificates **cannot be revoked**.
- **ServiceAccount token:** `create_token_request` on the user's SA (waits up
  to 10 s for the controller to create it). `POST /api/users/{name}/revoke`
  deletes the SA, which invalidates all tokens immediately (they are bound to
  the SA UID), marks live token entries `revokedAt` and drops the stored
  kubeconfig. The controller recreates the SA.
- `issue` refuses blocked users (409) and users without effective access
  (400). Lifetime must be `1..=ICECUBE_CERT_DAYS` (default 30). `reissue`
  reuses the namespace and days of the latest entry, capped at the current
  maximum.
- `record` appends to the status history and maintains
  `certificatesValidUntil` (certificates only, never tokens).

## Username reuse protection (`library/tombstone.rs`)

Deleting a user with valid certificates blocks that name until they expire;
otherwise a new user with the same name would revive the old certificates.

- **API create:** a certificate user gets 409 unless `allowNameReuse: true`.
  That flag sets annotation `icecube.io/allow-name-reuse: "true"`; the
  controller then clears the tombstone.
- **GitOps-created CRs** are not rejected. Instead the controller grants
  nothing and sets `status.blocked`.
- **ServiceAccount users are never blocked:** old certificates only match
  `User` subjects.
- The ConfigMap is written with JSON merge patches on single keys. Expired
  entries are pruned on write.

## HTTP API (all under `/api`, Basic auth)

```
GET  /session                         user, clusterName, server, maxCertDays, storesKubeconfigs
GET  /namespaces
GET  /discovery                       API groups -> resources (+ subresources) -> verbs, for the template editor
GET|POST /templates     GET|PUT|DELETE /templates/{name}
GET|POST /groups        PUT|DELETE /groups/{name}
GET|POST /users         GET|PUT|DELETE /users/{name}
POST /users/{name}/revoke             ServiceAccount users only
GET|POST /users/{name}/kubeconfig     GET = stored latest, POST = issue {namespace?, expirationDays?}
POST /users/{name}/kubeconfig/reissue
```

JSON is camelCase (`#[serde(rename_all = "camelCase")]`). Errors are
`{"error": "..."}`. Auth compares in constant time (`subtle`) and sends no
`WWW-Authenticate` header.

## API discovery (`usecase/discovery.rs`)

The template editor offers only API groups, resources and verbs the cluster
really serves.

- `GET /api/discovery` reads `/api` and `/apis` plus every group version,
  like `kubectl api-resources`. It merges all versions of a group and keeps
  subresources (`pods/log`, `deployments/scale`).
- The core group is `""` in the API. The UI shows it as `core`
  (`TemplateDialog` `toDraft`/`fromDraft`).
- A group version whose discovery fails (for example an aggregated API that
  is down) is skipped, not fatal.
- RBAC-only verbs that discovery never lists are added by `extra_verbs`:
  `bind`/`escalate` on roles, `impersonate`, `approve`. RBAC-only resources
  are added by `virtual_resources` (`users`, `groups`, `uids`, `signers`, …)
  with `rbacOnly: true`. Extend these tables rather than special-casing in
  the UI.
- No extra RBAC is needed: discovery is allowed for every authenticated
  identity (`system:discovery`).
- The backend does **not** validate template rules against discovery.
  Templates may reference CRDs that are not installed yet; the UI only
  warns.

## Configuration (`library/config.rs`)

| Env | Default | Notes |
| --- | --- | --- |
| `BASIC_AUTH_PASSWORD` | required | |
| `BASIC_AUTH_USER` | `admin` | |
| `CLUSTER_NAME` | `kubernetes` | |
| `CONTROL_PLANE_ADDRESS` | client URL | Chart requires it |
| `ICECUBE_CERT_DAYS` | `30` | Max lifetime for certificates and tokens (chart: `maxCertDays`) |
| `ICECUBE_LEADER_ELECTION` | `true` | Chart: `leaderElection` |
| `ICECUBE_INSTALL_CRDS` | `true` | Chart: `crds.manageOnStartup` |
| `ICECUBE_STORE_KUBECONFIGS` | `true` | Chart: `storeKubeconfigs` |
| `ICECUBE_NAMESPACE` | pod / context ns | Where SAs, Secrets, ConfigMap, Lease live |
| `ICECUBE_LISTEN` | `0.0.0.0:4000` | |
| `POD_NAME` | hostname | Leader identity |
| `RUST_LOG` | `icecube=info,tower_http=info` | |

When adding an option, update all of: `config.rs`, the chart `values.yaml`
and `templates/deployment.yaml`, `deploy/icecube.yaml` if relevant, the table
in `docs/development.md`, and the settings table in `README.md` if user
facing.

## RBAC ICEcube needs

There are two copies: `charts/icecube/templates/rbac.yaml` and
`deploy/icecube.yaml`. **Change both together.**

- **ClusterRole:**
  - CRDs (optional)
  - icecubeusers/groups (+ status)
  - namespaces `get/list/watch`
  - clusterroles (with `escalate`, `bind`)
  - role/clusterrolebindings
  - CSRs + approval + signer `approve`
- **Role `<fullname>-local` in its own namespace:**
  - secrets (only if storeKubeconfigs)
  - serviceaccounts
  - `serviceaccounts/token` create
  - configmaps
  - `coordination.k8s.io` leases

Any new Kubernetes call means you must check these rules.

## Frontend (`frontend/src`)

- **`lib/api.ts`:** fetch wrapper. Credentials are kept in memory only,
  `ApiError(status, message)`, 401 triggers logout.
- **`lib/types.ts`:** mirrors the backend DTOs. Update it with every DTO
  change.
- **`lib/rbac.ts`:** client-side preview of the generated objects. Naming
  constants must match `backend/src/library/model.rs`.
- **`lib/certs.ts`:** credential state, one of
  `none | valid | expiring | expired | revoked`.
- **Stores (Pinia, setup style):**
  - `session`
  - `cluster` (namespaces + templates; `apiGroups` from `/discovery`,
    loaded on demand through `loadDiscovery()`)
  - `groups` (+ `effective()`)
  - `users` (optimistic update with rollback; kubeconfigs cached in memory
    for the tab only, never in browser storage)
  - `toast`
- **Views:** `UsersView`, `GroupsView`, `TemplatesView`, `LoginView`.
- **Dialogs:**
  - `UserDialog`: credential choice, name-reuse prompt on 409.
  - `KubeconfigDialog`.
  - `GroupDialog`.
  - `TemplateDialog`:
    - API groups, resources and verbs are pickers fed by discovery.
    - Resources are filtered by the selected groups, and verbs by the
      selected resources.
    - Values that do not exist or are unsupported are highlighted amber but
      kept.
    - It falls back to free-text `UiTags` when discovery fails.
- **Dialog sizes:** `UiDialog` has only `size="form"` (default, `max-w-2xl`)
  and `size="editor"` (`max-w-6xl`, fixed height so tabs do not jump). Use
  `editor` for anything with tabs, editors, previews or YAML; do not add
  one-off widths.
- **Dropdowns** (`UiMultiPick`, `UiSelect`) render in a portal, may grow
  wider than their field (up to 32rem) and wrap long names instead of
  truncating them.
- **Primitives:** `components/ui/Ui*.vue` (UiButton, UiDialog, UiConfirm,
  UiBadge with tones `neutral|brand|amber|green|violet`, UiSelect, UiCode,
  UiTags for free text, UiMultiPick for a searchable multi-select with
  `options` and an optional `allowCustom`).
- Icons come from `lucide-vue-next`; headless components from `reka-ui`.
  Tailwind uses the `brand-*` palette from `style.css`. Use the `@/` alias for
  `src`.
- The Vite dev server proxies `/api` to `ICECUBE_API` (default
  `http://localhost:4000`).

## Workflows

```sh
make dev              # backend (cargo run) + Vite on :5173 against current kubeconfig
make dev-local        # same against a throwaway k3s in Docker (.dev/kubeconfig)
make test             # cargo test
make lint             # fmt --check, clippy -D warnings, vue-tsc, CRD drift, helm lint
make crd              # regenerate charts/icecube/crds/icecube-crds.yaml (run after model.rs CRD changes)
make build            # release binary with embedded UI
```

Before finishing a change, run at least:

- `cd backend && cargo fmt && ICECUBE_ALLOW_NO_UI=1 cargo clippy --all-targets -- -D warnings && cargo test`
- `cd frontend && bunx vue-tsc -b`
- `make crd` if a CRD type, doc comment or printcolumn changed. CI diffs
  `cargo run -- crd` against the chart file.
- `make helm-lint` if the chart changed (it uses Docker when helm is
  missing).

## Conventions and pitfalls

- **Dependency versions are pinned to these APIs:** kube 5.1 (`runtime`,
  `derive`), k8s-openapi 0.29 (`latest`). In 0.29 many spec fields are
  `Option` (e.g. `TokenRequest.spec`, `TokenRequestSpec.audiences`,
  `TokenRequestStatus.token/expiration_timestamp`), and times are
  `jiff::Timestamp` (`Time(pub Timestamp)`, `MicroTime`). Check
  `~/.cargo/registry/src/*/k8s-openapi-0.29.0` before guessing.
- **Writes:** use server-side apply (`PatchParams::apply(MANAGER).force()`)
  for objects ICEcube owns, and merge patches for CR spec/status edits. Never
  patch status through the main resource.
- **Labels** go through the `LABEL_*` / `ANNO_*` constants and the naming
  helpers (`binding_name`, `service_account_name`,
  `template_cluster_role`); never use string literals.
- **Names** are DNS-1123 labels (`validate_name`, max 63) because they end up
  in labels and object names.
- **Owner references** use `controller_owner_ref` with `controller` and
  `block_owner_deletion` cleared. A cluster-scoped user may own namespaced
  objects.
- **Errors:** handle "missing is fine" with `is_not_found`. Failing to store
  a kubeconfig must not fail the issue: log a warning instead.
- **Logs:** `tracing` with structured fields (`user = %name`). Use `info` for
  state changes and `warn` for degraded paths.
- **Comments** explain why, not what. The code base keeps module-level `//!`
  docs current; update them when behaviour changes.
- **Tests:** unit tests live next to the code (`#[cfg(test)] mod tests`);
  there are no cluster tests. Pure helpers (`effective_bindings`,
  `cert_valid_until`, `validate_name`, UI serving) are tested. Keep new logic
  pure where possible so it can be tested the same way.
- **Startup order** in `main.rs`:
  1. ensure CRDs
  2. seed templates
  3. seed the `default` group (only when no group exists)
  4. migrate legacy users
  5. spawn the controller
  6. serve

  `icecube crd` prints the CRDs and exits.
