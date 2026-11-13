use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::error::{ContractError, Result};

pub type Metadata = Vec<(String, Value)>;
pub const LINE_ESCAPES: [(char, &str); 3] = [
    ('\u{85}', "\\u0085"), ('\u{2028}', "\\u2028"), ('\u{2029}', "\\u2029"),
];

fn whitespace(c: char) -> bool { c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c) }
fn strip(text: &str) -> &str { text.trim_matches(whitespace) }

// Python's Unicode decimal category, unlike Rust's broader is_numeric().
fn decimal(c: char) -> bool {
    const ZEROES: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66,
        0xbe6, 0xc66, 0xce6, 0xd66, 0xde6, 0xe50, 0xed0, 0xf20, 0x1040,
        0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90, 0x1b50,
        0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0,
        0xaa50, 0xabf0, 0xff10, 0x104a0, 0x10d30, 0x11066, 0x110f0,
        0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0, 0x11650, 0x116c0,
        0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60,
        0x16ac0, 0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6,
        0x1e140, 0x1e2f0, 0x1e4f0, 0x1e950, 0x1fbf0,
    ];
    let code = c as u32;
    ZEROES.iter().any(|zero| code >= *zero && code - *zero < 10)
}

fn number_like(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    if chars.peek() == Some(&'-') { chars.next(); }
    let mut digits = 0;
    while chars.peek().is_some_and(|c| decimal(*c)) { chars.next(); digits += 1; }
    if digits == 0 { return false; }
    if chars.peek() == Some(&'.') {
        chars.next(); digits = 0;
        while chars.peek().is_some_and(|c| decimal(*c)) { chars.next(); digits += 1; }
        if digits == 0 { return false; }
    }
    if chars.peek().is_some_and(|c| *c == 'e' || *c == 'E') {
        chars.next();
        if chars.peek().is_some_and(|c| *c == '+' || *c == '-') { chars.next(); }
        digits = 0;
        while chars.peek().is_some_and(|c| decimal(*c)) { chars.next(); digits += 1; }
        if digits == 0 { return false; }
    }
    chars.next().is_none()
}

pub fn scalar(value: &str) -> Result<Value> {
    let value = strip(value);
    if value.is_empty() { return Err(ContractError::new("Empty metadata value")); }
    if matches!(value.as_bytes()[0], b'[' | b'{' | b'"')
        || matches!(value, "true" | "false" | "null") || number_like(value)
    {
        return json::decode(value, DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject, non_finite: NonFinite::Reject,
            ..DecodePolicy::default()
        }).map_err(|error| ContractError::new(format!("Invalid JSON-compatible metadata value: {error}")));
    }
    if value.starts_with('\'') {
        if value.len() < 2 || !value.ends_with('\'') {
            return Err(ContractError::new("Invalid single-quoted string"));
        }
        let inside = &value[1..value.len() - 1];
        let bytes = inside.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\'' && (i == 0 || bytes[i - 1] != b'\'')
                && (i + 1 == bytes.len() || bytes[i + 1] != b'\'')
            {
                return Err(ContractError::new("Invalid single-quoted string"));
            }
        }
        return Ok(Value::String(inside.replace("''", "'")));
    }
    if matches!(value.as_bytes()[0], b'!' | b'&' | b'*' | b'|' | b'>')
        || value.contains(" #") || value.contains(": ")
        || value.chars().any(|c| matches!(c, '{' | '}' | '[' | ']' | '\t'))
    {
        return Err(ContractError::new("Unsupported YAML syntax; quote the scalar or use JSON flow syntax"));
    }
    Ok(Value::String(value.into()))
}

struct Lines<'a>(&'a str);
impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() { return None; }
        for (offset, c) in self.0.char_indices() {
            if matches!(c, '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}') {
                let line = &self.0[..offset];
                let mut end = offset + c.len_utf8();
                if c == '\r' && self.0[end..].starts_with('\n') { end += 1; }
                self.0 = &self.0[end..];
                return Some(line);
            }
        }
        let line = self.0;
        self.0 = "";
        Some(line)
    }
}

fn key_value(line: &str) -> Option<(&str, Option<&str>)> {
    let (key, rest) = line.split_once(':')?;
    if !key.as_bytes().first()?.is_ascii_lowercase()
        || !key.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    { return None; }
    if rest.is_empty() { Some((key, None)) }
    else { rest.strip_prefix(' ').map(|value| (key, Some(value))) }
}

pub fn read<'a>(text: &'a str, allowed: &[&str]) -> Result<(Metadata, &'a str)> {
    if !text.starts_with("---\n") {
        return Err(ContractError::new("Frontmatter must start at byte zero with --- and LF"));
    }
    let end = text[4..].find("\n---\n").map(|index| index + 4)
        .ok_or_else(|| ContractError::new("Missing frontmatter terminator"))?;
    let mut lines = Lines(&text[4..end]).peekable();
    let mut metadata: Metadata = Vec::new();
    while let Some(line) = lines.next() {
        let (key, value) = key_value(line).ok_or_else(|| ContractError::new(
            "Metadata must use top-level known keys; unsupported nesting or syntax"))?;
        if !allowed.contains(&key) { return Err(ContractError::new(format!("Unknown metadata key: {key}"))); }
        if metadata.iter().any(|(previous, _)| previous == key) {
            return Err(ContractError::new(format!("Duplicate metadata key: {key}")));
        }
        let value = if value.is_none() || value == Some("") {
            let mut items = Vec::new();
            while let Some(line) = lines.peek().filter(|line| line.starts_with("  - ")) {
                let item = scalar(&line[4..])?;
                if !matches!(item, Value::String(_)) {
                    return Err(ContractError::new("Block-list items must be scalar strings"));
                }
                items.push(item);
                lines.next();
            }
            if items.is_empty() { return Err(ContractError::new("Metadata value or scalar block list required")); }
            Value::Array(items)
        } else { scalar(value.expect("nonempty metadata value"))? };
        metadata.push((key.into(), value));
    }
    Ok((metadata, &text[end + 5..]))
}

pub fn write(metadata: &[(String, Value)]) -> Result<String> {
    let mut text = String::from("---\n");
    for (key, value) in metadata {
        text.push_str(key);
        text.push_str(": ");
        let encoded = json::encode(value, json::UTF8_FINITE)?;
        for c in encoded.chars() {
            if let Some((_, escape)) = LINE_ESCAPES.iter().find(|(separator, _)| *separator == c) {
                text.push_str(escape);
            } else { text.push(c); }
        }
        text.push('\n');
    }
    text.push_str("---\n");
    Ok(text)
}
