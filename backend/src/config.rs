use std::{net::SocketAddr, path::PathBuf};

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
    pub static_dir: PathBuf,
    pub cert_days: u32,
    /// Install/upgrade the CRDs on startup. Turn off when they are managed
    /// elsewhere (Helm `crds/`, GitOps); the backend then only waits for them.
    pub install_crds: bool,
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

        let cert_days = env_or("ICECUBE_CERT_DAYS", "365")
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
            static_dir: env_or("ICECUBE_STATIC_DIR", "../frontend/dist").into(),
            cert_days,
            install_crds: !matches!(
                env_or("ICECUBE_INSTALL_CRDS", "true")
                    .to_ascii_lowercase()
                    .as_str(),
                "false" | "0" | "no"
            ),
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}
