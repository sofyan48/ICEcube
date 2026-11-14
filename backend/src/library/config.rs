use std::net::SocketAddr;

use anyhow::{Context, bail};

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub cluster_name: String,
    /// Public API server URL written into generated kubeconfigs.
    /// Falls back to the URL the backend itself uses when unset.
    pub control_plane_address: Option<String>,
    pub auth_user: String,
    pub auth_password: String,
    /// Longest lifetime of an issued certificate or token, in days.
    pub cert_days: u32,
    /// Run the controller with Lease-based leader election, so several
    /// replicas can run. Off: this process always runs the controller.
    pub leader_election: bool,
    /// Install/upgrade the CRDs on startup. Turn off when they are managed
    /// elsewhere (Helm `crds/`, GitOps); the backend then only waits for them.
    pub install_crds: bool,
    /// Keep each user's latest kubeconfig in a Secret so it can be viewed and
    /// downloaded again later.
    pub store_kubeconfigs: bool,
    /// Namespace for those Secrets. Defaults to the namespace ICEcube runs in
    /// (or the kubeconfig context's namespace when run locally).
    pub namespace: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let listen = env_or("ICECUBE_LISTEN", "0.0.0.0:4000")
            .parse()
            .context("ICECUBE_LISTEN must be host:port")?;

        let auth_password = std::env::var("BASIC_AUTH_PASSWORD").unwrap_or_default();
        if auth_password.is_empty() {
            bail!("BASIC_AUTH_PASSWORD must be set");
        }

        let cert_days = env_or("ICECUBE_CERT_DAYS", "30")
            .parse()
            .context("ICECUBE_CERT_DAYS must be a number")?;

        Ok(Self {
            listen,
            cluster_name: env_or("CLUSTER_NAME", "kubernetes"),
            control_plane_address: std::env::var("CONTROL_PLANE_ADDRESS")
                .ok()
                .filter(|s| !s.is_empty()),
            auth_user: env_or("BASIC_AUTH_USER", "admin"),
            auth_password,
            cert_days,
            leader_election: env_bool("ICECUBE_LEADER_ELECTION", true),
            install_crds: env_bool("ICECUBE_INSTALL_CRDS", true),
            store_kubeconfigs: env_bool("ICECUBE_STORE_KUBECONFIGS", true),
            namespace: std::env::var("ICECUBE_NAMESPACE")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key).map(|v| v.to_ascii_lowercase()) {
        Ok(v) if matches!(v.as_str(), "false" | "0" | "no") => false,
        Ok(v) if matches!(v.as_str(), "true" | "1" | "yes") => true,
        _ => default,
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}
