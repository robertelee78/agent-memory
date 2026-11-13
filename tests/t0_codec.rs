use agent_memory::codec::{frontmatter, json::{self, DecodePolicy, DuplicateKeys, NonFinite, Style, Value}};

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or_else(|| panic!("missing vector field {key}"))
}
fn string(value: &Value) -> &str { value.as_str().expect("vector string") }
fn boolean(value: &Value) -> bool { value.as_bool().expect("vector boolean") }
fn array(value: &Value) -> &[Value] { value.as_array().expect("vector array") }
fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors").join(name);
    let bytes = std::fs::read(path).expect("read golden vectors");
    json::decode_bytes(&bytes, DecodePolicy::default()).expect("decode golden vectors")
}
fn style(value: &Value) -> Style {
    Style { sort_keys: boolean(field(value, "sort_keys")), compact: boolean(field(value, "compact")),
        indent2: !matches!(field(value, "indent"), Value::Null),
        ensure_ascii: boolean(field(value, "ensure_ascii")), allow_nan: boolean(field(value, "allow_nan")) }
}

#[test]
fn python_json_encode_vectors_are_byte_exact() {
    let vectors = fixture("json-codec-v1.json");
    assert_eq!(string(field(&vectors, "schema")), "agent-memory/test-vectors-json-codec-v1");
    let styles = field(&vectors, "styles");
    for row in array(field(&vectors, "encode")) {
        let id = string(field(row, "id"));
        let value = json::decode(string(field(row, "input")), DecodePolicy::default()).unwrap();
        for (name, expected) in field(row, "outputs").as_object().unwrap() {
            let result = json::encode(&value, style(field(styles, name)));
            if matches!(expected, Value::Null) { assert!(result.is_err(), "{id}/{name}"); }
            else { assert_eq!(result.unwrap(), string(expected), "{id}/{name}"); }
        }
    }
}

#[test]
fn python_json_decode_vectors_match_every_policy() {
    let vectors = fixture("json-codec-v1.json");
    for row in array(field(&vectors, "decode")) {
        let id = string(field(row, "id"));
        let text = string(field(row, "text"));
        for (name, expected) in field(row, "policies").as_object().unwrap() {
            let policy = DecodePolicy {
                duplicate_keys: if name == "default" { DuplicateKeys::LastWins } else { DuplicateKeys::Reject },
                non_finite: if name == "unique_finite" { NonFinite::Reject } else { NonFinite::Accept },
                ..DecodePolicy::default()
            };
            let result = json::decode(text, policy);
            if let Some(delta) = row.get("rust") {
                assert!(!boolean(field(delta, "ok")), "unexpected declared delta {id}");
                let error = result.expect_err(id).to_string();
                let required = match string(field(delta, "reason")) {
                    "lone-surrogate" => "Lone surrogate escape", "depth" => "depth",
                    other => panic!("unknown declared delta {other}"),
                };
                assert!(error.contains(required), "{id}/{name}: {error}");
            } else if boolean(field(expected, "ok")) {
                let normalized = json::encode(&result.unwrap_or_else(|e| panic!("{id}/{name}: {e}")), json::SORTED_COMPACT_ASCII).unwrap();
                assert_eq!(normalized, string(field(expected, "normalized")), "{id}/{name}");
            } else { assert!(result.is_err(), "{id}/{name} unexpectedly accepted"); }
        }
    }
}

#[test]
fn python_frontmatter_read_vectors_match() {
    let vectors = fixture("frontmatter-v1.json");
    assert_eq!(string(field(&vectors, "schema")), "agent-memory/test-vectors-frontmatter-v1");
    for row in array(field(&vectors, "read")) {
        let id = string(field(row, "id"));
        let allowed: Vec<&str> = array(field(row, "allowed")).iter().map(string).collect();
        let result = frontmatter::read(string(field(row, "text")), &allowed);
        if boolean(field(row, "ok")) {
            let (metadata, body) = result.unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(json::encode(&Value::Object(metadata), json::COMPACT_UTF8).unwrap(), string(field(row, "metadata")), "{id}");
            assert_eq!(body, string(field(row, "body")), "{id}");
        } else {
            let error = result.expect_err(id).to_string();
            let expected = string(field(row, "error"));
            let prefix = "Invalid JSON-compatible metadata value: ";
            if expected.starts_with(prefix) { assert!(error.starts_with(prefix), "{id}: {error}"); }
            else { assert_eq!(error, expected, "{id}"); }
        }
    }
}

