//! Session prompt queue strip and durable prompt outbox for Brocode.
//!
//! - [`queue`]: queued/steering prompt strip above the composer, drag-to-reorder and
//!   edit-in-composer state machines, queue rewrite chains, and inbox SSE/listing reconciliation
//!   over the object-safe [`queue::QueueHost`] boundary.
//! - [`send_failed`]: durable prompt outbox, backoff pump, delivery receipt checks, and
//!   per-message send status footers over the object-safe [`send_failed::SendFailedHost`]
//!   boundary.

#![forbid(unsafe_code)]

pub mod queue;
pub mod send_failed;
