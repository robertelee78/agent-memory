use crate::error::{ContractError, Result};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{self, Write};

pub const MAX_DEPTH: usize = 256;
pub const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;

/// A validated, canonical decimal integer; never passes through floating point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Integer(String);

impl Integer {
    pub fn new(decimal: &str) -> Result<Self> {
        let digits = decimal.strip_prefix('-').unwrap_or(decimal);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit())
            || (digits.len() > 1 && digits.starts_with('0'))
        {
            return Err(ContractError::new("Invalid JSON integer"));
        }
        Ok(Self(if decimal == "-0" { "0".into() } else { decimal.into() }))
    }

    pub fn as_str(&self) -> &str { &self.0 }
    pub fn as_u64(&self) -> Option<u64> { self.0.parse().ok() }
    pub fn as_i64(&self) -> Option<i64> { self.0.parse().ok() }
    pub fn as_i128(&self) -> Option<i128> { self.0.parse().ok() }
}

impl fmt::Display for Integer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

macro_rules! integer_from {
    ($($t:ty),*) => { $(impl From<$t> for Integer {
        fn from(value: $t) -> Self { Self(value.to_string()) }
    })* };
}
integer_from!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize);

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(Integer),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Self::Object(pairs) => pairs.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        if let Self::Object(v) = self { Some(v) } else { None }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        if let Self::Array(v) = self { Some(v) } else { None }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(v) = self { Some(v) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(v) = self { Some(*v) } else { None }
    }
    pub fn as_int(&self) -> Option<&Integer> {
        if let Self::Int(v) = self { Some(v) } else { None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuplicateKeys { Reject, LastWins }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonFinite { Reject, Accept }

#[derive(Clone, Copy, Debug)]
pub struct DecodePolicy {
    pub duplicate_keys: DuplicateKeys,
    pub non_finite: NonFinite,
    pub max_bytes: usize,
    /// At most MAX_DEPTH containers; a larger requested limit cannot weaken this ceiling.
    pub max_depth: usize,
}
impl Default for DecodePolicy {
    fn default() -> Self {
        Self { duplicate_keys: DuplicateKeys::LastWins, non_finite: NonFinite::Accept,
            max_bytes: DEFAULT_MAX_BYTES, max_depth: MAX_DEPTH }
    }
}

pub fn decode(text: &str, policy: DecodePolicy) -> Result<Value> {
    if text.len() > policy.max_bytes { return Err(ContractError::new("JSON byte limit exceeded")); }
    if text.starts_with('\u{feff}') { return Err(ContractError::new("JSON byte order mark is not permitted")); }
    let mut parser = Parser { text, pos: 0, policy };
    let value = parser.value(0)?;
    parser.whitespace();
    if parser.pos != text.len() { return Err(parser.error("Extra data")); }
    Ok(value)
}

pub fn decode_bytes(bytes: &[u8], policy: DecodePolicy) -> Result<Value> {
    if bytes.len() > policy.max_bytes { return Err(ContractError::new("JSON byte limit exceeded")); }
    let text = std::str::from_utf8(bytes).map_err(|_| ContractError::new("JSON must be valid UTF-8"))?;
    decode(text, policy)
}

struct Parser<'a> { text: &'a str, pos: usize, policy: DecodePolicy }
impl Parser<'_> {
    fn error(&self, message: &str) -> ContractError {
        ContractError::new(format!("{message} at byte {}", self.pos))
    }
    fn peek(&self) -> Option<u8> { self.text.as_bytes().get(self.pos).copied() }
    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) { self.pos += 1; }
    }
    fn consume(&mut self, token: &str) -> bool {
        if self.text[self.pos..].starts_with(token) { self.pos += token.len(); true } else { false }
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        self.whitespace();
        match self.peek() {
            Some(b'"') => self.string().map(Value::String),
            Some(b'[' | b'{') => {
                if depth >= self.policy.max_depth.min(MAX_DEPTH) {
                    return Err(self.error("JSON depth limit exceeded"));
                }
                if self.peek() == Some(b'[') { self.array(depth + 1) } else { self.object(depth + 1) }
            }
            Some(b'n') if self.consume("null") => Ok(Value::Null),
            Some(b't') if self.consume("true") => Ok(Value::Bool(true)),
            Some(b'f') if self.consume("false") => Ok(Value::Bool(false)),
            Some(b'N') if self.consume("NaN") => self.non_finite(f64::NAN),
            Some(b'I') if self.consume("Infinity") => self.non_finite(f64::INFINITY),
            Some(b'-') if self.consume("-Infinity") => self.non_finite(f64::NEG_INFINITY),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("Expecting value")),
        }
    }
    fn non_finite(&self, value: f64) -> Result<Value> {
        if self.policy.non_finite == NonFinite::Reject {
            Err(ContractError::new("Non-finite JSON value"))
        } else { Ok(Value::Float(value)) }
    }
    fn array(&mut self, depth: usize) -> Result<Value> {
        self.pos += 1;
        self.whitespace();
        let mut values = Vec::new();
        if self.peek() == Some(b']') { self.pos += 1; return Ok(Value::Array(values)); }
        loop {
            values.push(self.value(depth)?);
            self.whitespace();
            match self.peek() {
                Some(b']') => { self.pos += 1; return Ok(Value::Array(values)); }
                Some(b',') => self.pos += 1,
                _ => return Err(self.error("Expecting ',' delimiter")),
            }
        }
    }
    fn object(&mut self, depth: usize) -> Result<Value> {
        self.pos += 1;
        self.whitespace();
        let mut pairs: Vec<(String, Value)> = Vec::new();
        let mut positions: Option<HashMap<String, usize>> = None;
        if self.peek() == Some(b'}') { self.pos += 1; return Ok(Value::Object(pairs)); }
        loop {
            self.whitespace();
            if self.peek() != Some(b'"') { return Err(self.error("Expecting property name enclosed in double quotes")); }
            let key = self.string()?;
            self.whitespace();
            if self.peek() != Some(b':') { return Err(self.error("Expecting ':' delimiter")); }
            self.pos += 1;
            let value = self.value(depth)?;
            if positions.is_none() && pairs.len() >= 8 {
                positions = Some(pairs.iter().enumerate()
                    .map(|(index, (key, _))| (key.clone(), index)).collect());
            }
            let previous = if let Some(positions) = &positions {
                positions.get(key.as_str()).copied()
            } else {
                pairs.iter().position(|(previous, _)| previous == &key)
            };
            if let Some(index) = previous {
                if self.policy.duplicate_keys == DuplicateKeys::Reject {
                    return Err(ContractError::new(format!("Duplicate metadata key: {key}")));
                }
                pairs[index].1 = value;
            } else {
                if let Some(positions) = &mut positions {
                    positions.insert(key.clone(), pairs.len());
                }
                pairs.push((key, value));
            }
            self.whitespace();
            match self.peek() {
                Some(b'}') => { self.pos += 1; return Ok(Value::Object(pairs)); }
                Some(b',') => self.pos += 1,
                _ => return Err(self.error("Expecting ',' delimiter")),
            }
        }
    }
    fn hex4(&mut self) -> Result<u16> {
        let mut value = 0u16;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(b @ b'0'..=b'9') => b - b'0',
                Some(b @ b'a'..=b'f') => b - b'a' + 10,
                Some(b @ b'A'..=b'F') => b - b'A' + 10,
                _ => return Err(self.error("Invalid \\uXXXX escape")),
            };
            self.pos += 1;
            value = value * 16 + u16::from(digit);
        }
        Ok(value)
    }
    fn string(&mut self) -> Result<String> {
        self.pos += 1;
        let mut result = String::new();
        let mut start = self.pos;
        loop {
            match self.peek() {
                None => return Err(self.error("Unterminated string")),
                Some(b'"') => {
                    result.push_str(&self.text[start..self.pos]);
                    self.pos += 1;
                    return Ok(result);
                }
                Some(0..=31) => return Err(self.error("Invalid control character")),
                Some(b'\\') => {
                    result.push_str(&self.text[start..self.pos]);
                    self.pos += 1;
                    let escape = self.peek().ok_or_else(|| self.error("Unterminated string"))?;
                    self.pos += 1;
                    match escape {
                        b'"' => result.push('"'), b'\\' => result.push('\\'), b'/' => result.push('/'),
                        b'b' => result.push('\u{8}'), b'f' => result.push('\u{c}'),
                        b'n' => result.push('\n'), b'r' => result.push('\r'), b't' => result.push('\t'),
                        b'u' => {
                            let high = self.hex4()?;
                            let code = if (0xd800..=0xdbff).contains(&high) {
                                if !self.consume("\\u") { return Err(self.error("Lone surrogate escape")); }
                                let low = self.hex4()?;
                                if !(0xdc00..=0xdfff).contains(&low) { return Err(self.error("Lone surrogate escape")); }
                                0x10000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
                            } else if (0xdc00..=0xdfff).contains(&high) {
                                return Err(self.error("Lone surrogate escape"));
                            } else { u32::from(high) };
                            result.push(char::from_u32(code).ok_or_else(|| self.error("Invalid Unicode escape"))?);
                        }
                        _ => return Err(self.error("Invalid escape")),
                    }
                    start = self.pos;
                }
                Some(_) => self.pos += 1,
            }
        }
    }
    fn number(&mut self) -> Result<Value> {
        let start = self.pos;
        if self.peek() == Some(b'-') { self.pos += 1; }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) { self.pos += 1; }
            }
            _ => return Err(self.error("Invalid number")),
        }
        let mut float = false;
        if self.peek() == Some(b'.') {
            float = true;
            self.pos += 1;
            let digits = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9')) { self.pos += 1; }
            if self.pos == digits { return Err(self.error("Invalid number")); }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) { self.pos += 1; }
            let digits = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9')) { self.pos += 1; }
            if self.pos == digits { return Err(self.error("Invalid number")); }
        }
        let lexeme = &self.text[start..self.pos];
        if float {
            let number = lexeme.parse::<f64>().map_err(|_| self.error("Invalid number"))?;
            // Python's parse_constant hook does not reject overflow in numeric literals.
            Ok(Value::Float(number))
        } else { Integer::new(lexeme).map(Value::Int) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub sort_keys: bool,
    pub compact: bool,
    pub indent2: bool,
    pub ensure_ascii: bool,
    pub allow_nan: bool,
}
impl Default for Style { fn default() -> Self { DEFAULT } }
pub const DEFAULT: Style = Style { sort_keys: false, compact: false, indent2: false, ensure_ascii: true, allow_nan: true };
pub const UTF8: Style = Style { ensure_ascii: false, ..DEFAULT };
pub const UTF8_FINITE: Style = Style { allow_nan: false, ..UTF8 };
pub const INDENT2_ASCII: Style = Style { indent2: true, ..DEFAULT };
pub const INDENT2_UTF8: Style = Style { indent2: true, ..UTF8 };
pub const INDENT2_UTF8_FINITE: Style = Style { allow_nan: false, ..INDENT2_UTF8 };
pub const SORTED_ASCII: Style = Style { sort_keys: true, ..DEFAULT };
pub const SORTED_UTF8: Style = Style { sort_keys: true, ..UTF8 };
pub const SORTED_INDENT2_ASCII: Style = Style { sort_keys: true, ..INDENT2_ASCII };
pub const SORTED_INDENT2_UTF8_FINITE: Style = Style { sort_keys: true, ..INDENT2_UTF8_FINITE };
pub const COMPACT_ASCII: Style = Style { compact: true, ..DEFAULT };
pub const COMPACT_UTF8: Style = Style { compact: true, ..UTF8 };
pub const SORTED_COMPACT_ASCII: Style = Style { sort_keys: true, ..COMPACT_ASCII };
pub const SORTED_COMPACT_UTF8: Style = Style { sort_keys: true, ..COMPACT_UTF8 };
pub const SORTED_COMPACT_UTF8_FINITE: Style = Style { allow_nan: false, ..SORTED_COMPACT_UTF8 };

