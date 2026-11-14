//! HTTP handlers: extract the request, call a usecase, shape the response.

pub mod auth;
pub mod discovery;
pub mod group;
pub mod kubeconfig;
pub mod namespace;
pub mod session;
pub mod template;
pub mod ui;
pub mod user;
