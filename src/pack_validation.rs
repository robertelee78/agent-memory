//! Read-only portable instruction validation. Candidate Python is never parsed or executed.
use crate::codec::{
    frontmatter as fm,
    json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value},
};
use crate::error::ContractError;
use crate::fs::{Descriptor, DirectoryChain};
use crate::lifecycle::{get, object, string, text};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::{Arc, RwLock};

pub const SECTIONS: [&str; 11] = [
    "Use when",
    "Required context",
    "Inputs and preconditions",
    "Observe",
    "Plan",
    "Apply",
    "Read back",
    "Verify",
    "Outcomes",
    "Failure handling",
    "Diagnostic fallback",
];
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub code: String,
    pub path: String,
    pub detail: String,
}
impl Invalid {
    pub fn new(code: &str, path: &str, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            path: path.into(),
            detail: detail.into(),
        }
    }
    pub fn issue(&self) -> Value {
        object([
            ("code", string(&self.code)),
            ("path", string(&self.path)),
            ("detail", string(&self.detail)),
        ])
    }
}
impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}
impl std::error::Error for Invalid {}
impl From<Invalid> for ContractError {
    fn from(e: Invalid) -> Self {
        Self::new(e.detail)
    }
}
pub type Result<T, E = Invalid> = std::result::Result<T, E>;
pub fn require(condition: bool, code: &str, path: &str, detail: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Invalid::new(code, path, detail))
    }
}
fn malformed(e: impl std::fmt::Display) -> Invalid {
    Invalid::new("malformed-pack", ".", e.to_string())
}
pub(crate) fn sha256(bytes: &[u8]) -> String {
    crate::validation_memo::hex(&Sha256::digest(bytes))
}
fn whitespace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}
pub(crate) fn strip(s: &str) -> &str {
    s.trim_matches(whitespace)
}
// Slicing invariant for the Markdown/shell scanners in this file (splitlines,
// fence, reference_spans, attr, inline_mask, link_occurrences, strip_tags,
// anchors, url_parts, words, remove_inline, sentences, marker_ranges,
// link_token, mandatory_targets, command_argv, normative, inline_python,
// declared_assets): every bound is either a `char_indices` position or a
// `find`/byte-scan position of an ASCII delimiter in the same string (or in its
// byte-for-byte equal-length mask), so it is in range and on a char boundary.
// tests/t1_a2.rs `malformed_input_contract` exercises them with generated text.
pub(crate) fn splitlines(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut iter = s.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            lines.push(&s[start..i]);
            start = i + c.len_utf8();
            if c == '\r' {
                if let Some((lf, _)) = iter.next_if(|(_, c)| *c == '\n') {
                    start = lf + 1;
                }
            }
        }
    }
    if start < s.len() {
        lines.push(&s[start..]);
    }
    lines
}
pub fn semantic_version(value: &Value) -> bool {
    let Some(s) = value.as_str() else {
        return false;
    };
    let (core_pre, build) = s.split_once('+').map_or((s, None), |(a, b)| (a, Some(b)));
    let valid_ids = |v: &str| {
        !v.is_empty()
            && v.split('.')
                .all(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    };
    if build.is_some_and(|v| !valid_ids(v)) {
        return false;
    }
    let (core, pre) = core_pre
        .split_once('-')
        .map_or((core_pre, None), |(a, b)| (a, Some(b)));
    if pre.is_some_and(|v| {
        !valid_ids(v)
            || v.split('.')
                .any(|p| p.bytes().all(|b| b.is_ascii_digit()) && p.len() > 1 && p.starts_with('0'))
    }) {
        return false;
    }
    let numbers: Vec<_> = core.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|v| {
            !v.is_empty()
                && v.bytes().all(|b| b.is_ascii_digit())
                && (v.len() == 1 || !v.starts_with('0'))
        })
}
pub fn frontmatter<'a>(body: &'a str, allowed: &[&str], path: &str) -> Result<(Value, &'a str)> {
    fm::read(body, allowed)
        .map(|(v, b)| (Value::Object(v), b))
        .map_err(|e| Invalid::new("frontmatter", path, e.to_string()))
}
pub type Inventory = BTreeMap<String, (Vec<u8>, u32)>;
pub fn inventory(root: &Path) -> Result<Inventory> {
    let chain = DirectoryChain::open(root).map_err(malformed)?;
    require(
        chain.directory().stat().map_err(malformed)?.mode & 0o500 == 0o500,
        "asset-mode",
        ".",
        "Pack root needs owner read/search mode",
    )?;
    let mut files = Inventory::new();
    let mut entries = 0usize;
    let mut size = 0usize;
    fn walk(
        dir: &Descriptor,
        prefix: &str,
        files: &mut Inventory,
        entries: &mut usize,
        size: &mut usize,
    ) -> Result<()> {
        let remaining = 4096usize.saturating_sub(*entries);
        let listed = dir.entries_bounded(remaining).map_err(|_| {
            Invalid::new(
                "inventory",
                ".",
                "Every pack directory must be completely readable",
            )
        })?;
        let names = listed.ok_or_else(|| {
            Invalid::new(
                "inventory-bound",
                ".",
                "Pack exceeds 4096 filesystem entries",
            )
        })?;
        *entries += names.len();
        let mut dirs = Vec::new();
        for name in names {
            let name_text = name
                .to_str()
                .ok_or_else(|| malformed("Pack asset path is not UTF-8"))?;
            let rel = if prefix.is_empty() {
                name_text.into()
            } else {
                format!("{prefix}/{name_text}")
            };
            let info = dir.stat_at(&name).map_err(malformed)?;
            require(
                !info.is_symlink(),
                "file-type",
                &rel,
                "Internal symlinks are not portable assets",
            )?;
            require(
                info.is_regular() || info.is_directory(),
                "file-type",
                &rel,
                "Only regular files/directories are supported",
            )?;
            if info.is_directory() {
                require(
                    info.mode & 0o500 == 0o500,
                    "asset-mode",
                    &rel,
                    "Directories need owner read/search mode; no subtree may be skipped",
                )?;
                let child = dir.open_directory_at(&name).map_err(|_| {
                    Invalid::new(
                        "inventory",
                        ".",
                        "Every pack directory must be completely readable",
                    )
                })?;
                if !child.stat().map_err(malformed)?.same_inode(info) {
                    return Err(malformed("Pack directory changed during inventory"));
                }
                dirs.push((rel, child));
                continue;
            }
            require(
                info.size <= 2097152,
                "inventory-bound",
                &rel,
                "Asset exceeds 2 MiB",
            )?;
            let mut file = dir
                .open_at(&name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
                .map_err(malformed)?;
            if file.stat().map_err(malformed)? != info {
                return Err(malformed("Pack asset changed during inventory"));
            }
            let raw = file.read_bounded(2097152).map_err(|e| match e {
                crate::fs::ReadError::SizeLimit { .. } => Invalid::new(
                    "inventory-bound",
                    &rel,
                    "Pack exceeds complete reader budget",
                ),
                _ => malformed(e),
            })?;
            *size += raw.len();
            require(
                *size <= 16777216,
                "inventory-bound",
                &rel,
                "Pack exceeds complete reader budget",
            )?;
            require(
                info.mode & 0o400 != 0,
                "asset-mode",
                &rel,
                "Assets must be owner-readable",
            )?;
            if rel.starts_with("bin/") || Path::new(&rel).extension() == Some(OsStr::new("sh")) {
                require(
                    info.mode & 0o100 != 0,
                    "asset-mode",
                    &rel,
                    "Directly executable assets need owner execute mode",
                )?;
            }
            if dir.stat_at(&name).map_err(malformed)? != info {
                return Err(malformed("Pack asset changed during inventory"));
            }
            files.insert(rel, (raw, info.mode & 0o7777));
        }
        for (rel, child) in dirs {
            walk(&child, &rel, files, entries, size)?;
        }
        Ok(())
    }
    walk(chain.directory(), "", &mut files, &mut entries, &mut size)?;
    chain.recheck().map_err(malformed)?;
    Ok(files)
}
/// The one manifest entry shape: `{"sha256": hex, "mode": oct(mode)}`.
pub fn file_entry(sha256: &str, mode: u32) -> Value {
    object([
        ("sha256", string(sha256)),
        ("mode", string(format!("0o{mode:o}"))),
    ])
}
/// One regular file outside the pack, read like an inventory asset: pinned
/// no-follow descriptor, explicit byte bound, identity rechecked after the read.
/// Returns the bytes and the permission bits (`S_IMODE`).
pub fn binary_asset(binary: &Path, entry: &str, limit: usize) -> Result<(Vec<u8>, u32)> {
    let name = binary.file_name().ok_or_else(|| {
        Invalid::new(
            "file-type",
            entry,
            "Runtime binary must name a regular file",
        )
    })?;
    let parent = match binary.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let parent = crate::fs::canonical_path(parent, true).map_err(malformed)?;
    let chain = DirectoryChain::open(&parent).map_err(malformed)?;
    let directory = chain.directory();
    let info = directory.stat_at(name).map_err(malformed)?;
    require(
        !info.is_symlink(),
        "file-type",
        entry,
        "Runtime binary must not be a symlink",
    )?;
    require(
        info.is_regular(),
        "file-type",
        entry,
        "Runtime binary must be a regular file",
    )?;
    let bound = i64::try_from(limit).map_err(|_| malformed("Runtime binary bound overflows"))?;
    require(
        info.size <= bound,
        "inventory-bound",
        entry,
        "Runtime binary exceeds its reader bound",
    )?;
    let mut file = directory
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(malformed)?;
    if file.stat().map_err(malformed)? != info {
        return Err(malformed("Runtime binary changed during inspection"));
    }
    let raw = file.read_bounded(limit).map_err(|e| match e {
        crate::fs::ReadError::SizeLimit { .. } => Invalid::new(
            "inventory-bound",
            entry,
            "Runtime binary exceeds its reader bound",
        ),
        _ => malformed(e),
    })?;
    if directory.stat_at(name).map_err(malformed)? != info {
        return Err(malformed("Runtime binary changed during inspection"));
    }
    chain.recheck().map_err(malformed)?;
    Ok((raw, info.mode & 0o7777))
}
fn fence(line: &str) -> Option<(u8, usize, &str)> {
    let trim = line.trim_start_matches(' ');
    if line.len() - trim.len() > 3 {
        return None;
    }
    let c = *trim.as_bytes().first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let n = trim.bytes().take_while(|b| *b == c).count();
    (n >= 3).then_some((c, n, &trim[n..]))
}
pub fn markdown(body: &str) -> Result<(String, Vec<String>)> {
    let mut prose = Vec::new();
    let mut shell = Vec::new();
    let mut block = Vec::new();
    let mut open: Option<(u8, usize, String)> = None;
    for line in splitlines(body) {
        if let Some((c, n, lang)) = &open {
            if fence(line).is_some_and(|(x, m, tail)| {
                x == *c && m >= *n && tail.trim_matches([' ', '\t']).is_empty()
            }) {
                if matches!(lang.as_str(), "sh" | "bash" | "shell" | "zsh") {
                    shell.push(block.join("\n"));
                }
                open = None;
                block.clear();
            } else {
                block.push(line);
            }
            prose.push("");
            continue;
        }
        if let Some((c, n, tail)) =
            fence(line).filter(|(c, _, tail)| *c != b'`' || !tail.contains('`'))
        {
            open = Some((c, n, strip(tail).into()));
            prose.push("");
        } else {
            prose.push(line);
        }
    }
    require(open.is_none(), "markdown", ".", "Unclosed code fence")?;
    Ok((prose.join("\n"), shell))
}
fn ref_key(s: &str) -> String {
    crate::unicode::casefold(
        &s.split(whitespace)
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    )
}
fn reference_spans(body: &str) -> Vec<(usize, usize, &str, &str)> {
    let mut result = Vec::new();
    let mut previous_end = 0;
    for start in std::iter::once(0).chain(body.match_indices('\n').map(|(i, _)| i + 1)) {
        if start < previous_end {
            continue;
        }
        let line = &body[start..];
        let trim = line.trim_start_matches(' ');
        if line.len() - trim.len() > 3 {
            continue;
        }
        let Some(rest) = trim.strip_prefix('[') else {
            continue;
        };
        let Some(close) = rest.find(']') else {
            continue;
        };
        if close == 0 {
            continue;
        }
        let key = &rest[..close];
        let Some(tail) = rest[close + 1..].strip_prefix(':') else {
            continue;
        };
        let tail = tail.trim_start_matches(whitespace);
        if tail.is_empty() {
            continue;
        }
        let angled = tail
            .starts_with('<')
            .then(|| tail.find('>').filter(|i| *i > 1))
            .flatten();
        let length = angled.map_or_else(|| tail.find(whitespace).unwrap_or(tail.len()), |i| i + 1);
        if length == 0 {
            continue;
        }
        let target = &tail[..length];
        let end = target.as_ptr() as usize - body.as_ptr() as usize + length;
        result.push((start, end, key, target));
        previous_end = end;
    }
    result
}
pub fn reference_definitions(body: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for (_, _, key, target) in reference_spans(body) {
        let key = ref_key(key);
        let target = target.trim_matches(['<', '>']).to_string();
        require(
            out.get(&key).is_none_or(|v| *v == target),
            "link",
            ".",
            "Conflicting reference definitions",
        )?;
        out.insert(key, target);
    }
    Ok(out)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkOccurrence {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub target: String,
    pub image: bool,
    byte_start: usize,
    byte_end: usize,
}
fn attr<'a>(tag: &'a str, names: &[&str]) -> Option<&'a str> {
    let bytes = tag.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        while i < bytes.len() && !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-' | b':'))
        {
            i += 1;
        }
        let key = &tag[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let Some(quote) = bytes.get(i).copied().filter(|q| matches!(q, b'\'' | b'"')) else {
            continue;
        };
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        if i < bytes.len() && names.iter().any(|n| key.eq_ignore_ascii_case(n)) && i > start {
            return Some(&tag[start..i]);
        }
        i += 1;
    }
    None
}
fn inline_mask(body: &str) -> String {
    let mut masked = body.as_bytes().to_vec();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let n = bytes[i..].iter().take_while(|b| **b == b'`').count();
        let mut j = i + n;
        let mut close = None;
        while j < bytes.len() && bytes[j] != b'\n' {
            if bytes[j] == b'`' {
                let m = bytes[j..].iter().take_while(|b| **b == b'`').count();
                if n == m {
                    close = Some(j + m);
                    break;
                }
                j += m;
            } else {
                j += 1;
            }
        }
        if let Some(end) = close {
            masked[i..end].fill(b' ');
            i = end;
        } else {
            i += n;
        }
    }
    // Masked spans run between ASCII backticks, so whole UTF-8 sequences are replaced.
    String::from_utf8(masked).unwrap_or_default()
}
pub fn link_occurrences(
    body: &str,
    references: Option<&BTreeMap<String, String>>,
) -> Result<Vec<LinkOccurrence>> {
    require(
        !body.contains('\0'),
        "markdown",
        ".",
        "NUL is not supported in instruction prose",
    )?;
    let own;
    let refs = if let Some(v) = references {
        v
    } else {
        own = reference_definitions(body)?;
        &own
    };
    let masked = inline_mask(body);
    let bytes = masked.as_bytes();
    let mut blocked = vec![false; bytes.len()];
    let mut out = Vec::new();
    for (start, end, _, _) in reference_spans(&masked) {
        blocked[start..end].fill(true);
    }
    fn add(
        out: &mut Vec<LinkOccurrence>,
        blocked: &mut [bool],
        start: usize,
        end: usize,
        label: &str,
        target: &str,
        image: bool,
    ) {
        if blocked[start..end].iter().any(|v| *v) {
            return;
        }
        blocked[start..end].fill(true);
        out.push(LinkOccurrence {
            start: 0,
            end: 0,
            label: label.into(),
            target: target.into(),
            image,
            byte_start: start,
            byte_end: end,
        });
    }
    // Full HTML anchors have priority over their inner Markdown and opening tag.
    let ascii_lower = masked.to_ascii_lowercase();
    for full in [true, false] {
        let mut i = 0;
        while let Some(pos) = masked[i..].find('<') {
            let start = i + pos;
            let Some(n) = masked[start..].find('>') else {
                break;
            };
            let end = start + n + 1;
            let tag = &masked[start..end];
            let name = tag[1..]
                .split(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .unwrap_or("");
            let image = name.eq_ignore_ascii_case("img");
            if name.eq_ignore_ascii_case("a") || (!full && image) {
                if let Some(target) = attr(tag, if image { &["href", "src"] } else { &["href"] }) {
                    let mut finish = end;
                    if full {
                        let lower = &ascii_lower[end..];
                        let mut found = None;
                        let mut next = 0;
                        while let Some(c) = lower[next..].find("</a") {
                            let at = next + c + 3;
                            if let Some(tail) = lower.get(at..) {
                                let spaces = tail.len() - tail.trim_start_matches(whitespace).len();
                                if tail[spaces..].starts_with('>') {
                                    found = Some(end + at + spaces + 1);
                                    break;
                                }
                            }
                            next = at;
                        }
                        if let Some(close) = found {
                            finish = close;
                        } else {
                            i = end;
                            continue;
                        }
                    }
                    add(
                        &mut out,
                        &mut blocked,
                        start,
                        finish,
                        "",
                        &crate::unicode::html_unescape(target),
                        image,
                    );
                }
            }
            i = end;
        }
    }
    // Direct inline links, then full/collapsed reference links, then shortcuts.
    for pass in 0..3 {
        let mut i = 0;
        while i < bytes.len() {
            let start = i;
            let image = bytes[i] == b'!';
            let bracket = i + usize::from(image);
            if bytes.get(bracket) != Some(&b'[') {
                i += 1;
                continue;
            }
            let Some(close) = masked[bracket + 1..].find(']').map(|n| bracket + 1 + n) else {
                break;
            };
            let label_raw = &masked[bracket + 1..close];
            if label_raw.contains('\n') {
                i = close + 1;
                continue;
            }
            let mut label_text = String::new();
            for (offset, c) in body[bracket + 1..close].char_indices() {
                let end = offset + c.len_utf8();
                if c.len_utf8() > 1 && label_raw.as_bytes()[offset..end].iter().all(|b| *b == b' ')
                {
                    label_text.push(' ');
                } else {
                    label_text.push_str(&label_raw[offset..end]);
                }
            }
            let label = label_text.as_str();
            let next = close + 1;
            if pass == 0 && bytes.get(next) == Some(&b'(') {
                let rest = &masked[next + 1..];
                let (target, mut consumed) = if rest.starts_with('<') {
                    if let Some(e) = rest
                        .find('>')
                        .filter(|e| *e > 1 && !rest[..*e].contains('\n'))
                    {
                        (&rest[1..e], e + 1)
                    } else {
                        i = next + 1;
                        continue;
                    }
                } else {
                    let n = rest
                        .find(|c: char| whitespace(c) || c == ')')
                        .unwrap_or(rest.len());
                    if n == 0 {
                        i = next + 1;
                        continue;
                    }
                    (&rest[..n], n)
                };
                let tail = &rest[consumed..];
                if !tail.starts_with(')') {
                    let spaces = tail.len() - tail.trim_start_matches(whitespace).len();
                    if spaces == 0 {
                        i = next + 1;
                        continue;
                    }
                    let title = &tail[spaces..];
                    if !title.starts_with(['\'', '"']) {
                        i = next + 1;
                        continue;
                    }
                    let Some(end) = title.find(')').filter(|end| {
                        !title[..*end].contains('\n') && title[..*end].ends_with(['\'', '"'])
                    }) else {
                        i = next + 1;
                        continue;
                    };
                    consumed += spaces + end;
                }
                if rest.as_bytes().get(consumed) == Some(&b')') {
                    add(
                        &mut out,
                        &mut blocked,
                        start,
                        next + consumed + 2,
                        label,
                        target,
                        image,
                    );
                }
            } else if pass == 1 && !label.is_empty() && bytes.get(next) == Some(&b'[') {
                if let Some(end) = masked[next + 1..].find(']').map(|n| next + 1 + n) {
                    let name = &masked[next + 1..end];
                    if !name.contains('\n') && !blocked[start..end + 1].iter().any(|v| *v) {
                        let key = ref_key(if name.is_empty() { label } else { name });
                        let target = refs
                            .get(&key)
                            .ok_or_else(|| Invalid::new("link", ".", "Undefined reference link"))?;
                        add(&mut out, &mut blocked, start, end + 1, label, target, image);
                    }
                }
            } else if pass == 2
                && !label.is_empty()
                && !bytes
                    .get(next)
                    .is_some_and(|c| matches!(c, b'[' | b'(' | b':'))
            {
                if let Some(target) = refs.get(&ref_key(label)) {
                    add(&mut out, &mut blocked, start, next, label, target, image);
                }
            }
            i = close + 1;
        }
    }
    out.sort_by_key(|v| (v.byte_start, v.byte_end));
    let mut at = 0;
    let mut chars = 0;
    for item in &mut out {
        chars += body[at..item.byte_start].chars().count();
        item.start = chars;
        chars += body[item.byte_start..item.byte_end].chars().count();
        item.end = chars;
        at = item.byte_end;
    }
    Ok(out)
}
pub fn links(
    body: &str,
    references: Option<&BTreeMap<String, String>>,
) -> Result<Vec<(String, String)>> {
    Ok(link_occurrences(body, references)?
        .into_iter()
        .filter(|v| !v.image)
        .map(|v| (v.label, v.target))
        .collect())
}
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find('>') {
            rest = &rest[start + end + 1..];
        } else {
            rest = &rest[start..];
            break;
        }
    }
    out.push_str(rest);
    out
}
pub fn anchors(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut counts = BTreeMap::<String, usize>::new();
    for line in body.split('\n') {
        let t = line.trim_start_matches(' ');
        if line.len() - t.len() > 3 {
            continue;
        }
        let n = t.bytes().take_while(|c| *c == b'#').count();
        if !(1..=6).contains(&n) || t.as_bytes().get(n) != Some(&b' ') {
            continue;
        }
        let title = t[n..].trim_start_matches(' ');
        if title.is_empty() {
            continue;
        }
        let mut heading = title;
        if title.ends_with('#') {
            let tail = title.trim_end_matches('#');
            if tail.ends_with(' ') {
                heading = tail.trim_end_matches(' ');
            }
        }
        let slug = strip_tags(heading)
            .to_lowercase()
            .chars()
            .filter(|c| crate::unicode::is_word(*c) || matches!(c, '-' | ' '))
            .map(|c| if c == ' ' { '-' } else { c })
            .collect::<String>();
        let count = counts.entry(slug.clone()).or_default();
        out.insert(if *count == 0 {
            slug.clone()
        } else {
            format!("{slug}-{count}")
        });
        *count += 1;
    }
    let mut rest = body;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        let tag = &rest[start..start + end + 1];
        let name = tag[1..]
            .split(|c: char| !c.is_ascii_alphanumeric())
            .next()
            .unwrap_or("");
        if name.eq_ignore_ascii_case("a") || name.eq_ignore_ascii_case("span") {
            if let Some(id) = attr(tag, &["id"]) {
                out.insert(id.into());
            }
        }
        rest = &rest[start + end + 1..];
    }
    out
}
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push((a * 16 + b) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
/// urlsplit's relevant fields; absolute URI schemes and network locations are external.
pub(crate) fn url_parts(target: &str) -> Result<(Option<String>, bool, String, String)> {
    let clean = target
        .trim_start_matches(|c: char| c <= ' ')
        .replace(['\t', '\r', '\n'], "");
    let (rest, fragment) = clean.split_once('#').unwrap_or((&clean, ""));
    let path = rest.split_once('?').map_or(rest, |(p, _)| p);
    let scheme = path.split_once(':').filter(|(s, _)| {
        s.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    });
    let remainder = scheme.map_or(path, |(_, tail)| tail);
    let netloc = remainder.starts_with("//");
    if netloc {
        let authority = remainder[2..].split('/').next().unwrap_or("");
        let open = authority.find('[');
        let close = authority.find(']');
        if open.is_some() != close.is_some() {
            return Err(malformed("Invalid IPv6 URL"));
        }
        if let (Some(start), Some(end)) = (open, close) {
            if end < start {
                return Err(malformed("Invalid IPv6 URL"));
            }
            let host = &authority[start + 1..end];
            let valid = if host.starts_with('v') {
                host[1..].split_once('.').is_some_and(|(version, tail)| {
                    !version.is_empty()
                        && version.bytes().all(|b| b.is_ascii_hexdigit())
                        && !tail.is_empty()
                })
            } else {
                let mut scope = host.split('%');
                let address = scope.next().unwrap_or("");
                let suffix = scope.next();
                scope.next().is_none()
                    && suffix.is_none_or(|s| !s.is_empty())
                    && address.parse::<std::net::Ipv6Addr>().is_ok()
            };
            if !valid {
                return Err(malformed("Invalid IPv6 URL"));
            }
        }
        if authority
            .chars()
            .any(|c| crate::remote_identity::NFKC_NETLOC_DELIMITERS.contains(&(c as u32)))
        {
            return Err(malformed(
                "netloc contains invalid characters under NFKC normalization",
            ));
        }
    }
    Ok((
        scheme.map(|(s, _)| s.to_ascii_lowercase()),
        netloc,
        path.into(),
        fragment.into(),
    ))
}
pub fn destination(
    source: &str,
    target: &str,
    files: &Inventory,
) -> Result<(Option<String>, String)> {
    let (scheme, netloc, path, fragment) = url_parts(target)?;
    if scheme.is_some() || netloc {
        return Ok((None, String::new()));
    }
    let path = percent_decode(&path);
    require(
        !path.contains('\\') && !path.starts_with('/'),
        "link",
        source,
        "Relative links must stay inside the pack",
    )?;
    let mut parts = source.split('/').collect::<Vec<_>>();
    parts.pop();
    if !path.is_empty() {
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    require(!parts.is_empty(), "link", source, "Link escapes the pack")?;
                    parts.pop();
                }
                _ => parts.push(part),
            }
        }
    }
    let rel = if path.is_empty() {
        source.into()
    } else {
        parts.join("/")
    };
    require(
        files.contains_key(&rel),
        "link",
        source,
        &format!("Missing relative asset: {rel}"),
    )?;
    Ok((Some(rel), percent_decode(&fragment)))
}

