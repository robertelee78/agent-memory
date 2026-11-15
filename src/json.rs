//! Small ordered JSON value: parse (duplicate keys refused) and the few
//! encodings the on-disk formats use.
use crate::util::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Value::Object(pairs) => pairs.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    /// The field as text, when present and a string.
    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }
    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object(pairs) => Some(pairs),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
    /// Set or append a field on an object.
    pub fn set(&mut self, key: &str, value: Value) {
        if let Value::Object(pairs) = self {
            match pairs.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = value,
                None => pairs.push((key.to_owned(), value)),
            }
        }
    }
    pub fn remove(&mut self, key: &str) {
        if let Value::Object(pairs) = self {
            pairs.retain(|(k, _)| k != key);
        }
    }
}

pub fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
pub fn s(text: impl Into<String>) -> Value {
    Value::String(text.into())
}
pub fn strings<I: IntoIterator<Item = S>, S: Into<String>>(items: I) -> Value {
    Value::Array(items.into_iter().map(|v| Value::String(v.into())).collect())
}

// ---------------------------------------------------------------- parsing

pub fn parse(raw: &[u8]) -> Result<Value> {
    let text = std::str::from_utf8(raw).map_err(|_| Error::new("JSON is not UTF-8"))?;
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
        depth: 0,
    };
    parser.space();
    let value = parser.value()?;
    parser.space();
    if parser.at != parser.bytes.len() {
        return Err(parser.error("trailing data"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    depth: usize,
}

impl Parser<'_> {
    fn error(&self, what: &str) -> Error {
        Error::new(format!("Invalid JSON at byte {}: {what}", self.at))
    }
    fn space(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.at += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> Result<()> {
        if self.bytes.get(self.at) == Some(&byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", byte as char)))
        }
    }
    fn literal(&mut self, word: &str, value: Value) -> Result<Value> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("unknown literal"))
        }
    }
    fn value(&mut self) -> Result<Value> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error("nesting too deep"));
        }
        let value = match self.bytes.get(self.at) {
            Some(b'{') => self.object()?,
            Some(b'[') => self.array()?,
            Some(b'"') => Value::String(self.string()?),
            Some(b't') => self.literal("true", Value::Bool(true))?,
            Some(b'f') => self.literal("false", Value::Bool(false))?,
            Some(b'n') => self.literal("null", Value::Null)?,
            Some(b'-' | b'0'..=b'9') => self.number()?,
            _ => return Err(self.error("unexpected input")),
        };
        self.depth -= 1;
        Ok(value)
    }
    fn object(&mut self) -> Result<Value> {
        self.eat(b'{')?;
        let mut pairs: Vec<(String, Value)> = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(Value::Object(pairs));
        }
        loop {
            self.space();
            let key = self.string()?;
            if pairs.iter().any(|(k, _)| *k == key) {
                return Err(Error::new(format!("Duplicate JSON key: {key}")));
            }
            self.space();
            self.eat(b':')?;
            self.space();
            let value = self.value()?;
            pairs.push((key, value));
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Object(pairs));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }
    fn array(&mut self) -> Result<Value> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b']') {
            self.at += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.space();
            items.push(self.value()?);
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }
    fn hex4(&mut self) -> Result<u32> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u32::from_str_radix(d, 16).ok())
            .ok_or_else(|| self.error("bad \\u escape"))?;
        self.at += 4;
        Ok(digits)
    }
    fn string(&mut self) -> Result<String> {
        self.eat(b'"')?;
        let mut out = String::new();
        loop {
            let start = self.at;
            while self
                .bytes
                .get(self.at)
                .is_some_and(|b| *b != b'"' && *b != b'\\' && *b >= 0x20)
            {
                self.at += 1;
            }
            // The input is valid UTF-8 and the run stops at ASCII bytes.
            out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or_default());
            match self.bytes.get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = *self
                        .bytes
                        .get(self.at)
                        .ok_or_else(|| self.error("bad escape"))?;
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xd800..0xdc00).contains(&code)
                                && self.bytes[self.at..].starts_with(b"\\u")
                            {
                                self.at += 2;
                                let low = self.hex4()?;
                                if !(0xdc00..0xe000).contains(&low) {
                                    return Err(self.error("bad surrogate pair"));
                                }
                                code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
                            }
                            out.push(
                                char::from_u32(code).ok_or_else(|| self.error("lone surrogate"))?,
                            );
                        }
                        _ => return Err(self.error("bad escape")),
                    }
                }
                _ => return Err(self.error("unterminated string")),
            }
        }
    }
    fn number(&mut self) -> Result<Value> {
        let start = self.at;
        let mut float = false;
        while let Some(b) = self.bytes.get(self.at) {
            match b {
                b'0'..=b'9' | b'-' | b'+' => {}
                b'.' | b'e' | b'E' => float = true,
                _ => break,
            }
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or_default();
        if !float {
            if let Ok(n) = text.parse::<i64>() {
                return Ok(Value::Int(n));
            }
        }
        text.parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(Value::Float)
            .ok_or_else(|| self.error("bad number"))
    }
}

