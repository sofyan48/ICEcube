# ICEcube: technical notes and development

This page is for people working on ICEcube itself. To install and use it, see the
[README](../README.md).

- **Backend:** Rust (axum + kube-rs), in `backend/`
- **Frontend:** Vue 3 + Pinia + Reka UI + Tailwind v4, built with Bun, in `frontend/`

## How it works

| Concept    | Kubernetes object |
| ---------- | ----------------- |
| User       | `IcecubeUser` (cluster-scoped CRD `icecubeusers.icecube.io`, short name `icu`) |
| Group      | `IcecubeGroup` (CRD `icecubegroups.icecube.io`, short name `icg`): a named set of grants |
| Template   | `ClusterRole` named `icecube-template-<name>`, labelled `icecube.io/template` |
| Grant      | `RoleBinding` (in one namespace) or `ClusterRoleBinding` (cluster-wide), named `icecube-<user>-<template>` |
| Kubeconfig | Client certificate (CN = user) from a `CertificateSigningRequest` using the `kubernetes.io/kube-apiserver-client` signer |

- On startup the backend installs the CRDs and seeds four builtin templates: `viewer`,
  `developer`, `namespace-lister` and `cluster-viewer`.
- On first install it also creates the `default` group (`cluster-viewer`, `namespace-lister` and
  `viewer`, all cluster-wide), marked *default for new users*. It is only seeded while no group
  exists, so you can edit or delete it freely.
- A user's spec is the source of truth. Effective access is the user's direct grants plus the
  grants of every group the user is in. Editing a group re-applies access for all of its members
  right away, and deleting a group removes it from its members.
- New users join the default groups unless the request lists `groups` explicitly (`[]` means no
  group).
- Bindings are reconciled: saving a user creates the missing bindings and deletes the ones that
  were removed. Each binding is owned by its `IcecubeUser`, so deleting the user also
  garbage-collects anything left behind.
- The private key for a kubeconfig is generated per request. Certificates cannot be revoked one
  by one. Removing a user's bindings revokes their access immediately, but the certificate stays
  valid until it expires, so keep lifetimes short.
- The latest kubeconfig of each user is kept in a Secret `icecube-kubeconfig-<user>` in ICEcube's
  namespace (keys `kubeconfig` and `certificate.json`). It is overwritten on every issue and owned
  by the `IcecubeUser`, so deleting the user deletes it. `GET /api/users/{name}/kubeconfig` returns
  it. Access to it is granted by a namespaced Role, so ICEcube cannot read Secrets anywhere else.
  Anyone who can read Secrets in that namespace can already read the admin password and mint any
  kubeconfig, so this does not widen who can act as a user. Set `ICECUBE_STORE_KUBECONFIGS=false`
  (Helm: `storeKubeconfigs: false`) to keep nothing; a kubeconfig is then only shown right after
  it is issued.
- Each issue is recorded in the user's `status.certificates`: serial, issue time, expiry,
  namespace and requested days. The last 10 are kept, and keys are never stored. Expiry is read
  from the signed certificate, so it reflects any cap the cluster applies.
- Re-issue uses the previous namespace and lifetime, or the defaults for users without history.
  The UI marks certificates as expiring when less than a fifth of their lifetime is left (between
  1 and 7 days).

### Credentials

The backend talks to Kubernetes with `kube::Config::infer()`: it uses `KUBECONFIG` or
`~/.kube/config` when present, and otherwise the pod's ServiceAccount token. In the cluster it
runs as the `icecube` ServiceAccount, with the permissions in `charts/icecube/templates/rbac.yaml`.

### Client-side model

The UI loads users, groups, templates and namespaces once into Pinia stores
(`frontend/src/stores`). It then does the following without calling the server:

- searching and filtering
- computing statistics, effective access and template usage
- validating input, using the same rules as the server
- rendering the exact YAML the backend will create (`frontend/src/lib/rbac.ts`)

