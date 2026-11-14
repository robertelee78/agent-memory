//! Every registration point in the crate is accounted for: it is either
//! called by the binary's composition root (`cli::register_providers`) or
//! listed in `tests/wiring-pending.txt` with the module it waits for.
//! A new `register_*` function therefore cannot ship unwired by accident.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const ROOT: &str = "src/cli/mod.rs";
const PENDING: &str = "tests/wiring-pending.txt";

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// `src/bank_writes.rs` -> `bank_writes`; `src/cli/mod.rs` -> `cli`;
/// `src/fs/lock.rs` -> `fs::lock`.
fn module_path(file: &Path) -> String {
    let relative = file.strip_prefix(manifest().join("src")).unwrap();
    let mut parts: Vec<String> = relative
        .with_extension("")
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.last().is_some_and(|last| last == "mod") {
        parts.pop();
    }
    parts.join("::")
}

fn identifier_at(text: &str, start: usize) -> &str {
    let end = text[start..]
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(text.len(), |offset| start + offset);
    &text[start..end]
}

/// `module::register_name` for every `fn register_*` defined under `src/`,
/// except the composition root itself and the developer-only binary.
fn registration_points() -> BTreeSet<String> {
    let mut files = Vec::new();
    sources(&manifest().join("src"), &mut files);
    let mut points = BTreeSet::new();
    for file in files {
        let module = module_path(&file);
        if module.starts_with("bin::") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for (offset, _) in text.match_indices("fn register_") {
            let name = identifier_at(&text, offset + "fn ".len());
            if module == "cli" && name == "register_providers" {
                continue;
            }
            points.insert(format!("{module}::{name}"));
        }
    }
    points
}

/// Registrations the composition root calls, as `module::register_name`.
fn wired() -> BTreeSet<String> {
    let text = std::fs::read_to_string(manifest().join(ROOT)).unwrap();
    let start = text.find("pub fn register_providers").unwrap();
    let end = start + text[start..].find("\n}\n").unwrap();
    let body = &text[start..end];
    let mut calls = BTreeSet::new();
    for (offset, _) in body.match_indices("::register_") {
        let name = identifier_at(body, offset + "::".len());
        let module_start = body[..offset]
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
            .map_or(0, |index| index + 1);
        let module = body[module_start..offset].trim_start_matches("crate::");
        calls.insert(format!("{module}::{name}"));
    }
    calls
}

/// `module::register_name` -> the modules it waits for.
fn pending() -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(manifest().join(PENDING)).unwrap();
    let mut listed = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (point, reason) = line
            .split_once(char::is_whitespace)
            .unwrap_or_else(|| panic!("{PENDING}: `{line}` names no module to wait for"));
        let reason = reason.trim();
        assert!(
            reason.starts_with("waits for "),
            "{PENDING}: `{line}` must say `waits for <modules>`"
        );
        assert!(
            listed.insert(point.to_owned(), reason.to_owned()).is_none(),
            "{PENDING}: `{point}` is listed twice"
        );
    }
    listed
}

#[test]
fn every_registration_point_is_wired_or_listed_as_pending() {
    let points = registration_points();
    let wired = wired();
    let pending = pending();
    let listed: BTreeSet<String> = pending.keys().cloned().collect();

    let unknown_calls: Vec<_> = wired.difference(&points).collect();
    assert!(
        unknown_calls.is_empty(),
        "composition root calls registrations that do not exist: {unknown_calls:?}"
    );
    let stale: Vec<_> = listed.difference(&points).collect();
    assert!(
        stale.is_empty(),
        "{PENDING} lists registrations that no longer exist: {stale:?}"
    );
    let both: Vec<_> = wired.intersection(&listed).collect();
    assert!(
        both.is_empty(),
        "wired registrations are still listed as pending: {both:?}"
    );
    let unaccounted: Vec<_> = points
        .iter()
        .filter(|point| !wired.contains(*point) && !listed.contains(*point))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "registration points neither wired in {ROOT} nor listed in {PENDING}: {unaccounted:?}"
    );
}

#[test]
fn a_pending_registration_names_a_module_that_is_not_ported_yet() {
    let present: BTreeSet<String> = {
        let text = std::fs::read_to_string(manifest().join("src/lib.rs")).unwrap();
        text.lines()
            .filter_map(|line| line.trim().strip_prefix("pub mod "))
            .map(|name| name.trim_end_matches(';').to_owned())
            .collect()
    };
    let mut ready = Vec::new();
    for (point, reason) in pending() {
        let waits: Vec<&str> = reason
            .trim_start_matches("waits for ")
            .split(',')
            .map(str::trim)
            .collect();
        if waits.iter().all(|module| present.contains(*module)) {
            ready.push(format!("{point} ({reason})"));
        }
    }
    assert!(
        ready.is_empty(),
        "every module these registrations wait for now exists; wire them: {ready:?}"
    );
}

/// Release gate: nothing may still be pending when the port is complete.
#[test]
#[ignore = "cutover gate: run when every module is ported"]
fn no_registration_is_pending() {
    let pending = pending();
    assert!(pending.is_empty(), "still pending: {pending:?}");
}
