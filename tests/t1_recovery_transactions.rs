#[allow(unused_imports)]
mod support;
use agent_memory::{
    bank_transactions as tx,
    codec::json::{self, Value},
    developer_identity::{digest, encoded},
    recovery_transactions as recovery,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use support::{
    record_fixtures,
    world::{RunOptions, World},
};
fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}
fn n(v: usize) -> Value {
    Value::Int(v.into())
}
fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn g<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap()
}
fn t<'a>(v: &'a Value, k: &str) -> &'a str {
    g(v, k).as_str().unwrap()
}
fn array(v: &Value) -> &[Value] {
    v.as_array().unwrap()
}
fn set(v: &mut Value, k: &str, x: Value) {
    if let Value::Object(rows) = v {
        if let Some((_, v)) = rows.iter_mut().find(|(key, _)| key == k) {
            *v = x;
        } else {
            rows.push((k.into(), x));
        }
    }
}
fn sha(v: &Value) -> String {
    digest(Some(&encoded(v).unwrap())).unwrap()
}
fn bytes_sha(v: &[u8]) -> String {
    digest(Some(v)).unwrap()
}
fn uuid() -> String {
    agent_memory::host::uuid4().unwrap()
}
fn private(path: &Path, value: &Value) {
    fs::write(path, encoded(value).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn proposal(base: &Value, files: &Value) -> Value {
    obj([
        ("observed_hashes", tx::file_hashes(base).unwrap()),
        ("files", files.clone()),
    ])
}
#[test]
fn legal_partial_accepts_only_forward_order_with_router_last() {
    let base = obj([
        ("MEMORY.md", s("old router")),
        ("topics/a.md", s("old a")),
        ("topics/deleted.md", s("delete")),
    ]);
    let files = obj([
        ("MEMORY.md", s("new router")),
        ("topics/a.md", s("new a")),
        ("topics/new.md", s("new source")),
    ]);
    let p = proposal(&base, &files);
    let mut running = tx::file_hashes(&base).unwrap();
    recovery::legal_partial(&p, &base, &running).unwrap();
    let final_hashes = tx::file_hashes(&files).unwrap();
    for path in [
        "topics/a.md",
        "topics/deleted.md",
        "topics/new.md",
        "MEMORY.md",
    ] {
        if let Some(hash) = final_hashes.get(path) {
            set(&mut running, path, hash.clone());
        } else if let Value::Object(rows) = &mut running {
            rows.retain(|(k, _)| k != path);
        }
        recovery::legal_partial(&p, &base, &running).unwrap();
    }
    for path in ["MEMORY.md", "topics/new.md"] {
        let mut bad = tx::file_hashes(&base).unwrap();
        set(&mut bad, path, g(&final_hashes, path).clone());
        assert!(recovery::legal_partial(&p, &base, &bad).is_err());
    }
    set(&mut running, "external.md", s("a".repeat(64)));
    assert!(recovery::legal_partial(&p, &base, &running).is_err());
}
#[test]
fn rollback_plan_keeps_durable_initial_state_and_accepts_only_ordered_rollback_boundaries() {
    let world = World::new().unwrap();
    let dir = world.inside("rollback").unwrap();
    fs::create_dir(&dir).unwrap();
    let base = obj([("MEMORY.md", s("old router")), ("topics/a.md", s("old a"))]);
    let files = obj([
        ("MEMORY.md", s("new router")),
        ("topics/a.md", s("new a")),
        ("topics/new.md", s("new item")),
    ]);
    let journal = obj([
        ("operation_id", s(uuid())),
        ("nested_id", s(uuid())),
        ("proposal", proposal(&base, &files)),
        ("base_files", base.clone()),
    ]);
    let observed = tx::file_hashes(&files).unwrap();
    let (plan, boundary) = recovery::rollback_plan(&dir, &journal, &observed).unwrap();
    assert_eq!(boundary, 0);
    let actions = array(g(&plan, "actions"));
    assert_eq!(
        actions.iter().map(|a| t(a, "path")).collect::<Vec<_>>(),
        vec!["topics/a.md", "topics/new.md", "MEMORY.md"]
    );
    private(&dir.join("rollback-plan.json"), &plan);
    let durable = fs::read(dir.join("rollback-plan.json")).unwrap();
    let mut running = observed.clone();
    for (index, action) in actions.iter().enumerate() {
        let path = t(action, "path");
        if g(action, "after") == &Value::Null {
            if let Value::Object(rows) = &mut running {
                rows.retain(|(k, _)| k != path);
            }
        } else {
            set(&mut running, path, g(action, "after").clone());
        }
        let (again, boundary) = recovery::rollback_plan(&dir, &journal, &running).unwrap();
        assert_eq!(again, plan);
        assert_eq!(boundary, index + 1);
    }
    assert_eq!(fs::read(dir.join("rollback-plan.json")).unwrap(), durable);
    let mut external = running.clone();
    set(&mut external, "foreign.md", s("0".repeat(64)));
    assert!(recovery::rollback_plan(&dir, &journal, &external).is_err());
    let mut corrupt = plan.clone();
    set(&mut corrupt, "journal_sha256", s("0".repeat(64)));
    private(&dir.join("rollback-plan.json"), &corrupt);
    assert!(recovery::rollback_plan(&dir, &journal, &observed).is_err());
}
#[test]
fn checkpoint_receipt_preserves_write_identity_and_rejects_every_changed_binding() {
    let world = World::new().unwrap();
    let path = world.inside("receipt.json").unwrap();
    let base = obj([("MEMORY.md", s("canonical router"))]);
    let observed = tx::file_hashes(&base).unwrap();
    let write = obj([
        ("operation_id", s(uuid())),
        ("project_id", s(uuid())),
        ("observed_hashes", observed.clone()),
        ("path", s("activeContext.md")),
        (
            "content",
            s("# Current focus\n\nPreserve exact checkpoint evidence.\n"),
        ),
    ]);
    let journal = obj([("write_proposal", write.clone())]);
    assert!(recovery::nested_receipt(&journal, &path).unwrap().is_none());
    let mut resulting = observed;
    set(
        &mut resulting,
        "activeContext.md",
        s(bytes_sha(t(&write, "content").as_bytes())),
    );
    let receipt = obj([
        ("schema", s("agent-memory/write-receipt-v1")),
        ("operation_id", g(&write, "operation_id").clone()),
        ("project_id", g(&write, "project_id").clone()),
        ("request_sha256", s(sha(&write))),
        ("resulting_hashes", resulting),
        ("outcome", s("replaced")),
        ("verified", Value::Bool(true)),
    ]);
    private(&path, &receipt);
    assert_eq!(
        encoded(&recovery::nested_receipt(&journal, &path).unwrap().unwrap()).unwrap(),
        encoded(&receipt).unwrap()
    );
    for (key, value) in [
        ("schema", s("foreign")),
        ("operation_id", s(uuid())),
        ("project_id", s(uuid())),
        ("request_sha256", s("0".repeat(64))),
        ("resulting_hashes", obj([])),
        ("outcome", s("rolled-back")),
        ("verified", Value::Bool(false)),
    ] {
        let mut bad = receipt.clone();
        set(&mut bad, key, value);
        private(&path, &bad);
        let before = fs::read(&path).unwrap();
        assert!(recovery::nested_receipt(&journal, &path).is_err());
        assert_eq!(before, fs::read(&path).unwrap());
    }
}
#[test]
fn transaction_route_is_declared_in_baseline_position_and_has_native_parser() {
    agent_memory::cli::register_providers().unwrap();
    let names = agent_memory::cli::table::ENTRIES
        .iter()
        .map(|e| e.name())
        .collect::<Vec<_>>();
    let position = names.iter().position(|n| *n == "transaction").unwrap();
    assert_eq!(names[position - 1], "writes");
    assert_eq!(names[position + 1], "migrate");
    let result = agent_memory::cli::dispatch(
        &["transaction".into(), "preview".into(), "--help".into()],
        "am",
    );
    assert_eq!(result.result.exit_code, 0);
    assert!(result.result.stdout.contains("--files"));
}
const POLICY: &str = "# Synthetic policy 1\n\nIn the synthetic offline-queue environment, attempt delivery at most three times, retain every queued item after failure, and resume delivery only after the operator changes the environment to online.\n";
fn installed_runtime(world: &World) -> (PathBuf, PathBuf) {
    fn copy_assets(source: &Path, target: &Path) {
        fs::create_dir_all(target).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy_assets(&entry.path(), &target.join(entry.file_name()));
            } else if entry.file_type().unwrap().is_file() {
                fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
            }
        }
    }
    let source = world.inside("development-runtime").unwrap();
    copy_assets(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        &source.join("skills/agent-memory"),
    );
    for relative in ["target/release/am", "bin/am"] {
        let binary = source.join(relative);
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_am"), &binary).unwrap();
        fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = world
        .am(vec![
            OsString::from("--json"),
            "install".into(),
            "--development-source".into(),
            source.into_os_string(),
            "--mode".into(),
            "link".into(),
            "--presets".into(),
            "none".into(),
            "--no-guidance".into(),
            "--bin-dir".into(),
            world.inside("bin").unwrap().into_os_string(),
        ])
        .unwrap();
    assert_eq!(output.status, agent_memory::process::Status::Code(0));
    let receipt = json::decode_bytes(
        &fs::read(
            world
                .inside("state/agent-memory/install/receipt.json")
                .unwrap(),
        )
        .unwrap(),
        Default::default(),
    )
    .unwrap();
    let source = g(&receipt, "source");
    let command = agent_memory::installation::source_binary(source).unwrap();
    let pack = Path::new(t(source, "pack"));
    (
        pack.parent().unwrap().parent().unwrap().to_path_buf(),
        command,
    )
}
struct Fixture {
    world: World,
    repo: PathBuf,
    bank: PathBuf,
    runtime: PathBuf,
    command: PathBuf,
    project: String,
    actor: String,
    operation: String,
    directory: PathBuf,
    manifest: Value,
    native: Value,
}
impl Fixture {
    fn call(&self, args: Vec<OsString>, fault: Option<(&str, &str)>) -> (i32, Value) {
        let env = fault
            .map(|(stage, mode)| {
                vec![
                    ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
                    ("AGENT_MEMORY_FAULT_MODE".into(), mode.into()),
                ]
            })
            .unwrap_or_default();
        let output = self
            .world
            .run_with(
                std::iter::once(self.command.clone().into_os_string())
                    .chain(std::iter::once(OsString::from("--json")))
                    .chain(args),
                RunOptions {
                    check: false,
                    env: &env,
                    ..Default::default()
                },
            )
            .unwrap();
        let code = match output.status {
            agent_memory::process::Status::Code(code) => code,
            _ => panic!("Child did not exit normally"),
        };
        let value = if output.stdout.is_empty() {
            Value::Null
        } else {
            json::decode_bytes(&output.stdout, Default::default()).unwrap()
        };
        (code, value)
    }
    fn good(&self, args: Vec<OsString>) -> Value {
        let (code, value) = self.call(args, None);
        assert_eq!(code, 0, "{value:?}");
        value
    }
    fn save(&self, value: &Value) -> OsString {
        let path = self.world.inside(format!("input-{}.json", uuid())).unwrap();
        private(&path, value);
        path.into_os_string()
    }
    fn layout(&self, operation: &str, extra: Vec<OsString>, expected: i32) -> Value {
        let mut args = vec![
            "bank".into(),
            "recover-layout".into(),
            operation.into(),
            "--repo".into(),
            self.repo.clone().into_os_string(),
        ];
        args.extend(extra);
        let (code, value) = self.call(args, None);
        assert_eq!(code, expected, "{value:?}");
        value
    }
    fn private_layout(&self, operation: &str, extra: Vec<OsString>) -> Value {
        let path = self
            .world
            .inside(format!("private-{}.json", uuid()))
            .unwrap();
        let mut extra = extra;
        extra.extend(["--output".into(), path.clone().into_os_string()]);
        self.layout(operation, extra, 0);
        json::decode_bytes(&fs::read(path).unwrap(), Default::default()).unwrap()
    }
    fn new() -> Self {
        Self::with_checkpoint(false)
    }
    fn with_checkpoint(checkpoint: bool) -> Self {
        agent_memory::cli::register_providers().unwrap();
        let world = World::new().unwrap();
        let (runtime, command) = installed_runtime(&world);
        let repo = world.repository("repo", false).unwrap();
        fs::write(repo.join("synthetic-policy.md"), POLICY).unwrap();
        world.git(["add", "."]).unwrap();
        world
            .git(["commit", "-m", "Independent disposable policy evidence"])
            .unwrap();
        let mut proposal = json::decode(
            include_str!("../skills/agent-memory/templates/router-proposal.example.json"),
            Default::default(),
        )
        .unwrap();
        let project = uuid();
        set(&mut proposal, "project-id", s(&project));
        if checkpoint {
            set(
                proposal.get_mut("contexts").unwrap(),
                "activeContext.md",
                obj([
                    (
                        "body",
                        s(
                            "# Active context\n\n## Current focus\n\nOld focus.\n\n## Recent changes\n\nEarlier meaningful result with its source.\n\n## Open questions\n\nDoes this hold beyond the retry component?\n\n## Next step\n\nInspect the other clone.\n\n## Source notes\n\nKeep branch:legacy, original developer and [evidence](../legacy-result.md).\n\n```markdown\n## Current focus\nThis is an example, not a duplicate checkpoint section.\n```\n",
                        ),
                    ),
                    ("purpose", s("Active context")),
                    ("scope", s("repository")),
                    ("lifecycle", s("active")),
                    ("read-when", s("When resuming")),
                ]),
            );
            if let Value::Object(omitted) = proposal.get_mut("omitted").unwrap() {
                omitted.retain(|(k, _)| k != "activeContext.md");
            }
        }
        let rendered = agent_memory::bank_contract::render(&proposal).unwrap();
        let bank = repo.join("memory-bank");
        fs::create_dir(&bank).unwrap();
        for (path, body) in g(&rendered, "files").as_object().unwrap() {
            fs::write(bank.join(path), body.as_str().unwrap()).unwrap();
        }
        let native = g(&rendered, "files").clone();
        let scope = support::identity_fixtures::bind(&world, &repo).unwrap();
        let actor = t(&scope, "developer_id").to_owned();
        fs::write(bank.join("handoff-prompt.md"), POLICY).unwrap();
        let mut f = Self {
            world,
            repo,
            bank,
            runtime,
            command,
            project,
            actor,
            operation: String::new(),
            directory: PathBuf::new(),
            manifest: Value::Null,
            native,
        };
        let observed = f.private_layout("inspect", vec![]);
        f.operation = t(&observed, "operation_id").to_owned();
        f.manifest = g(&observed, "manifest").clone();
        let request = f.layout_request();
        let proposal = f.private_layout(
            "prepare",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--input".into(),
                f.save(&request),
            ],
        );
        f.layout("apply", vec!["--proposal".into(), f.save(&proposal)], 0);
        f.directory = PathBuf::from(
            f.world
                .env
                .get(std::ffi::OsStr::new("AGENT_MEMORY_STATE_HOME"))
                .unwrap(),
        )
        .join("writes")
        .join(bytes_sha(f.bank.to_str().unwrap().as_bytes()))
        .join("layout-recovery")
        .join(&f.operation);
        assert!(f.directory.join("ledger-head.json").exists());
        f
    }
    fn layout_request(&self) -> Value {
        let m = &self.manifest;
        let author = obj([
            ("developer_id", s(&self.actor)),
            ("harness", s("controlled-source-author")),
            ("session", s(format!("author-{}", self.operation))),
            (
                "evidence",
                s("Controlled fixture authorship, not product acceptance."),
            ),
        ]);
        let mut reviewer = author.clone();
        set(
            &mut reviewer,
            "harness",
            s("controlled-independent-reviewer"),
        );
        set(
            &mut reviewer,
            "session",
            s(format!("reviewer-{}", self.operation)),
        );
        let mut cursor = 0usize;
        let units=array(g(m,"sources")).iter().map(|source|{let path=t(source,"path");let count=g(source,"byte_count").as_int().unwrap().as_u64().unwrap() as usize;let pages=(count.div_ceil(24576)).max(1);let context=(cursor..cursor+pages).map(n).collect();cursor+=pages;let pending=path=="handoff-prompt.md";obj([("unit_id",s(uuid())),("source_ranges",Value::Array(vec![obj([("source_id",g(source,"source_id").clone()),("start",n(0)),("end",n(count))])])),("semantic_kind",s(if pending{"unadmitted-source"}else if path=="MEMORY.md"{"canonical-framing"}else{"canonical-guidance"})),("existing_record",Value::Null),("provenance",obj([("origin",s(if pending{"unknown"}else{"existing canonical fixture"})),("uncertainty",Value::Bool(pending))])),("disposition",s(if pending{"pending-admission"}else{"preserve"})),("targets",Value::Array(if pending{vec![]}else{vec![obj([("path",s(path)),("sha256",s(bytes_sha(t(&self.native,path).as_bytes())))])]})),("reason",s(if pending{"Unconfirmed candidate needs separate ordinary admission"}else{"Preserve complete synthetic meaning and conditions"})),("required_actions",Value::Array(if pending{vec![s("Confirm independent repository evidence before ordinary reviewed admission")]}else{vec![]})),("context_pages",Value::Array(context))])}).collect::<Vec<_>>();
        let unit_map = obj([
            ("schema", s("agent-memory/layout-unit-map-v1")),
            ("operation_id", s(&self.operation)),
            ("manifest_sha256", s(sha(m))),
            ("units", Value::Array(units.clone())),
        ]);
        let physical = array(g(m, "physical_entries"));
        let metadata = obj([
            (
                "files",
                Value::Object(
                    self.native
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(path, _)| {
                            let row = physical.iter().find(|r| t(r, "path") == path).unwrap();
                            (
                                path.clone(),
                                obj([
                                    ("uid", g(row, "uid").clone()),
                                    ("gid", g(row, "gid").clone()),
                                    (
                                        "mode",
                                        Value::Int(
                                            (g(row, "mode").as_int().unwrap().as_u64().unwrap()
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
        let runtime = agent_memory::recovery_quiescence::measured_runtime(&self.runtime).unwrap();
        let review_id = uuid();
        let attestation = obj([
            ("schema", s("agent-memory/layout-quiescence-attestation-v1")),
            ("attestation_id", s(uuid())),
            ("operation_id", s(&self.operation)),
            ("project_id", s(&self.project)),
            ("repo_root", s(self.repo.to_str().unwrap())),
            ("bank_path", s(self.bank.to_str().unwrap())),
            (
                "checkout_key",
                g(g(g(m, "identity"), "scope_context"), "checkout_key").clone(),
            ),
            (
                "state_root",
                s(Path::new(
                    self.world
                        .env
                        .get(std::ffi::OsStr::new("AGENT_MEMORY_STATE_HOME"))
                        .unwrap(),
                )
                .to_str()
                .unwrap()),
            ),
            ("manifest_sha256", s(sha(m))),
            (
                "runtime_artifact_digest",
                g(&runtime, "artifact_digest").clone(),
            ),
            ("operator_developer_id", s(&self.actor)),
            ("issued_at", g(m, "inspected_at").clone()),
            ("declared_principals", Value::Array(vec![s(&self.actor)])),
            (
                "declared_invocation_routes",
                Value::Array(vec![s(self.command.to_str().unwrap())]),
            ),
            ("stopped_sessions", Value::Array(vec![])),
            (
                "statement",
                s(
                    "All authorized legacy invocation routes and in-flight sessions are disabled or quiesced until terminal recovery retirement, or use the independently verified capable runtime and bound private state.",
                ),
            ),
            ("review_id", s(&review_id)),
        ]);
        let review = obj([
            ("schema", s("agent-memory/layout-review-v1")),
            ("review_id", s(review_id)),
            ("operation_id", s(&self.operation)),
            ("manifest_sha256", s(sha(m))),
            ("unit_map_sha256", s(sha(&unit_map))),
            ("intended_files_sha256", s(sha(&self.native))),
            ("metadata_plan_sha256", s(sha(&metadata))),
            (
                "authority_sha256",
                s(sha(g(g(m, "identity"), "project_evidence"))),
            ),
            ("author", author.clone()),
            ("reviewer", reviewer),
            ("reviewed_at", g(m, "inspected_at").clone()),
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
            ("semantic_preservation", Value::Bool(true)),
            ("security", Value::Bool(true)),
            ("identity", Value::Bool(true)),
            ("decision", s("approve")),
            (
                "evidence",
                s(
                    "Controlled distinct-session review of all source bytes, context and intended native result.",
                ),
            ),
            ("quiescence_attestation_sha256", s(sha(&attestation))),
        ]);
        obj([
            ("schema", s("agent-memory/layout-prepare-v1")),
            ("operation_id", s(&self.operation)),
            ("author", author),
            ("unit_map", unit_map),
            ("intended_files", self.native.clone()),
            ("metadata_plan", metadata),
            ("review", review),
            ("quiescence_attestation", attestation),
            ("retained_source_ids", Value::Array(vec![])),
        ])
    }
    fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        snapshot(&self.bank, true)
    }
    fn nested_preview(&self, nested: &str, mode: &str) -> Value {
        self.private_layout(
            "recover-nested",
            vec![
                "--operation".into(),
                self.operation.clone().into(),
                "--nested".into(),
                nested.into(),
                "--mode".into(),
                mode.into(),
            ],
        )
    }
    fn unlock(&self) {
        let value = self.good(vec![
            "writes".into(),
            "recover".into(),
            "--repo".into(),
            self.repo.clone().into_os_string(),
        ]);
        if value.get("proposal").is_some_and(|v| v != &Value::Null) {
            self.good(vec![
                "writes".into(),
                "recover".into(),
                "--repo".into(),
                self.repo.clone().into_os_string(),
                "--proposal".into(),
                self.save(&value),
            ]);
        }
    }
    fn admission(&self, full: bool) -> (Value, String, String) {
        let raw = record_fixtures::record_text_with_ids(
            POLICY,
            &self.project,
            record_fixtures::RecordOptions {
                status: "candidate",
                ..Default::default()
            },
            &uuid(),
            &self.actor,
        )
        .unwrap();
        let allowed = g(&agent_memory::records::MODEL, "properties")
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>();
        let (metadata, _) = agent_memory::codec::frontmatter::read(&raw, &allowed).unwrap();
        let mut metadata = Value::Object(metadata);
        set(
            &mut metadata,
            "evidence",
            Value::Array(vec![s("synthetic-policy.md")]),
        );
        set(
            &mut metadata,
            "source",
            obj([
                ("kind", s("verified-repository")),
                ("actor", s("Explicit disposable repository evidence")),
            ]),
        );
        let request = obj([
            ("project_id", s(&self.project)),
            (
                "candidate",
                obj([("metadata", metadata), ("body", s(POLICY))]),
            ),
            ("source_class", s("verified-repository")),
            (
                "repository_evidence",
                obj([
                    ("path", s("synthetic-policy.md")),
                    (
                        "sha256",
                        s(bytes_sha(
                            &fs::read(self.repo.join("synthetic-policy.md")).unwrap(),
                        )),
                    ),
                ]),
            ),
            (
                "assessment",
                obj([
                    ("category", s("durable-discovery")),
                    (
                        "utility",
                        s("Prevent repeated excessive retries under these conditions."),
                    ),
                    ("durable", Value::Bool(true)),
                    ("evidence_confidence", s("confirmed")),
                    (
                        "evidence_reason",
                        s("The independently readable synthetic repository policy confirms this."),
                    ),
                    ("confirmed", Value::Bool(true)),
                    ("source_redundant", Value::Bool(false)),
                    ("work_state_copy", Value::Bool(false)),
                    (
                        "review",
                        s(
                            "Controlled review of actual independently readable repository evidence.",
                        ),
                    ),
                ]),
            ),
        ]);
        let captured = self.good(vec![
            "capture".into(),
            "--repo".into(),
            self.repo.clone().into_os_string(),
            "--request".into(),
            self.save(&request),
        ]);
        let captured = g(&captured, "capture");
        let compared = obj([
            ("project_id", s(&self.project)),
            ("candidate", g(g(captured, "request"), "candidate").clone()),
            (
                "searches",
                Value::Array(vec![
                    s("offline delivery queue"),
                    s("retain items after failure"),
                ]),
            ),
            ("read_paths", Value::Array(vec![])),
        ]);
        let inspection = self
            .world
            .inside(format!("comparison-{}.json", uuid()))
            .unwrap();
        self.good(vec![
            "compare".into(),
            "inspect".into(),
            "--repo".into(),
            self.repo.clone().into_os_string(),
            "--request".into(),
            self.save(&compared),
            "--private-output".into(),
            inspection.clone().into_os_string(),
        ]);
        let view = json::decode_bytes(&fs::read(inspection).unwrap(), Default::default()).unwrap();
        let nested = uuid();
        let stamp = g(&self.manifest, "inspected_at");
        let intended = agent_memory::admission::desired(
            &agent_memory::bank_transactions::files_at(&self.bank).unwrap(),
            &obj([
                ("capture", captured.clone()),
                ("recorded_at", stamp.clone()),
            ]),
            &nested,
        )
        .unwrap();
        let target = format!(
            "topics/{}.md",
            t(g(g(g(captured, "request"), "candidate"), "metadata"), "id")
        );
        let metadata = g(g(g(captured, "request"), "candidate"), "metadata");
        let pending=array(g(&view,"pending_matches")).iter().map(|row|{let claim=obj([("unit_id",g(row,"unit_id").clone()),("coverage",s(if full{"full"}else{"none"})),("covered_source_ranges",if full{g(row,"source_ranges").clone()}else{Value::Array(vec![])}),("target_record_refs",Value::Array(if full{vec![obj([("id",g(metadata,"id").clone()),("version",n(1)),("path",s(&target)),("resulting_sha256",s(bytes_sha(t(&intended,&target).as_bytes())))])]}else{vec![]}))]);obj([("unit_id",g(row,"unit_id").clone()),("classification",s("distinct-knowledge")),("rationale",s("Complete original source meaning and conditions reviewed independently.")),("coverage_claim",claim)])}).collect::<Vec<_>>();
        let judgment = obj([
            ("observation", g(&view, "observation").clone()),
            (
                "decisions",
                Value::Array(
                    array(g(&view, "matches"))
                        .iter()
                        .map(|row| {
                            obj([
                                ("path", g(row, "path").clone()),
                                ("classification", s("distinct-knowledge")),
                                (
                                    "rationale",
                                    s("Different independently inspected prior meaning."),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("pending_decisions", Value::Array(pending.clone())),
            (
                "rationale",
                s("Complete controlled comparison, not a product acceptance verdict."),
            ),
        ]);
        let proof = obj([
            ("schema", s("agent-memory/comparison-proof-v2")),
            ("judgment", judgment.clone()),
            ("disposition", s("create-distinct")),
            ("targets", Value::Array(vec![])),
        ]);
        let head = json::decode_bytes(
            &fs::read(self.directory.join("ledger-head.json")).unwrap(),
            Default::default(),
        )
        .unwrap();
        let generation = g(&head, "generation").as_int().unwrap().as_u64().unwrap();
        let ledger = json::decode_bytes(
            &fs::read(
                self.directory
                    .join("ledger")
                    .join(format!("{generation:08}.json")),
            )
            .unwrap(),
            Default::default(),
        )
        .unwrap();
        let review = obj([
            ("schema", s("agent-memory/layout-coverage-review-v1")),
            ("review_id", s(uuid())),
            ("operation_id", s(&self.operation)),
            ("reviewer_developer_id", s(&self.actor)),
            ("reviewer_harness", s("controlled-coverage-reviewer")),
            ("reviewer_session", s(format!("coverage-{nested}"))),
            ("review_time", stamp.clone()),
            ("manifest_sha256", g(&ledger, "manifest_sha256").clone()),
            ("unit_map_sha256", g(&ledger, "unit_map_sha256").clone()),
            ("ledger_head_sha256", g(&head, "sha256").clone()),
            ("comparison_proof_input_sha256", s(sha(&proof))),
            ("intended_output_sha256", s(sha(&intended))),
            (
                "claims",
                Value::Array(
                    pending
                        .iter()
                        .map(|row| g(row, "coverage_claim").clone())
                        .collect(),
                ),
            ),
            (
                "rationale",
                s(
                    "Controlled distinct-session review of full original ranges and complete intended result.",
                ),
            ),
            ("decision", s("approve")),
        ]);
        let proof_path = self.world.inside(format!("proof-{}.json", uuid())).unwrap();
        self.good(vec![
            "compare".into(),
            "review".into(),
            "--repo".into(),
            self.repo.clone().into_os_string(),
            "--request".into(),
            self.save(&compared),
            "--judgment".into(),
            self.save(&judgment),
            "--coverage-review".into(),
            self.save(&review),
            "--private-output".into(),
            proof_path.clone().into_os_string(),
        ]);
        let output = self
            .world
            .inside(format!("admission-{}.json", uuid()))
            .unwrap();
        self.good(vec![
            "admission".into(),
            "preview".into(),
            "--repo".into(),
            self.repo.clone().into_os_string(),
            "--capture".into(),
            self.save(captured),
            "--request".into(),
            self.save(&compared),
            "--proof".into(),
            proof_path.into_os_string(),
            "--operation-id".into(),
            nested.clone().into(),
            "--recorded-at".into(),
            stamp.as_str().unwrap().into(),
            "--private-output".into(),
            output.clone().into_os_string(),
        ]);
        (
            json::decode_bytes(&fs::read(output).unwrap(), Default::default()).unwrap(),
            nested,
            target,
        )
    }
    fn fault_admission(&self, p: &Value, stage: &str, mode: &str) {
        let (code, value) = self.call(
            vec![
                "admission".into(),
                "apply".into(),
                "--proposal".into(),
                self.save(p),
            ],
            Some((stage, mode)),
        );
        assert_eq!(code, if mode == "exit" { 86 } else { 1 }, "{value:?}");
        if mode == "exit" {
            self.unlock();
        }
    }
    fn recover(&self, p: &Value, expected: i32) {
        self.layout(
            "recover-nested",
            vec!["--proposal".into(), self.save(p)],
            expected,
        );
    }
}
fn snapshot(root: &Path, canonical: bool) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, here: &Path, canonical: bool, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(here).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, canonical, out);
            } else if !canonical
                || !path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with(".agent-memory")
            {
                out.insert(
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
    let mut out = BTreeMap::new();
    visit(root, root, canonical, &mut out);
    out
}
// obligation: tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_durable_canonical_receipt_reconciles_only_its_exact_journaled_head_gap
#[test]
#[ignore = "needs recovery_frontier registered"]
fn durable_canonical_receipt_reconciles_only_its_exact_journaled_head_gap() {
    for mode in ["error", "exit"] {
        let f = Fixture::new();
        let originals = snapshot(&f.directory, false);
        let (p, nested, target) = f.admission(true);
        f.fault_admission(&p, "layout-recovery-ledger-generation-after-head", mode);
        let head = json::decode_bytes(
            &fs::read(f.directory.join("ledger-head.json")).unwrap(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(g(&head, "generation"), &n(1));
        let index_path = f.directory.parent().unwrap().join("index.json");
        let index = fs::read(&index_path).unwrap();
        let indexed = json::decode_bytes(&index, Default::default()).unwrap();
        assert_ne!(
            g(
                g(g(&indexed, "operations"), &f.operation),
                "ledger_head_sha256"
            ),
            &s(sha(&head))
        );
        let marker = fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap();
        let bank_before = f.snapshot();
        assert_eq!(
            bank_before[&target],
            t(g(g(&p, "proposal"), "files"), &target).as_bytes()
        );
        f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            1,
        );
        let transition = f.directory.join("ledger-transitions/00000001.json");
        let raw = fs::read(&transition).unwrap();
        let mut corrupt = json::decode_bytes(&raw, Default::default()).unwrap();
        set(&mut corrupt, "units_sha256", s("0".repeat(64)));
        private(&transition, &corrupt);
        f.layout(
            "recover-nested",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--nested".into(),
                nested.clone().into(),
                "--mode".into(),
                "finish".into(),
                "--output".into(),
                f.world.inside("refused.json").unwrap().into_os_string(),
            ],
            1,
        );
        assert_eq!(f.snapshot(), bank_before);
        assert_eq!(fs::read(&index_path).unwrap(), index);
        assert_eq!(
            fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
            marker
        );
        fs::write(&transition, raw).unwrap();
        let private_before = snapshot(&f.directory, false);
        let preview = f.nested_preview(&nested, "finish");
        assert_eq!(snapshot(&f.directory, false), private_before);
        assert_eq!(fs::read(&index_path).unwrap(), index);
        f.layout(
            "recover-nested",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--nested".into(),
                nested.clone().into(),
                "--mode".into(),
                "rollback".into(),
                "--output".into(),
                f.world
                    .inside("refused-rollback.json")
                    .unwrap()
                    .into_os_string(),
            ],
            1,
        );
        if mode == "error" {
            let rendezvous = f.world.inside("contention").unwrap();
            let env = vec![
                (
                    "AGENT_MEMORY_FAULT_STAGE".into(),
                    "layout-recovery-admission-receipt-before-resolution".into(),
                ),
                ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
                (
                    "AGENT_MEMORY_FAULT_DIR".into(),
                    rendezvous.clone().into_os_string(),
                ),
            ];
            let input = f.save(&preview);
            let child = f
                .world
                .spawn_with(
                    vec![
                        f.command.clone().into_os_string(),
                        "bank".into(),
                        "recover-layout".into(),
                        "recover-nested".into(),
                        "--repo".into(),
                        f.repo.clone().into_os_string(),
                        "--proposal".into(),
                        input,
                    ],
                    RunOptions {
                        check: false,
                        env: &env,
                        ..Default::default()
                    },
                )
                .unwrap();
            let ready =
                rendezvous.join("layout-recovery-admission-receipt-before-resolution.ready");
            let deadline = agent_memory::clock::Deadline::from_seconds(10.0).unwrap();
            while !ready.exists() && !deadline.expired() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(ready.exists());
            let competing = f
                .world
                .spawn(vec![
                    f.command.clone().into_os_string(),
                    "bank".into(),
                    "recover-layout".into(),
                    "recover-nested".into(),
                    "--repo".into(),
                    f.repo.clone().into_os_string(),
                    "--operation".into(),
                    f.operation.clone().into(),
                    "--nested".into(),
                    nested.clone().into(),
                    "--mode".into(),
                    "finish".into(),
                    "--output".into(),
                    f.world.inside("competing.json").unwrap().into_os_string(),
                ])
                .unwrap();
            assert_eq!(f.snapshot(), bank_before);
            fs::write(
                rendezvous.join("layout-recovery-admission-receipt-before-resolution.release"),
                b"",
            )
            .unwrap();
            assert_eq!(
                child.wait().unwrap().status,
                agent_memory::process::Status::Code(0)
            );
            let competing = competing.wait().unwrap();
            assert!(matches!(
                competing.status,
                agent_memory::process::Status::Code(0 | 1)
            ));
        } else {
            f.recover(&preview, 0);
        }
        assert_eq!(f.snapshot(), bank_before);
        assert!(!f.bank.join(".agent-memory-transaction.md").exists());
        assert!(!f.bank.join(".agent-memory-recovery.md").exists());
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(g(&status, "semantic_recovery_complete"), &Value::Bool(true));
        let ledger = json::decode_bytes(
            &fs::read(f.directory.join("ledger/00000001.json")).unwrap(),
            Default::default(),
        )
        .unwrap();
        let unit = array(g(&ledger, "units"))
            .iter()
            .find(|u| g(u, "state") == &s("admitted"))
            .unwrap();
        let links = array(g(unit, "terminal_receipts"));
        assert_eq!(links.len(), 1);
        let receipt = json::decode_bytes(
            &fs::read(
                f.directory
                    .join("resolutions")
                    .join(format!("{}.json", t(&links[0], "resolution_id"))),
            )
            .unwrap(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(g(&receipt, "canonical_operation_id"), &s(&nested));
        assert_eq!(g(&links[0], "sha256"), &s(sha(&receipt)));
        for (name, bytes) in originals
            .iter()
            .filter(|(name, _)| name.starts_with("originals/"))
        {
            assert_eq!(fs::read(f.directory.join(name)).unwrap(), *bytes);
        }
    }
}
// obligation: tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_interrupted_pre_receipt_rollback_resumes_ordered_plan_not_forward_prefix
#[test]
#[ignore = "needs recovery_frontier registered"]
fn interrupted_pre_receipt_rollback_resumes_ordered_plan_not_forward_prefix() {
    for mode in ["error", "exit"] {
        let f = Fixture::new();
        let hold = fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap();
        let sentinel = fs::read(f.bank.join(".agent-memory-recovery.md")).unwrap();
        let base = f.snapshot();
        let (p, nested, target) = f.admission(false);
        f.fault_admission(&p, "layout-recovery-nested-before-receipt", "error");
        f.layout(
            "recover-nested",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--nested".into(),
                nested.clone().into(),
                "--mode".into(),
                "finish".into(),
                "--output".into(),
                f.world
                    .inside("refused-finish.json")
                    .unwrap()
                    .into_os_string(),
            ],
            1,
        );
        let preview = f.nested_preview(&nested, "rollback");
        let event = format!("history/events/{nested}.md");
        let stage = if mode == "error" {
            format!("transaction-before-recheck:{target}")
        } else {
            format!("layout-recovery-nested-rollback-after-rename:{event}")
        };
        let (code, _) = f.call(
            vec![
                "bank".into(),
                "recover-layout".into(),
                "recover-nested".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
                "--proposal".into(),
                f.save(&preview),
            ],
            Some((&stage, mode)),
        );
        assert_eq!(code, if mode == "exit" { 86 } else { 1 });
        if mode == "exit" {
            f.unlock();
        }
        let nested_dir = f.directory.join("nested").join(&nested);
        let plan = fs::read(nested_dir.join("rollback-plan.json")).unwrap();
        assert!(!f.bank.join(&event).exists());
        assert!(f.bank.join(&target).exists());
        let interrupted = f.snapshot();
        f.recover(&preview, 1);
        assert_eq!(f.snapshot(), interrupted);
        let resume = f.nested_preview(&nested, "rollback");
        let context = f.bank.join("projectbrief.md");
        let before = fs::read(&context).unwrap();
        let mut changed = before.clone();
        changed.extend_from_slice(b"\nExternal unrelated edit\n");
        fs::write(&context, &changed).unwrap();
        f.recover(&resume, 1);
        assert_eq!(fs::read(&context).unwrap(), changed);
        fs::write(&context, before).unwrap();
        f.recover(&resume, 0);
        assert_eq!(
            fs::read(nested_dir.join("rollback-plan.json")).unwrap(),
            plan
        );
        assert_eq!(f.snapshot(), base);
        assert_eq!(
            fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
            hold
        );
        assert_eq!(
            fs::read(f.bank.join(".agent-memory-recovery.md")).unwrap(),
            sentinel
        );
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(
            g(&status, "semantic_recovery_complete"),
            &Value::Bool(false)
        );
        let count = fs::read_dir(nested_dir.join("rollback-progress"))
            .unwrap()
            .count();
        assert_eq!(count, if mode == "error" { 3 } else { 2 });
    }
}
// obligation: tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_nested_retirement_resumes_active_binding_after_terminal_index
#[test]
#[ignore = "needs recovery_frontier registered"]
fn nested_retirement_resumes_active_binding_after_terminal_index() {
    for mode in ["error", "exit"] {
        let f = Fixture::new();
        let (p, nested, _) = f.admission(true);
        f.fault_admission(&p, "layout-recovery-retirement-after-index", mode);
        let before = f.snapshot();
        let index = json::decode_bytes(
            &fs::read(f.directory.parent().unwrap().join("index.json")).unwrap(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            g(g(g(&index, "operations"), &f.operation), "state"),
            &s("terminal")
        );
        let preview = f.nested_preview(&nested, "finish");
        f.recover(&preview, 0);
        assert_eq!(f.snapshot(), before);
        assert!(!f.bank.join(".agent-memory-transaction.md").exists());
        assert!(!f.bank.join(".agent-memory-recovery.md").exists());
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(g(&status, "semantic_recovery_complete"), &Value::Bool(true));
    }
}
fn exact_retirement(stage: &str) {
    for mode in ["error", "exit"] {
        let f = Fixture::new();
        let (p, nested, target) = f.admission(true);
        f.fault_admission(&p, stage, mode);
        let before = f.snapshot();
        let private_before = snapshot(&f.directory, false);
        let index = fs::read(f.directory.parent().unwrap().join("index.json")).unwrap();
        let safeguards = [".agent-memory-transaction.md", ".agent-memory-recovery.md"]
            .into_iter()
            .map(|name| (name, fs::read(f.bank.join(name)).ok()))
            .collect::<Vec<_>>();
        let completion = f
            .directory
            .join("nested")
            .join(&nested)
            .join("completion.json");
        let bytes = fs::read(&completion).unwrap();
        let mut damaged = json::decode_bytes(&bytes, Default::default()).unwrap();
        set(&mut damaged, "canonical_receipt_sha256", s("0".repeat(64)));
        private(&completion, &damaged);
        f.layout(
            "recover-nested",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--nested".into(),
                nested.clone().into(),
                "--mode".into(),
                "finish".into(),
                "--output".into(),
                f.world
                    .inside("bad-completion.json")
                    .unwrap()
                    .into_os_string(),
            ],
            1,
        );
        fs::write(&completion, &bytes).unwrap();
        f.layout(
            "recover-nested",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--nested".into(),
                nested.clone().into(),
                "--mode".into(),
                "rollback".into(),
                "--output".into(),
                f.world
                    .inside("bad-rollback.json")
                    .unwrap()
                    .into_os_string(),
            ],
            1,
        );
        let preview = f.nested_preview(&nested, "finish");
        assert_eq!(snapshot(&f.directory, false), private_before);
        assert_eq!(
            fs::read(f.directory.parent().unwrap().join("index.json")).unwrap(),
            index
        );
        for (name, original) in &safeguards {
            let foreign = encoded(&obj([
                ("schema", s("agent-memory/layout-frontier-transaction-v1")),
                ("project_id", s(&f.project)),
                ("operation_id", s(&f.operation)),
                ("hold_id", s(uuid())),
                ("nested_id", s(uuid())),
            ]))
            .unwrap();
            fs::write(f.bank.join(name), &foreign).unwrap();
            f.recover(&preview, 1);
            assert_eq!(fs::read(f.bank.join(name)).unwrap(), foreign);
            assert_eq!(snapshot(&f.directory, false), private_before);
            assert_eq!(
                fs::read(f.directory.parent().unwrap().join("index.json")).unwrap(),
                index
            );
            match original {
                Some(bytes) => fs::write(f.bank.join(name), bytes).unwrap(),
                None => fs::remove_file(f.bank.join(name)).unwrap(),
            }
        }
        let mut changed = before[&target].clone();
        changed.extend_from_slice(b"\nExternal replacement must remain untouched.\n");
        fs::write(f.bank.join(&target), &changed).unwrap();
        f.layout(
            "recover-nested",
            vec![
                "--operation".into(),
                f.operation.clone().into(),
                "--nested".into(),
                nested.clone().into(),
                "--mode".into(),
                "finish".into(),
                "--output".into(),
                f.world
                    .inside("external-refused.json")
                    .unwrap()
                    .into_os_string(),
            ],
            1,
        );
        f.recover(&preview, 1);
        assert_eq!(fs::read(f.bank.join(&target)).unwrap(), changed);
        assert_eq!(snapshot(&f.directory, false), private_before);
        fs::write(f.bank.join(&target), &before[&target]).unwrap();
        f.recover(&preview, 0);
        assert_eq!(f.snapshot(), before);
        assert_eq!(snapshot(&f.directory, false), private_before);
        assert!(!f.bank.join(".agent-memory-transaction.md").exists());
        assert!(!f.bank.join(".agent-memory-recovery.md").exists());
        let status = f.layout(
            "status",
            vec!["--operation".into(), f.operation.clone().into()],
            0,
        );
        assert_eq!(g(&status, "semantic_recovery_complete"), &Value::Bool(true));
    }
}
// obligation: tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_exact_nested_finish_resumes_sentinel_retirement_without_canonical_replay
#[test]
#[ignore = "needs recovery_frontier registered"]
fn exact_nested_finish_resumes_sentinel_retirement_without_canonical_replay() {
    exact_retirement("layout-recovery-sentinel-retirement");
}
// obligation: tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_exact_nested_finish_resumes_after_marker_removal_without_canonical_replay
#[test]
#[ignore = "needs recovery_frontier registered"]
fn exact_nested_finish_resumes_after_marker_removal_without_canonical_replay() {
    exact_retirement("layout-recovery-retirement-after-marker-removal");
}
// obligation: tests/contracts/test_layout_nested_reconciliation.py::NestedReconciliationContract.test_exact_nested_finish_resumes_after_sentinel_removal_without_canonical_replay
#[test]
#[ignore = "needs recovery_frontier registered"]
fn exact_nested_finish_resumes_after_sentinel_removal_without_canonical_replay() {
    exact_retirement("layout-recovery-retirement-after-sentinel-removal");
}
fn checkpoint_fault(after_receipt: bool, mode: &str) {
    let f = Fixture::with_checkpoint(true);
    let feature = f.repo.join("feature.txt");
    fs::write(&feature, "Feature baseline\n").unwrap();
    f.world.git(["add", "feature.txt"]).unwrap();
    f.world.git(["commit", "-m", "Feature baseline"]).unwrap();
    fs::write(&feature, "Staged feature\n").unwrap();
    f.world.git(["add", "feature.txt"]).unwrap();
    fs::write(&feature, "Unstaged feature\n").unwrap();
    let index = fs::read(f.repo.join(".git/index")).unwrap();
    let head = f.world.git(["rev-parse", "HEAD"]).unwrap().stdout;
    let originals = snapshot(&f.directory, false);
    let hold = fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap();
    let sentinel = fs::read(f.bank.join(".agent-memory-recovery.md")).unwrap();
    let before = f.snapshot();
    let modes = before
        .keys()
        .map(|path| {
            (
                path.clone(),
                fs::metadata(f.bank.join(path))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let current = f.good(vec![
        "checkpoint".into(),
        "inspect".into(),
        "--repo".into(),
        f.repo.clone().into_os_string(),
    ]);
    let payload = obj([
        (
            "focus",
            s("Verify the retry lesson against the current branch; [task](../task.md)."),
        ),
        (
            "recent_changes",
            Value::Array(vec![s(
                "The isolated retry result was inspected; preserve its conditions and evidence.",
            )]),
        ),
        (
            "open_questions",
            Value::Array(vec![s(
                "Applicability outside this component remains unknown.",
            )]),
        ),
        (
            "next_step",
            s("Inspect the second clone before broadening the lesson."),
        ),
        (
            "evidence",
            Value::Array(vec![s("../task.md"), s("../retry-result.md")]),
        ),
        (
            "conditions",
            obj([("branch", s("main")), ("component", s("src/retry"))]),
        ),
        ("harness", s("custom")),
        ("session", s("session-1")),
        (
            "observed_active_sha256",
            g(&current, "observed_active_sha256").clone(),
        ),
    ]);
    let p = f.good(vec![
        "checkpoint".into(),
        "preview".into(),
        "--repo".into(),
        f.repo.clone().into_os_string(),
        "--input".into(),
        f.save(&payload),
    ]);
    let nested = t(g(g(&p, "proposal"), "write"), "operation_id");
    let stage = if after_receipt {
        "layout-recovery-nested-after-receipt"
    } else {
        "layout-recovery-nested-before-receipt"
    };
    let (code, value) = f.call(
        vec![
            "checkpoint".into(),
            "apply".into(),
            "--proposal".into(),
            f.save(&p),
        ],
        Some((stage, mode)),
    );
    assert_eq!(code, if mode == "exit" { 86 } else { 1 }, "{value:?}");
    if mode == "exit" {
        f.unlock();
    }
    let journal = f.directory.join("nested").join(nested).join("journal.json");
    let journal_bytes = fs::read(&journal).unwrap();
    let changed = f.snapshot();
    assert_ne!(changed["activeContext.md"], before["activeContext.md"]);
    let wanted = if after_receipt { "finish" } else { "rollback" };
    let wrong = if after_receipt { "rollback" } else { "finish" };
    f.layout(
        "recover-nested",
        vec![
            "--operation".into(),
            f.operation.clone().into(),
            "--nested".into(),
            nested.into(),
            "--mode".into(),
            wrong.into(),
            "--output".into(),
            f.world
                .inside("checkpoint-refused.json")
                .unwrap()
                .into_os_string(),
        ],
        1,
    );
    let preview = f.nested_preview(nested, wanted);
    f.recover(&preview, 0);
    assert_eq!(f.snapshot(), if after_receipt { changed } else { before });
    if after_receipt {
        let historical = f.good(vec![
            "checkpoint".into(),
            "apply".into(),
            "--proposal".into(),
            f.save(&p),
        ]);
        assert_eq!(
            g(g(&historical, "write"), "already_applied"),
            &Value::Bool(true)
        );
        assert_eq!(g(&historical, "writes_performed"), &Value::Bool(false));
    }
    assert_eq!(fs::read(journal).unwrap(), journal_bytes);
    assert_eq!(
        fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
        hold
    );
    assert_eq!(
        fs::read(f.bank.join(".agent-memory-recovery.md")).unwrap(),
        sentinel
    );
    for (path, mode) in modes {
        assert_eq!(
            fs::metadata(f.bank.join(path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            mode
        );
    }
    for (path, bytes) in originals
        .iter()
        .filter(|(name, _)| name.starts_with("originals/"))
    {
        assert_eq!(fs::read(f.directory.join(path)).unwrap(), *bytes);
    }
    let status = f.layout(
        "status",
        vec!["--operation".into(), f.operation.clone().into()],
        0,
    );
    assert_eq!(
        g(&status, "semantic_recovery_complete"),
        &Value::Bool(false)
    );
    assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), index);
    assert_eq!(f.world.git(["rev-parse", "HEAD"]).unwrap().stdout, head);
    assert_eq!(fs::read_to_string(feature).unwrap(), "Unstaged feature\n");
}
// obligation: tests/contracts/test_layout_checkpoint_faults.py::LayoutCheckpointFaults.test_before_receipt_error
#[test]
#[ignore = "needs recovery_frontier registered"]
fn checkpoint_before_receipt_error() {
    checkpoint_fault(false, "error");
}
// obligation: tests/contracts/test_layout_checkpoint_faults.py::LayoutCheckpointFaults.test_before_receipt_exit
#[test]
#[ignore = "needs recovery_frontier registered"]
fn checkpoint_before_receipt_exit() {
    checkpoint_fault(false, "exit");
}
// obligation: tests/contracts/test_layout_checkpoint_faults.py::LayoutCheckpointFaults.test_after_receipt_error
#[test]
#[ignore = "needs recovery_frontier registered"]
fn checkpoint_after_receipt_error() {
    checkpoint_fault(true, "error");
}
// obligation: tests/contracts/test_layout_checkpoint_faults.py::LayoutCheckpointFaults.test_after_receipt_exit
#[test]
#[ignore = "needs recovery_frontier registered"]
fn checkpoint_after_receipt_exit() {
    checkpoint_fault(true, "exit");
}
