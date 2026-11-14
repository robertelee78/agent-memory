//! Source-preserving layout recovery and exact-file retention contracts.
//! Full public journeys stay ignored until the real recovery_frontier provider
//! exists; the value, byte-boundary and exact private-file contracts run now.
// Shared process fixture also serves admission tests with extra entry points.
#[allow(dead_code)]
#[path = "support/admission.rs"]
mod admission_world;
use admission_world::World;
use agent_memory::codec::json::Value;
use agent_memory::developer_identity::encoded;
use agent_memory::{host, layout_recovery as r, layout_recovery_retention as retention};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}
fn n(v: usize) -> Value {
    Value::Int(v.into())
}
fn obj<const N: usize>(v: [(&str, Value); N]) -> Value {
    Value::Object(v.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn g<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap()
}
fn t<'a>(v: &'a Value, k: &str) -> &'a str {
    g(v, k).as_str().unwrap()
}
fn set(v: &mut Value, k: &str, value: Value) {
    *v.get_mut(k).unwrap() = value;
}
fn sha(v: &Value) -> String {
    sha256_hex(&encoded(v).unwrap())
}
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn uuid() -> String {
    host::uuid4().unwrap()
}
trait ArrayMut {
    fn as_array_mut(&mut self) -> Option<&mut Vec<Value>>;
}
impl ArrayMut for Value {
    fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        if let Value::Array(rows) = self {
            Some(rows)
        } else {
            None
        }
    }
}
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let root = fs::canonicalize("/tmp")
            .unwrap()
            .join(format!("agent-memory-test-layout-{}", uuid()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        Self(root)
    }
    fn join(&self, p: &str) -> PathBuf {
        self.0.join(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn private(path: &Path, body: &[u8]) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn retention_fixture() -> (Root, PathBuf, PathBuf, PathBuf) {
    let root = Root::new();
    let directory = root.join("operation");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(directory.join("originals"))
        .unwrap();
    let source = directory.join("originals").join(format!("{}.bin", uuid()));
    private(&source, b"Original exact bytes and conditions.\n");
    let receipt = directory.join("receipt.json");
    private(&receipt, b"{\n  \"outcome\": \"layout-restored\"\n}\n");
    (root, directory, source, receipt)
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_unknown_backup_even_empty_directory_is_not_disposable
#[test]
fn unknown_backup_even_empty_directory_is_not_disposable() {
    for kind in ["file", "directory", "incomplete-staging"] {
        let (_root, directory, source, receipt) = retention_fixture();
        let unknown = directory.join(if kind == "incomplete-staging" {
            "staging.json"
        } else {
            "unknown-backup"
        });
        if kind == "directory" {
            fs::DirBuilder::new().mode(0o700).create(&unknown).unwrap();
        } else {
            private(
                &unknown,
                if kind == "file" {
                    b"Unowned evidence\n"
                } else {
                    b"{\"observation_complete\":false}"
                },
            );
        }
        let before = (fs::read(&source).unwrap(), fs::read(&receipt).unwrap());
        assert!(retention::inventory(&directory).is_err());
        assert_eq!(
            before,
            (fs::read(&source).unwrap(), fs::read(&receipt).unwrap())
        );
        assert!(unknown.exists());
    }
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_resolution_and_quiescence_metadata_remain_retained
#[test]
fn resolution_and_quiescence_metadata_remain_retained() {
    let (_root, directory, _, _) = retention_fixture();
    let names = vec![
        format!("coverage-reviews/{}.json", "a".repeat(64)),
        format!("union-reviews/{}.json", uuid()),
        format!("discard-reviews/{}.json", uuid()),
        format!("quiescence-reviews/{}.json", uuid()),
        format!("attestations/{}.json", uuid()),
        format!("reviews/{}.json", uuid()),
        "ledger-transitions/00000001.json".into(),
        "quiescence-attestations.json".into(),
    ];
    for name in &names {
        let path = directory.join(name);
        if !path.parent().unwrap().exists() {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(path.parent().unwrap())
                .unwrap();
        }
        private(&path, b"{}\n");
    }
    let inventory = retention::inventory(&directory).unwrap();
    for name in names {
        assert!(retention::retained(&name));
        assert!(g(&inventory, "files").get(&name).is_some());
    }
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_owned_incomplete_staging_is_retained_not_promoted_to_complete
#[test]
fn owned_incomplete_staging_is_retained_not_promoted_to_complete() {
    let (_root, directory, _, _) = retention_fixture();
    let staging = obj([
        ("schema", s("agent-memory/layout-recovery-staging-v1")),
        ("operation_id", s(uuid())),
        ("project_id", s(uuid())),
        ("repo_root", s("/controlled/repo")),
        ("bank_path", s("/controlled/repo/memory-bank")),
        ("identity", obj([])),
        ("observation_complete", Value::Bool(false)),
    ]);
    private(&directory.join("staging.json"), &encoded(&staging).unwrap());
    assert!(retention::retained("staging.json"));
    let inventory = retention::inventory(&directory).unwrap();
    assert!(g(&inventory, "files").get("staging.json").is_some());
    assert_eq!(
        fs::read(directory.join("staging.json")).unwrap(),
        encoded(&staging).unwrap()
    );
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_symlink_hardlink_and_unsafe_mode_never_gain_cleanup_ownership
#[test]
fn symlink_hardlink_and_unsafe_mode_never_gain_cleanup_ownership() {
    for kind in ["symlink", "hardlink", "mode"] {
        let (_root, directory, source, receipt) = retention_fixture();
        let alias = source.parent().unwrap().join(format!("{}.bin", uuid()));
        match kind {
            "symlink" => symlink(&source, &alias).unwrap(),
            "hardlink" => fs::hard_link(&source, &alias).unwrap(),
            _ => fs::set_permissions(&source, fs::Permissions::from_mode(0o644)).unwrap(),
        }
        assert!(retention::inventory(&directory).is_err());
        assert!(source.exists());
        assert!(receipt.exists());
    }
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_individual_unlink_keeps_receipt_and_directories_and_checks_changed_bytes
#[test]
fn individual_unlink_keeps_receipt_and_directories_and_checks_changed_bytes() {
    let (_root, directory, source, receipt) = retention_fixture();
    let inventory = retention::inventory(&directory).unwrap();
    let relative = source.strip_prefix(&directory).unwrap().to_str().unwrap();
    private(&source, b"External replacement must survive.\n");
    assert!(
        retention::remove(
            &directory,
            relative,
            g(g(&inventory, "files"), relative),
            g(&inventory, "directories")
        )
        .is_err()
    );
    assert_eq!(
        fs::read(&source).unwrap(),
        b"External replacement must survive.\n"
    );
    let current = retention::inventory(&directory).unwrap();
    retention::remove(
        &directory,
        relative,
        g(g(&current, "files"), relative),
        g(&current, "directories"),
    )
    .unwrap();
    assert!(!source.exists());
    assert!(receipt.exists());
    assert!(source.parent().unwrap().is_dir());
    assert!(directory.is_dir());
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_resume_accepts_only_the_one_unlink_before_progress_boundary
#[test]
fn resume_accepts_only_the_one_unlink_before_progress_boundary() {
    let (_root, directory, source, receipt) = retention_fixture();
    let second = source.parent().unwrap().join(format!("{}.bin", uuid()));
    private(&second, b"Second independently pinned original\n");
    let inventory = retention::inventory(&directory).unwrap();
    let mut delete = g(&inventory, "files")
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| k.starts_with("originals/"))
        .cloned()
        .collect::<Vec<_>>();
    delete.sort_by(|(a, _), (b, _)| a.cmp(b));
    let ordered: Vec<_> = delete.iter().map(|(k, _)| k.clone()).collect();
    let proposal = obj([
        ("delete", Value::Object(delete)),
        (
            "retain",
            obj([(
                "receipt.json",
                g(g(&inventory, "files"), "receipt.json").clone(),
            )]),
        ),
    ]);
    fs::remove_file(directory.join(&ordered[0])).unwrap();
    retention::remaining(&directory, &proposal, &[]).unwrap();
    fs::remove_file(directory.join(&ordered[1])).unwrap();
    assert!(retention::remaining(&directory, &proposal, &[]).is_err());
    retention::remaining(&directory, &proposal, &ordered).unwrap();
    assert!(receipt.exists());
}
#[test]
fn safe_envelope_never_exports_private_sources_or_proof_digests() {
    let value = r::result(
        "apply",
        Some("11111111-1111-4111-8111-111111111111"),
        [
            ("manifest", s("secret")),
            ("proposal", s("secret")),
            (
                "receipt",
                obj([
                    ("outcome", s("layout-restored")),
                    ("request_sha256", s("private-proof")),
                    ("resulting_view", s("secret")),
                ]),
            ),
            (
                "recall",
                obj([("state", s("evaluated")), ("result", s("secret"))]),
            ),
        ],
    );
    let safe = r::safe_envelope(&value, None);
    let output = encoded(&safe).unwrap();
    assert!(!String::from_utf8(output).unwrap().contains("secret"));
    assert!(g(&safe, "receipt").get("request_sha256").is_none());
    assert_eq!(g(g(&safe, "recall"), "state"), &s("evaluated"));
}
fn value_fixture() -> (Value, Value, Value) {
    let source = "11111111-1111-4111-8111-111111111111";
    let manifest = obj([
        ("operation_id", s("22222222-2222-4222-8222-222222222222")),
        (
            "sources",
            Value::Array(vec![obj([
                ("source_id", s(source)),
                ("path", s("activeContext.md")),
                ("byte_count", n(4)),
            ])]),
        ),
    ]);
    let intended = obj([("activeContext.md", s("new\n"))]);
    let unit = obj([
        ("unit_id", s("33333333-3333-4333-8333-333333333333")),
        (
            "source_ranges",
            Value::Array(vec![obj([
                ("source_id", s(source)),
                ("start", n(0)),
                ("end", n(4)),
            ])]),
        ),
        ("semantic_kind", s("canonical-guidance")),
        ("existing_record", Value::Null),
        ("provenance", obj([])),
        ("disposition", s("preserve")),
        (
            "targets",
            Value::Array(vec![obj([
                ("path", s("activeContext.md")),
                (
                    "sha256",
                    s("7aa7a5359173d05b63cfd682e3c38487f3cb4f7f1d60659fe59fab1505977d4c"),
                ),
            ])]),
        ),
        ("reason", s("Preserve original conditions")),
        ("required_actions", Value::Array(vec![])),
        ("context_pages", Value::Array(vec![n(0)])),
    ]);
    let map = obj([
        ("schema", s("agent-memory/layout-unit-map-v1")),
        ("operation_id", g(&manifest, "operation_id").clone()),
        ("manifest_sha256", s(sha(&manifest))),
        ("units", Value::Array(vec![unit])),
    ]);
    (manifest, intended, map)
}
#[test]
fn unit_coverage_requires_exact_ordered_bytes_context_and_targets() {
    let (manifest, intended, map) = value_fixture();
    r::unit_map_value(&map, &manifest, &intended, None).unwrap();
    for change in ["gap", "overlap", "context", "target", "demote", "missing"] {
        let mut broken = map.clone();
        let units = broken.get_mut("units").unwrap().as_array_mut().unwrap();
        match change {
            "gap" => set(
                units[0]
                    .get_mut("source_ranges")
                    .unwrap()
                    .as_array_mut()
                    .unwrap()
                    .first_mut()
                    .unwrap(),
                "end",
                n(3),
            ),
            "overlap" => {
                let mut repeated = units[0].clone();
                set(&mut repeated, "unit_id", s(uuid()));
                units.push(repeated);
            }
            "context" => set(&mut units[0], "context_pages", Value::Array(vec![n(1)])),
            "target" => set(
                units[0]
                    .get_mut("targets")
                    .unwrap()
                    .as_array_mut()
                    .unwrap()
                    .first_mut()
                    .unwrap(),
                "sha256",
                s("0".repeat(64)),
            ),
            "demote" => {
                set(&mut units[0], "disposition", s("pending-admission"));
                set(&mut units[0], "semantic_kind", s("unadmitted-source"));
            }
            _ => units.clear(),
        }
        assert!(
            r::unit_map_value(&broken, &manifest, &intended, None).is_err(),
            "{change}"
        );
    }
}
#[test]
fn review_independence_uses_real_session_not_developer_uuid() {
    let author = obj([
        ("developer_id", s("11111111-1111-4111-8111-111111111111")),
        ("harness", s("fixture")),
        ("session", s("author")),
        ("evidence", s("Actual source-bound observation")),
    ]);
    let mut reviewer = author.clone();
    assert!(r::independent(&author, &reviewer).is_err());
    set(&mut reviewer, "session", s("reviewer"));
    r::independent(&author, &reviewer).unwrap();
    set(&mut reviewer, "developer_id", s("not-a-developer"));
    assert!(r::independent(&author, &reviewer).is_err());
}
#[test]
fn supplementary_groups_are_available_and_bounded() {
    assert!(host::groups().unwrap().len() <= 65_536);
}

// Real public journeys. No stand-in recovery frontier is installed.
fn isolated(name: &str, body: impl FnOnce(&World)) {
    isolated_variant(name, "", body);
}
fn isolated_variant(name: &str, variant: &str, body: impl FnOnce(&World)) {
    if std::env::var("AM_LAYOUT_CASE").ok().as_deref() == Some(name) {
        if std::env::var("AM_LAYOUT_VARIANT").unwrap_or_default() != variant {
            return;
        }
        agent_memory::cli::register_providers().unwrap();
        if let Ok(operation) = std::env::var("AM_LAYOUT_ACTION") {
            let path = |key| std::env::var_os(key).map(PathBuf::from);
            let repo = path("AM_LAYOUT_REPO").unwrap();
            let runtime = path("AM_LAYOUT_RUNTIME").unwrap();
            let proposal = path("AM_LAYOUT_PROPOSAL");
            let input = path("AM_LAYOUT_INPUT");
            let output = path("AM_LAYOUT_OUTPUT");
            let id = std::env::var("AM_LAYOUT_OPERATION").ok();
            let stage = std::env::var("AM_LAYOUT_STAGE").ok();
            let result = r::command(
                r::Command {
                    operation: &operation,
                    stage: stage.as_deref(),
                    repo: &repo,
                    operation_id: id.as_deref(),
                    input: input.as_deref(),
                    proposal: proposal.as_deref(),
                    output: output.as_deref(),
                    page: None,
                    identity_review: None,
                    mode: None,
                    nested: None,
                    unit: None,
                },
                &runtime,
            );
            print!("{}", result.stdout);
            eprint!("{}", result.stderr);
            std::process::exit(result.exit_code);
        }
        body(&World {
            root: PathBuf::from(std::env::var_os("AM_LAYOUT_ROOT").unwrap()),
        });
        return;
    }
    let root = Root::new();
    fs::create_dir(root.join("home")).unwrap();
    private(
        &root.join(".test-world.json"),
        b"{\"token\":\"layout-contract\"}",
    );
    let world = World {
        root: root.0.clone(),
    };
    let output = world.command(
        &[
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            name.into(),
            "--include-ignored".into(),
            "--nocapture".into(),
        ],
        &[
            ("AM_LAYOUT_CASE", name.into()),
            ("AM_LAYOUT_ROOT", root.0.as_os_str().to_owned()),
            ("AM_LAYOUT_VARIANT", variant.into()),
        ],
    );
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &target.join(entry.file_name()));
        } else if path.is_file() {
            fs::copy(path, target.join(entry.file_name())).unwrap();
        }
    }
}
struct Fixture {
    repo: PathBuf,
    bank: PathBuf,
    runtime: PathBuf,
    native: Value,
    originals: BTreeMap<String, Vec<u8>>,
    dev: String,
}
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, path: &Path, rows: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, rows);
            } else if !path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(".agent-memory")
            {
                rows.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut rows = BTreeMap::new();
    visit(root, root, &mut rows);
    rows
}
fn fixture(w: &World, pending: bool) -> Fixture {
    let (repo, bank, dev) = w.fixture();
    let sentence = "Under the synthetic review condition, preserve existing guidance and check the original source before acting.\n";
    let concise = format!("# Active context\n\n{sentence}");
    let mut proposal = agent_memory::developer_identity::load(include_bytes!(
        "../skills/agent-memory/templates/router-proposal.example.json"
    ))
    .unwrap();
    set(
        &mut proposal,
        "project-id",
        s("75b38e17-c36e-4fab-8ba9-5fa51f25de91"),
    );
    if let Value::Object(contexts) = proposal.get_mut("contexts").unwrap() {
        contexts.push((
            "activeContext.md".into(),
            obj([
                ("body", s(concise.as_str())),
                ("purpose", s("Synthetic source-preserving orientation")),
                ("scope", s("repository")),
                ("lifecycle", s("active")),
                (
                    "read-when",
                    s("When reviewing the synthetic recovery condition"),
                ),
            ]),
        ));
    }
    if let Value::Object(omitted) = proposal.get_mut("omitted").unwrap() {
        omitted.retain(|(k, _)| k != "activeContext.md");
    }
    let mut native = g(
        &agent_memory::bank_contract::render(&proposal).unwrap(),
        "files",
    )
    .clone();
    let mut record = agent_memory::developer_identity::load(include_bytes!(
        "../skills/agent-memory/templates/record-proposal.example.json"
    ))
    .unwrap();
    let metadata = record.get_mut("metadata").unwrap();
    set(metadata, "id", s(uuid()));
    set(metadata, "status", s("active"));
    set(metadata, "scope", s("repository"));
    set(metadata, "conditions", obj([]));
    set(metadata.get_mut("origin").unwrap(), "developer-id", s(&dev));
    set(
        metadata.get_mut("origin").unwrap(),
        "code-state",
        s("not-code"),
    );
    if let Value::Object(pairs) = metadata {
        pairs.extend([
            ("trust".into(), s("reviewed")),
            ("visibility".into(), s("visible")),
            (
                "admission-reason".into(),
                s("Controlled existing guidance fixture"),
            ),
        ]);
    }
    let record_path = format!("topics/{}.md", t(metadata, "id"));
    let body = agent_memory::codec::frontmatter::write(metadata.as_object().unwrap()).unwrap()
        + "# Existing typed guidance\n\nRetain source conditions and actual provenance.\n";
    let (header, mut routes, omitted) =
        agent_memory::bank_contract::parse_router(t(&native, "MEMORY.md")).unwrap();
    routes.push(obj([
        ("path", s(&record_path)),
        ("purpose", s("Existing typed guidance")),
        ("scope", s("repository")),
        ("lifecycle", s("active")),
        ("read-when", s("When checking source preservation")),
    ]));
    set(
        &mut native,
        "MEMORY.md",
        s(agent_memory::bank_contract::router_text(&header, &routes, &omitted).unwrap()),
    );
    if let Value::Object(pairs) = &mut native {
        pairs.push((record_path, s(body)));
    }
    for (p, b) in native.as_object().unwrap() {
        fs::create_dir_all(bank.join(p).parent().unwrap()).unwrap();
        fs::write(bank.join(p), b.as_str().unwrap()).unwrap();
    }
    let mut large = "# Active context\n\n".to_owned();
    while large.len() + sentence.len() <= 79_054 {
        large.push_str(sentence);
    }
    large.push_str(&" ".repeat(79_054 - large.len()));
    fs::write(bank.join("activeContext.md"), large).unwrap();
    if pending {
        let sentence = "Under the controlled pending-source condition, confirm independent repository evidence before ordinary admission.\n";
        let mut body = "# Synthetic pending source\n\n".to_owned();
        while body.len() + sentence.len() <= 28_478 {
            body.push_str(sentence);
        }
        body.push_str(&" ".repeat(28_478 - body.len()));
        fs::write(bank.join("handoff-prompt.md"), body).unwrap();
    }
    let originals = snapshot(&bank);
    let runtime = w.root.join("runtime");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        &runtime.join("skills/agent-memory"),
    );
    fs::create_dir_all(runtime.join("bin")).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_am"), runtime.join("bin/am")).unwrap();
    Fixture {
        repo,
        bank,
        runtime,
        native,
        originals,
        dev,
    }
}
fn request(f: &Fixture, observed: &Value) -> Value {
    let manifest = g(observed, "manifest");
    let id = t(manifest, "operation_id");
    let author = obj([
        ("developer_id", s(&f.dev)),
        ("harness", s("controlled-author")),
        ("session", s(format!("author-{id}"))),
        (
            "evidence",
            s("Disposable test observation, not product acceptance"),
        ),
    ]);
    let mut reviewer = author.clone();
    set(&mut reviewer, "session", s(format!("reviewer-{id}")));
    let mut units = vec![];
    let mut page = 0;
    for source in g(manifest, "sources").as_array().unwrap() {
        let count = g(source, "byte_count").as_int().unwrap().as_i128().unwrap() as usize;
        let pages = count.div_ceil(24576).max(1);
        let path = t(source, "path");
        let pending = f.native.get(path).is_none();
        let lengths = if path == "handoff-prompt.md" && count == 28_478 {
            vec![3560, 3560, 3560, 3560, 3560, 3560, 3560, 3558]
        } else {
            vec![count]
        };
        let mut offset = 0;
        for length in lengths {
            units.push(obj([
                ("unit_id", s(uuid())),
                (
                    "source_ranges",
                    Value::Array(vec![obj([
                        ("source_id", g(source, "source_id").clone()),
                        ("start", n(offset)),
                        ("end", n(offset + length)),
                    ])]),
                ),
                (
                    "semantic_kind",
                    s(if pending {
                        "unadmitted-source"
                    } else {
                        "canonical-guidance"
                    }),
                ),
                (
                    "existing_record",
                    f.native
                        .get(path)
                        .and_then(Value::as_str)
                        .and_then(|body| agent_memory::records::parse(body, &[]).ok())
                        .filter(|(kind, _, _)| {
                            ["standalone-record", "optional-history-event", "tombstone"]
                                .contains(&kind.as_str())
                        })
                        .map_or(Value::Null, |(_, metadata, _)| {
                            obj([
                                ("id", g(&metadata, "id").clone()),
                                (
                                    "version",
                                    metadata.get("version").cloned().unwrap_or(Value::Null),
                                ),
                            ])
                        }),
                ),
                ("provenance", obj([("origin", s("Controlled fixture"))])),
                (
                    "disposition",
                    s(if pending {
                        "pending-admission"
                    } else {
                        "preserve"
                    }),
                ),
                (
                    "targets",
                    Value::Array(if pending {
                        vec![]
                    } else {
                        vec![obj([
                            ("path", s(path)),
                            ("sha256", s(sha256_hex(t(&f.native, path).as_bytes()))),
                        ])]
                    }),
                ),
                (
                    "reason",
                    s("Preserve complete synthetic meaning and conditions"),
                ),
                (
                    "required_actions",
                    Value::Array(if pending {
                        vec![s("Confirm independent evidence before ordinary admission")]
                    } else {
                        vec![]
                    }),
                ),
                (
                    "context_pages",
                    Value::Array((page..page + pages).map(n).collect()),
                ),
            ]));
            offset += length;
        }
        page += pages;
    }
    let map = obj([
        ("schema", s("agent-memory/layout-unit-map-v1")),
        ("operation_id", s(id)),
        ("manifest_sha256", s(sha(manifest))),
        ("units", Value::Array(units.clone())),
    ]);
    let metadata = obj([
        (
            "files",
            Value::Object(
                f.native
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(p, _)| {
                        let row = g(manifest, "physical_entries")
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|r| t(r, "path") == p)
                            .unwrap();
                        (
                            p.clone(),
                            obj([
                                ("uid", g(row, "uid").clone()),
                                ("gid", g(row, "gid").clone()),
                                (
                                    "mode",
                                    Value::Int(
                                        (g(row, "mode").as_int().unwrap().as_i128().unwrap()
                                            & 0o777)
                                            .into(),
                                    ),
                                ),
                                ("xattrs", g(row, "xattrs").clone()),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
        ("directories", obj([])),
    ]);
    let review_id = uuid();
    let pending = units
        .iter()
        .any(|u| t(u, "disposition") == "pending-admission");
    let attestation = if pending {
        let runtime = agent_memory::recovery_quiescence::measured_runtime(&f.runtime).unwrap();
        obj([
            ("schema", s("agent-memory/layout-quiescence-attestation-v1")),
            ("attestation_id", s(uuid())),
            ("operation_id", s(id)),
            ("project_id", g(manifest, "project_id").clone()),
            ("repo_root", g(manifest, "repo_root").clone()),
            ("bank_path", g(manifest, "bank_path").clone()),
            (
                "checkout_key",
                g(g(g(manifest, "identity"), "scope_context"), "checkout_key").clone(),
            ),
            (
                "state_root",
                s(std::env::var("AGENT_MEMORY_STATE_HOME").unwrap()),
            ),
            ("manifest_sha256", s(sha(manifest))),
            (
                "runtime_artifact_digest",
                g(&runtime, "artifact_digest").clone(),
            ),
            ("operator_developer_id", s(&f.dev)),
            ("issued_at", g(manifest, "inspected_at").clone()),
            ("declared_principals", Value::Array(vec![s(&f.dev)])),
            (
                "declared_invocation_routes",
                Value::Array(vec![s(f.runtime.join("bin/am").to_str().unwrap())]),
            ),
            ("stopped_sessions", Value::Array(vec![])),
            (
                "statement",
                s(
                    "All authorized legacy invocation routes and in-flight sessions are disabled or quiesced until terminal recovery retirement, or use the independently verified capable runtime and bound private state.",
                ),
            ),
            ("review_id", s(&review_id)),
        ])
    } else {
        Value::Null
    };
    let review = obj([
        ("schema", s("agent-memory/layout-review-v1")),
        ("review_id", s(review_id)),
        ("operation_id", s(id)),
        ("manifest_sha256", s(sha(manifest))),
        ("unit_map_sha256", s(sha(&map))),
        ("intended_files_sha256", s(sha(&f.native))),
        ("metadata_plan_sha256", s(sha(&metadata))),
        (
            "authority_sha256",
            s(sha(g(g(manifest, "identity"), "project_evidence"))),
        ),
        ("author", author.clone()),
        ("reviewer", reviewer),
        ("reviewed_at", g(manifest, "inspected_at").clone()),
        (
            "unit_decisions",
            Value::Array(
                units
                    .iter()
                    .map(|u| {
                        obj([
                            ("unit_id", g(u, "unit_id").clone()),
                            ("disposition", g(u, "disposition").clone()),
                            ("rationale", g(u, "reason").clone()),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("inherited_frontier_sha256", s(sha(&Value::Array(vec![])))),
        ("inherited_unit_decisions", Value::Array(vec![])),
        (
            "quiescence_attestation_sha256",
            if pending {
                s(sha(&attestation))
            } else {
                Value::Null
            },
        ),
        ("semantic_preservation", Value::Bool(true)),
        ("security", Value::Bool(true)),
        ("identity", Value::Bool(true)),
        ("decision", s("approve")),
        (
            "evidence",
            s("Controlled full-source/result review, not release acceptance"),
        ),
    ]);
    obj([
        ("schema", s("agent-memory/layout-prepare-v1")),
        ("operation_id", s(id)),
        ("author", author),
        ("unit_map", map),
        ("intended_files", f.native.clone()),
        ("metadata_plan", metadata),
        ("review", review),
        ("quiescence_attestation", attestation),
        ("retained_source_ids", Value::Array(vec![])),
    ])
}
fn prepared(f: &Fixture) -> (Value, Value) {
    let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
    let request = request(f, &observed);
    let proposal = r::prepare(&f.repo, t(&observed, "operation_id"), &request, &f.runtime).unwrap();
    (observed, proposal)
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_all_preserve_apply_keeps_metadata_feature_head_and_exact_original_index
#[test]
#[ignore = "needs recovery_frontier registered"]
fn all_preserve_apply_keeps_metadata_feature_head_and_exact_original_index() {
    isolated(
        "all_preserve_apply_keeps_metadata_feature_head_and_exact_original_index",
        |w| {
            let f = fixture(w, false);
            fs::write(f.repo.join("feature.txt"), "Staged feature\n").unwrap();
            w.git(&f.repo, &["add", "feature.txt"]);
            fs::write(f.repo.join("feature.txt"), "Unstaged feature\n").unwrap();
            fs::write(f.repo.join("untracked.txt"), "Untracked feature\n").unwrap();
            let before = (
                fs::read(f.repo.join(".git/index")).unwrap(),
                w.git(&f.repo, &["rev-parse", "HEAD"]),
            );
            let (observed, proposal) = prepared(&f);
            let applied = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
            assert_eq!(g(&applied, "canonical_eligibility"), &Value::Bool(true));
            assert_eq!(
                g(&applied, "semantic_recovery_complete"),
                &Value::Bool(true)
            );
            assert_eq!(
                before,
                (
                    fs::read(f.repo.join(".git/index")).unwrap(),
                    w.git(&f.repo, &["rev-parse", "HEAD"])
                )
            );
            assert_eq!(
                fs::read(f.repo.join("feature.txt")).unwrap(),
                b"Unstaged feature\n"
            );
            assert_eq!(
                fs::read(f.repo.join("untracked.txt")).unwrap(),
                b"Untracked feature\n"
            );
            assert_eq!(
                snapshot(&f.bank),
                f.native
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(p, b)| (p.clone(), b.as_str().unwrap().as_bytes().to_vec()))
                    .collect()
            );
            let op = r::operation(&f.repo, t(&observed, "operation_id"), &f.runtime).unwrap();
            for source in g(&op.manifest, "sources").as_array().unwrap() {
                assert_eq!(
                    fs::read(op.directory.join(t(source, "blob"))).unwrap(),
                    f.originals[t(source, "path")]
                );
            }
            assert!(!f.bank.join(r::MARKER).exists());
            assert!(!f.bank.join(r::SENTINEL).exists());
        },
    );
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_exact_retry_is_historical_and_never_overwrites_later_context
#[test]
#[ignore = "needs recovery_frontier registered"]
fn exact_retry_is_historical_and_never_overwrites_later_context() {
    isolated(
        "exact_retry_is_historical_and_never_overwrites_later_context",
        |w| {
            let f = fixture(w, false);
            let (observed, proposal) = prepared(&f);
            r::apply(&f.repo, &proposal, &f.runtime).unwrap();
            let changed = format!(
                "{}\nLater independently retained context.\n",
                t(&f.native, "activeContext.md")
            );
            fs::write(f.bank.join("activeContext.md"), &changed).unwrap();
            let retry = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
            for name in ["already_applied", "receipt_is_historical"] {
                assert_eq!(g(&retry, name), &Value::Bool(true));
            }
            for name in ["current_receipt_match", "semantic_recovery_complete"] {
                assert_eq!(g(&retry, name), &Value::Bool(false));
            }
            assert!(
                r::rollback(
                    &f.repo,
                    Some(t(&observed, "operation_id")),
                    None,
                    &f.runtime
                )
                .is_err()
            );
            assert_eq!(
                fs::read_to_string(f.bank.join("activeContext.md")).unwrap(),
                changed
            );
        },
    );
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_prepare_requires_full_coverage_independent_review_real_identity_and_safe_output
#[test]
#[ignore = "needs recovery_frontier registered"]
fn prepare_requires_full_coverage_independent_review_real_identity_and_safe_output() {
    isolated(
        "prepare_requires_full_coverage_independent_review_real_identity_and_safe_output",
        |w| {
            let f = fixture(w, true);
            let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
            let good = request(&f, &observed);
            for kind in [
                "reviewer",
                "coverage",
                "target",
                "identity",
                "attestation",
                "extra",
                "inherited",
            ] {
                let mut broken = good.clone();
                match kind {
                    "reviewer" => {
                        let author = g(&broken, "author").clone();
                        set(broken.get_mut("review").unwrap(), "reviewer", author);
                    }
                    "coverage" => {
                        broken
                            .get_mut("unit_map")
                            .unwrap()
                            .get_mut("units")
                            .unwrap()
                            .as_array_mut()
                            .unwrap()
                            .pop();
                    }
                    "target" => {
                        let u = broken
                            .get_mut("unit_map")
                            .unwrap()
                            .get_mut("units")
                            .unwrap()
                            .as_array_mut()
                            .unwrap()
                            .first_mut()
                            .unwrap();
                        set(
                            u.get_mut("targets")
                                .unwrap()
                                .as_array_mut()
                                .unwrap()
                                .first_mut()
                                .unwrap(),
                            "sha256",
                            s("0".repeat(64)),
                        );
                    }
                    "identity" => set(broken.get_mut("author").unwrap(), "developer_id", s(uuid())),
                    "attestation" => set(&mut broken, "quiescence_attestation", Value::Null),
                    "inherited" => {
                        if let Value::Object(p) = broken.get_mut("review").unwrap() {
                            p.retain(|(k, _)| k != "inherited_unit_decisions");
                        }
                    }
                    _ => {
                        if let Value::Object(p) = &mut broken {
                            p.push(("skip_security".into(), Value::Bool(true)));
                        }
                    }
                }
                assert!(
                    r::prepare(&f.repo, t(&observed, "operation_id"), &broken, &f.runtime).is_err(),
                    "{kind}"
                );
                assert_eq!(snapshot(&f.bank), f.originals);
            }
            assert!(
                agent_memory::sharing_limits::preflight_private_output(
                    &f.repo.join("unsafe.json"),
                    &f.repo,
                    &f.runtime
                )
                .is_err()
            );
        },
    );
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_changed_source_or_metadata_invalidates_apply_before_any_canonical_replacement
#[test]
#[ignore = "needs recovery_frontier registered"]
fn changed_source_or_metadata_invalidates_apply_before_any_canonical_replacement() {
    isolated(
        "changed_source_or_metadata_invalidates_apply_before_any_canonical_replacement",
        |w| {
            let f = fixture(w, false);
            let original_mode = fs::metadata(f.bank.join("activeContext.md"))
                .unwrap()
                .permissions()
                .mode();
            let index = fs::read(f.repo.join(".git/index")).unwrap();
            for kind in ["bytes", "mode", "index"] {
                let (_, proposal) = prepared(&f);
                match kind {
                    "bytes" => {
                        fs::write(f.bank.join("activeContext.md"), "Later source\n").unwrap()
                    }
                    "mode" => fs::set_permissions(
                        f.bank.join("activeContext.md"),
                        fs::Permissions::from_mode(0o600),
                    )
                    .unwrap(),
                    _ => {
                        fs::write(f.repo.join("staged.txt"), "Different original index\n").unwrap();
                        w.git(&f.repo, &["add", "staged.txt"]);
                    }
                }
                let before = snapshot(&f.bank);
                assert!(r::apply(&f.repo, &proposal, &f.runtime).is_err());
                assert_eq!(snapshot(&f.bank), before);
                fs::write(
                    f.bank.join("activeContext.md"),
                    &f.originals["activeContext.md"],
                )
                .unwrap();
                fs::set_permissions(
                    f.bank.join("activeContext.md"),
                    fs::Permissions::from_mode(original_mode),
                )
                .unwrap();
                fs::write(f.repo.join(".git/index"), &index).unwrap();
            }
        },
    );
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_completed_restore_has_distinct_child_and_keeps_parent_historical_receipt
#[test]
#[ignore = "needs recovery_frontier registered"]
fn completed_restore_has_distinct_child_and_keeps_parent_historical_receipt() {
    isolated(
        "completed_restore_has_distinct_child_and_keeps_parent_historical_receipt",
        |w| {
            let f = fixture(w, false);
            let (observed, proposal) = prepared(&f);
            r::apply(&f.repo, &proposal, &f.runtime).unwrap();
            let op = r::operation(&f.repo, t(&observed, "operation_id"), &f.runtime).unwrap();
            let before = fs::read(op.directory.join("receipt.json")).unwrap();
            let child = r::rollback(
                &f.repo,
                Some(t(&observed, "operation_id")),
                None,
                &f.runtime,
            )
            .unwrap();
            assert_ne!(g(&child, "operation_id"), g(&observed, "operation_id"));
            r::rollback(&f.repo, None, Some(&child), &f.runtime).unwrap();
            assert_eq!(snapshot(&f.bank), f.originals);
            assert_eq!(fs::read(op.directory.join("receipt.json")).unwrap(), before);
            let retry = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
            assert_eq!(g(&retry, "receipt_is_historical"), &Value::Bool(true));
            assert_eq!(g(&retry, "current_receipt_match"), &Value::Bool(false));
        },
    );
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_public_pending_units_cannot_be_deleted_by_retention
#[test]
#[ignore = "needs recovery_frontier registered"]
fn public_pending_units_cannot_be_deleted_by_retention() {
    isolated("public_pending_units_cannot_be_deleted_by_retention", |w| {
        let f = fixture(w, true);
        let (observed, proposal) = prepared(&f);
        let applied = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
        assert_eq!(
            g(&applied, "semantic_recovery_complete"),
            &Value::Bool(false)
        );
        let before = snapshot(&f.bank);
        assert!(
            retention::inspect(&f.repo, t(&observed, "operation_id"), None, &f.runtime).is_err()
        );
        assert_eq!(snapshot(&f.bank), before);
    });
}

fn fault_action(
    w: &World,
    f: &Fixture,
    operation: &str,
    proposal: Option<&Path>,
    stage: &str,
) -> agent_memory::process::Output {
    let case = std::env::var("AM_LAYOUT_CASE").unwrap();
    let mut extra = vec![
        ("AM_LAYOUT_CASE", case.clone().into()),
        (
            "AM_LAYOUT_VARIANT",
            std::env::var_os("AM_LAYOUT_VARIANT").unwrap_or_default(),
        ),
        ("AM_LAYOUT_ROOT", w.root.as_os_str().to_owned()),
        ("AM_LAYOUT_ACTION", operation.into()),
        ("AM_LAYOUT_REPO", f.repo.as_os_str().to_owned()),
        ("AM_LAYOUT_RUNTIME", f.runtime.as_os_str().to_owned()),
        (
            "AM_LAYOUT_OUTPUT",
            w.root
                .join(format!("fault-{}.json", uuid()))
                .into_os_string(),
        ),
        ("AGENT_MEMORY_TEST_ROOT", w.root.as_os_str().to_owned()),
        ("AGENT_MEMORY_TEST_TOKEN", "layout-contract".into()),
        ("AGENT_MEMORY_FAULT_STAGE", stage.into()),
    ];
    if let Some(proposal) = proposal {
        extra.push(("AM_LAYOUT_PROPOSAL", proposal.as_os_str().to_owned()));
    }
    if operation == "retention" {
        extra.push(("AM_LAYOUT_STAGE", "apply".into()));
    }
    w.command(
        &[
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            case.into(),
            "--include-ignored".into(),
            "--nocapture".into(),
        ],
        &extra,
    )
}

// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_initial_quiescence_requires_exact_independent_review
#[test]
#[ignore = "needs recovery_frontier registered"]
fn initial_quiescence_requires_exact_independent_review() {
    isolated(
        "initial_quiescence_requires_exact_independent_review",
        |w| {
            let f = fixture(w, true);
            let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
            let good = request(&f, &observed);
            r::prepare(&f.repo, t(&observed, "operation_id"), &good, &f.runtime).unwrap();
            for kind in [
                "changed-attestation",
                "missing-pin",
                "wrong-pin",
                "same-session",
            ] {
                let mut broken = good.clone();
                match kind {
                    "changed-attestation" => set(
                        broken.get_mut("quiescence_attestation").unwrap(),
                        "attestation_id",
                        s(uuid()),
                    ),
                    "missing-pin" => {
                        if let Value::Object(rows) = broken.get_mut("review").unwrap() {
                            rows.retain(|(k, _)| k != "quiescence_attestation_sha256");
                        }
                    }
                    "wrong-pin" => set(
                        broken.get_mut("review").unwrap(),
                        "quiescence_attestation_sha256",
                        s("0".repeat(64)),
                    ),
                    _ => {
                        let author = g(&broken, "author").clone();
                        set(broken.get_mut("review").unwrap(), "reviewer", author);
                    }
                }
                assert!(
                    r::prepare(&f.repo, t(&observed, "operation_id"), &broken, &f.runtime).is_err()
                );
                assert_eq!(snapshot(&f.bank), f.originals);
            }
        },
    );
}

// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_partial_original_staging_remains_incomplete_and_fresh_inspection_preserves_it
#[test]
#[ignore = "needs recovery_frontier registered"]
fn partial_original_staging_remains_incomplete_and_fresh_inspection_preserves_it() {
    isolated(
        "partial_original_staging_remains_incomplete_and_fresh_inspection_preserves_it",
        |w| {
            let f = fixture(w, false);
            let output = fault_action(
                w,
                &f,
                "inspect",
                None,
                "layout-recovery-partial-original-staging",
            );
            assert_eq!(output.status, agent_memory::process::Status::Code(1));
            let line = String::from_utf8(output.stdout).unwrap();
            let start = line.find('{').unwrap();
            let failure =
                agent_memory::developer_identity::load(line.get(start..).unwrap().as_bytes())
                    .unwrap();
            let operation = t(&failure, "operation_id");
            let directory =
                agent_memory::layout_recovery_state::paths(&f.repo, Some(operation), &f.runtime)
                    .unwrap()
                    .3
                    .unwrap();
            let staging = fs::read(directory.join("staging.json")).unwrap();
            assert!(!directory.join("manifest.json").exists());
            let status = r::status(&f.repo, Some(operation), &f.runtime).unwrap();
            assert_eq!(g(&status, "state"), &s("observation-incomplete"));
            assert!(r::prepare(&f.repo, operation, &obj([]), &f.runtime).is_err());
            let fresh = r::inspect(&f.repo, None, &f.runtime).unwrap();
            assert_ne!(g(&fresh, "operation_id"), g(&failure, "operation_id"));
            assert_eq!(fs::read(directory.join("staging.json")).unwrap(), staging);
            assert_eq!(snapshot(&f.bank), f.originals);
        },
    );
}

fn recover_fault(w: &World, pending: bool, stage: &str, mode: &str) {
    let f = fixture(w, pending);
    let (observed, proposal) = prepared(&f);
    let file = w.save(&proposal, &format!("fault-proposal-{}.json", uuid()));
    let output = fault_action(w, &f, "apply", Some(&file), stage);
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let preview = r::recover(
        &f.repo,
        Some(t(&observed, "operation_id")),
        Some(mode),
        None,
        &f.runtime,
    )
    .unwrap();
    let completed = r::recover(&f.repo, None, None, Some(&preview), &f.runtime).unwrap();
    assert_eq!(g(&completed, "verified"), &Value::Bool(true));
    let wanted = if mode == "finish" {
        f.native
            .as_object()
            .unwrap()
            .iter()
            .map(|(p, b)| (p.clone(), b.as_str().unwrap().as_bytes().to_vec()))
            .collect()
    } else {
        f.originals.clone()
    };
    assert_eq!(snapshot(&f.bank), wanted);
    let retry = r::recover(&f.repo, None, None, Some(&preview), &f.runtime).unwrap();
    assert_eq!(g(&retry, "receipt_is_historical"), &Value::Bool(true));
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_first_replacement_before_progress_supports_known_finish_and_rollback
#[test]
#[ignore = "needs recovery_frontier registered"]
fn first_replacement_before_progress_supports_known_finish_and_rollback() {
    for mode in ["finish", "rollback"] {
        isolated_variant(
            "first_replacement_before_progress_supports_known_finish_and_rollback",
            mode,
            |w| {
                recover_fault(
                    w,
                    true,
                    "layout-recovery-after-rename:activeContext.md",
                    mode,
                );
            },
        );
    }
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_known_finish_and_rollback_follow_exact_journal_boundaries
#[test]
#[ignore = "needs recovery_frontier registered"]
fn known_finish_and_rollback_follow_exact_journal_boundaries() {
    for mode in ["finish", "rollback"] {
        for stage in [
            "layout-recovery-after-journal",
            "layout-recovery-after-marker",
            "layout-recovery-after-router",
            "layout-recovery-after-receipt",
        ] {
            if mode == "rollback" && stage == "layout-recovery-after-receipt" {
                continue;
            }
            isolated_variant(
                "known_finish_and_rollback_follow_exact_journal_boundaries",
                &format!("{mode}:{stage}"),
                |w| {
                    recover_fault(w, false, stage, mode);
                },
            );
        }
    }
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_reported_sizes_fail_normal_gate_and_public_pages_reconstruct_every_original
#[test]
#[ignore = "needs recovery_frontier registered"]
fn reported_sizes_fail_normal_gate_and_public_pages_reconstruct_every_original() {
    isolated(
        "reported_sizes_fail_normal_gate_and_public_pages_reconstruct_every_original",
        |w| {
            let f = fixture(w, true);
            assert!(agent_memory::bank_contract::validate(&f.repo, None, None).is_err());
            let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
            let count = g(&observed, "page_count")
                .as_int()
                .unwrap()
                .as_i128()
                .unwrap();
            let mut rebuilt: BTreeMap<String, Vec<u8>> = BTreeMap::new();
            for number in 0..count {
                let page =
                    r::page(&f.repo, t(&observed, "operation_id"), number, &f.runtime).unwrap();
                let mut decoded = Vec::new();
                let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                let mut bits = 0u32;
                let mut width = 0;
                for byte in t(&page, "body").bytes().take_while(|b| *b != b'=') {
                    bits = (bits << 6) | alphabet.iter().position(|b| *b == byte).unwrap() as u32;
                    width += 6;
                    if width >= 8 {
                        width -= 8;
                        decoded.push((bits >> width) as u8);
                    }
                }
                let body = rebuilt.entry(t(&page, "path").to_owned()).or_default();
                assert_eq!(g(&page, "offset"), &n(body.len()));
                body.extend(decoded);
                assert!(encoded(&page).unwrap().len() <= 65536);
            }
            assert_eq!(rebuilt, f.originals);
            assert!(r::page(&f.repo, t(&observed, "operation_id"), -1, &f.runtime).is_err());
            assert!(r::page(&f.repo, t(&observed, "operation_id"), count, &f.runtime).is_err());
        },
    );
}
// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_prepare_refuses_demoting_canonical_context_or_typed_prior_guidance
#[test]
#[ignore = "needs recovery_frontier registered"]
fn prepare_refuses_demoting_canonical_context_or_typed_prior_guidance() {
    isolated(
        "prepare_refuses_demoting_canonical_context_or_typed_prior_guidance",
        |w| {
            let f = fixture(w, false);
            let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
            for path in [
                "activeContext.md".to_owned(),
                f.native
                    .as_object()
                    .unwrap()
                    .iter()
                    .find(|(p, _)| p.starts_with("topics/"))
                    .unwrap()
                    .0
                    .clone(),
            ] {
                let mut broken = request(&f, &observed);
                let source_id = g(g(&observed, "manifest"), "sources")
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|source| t(source, "path") == path)
                    .unwrap()
                    .get("source_id")
                    .unwrap()
                    .clone();
                let unit = broken
                    .get_mut("unit_map")
                    .unwrap()
                    .get_mut("units")
                    .unwrap()
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|unit| {
                        g(
                            &g(unit, "source_ranges").as_array().unwrap()[0],
                            "source_id",
                        ) == &source_id
                    })
                    .unwrap();
                let unit_id = g(unit, "unit_id").clone();
                set(unit, "disposition", s("pending-admission"));
                set(unit, "semantic_kind", s("unadmitted-source"));
                set(unit, "targets", Value::Array(vec![]));
                set(
                    unit,
                    "required_actions",
                    Value::Array(vec![s("Defer already canonical guidance")]),
                );
                let map_sha = sha(g(&broken, "unit_map"));
                let review = broken.get_mut("review").unwrap();
                set(review, "unit_map_sha256", s(map_sha));
                let decision = review
                    .get_mut("unit_decisions")
                    .unwrap()
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|decision| g(decision, "unit_id") == &unit_id)
                    .unwrap();
                set(decision, "disposition", s("pending-admission"));
                assert!(
                    r::prepare(&f.repo, t(&observed, "operation_id"), &broken, &f.runtime).is_err()
                );
                assert_eq!(snapshot(&f.bank), f.originals);
            }
        },
    );
}

fn cleanup_review(f: &Fixture, id: &str, author: &Value) -> Value {
    let first = retention::inspect(&f.repo, id, None, &f.runtime).unwrap();
    let proposal = g(&first, "proposal");
    let mut reviewer = author.clone();
    set(&mut reviewer, "session", s(uuid()));
    let review = obj([
        ("schema", s(retention::REVIEW)),
        ("review_id", s(uuid())),
        ("cleanup_id", g(proposal, "cleanup_id").clone()),
        ("preview_sha256", s(sha(proposal))),
        ("reviewer", reviewer),
        ("reviewed_at", s("2026-11-14T00:00:00+00:00")),
        ("decision", s("approve")),
        ("dependency_review_complete", Value::Bool(true)),
        ("rollback_loss_acknowledged", Value::Bool(true)),
        (
            "operator_authorization",
            obj([
                ("developer_id", s(&f.dev)),
                ("author_harness", g(author, "harness").clone()),
                ("author_session", g(author, "session").clone()),
                (
                    "evidence",
                    s("Controlled disposable authorization; not release acceptance"),
                ),
                (
                    "statement",
                    s("Delete only the reviewed evidence; surrender rollback"),
                ),
            ]),
        ),
        ("rationale", s("Complete source-bound dependency review")),
    ]);
    retention::inspect(
        &f.repo,
        id,
        Some(&obj([
            ("cleanup_id", g(proposal, "cleanup_id").clone()),
            ("review", review),
        ])),
        &f.runtime,
    )
    .unwrap()
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_public_terminal_cleanup_crash_resume_and_real_historical_retry
#[test]
#[ignore = "needs recovery_frontier registered"]
fn public_terminal_cleanup_crash_resume_and_real_historical_retry() {
    isolated(
        "public_terminal_cleanup_crash_resume_and_real_historical_retry",
        |w| {
            let f = fixture(w, false);
            let (observed, proposal) = prepared(&f);
            r::apply(&f.repo, &proposal, &f.runtime).unwrap();
            let before = snapshot(&f.bank);
            let reviewed = cleanup_review(
                &f,
                t(&observed, "operation_id"),
                g(g(&proposal, "proposal"), "author"),
            );
            let path = w.save(&reviewed, "reviewed-cleanup.json");
            let output = fault_action(
                w,
                &f,
                "retention",
                Some(&path),
                "layout-recovery-retention-after-delete-0",
            );
            assert_eq!(output.status, agent_memory::process::Status::Code(1));
            retention::apply(&f.repo, &reviewed, &f.runtime).unwrap();
            let retry = retention::apply(&f.repo, &reviewed, &f.runtime).unwrap();
            assert_eq!(g(&retry, "receipt_is_historical"), &Value::Bool(true));
            assert_eq!(
                g(
                    &r::apply(&f.repo, &proposal, &f.runtime).unwrap(),
                    "receipt_is_historical"
                ),
                &Value::Bool(true)
            );
            assert_eq!(snapshot(&f.bank), before);
        },
    );
}
// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_public_child_restore_cleanup_uses_real_parent_receipts_without_child_ledger
#[test]
#[ignore = "needs recovery_frontier registered"]
fn public_child_restore_cleanup_uses_real_parent_receipts_without_child_ledger() {
    isolated(
        "public_child_restore_cleanup_uses_real_parent_receipts_without_child_ledger",
        |w| {
            let f = fixture(w, false);
            let (observed, proposal) = prepared(&f);
            r::apply(&f.repo, &proposal, &f.runtime).unwrap();
            let child = r::rollback(
                &f.repo,
                Some(t(&observed, "operation_id")),
                None,
                &f.runtime,
            )
            .unwrap();
            assert!(
                retention::inspect(&f.repo, t(&observed, "operation_id"), None, &f.runtime)
                    .is_err()
            );
            r::rollback(&f.repo, None, Some(&child), &f.runtime).unwrap();
            let reviewed = cleanup_review(
                &f,
                t(&child, "operation_id"),
                g(g(&proposal, "proposal"), "author"),
            );
            assert_eq!(
                g(g(g(&reviewed, "proposal"), "terminal"), "kind"),
                &s("child-restore")
            );
            retention::apply(&f.repo, &reviewed, &f.runtime).unwrap();
            let parent = cleanup_review(
                &f,
                t(&observed, "operation_id"),
                g(g(&proposal, "proposal"), "author"),
            );
            retention::apply(&f.repo, &parent, &f.runtime).unwrap();
            let historical = r::rollback(&f.repo, None, Some(&child), &f.runtime).unwrap();
            assert_eq!(g(&historical, "receipt_is_historical"), &Value::Bool(true));
            assert_eq!(snapshot(&f.bank), f.originals);
        },
    );
}

#[test]
fn journal_accepts_only_an_attainable_ordered_action_prefix() {
    let root = Root::new();
    let bank = root.join("bank");
    fs::create_dir(&bank).unwrap();
    fs::write(bank.join("a.md"), b"Original A\n").unwrap();
    fs::write(bank.join("b.md"), b"Original B\n").unwrap();
    let original = r::view(&bank, &[]).unwrap();
    let facts = |row: &Value| {
        obj([
            ("uid", g(row, "uid").clone()),
            ("gid", g(row, "gid").clone()),
            (
                "mode",
                Value::Int((g(row, "mode").as_int().unwrap().as_i128().unwrap() & 0o7777).into()),
            ),
            ("xattrs", g(row, "xattrs").clone()),
        ])
    };
    let a = original
        .iter()
        .find(|row| t(row, "path") == "a.md")
        .unwrap();
    let b = original
        .iter()
        .find(|row| t(row, "path") == "b.md")
        .unwrap();
    let manifest = obj([("physical_entries", Value::Array(original.clone()))]);
    let actions = Value::Array(vec![
        obj([
            ("path", s("a.md")),
            ("before", g(a, "sha256").clone()),
            ("after", s(sha256_hex(b"New A\n"))),
        ]),
        obj([
            ("path", s("b.md")),
            ("before", g(b, "sha256").clone()),
            ("after", s(sha256_hex(b"New B\n"))),
        ]),
    ]);
    let proposal = obj([
        ("actions", actions),
        (
            "intended_files",
            obj([("a.md", s("New A\n")), ("b.md", s("New B\n"))]),
        ),
        (
            "metadata_plan",
            obj([
                ("files", obj([("a.md", facts(a)), ("b.md", facts(b))])),
                ("directories", obj([])),
            ]),
        ),
    ]);
    assert_eq!(
        r::partial_boundary(&proposal, &manifest, &original).unwrap(),
        0
    );
    fs::write(bank.join("b.md"), b"New B\n").unwrap();
    assert!(r::partial_boundary(&proposal, &manifest, &r::view(&bank, &[]).unwrap()).is_err());
    fs::write(bank.join("b.md"), b"Original B\n").unwrap();
    // Refresh the physical observation of this unchanged participant before
    // exercising a permitted prefix.
    let original = r::view(&bank, &[]).unwrap();
    let manifest = obj([("physical_entries", Value::Array(original.clone()))]);
    fs::write(bank.join("a.md"), b"New A\n").unwrap();
    let prefix = r::view(&bank, &[]).unwrap();
    assert_eq!(
        r::partial_boundary(&proposal, &manifest, &prefix).unwrap(),
        1
    );
    fs::write(bank.join("unowned.bin"), b"External occupant\n").unwrap();
    assert!(r::partial_boundary(&proposal, &manifest, &r::view(&bank, &[]).unwrap()).is_err());
    fs::remove_file(bank.join("unowned.bin")).unwrap();
    fs::write(bank.join("b.md"), b"New B\n").unwrap();
    let complete = r::view(&bank, &[]).unwrap();
    assert_eq!(
        r::partial_boundary(&proposal, &manifest, &complete).unwrap(),
        2
    );
    let plan = obj([
        ("initial_view", Value::Array(complete)),
        (
            "actions",
            Value::Array(vec![
                obj([("path", s("a.md")), ("after", g(a, "sha256").clone())]),
                obj([("path", s("b.md")), ("after", g(b, "sha256").clone())]),
            ]),
        ),
    ]);
    fs::write(bank.join("a.md"), b"Original A\n").unwrap();
    assert_eq!(
        r::rollback_boundary(&plan, &manifest, &r::view(&bank, &[]).unwrap()).unwrap(),
        1
    );
}

// obligation: tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract.test_public_purge_and_age_cannot_retire_pending_hold
#[test]
#[ignore = "needs recovery_frontier registered"]
fn public_purge_and_age_cannot_retire_pending_hold() {
    isolated("public_purge_and_age_cannot_retire_pending_hold", |w| {
        let f = fixture(w, true);
        let env = agent_memory::installation::Environment::process();
        let services = agent_memory::installation::Services::registered().unwrap();
        let mut purge = agent_memory::purging::PurgeRequest {
            repo: f.repo.clone(),
            path: f.bank.clone(),
            check: true,
            confirm: None,
        };
        let preview = agent_memory::purging::run(&env, &services, &purge).unwrap();
        let confirmation = t(g(&preview, "preview"), "confirmation").to_owned();
        let (observed, proposal) = prepared(&f);
        let applied = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
        let operation = r::operation(&f.repo, t(&observed, "operation_id"), &f.runtime).unwrap();
        fn age(path: &Path) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    age(&path);
                } else {
                    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
                    fs::File::open(path)
                        .unwrap()
                        .set_times(fs::FileTimes::new().set_accessed(time).set_modified(time))
                        .unwrap();
                }
            }
        }
        age(&operation.directory);
        let before = (snapshot(&f.bank), snapshot(&operation.directory));
        for check in [true, false] {
            purge.check = check;
            purge.confirm = if check {
                None
            } else {
                Some(confirmation.clone())
            };
            assert!(agent_memory::purging::run(&env, &services, &purge).is_err());
            assert_eq!((snapshot(&f.bank), snapshot(&operation.directory)), before);
        }
        let status = r::status(&f.repo, Some(t(&observed, "operation_id")), &f.runtime).unwrap();
        assert_eq!(
            g(&status, "semantic_recovery_complete"),
            &Value::Bool(false)
        );
        assert_eq!(
            g(&status, "pending_unit_ids"),
            g(&applied, "pending_unit_ids")
        );
    });
}

#[test]
fn publication_judgment_matches_python_strip_without_rewriting_padded_value() {
    let padded = s("\u{001f} source-bound judgment \u{001f}");
    assert_eq!(
        agent_memory::layout_recovery_publication::text(&padded, 4096).unwrap(),
        "\u{001f} source-bound judgment \u{001f}"
    );
    let only_python_whitespace = s("\u{001f} \u{001f}");
    assert_eq!(
        agent_memory::layout_recovery_publication::text(&only_python_whitespace, 4096)
            .unwrap_err()
            .to_string(),
        "A bounded explicit source-preservation judgment is required"
    );
}

#[path = "support/fanout_c.rs"]
mod fanout_c;