pub fn encode(value: &Value, style: Style) -> Result<String> {
    let mut output = String::new();
    emit(value, style, 0, &mut output)?;
    Ok(output)
}

fn quoted(text: &str, ascii: bool, output: &mut String) {
    output.push('"');
    for c in text.chars() {
        match c {
            '"' => output.push_str("\\\""), '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"), '\r' => output.push_str("\\r"), '\t' => output.push_str("\\t"),
            '\u{8}' => output.push_str("\\b"), '\u{c}' => output.push_str("\\f"),
            c if c < '\u{20}' || (ascii && c >= '\u{7f}') => {
                let code = c as u32;
                if code <= 0xffff { let _ = write!(output, "\\u{code:04x}"); }
                else {
                    let high = 0xd800 + ((code - 0x10000) >> 10);
                    let low = 0xdc00 + ((code - 0x10000) & 0x3ff);
                    let _ = write!(output, "\\u{high:04x}\\u{low:04x}");
                }
            }
            c => output.push(c),
        }
    }
    output.push('"');
}
fn newline(depth: usize, output: &mut String) {
    output.push('\n');
    for _ in 0..depth { output.push_str("  "); }
}
fn separator(index: usize, depth: usize, style: Style, output: &mut String) {
    if index > 0 {
        output.push(',');
        if !style.indent2 && !style.compact { output.push(' '); }
    }
    if style.indent2 { newline(depth + 1, output); }
}
fn emit(value: &Value, style: Style, depth: usize, output: &mut String) -> Result<()> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(v) => output.push_str(if *v { "true" } else { "false" }),
        Value::Int(v) => output.push_str(v.as_str()),
        Value::Float(v) => {
            if !v.is_finite() {
                if !style.allow_nan { return Err(ContractError::new("Out of range float values are not JSON compliant")); }
                output.push_str(if v.is_nan() { "NaN" } else if v.is_sign_negative() { "-Infinity" } else { "Infinity" });
            } else { float_repr(*v, output); }
        }
        Value::String(v) => quoted(v, style.ensure_ascii, output),
        Value::Array(values) => {
            if depth >= MAX_DEPTH { return Err(ContractError::new("JSON depth limit exceeded")); }
            output.push('[');
            for (i, value) in values.iter().enumerate() {
                separator(i, depth, style, output);
                emit(value, style, depth + 1, output)?;
            }
            if style.indent2 && !values.is_empty() { newline(depth, output); }
            output.push(']');
        }
        Value::Object(pairs) => {
            if depth >= MAX_DEPTH { return Err(ContractError::new("JSON depth limit exceeded")); }
            output.push('{');
            if style.sort_keys {
                // Sort references, not recursively cloned metadata.
                let mut sorted = pairs.iter().collect::<Vec<_>>();
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                for (i, pair) in sorted.into_iter().enumerate() {
                    emit_pair(pair, i, style, depth, output)?;
                }
            } else {
                for (i, pair) in pairs.iter().enumerate() {
                    emit_pair(pair, i, style, depth, output)?;
                }
            }
            if style.indent2 && !pairs.is_empty() { newline(depth, output); }
            output.push('}');
        }
    }
    Ok(())
}

