//! Core session engine for baude: PTY management, Claude Code session
//! metadata, state persistence, and git worktree helpers. No UI dependencies —
//! shared by the `baude` TUI and the `bauded` daemon.

pub use vt100;

pub mod backend;
pub mod bridge;
pub mod git;
pub mod hook;
pub mod meta;
pub mod permission;
pub mod persist;
pub mod pty;
pub mod session;
pub mod workspace;
