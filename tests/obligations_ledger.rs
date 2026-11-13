#[path = "../src/bin/am_dev/obligations.rs"]
mod obligations;

use agent_memory::codec::json::{self, Value};
use obligations::Annotation;
use std::io::{Read, Write};
use std::path::Path;

const MAX_LEDGER: u64 = 16 * 1024 * 1024;

fn fixture() -> Value {
    json::decode(r#"{"schema":"agent-memory/port-obligations-v1","status_values":["unported","partial","ported","implementation-only","blocker"],"methods":[{"id":"a","status":"unported","registered":true},{"id":"b","status":"ported","registered":false}],"counts":{"static_methods":2,"generated_selectors":0,"registered":1,"unregistered":1,"by_status":{"unported":1,"ported":1}},"other":{}}"#, Default::default()).unwrap()
}

fn ann(id: &str, attached: bool) -> Annotation {
    Annotation {
        file: "tests/example.rs".into(),
        line: 4,
        id: id.into(),
        attached,
    }
}

fn has(findings: &[String], text: &str) -> bool {
    findings.iter().any(|finding| finding.contains(text))
}

#[test]
fn live_ledger_is_consistent() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bytes = Vec::new();
    std::fs::File::open(manifest.join("tests/obligations/python-baseline-v1.json"))
        .unwrap()
        .take(MAX_LEDGER + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(
        bytes.len() as u64 <= MAX_LEDGER,
        "ledger exceeds byte limit"
    );
    let ledger = obligations::parse_ledger(&bytes).unwrap();
    let annotations = obligations::load_annotations(manifest).unwrap();
    let findings = obligations::check(&ledger, &annotations);
    if !findings.is_empty() {
        writeln!(std::io::stderr().lock(), "{}", findings.join("\n")).unwrap();
        assert_eq!(
            std::env::var("AM_LEDGER_REPORT_ONLY").as_deref(),
            Ok("1"),
            "obligation ledger has {} findings",
            findings.len()
        );
    }
}

#[test]
fn synthetic_rules_report_each_offender() {
    let ledger = fixture();
    assert!(obligations::check(&ledger, &[ann("b", true)]).is_empty());
    let findings = obligations::check(&ledger, &[ann("missing", true), ann("a", true)]);
    assert!(
        has(&findings, "missing: unknown annotation id"),
        "{findings:?}"
    );
    assert!(
        has(&findings, "a: annotated id is unported"),
        "{findings:?}"
    );
    assert!(
        has(&findings, "b: ported entry has no annotation"),
        "{findings:?}"
    );
    let findings = obligations::check(&ledger, &[ann("b", false)]);
    assert!(
        has(&findings, "b: annotation not directly followed"),
        "{findings:?}"
    );
    let mut altered = fixture();
    *altered
        .get_mut("counts")
        .unwrap()
        .get_mut("by_status")
        .unwrap()
        .get_mut("ported")
        .unwrap() = Value::Int(3.into());
    assert!(has(
        &obligations::check(&altered, &[ann("b", true)]),
        "counts.by_status.ported disagrees"
    ));
    let mut altered = fixture();
    let Value::Array(methods) = altered.get_mut("methods").unwrap() else {
        panic!("fixture")
    };
    *methods[0].get_mut("status").unwrap() = Value::String("invented".into());
    assert!(has(
        &obligations::check(&altered, &[ann("b", true)]),
        "a: invalid status invented"
    ));
    let mut altered = fixture();
    let Value::Array(methods) = altered.get_mut("methods").unwrap() else {
        panic!("fixture")
    };
    methods.push(methods[0].clone());
    assert!(has(
        &obligations::check(&altered, &[ann("b", true)]),
        "a: duplicate ledger id"
    ));
}

#[test]
fn annotation_adjacency_allows_comments_blanks_and_attributes() {
    let source = "// obligation: a\n// obligation: b\n#[test]\n#[cfg(\n  all(unix)\n)]\nfn example() {}\n// obligation: c\n// explanatory\n\n#[test]\nfn other() {}\n// obligation: d\nlet x = 1;\nfn wrong() {}\n// obligation: e\n";
    let annotations = obligations::annotations("test.rs", source);
    assert_eq!(
        annotations
            .iter()
            .map(|a| (a.id.as_str(), a.attached))
            .collect::<Vec<_>>(),
        vec![
            ("a", true),
            ("b", true),
            ("c", true),
            ("d", false),
            ("e", false)
        ]
    );
}

#[test]
fn baseline_encoding_round_trips_exactly() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bytes = Vec::new();
    std::fs::File::open(manifest.join("tests/obligations/python-baseline-v1.json"))
        .unwrap()
        .take(MAX_LEDGER + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() as u64 <= MAX_LEDGER);
    assert_eq!(
        obligations::encode_ledger(&obligations::parse_ledger(&bytes).unwrap())
            .unwrap()
            .as_bytes(),
        bytes
    );
    let _run: fn(&[String], &Path) -> i32 = obligations::run;
}

#[test]
fn mark_updates_counts_and_refuses_unknown_ids_without_writing() {
    use std::os::unix::fs::DirBuilderExt;
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "am-ledger-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::create_dir_all(root.join("tests/obligations")).unwrap();
    let ledger_path = root.join("tests/obligations/python-baseline-v1.json");
    std::fs::write(
        &ledger_path,
        obligations::encode_ledger(&fixture()).unwrap(),
    )
    .unwrap();
    let read_fixture = || {
        let mut bytes = Vec::new();
        std::fs::File::open(&ledger_path)
            .unwrap()
            .take(MAX_LEDGER + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.len() as u64 <= MAX_LEDGER);
        bytes
    };
    let ids_path = root.join("ids");
    std::fs::write(&ids_path, "b\n").unwrap();
    let args = vec![
        "mark".into(),
        "--status".into(),
        "partial".into(),
        "--ids-file".into(),
        ids_path.display().to_string(),
    ];
    assert_eq!(obligations::run(&args, &root), 0);
    let updated = read_fixture();
    let ledger = obligations::parse_ledger(&updated).unwrap();
    assert!(obligations::check(&ledger, &[ann("b", true)]).is_empty());
    assert_eq!(obligations::run(&args, &root), 0);
    assert_eq!(read_fixture(), updated);
    std::fs::write(&ids_path, "a\nunknown\n").unwrap();
    assert_ne!(obligations::run(&args, &root), 0);
    assert_eq!(read_fixture(), updated);
}