fn emit_pair(pair: &(String, Value), index: usize, style: Style, depth: usize, output: &mut String) -> Result<()> {
    separator(index, depth, style, output);
    quoted(&pair.0, style.ensure_ascii, output);
    output.push(':');
    if !style.compact || style.indent2 { output.push(' '); }
    emit(&pair.1, style, depth + 1, output)
}

// Fixed storage suffices for every finite binary64/decimal midpoint comparison.
struct Wide([u32; 40]);
impl Wide {
    fn new(value: u64) -> Self {
        let mut words = [0; 40]; words[0] = value as u32; words[1] = (value >> 32) as u32;
        Self(words)
    }
    fn multiply5(&mut self, count: usize) {
        for _ in 0..count {
            let mut carry = 0u64;
            for word in &mut self.0 {
                let product = u64::from(*word) * 5 + carry;
                *word = product as u32; carry = product >> 32;
            }
            debug_assert_eq!(carry, 0);
        }
    }
    fn shift(&mut self, bits: usize) {
        let words = bits / 32;
        let remainder = bits % 32;
        for i in (0..40).rev() {
            self.0[i] = if i < words { 0 } else {
                (self.0[i - words] << remainder)
                    | if remainder > 0 && i > words { self.0[i - words - 1] >> (32 - remainder) } else { 0 }
            };
        }
    }
    fn cmp(&self, rhs: &Self) -> Ordering { self.0.iter().rev().cmp(rhs.0.iter().rev()) }
}

