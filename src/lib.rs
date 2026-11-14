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
pub mod bank_observation;
pub mod bank_state;
pub mod bank_transactions;
pub mod bank_writes;
pub mod boundary_assessment;
pub mod cache_recovery;
pub mod cli;
pub mod client_hooks;
pub mod clock;
pub mod codec;
pub mod console;
pub mod developer_identity;
pub mod error;
pub mod fs;
pub mod handoff;
pub mod hook_failure;
pub mod host;
pub mod identity_git;
pub mod layout_history;
pub mod layout_recovery_state;
pub mod lifecycle;
pub mod local_registry;
pub mod managed_source;
pub mod migration_codecs;
pub mod migration_contract;
pub mod model_reference;
pub mod omp_installation;
pub mod pack_validation;
pub mod package;
pub mod private_state;
pub mod private_workspace;
pub mod process;
pub mod project_identity;
pub mod records;
#[cfg(feature = "dev-tools")]
#[path = "bin/am_dev/release.rs"]
pub mod release_builder;
pub mod release_distribution;
pub mod remote_identity;
pub mod repository_config;
pub mod scope_binding;
pub mod scopes;
pub mod sharing_hold_mapping;
pub mod sharing_limits;
pub mod tombstones;
pub mod turn_check;
pub mod unicode;
pub mod validation_memo;