// ---------------------------------------------------------------- encoding

/// Encoding layout. `Pretty` is two-space indentation; `Compact` has no
/// spaces; `Python` is `json.dumps` default `", "`/`": "` separators.
#[derive(Clone, Copy, PartialEq)]
pub enum Layout {
    Compact,
    Python,
    Pretty,
}

#[derive(Clone, Copy)]
pub struct Style {
    pub layout: Layout,
    pub sort_keys: bool,
    pub ascii: bool,
}

/// Canonical form used for signed inventories and digests.
pub const CANONICAL: Style = Style {
    layout: Layout::Compact,
    sort_keys: true,
    ascii: true,
};
/// Directory fingerprints: Python `json.dumps(indent=2, sort_keys=True)`.
pub const SORTED_PRETTY_ASCII: Style = Style {
    layout: Layout::Pretty,
    sort_keys: true,
    ascii: true,
};
/// Human-edited configuration files (keeps key order and Unicode).
pub const PRETTY: Style = Style {
    layout: Layout::Pretty,
    sort_keys: false,
    ascii: false,
};
/// Frontmatter values: one line, Python default separators, Unicode kept.
pub const LINE: Style = Style {
    layout: Layout::Python,
    sort_keys: false,
    ascii: false,
};

pub fn encode(value: &Value, style: Style) -> String {
    let mut out = String::new();
    write_value(&mut out, value, style, 0);
    out
}

/// Pretty encoding plus a final newline, for files.
pub fn pretty_file(value: &Value) -> Vec<u8> {
    let mut text = encode(value, PRETTY);
    text.push('\n');
    text.into_bytes()
}

fn write_string(out: &mut String, text: &str, ascii: bool) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (ascii && (c as u32) > 0x7f) => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn newline(out: &mut String, style: Style, depth: usize) {
    if style.layout == Layout::Pretty {
        out.push('\n');
        for _ in 0..depth {
            out.push_str("  ");
        }
    }
}

fn write_value(out: &mut String, value: &Value, style: Style, depth: usize) {
    let (item, colon) = match style.layout {
        Layout::Compact => (",", ":"),
        Layout::Python => (", ", ": "),
        Layout::Pretty => (",", ": "),
    };
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e16 {
                out.push_str(&format!("{f:.1}"));
            } else {
                out.push_str(&f.to_string());
            }
        }
        Value::String(text) => write_string(out, text, style.ascii),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, child) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(item);
                }
                newline(out, style, depth + 1);
                write_value(out, child, style, depth + 1);
            }
            newline(out, style, depth);
            out.push(']');
        }
        Value::Object(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut ordered: Vec<&(String, Value)> = pairs.iter().collect();
            if style.sort_keys {
                // Python sorts by code point; UTF-16 order differs only for
                // astral characters, which keys here never contain.
                ordered.sort_by(|a, b| a.0.cmp(&b.0));
            }
            out.push('{');
            for (index, (key, child)) in ordered.into_iter().enumerate() {
                if index > 0 {
                    out.push_str(item);
                }
                newline(out, style, depth + 1);
                write_string(out, key, style.ascii);
                out.push_str(colon);
                write_value(out, child, style, depth + 1);
            }
            newline(out, style, depth);
            out.push('}');
        }
    }
}

/// Structural equality independent of object key order.
pub fn same(left: &Value, right: &Value) -> bool {
    encode(left, CANONICAL) == encode(right, CANONICAL)
}
