//! Issue a credential for a user and wrap it in a kubeconfig: a client
//! certificate through the CSR API, or a ServiceAccount token through the
//! TokenRequest API, depending on the user's `credential`. Metadata is
//! recorded in the user's status (expiry, re-issue settings); the kubeconfig
//! itself is kept in a Secret by `kubeconfig_store` unless that is disabled.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use k8s_openapi::{
    ByteString,
    api::{
        authentication::v1::{TokenRequest, TokenRequestSpec},
        certificates::v1::{
            CertificateSigningRequest, CertificateSigningRequestCondition,
            CertificateSigningRequestSpec,
        },
        core::v1::ServiceAccount,
    },
};
use kube::api::{Api, DeleteParams, ObjectMeta, Patch, PatchParams, PostParams};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use serde_json::json;

use crate::{
    library::{
        error::{AppError, AppResult},
        kubeconfig_store as store,
        model::*,
        state::AppState,
    },
    usecase::{group as groups, user as users},
};

const SIGNER: &str = "kubernetes.io/kube-apiserver-client";
const MIN_DAYS: u32 = 1;
/// Certificates kept in a user's history; older ones are dropped.
const HISTORY: usize = 10;

/// Lifetime used when nothing was issued before and none is requested.
const DEFAULT_DAYS: u32 = 30;

/// The user's last issued kubeconfig, when storage is enabled.
pub async fn stored(state: &AppState, user: &str) -> AppResult<KubeconfigDto> {
    store::load(state, user).await
}

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
    if let Some(reason) = cr.status.as_ref().and_then(|s| s.blocked.as_deref()) {
        return Err(AppError::Conflict(format!(
            "user '{user}' is blocked: {reason}"
        )));
    }
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

    let (auth, certificate) = match cr.spec.credential {
        CredentialKind::Certificate => issue_certificate(state, user, &namespace, days).await?,
        CredentialKind::ServiceAccount => issue_token(state, user, &namespace, days).await?,
    };

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
        "users": [{ "name": user, "user": auth }],
        "contexts": [{
            "name": context,
            "context": { "cluster": cluster, "user": user, "namespace": namespace }
        }],
        "current-context": context,
    });

    let yaml = serde_saphyr::to_string(&kubeconfig).map_err(AppError::internal)?;
    record(state, user, &certificate).await?;
    let issued = KubeconfigDto {
        kubeconfig: yaml,
        certificate,
    };
    // The credential is already valid; failing to keep a copy must not hide
    // it from the admin who just asked for it.
    if let Err(e) = store::save(state, &cr, &issued).await {
        tracing::warn!(user, error = %e, "could not store kubeconfig");
    }
    tracing::info!(user, days, kind = ?issued.certificate.kind, serial = %issued.certificate.serial, expires_at = %issued.certificate.expires_at, "kubeconfig issued");
    Ok(issued)
}

/// Client certificate through the CSR API. Returns the kubeconfig `user` entry.
async fn issue_certificate(
    state: &AppState,
    user: &str,
    namespace: &str,
    days: u32,
) -> AppResult<(serde_json::Value, IssuedCertificate)> {
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
    let certificate = describe(&cert_pem, namespace.to_string(), days)?;
    let auth = json!({
        "client-certificate-data": B64.encode(&cert_pem),
        "client-key-data": B64.encode(key.serialize_pem()),
    });
    Ok((auth, certificate))
}

/// Bound token of the user's ServiceAccount through the TokenRequest API.
/// Deleting the ServiceAccount (revoke) invalidates it immediately.
async fn issue_token(
    state: &AppState,
    user: &str,
    namespace: &str,
    days: u32,
) -> AppResult<(serde_json::Value, IssuedCertificate)> {
    let api: Api<ServiceAccount> = Api::namespaced(state.client.clone(), &state.namespace);
    let sa = service_account_name(user);
    // The controller creates it; wait briefly right after a user was created
    // or revoked.
    let mut found = false;
    for _ in 0..20 {
        if api.get_opt(&sa).await?.is_some() {
            found = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    if !found {
        return Err(AppError::Conflict(format!(
            "ServiceAccount {}/{sa} does not exist yet; is the ICEcube controller running?",
            state.namespace
        )));
    }

    let request = TokenRequest {
        metadata: Default::default(),
        // No audiences: the API server's own are used.
        spec: Some(TokenRequestSpec {
            expiration_seconds: Some(i64::from(days) * 86_400),
            ..Default::default()
        }),
        status: None,
    };
    let response = api
        .create_token_request(&sa, &PostParams::default(), &request)
        .await?;
    let status = response.status.unwrap_or_default();
    let (Some(token), Some(expires)) = (status.token, status.expiration_timestamp) else {
        return Err(AppError::internal("TokenRequest returned no token"));
    };
    let now = jiff::Timestamp::now()
        .round(jiff::Unit::Second)
        .map_err(AppError::internal)?;
    let certificate = IssuedCertificate {
        // Tokens have no serial; the issue time tells history entries apart.
        serial: format!("token-{}", now.as_millisecond()),
        issued_at: now.to_string(),
        // The API server may cap the lifetime (--service-account-max-token-expiration).
        expires_at: expires
            .0
            .round(jiff::Unit::Second)
            .map_err(AppError::internal)?
            .to_string(),
        namespace: namespace.to_string(),
        requested_days: days,
        kind: CredentialKind::ServiceAccount,
        revoked_at: None,
    };
    Ok((json!({ "token": token }), certificate))
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
        kind: CredentialKind::Certificate,
        revoked_at: None,
    })
}

/// Append to the user's certificate history (status subresource).
async fn record(state: &AppState, user: &str, cert: &IssuedCertificate) -> AppResult<()> {
    let api: Api<IcecubeUser> = Api::all(state.client.clone());
    // Re-read right before writing to keep the window for concurrent issues small.
    let current = api.get_status(user).await?;
    let status = current.status.unwrap_or_default();
    // Track the latest certificate expiry separately, so dropping old entries
    // from the history never shortens how long a deleted name stays blocked.
    let valid_until = status
        .certificates
        .iter()
        .filter(|c| c.kind == CredentialKind::Certificate)
        .map(|c| c.expires_at.as_str())
        .chain(status.certificates_valid_until.as_deref())
        .chain((cert.kind == CredentialKind::Certificate).then_some(cert.expires_at.as_str()))
        .filter_map(|s| s.parse::<jiff::Timestamp>().ok())
        .max()
        .map(|t| t.to_string());
    let mut certificates = status.certificates;
    certificates.push(cert.clone());
    let skip = certificates.len().saturating_sub(HISTORY);
    certificates.drain(..skip);
    api.patch_status(
        user,
        &PatchParams::default(),
        &Patch::Merge(json!({ "status": {
            "certificates": certificates,
            "certificatesValidUntil": valid_until,
        } })),
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
