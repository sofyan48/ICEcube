//! Business logic. Each module works on Kubernetes objects and returns DTOs;
//! none of it depends on axum.

pub mod controller;
pub mod discovery;
pub mod group;
pub mod kubeconfig;
pub mod template;
pub mod user;