// Compare twice the binary value with an odd decimal midpoint coefficient.
fn midpoint_cmp(value: f64, midpoint: u64, decimal_exp: i32) -> Ordering {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, binary_exp) = if exponent == 0 { (fraction, -1074) }
        else { (fraction | (1u64 << 52), exponent - 1023 - 52) };
    let mut binary = Wide::new(mantissa * 2);
    let mut decimal = Wide::new(midpoint);
    if decimal_exp >= 0 { decimal.multiply5(decimal_exp as usize); }
    else { binary.multiply5((-decimal_exp) as usize); }
    let shift = binary_exp - decimal_exp;
    if shift >= 0 { binary.shift(shift as usize); } else { decimal.shift((-shift) as usize); }
    binary.cmp(&decimal)
}

struct FloatText { bytes: [u8; 64], len: usize }
impl FloatText {
    fn new() -> Self { Self { bytes: [0; 64], len: 0 } }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).expect("numeric formatting is ASCII")
    }
}
impl fmt::Write for FloatText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        if end > self.bytes.len() { return Err(fmt::Error); }
        self.bytes[self.len..end].copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn float_repr(value: f64, output: &mut String) {
    if value.is_sign_negative() { output.push('-'); }
    let value = value.abs();
    if value == 0.0 { output.push_str("0.0"); return; }
    let mut scientific = FloatText::new();
    write!(scientific, "{value:e}").expect("binary64 fits numeric buffer");
    let (digits, exponent) = scientific.as_str().split_once('e').expect("finite float scientific format");
    let exponent: i32 = exponent.parse().expect("formatted exponent");
    let mut coefficient = 0u64;
    let mut count = 0usize;
    for b in digits.bytes().filter(|b| *b != b'.') {
        coefficient = coefficient * 10 + u64::from(b - b'0'); count += 1;
    }
    let mut decimal_exp = exponent - count as i32 + 1;
    // Rust's shortest formatter can choose the other last digit at a decimal tie.
    // CPython selects the nearest candidate, breaking exact ties to an even digit.
    for candidate in [coefficient - 1, coefficient + 1] {
        if candidate == 0 { continue; }
        let mut candidate_text = FloatText::new();
        write!(candidate_text, "{candidate}e{decimal_exp}").expect("binary64 candidate fits numeric buffer");
        if candidate_text.as_str().parse::<f64>().ok() != Some(value) { continue; }
        let midpoint = coefficient + candidate;
        let comparison = midpoint_cmp(value, midpoint, decimal_exp);
        let better = if candidate < coefficient { comparison == Ordering::Less }
            else { comparison == Ordering::Greater };
        if better || (comparison == Ordering::Equal && candidate % 2 == 0) {
            coefficient = candidate;
        }
    }
    while coefficient % 10 == 0 { coefficient /= 10; decimal_exp += 1; }
    let mut coefficient_text = FloatText::new();
    write!(coefficient_text, "{coefficient}").expect("binary64 coefficient fits numeric buffer");
    let digits = coefficient_text.as_str();
    let exponent = decimal_exp + digits.len() as i32 - 1;
    if !(-4..16).contains(&exponent) {
        output.push(digits.as_bytes()[0] as char);
        if digits.len() > 1 { output.push('.'); output.push_str(&digits[1..]); }
        output.push('e');
        output.push(if exponent < 0 { '-' } else { '+' });
        let _ = write!(output, "{:02}", exponent.abs());
    } else {
        let point = exponent + 1;
        if point <= 0 {
            output.push_str("0.");
            for _ in 0..-point { output.push('0'); }
            output.push_str(digits);
        } else if point as usize >= digits.len() {
            output.push_str(digits);
            for _ in digits.len()..point as usize { output.push('0'); }
            output.push_str(".0");
        } else {
            output.push_str(&digits[..point as usize]); output.push('.'); output.push_str(&digits[point as usize..]);
        }
    }
}
