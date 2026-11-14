//! Shared constants, DTOs and validation helpers.

use k8s_openapi::api::rbac::v1::PolicyRule;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::library::error::{AppError, AppResult};

pub const MANAGER: &str = "icecube";
pub const LABEL_MANAGED_BY: &str = "app.kubernetes.io/managed-by";
pub const LABEL_USER: &str = "icecube.io/user";
pub const LABEL_TEMPLATE: &str = "icecube.io/template";
pub const LABEL_TEMPLATE_REF: &str = "icecube.io/template-ref";
pub const LABEL_BUILTIN: &str = "icecube.io/builtin";
pub const ANNO_SCOPE: &str = "icecube.io/scope";
pub const ANNO_DESCRIPTION: &str = "icecube.io/description";
pub const TEMPLATE_PREFIX: &str = "icecube-template-";
pub const RBAC_GROUP: &str = "rbac.authorization.k8s.io";

/// A user managed by ICEcube. Cluster scoped; it owns all of the user's bindings
/// so deleting it lets the garbage collector clean up whatever is left.
///
/// The spec is the source of truth: the user's RoleBindings/ClusterRoleBindings
/// are always derived from `bindings` plus the bindings of every group in `groups`.
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "icecube.io",
    version = "v1alpha1",
    kind = "IcecubeUser",
    plural = "icecubeusers",
    shortname = "icu",
    status = "IcecubeUserStatus",
    printcolumn = r#"{"name":"Description","type":"string","jsonPath":".spec.description"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct IcecubeUserSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Direct grants. `None` only on users created before groups existed; they
    /// are migrated on startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bindings: Option<Vec<BindingDto>>,
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Kept in the status subresource so spec edits (access changes) and certificate
/// bookkeeping never overwrite each other.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IcecubeUserStatus {
    /// Most recent last. Only metadata: private keys are never stored.
    #[serde(default)]
    pub certificates: Vec<IssuedCertificate>,
}

/// Bookkeeping for one issued client certificate. Times are RFC 3339 and
/// `expiresAt` is read from the signed certificate, so it reflects any cap
/// the cluster signer applied.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuedCertificate {
    pub serial: String,
    pub issued_at: String,
    pub expires_at: String,
    /// Default namespace written into that kubeconfig.
    pub namespace: String,
    /// Lifetime that was requested, reused when re-issuing.
    pub requested_days: u32,
}

/// A reusable set of grants. Members get every binding of the group.
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "icecube.io",
    version = "v1alpha1",
    kind = "IcecubeGroup",
    plural = "icecubegroups",
    shortname = "icg",
    printcolumn = r#"{"name":"Default","type":"boolean","jsonPath":".spec.defaultForNewUsers"}"#,
    printcolumn = r#"{"name":"Description","type":"string","jsonPath":".spec.description"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct IcecubeGroupSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub bindings: Vec<BindingDto>,
    /// New users join this group unless the request lists groups explicitly.
    #[serde(default)]
    pub default_for_new_users: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Bound per namespace (RoleBinding) or across all namespaces (ClusterRoleBinding).
    Namespaced,
    /// Cluster-scoped resources; only ever bound with a ClusterRoleBinding.
    Cluster,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Namespaced => "namespaced",
            Scope::Cluster => "cluster",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "namespaced" => Some(Scope::Namespaced),
            "cluster" => Some(Scope::Cluster),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDto {
    pub name: String,
    pub cluster_role: String,
    pub scope: Scope,
    pub description: String,
    pub builtin: bool,
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInput {
    pub name: String,
    pub scope: Scope,
    #[serde(default)]
    pub description: String,
    pub rules: Vec<PolicyRule>,
}

/// One grant: a template bound in a namespace, or cluster-wide when `namespace` is `None`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct BindingDto {
    pub template: String,
    #[serde(default)]
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub name: String,
    pub description: Option<String>,
    pub created_at: Option<String>,
    /// Direct grants only; the UI combines them with the groups' grants.
    pub bindings: Vec<BindingDto>,
    pub groups: Vec<String>,
    /// Issued certificates, newest first.
    pub certificates: Vec<IssuedCertificate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserInput {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub bindings: Vec<BindingDto>,
    /// `None` means "the default groups".
    #[serde(default)]
    pub groups: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserInput {
    #[serde(default)]
    pub description: Option<String>,
    pub bindings: Vec<BindingDto>,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupDto {
    pub name: String,
    pub description: Option<String>,
    pub bindings: Vec<BindingDto>,
    pub default_for_new_users: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInput {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub bindings: Vec<BindingDto>,
    #[serde(default)]
    pub default_for_new_users: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeconfigInput {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub expiration_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeconfigDto {
    pub kubeconfig: String,
    pub certificate: IssuedCertificate,
}

/// DNS-1123 label: lowercase alphanumerics and '-', max 63 chars.
/// Kept strict so the value is also usable as a label value and in object names.
pub fn validate_name(what: &str, s: &str) -> AppResult<()> {
    let ok = !s.is_empty()
        && s.len() <= 63
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "invalid {what} name '{s}': use lowercase letters, digits and '-' (max 63 chars)"
        )))
    }
}

pub fn template_cluster_role(template: &str) -> String {
    format!("{TEMPLATE_PREFIX}{template}")
}

pub fn binding_name(user: &str, template: &str) -> String {
    format!("icecube-{user}-{template}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_dns_labels() {
        for ok in ["a", "jane-doe", "team1", &"x".repeat(63)] {
            assert!(validate_name("user", ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn rejects_invalid_names() {
        for bad in [
            "",
            "Jane",
            "-a",
            "a-",
            "a_b",
            "a.b",
            "a/b",
            "system:admin",
            &"x".repeat(64),
        ] {
            assert!(validate_name("user", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn binding_dto_uses_camel_case_and_optional_namespace() {
        let b: BindingDto = serde_json::from_str(r#"{"template":"viewer"}"#).unwrap();
        assert_eq!(
            b,
            BindingDto {
                template: "viewer".into(),
                namespace: None
            }
        );
        let s: Scope = serde_json::from_str(r#""cluster""#).unwrap();
        assert_eq!(s, Scope::Cluster);
    }
}
