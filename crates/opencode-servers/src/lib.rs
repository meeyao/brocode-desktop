//! Multi-server desktop shell state machines, server registry, window tab schema, and legacy
//! persistence migration for Brocode, ported transition for transition from Desktop's
//! `*-machine.ts` (`packages/app/src/runtime/server`, `home/`, `settings/servers/`,
//! `shell/titlebar/`, `session/`):
//!
//! | Machine | Desktop |
//! | --- | --- |
//! | [`controllers`] | `runtime/server/machine.ts` |
//! | [`health`] | `runtime/server/health-machine.ts` |
//! | [`plugin_server`] | `runtime/server/plugin-machine.ts` |
//! | [`sidecar`] | `desktop/src/main/service/background-service-machine.ts` |
//! | [`resolution`] | `session/resolution-machine.ts` |
//! | [`tab_load`] | `shell/titlebar/tab-load-machine.ts` |
//! | [`home`] | `home/machine.ts` |
//! | [`home_load`] | `home/sessions/load-machine.ts` |
//! | [`settings::step`] | `settings/servers/machine.ts` |
//!
//! General protocol/client primitives (`key`, `probe`, `fs`, `link`, `sync`) remain in
//! `opencode_kit::servers` so other embedders never pull desktop shell machines.

#![forbid(unsafe_code)]

pub mod controllers;
pub mod health;
pub mod home;
pub mod home_load;
pub mod migration;
pub mod plugin_server;
pub mod registry;
pub mod resolution;
pub mod settings;
pub mod sidecar;
pub mod tab_load;
pub mod tabs;

pub use opencode_kit::servers::{HttpBase, ProbeResult, Route, ServerKey, ServerScope, TabKey};
pub use registry::{Credentials, ServerConn};