fn words(body: &str) -> Vec<(usize, usize, &str)> {
    let mut result = Vec::new();
    let mut start = None;
    for (i, c) in body.char_indices() {
        if crate::unicode::is_word(c) {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            result.push((s, i, &body[s..i]));
        }
    }
    if let Some(s) = start {
        result.push((s, body.len(), &body[s..]));
    }
    result
}
fn remove_inline(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut result = String::with_capacity(body.len());
    let mut position = 0;
    while let Some(at) = body[position..].find('`') {
        let start = position + at;
        result.push_str(&body[position..start]);
        let opening = bytes[start..].iter().take_while(|b| **b == b'`').count();
        let tail = &body[start + opening..];
        if let Some(end) = tail
            .find(['`', '\n'])
            .filter(|end| tail.as_bytes()[*end] == b'`')
        {
            let close = start + opening + end;
            position = close + bytes[close..].iter().take_while(|b| **b == b'`').count();
        } else {
            result.push_str(&body[start..start + opening]);
            position = start + opening;
        }
    }
    result.push_str(&body[position..]);
    result
}
fn sentences(body: &str) -> Vec<&str> {
    let mut cuts = Vec::new();
    let tokens = words(body);
    for (index, (start, _, word)) in tokens.iter().enumerate() {
        if matches!(
            word.to_ascii_lowercase().as_str(),
            "and" | "but" | "or" | "then"
        ) && tokens.get(index + 1).is_some_and(|(_, _, next)| {
            matches!(next.to_ascii_lowercase().as_str(), "if" | "when" | "unless")
        }) && *start > 0
            && body[..*start].chars().next_back().is_some_and(whitespace)
        {
            let cut = body[..*start]
                .trim_end_matches(whitespace)
                .trim_end_matches(',')
                .len();
            cuts.push(cut);
        }
    }
    let mut iter = body.char_indices().peekable();
    while let Some((at, c)) = iter.next() {
        if matches!(c, '.' | '!' | '?' | ';') && iter.peek().is_some_and(|(_, c)| whitespace(*c)) {
            cuts.push(at + c.len_utf8());
        }
        if c == '\n' {
            let rest = &body[at + 1..];
            let whitespace = rest.len() - rest.trim_start_matches(whitespace).len();
            if rest[..whitespace].contains('\n') {
                cuts.push(at + 1);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut start = 0;
    let mut result = Vec::new();
    for end in cuts {
        result.push(&body[start..end]);
        start = end;
    }
    result.push(&body[start..]);
    result
}
fn marker_ranges(body: &str) -> Vec<(usize, usize, &str)> {
    let tokens = words(body);
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let (start, end, word) = tokens[i];
        let mut finish = end;
        let known = matches!(
            word,
            "MUST" | "SHALL" | "SHOULD" | "REQUIRED" | "RECOMMENDED" | "MAY" | "OPTIONAL"
        );
        if matches!(word, "MUST" | "SHALL" | "SHOULD")
            && tokens
                .get(i + 1)
                .is_some_and(|(s, _, w)| *w == "NOT" && strip(&body[end..*s]).is_empty())
        {
            i += 1;
            finish = tokens[i].1;
        }
        if word == "NOT"
            && tokens
                .get(i + 1)
                .is_some_and(|(s, _, w)| *w == "RECOMMENDED" && strip(&body[end..*s]).is_empty())
        {
            i += 1;
            finish = tokens[i].1;
            out.push((start, finish, &body[start..finish]));
        } else if known {
            out.push((start, finish, &body[start..finish]));
        }
        i += 1;
    }
    out
}
fn link_token(body: &str) -> Option<(usize, usize)> {
    let rest = body.strip_prefix("\0L")?;
    let end = rest.find('\0')?;
    let index = rest[..end].parse().ok()?;
    Some((index, end + 3))
}
pub fn mandatory_targets(
    prose: &str,
    path: &str,
    files: &Inventory,
    references: Option<&BTreeMap<String, String>>,
) -> Result<BTreeSet<String>> {
    let mut targets = Vec::new();
    let mut body = String::with_capacity(prose.len());
    let mut at = 0;
    for link in link_occurrences(prose, references)? {
        body.push_str(&prose[at..link.byte_start]);
        if link.image {
            body.push(' ');
        } else {
            body.push_str(&format!("\0L{}\0", targets.len()));
        }
        targets.push(link.target);
        at = link.byte_end;
    }
    body.push_str(&prose[at..]);
    let body = remove_inline(&body);
    let mut filtered = String::new();
    let mut offset = 0;
    for (start, end, _, _) in reference_spans(&body) {
        filtered.push_str(&body[offset..start]);
        offset = end;
    }
    filtered.push_str(&body[offset..]);
    let mut result = BTreeSet::new();
    for sentence in sentences(&filtered) {
        let tokens = words(sentence);
        let lower = tokens
            .iter()
            .map(|(_, _, w)| w.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let conditional = lower
            .iter()
            .any(|w| matches!(w.as_str(), "when" | "if" | "unless"))
            || lower.windows(2).any(|w| {
                w[0] == "for"
                    && matches!(w[1].as_str(), "qualified" | "eligible")
                    && strip(sentence).to_ascii_lowercase().starts_with("for ")
            })
            || lower.iter().enumerate().any(|(i, w)| {
                if w != "at" {
                    return false;
                }
                let mut j = i + 1;
                if lower.get(j).is_some_and(|w| w == "a") {
                    j += 1;
                }
                if lower.get(j).is_none_or(|w| w != "natural") {
                    return false;
                }
                j += 1;
                if lower.get(j).is_some_and(|w| w == "turn") {
                    j += 1;
                }
                lower
                    .get(j)
                    .is_some_and(|w| matches!(w.as_str(), "boundary" | "boundaries"))
            });
        let markers = marker_ranges(sentence);
        for (index, (_, end, marker)) in markers.iter().enumerate() {
            if conditional || !matches!(*marker, "MUST" | "SHALL") {
                continue;
            }
            let finish = markers.get(index + 1).map_or(sentence.len(), |v| v.0);
            let clause = &sentence[*end..finish];
            for (_, end, word) in words(clause) {
                if !matches!(word, "read" | "load" | "open" | "preload" | "follow") {
                    continue;
                }
                let raw = &clause[end..];
                let mut tail = raw.trim_start_matches(whitespace);
                if tail.len() == raw.len() {
                    continue;
                }
                let word_start = |prefix: &str| {
                    tail.strip_prefix(prefix).is_some_and(|rest| {
                        rest.chars()
                            .next()
                            .is_none_or(|c| !crate::unicode::is_word(c))
                    })
                };
                let whole = ["all", "every", "the whole", "the entire"]
                    .into_iter()
                    .any(word_start);
                require(
                    !whole,
                    "loading",
                    path,
                    "Unconditional whole-tree loading is forbidden",
                )?;
                if let Some(rest) = tail
                    .strip_prefix("and")
                    .and_then(|r| r.strip_prefix(whitespace))
                {
                    let rest = rest.trim_start_matches(whitespace);
                    if let Some(rest) = rest
                        .strip_prefix("follow")
                        .and_then(|r| r.strip_prefix(whitespace))
                    {
                        tail = rest.trim_start_matches(whitespace);
                    }
                }
                if let Some(rest) = tail
                    .strip_prefix("the")
                    .and_then(|r| r.strip_prefix(whitespace))
                {
                    tail = rest.trim_start_matches(whitespace);
                }
                let mut indices = Vec::new();
                if let Some((index, end)) = link_token(tail) {
                    indices.push(index);
                    tail = &tail[end..];
                } else {
                    continue;
                }
                loop {
                    let rest = tail.trim_start_matches(whitespace);
                    let next = if let Some(rest) = rest.strip_prefix(',') {
                        let rest = rest.trim_start_matches(whitespace);
                        rest.strip_prefix("and")
                            .and_then(|s| s.strip_prefix(whitespace))
                            .unwrap_or(rest)
                    } else if let Some(rest) = rest
                        .strip_prefix("and")
                        .and_then(|s| s.strip_prefix(whitespace))
                    {
                        rest
                    } else {
                        break;
                    };
                    let mut next = next.trim_start_matches(whitespace);
                    if let Some(rest) = next
                        .strip_prefix("the")
                        .and_then(|s| s.strip_prefix(whitespace))
                    {
                        next = rest.trim_start_matches(whitespace);
                    }
                    if let Some((index, end)) = link_token(next) {
                        indices.push(index);
                        tail = &next[end..];
                    } else {
                        break;
                    }
                }
                for index in indices {
                    if let Some(target) = targets.get(index) {
                        let (dest, _) = destination(path, target, files)?;
                        if let Some(dest) = dest.filter(|v| v.ends_with(".md")) {
                            result.insert(dest);
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

pub fn git_operation(argv: &[String], path: &str) -> Result<String> {
    let mut index = 1;
    while argv
        .get(index)
        .is_some_and(|s| matches!(s.as_str(), "-C" | "-c" | "--git-dir" | "--work-tree"))
    {
        require(
            index + 1 < argv.len(),
            "dispatch",
            path,
            "Missing Git global-option value",
        )?;
        index += 2;
    }
    require(
        argv.get(index).is_some_and(|s| s == "config"),
        "dispatch",
        path,
        "Unsupported external Git operation",
    )?;
    Ok("config".to_owned())
}
pub fn command_argv(line: &str, path: &str) -> Result<Vec<String>> {
    require(
        !line.contains("$(") && !line.contains('`'),
        "dispatch",
        path,
        "Command substitutions are outside the literal command contract",
    )?;
    let mut lexer = String::new();
    let mut outside = String::new();
    let mut quote = None;
    let mut escape = false;
    let mut argv = Vec::new();
    let mut active = false;
    let mut punctuation = false;
    let mut allowed_expansion = Vec::new();
    for c in line.chars() {
        if escape {
            if quote.is_none() {
                outside.push(c);
            }
            if quote == Some('"') && !matches!(c, '$' | '`' | '"' | '\\') {
                lexer.push('\\');
            }
            lexer.push(c);
            escape = false;
            active = true;
            continue;
        }
        if c == '\\' && quote != Some('\'') {
            escape = true;
            active = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                lexer.push(c);
            }
            active = true;
            continue;
        }
        if matches!(c, '\'' | '"') {
            if punctuation && !lexer.is_empty() {
                argv.push(std::mem::take(&mut lexer));
            }
            quote = Some(c);
            active = true;
            punctuation = false;
            continue;
        }
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            if active {
                argv.push(std::mem::take(&mut lexer));
                active = false;
            }
            punctuation = false;
            continue;
        }
        outside.push(c);
        let p = matches!(c, ';' | '&' | '|' | '<' | '>');
        if active && p != punctuation {
            argv.push(std::mem::take(&mut lexer));
        }
        lexer.push(c);
        active = true;
        punctuation = p;
    }
    if escape || quote.is_some() {
        return Err(malformed(if escape {
            "No escaped character"
        } else {
            "No closing quotation"
        }));
    }
    if active {
        argv.push(lexer);
    }
    // Only the double-quoted, fixed helper/launcher path can contain a dollar.
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else {
            break;
        };
        let value = &after[..end];
        let valid = value.strip_prefix("$SKILL_DIR/").is_some_and(|v| {
            v == "hooks/python3.sh"
                || v.strip_prefix("scripts/")
                    .and_then(|v| v.strip_suffix(".py"))
                    .is_some_and(|v| {
                        !v.is_empty() && v.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                    })
        });
        if valid {
            allowed_expansion.push(value);
        }
        rest = &after[end + 1..];
    }
    let dollars = line.bytes().filter(|b| *b == b'$').count();
    require(
        dollars == allowed_expansion.len(),
        "dispatch",
        path,
        "Only the quoted SKILL_DIR helper path may expand",
    )?;
    require(
        !outside
            .chars()
            .any(|c| matches!(c, '*' | '?' | '[' | ']' | '{' | '}' | '~')),
        "dispatch",
        path,
        "Unquoted shell expansions are outside the literal command contract",
    )?;
    require(
        !argv.is_empty()
            && !argv.iter().any(|s| {
                s != ">"
                    && !s.is_empty()
                    && s.chars().all(|c| matches!(c, ';' | '&' | '|' | '<' | '>'))
            }),
        "dispatch",
        path,
        "Document one supported command per line, without hidden shell operations",
    )?;
    if argv.iter().any(|v| v == ">") {
        require(
            argv.iter().filter(|s| s.as_str() == ">").count() == 1
                && argv.len() >= 3
                && argv.iter().rev().nth(1).is_some_and(|v| v == ">"),
            "dispatch",
            path,
            "Only a trailing stdout-file redirection is supported",
        )?;
        argv.truncate(argv.len() - 2);
    }
    Ok(argv)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRoute {
    pub helper: String,
    pub prefix: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub module: String,
    pub commands: BTreeSet<String>,
    pub flags: BTreeSet<String>,
    pub routes: BTreeMap<String, CommandRoute>,
}
/// Supplies only authenticated command-surface data. It never parses Python.
pub trait CommandSurface: Send + Sync {
    fn dispatch(&self, script: &str, files: &Inventory) -> Result<Dispatch>;
}
static COMMAND_SURFACE: RwLock<(u64, Option<Arc<dyn CommandSurface>>)> = RwLock::new((0, None));
type InventoryKey = Vec<(String, u32, [u8; 32])>;
static LAST_VALIDATION: RwLock<Option<(u64, InventoryKey, Value)>> = RwLock::new(None);
pub fn register_command_surface(provider: Option<Arc<dyn CommandSurface>>) -> Result<()> {
    let mut registry = COMMAND_SURFACE
        .write()
        .map_err(|_| malformed("Command surface registry is poisoned"))?;
    registry.0 = registry
        .0
        .checked_add(1)
        .ok_or_else(|| malformed("Command surface registry generation exhausted"))?;
    registry.1 = provider;
    *LAST_VALIDATION
        .write()
        .map_err(|_| malformed("Pack validation memo is poisoned"))? = None;
    Ok(())
}
fn registered_surface() -> Result<(u64, Option<Arc<dyn CommandSurface>>)> {
    Ok(COMMAND_SURFACE
        .read()
        .map_err(|_| malformed("Command surface registry is poisoned"))?
        .clone())
}

fn array_strings(values: impl IntoIterator<Item = impl Into<String>>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}
fn identifier(value: &str) -> bool {
    value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn normative(prose: &str, path: &str) -> Result<usize> {
    let clean = remove_inline(prose);
    let tokens = words(&clean);
    require(
        !tokens
            .iter()
            .any(|(_, _, w)| matches!(*w, "must" | "shall")),
        "rfc-form",
        path,
        "Normative must/shall must use uppercase RFC forms",
    )?;
    for (i, (_, end, word)) in tokens.iter().enumerate() {
        if !matches!(*word, "MUST" | "SHALL" | "SHOULD") {
            continue;
        }
        if let Some((start, _, next)) = tokens
            .get(i + 1)
            .filter(|(start, _, _)| *start > *end && strip(&clean[*end..*start]).is_empty())
        {
            require(
                !matches!(*next, "not" | "may" | "should" | "must" | "shall"),
                "rfc-form",
                path,
                "Ambiguous or mixed-case RFC form",
            )?;
            let mut next_index = i + 1;
            if *next == "NOT" {
                next_index += 1;
            }
            if let Some((next_start, _, next)) = tokens.get(next_index) {
                let previous_end = tokens[next_index - 1].1;
                require(
                    !(strip(&clean[previous_end..*next_start]).is_empty()
                        && matches!(
                            *next,
                            "MUST" | "SHALL" | "SHOULD" | "MAY" | "OPTIONAL" | "REQUIRED"
                        )),
                    "rfc-form",
                    path,
                    "Compound RFC requirement form is ambiguous",
                )?;
            }
            let _ = start;
        }
    }
    if !path.starts_with("adapters/") && !path.starts_with("lifecycle/") {
        require(
            !tokens
                .iter()
                .any(|(_, _, w)| matches!(*w, "Claude" | "Codex" | "OpenCode")),
            "client-neutrality",
            path,
            "Core instructions must remain client-neutral",
        )?;
    }
    Ok(tokens
        .iter()
        .filter(|(_, _, w)| {
            matches!(
                *w,
                "MUST" | "SHALL" | "SHOULD" | "MAY" | "REQUIRED" | "RECOMMENDED" | "OPTIONAL"
            )
        })
        .count())
}
fn inline_python(prose: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = prose;
    while let Some(start) = rest.find("`python3 ") {
        let tail = &rest[start + 1..];
        if let Some(end) = tail.find(['`', '\n']) {
            if tail.as_bytes()[end] == b'`' {
                out.push(tail[..end].into());
            }
            rest = &tail[end + 1..];
        } else {
            break;
        }
    }
    out
}
fn declared_assets(prose: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = prose;
    while let Some(start) = rest.find('`') {
        let tail = &rest[start + 1..];
        if let Some(end) = tail.find(['`', '\n']) {
            let value = &tail[..end];
            if tail.as_bytes()[end] == b'`'
                && (value.starts_with("schemas/") || value.starts_with("templates/"))
            {
                out.push(value);
            }
            rest = &tail[end + 1..];
        } else {
            break;
        }
    }
    out
}
fn check_dispatch(
    prose_path: &str,
    argv: &[String],
    files: &Inventory,
    surface: Option<&dyn CommandSurface>,
    dispatches: &mut BTreeMap<String, Dispatch>,
) -> Result<Value> {
    let mut argv = argv.to_vec();
    if argv.starts_with(&["/bin/sh".into(), "$SKILL_DIR/hooks/python3.sh".into()]) {
        require(
            files.contains_key("hooks/python3.sh"),
            "dispatch",
            prose_path,
            "Supported Python launcher is missing",
        )?;
        argv.splice(..2, ["python3".into(), "-I".into()]);
    }
    if argv.starts_with(&["python3".into(), "-B".into(), "-I".into()]) {
        argv.remove(1);
    }
    require(
        matches!(
            argv.as_slice(),
            [program, isolated, script, ..]
                if program == "python3"
                    && isolated == "-I"
                    && script.starts_with("$SKILL_DIR/scripts/")
        ),
        "dispatch",
        prose_path,
        "Undeclared shell command form",
    )?;
    let Some(script) = argv.get(2).and_then(|v| v.strip_prefix("$SKILL_DIR/")) else {
        return Err(Invalid::new(
            "dispatch",
            prose_path,
            "Undeclared shell command form",
        ));
    };
    require(
        files.contains_key(script),
        "dispatch",
        prose_path,
        &format!("Documented helper is missing: {script}"),
    )?;
    if !dispatches.contains_key(script) {
        let provider = surface.ok_or_else(|| {
            Invalid::new(
                "command-surface-unregistered",
                script,
                "Signed command surface provider is not registered; command validation refuses",
            )
        })?;
        dispatches.insert(script.into(), provider.dispatch(script, files)?);
    }
    let declared = dispatches.get(script).ok_or_else(|| {
        Invalid::new(
            "command-surface-unregistered",
            script,
            "Signed command surface provider is not registered; command validation refuses",
        )
    })?;
    let verb = argv.get(3).filter(|v| !v.starts_with('-'));
    let operands = argv.get(3..).unwrap_or_default();
    let help_only = operands == ["--help"] || operands == ["-h"];
    require(
        help_only
            || if declared.commands.is_empty() {
                verb.is_none()
            } else {
                verb.is_some_and(|v| declared.commands.contains(v))
            },
        "dispatch",
        prose_path,
        &format!("Unknown documented operation for {script}"),
    )?;
    let mut tail = operands.to_vec();
    let mut flags = declared.flags.clone();
    let mut module = declared.module.clone();
    if let Some(route) = verb.and_then(|v| declared.routes.get(v)) {
        let child = format!("scripts/{}", route.helper);
        let provider = surface.ok_or_else(|| {
            Invalid::new(
                "command-surface-unregistered",
                &child,
                "Signed command surface provider is not registered; command validation refuses",
            )
        })?;
        let child = provider.dispatch(&child, files)?;
        let mut delegated = route.prefix.clone();
        delegated.extend_from_slice(argv.get(4..).unwrap_or_default());
        let child_verb = delegated.first().filter(|v| !v.starts_with('-'));
        let help = delegated == ["--help"] || delegated == ["-h"];
        require(
            help || if child.commands.is_empty() {
                child_verb.is_none()
            } else {
                child_verb.is_some_and(|v| child.commands.contains(v))
            },
            "dispatch",
            prose_path,
            &format!("Unknown documented delegated operation for {script}"),
        )?;
        tail = delegated;
        flags = child.flags;
        flags.insert("--json".into());
        module = child.module;
    }
    require(
        tail.iter()
            .filter(|v| v.starts_with("--"))
            .all(|v| flags.contains(v.split_once('=').map_or(v.as_str(), |(flag, _)| flag))),
        "dispatch",
        prose_path,
        &format!("Unknown documented option for {script}"),
    )?;
    Ok(object([
        ("guide", string(prose_path)),
        ("script", string(script)),
        ("module", string(module)),
        ("operation", verb.map_or(Value::Null, string)),
    ]))
}
fn validate_inventory(files: &Inventory, surface: Option<&dyn CommandSurface>) -> Result<Value> {
    let skills = files
        .keys()
        .filter(|p| Path::new(p).file_name() == Some(OsStr::new("SKILL.md")))
        .map(String::as_str)
        .collect::<Vec<_>>();
    require(
        skills == ["SKILL.md"],
        "registration",
        ".",
        "Exactly one root SKILL.md is required",
    )?;
    let mut documents = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    let mut shells = BTreeMap::new();
    let mut bodies = BTreeMap::new();
    for (path, (raw, _)) in files {
        if path.starts_with("schemas/") && path.ends_with(".json") {
            json::decode_bytes(
                raw,
                DecodePolicy {
                    duplicate_keys: DuplicateKeys::Reject,
                    non_finite: NonFinite::Reject,
                    ..DecodePolicy::default()
                },
            )
            .map_err(malformed)?;
        }
        if !path.ends_with(".md") {
            continue;
        }
        let document = std::str::from_utf8(raw).map_err(malformed)?;
        let (meta, body) = if path == "SKILL.md" {
            let (meta, body) = frontmatter(
                document,
                &[
                    "name",
                    "description",
                    "metadata",
                    "license",
                    "compatibility",
                ],
                path,
            )?;
            require(
                text(&meta, "name") == Some("agent-memory")
                    && text(&meta, "description")
                        .is_some_and(|s| !s.is_empty() && s.chars().count() <= 1024),
                "frontmatter",
                path,
                "Invalid discovery name/description",
            )?;
            require(
                get(&meta, "metadata").as_object().is_some()
                    && semantic_version(get(get(&meta, "metadata"), "version")),
                "frontmatter",
                path,
                "metadata.version must be a semantic version",
            )?;
            (meta, body)
        } else if Path::new(path).file_name() == Some(OsStr::new("GUIDE.md")) {
            let (meta, body) = frontmatter(document, &["kind", "domain", "file-operations"], path)?;
            require(
                matches!(text(&meta, "kind"), Some("domain" | "operation" | "model")),
                "frontmatter",
                path,
                "Unknown guide kind",
            )?;
            require(
                text(&meta, "kind") == Some("operation")
                    || meta
                        .as_object()
                        .is_some_and(|v| v.len() == 1 && v[0].0 == "kind"),
                "frontmatter",
                path,
                "Only operations declare an owner or file-only operations",
            )?;
            (meta, body)
        } else {
            (object(std::iter::empty::<(&str, Value)>()), document)
        };
        let (prose, mut shell) = markdown(body)?;
        shell.extend(inline_python(&prose));
        documents.insert(path.clone(), prose);
        metadata.insert(path.clone(), meta);
        shells.insert(path.clone(), shell);
        bodies.insert(path.clone(), body.to_owned());
    }
    let mut graph = documents
        .keys()
        .map(|p| (p.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    let mut mandatory = graph.clone();
    let all_anchors = documents
        .iter()
        .map(|(p, t)| (p.clone(), anchors(t)))
        .collect::<BTreeMap<_, _>>();
    let mut mappings = Vec::new();
    let mut dispatches = BTreeMap::new();
    let mut file_ops = Vec::new();
    let mut token_count = 0usize;
    for (path, prose) in &documents {
        for (_, target) in reference_definitions(prose)? {
            destination(path, &target, files)?;
        }
        for link in link_occurrences(prose, None)? {
            let (dest, anchor) = destination(path, &link.target, files)?;
            if let Some(dest) = dest {
                if graph.contains_key(&dest) && !link.image {
                    if let Some(edges) = graph.get_mut(path) {
                        edges.insert(dest.clone());
                    }
                }
                require(
                    anchor.is_empty()
                        || all_anchors.get(&dest).is_some_and(|v| v.contains(&anchor)),
                    "anchor",
                    path,
                    &format!("Missing anchor in {dest}: {anchor}"),
                )?;
            }
        }
    }
    let root = documents.get("SKILL.md").ok_or_else(|| {
        Invalid::new("registration", ".", "Exactly one root SKILL.md is required")
    })?;
    let (_, route) = root.split_once("## Route\n").ok_or_else(|| {
        Invalid::new(
            "route",
            "SKILL.md",
            "Root needs its intent/domain/operation route table",
        )
    })?;
    let route = route.split("\n## ").next().unwrap_or_default();
    let references = reference_definitions(root)?;
    for line in splitlines(route).into_iter().filter(|s| s.starts_with('|')) {
        let targets = links(line, Some(&references))?
            .into_iter()
            .map(|(_, target)| destination("SKILL.md", &target, files).map(|v| v.0))
            .collect::<Result<Vec<_>>>()?;
        if targets.len() == 2 {
            if let Some(operation) = targets[1]
                .as_ref()
                .and_then(|p| metadata.get(p))
                .filter(|m| text(m, "kind") == Some("operation"))
            {
                let owner = format!("{}/GUIDE.md", text(operation, "domain").unwrap_or("None"));
                if metadata
                    .get(&owner)
                    .is_some_and(|m| text(m, "kind") == Some("domain"))
                {
                    require(
                        targets[0].as_deref() == Some(owner.as_str()),
                        "route",
                        "SKILL.md",
                        "Root route names the wrong primary domain",
                    )?;
                }
            }
        }
    }
    for (path, prose) in &documents {
        for asset in declared_assets(prose) {
            require(
                files.contains_key(asset),
                "asset",
                path,
                &format!("Missing declared schema/template: {asset}"),
            )?;
        }
        // `metadata`, `bodies` and `shells` hold exactly the `documents` keys.
        let meta = &metadata[path];
        let operation = text(meta, "kind") == Some("operation");
        if operation {
            let headings = prose
                .split('\n')
                .filter_map(|line| line.strip_prefix("## ").filter(|s| !s.is_empty()))
                .collect::<Vec<_>>();
            require(
                headings == SECTIONS,
                "leaf-contract",
                path,
                "Operation needs the eleven required sections in order",
            )?;
            let positions = splitlines(prose)
                .iter()
                .enumerate()
                .filter_map(|(i, line)| line.starts_with("## ").then_some(i))
                .collect::<Vec<_>>();
            let original = splitlines(&bodies[path]);
            for (i, heading) in SECTIONS.iter().enumerate() {
                let end = positions.get(i + 1).copied().unwrap_or(original.len());
                // Python slicing: an out-of-range or reversed span is empty.
                let section = positions
                    .get(i)
                    .and_then(|start| original.get(start + 1..end))
                    .unwrap_or_default();
                require(
                    !strip(&section.join("\n")).is_empty(),
                    "leaf-contract",
                    path,
                    &format!("Empty required section: {heading}"),
                )?;
            }
            let domain = text(meta, "domain").filter(|d| identifier(d));
            let Some(domain) = domain else {
                return Err(Invalid::new(
                    "domain",
                    path,
                    "Operation needs one owning domain",
                ));
            };
            let parent = format!("{domain}/GUIDE.md");
            require(
                metadata
                    .get(&parent)
                    .is_some_and(|m| text(m, "kind") == Some("domain")),
                "domain",
                path,
                "Owning domain is missing",
            )?;
            let context = prose
                .split_once("## Required context\n")
                .map(|(_, rest)| rest.split("\n## ").next().unwrap_or_default())
                .ok_or_else(|| {
                    Invalid::new(
                        "leaf-contract",
                        path,
                        "Operation needs the eleven required sections in order",
                    )
                })?;
            require(
                mandatory_targets(context, path, files, Some(&reference_definitions(prose)?))?
                    .contains(&parent),
                "domain",
                path,
                "Required context must positively and unconditionally require its owning domain",
            )?;
            require(
                graph.get(&parent).is_some_and(|edges| edges.contains(path)),
                "domain",
                path,
                "Owning domain must route to its operation",
            )?;
            if let Some(loads) = mandatory.get_mut(path) {
                loads.insert(parent);
            }
        }
        if let Some(ops) = meta.get("file-operations") {
            let array = ops.as_array();
            require(
                operation
                    && array.is_some_and(|v| {
                        !v.is_empty()
                            && v.iter().all(|s| s.as_str().is_some_and(identifier))
                            && v.iter().enumerate().all(|(i, s)| !v[..i].contains(s))
                    }),
                "file-operation",
                path,
                "File-only operations need explicit distinct names",
            )?;
            file_ops.push((path.clone(), ops.clone()));
        }
        let targets = mandatory_targets(prose, path, files, None)?;
        for target in targets {
            if mandatory.contains_key(&target) {
                if let Some(loads) = mandatory.get_mut(path) {
                    loads.insert(target);
                }
            }
        }
        token_count += normative(prose, path)?;
        let mut count = 0usize;
        for block in &shells[path] {
            for line in splitlines(&block.replace("\\\n", " ")) {
                let line = strip(line);
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let argv = command_argv(line, path)?;
                if argv.first().is_some_and(|program| program == "git") {
                    mappings.push(object([
                        ("guide", string(path)),
                        ("external", string("git")),
                        ("operation", string(git_operation(&argv, path)?)),
                    ]));
                    continue;
                }
                mappings.push(check_dispatch(
                    path,
                    &argv,
                    files,
                    surface,
                    &mut dispatches,
                )?);
                count += 1;
            }
        }
        require(
            !operation || count > 0 || meta.get("file-operations").is_some(),
            "dispatch",
            path,
            "Operation needs a documented helper or declared file-only operation",
        )?;
    }
    let mut reached = BTreeSet::new();
    let mut pending = vec!["SKILL.md".to_string()];
    while let Some(path) = pending.pop() {
        if reached.insert(path.clone()) {
            pending.extend(graph.get(&path).into_iter().flatten().cloned());
        }
    }
    let missing = documents
        .keys()
        .filter(|p| !reached.contains(*p))
        .cloned()
        .collect::<Vec<_>>();
    require(
        missing.is_empty(),
        "reachability",
        ".",
        &format!("Unreachable guides: {}", missing.join(", ")),
    )?;
    fn visit(
        path: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        visiting: &mut BTreeSet<String>,
        complete: &mut BTreeSet<String>,
    ) -> Result<()> {
        require(
            !visiting.contains(path),
            "loading-cycle",
            path,
            "Mandatory guide loading contains a cycle",
        )?;
        if complete.contains(path) {
            return Ok(());
        }
        visiting.insert(path.into());
        for child in graph.get(path).into_iter().flatten() {
            visit(child, graph, visiting, complete)?;
        }
        visiting.remove(path);
        complete.insert(path.into());
        Ok(())
    }
    let mut visiting = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for path in mandatory.keys() {
        visit(path, &mandatory, &mut visiting, &mut complete)?;
    }
    Ok(object([
        ("schema", string("agent-memory/pack-validation-v1")),
        ("valid", Value::Bool(true)),
        ("writes_performed", Value::Bool(false)),
        ("candidate_code_executed", Value::Bool(false)),
        ("semantic_completeness_verified", Value::Bool(false)),
        (
            "version",
            get(get(&metadata["SKILL.md"], "metadata"), "version").clone(),
        ),
        (
            "files",
            Value::Object(
                files
                    .iter()
                    .map(|(p, (raw, mode))| (p.clone(), file_entry(&sha256(raw), *mode)))
                    .collect(),
            ),
        ),
        ("guides", array_strings(documents.keys().cloned())),
        (
            "mandatory_loads",
            Value::Object(
                mandatory
                    .into_iter()
                    .map(|(p, v)| (p, array_strings(v)))
                    .collect(),
            ),
        ),
        ("documented_commands", Value::Array(mappings)),
        ("file_operations", Value::Object(file_ops)),
        ("rfc_tokens_observed", Value::Int(token_count.into())),
        (
            "limits",
            array_strings([
                "Supported Markdown links and literal argparse dispatch are mechanically checked.",
                "Conditioned navigation is distinct from recognized direct mandatory loading sentences.",
                "Parameter semantics, arbitrary English obligations and behavioral correctness require independent review.",
                "Documentation refactors also require preserved source, rule/strength/location mapping and actual independent use.",
            ]),
        ),
    ]))
}

/// Brackets the caller's complete inspection, including later declaration reads.
pub fn with_validated<T>(pack: &Path, body: impl FnOnce(&Path, &Value) -> Result<T>) -> Result<T> {
    let (generation, surface) = registered_surface()?;
    validated_inner(pack, surface.as_deref(), Some(generation), body)
}
pub fn with_validated_surface<T>(
    pack: &Path,
    surface: Option<&dyn CommandSurface>,
    body: impl FnOnce(&Path, &Value) -> Result<T>,
) -> Result<T> {
    validated_inner(pack, surface, None, body)
}
fn validated_inner<T>(
    pack: &Path,
    surface: Option<&dyn CommandSurface>,
    memo: Option<u64>,
    body: impl FnOnce(&Path, &Value) -> Result<T>,
) -> Result<T> {
    let root = crate::fs::canonical_path(pack, true).map_err(malformed)?;
    Descriptor::pin_directory(&root).map_err(|e| {
        if e.raw_os_error() == Some(libc::ENOTDIR) {
            Invalid::new("inventory", ".", "Pack must be a directory")
        } else {
            malformed(e)
        }
    })?;
    let files = inventory(&root)?;
    let key = if memo.is_some() {
        Some(
            files
                .iter()
                .map(|(path, (raw, mode))| (path.clone(), *mode, Sha256::digest(raw).into()))
                .collect::<InventoryKey>(),
        )
    } else {
        None
    };
    let cached = if let Some(key) = &key {
        LAST_VALIDATION
            .read()
            .map_err(|_| malformed("Pack validation memo is poisoned"))?
            .as_ref()
            .filter(|(generation, prior, _)| Some(*generation) == memo && prior == key)
            .map(|(_, _, value)| value.clone())
    } else {
        None
    };
    let validation = if let Some(value) = cached {
        value
    } else {
        validate_inventory(&files, surface)?
    };
    let result = body(&root, &validation)?;
    require(
        files == inventory(&root)?,
        "changed-pack",
        ".",
        "Pack changed during validation",
    )?;
    if let (Some(generation), Some(key)) = (memo, key) {
        *LAST_VALIDATION
            .write()
            .map_err(|_| malformed("Pack validation memo is poisoned"))? =
            Some((generation, key, validation));
    }
    Ok(result)
}
pub fn validate(pack: &Path) -> Result<Value> {
    with_validated(pack, |_, v| Ok(v.clone()))
}
pub fn validate_with_surface(pack: &Path, surface: Option<&dyn CommandSurface>) -> Result<Value> {
    with_validated_surface(pack, surface, |_, v| Ok(v.clone()))
}
pub fn command(pack: &Path) -> crate::lifecycle::CommandResult {
    let value = validate(pack).unwrap_or_else(|e| {
        object([
            ("schema", string("agent-memory/pack-validation-v1")),
            ("valid", Value::Bool(false)),
            ("writes_performed", Value::Bool(false)),
            ("candidate_code_executed", Value::Bool(false)),
            ("semantic_completeness_verified", Value::Bool(false)),
            ("issues", Value::Array(vec![e.issue()])),
        ])
    });
    let exit_code = if get(&value, "valid") == &Value::Bool(true) {
        0
    } else {
        1
    };
    crate::lifecycle::CommandResult::json(value, exit_code, json::INDENT2_ASCII)
}
