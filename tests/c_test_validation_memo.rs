mod support;
use agent_memory::codec::json::Value;
use agent_memory::{bank_contract as bank, validation_memo as memo};
use support::cutover::*;

#[test]
// obligation: tests/contracts/test_validation_memo.py::ValidationMemoContract.test_parsed_record_output_is_produced_even_after_a_format_hit
fn parsed_record_output_survives_format_hits_without_caller_aliasing() {
    agent_memory::cli::register_providers().unwrap();
    let files = render(
        &[(
            "projectbrief.md",
            "# Project brief\n\nGoals and boundaries: [project source](../README.md).\n",
        )],
        "2026-10-22T17:00:00Z",
    );
    let path = "topics/native-record.md";
    let files = optional(
        &files,
        &[(
            path.into(),
            "# Native record\n\nKeep distinct source evidence.\n".into(),
        )],
    );
    memo::operation(|_| {
        bank::validate_files(&files, None).unwrap();
        let mut parsed = bank::ParsedRecords::new();
        let mut first = bank::validate_files(&files, Some(&mut parsed)).unwrap();
        assert!(parsed.contains_key(path));
        assert!(parsed[path].2.contains("Keep distinct source evidence."));
        let record = parsed.get_mut(path).unwrap();
        put(&mut record.1, "caller-only", text("untrusted"));
        record.2 = "caller replacement".into();
        *first.get_mut("entries").unwrap() = Value::Array(vec![]);
        let mut again = bank::ParsedRecords::new();
        let second = bank::validate_files(&files, Some(&mut again)).unwrap();
        assert!(!get(&second, "entries").as_array().unwrap().is_empty());
        assert!(again[path].1.get("caller-only").is_none());
        assert!(again[path].2.contains("Keep distinct source evidence."));
        let mut invalid = bank::ParsedRecords::from([(path.to_owned(), again[path].clone())]);
        assert!(bank::validate_files(&files, Some(&mut invalid)).is_err());
        // Rust's typed ParsedRecords excludes Python's list/string output types.
        let mut changed = files.clone();
        put(
            &mut changed,
            path,
            text(string(&files, path).replace(
                "Keep distinct source evidence.",
                "Keep changed source evidence.",
            )),
        );
        let mut updated = bank::ParsedRecords::new();
        bank::validate_files(&changed, Some(&mut updated)).unwrap();
        assert!(updated[path].2.contains("Keep changed source evidence."));
        assert!(again[path].2.contains("Keep distinct source evidence."));
    });
    assert!(memo::current().is_none());
}
