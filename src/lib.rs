//! agent-memory: repository-owned Markdown memory for coding agents.
//!
//! `clock`, `codec`, `error`, `fs` and `process` are the foundation: every
//! domain module reaches the system through them and none may weaken a
//! refusal defined there.

pub mod boundary_assessment;
pub mod clock;
pub mod codec;
pub mod error;
pub mod fs;
pub mod identity_git;
pub mod lifecycle;
pub mod model_reference;
pub mod process;
pub mod remote_identity;
pub mod validation_memo;
