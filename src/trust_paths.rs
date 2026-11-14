//! Read-only Unix lifecycle path policy, shared with descriptor walkers.
//!
//! The foundation already owns this policy and its pinned traversal. Keep one
//! implementation: this module is the baseline domain's public entry point.
pub use crate::fs::trust::{
    InspectOptions, Inspection, TreeInspection, git_pointer, inspect, inspect_entry, record,
    refuse, source, state, tree,
};
