//! Lease-based leader election (coordination.k8s.io/v1), so several replicas
//! can serve the API while exactly one runs the controller.
//!
//! Every write goes through `replace` with the Lease's resourceVersion, so two
//! candidates can never both believe they took the same term.

use std::{future::Future, time::Duration};

use jiff::{SignedDuration, Timestamp};
use k8s_openapi::{
    api::coordination::v1::{Lease, LeaseSpec},
    apimachinery::pkg::apis::meta::v1::MicroTime,
};
use kube::{
    Api,
    api::{ObjectMeta, PostParams},
};
use tokio::sync::watch;

use crate::library::{error::AppResult, model::*, state::AppState};

const LEASE_NAME: &str = "icecube-controller";
const LEASE_DURATION: Duration = Duration::from_secs(15);
const RETRY_PERIOD: Duration = Duration::from_secs(2);
/// Give up leadership when the lease could not be renewed for this long, well
/// before another replica may take it over.
const RENEW_DEADLINE: Duration = Duration::from_secs(10);

pub struct Elector {
    api: Api<Lease>,
    identity: String,
}

impl Elector {
    pub fn new(state: &AppState) -> Self {
        let identity = std::env::var("POD_NAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .ok()
            .filter(|s| !s.is_empty())
            .map_or_else(
                || format!("icecube-{}", std::process::id()),
                |host| format!("{host}-{}", Timestamp::now().as_millisecond()),
            );
        Self {
            api: Api::namespaced(state.client.clone(), &state.namespace),
            identity,
        }
    }

    /// Run `work` whenever this replica is the leader. `work` gets a receiver
    /// that flips to `true` when leadership is lost and must then return.
    /// Returns once `shutdown` resolves, releasing the lease if held.
    pub async fn run<F, Fut>(self, work: F, shutdown: impl Future<Output = ()>)
    where
        F: Fn(watch::Receiver<bool>) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        tokio::pin!(shutdown);
        loop {
            // Candidate: wait until the lease is ours.
            loop {
                match self.try_acquire_or_renew().await {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(e) => tracing::warn!(error = %e, "leader election: lease request failed"),
                }
                tokio::select! {
                    () = &mut shutdown => return,
                    () = tokio::time::sleep(RETRY_PERIOD) => {}
                }
            }

            tracing::info!(identity = %self.identity, "became leader, starting controller");
            let (stop_tx, stop_rx) = watch::channel(false);
            let mut task = tokio::spawn(work(stop_rx));
            let mut last_renew = tokio::time::Instant::now();
            let stopping = loop {
                tokio::select! {
                    () = &mut shutdown => break true,
                    _ = &mut task => {
                        tracing::warn!("controller exited unexpectedly; stepping down");
                        break false;
                    }
                    () = tokio::time::sleep(RETRY_PERIOD) => {}
                }
                match self.try_acquire_or_renew().await {
                    Ok(true) => last_renew = tokio::time::Instant::now(),
                    Ok(false) => {
                        tracing::warn!("lease taken by another replica; stepping down");
                        break false;
                    }
                    Err(e) if last_renew.elapsed() > RENEW_DEADLINE => {
                        tracing::warn!(error = %e, "could not renew lease in time; stepping down");
                        break false;
                    }
                    Err(e) => tracing::debug!(error = %e, "lease renewal failed, retrying"),
                }
            };

            let _ = stop_tx.send(true);
            if !task.is_finished() {
                let _ = task.await;
            }
            if stopping {
                self.release().await;
                return;
            }
        }
    }

    /// `true` when this replica holds the lease afterwards.
    async fn try_acquire_or_renew(&self) -> AppResult<bool> {
        let now = MicroTime(Timestamp::now());
        let duration = i32::try_from(LEASE_DURATION.as_secs()).unwrap_or(15);

        let Some(mut lease) = self.api.get_opt(LEASE_NAME).await? else {
            let lease = Lease {
                metadata: ObjectMeta {
                    name: Some(LEASE_NAME.into()),
                    labels: Some([(LABEL_MANAGED_BY.to_string(), MANAGER.to_string())].into()),
                    ..Default::default()
                },
                spec: Some(LeaseSpec {
                    holder_identity: Some(self.identity.clone()),
                    lease_duration_seconds: Some(duration),
                    acquire_time: Some(now.clone()),
                    renew_time: Some(now),
                    lease_transitions: Some(0),
                    ..Default::default()
                }),
            };
            return match self.api.create(&PostParams::default(), &lease).await {
                Ok(_) => Ok(true),
                Err(kube::Error::Api(s)) if s.code == 409 => Ok(false),
                Err(e) => Err(e.into()),
            };
        };

        let spec = lease.spec.get_or_insert_with(Default::default);
        let holder = spec.holder_identity.clone().unwrap_or_default();
        let ours = holder == self.identity;
        if !ours && !holder.is_empty() {
            let held_for = SignedDuration::from_secs(i64::from(
                spec.lease_duration_seconds.unwrap_or(duration),
            ));
            let expired = spec
                .renew_time
                .as_ref()
                .is_none_or(|t| t.0 + held_for < Timestamp::now());
            if !expired {
                return Ok(false);
            }
        }
        if !ours {
            spec.holder_identity = Some(self.identity.clone());
            spec.acquire_time = Some(now.clone());
            spec.lease_transitions = Some(spec.lease_transitions.unwrap_or(0) + 1);
        }
        spec.lease_duration_seconds = Some(duration);
        spec.renew_time = Some(now);
        match self
            .api
            .replace(LEASE_NAME, &PostParams::default(), &lease)
            .await
        {
            Ok(_) => Ok(true),
            Err(kube::Error::Api(s)) if s.code == 409 => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Hand the lease back so another replica takes over without waiting for it to expire.
    async fn release(&self) {
        let Ok(Some(mut lease)) = self.api.get_opt(LEASE_NAME).await else {
            return;
        };
        let Some(spec) = lease.spec.as_mut() else {
            return;
        };
        if spec.holder_identity.as_deref() != Some(self.identity.as_str()) {
            return;
        }
        spec.holder_identity = None;
        spec.renew_time = None;
        match self
            .api
            .replace(LEASE_NAME, &PostParams::default(), &lease)
            .await
        {
            Ok(_) => tracing::info!("released leader lease"),
            Err(e) => tracing::warn!(error = %e, "could not release leader lease"),
        }
    }
}
