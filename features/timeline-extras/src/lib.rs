//! Cohesive session timeline interaction docks and live row helpers for Brocode:
//! - [`todos`]: agent plan pill, floating todo list, and completion fill/strike animations over [`TodosHost`].
//! - [`permission`]: permission/form reply state machines, timeout orchestration, and wire-error recovery over [`PermissionHost`].
//! - [`question`]: question dock host boundary, reply state machine, and long-question fixtures over [`QuestionHost`].
//! - [`agent_rows`]: live subagent step log, steered-call windowing, and shared run clock over [`AgentRowsHost`].
//! - [`auto_accept`]: global and per-project permission auto-accept rules and toggle toast over [`AutoAcceptHost`].
//! - [`shell_fold`] and [`short_group`]: timeline capture transcript builders.
//!
//! All crate entrypoints operate on non-generic `&dyn *Host` / `&mut dyn *Host` boundaries,
//! keeping thin `H: ... + 'static` wrappers strictly at `rune::El` callback boundaries.

#![forbid(unsafe_code)]

pub mod agent_rows;
pub mod auto_accept;
pub mod permission;
pub mod question;
pub mod shell_fold;
pub mod short_group;
pub mod todos;

pub use agent_rows::AgentRowsHost;
pub use auto_accept::AutoAcceptHost;
pub use permission::PermissionHost;
pub use question::QuestionHost;
pub use todos::TodosHost;