#[test]
fn python_frontmatter_write_vectors_are_byte_exact() {
    let vectors = fixture("frontmatter-v1.json");
    for row in array(field(&vectors, "write")) {
        let id = string(field(row, "id"));
        let metadata = json::decode(string(field(row, "metadata")), DecodePolicy::default()).unwrap();
        let result = frontmatter::write(metadata.as_object().unwrap());
        let expected = field(row, "output");
        if matches!(expected, Value::Null) { assert!(result.is_err(), "{id}"); }
        else { assert_eq!(result.unwrap(), string(expected), "{id}"); }
    }
}

#[test]
fn integers_never_lose_inode_or_nanosecond_bits() {
    for text in ["9007199254740993", "18446744073709551615", "18446744073709551616",
        "-170141183460469231731687303715884105729", "123456789012345678901234567890123456789012345678901234567890"]
    {
        let value = json::decode(text, DecodePolicy::default()).unwrap();
        assert!(matches!(value, Value::Int(_)));
        assert_eq!(json::encode(&value, json::DEFAULT).unwrap(), text);
    }
    assert_eq!(json::encode(&json::decode("-0", DecodePolicy::default()).unwrap(), json::DEFAULT).unwrap(), "0");
    assert_ne!(json::decode("true", DecodePolicy::default()).unwrap(), json::decode("1", DecodePolicy::default()).unwrap());
}

#[test]
fn bounds_refuse_before_unbounded_recursion() {
    assert!(json::decode("null", DecodePolicy { max_bytes: 3, ..DecodePolicy::default() }).unwrap_err().to_string().contains("byte"));
    assert!(json::decode("[[]]", DecodePolicy { max_depth: 1, ..DecodePolicy::default() }).unwrap_err().to_string().contains("depth"));
    let permitted = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    assert!(json::decode(&permitted, DecodePolicy::default()).is_ok());
    for depth in [257, 100_000] {
        let text = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        assert!(json::decode(&text, DecodePolicy::default()).unwrap_err().to_string().contains("depth"));
    }
    assert!(json::decode("[]", DecodePolicy { max_depth: 0, ..DecodePolicy::default() }).is_err());
    assert!(json::decode("0", DecodePolicy { max_depth: 0, ..DecodePolicy::default() }).is_ok());
}

