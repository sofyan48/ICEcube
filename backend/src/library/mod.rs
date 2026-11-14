//! Shared building blocks: configuration, errors, models, application state
//! and low-level storage helpers. Nothing here knows about HTTP routes.

pub mod config;
pub mod error;
pub mod kubeconfig_store;
pub mod model;
pub mod state;
