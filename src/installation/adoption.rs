//! Python-era (receipt v1/v2) recognition. Exact prior bytes and command
//! strings are reconstructed from the receipt's own recorded pack, never
//! executed; anything else is foreign and refused without mutation.
use super::filesystem::same;
use super::{LEGACY_RECEIPT_SCHEMAS, Result, path_text, require, sha};
use crate::client_hooks::{python_era_entries, shell_quote};
use crate::codec::json::{self, Value};
use crate::lifecycle::{get, object, string, text};
use std::path::Path;

/// A Python-era receipt: an adoption input, never written by Rust.
pub fn python_era_kind(receipt: &Value) -> bool {
    text(receipt, "schema").is_some_and(|schema| LEGACY_RECEIPT_SCHEMAS.contains(&schema))
}

/// Baseline `wrapper(pack)`: the exact v0.0.x `bin_dir/{am,agent-memory}` bytes.
pub fn legacy_wrapper(pack: &Path) -> Result<Vec<u8>> {
    let target = shell_quote(path_text(&pack.join("scripts/installer.py"))?);
    let mut text = String::from("#!/bin/sh\nold_ifs=$IFS\nIFS=:\nset -f\nfor dir in $PATH; do\n");
    text.push_str("  [ -n \"$dir\" ] || dir=.\n");
    text.push_str("  candidate=$dir/python3\n");
    text.push_str("  if [ -x \"$candidate\" ] && \"$candidate\" -I -c 'import sys; sys.exit(sys.version_info < (3, 11))' >/dev/null 2>&1; then\n");
    text.push_str("    IFS=$old_ifs\n");
    text.push_str("    exec \"$candidate\" -I -B -c 'import runpy,sys; sys.orig_argv=[sys.executable,sys.argv[1]]; sys.argv=sys.argv[1:]; runpy.run_path(sys.argv.pop(1),run_name=\"__main__\")' \"$0\" ");
    text.push_str(&target);
    text.push_str(" \"$@\"\n");
    text.push_str("  fi\ndone\nIFS=$old_ifs\n");
    text.push_str("printf '%s\\n' 'am: Python 3.11+ must be available as python3 on PATH.' >&2\n");
    text.push_str("exit 1\n");
    Ok(text.into_bytes())
}

/// Python `repr(str)` for the plain paths this wrapper recorded; `None` when
/// the path needs escapes this recogniser does not reproduce (then foreign).
fn python_repr(value: &str) -> Option<String> {
    let plain = value
        .chars()
        .all(|c| !c.is_control() && c != '\'' && c != '\\' && (c.is_ascii() || !c.is_whitespace()));
    plain.then(|| format!("'{value}'"))
}

/// Baseline legacy launcher written when the runtime's commands lacked `am`.
pub fn legacy_source_only_wrapper(pack: &Path) -> Result<Option<Vec<u8>>> {
    let Some(installer) = python_repr(path_text(&pack.join("scripts/installer.py"))?) else {
        return Ok(None);
    };
    let mut text = String::from(
        "#!/usr/bin/env python3\nimport runpy, sys, os, stat, tempfile\nsys.dont_write_bytecode = True\n",
    );
    text.push_str("parent = os.path.realpath(\"/tmp\")\nnode = \"/\"\n");
    text.push_str("for part in parent.split(\"/\"):\n    node = os.path.join(node, part)\n    value = os.lstat(node)\n    if not stat.S_ISDIR(value.st_mode) or value.st_uid != 0 or (value.st_mode & 0o022 and not value.st_mode & stat.S_ISVTX): raise RuntimeError(\"Unsafe source-only startup temporary root\")\n");
    text.push_str("guard = tempfile.TemporaryDirectory(prefix=\"am-source-only-\", dir=parent)\nsys.pycache_prefix = guard.name\n");
    text.push_str(&format!(
        "runpy.run_path({installer}, run_name='__main__')\n"
    ));
    Ok(Some(text.into_bytes()))
}

/// Baseline `omp_installation.extension_wrapper(active_pack, digest, binding)`.
pub fn legacy_extension_wrapper(
    active_pack: &Path,
    digest: &str,
    binding: &Value,
) -> Result<Vec<u8>> {
    let source = json::encode(
        &string(path_text(&active_pack.join("hooks/omp.ts"))?),
        json::DEFAULT,
    )?;
    let options = json::encode(
        &object([
            ("artifact", string(digest)),
            ("agentDir", get(binding, "agent_dir").clone()),
            (
                "readerCapability",
                get(binding, "peer_reader_capability").clone(),
            ),
            ("rolloutAttested", Value::Bool(true)),
        ]),
        json::COMPACT_ASCII,
    )?;
    Ok(format!(
        "// Receipt-owned native executable. Never edit provider or approval settings.\nimport nativeMemory from {source};\nexport default pi => nativeMemory(pi, {options});\n"
    )
    .into_bytes())
}

fn file_identity(raw: &[u8], mode: u32) -> Value {
    object([
        ("kind", string("file")),
        ("hash", string(sha(raw))),
        ("mode", Value::Int(mode.into())),
    ])
}

fn old_pack(old: &Value) -> Result<&Path> {
    text(get(old, "source"), "pack")
        .map(Path::new)
        .ok_or_else(|| {
            crate::error::ContractError::new("Python-era receipt has no recorded pack").into()
        })
}

/// Wrapper row: replace only when the recorded identity is exactly the
/// wrapper this receipt's installer wrote.
pub(crate) fn require_wrapper(previous: &Value, path: &Path, old: &Value) -> Result<()> {
    let pack = old_pack(old)?;
    let identity = get(previous, "identity");
    let binding = get(old, "omp_binding");
    let mut candidates = Vec::new();
    if text(binding, "extension_target") == Some(path_text(path)?) {
        let digest = text(get(old, "source"), "artifact_digest").unwrap_or_default();
        candidates.push(file_identity(
            &legacy_extension_wrapper(pack, digest, binding)?,
            0o644,
        ));
    } else {
        candidates.push(file_identity(&legacy_wrapper(pack)?, 0o755));
        if let Some(raw) = legacy_source_only_wrapper(pack)? {
            candidates.push(file_identity(&raw, 0o755));
        }
    }
    require(
        candidates.iter().any(|candidate| same(candidate, identity)),
        format!(
            "Python-era wrapper differs from the exact recorded wrapper; preserve as foreign: {}",
            path.display()
        ),
    )
}

/// Client hook row: the receipt must own exactly the prior installer's
/// command strings for this client and pack.
pub(crate) fn require_hook(previous: &Value, client: &str, path: &Path, old: &Value) -> Result<()> {
    let (prompt, stop) = python_era_entries(client, old_pack(old)?)?;
    let stop_ok = match previous.get("stop_entry") {
        None => true,
        Some(entry) => same(entry, &stop),
    };
    require(
        same(get(previous, "hook_entry"), &prompt) && stop_ok,
        format!(
            "Python-era client hook command is not the exact installer entry; preserve: {}",
            path.display()
        ),
    )
}

/// OpenCode's prior entry is the exact installer-owned plugin link.
pub(crate) fn require_hook_link(previous: &Value, path: &Path, old: &Value) -> Result<()> {
    let expected = object([
        ("kind", string("link")),
        (
            "target",
            string(path_text(&old_pack(old)?.join("hooks/opencode.js"))?),
        ),
    ]);
    require(
        same(get(previous, "identity"), &expected),
        format!(
            "Python-era OpenCode hook link is not the exact installer entry; preserve: {}",
            path.display()
        ),
    )
}
