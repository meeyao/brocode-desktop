//! Workspace tooling subsystems for Brocode:
//! - [`new_worktree`]: new-session worktree selection, location cycling, and branch typeahead picker.
//! - [`rune_mcp`]: per-folder `rune` client MCP WebSocket lifecycle, multi-window hub, and session call routing.
//! - [`home_search`]: Home session search state machine, exact session-ID lookup, list hold, and key handling.
//! - [`fulltext`]: local SQLite FTS message-body search surfaces for the command palette and Home.
//!
//! All crate entrypoints operate on non-generic `&dyn *Host` / `&mut dyn *Host` trait objects,
//! keeping thin `H: *Host + 'static` generic wrappers strictly at `rune::El` view callback boundaries.

#![forbid(unsafe_code)]

pub mod fulltext;
pub mod home_project_sessions;
pub mod home_search;
pub mod new_worktree;
pub mod rune_mcp;

pub use fulltext::FulltextHost;
pub use home_search::HomeSearchHost;
pub use new_worktree::NewWorktreeHost;
pub use rune_mcp::RuneMcpHost;
