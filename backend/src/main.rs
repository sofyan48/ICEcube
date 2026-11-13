mod api;
mod config;
mod error;
mod groups;
mod kubeconfig;
mod model;
mod state;
mod templates;
mod users;

use std::time::Duration;

use anyhow::Context;
use axum::{Router, routing::get};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    Api, CustomResourceExt, Resource,
    api::{Patch, PatchParams},
};
use serde::de::DeserializeOwned;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

use crate::{
    config::Config,
    model::{IcecubeGroup, IcecubeUser},
    state::AppState,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "icecube=info,tower_http=info".into()),
        )
        .init();

    // `icecube crd` prints the CRDs so they can be applied ahead of time (e.g. from a Helm chart).
    if std::env::args().nth(1).as_deref() == Some("crd") {
        println!("{}---", serde_saphyr::to_string(&IcecubeUser::crd())?);
        print!("{}", serde_saphyr::to_string(&IcecubeGroup::crd())?);
        return Ok(());
    }

    let config = Config::from_env()?;
    let state = AppState::new(config.clone()).await?;

    ensure_crd::<IcecubeUser>(&state).await?;
    ensure_crd::<IcecubeGroup>(&state).await?;
    templates::seed_builtins(&state.client)
        .await
        .context("seeding builtin templates")?;
    groups::seed_default(&state.client)
        .await
        .context("seeding the default group")?;
    users::migrate_legacy(&state.client)
        .await
        .context("migrating users to spec bindings")?;

    let index = config.static_dir.join("index.html");
    let spa = ServeDir::new(&config.static_dir).fallback(ServeFile::new(index));

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api", api::router(state.clone()))
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    tracing::info!(
        listen = %config.listen,
        cluster = %config.cluster_name,
        server = %state.server_url,
        "ICEcube is ready"
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn ensure_crd<K>(state: &AppState) -> anyhow::Result<()>
where
    K: CustomResourceExt + Resource<DynamicType = ()> + Clone + std::fmt::Debug + DeserializeOwned,
{
    let crd = K::crd();
    let name = crd.metadata.name.clone().unwrap_or_default();
    if state.config.install_crds {
        let crds: Api<CustomResourceDefinition> = Api::all(state.client.clone());
        crds.patch(
            &name,
            &PatchParams::apply(model::MANAGER).force(),
            &Patch::Apply(&crd),
        )
        .await
        .with_context(|| format!("installing CRD {name}"))?;
    }

    // Wait until the API server serves the new type.
    let api: Api<K> = Api::all(state.client.clone());
    for _ in 0..30 {
        if api.list(&Default::default()).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if state.config.install_crds {
        anyhow::bail!("CRD {name} did not become available")
    }
    anyhow::bail!(
        "CRD {name} is not installed (ICECUBE_INSTALL_CRDS=false); apply it first, e.g. `icecube crd | kubectl apply -f -`"
    )
}
