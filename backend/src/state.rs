use std::sync::Arc;

use anyhow::Context;
use base64::Engine;
use kube::Client;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    pub client: Client,
    pub config: Config,
    /// API server URL embedded into generated kubeconfigs.
    pub server_url: String,
    /// Cluster CA as PEM, embedded into generated kubeconfigs.
    pub ca_pem: Option<String>,
}

impl std::ops::Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let kube_config = kube::Config::infer()
            .await
            .context("cannot load Kubernetes config (in-cluster or ~/.kube/config)")?;

        let ca_pem = match &kube_config.root_cert_file {
            Some(path) => Some(
                std::fs::read_to_string(path)
                    .with_context(|| format!("reading CA bundle {}", path.display()))?,
            ),
            None => kube_config
                .root_cert
                .as_ref()
                .map(|certs| certs.iter().map(|der| der_to_pem(der)).collect::<String>()),
        };

        let server_url = config
            .control_plane_address
            .clone()
            .unwrap_or_else(|| kube_config.cluster_url.to_string())
            .trim_end_matches('/')
            .to_string();

        let client = Client::try_from(kube_config).context("building Kubernetes client")?;

        Ok(Self(Arc::new(Inner {
            client,
            config,
            server_url,
            ca_pem,
        })))
    }
}

fn der_to_pem(der: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut out = String::from("-----BEGIN CERTIFICATE-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).expect("base64 is ascii"));
        out.push('\n');
    }
    out.push_str("-----END CERTIFICATE-----\n");
    out
}
