//! The declared command table of `am`, in baseline order.
//!
//! Registration recipe: a ported helper adds `src/cli/helpers/<module>.rs`
//! (parser declaration + typed handler), one `pub mod` line in
//! `helpers/mod.rs`, and one line per public command below at its baseline
//! position. The command surface and its tests follow automatically.

use super::helpers;
use super::{Entry, Route};

const fn route(name: &'static str, helper: &'static super::Helper) -> Entry {
    Entry::Route(Route {
        name,
        helper,
        prefix: &[],
    })
}

/// Baseline `installation.MEMORY_ROUTES` order: developer, project, identity,
/// init, inspect, bank, search, discover, get, explain, context, capture,
/// admission, claude-boundary, codex-boundary, omp-boundary, private, compare,
/// compact, checkpoint, govern, correct, supersede, revoke, forget, publish,
/// refresh, boundary, semantic, sharing, exchange, projection, writes,
/// transaction, migrate; then helper-only commands (script stem with `-`);
/// then installation's own install, update, uninstall, purge-data, recover,
/// version, status, doctor. Unported modules have no line.
pub static ENTRIES: &[Entry] = &[
    route("developer", &helpers::developer_identity::HELPER),
    route("identity", &helpers::scope_binding::HELPER),
    route("bank", &helpers::bank_contract::HELPER),
    route("writes", &helpers::bank_writes::HELPER),
    route("artifact", &helpers::artifact::HELPER),
    route("authority-policy", &helpers::authority::HELPER),
    route("bank-layout", &helpers::bank_layout::HELPER),
    route("lifecycle-model", &helpers::lifecycle::HELPER),
    route("record-contract", &helpers::records::HELPER),
    route("scope-policy", &helpers::scopes::HELPER),
    route("security-policy", &helpers::security::HELPER),
    route("validate-pack", &helpers::pack_validation::HELPER),
];

/// The delegated route named `name`, if registered.
pub fn route_named(name: &str) -> Option<&'static Route> {
    ENTRIES.iter().find_map(|entry| match entry {
        Entry::Route(route) if route.name == name => Some(route),
        _ => None,
    })
}
