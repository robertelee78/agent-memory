//! agent-memory: repository-owned Markdown memory for coding agents.
//!
//! `clock`, `codec`, `error`, `fs` and `process` are the foundation: every
//! domain module reaches the system through them and none may weaken a
//! refusal defined there.

pub mod artifact;
pub mod authority;
pub mod bank_capacity;
pub mod bank_contract;
pub mod bank_layout;
pub mod bank_writes;
pub mod boundary_assessment;
pub mod clock;
pub mod codec;
pub mod developer_identity;
pub mod error;
pub mod fs;
pub mod handoff;
pub mod host;
pub mod identity_git;
pub mod lifecycle;
pub mod local_registry;
pub mod model_reference;
pub mod omp_installation;
pub mod pack_validation;
pub mod process;
pub mod records;
pub mod remote_identity;
pub mod repository_config;
pub mod scopes;
pub mod sharing_limits;
pub mod unicode;
pub mod validation_memo;