User updates are optimistic and roll back if the server rejects them. Credentials are kept in
memory and in `sessionStorage`, so closing the tab signs the user out. Kubeconfigs are cached in
memory only (never in browser storage) and fetched from the server again after a reload.

### One binary

In production ICEcube is a single process. `cargo build --release` embeds the built UI
(`frontend/dist`) into the binary with [rust-embed](https://crates.io/crates/rust-embed), so the
binary serves `/api`, `/healthz` and the UI by itself. The container image (distroless, about
75 MB) contains only that binary.

- **Caching:** hashed assets under `assets/` are cached for a year (`immutable`). `index.html` is
  revalidated on every load with an ETag.
- **Compression:** responses are gzip/brotli compressed.
- **Client-side routes:** deep links get `index.html`. Missing files and unknown `/api` routes
  return `404`.
- **Headers:** `X-Frame-Options: DENY`, `nosniff` and `Referrer-Policy` are set.
- **Shutdown:** the process stops cleanly on SIGTERM.

The service is stateless: everything lives in the Kubernetes API, and private keys are never
stored. Keep it at one replica, though: two pods reconciling the same user concurrently could race.

Build the frontend first, because it is embedded at compile time. `make build` does both steps,
and a release build without `frontend/dist` fails with instructions. Set `ICECUBE_ALLOW_NO_UI=1`
to compile a release binary without the UI (CI and the Docker dependency cache do this).

The Docker build and runtime images must share the same Debian release (glibc). Both are pinned to
trixie / debian13 in the `Dockerfile`; change them together.

## Development

You need Rust 1.95 or newer, Bun 1.4 or newer, Docker and `kubectl`.

```sh
# backend: uses ~/.kube/config (or KUBECONFIG)
cd backend && BASIC_AUTH_PASSWORD=secret cargo run

# frontend: Vite dev server, proxies /api to :4000
cd frontend && bun install && bun dev
```

`make dev` runs exactly these two commands in one terminal. Open http://localhost:5173 and sign in
as `admin` / `secret`. Ctrl+C stops both.

The backend uses your **current kubeconfig** and installs its CRDs, builtin templates and the
`default` group into that cluster on startup. To try things out without touching a real cluster,
run `make dev-local`. It starts a throwaway k3s cluster in Docker (namespaces `team-a` and
`team-b`, kubeconfig in `.dev/kubeconfig`) and runs `make dev` against it. Remove the cluster
with `make cluster-down`.

| Target | What it does |
| --- | --- |
| `make dev` | Backend + frontend against your current kubeconfig |
| `make dev-local` | Same, against a throwaway k3s cluster |
| `make dev-backend` / `make dev-frontend` | Run one side only |
| `make build` | Build the single release binary `backend/target/release/icecube` with the UI embedded |
| `make run` | Build it and run it on :4000 |
| `make lint` | rustfmt, clippy, `vue-tsc`, Helm lint and CRD drift check |
| `make test` | Backend tests |
| `make crd` | Regenerate `charts/icecube/crds/` after changing `backend/src/model.rs` |
| `make docker-build` | Build the container image |
| `make cluster-status` | Show the dev cluster and ICEcube objects in it |
| `make cluster-down` | Delete the dev cluster |

Backend settings (below) can be passed to make, for example
`make dev CLUSTER_NAME=prod ICECUBE_CERT_DAYS=7`. Use an absolute path for `KUBECONFIG`, because
the backend runs from `backend/`.

### Configuration

| Variable                | Default            | Description |
| ----------------------- | ------------------ | ----------- |
| `BASIC_AUTH_PASSWORD`   | *(required)*       | UI/API password |
| `BASIC_AUTH_USER`       | `admin`            | UI/API username |
| `CLUSTER_NAME`          | `kubernetes`       | Cluster name used in generated kubeconfigs |
| `CONTROL_PLANE_ADDRESS` | client's API URL   | Public API server URL written into kubeconfigs |
| `ICECUBE_CERT_DAYS`     | `365`              | Maximum certificate lifetime |
| `ICECUBE_LISTEN`        | `0.0.0.0:4000`     | Listen address |
| `ICECUBE_INSTALL_CRDS`  | `true`             | Apply the CRDs on startup |
| `ICECUBE_STORE_KUBECONFIGS` | `true`         | Keep each user's latest kubeconfig in a Secret |
| `ICECUBE_NAMESPACE`     | pod / context namespace | Namespace for those Secrets |
| `RUST_LOG`              | `icecube=info,tower_http=info` | Log filter |

`icecube crd` prints the CRD YAML, so you can apply it ahead of time (for example from GitOps).

## API

All `/api` routes require HTTP Basic auth.

| Method | Path | Body |
| ------ | ---- | ---- |
| GET    | `/api/session` | |
| GET    | `/api/namespaces` | |
| GET    | `/api/templates` | |
| POST   | `/api/templates` | `{ name, scope, description, rules }` |
| PUT    | `/api/templates/{name}` | same as POST (scope cannot change) |
| DELETE | `/api/templates/{name}` | refused while still used by users or groups |
| GET    | `/api/groups` | |
| POST   | `/api/groups` | `{ name, description, bindings, defaultForNewUsers }` |
| PUT    | `/api/groups/{name}` | same as POST; re-applies access for members |
| DELETE | `/api/groups/{name}` | removes the group from its members |
| GET    | `/api/users` | |
| POST   | `/api/users` | `{ name, description, bindings, groups? }`; without `groups` the user joins the default groups |
| PUT    | `/api/users/{name}` | `{ description, bindings, groups }`, replaces both |
| DELETE | `/api/users/{name}` | |
| GET    | `/api/users/{name}/kubeconfig` | the stored latest kubeconfig (`404` if none or storage is off) |
| POST   | `/api/users/{name}/kubeconfig` | `{ namespace?, expirationDays? }` |
| POST   | `/api/users/{name}/kubeconfig/reissue` | no body; same settings as the latest certificate |

A binding is `{ "template": "developer", "namespace": "team-a" }`. Set `"namespace": null` for a
cluster-wide grant.

## Helm chart

### GitOps (Argo CD / Flux)

- **Always set `auth.existingSecret`.** Argo CD renders charts with `helm template`, where the
  chart cannot look up the existing Secret, so a generated password would change on every sync.
- **To manage the CRDs from Git as well**, set `crds.manageOnStartup: false` and apply them
  yourself, either with `kubectl apply -f charts/icecube/crds/icecube-crds.yaml` or with
  `docker run --rm ghcr.io/sofyan48/icecube:<version> crd | kubectl apply -f -`. With this setting
  the backend only waits for the CRDs and needs no permission to change them.

### CRDs

Helm installs the files in `charts/icecube/crds/` on first install but never upgrades them. With
the default `crds.manageOnStartup: true`, the backend applies its own CRDs when it starts, so
upgrades pick up schema changes. With `false`, apply the new CRDs **before** upgrading.

After changing `backend/src/model.rs`, regenerate them with `make crd`. CI fails if they drift
from the code.

### Without Helm

Plain manifests are in [`deploy/icecube.yaml`](../deploy/icecube.yaml). Edit the Secret,
`CLUSTER_NAME` and `CONTROL_PLANE_ADDRESS`, then run `kubectl apply -f deploy/icecube.yaml`.

## Releasing

Pushing a `vX.Y.Z` tag runs `.github/workflows/release.yaml`, which:

- builds and pushes `ghcr.io/sofyan48/icecube:X.Y.Z` (amd64 + arm64)
- packages the chart with version/appVersion `X.Y.Z` and pushes it to `oci://ghcr.io/sofyan48/charts/icecube`
- creates a GitHub release with the chart attached

```sh
git tag v0.1.0 && git push origin v0.1.0
```

GHCR packages are private by default. After the first release, make `icecube` and
`charts/icecube` public under *Packages → Package settings*.
