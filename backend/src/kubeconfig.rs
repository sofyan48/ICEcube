//! Issue a client certificate for a user through the Kubernetes CSR API and
//! wrap it in a kubeconfig. The private key is generated here, returned once,
//! and never stored.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use k8s_openapi::{
    ByteString,
    api::certificates::v1::{
        CertificateSigningRequest, CertificateSigningRequestCondition,
        CertificateSigningRequestSpec,
    },
};
use kube::api::{Api, DeleteParams, ObjectMeta, Patch, PatchParams, PostParams};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    model::*,
    state::AppState,
    users,
};

const SIGNER: &str = "kubernetes.io/kube-apiserver-client";
const MIN_DAYS: u32 = 1;

pub async fn issue(
    state: &AppState,
    user: &str,
    input: KubeconfigInput,
) -> AppResult<KubeconfigDto> {
    let dto = users::get(&state.client, user).await?;

    let days = input.expiration_days.unwrap_or(state.config.cert_days);
    if days < MIN_DAYS || days > state.config.cert_days {
        return Err(AppError::BadRequest(format!(
            "expirationDays must be between {MIN_DAYS} and {}",
            state.config.cert_days
        )));
    }

    // Default to the first namespace the user can access so `kubectl get pods` works out of the box.
    let namespace = input
        .namespace
        .filter(|n| !n.is_empty())
        .or_else(|| dto.bindings.iter().find_map(|b| b.namespace.clone()))
        .unwrap_or_else(|| "default".to_string());
    validate_name("namespace", &namespace)?;

    let key = KeyPair::generate().map_err(AppError::internal)?;
    let mut params = CertificateParams::new(Vec::<String>::new()).map_err(AppError::internal)?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, user);
    params.distinguished_name = dn;
    let csr_pem = params
        .serialize_request(&key)
        .and_then(|c| c.pem())
        .map_err(AppError::internal)?;

    let cert_pem = sign(state, user, csr_pem.into_bytes(), days).await?;

    let cluster = state.config.cluster_name.clone();
    let context = format!("{user}@{cluster}");
    let mut cluster_entry = json!({ "server": state.server_url });
    match &state.ca_pem {
        Some(ca) => cluster_entry["certificate-authority-data"] = json!(B64.encode(ca)),
        None => cluster_entry["insecure-skip-tls-verify"] = json!(true),
    }
    let kubeconfig = json!({
        "apiVersion": "v1",
        "kind": "Config",
        "clusters": [{ "name": cluster, "cluster": cluster_entry }],
        "users": [{
            "name": user,
            "user": {
                "client-certificate-data": B64.encode(&cert_pem),
                "client-key-data": B64.encode(key.serialize_pem()),
            }
        }],
        "contexts": [{
            "name": context,
            "context": { "cluster": cluster, "user": user, "namespace": namespace }
        }],
        "current-context": context,
    });

    let yaml = serde_saphyr::to_string(&kubeconfig).map_err(AppError::internal)?;
    let expires_at = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_hours(24 * i64::from(days)))
        .ok()
        .map(|t| t.to_string());
    tracing::info!(user, days, "kubeconfig issued");
    Ok(KubeconfigDto {
        kubeconfig: yaml,
        expires_at,
    })
}

async fn sign(state: &AppState, user: &str, csr_pem: Vec<u8>, days: u32) -> AppResult<Vec<u8>> {
    let api: Api<CertificateSigningRequest> = Api::all(state.client.clone());
    let name = format!("icecube-{user}-{}", jiff::Timestamp::now().as_millisecond());

    let csr = CertificateSigningRequest {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(
                [
                    (LABEL_MANAGED_BY.to_string(), MANAGER.to_string()),
                    (LABEL_USER.to_string(), user.to_string()),
                ]
                .into(),
            ),
            ..Default::default()
        },
        spec: CertificateSigningRequestSpec {
            request: ByteString(csr_pem),
            signer_name: SIGNER.into(),
            expiration_seconds: i32::try_from(u64::from(days) * 86_400).ok(),
            usages: Some(vec![
                "client auth".into(),
                "digital signature".into(),
                "key encipherment".into(),
            ]),
            ..Default::default()
        },
        status: None,
    };
    api.create(&PostParams::default(), &csr).await?;

    let result = approve_and_wait(&api, &name).await;
    // The certificate has been copied out (or failed); the CSR object is no longer useful.
    if let Err(e) = api.delete(&name, &DeleteParams::default()).await {
        tracing::warn!(csr = %name, error = %e, "failed to clean up CSR");
    }
    result
}

async fn approve_and_wait(api: &Api<CertificateSigningRequest>, name: &str) -> AppResult<Vec<u8>> {
    let approval = json!({
        "status": {
            "conditions": [CertificateSigningRequestCondition {
                type_: "Approved".into(),
                status: "True".into(),
                reason: Some("IcecubeApproved".into()),
                message: Some("Approved by ICEcube".into()),
                ..Default::default()
            }]
        }
    });
    api.patch_approval(name, &PatchParams::default(), &Patch::Merge(&approval))
        .await?;

    for _ in 0..30 {
        let csr = api.get_status(name).await?;
        if let Some(status) = csr.status {
            if let Some(cond) = status
                .conditions
                .iter()
                .flatten()
                .find(|c| c.type_ == "Denied" || c.type_ == "Failed")
            {
                return Err(AppError::internal(format!(
                    "CSR {}: {}",
                    cond.type_,
                    cond.message.clone().unwrap_or_default()
                )));
            }
            if let Some(cert) = status.certificate.filter(|c| !c.0.is_empty()) {
                return Ok(cert.0);
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(AppError::internal(
        "timed out waiting for the cluster to sign the certificate (is the kube-controller-manager signer enabled?)",
    ))
}
