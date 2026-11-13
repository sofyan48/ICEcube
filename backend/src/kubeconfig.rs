//! Issue a client certificate for a user through the Kubernetes CSR API and
//! wrap it in a kubeconfig. The private key is generated here, returned once,
//! and never stored; only certificate metadata is recorded in the user's status
//! so the UI can show expiry and re-issue with the same settings.

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
    groups,
    model::*,
    state::AppState,
    users,
};

const SIGNER: &str = "kubernetes.io/kube-apiserver-client";
const MIN_DAYS: u32 = 1;
/// Certificates kept in a user's history; older ones are dropped.
const HISTORY: usize = 10;

/// Lifetime used when nothing was issued before and none is requested.
const DEFAULT_DAYS: u32 = 30;

/// Issue again with the settings of the most recent certificate. Users without
/// any history (new users, or certificates issued before history existed) get
/// the defaults, so this always works as a one-click action.
pub async fn reissue(state: &AppState, user: &str) -> AppResult<KubeconfigDto> {
    let cr = users::get_cr(&state.client, user).await?;
    let last = cr.status.as_ref().and_then(|s| s.certificates.last());
    let input = match last {
        Some(last) => KubeconfigInput {
            namespace: Some(last.namespace.clone()),
            // The maximum may have been lowered since; never exceed it.
            expiration_days: Some(last.requested_days.min(state.config.cert_days)),
        },
        None => KubeconfigInput {
            namespace: None,
            expiration_days: Some(DEFAULT_DAYS.min(state.config.cert_days)),
        },
    };
    issue(state, user, input).await
}

pub async fn issue(
    state: &AppState,
    user: &str,
    input: KubeconfigInput,
) -> AppResult<KubeconfigDto> {
    let cr = users::get_cr(&state.client, user).await?;
    let effective = users::effective_bindings(&cr, &groups::list_crs(&state.client).await?);
    if effective.is_empty() {
        return Err(AppError::BadRequest(format!(
            "user '{user}' has no access; grant something before issuing a kubeconfig"
        )));
    }

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
        .or_else(|| effective.iter().find_map(|b| b.namespace.clone()))
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
    let certificate = describe(&cert_pem, namespace, days)?;
    record(state, user, &certificate).await?;
    tracing::info!(user, days, serial = %certificate.serial, expires_at = %certificate.expires_at, "kubeconfig issued");
    Ok(KubeconfigDto {
        kubeconfig: yaml,
        certificate,
    })
}

/// Read serial and expiry from the signed certificate.
fn describe(
    cert_pem: &[u8],
    namespace: String,
    requested_days: u32,
) -> AppResult<IssuedCertificate> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(cert_pem)
        .map_err(|e| AppError::internal(format!("signed certificate is not PEM: {e}")))?;
    let cert = pem
        .parse_x509()
        .map_err(|e| AppError::internal(format!("signed certificate is invalid: {e}")))?;
    let ts = |t: i64| {
        jiff::Timestamp::from_second(t)
            .map(|t| t.to_string())
            .map_err(AppError::internal)
    };
    Ok(IssuedCertificate {
        serial: cert.raw_serial_as_string(),
        // Not `notBefore`: the Kubernetes signer backdates it by a few minutes
        // to tolerate clock skew.
        issued_at: jiff::Timestamp::now()
            .round(jiff::Unit::Second)
            .map_err(AppError::internal)?
            .to_string(),
        expires_at: ts(cert.validity().not_after.timestamp())?,
        namespace,
        requested_days,
    })
}

/// Append to the user's certificate history (status subresource).
async fn record(state: &AppState, user: &str, cert: &IssuedCertificate) -> AppResult<()> {
    let api: Api<IcecubeUser> = Api::all(state.client.clone());
    // Re-read right before writing to keep the window for concurrent issues small.
    let current = api.get_status(user).await?;
    let mut certificates = current.status.map(|s| s.certificates).unwrap_or_default();
    certificates.push(cert.clone());
    let skip = certificates.len().saturating_sub(HISTORY);
    certificates.drain(..skip);
    api.patch_status(
        user,
        &PatchParams::default(),
        &Patch::Merge(json!({ "status": { "certificates": certificates } })),
    )
    .await?;
    Ok(())
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
        // Plain `get` already includes status and only needs the `get` verb on
        // certificatesigningrequests (no access to the /status subresource).
        let csr = api.get(name).await?;
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
