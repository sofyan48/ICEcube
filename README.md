# ICEcube

[![CI](https://github.com/sofyan48/ICEcube/actions/workflows/ci.yaml/badge.svg)](https://github.com/sofyan48/ICEcube/actions/workflows/ci.yaml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A web UI for giving people access to your Kubernetes cluster.

Create a user, choose what they can do and where, and hand them a ready-to-use kubeconfig.
You don't need to write any RBAC YAML.

## Features

- **Users:** create people and give them access per namespace or across the whole cluster.
- **Groups:** bundle access once and reuse it. New users join the `default` group automatically,
  so they can start right away.
- **Permission templates:** `viewer`, `developer`, `namespace-lister` and `cluster-viewer` are
  ready to use, and you can create your own.
- **Kubeconfig in one click:** download a kubeconfig for any user. When it is about to expire,
  re-issue it with one click.
- **Instant changes:** change a user's or a group's access and it applies immediately. Delete a
  user and their access is gone.

## Install

You need a Kubernetes cluster (1.30+), `kubectl` with admin access, and Helm 3.8+.

**1. Find your cluster's address.** Users will connect to this address with their kubeconfig:

```sh
kubectl config view --minify -o jsonpath='{.clusters[0].cluster.server}'; echo
```

**2. Install ICEcube.** Replace the address with the one from step 1:

```sh
helm install icecube oci://ghcr.io/sofyan48/charts/icecube \
  --namespace icecube --create-namespace \
  --set clusterName=my-cluster \
  --set controlPlaneAddress=https://k8s.example.com:6443
```

**3. Get the admin password:**

```sh
kubectl -n icecube get secret icecube-auth \
  -o jsonpath='{.data.BASIC_AUTH_PASSWORD}' | base64 -d; echo
```

**4. Open ICEcube:**

```sh
kubectl -n icecube port-forward svc/icecube 4000:80
```

Go to http://localhost:4000 and sign in as `admin` with the password from step 3.

### Open it on a domain

To reach ICEcube on your own domain instead of using port-forward, enable the ingress. Always
use HTTPS:

```sh
helm upgrade icecube oci://ghcr.io/sofyan48/charts/icecube -n icecube --reuse-values \
  --set ingress.enabled=true \
  --set ingress.className=nginx \
  --set 'ingress.hosts[0].host=icecube.example.com' \
  --set 'ingress.hosts[0].paths[0].path=/' \
  --set 'ingress.hosts[0].paths[0].pathType=Prefix' \
  --set 'ingress.tls[0].secretName=icecube-tls' \
  --set 'ingress.tls[0].hosts[0]=icecube.example.com'
```

## How to use

### Give someone access

1. Open **Users** and click **New user**.
2. Enter a username, for example `jane`. The `default` group is already selected, so Jane can
   already see the cluster in read-only mode.
3. Optional: open **Direct access** to give extra rights, for example `developer` in the
   `team-a` namespace.
4. Click **Create user**.

### Send them a kubeconfig

1. Click **Kubeconfig** next to the user.
2. Choose the default namespace and how many days it should be valid.
3. Click **Issue kubeconfig**, then **Download**.
4. Send the file to the user. They can use it right away:

   ```sh
   kubectl --kubeconfig jane.kubeconfig get pods
   ```

The file contains a private key. Send it privately, the same way you would send a password.

### Renew a kubeconfig

A badge next to each user shows when their kubeconfig expires. When it is about to expire, a
yellow **Re-issue** button appears. Click it to get a new kubeconfig with the same settings.
Use **Needs re-issue** at the top of the list to see everyone who needs a new one.

### Give many people the same access

1. Open **Groups** and click **New group**, for example `backend-team`.
2. Choose what the group can access.
3. Add users to the group when you create or edit them.

When you change a group, all of its members get the new access immediately. Turn on **Default
for new users** to add every new user to the group automatically.

### Create your own permission

Open **Templates** and click **New template**. Choose which resources (for example `pods`,
`deployments`) and which actions (`get`, `list`, `create`, ...) it allows. A preview shows exactly
what will be created.

### Remove access

- **Remove part of someone's access:** edit the user and untick it.
- **Remove all of someone's access:** delete the user.

Either change applies immediately.

## Good to know

- **Treat ICEcube like an admin account.** Anyone who can sign in can give anyone access to
  anything. Keep the password safe and use HTTPS.
- **Keep kubeconfigs short-lived.** A kubeconfig that was already sent cannot be cancelled. It
  stops working when the user loses access or when it expires. Short lifetimes (for example 7–30
  days) are safest, and renewing takes one click.
- **Users don't depend on ICEcube.** Kubeconfigs keep working even if ICEcube is down, and
  restarting ICEcube loses nothing.

## Upgrade and uninstall

```sh
# upgrade
helm upgrade icecube oci://ghcr.io/sofyan48/charts/icecube -n icecube --reuse-values

# uninstall (users and their access stay in the cluster)
helm uninstall icecube -n icecube
```

To also remove every user, group and the access they were given:

```sh
kubectl delete crd icecubeusers.icecube.io icecubegroups.icecube.io
kubectl delete clusterrole,clusterrolebinding -l app.kubernetes.io/managed-by=icecube
kubectl delete rolebinding -A -l app.kubernetes.io/managed-by=icecube
kubectl delete namespace icecube
```

## Troubleshooting

| Problem | What to do |
| --- | --- |
| Install fails with `controlPlaneAddress is required` | Add `--set controlPlaneAddress=...` (step 1 of Install). |
| The ICEcube pod does not start (`ImagePullBackOff`) | Check that the cluster can reach `ghcr.io`. |
| A kubeconfig cannot be issued | Your cluster must allow client certificates. Self-hosted clusters (kubeadm, k3s, RKE2) do by default; some managed services don't. |
| A kubeconfig gives `Unauthorized` | `controlPlaneAddress` must be the address of the Kubernetes API itself, not a proxy in front of it. |

To see what ICEcube is doing:

```sh
kubectl -n icecube logs deploy/icecube
```

## More

- All install options: [`charts/icecube/values.yaml`](charts/icecube/values.yaml)
- How it works, API and development: [`docs/development.md`](docs/development.md)

## License

[MIT](LICENSE)
