mod handler;
mod library;
mod route;
mod usecase;

use std::time::Duration;

use anyhow::Context;

use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    Api, CustomResourceExt, Resource,
    api::{Patch, PatchParams},
};
use serde::de::DeserializeOwned;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

use tokio::sync::watch;

use crate::{
    handler::ui,
    library::{
        config::Config,
        leader::Elector,
        model::{self, IcecubeGroup, IcecubeUser},
        state::AppState,
    },
    usecase::{controller, group, template, user},
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
    template::seed_builtins(&state.client)
        .await
        .context("seeding builtin templates")?;
    group::seed_default(&state.client)
        .await
        .context("seeding the default group")?;
    user::migrate_legacy(&state.client)
        .await
        .context("migrating users to spec bindings")?;

    let app = route::app(state.clone())
        // gzip/brotli for the UI bundle and JSON; skips tiny and already-compressed bodies.
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    let controller_task = tokio::spawn(run_controller(state.clone(), shutdown_rx.clone()));

    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    tracing::info!(
        listen = %config.listen,
        cluster = %config.cluster_name,
        server = %state.server_url,
        leader_election = config.leader_election,
        ui_files = ui::file_count(),
        "ICEcube is ready"
    );
    let mut stop = shutdown_rx;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = stop.wait_for(|s| *s).await;
        })
        .await?;
    let _ = controller_task.await;
    Ok(())
}

/// Reconcile IcecubeUsers until shutdown; with leader election only while
/// this replica holds the lease.
async fn run_controller(state: AppState, shutdown: watch::Receiver<bool>) {
    if !state.config.leader_election {
        controller::run(state, shutdown).await;
        return;
    }
    let mut stop = shutdown.clone();
    Elector::new(&state)
        .run(
            move |lost| {
                let state = state.clone();
                let shutdown = shutdown.clone();
                async move { controller::run(state, either(lost, shutdown)).await }
            },
            async move {
                let _ = stop.wait_for(|s| *s).await;
            },
        )
        .await;
}

/// A receiver that turns `true` as soon as either input does.
fn either(mut a: watch::Receiver<bool>, mut b: watch::Receiver<bool>) -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(*a.borrow() || *b.borrow());
    tokio::spawn(async move {
        tokio::select! {
            _ = a.wait_for(|v| *v) => {}
            _ = b.wait_for(|v| *v) => {}
            () = tx.closed() => return,
        }
        let _ = tx.send(true);
    });
    rx
}

/// Ctrl+C locally, SIGTERM when Kubernetes stops the pod.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = term => {},
    }
    tracing::info!("shutting down");
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
