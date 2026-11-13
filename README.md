# ICEcube

[![CI](https://github.com/sofyan48/ICEcube/actions/workflows/ci.yaml/badge.svg)](https://github.com/sofyan48/ICEcube/actions/workflows/ci.yaml)

Kubernetes RBAC manager, Create users, grant them permission templates per namespace or cluster-wide, and hand out
certificate-based kubeconfigs.

- **Backend:** Rust (axum + kube-rs), in `backend/`
- **Frontend:** Vue 3 + Pinia + Reka UI + Tailwind v4, built with Bun, in `frontend/`

## How it works

| Concept    | Kubernetes object                                                                 |
| ---------- | --------------------------------------------------------------------------------- |
| User       | `IcecubeUser` (cluster-scoped CRD `icecubeusers.icecube.io`, short name `icu`)    |
| Group      | `IcecubeGroup` (CRD `icecubegroups.icecube.io`, short name `icg`): a named set of grants |
| Template   | `ClusterRole` named `icecube-template-<name>`, labelled `icecube.io/template`     |
| Grant      | `RoleBinding` (in one namespace) or `ClusterRoleBinding` (cluster-wide), named `icecube-<user>-<template>` |
| Kubeconfig | Client certificate (CN = user) from a `CertificateSigningRequest` using the `kubernetes.io/kube-apiserver-client` signer |

- On startup the backend installs the CRD and seeds four builtin templates: `viewer`, `developer`,
  `namespace-lister` and `cluster-viewer`.
- On first install it also creates the `default` group (`cluster-viewer`, `namespace-lister` and
  `viewer`, all cluster-wide), marked *default for new users*. It is only seeded while no group
  exists, so you can edit or delete it freely.
- A user's spec is the source of truth. Effective access is the user's direct grants plus the
  grants of every group the user is in. Editing a group re-applies access for all of its members
  right away, and deleting a group removes it from its members.
- New users join the default groups unless the request lists `groups` explicitly (`[]` means no
  group).
- Bindings are reconciled: saving a user creates the missing bindings and deletes the ones that were
  removed. Each binding is owned by its `IcecubeUser`, so deleting the user also garbage-collects
  anything left behind.
- The private key for a kubeconfig is generated per request, returned once and never stored.
  Certificates cannot be revoked one by one. Removing a user's bindings revokes their access
  immediately, but the certificate stays valid until it expires, so keep lifetimes short.

### Client-side model

The UI loads users, templates and namespaces once into Pinia stores (`frontend/src/stores`). It then
does the following without calling the server:

- searching and filtering
- computing statistics and template usage
- validating input, using the same rules as the server
- rendering the exact YAML the backend will create (`frontend/src/lib/rbac.ts`)

User updates are optimistic and roll back if the server rejects them. Credentials are kept in memory
and in `sessionStorage`, so closing the tab signs the user out.

## Development

You need Rust ≥ 1.95, Bun ≥ 1.4, and a kubeconfig for a cluster where you are admin.

```sh
# backend: uses ~/.kube/config (or in-cluster config)
cd backend
BASIC_AUTH_PASSWORD=secret CLUSTER_NAME=dev cargo run

# frontend: Vite dev server, proxies /api to :4000
cd frontend
bun install
bun dev
```

Open http://localhost:5173 and sign in as `admin` / `secret`.

To serve the built UI from the backend, run `bun run build`. The backend then serves
`../frontend/dist` on port 4000.

### Configuration

| Variable                | Default            | Description                                         |
| ----------------------- | ------------------ | --------------------------------------------------- |
| `BASIC_AUTH_PASSWORD`   | *(required)*       | UI/API password                                     |
| `BASIC_AUTH_USER`       | `admin`            | UI/API username                                     |
| `CLUSTER_NAME`          | `kubernetes`       | Cluster name used in generated kubeconfigs          |
| `CONTROL_PLANE_ADDRESS` | client's API URL   | Public API server URL written into kubeconfigs      |
| `ICECUBE_CERT_DAYS`     | `365`              | Maximum certificate lifetime                        |
| `ICECUBE_LISTEN`        | `0.0.0.0:4000`     | Listen address                                      |
| `ICECUBE_STATIC_DIR`    | `../frontend/dist` | Built UI                                            |

`icecube crd` prints the CRD YAML, so you can apply it ahead of time (for example from GitOps).

## API

All `/api` routes require HTTP Basic auth.

| Method | Path                           | Body                                     |
| ------ | ------------------------------ | ---------------------------------------- |
| GET    | `/api/session`                 |                                          |
| GET    | `/api/namespaces`              |                                          |
| GET    | `/api/templates`               |                                          |
| POST   | `/api/templates`               | `{ name, scope, description, rules }`    |
| PUT    | `/api/templates/{name}`        | same as POST (scope cannot change)       |
| DELETE | `/api/templates/{name}`        | refused while still bound to users       |
| GET    | `/api/groups`                  |                                          |
| POST   | `/api/groups`                  | `{ name, description, bindings, defaultForNewUsers }` |
| PUT    | `/api/groups/{name}`           | same as POST; re-applies access for members |
| DELETE | `/api/groups/{name}`           | removes the group from its members       |
| GET    | `/api/users`                   |                                          |
| POST   | `/api/users`                   | `{ name, description, bindings, groups? }`; without `groups` the user joins the default groups |
| PUT    | `/api/users/{name}`            | `{ description, bindings, groups }`, replaces both |
| DELETE | `/api/users/{name}`            |                                          |
| POST   | `/api/users/{name}/kubeconfig` | `{ namespace?, expirationDays? }`        |

A binding is `{ "template": "developer", "namespace": "team-a" }`. Set `"namespace": null` for a
cluster-wide grant.

## Install with Helm

### Prerequisites

- Kubernetes **1.30+**, and `kubectl` access as cluster-admin (the chart creates CRDs and a
  ClusterRole).
- Helm **3.8+**, which is the first version that can install charts from OCI registries.
- The API server must accept **client certificates**, and the `kubernetes.io/kube-apiserver-client`
  signer must be enabled. Self-managed clusters (kubeadm, k3s, RKE2, and so on) have both by
  default. Some managed services restrict client-certificate auth or this signer, so check your
  provider's documentation before installing there.

### 1. Find the API server address for users

Kubeconfigs issued by ICEcube point at `controlPlaneAddress`. It must be an address that **your
users** can reach, not the in-cluster `kubernetes.default.svc`. Usually it is the same server that
is in your own kubeconfig:

```sh
kubectl config view --minify -o jsonpath='{.clusters[0].cluster.server}'; echo
```

### 2. Write a values file

Start with `values-icecube.yaml`:

```yaml
clusterName: prod                                  # cluster/context name in kubeconfigs
controlPlaneAddress: https://k8s.example.com:6443  # from step 1 (required)
maxCertDays: 90                                    # longest certificate lifetime users can pick

auth:
  username: admin
  # password: leave unset to generate one (see step 4)

ingress:
  enabled: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt    # if you use cert-manager
  hosts:
    - host: icecube.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: icecube-tls
      hosts: [icecube.example.com]
```

The ingress is optional. Leave it out and use `kubectl port-forward` instead (step 4).

### 3. Install

From the release published to GHCR:

```sh
helm install icecube oci://ghcr.io/sofyan48/charts/icecube \
  --version 0.1.0 \
  --namespace icecube --create-namespace \
  -f values-icecube.yaml
```

Or from a checkout of this repository:

```sh
git clone https://github.com/sofyan48/ICEcube.git && cd ICEcube
helm install icecube ./charts/icecube \
  --namespace icecube --create-namespace \
  -f values-icecube.yaml
```

Wait for the pod to become ready:

```sh
kubectl -n icecube rollout status deploy/icecube
```

On first start ICEcube applies its CRDs, then creates the builtin templates (`viewer`, `developer`,
`namespace-lister`, `cluster-viewer`) and the `default` group.

### 4. Sign in

Read the password. It is generated if you did not set `auth.password`:

```sh
kubectl -n icecube get secret icecube-auth \
  -o jsonpath='{.data.BASIC_AUTH_PASSWORD}' | base64 -d; echo
```

Then open the UI:

- **With an ingress:** go to `https://icecube.example.com`.
- **Without an ingress:**

  ```sh
  kubectl -n icecube port-forward svc/icecube 4000:80
  ```

  Then open http://localhost:4000.

Sign in as `admin`. `helm install` also prints these steps.

### 5. Verify

```sh
helm test icecube -n icecube
```

The test pod calls `/healthz` and then an authenticated API endpoint, which proves that the
service, the credentials and ICEcube's access to the Kubernetes API all work.

### Common values

See [`charts/icecube/values.yaml`](charts/icecube/values.yaml) for every option.

| Value                   | Default                    | Description                                                   |
| ----------------------- | -------------------------- | ------------------------------------------------------------- |
| `controlPlaneAddress`   | *(required)*               | API server URL written into kubeconfigs                       |
| `clusterName`           | `kubernetes`               | Cluster name in kubeconfigs                                   |
| `maxCertDays`           | `365`                      | Longest certificate lifetime                                  |
| `auth.username`         | `admin`                    | UI username                                                   |
| `auth.password`         | *(generated)*              | UI password                                                   |
| `auth.existingSecret`   |                            | Use your own Secret instead (keys `BASIC_AUTH_USER`, `BASIC_AUTH_PASSWORD`) |
| `ingress.enabled`       | `false`                    | Expose through an Ingress. Always use TLS                     |
| `crds.manageOnStartup`  | `true`                     | Backend applies the CRDs on startup. Set `false` when CRDs are managed elsewhere |
| `image.repository`      | `ghcr.io/sofyan48/icecube` | Image                                                         |
| `image.tag`             | chart `appVersion`         | Image tag                                                     |
| `resources`             | 20m / 32Mi, limit 128Mi    | Pod resources                                                 |

### Using your own password Secret

```sh
kubectl create namespace icecube
kubectl -n icecube create secret generic icecube-credentials \
  --from-literal=BASIC_AUTH_USER=admin \
  --from-literal=BASIC_AUTH_PASSWORD='a-long-random-password'

helm install icecube oci://ghcr.io/sofyan48/charts/icecube -n icecube \
  -f values-icecube.yaml --set auth.existingSecret=icecube-credentials
```

### Argo CD / Flux (GitOps)

- **Always set `auth.existingSecret`.** Argo CD renders charts with `helm template`, where the
  chart cannot look up the existing Secret. A generated password would therefore change on
  every sync.
- **To manage CRDs from Git as well**, set `crds.manageOnStartup: false` and apply them
  yourself. Pick one of:

  ```sh
  kubectl apply -f charts/icecube/crds/icecube-crds.yaml
  ```

  ```sh
  docker run --rm ghcr.io/sofyan48/icecube:0.1.0 crd | kubectl apply -f -
  ```

  With this setting the backend no longer needs permission to change CRDs. It only waits for
  them to exist.

### Upgrade

```sh
helm upgrade icecube oci://ghcr.io/sofyan48/charts/icecube --version <new> \
  -n icecube -f values-icecube.yaml
```

- The generated password is kept across upgrades.
- Helm never upgrades files in `crds/`. With the default `crds.manageOnStartup: true`, the new
  version applies its own CRDs when it starts.
- With `crds.manageOnStartup: false`, apply the new CRDs **before** upgrading.

### Uninstall

```sh
helm uninstall icecube -n icecube
```

This only removes the ICEcube app. Helm intentionally keeps CRDs, so your users, groups,
templates and the access they grant **stay in place** and keep working.

To remove everything ICEcube created and **revoke all access it granted**, run:

```sh
# Deleting the CRDs deletes all users and groups. Their bindings are garbage-collected.
kubectl delete crd icecubeusers.icecube.io icecubegroups.icecube.io
# Templates and any bindings left behind
kubectl delete clusterrole,clusterrolebinding -l app.kubernetes.io/managed-by=icecube
kubectl delete rolebinding -A -l app.kubernetes.io/managed-by=icecube
kubectl delete namespace icecube
```

Certificates that were already issued stay valid until they expire. They no longer grant
anything once the bindings are gone.

### Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| `controlPlaneAddress is required` during install | Set `controlPlaneAddress` (step 1). |
| Pod in `ImagePullBackOff` | GHCR packages are private until made public. Make `icecube` public, or set `imagePullSecrets`. |
| Pod restarts with `CRD ... is not installed` | `crds.manageOnStartup` is `false` but the CRDs were never applied. Apply them, see the GitOps section. |
| Issuing a kubeconfig times out | The cluster is not signing `kubernetes.io/kube-apiserver-client` CSRs. Check that kube-controller-manager has signing enabled. |
| `kubectl` with the issued kubeconfig gets `Unauthorized` | `controlPlaneAddress` points at an endpoint that does not accept client certificates (for example a proxy). Use the API server address directly. |
| Password changes after every sync | You are using GitOps without `auth.existingSecret`. See the GitOps section. |

Logs:

```sh
kubectl -n icecube logs deploy/icecube
```

### Without Helm

Plain manifests are in [`deploy/icecube.yaml`](deploy/icecube.yaml). Edit the Secret,
`CLUSTER_NAME` and `CONTROL_PLANE_ADDRESS`, then run `kubectl apply -f deploy/icecube.yaml`.

### Security

ICEcube can grant any permission in the cluster. Its service account can escalate and bind
ClusterRoles and approve client certificates. Treat access to the UI like cluster-admin, serve it
only over TLS (it uses Basic auth and hands out private keys), and keep `maxCertDays` short.

### Maintaining the chart

The CRDs are kept in `charts/icecube/crds/`. After changing `backend/src/model.rs`, regenerate
them; CI fails if they drift from the code:

```sh
cd backend && cargo run -- crd > ../charts/icecube/crds/icecube-crds.yaml
```

## Releasing

Pushing a `vX.Y.Z` tag runs `.github/workflows/release.yaml`, which:

- builds and pushes `ghcr.io/sofyan48/icecube:X.Y.Z` (amd64 + arm64)
- packages the chart with version/appVersion `X.Y.Z` and pushes it to `oci://ghcr.io/sofyan48/charts/icecube`
- creates a GitHub release with the chart attached

```sh
git tag v0.1.0 && git push origin v0.1.0
```

GHCR packages are private by default. After the first release, make `icecube` and
`charts/icecube` public under *Packages → Package settings*, or add an `imagePullSecrets` entry.
