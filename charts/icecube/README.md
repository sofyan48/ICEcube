# icecube Helm chart

A Kubernetes RBAC manager: users, groups and permission templates managed from a web UI, plus
certificate-based kubeconfigs for each user.

How to install and use ICEcube: https://github.com/sofyan48/ICEcube#install
GitOps and other advanced setups: https://github.com/sofyan48/ICEcube/blob/main/docs/development.md#helm-chart

## Quick start

```sh
helm install icecube oci://ghcr.io/sofyan48/charts/icecube \
  --namespace icecube --create-namespace \
  --set clusterName=prod \
  --set controlPlaneAddress=https://k8s.example.com:6443

# password (generated when auth.password is not set)
kubectl -n icecube get secret icecube-auth -o jsonpath='{.data.BASIC_AUTH_PASSWORD}' | base64 -d; echo

kubectl -n icecube port-forward svc/icecube 4000:80   # then open http://localhost:4000
helm test icecube -n icecube
```

`controlPlaneAddress` is required. It is the API server URL that your users can reach, and it is
written into every kubeconfig ICEcube issues.

## Values

| Key | Default | Description |
| --- | --- | --- |
| `controlPlaneAddress` | `""` | **Required.** API server URL written into kubeconfigs |
| `clusterName` | `kubernetes` | Cluster/context name in kubeconfigs |
| `maxCertDays` | `365` | Longest certificate lifetime |
| `storeKubeconfigs` | `true` | Keep each user's latest kubeconfig (a Secret in the release namespace) so it can be downloaded again |
| `auth.username` | `admin` | UI username |
| `auth.password` | `""` | UI password. Generated and kept across upgrades when empty |
| `auth.existingSecret` | `""` | Existing Secret holding the credentials (use this with GitOps) |
| `auth.usernameKey` / `auth.passwordKey` | `BASIC_AUTH_USER` / `BASIC_AUTH_PASSWORD` | Keys in that Secret |
| `crds.manageOnStartup` | `true` | Backend applies CRDs on startup (Helm never upgrades `crds/`) |
| `image.repository` | `ghcr.io/sofyan48/icecube` | Image |
| `image.tag` | `""` (appVersion) | Image tag |
| `image.pullPolicy` | `IfNotPresent` | |
| `imagePullSecrets` | `[]` | |
| `replicaCount` | `1` | Keep at 1 |
| `serviceAccount.create` / `.name` / `.annotations` | `true` / `""` / `{}` | |
| `rbac.create` | `true` | ClusterRole and ClusterRoleBinding for ICEcube |
| `service.type` / `service.port` | `ClusterIP` / `80` | |
| `ingress.enabled` | `false` | |
| `ingress.className`, `.annotations`, `.hosts`, `.tls` | | Standard Ingress settings |
| `resources` | requests 20m/32Mi, limit 128Mi | |
| `logLevel` | `icecube=info,tower_http=info` | `RUST_LOG` filter |
| `extraEnv` | `[]` | Extra container env vars |
| `podAnnotations`, `podLabels`, `nodeSelector`, `tolerations`, `affinity` | | Pod scheduling and metadata |
| `podSecurityContext`, `securityContext` | non-root, read-only root FS | |

ICEcube can grant any permission in the cluster, so treat access to it like cluster-admin and
serve it only over TLS.
