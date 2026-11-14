//! Reconciles IcecubeUsers with kube-runtime, so the CRDs are the only source
//! of truth: users and groups applied with kubectl/GitOps get their bindings,
//! and bindings or ServiceAccounts removed by hand are recreated.
//!
//! Triggers: any IcecubeUser change, any change to an owned RoleBinding,
//! ClusterRoleBinding or ServiceAccount, group changes (members only),
//! namespace changes (a binding may wait for its namespace) and a periodic
//! resync.
//!
//! A finalizer revokes access when a user is deleted and remembers the name
//! while certificates issued to it are still valid (`tombstone`).

use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use k8s_openapi::api::{
    core::v1::{Namespace, ServiceAccount},
    rbac::v1::{ClusterRoleBinding, RoleBinding},
};
use kube::{
    Api, ResourceExt,
    api::{Patch, PatchParams},
    runtime::{
        Controller,
        controller::{self, Action},
        finalizer::{self, Event},
        reflector::{ObjectRef, Store},
        watcher,
    },
};
use serde_json::json;
use tokio::sync::watch;

use crate::{
    library::{
        error::{AppError, AppResult},
        kubeconfig_store,
        model::*,
        state::AppState,
        tombstone,
    },
    usecase::{group as groups, user as users},
};

const FINALIZER: &str = "icecube.io/cleanup";
const RESYNC: Duration = Duration::from_secs(300);
const RETRY: Duration = Duration::from_secs(15);

struct Ctx {
    state: AppState,
}

/// Run the controller until `stop` flips to `true`.
pub async fn run(state: AppState, mut stop: watch::Receiver<bool>) {
    let client = state.client.clone();
    let managed = watcher::Config::default().labels(&format!("{LABEL_MANAGED_BY}={MANAGER}"));

    let controller = Controller::new(
        Api::<IcecubeUser>::all(client.clone()),
        watcher::Config::default(),
    )
    .with_config(controller::Config::default().concurrency(4))
    .owns(Api::<RoleBinding>::all(client.clone()), managed.clone())
    .owns(
        Api::<ClusterRoleBinding>::all(client.clone()),
        managed.clone(),
    )
    .owns(
        Api::<ServiceAccount>::namespaced(client.clone(), &state.namespace),
        managed,
    );
    let store = controller.store();

    let members = store.clone();
    let by_ns = store.clone();
    controller
        .watches(
            Api::<IcecubeGroup>::all(client.clone()),
            watcher::Config::default(),
            move |group| members_of(&members, &group.name_any()),
        )
        .watches(
            Api::<Namespace>::all(client.clone()),
            watcher::Config::default(),
            move |_| all_users(&by_ns),
        )
        .graceful_shutdown_on(async move {
            let _ = stop.wait_for(|stop| *stop).await;
        })
        .run(reconcile, error_policy, Arc::new(Ctx { state }))
        .for_each(|res| async move {
            match res {
                Ok((obj, _)) => tracing::debug!(user = %obj.name, "reconciled"),
                Err(controller::Error::ObjectNotFound(_)) => {}
                Err(e) => tracing::warn!(error = %e, "reconcile failed"),
            }
        })
        .await;
    tracing::info!("controller stopped");
}

fn members_of(store: &Store<IcecubeUser>, group: &str) -> Vec<ObjectRef<IcecubeUser>> {
    store
        .state()
        .iter()
        .filter(|u| u.spec.groups.iter().any(|g| g == group))
        .map(|u| ObjectRef::from_obj(u.as_ref()))
        .collect()
}

fn all_users(store: &Store<IcecubeUser>) -> Vec<ObjectRef<IcecubeUser>> {
    store
        .state()
        .iter()
        .map(|u| ObjectRef::from_obj(u.as_ref()))
        .collect()
}

async fn reconcile(
    user: Arc<IcecubeUser>,
    ctx: Arc<Ctx>,
) -> Result<Action, finalizer::Error<AppError>> {
    let api: Api<IcecubeUser> = Api::all(ctx.state.client.clone());
    finalizer::finalizer(&api, FINALIZER, user, |event| async {
        match event {
            Event::Apply(user) => apply(&ctx.state, &user).await,
            Event::Cleanup(user) => cleanup(&ctx.state, &user).await,
        }
    })
    .await
}

fn error_policy(user: Arc<IcecubeUser>, err: &finalizer::Error<AppError>, _: Arc<Ctx>) -> Action {
    tracing::warn!(user = %user.name_any(), error = %err, "reconcile failed, retrying");
    Action::requeue(RETRY)
}

async fn apply(state: &AppState, user: &IcecubeUser) -> AppResult<Action> {
    let name = user.name_any();

    if let Some(until) = reuse_block(state, user).await? {
        users::revoke_all(state, user).await?;
        let reason = format!(
            "username was deleted earlier and certificates issued to it are valid until {until}; \
             no access is granted until then. Set annotation {ANNO_ALLOW_NAME_REUSE}=\"true\" to accept \
             the old certificates, or use credential: serviceAccount"
        );
        set_blocked(state, user, Some(&reason)).await?;
        tracing::warn!(user = %name, %until, "username reuse blocked");
        let wait = (until - jiff::Timestamp::now())
            .try_into()
            .unwrap_or(RESYNC)
            .min(RESYNC);
        return Ok(Action::requeue(wait + Duration::from_secs(1)));
    }
    set_blocked(state, user, None).await?;

    match user.spec.credential {
        CredentialKind::ServiceAccount => users::ensure_service_account(state, user).await?,
        CredentialKind::Certificate => users::delete_service_account(state, &name).await?,
    }
    let groups = groups::list_crs(&state.client).await?;
    users::reconcile(state, user, &groups).await?;
    Ok(Action::requeue(RESYNC))
}

/// `Some(until)` when the user must not be bound because certificates of a
/// deleted user with the same name are still valid.
async fn reuse_block(state: &AppState, user: &IcecubeUser) -> AppResult<Option<jiff::Timestamp>> {
    // Only `User` subjects match those certificates; ServiceAccount users are safe.
    if user.spec.credential != CredentialKind::Certificate {
        return Ok(None);
    }
    let name = user.name_any();
    let Some(until) = tombstone::active(state, &name).await? else {
        return Ok(None);
    };
    if user
        .annotations()
        .get(ANNO_ALLOW_NAME_REUSE)
        .map(String::as_str)
        == Some("true")
    {
        tombstone::clear(state, &name).await?;
        tracing::warn!(user = %name, %until, "username reused on purpose; old certificates are valid again");
        return Ok(None);
    }
    Ok(Some(until))
}

async fn set_blocked(state: &AppState, user: &IcecubeUser, reason: Option<&str>) -> AppResult<()> {
    let current = user.status.as_ref().and_then(|s| s.blocked.as_deref());
    if current == reason {
        return Ok(());
    }
    let api: Api<IcecubeUser> = Api::all(state.client.clone());
    api.patch_status(
        &user.name_any(),
        &PatchParams::default(),
        &Patch::Merge(json!({ "status": { "blocked": reason } })),
    )
    .await?;
    Ok(())
}

async fn cleanup(state: &AppState, user: &IcecubeUser) -> AppResult<Action> {
    let name = user.name_any();
    users::revoke_all(state, user).await?;
    if let Some(until) = users::cert_valid_until(user).filter(|t| *t > jiff::Timestamp::now()) {
        tombstone::record(state, &name, until).await?;
    }
    kubeconfig_store::delete(state, &name).await?;
    tracing::info!(user = %name, "user removed, access revoked");
    Ok(Action::await_change())
}
