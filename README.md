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

### 1. Find your cluster's address

Users will connect to this address with their kubeconfig. Usually it is the same address you use:

```sh
kubectl config view --minify -o jsonpath='{.clusters[0].cluster.server}'; echo
```

It must be reachable by your users, for example `https://k8s.example.com:6443`, not an internal
address like `https://10.0.0.1:443`.

### 2. Fill in your settings

Create a file named `values.yaml` and fill it in:

```yaml
# ── Required ────────────────────────────────────────────────────────────

# The address from step 1. Written into every kubeconfig ICEcube creates.
controlPlaneAddress: https://k8s.example.com:6443

# ── Recommended ─────────────────────────────────────────────────────────

# A name for this cluster. Users see it in their kubeconfig (e.g. jane@production).
clusterName: production

# Longest time (in days) a kubeconfig can be valid. Shorter is safer;
# renewing takes one click.
maxCertDays: 30

# Sign-in for the ICEcube page.
auth:
  username: admin
  password: ""        # leave empty to generate a random password (see step 4)

# ── Optional: open ICEcube on your own domain ───────────────────────────
# Leave this out to open it with port-forward instead (step 5).
ingress:
  enabled: true
  className: nginx                       # your ingress controller
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt   # if you use cert-manager
  hosts:
    - host: icecube.example.com          # your domain
      paths:
        - path: /
          pathType: Prefix
  tls:                                   # always use HTTPS
    - secretName: icecube-tls
      hosts: [icecube.example.com]
```

What each setting means:

| Setting | Needed? | What to put |
| --- | --- | --- |
| `controlPlaneAddress` | **Required** | Your cluster's address from step 1. Install fails without it. |
| `clusterName` | Recommended | A short name for the cluster, like `production` or `staging`. Default: `kubernetes`. |
| `maxCertDays` | Recommended | Maximum kubeconfig lifetime in days (certificates and tokens). Default: `30`. |
| `auth.username` | Optional | Sign-in name. Default: `admin`. |
| `auth.password` | Optional | Sign-in password. Leave empty to generate one. |
| `ingress` | Optional | Only if you want ICEcube on a domain. Set `enabled: true`, your domain in both `hosts` and `tls`, and your controller in `className`. |
| `storeKubeconfigs` | Optional | `true` (default) lets you download a user's kubeconfig again later. `false` shows it only once. |

You don't need to change any other setting. The full list, with defaults, is in
[`charts/icecube/values.yaml`](charts/icecube/values.yaml).

### 3. Install ICEcube

```sh
helm install icecube oci://ghcr.io/sofyan48/charts/icecube \
  --namespace icecube --create-namespace \
  -f values.yaml
```

### 4. Get the admin password

If you left `auth.password` empty, read the generated one:

```sh
kubectl -n icecube get secret icecube-auth \
  -o jsonpath='{.data.BASIC_AUTH_PASSWORD}' | base64 -d; echo
```

### 5. Open ICEcube

- **With a domain (ingress):** go to `https://icecube.example.com`.
- **Without a domain:** run the command below, then go to http://localhost:4000.

  ```sh
  kubectl -n icecube port-forward svc/icecube 4000:80
  ```

Sign in with the username and password from your settings, or `admin` and the password from
step 4.

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

ICEcube keeps the latest kubeconfig of each user. To download it again later, just click
**View kubeconfig**, even after a refresh or from another computer. You don't need to issue a new one.

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
- **Saved kubeconfigs live in the `icecube` namespace.** Only cluster admins should be able to
  read Secrets there. To turn saving off, install with `--set storeKubeconfigs=false`. A
  kubeconfig is then only shown right after it is issued.

## Upgrade and uninstall

To change a setting, edit your `values.yaml` and run the same command as `upgrade`. Do the same
to update ICEcube to a new version:

```sh
# upgrade, or apply changed settings
helm upgrade icecube oci://ghcr.io/sofyan48/charts/icecube -n icecube -f values.yaml

# uninstall (users and their access stay in the cluster)
helm uninstall icecube -n icecube
```

Keep your `values.yaml` somewhere safe, for example in your infrastructure repository, so you
can upgrade with the same settings. If it contains a password, don't commit it to a public
repository.

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
| Install fails with `controlPlaneAddress is required` | Set `controlPlaneAddress` in your `values.yaml` (Install, step 2). |
| The page doesn't open on your domain | Check `ingress.className` matches your ingress controller, and that your domain points to it. |
| The ICEcube pod does not start (`ImagePullBackOff`) | Check that the cluster can reach `ghcr.io`. |
| A kubeconfig cannot be issued | Your cluster must allow client certificates. Self-hosted clusters (kubeadm, k3s, RKE2) do by default; some managed services don't. |
| A kubeconfig gives `Unauthorized` | `controlPlaneAddress` must be the address of the Kubernetes API itself, not a proxy in front of it. |

To see what ICEcube is doing:

```sh
kubectl -n icecube logs deploy/icecube
```

## More

- All settings, with defaults: [`charts/icecube/values.yaml`](charts/icecube/values.yaml)
- How it works, API and development: [`docs/development.md`](docs/development.md)

## License

[MIT](LICENSE)
