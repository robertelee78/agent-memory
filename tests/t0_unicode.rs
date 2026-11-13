use agent_memory::{
    codec::json::{self, DecodePolicy, Value},
    unicode,
};
fn decode(value: &str) -> Value {
    json::decode(value, DecodePolicy::default()).unwrap()
}
fn hex(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}
fn scalars(value: &str) -> String {
    value
        .split_whitespace()
        .map(|s| char::from_u32(hex(s)).unwrap())
        .collect()
}
#[test]
fn casefold_table_is_exact_baseline_fixture() {
    let vector = decode(include_str!("vectors/casefold-v1.json"));
    let rows = vector.get("mappings").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), unicode::CASEFOLD.len());
    for (row, (source, target)) in rows.iter().zip(unicode::CASEFOLD) {
        let row = row.as_array().unwrap();
        assert_eq!(hex(row[0].as_str().unwrap()), *source);
        assert_eq!(scalars(row[1].as_str().unwrap()), *target);
    }
    assert_eq!(unicode::casefold("Straße Σςİ"), "strasse σσi\u{307}");
    assert_eq!(unicode::nfc_casefold("E\u{301}"), "é");
}
#[test]
fn word_ranges_are_exact_baseline_fixture() {
    let vector = decode(include_str!("vectors/unicode-alnum-v1.json"));
    let rows = vector.get("ranges").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), unicode::ALNUM.len());
    for (row, (start, end)) in rows.iter().zip(unicode::ALNUM) {
        let row = row.as_array().unwrap();
        assert_eq!(
            (hex(row[0].as_str().unwrap()), hex(row[1].as_str().unwrap())),
            (*start, *end)
        );
        assert!(unicode::is_word(char::from_u32(*start).unwrap()));
        assert!(unicode::is_word(char::from_u32(*end).unwrap()));
    }
    assert!(unicode::is_word('_'));
    assert!(!unicode::is_word('\u{301}'));
}
#[test]
fn html_entities_and_numeric_rules_are_exact_baseline_fixtures() {
    let vector = decode(include_str!("vectors/html5-entities-v1.json"));
    let rows = vector.get("entities").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), unicode::HTML5.len());
    for (row, (name, target)) in rows.iter().zip(unicode::HTML5) {
        let row = row.as_array().unwrap();
        assert_eq!(row[0].as_str().unwrap(), *name);
        assert_eq!(scalars(row[1].as_str().unwrap()), *target);
    }
    let rows = vector.get("invalid_charrefs").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), unicode::INVALID_CHARREFS.len());
    for (row, (source, target)) in rows.iter().zip(unicode::INVALID_CHARREFS) {
        let row = row.as_array().unwrap();
        assert_eq!(hex(row[0].as_str().unwrap()), *source);
        assert_eq!(scalars(row[1].as_str().unwrap()), *target);
    }
    let rows = vector
        .get("invalid_codepoints")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|v| hex(v.as_str().unwrap()))
            .collect::<Vec<_>>(),
        unicode::INVALID_CODEPOINTS
    );
    for row in vector.get("samples").unwrap().as_array().unwrap() {
        let row = row.as_array().unwrap();
        assert_eq!(
            unicode::html_unescape(row[0].as_str().unwrap()),
            row[1].as_str().unwrap()
        );
    }
}
