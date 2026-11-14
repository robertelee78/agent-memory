//! Rust-only payload audit (ADR-0029, Gate C): the repository tree and the
//! portable release payload hold no Python source, bytecode, cache, interpreter
//! launcher or interpreter shebang. Only the listed historical evidence and the
//! refusal/recognition code may name the prior launcher in text.

use agent_memory::release_distribution::{self as release, Inventory};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const MAX_LISTING: usize = 8 * 1024 * 1024;
const MAX_TREE_FILE: u64 = 64 * 1024 * 1024;
/// Bytes read from an allowlisted file: enough for its first (shebang) line.
const HEAD: u64 = 256;
/// Text that would route a consumer into an interpreter.
const LAUNCHER_TEXT: [&str; 2] = ["python3", "#!/usr/bin/env python"];

/// Paths whose text may truthfully name the prior launcher. An entry ending in
/// `/` or `-` is a prefix; any other entry is one exact file.
const ALLOWED_TEXT: &[(&str, &str)] = &[
    ("docs/research/", "historical research evidence"),
    ("data/", "historical research data"),
    ("memory-bank/", "historical project memory"),
    ("docs/adr/", "ADR history"),
    ("docs/design/v0.0.1-", "design history before v0.0.15"),
    ("docs/design/v0.0.3-", "design history before v0.0.15"),
    ("docs/design/v0.0.14-", "design history before v0.0.15"),
    (
        "docs/design/v0.0.15-omp-integration.md",
        "pre-cutover v0.0.15 design history",
    ),
    (
        "docs/design/v0.0.15-rust-cutover.md",
        "the cutover design naming the Python-era state it recognises",
    ),
    (
        "docs/testing/observed-guide-examples.json",
        "retained capture of the superseded runtime, documented as historical",
    ),
    (
        "src/pack_validation.rs",
        "pack-validation refusal of Python launcher strings",
    ),
    ("tests/t2_cli.rs", "pack-validation refusal vectors"),
    (
        "tests/c_test_read_only_audits.rs",
        "pack-validation refusal vectors",
    ),
    (
        "src/installation/adoption.rs",
        "recognises the exact Python-era wrapper bytes for adoption",
    ),
    (
        "src/client_hooks.rs",
        "recognises the exact Python-era hook commands for adoption",
    ),
    (
        "tests/fixtures/v014-state/",
        "captured Python-era installation state (adoption inputs)",
    ),
    (
        "tests/t2_hooks.rs",
        "Python-era hook command adoption vectors",
    ),
    (
        "tests/t2_installation.rs",
        "Python-era hook command adoption vectors",
    ),
    (
        "tests/t2_leaves.rs",
        "Python-era hook command adoption vectors",
    ),
    (
        "tests/t2_diagnostics.rs",
        "foreign Python hook entries preserved by diagnostics",
    ),
    ("tests/payload_audit.rs", "this audit's own patterns"),
];

/// Released-version notes and the historical section of the current notes.
const RELEASE_NOTES: &str = "docs/releases/v0.0.15.md";
const HISTORICAL_SECTION: &str = "\n## Superseded Python candidate record (historical)\n";

fn python_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/').any(|part| part == "__pycache__")
        || [".py", ".pyc", ".pyo", ".pyd", ".pyw"]
            .iter()
            .any(|extension| name.ends_with(extension))
        || matches!(name, "python" | "python3")
        || (name.starts_with("python") && name.ends_with(".sh"))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let first = *needle.first()?;
    haystack
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == first)
        .map(|(index, _)| index)
        .find(|index| {
            haystack
                .get(*index..index + needle.len())
                .is_some_and(|window| window == needle)
        })
}

fn python_shebang(raw: &[u8]) -> bool {
    raw.starts_with(b"#!")
        && raw
            .split(|byte| *byte == b'\n')
            .next()
            .is_some_and(|line| find(line, b"python").is_some())
}

fn launcher_text(raw: &[u8]) -> Option<&'static str> {
    LAUNCHER_TEXT
        .iter()
        .copied()
        .find(|text| find(raw, text.as_bytes()).is_some())
}

fn allowed_text(path: &str) -> bool {
    ALLOWED_TEXT.iter().any(|(entry, _)| {
        if entry.ends_with('/') || entry.ends_with('-') {
            path.starts_with(entry)
        } else {
            path == *entry
        }
    }) || (path.starts_with("docs/releases/") && path != RELEASE_NOTES)
}

/// Launcher text outside the allowlist, or a Python shebang anywhere.
fn content_finding(path: &str, raw: &[u8]) -> Option<String> {
    if python_shebang(raw) {
        return Some(format!("{path}: Python interpreter shebang"));
    }
    if allowed_text(path) {
        return None;
    }
    let active = if path == RELEASE_NOTES {
        find(raw, HISTORICAL_SECTION.as_bytes()).map_or(raw, |end| &raw[..end])
    } else {
        raw
    };
    launcher_text(active).map(|text| format!("{path}: names `{text}`"))
}