#[test]
fn duplicate_assignment_keeps_first_key_position() {
    let text = r#"{"b":1,"a":2,"b":3}"#;
    let value = json::decode(text, DecodePolicy::default()).unwrap();
    assert_eq!(json::encode(&value, json::COMPACT_ASCII).unwrap(), r#"{"b":3,"a":2}"#);
    assert_eq!(value.get("b").unwrap().as_int().unwrap().as_u64(), Some(3));
    let strict = DecodePolicy { duplicate_keys: DuplicateKeys::Reject, ..DecodePolicy::default() };
    assert_eq!(json::decode(text, strict).unwrap_err().to_string(), "Duplicate metadata key: b");
    assert_eq!(json::decode(r#"{"x":{"a":1,"a":2}}"#, strict).unwrap_err().to_string(), "Duplicate metadata key: a");
}

#[test]
fn float_layout_boundaries_match_cpython() {
    for (number, expected) in [
        (1e-5, "1e-05"), (0.0001, "0.0001"), (1e16, "1e+16"),
        (9999999999999998.0, "9999999999999998.0"), (f64::from_bits(1), "5e-324"),
        (f64::MAX, "1.7976931348623157e+308"), (-0.0, "-0.0"),
        (1.0, "1.0"), (100000.0, "100000.0"), (1.5e16, "1.5e+16"),
        (2.9802322387695312e-8, "2.9802322387695312e-08"),
    ] {
        assert_eq!(json::encode(&Value::Float(number), json::DEFAULT).unwrap(), expected);
    }
}

#[test]
fn bytes_and_surrogates_are_explicit_refusals() {
    assert!(json::decode_bytes(b"\xff", DecodePolicy::default()).unwrap_err().to_string().contains("UTF-8"));
    assert!(json::decode_bytes(b"\xef\xbb\xbf{}", DecodePolicy::default()).unwrap_err().to_string().contains("byte order mark"));
    for text in [r#""\ud800""#, r#""\udc00""#, r#""\udc00\ud800""#, r#"{"\ud800":0}"#] {
        assert!(json::decode(text, DecodePolicy::default()).unwrap_err().to_string().contains("Lone surrogate escape"));
    }
    assert_eq!(json::decode(r#""\ud83d\ude00""#, DecodePolicy::default()).unwrap(), Value::String("😀".into()));
}

#[test]
fn frontmatter_preserves_python_unicode_line_and_strip_semantics() {
    let text = "---\ntitle: \u{1f}one\u{1f}\u{85}tags:\u{2028}  - 'two'\u{2029}count: 3\n---\nbody\r\n";
    let (metadata, body) = frontmatter::read(text, &["title", "tags", "count"]).unwrap();
    assert_eq!(metadata[0].1, Value::String("one".into()));
    assert_eq!(metadata[1].1, Value::Array(vec![Value::String("two".into())]));
    assert_eq!(body, "body\r\n");
    assert_eq!(frontmatter::scalar("'a'''b'").unwrap(), Value::String("a''b".into()));
    let written = frontmatter::write(&[("title".into(), Value::String("a\u{85}b\u{2028}c\u{2029}".into()))]).unwrap();
    assert_eq!(written, "---\ntitle: \"a\\u0085b\\u2028c\\u2029\"\n---\n");
}

#[test]
fn unicode_decimal_digits_are_not_confused_with_other_numeric_characters() {
    for digit in ["١", "０", "\u{11f50}", "\u{1e4f0}"] {
        assert!(frontmatter::scalar(digit).unwrap_err().to_string()
            .starts_with("Invalid JSON-compatible metadata value: "));
    }
    for numeric in ["²", "Ⅷ", "½"] {
        assert_eq!(frontmatter::scalar(numeric).unwrap(), Value::String(numeric.into()));
    }
}

#[test]
fn large_objects_preserve_order_and_duplicate_policy() {
    use std::fmt::Write;
    const COUNT: usize = 200_000;
    for repeat_first in [false, true] {
        let mut text = String::with_capacity(COUNT * 20);
        text.push('{');
        for index in 0..COUNT {
            if index > 0 { text.push(','); }
            if repeat_first && index == COUNT - 1 {
                write!(text, "\"key\\u0030\":{index}").unwrap();
            } else {
                write!(text, "\"key{index}\":{index}").unwrap();
            }
        }
        text.push('}');
        let value = json::decode(&text, DecodePolicy::default()).unwrap();
        let pairs = value.as_object().unwrap();
        assert_eq!(pairs.len(), COUNT - usize::from(repeat_first));
        for (index, (key, value)) in pairs.iter().enumerate() {
            assert_eq!(key, &format!("key{index}"));
            let expected = if repeat_first && index == 0 { COUNT - 1 } else { index };
            assert_eq!(value.as_int().unwrap().as_u64(), Some(expected as u64));
        }
        let strict = DecodePolicy { duplicate_keys: DuplicateKeys::Reject, ..DecodePolicy::default() };
        let result = json::decode(&text, strict);
        if repeat_first {
            assert_eq!(result.unwrap_err().to_string(), "Duplicate metadata key: key0");
        } else {
            assert_eq!(result.unwrap().as_object().unwrap().len(), COUNT);
        }
    }
}