fn read_bounded(path: &Path, limit: u64) -> Vec<u8> {
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .unwrap()
        .take(limit + 1)
        .read_to_end(&mut raw)
        .unwrap();
    raw
}

/// Every file Git would commit from this checkout (tracked or untracked but
/// not ignored, present on disk); a copy without Git metadata audits every file.
fn tree(root: &Path) -> Vec<String> {
    if std::fs::symlink_metadata(root.join(".git")).is_err() {
        let mut files = Vec::new();
        walk(root, "", &mut files);
        return files;
    }
    let output = Command::new("git")
        .args([
            "-c",
            "core.quotePath=false",
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success(), "git ls-files failed");
    assert!(output.stdout.len() <= MAX_LISTING, "tree listing too large");
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8(name.to_vec()).unwrap())
        .filter(|name| std::fs::symlink_metadata(root.join(name)).is_ok())
        .collect()
}

fn walk(root: &Path, relative: &str, files: &mut Vec<String>) {
    for entry in std::fs::read_dir(root.join(relative)).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let path = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        if relative.is_empty() && matches!(name.as_str(), "target" | ".git") {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            walk(root, &path, files);
        } else {
            files.push(path);
        }
    }
}

fn audit_inventory(files: &Inventory) -> Vec<String> {
    let mut findings = Vec::new();
    for (path, (raw, _)) in files {
        if python_path(path) {
            findings.push(format!("{path}: Python path"));
        }
        if python_shebang(raw) {
            findings.push(format!("{path}: Python interpreter shebang"));
        }
        if let Some(text) = launcher_text(raw) {
            findings.push(format!("{path}: names `{text}`"));
        }
    }
    findings
}

#[test]
fn repository_tree_holds_no_python_payload() {
    let root = Path::new(ROOT);
    let files = tree(root);
    assert!(
        files.iter().any(|path| path == "Cargo.toml"),
        "tree listing"
    );
    let mut findings = Vec::new();
    for path in &files {
        if python_path(path) {
            findings.push(format!("{path}: Python path"));
        }
        let metadata = std::fs::symlink_metadata(root.join(path)).unwrap();
        if !metadata.is_file() {
            continue;
        }
        let limit = if allowed_text(path) {
            HEAD
        } else {
            MAX_TREE_FILE
        };
        let raw = read_bounded(&root.join(path), limit);
        assert!(
            raw.len() as u64 <= MAX_TREE_FILE,
            "{path} exceeds the bound"
        );
        findings.extend(content_finding(path, &raw));
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn portable_pack_and_built_release_archive_hold_no_python_payload() {
    let pack = Path::new(ROOT).join("skills/agent-memory");
    let inventory = release::portable_files(&pack).unwrap();
    assert!(inventory.contains_key("SKILL.md"));
    let findings = audit_inventory(&inventory);
    assert!(findings.is_empty(), "{findings:#?}");
    // The archive exactly as the release builder encodes the tracked pack.
    let members: Inventory = inventory
        .into_iter()
        .map(|(path, entry)| (format!("{}{path}", release::PREFIX), entry))
        .collect();
    let archive = release::encode_archive(&members).unwrap();
    let mut expanded = Vec::new();
    flate2::read::GzDecoder::new(archive.as_slice())
        .take((release::MAX_TAR + 1) as u64)
        .read_to_end(&mut expanded)
        .unwrap();
    let entries = release::tar_entries(&expanded).unwrap();
    assert_eq!(entries, members);
    let findings = audit_inventory(&entries);
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn audit_detects_every_python_payload_form() {
    for path in [
        "skills/agent-memory/scripts/installer.py",
        "a/__pycache__/x.cpython-311.pyc",
        "tests/x.pyo",
        "skills/agent-memory/hooks/python3.sh",
        "bin/python3",
    ] {
        assert!(python_path(path), "{path}");
    }
    for path in [
        "tests/obligations/python-baseline-v1.json",
        "src/pack_validation.rs",
        "skills/agent-memory/scope/GUIDE.md",
    ] {
        assert!(!python_path(path), "{path}");
    }
    assert!(content_finding("x.sh", b"#!/usr/bin/python3 -I\n").is_some());
    assert!(content_finding("docs/research/x.txt", b"#!/usr/bin/env python3\n").is_some());
    assert!(content_finding("README.md", b"run python3 -I x\n").is_some());
    assert!(content_finding("docs/research/x.md", b"run python3 -I x\n").is_none());
    let notes = format!("# v0.0.15\n\nam only\n{HISTORICAL_SECTION}\nold `python3 -I`\n");
    assert!(content_finding(RELEASE_NOTES, notes.as_bytes()).is_none());
    let stale = format!("# v0.0.15\n\nuse python3\n{HISTORICAL_SECTION}");
    assert!(content_finding(RELEASE_NOTES, stale.as_bytes()).is_some());
    assert!(content_finding("docs/releases/v0.0.14.md", b"python3 -I").is_none());
}
