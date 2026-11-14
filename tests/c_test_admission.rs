mod support;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::{Output, Status};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use support::cutover::*;
use support::{RunOptions, World};

struct Admission {
    recall: Recall,
    actor: String,
}
impl Admission {
    fn new() -> Self {
        let recall = Recall::new();
        fs::write(
            recall.repo.join("evidence.md"),
            "# Observed result\n\nA disposable measured fixture, not actual project admission.\n",
        )
        .unwrap();
        let status = good(&recall.world, "am developer", &["status".into()]);
        let actor = string(&status, "developer_id").to_owned();
        Self { recall, actor }
    }
    fn request(&self, source: &str, category: &str) -> Value {
        let mut candidate = json::decode(
            include_str!("../skills/agent-memory/templates/record-proposal.example.json"),
            Default::default(),
        )
        .unwrap();
        let metadata = candidate.get_mut("metadata").unwrap();
        for (key, value) in [
            ("project-id", text(PROJECT)),
            ("scope", text("repository")),
            ("conditions", object(Vec::<(String, Value)>::new())),
            (
                "source",
                object([
                    ("kind", text(source)),
                    ("actor", text("Explicit fixture source; not authenticated")),
                ]),
            ),
        ] {
            put(metadata, key, value);
        }
        let origin = metadata.get_mut("origin").unwrap();
        put(origin, "developer-id", text(&self.actor));
        put(origin, "code-state", text("not-code"));
        object([
            ("project_id", text(PROJECT)),
            ("candidate", candidate),
            ("source_class", text(source)),
            ("repository_evidence", Value::Null),
            (
                "assessment",
                object([
                    ("category", text(category)),
                    (
                        "utility",
                        text("Prevent repeated excessive retries under these conditions."),
                    ),
                    ("durable", Value::Bool(true)),
                    ("evidence_confidence", text("confirmed")),
                    (
                        "evidence_reason",
                        text("The operator explicitly confirmed the correction."),
                    ),
                    ("confirmed", Value::Bool(true)),
                    ("source_redundant", Value::Bool(false)),
                    ("work_state_copy", Value::Bool(false)),
                    ("review", Value::Null),
                ]),
            ),
        ])
    }
    fn evidence_request(&self, source: &str, category: &str) -> Value {
        let mut request = self.request(source, category);
        let sha = agent_memory::developer_identity::digest(Some(
            &fs::read(self.recall.repo.join("evidence.md")).unwrap(),
        ))
        .unwrap();
        put(
            &mut request,
            "repository_evidence",
            object([("path", text("evidence.md")), ("sha256", text(sha))]),
        );
        put(
            request.get_mut("assessment").unwrap(),
            "review",
            text("Reviewed the actual supporting result and its limitations."),
        );
        request
    }
    fn capture(&self, request: &Value) -> Value {
        good(
            &self.recall.world,
            "am capture",
            &[
                "--repo".into(),
                self.recall.repo.as_os_str().to_owned(),
                "--request".into(),
                save(&self.recall.world, request, "capture-request.json").into_os_string(),
            ],
        )
    }
    fn preview(&self, request: &Value, classification: &str) -> Value {
        let captured = self.capture(request);
        assert_eq!(string(&captured, "decision"), "eligible");
        let compared = object([
            ("project_id", text(PROJECT)),
            ("candidate", get(request, "candidate").clone()),
            (
                "searches",
                Value::Array(vec![text("retry policy"), text("attempt cap")]),
            ),
            ("read_paths", Value::Array(vec![])),
        ]);
        let w = &self.recall.world;
        let input = save(w, &compared, "comparison-request.json");
        let inspected = good(
            w,
            "am compare",
            &[
                "inspect".into(),
                "--repo".into(),
                self.recall.repo.as_os_str().to_owned(),
                "--request".into(),
                input.as_os_str().to_owned(),
            ],
        );
        let decisions = get(&inspected,"matches").as_array().unwrap().iter().map(|row| object([("path",get(row,"path").clone()),("classification",text(classification)),("rationale",text("The fixture meaning has been assessed with its evidence and conditions."))])).collect();
        let judgment = object([
            ("observation", get(&inspected, "observation").clone()),
            ("decisions", Value::Array(decisions)),
            ("pending_decisions", Value::Array(vec![])),
            (
                "rationale",
                text("Read the actual bounded plausible sources before deciding novelty."),
            ),
        ]);
        let reviewed = good(
            w,
            "am compare",
            &[
                "review".into(),
                "--repo".into(),
                self.recall.repo.as_os_str().to_owned(),
                "--request".into(),
                input.as_os_str().to_owned(),
                "--judgment".into(),
                save(w, &judgment, "judgment.json").into_os_string(),
            ],
        );
        good(
            w,
            "am admission",
            &[
                "preview".into(),
                "--repo".into(),
                self.recall.repo.as_os_str().to_owned(),
                "--capture".into(),
                save(w, &captured, "capture.json").into_os_string(),
                "--request".into(),
                input.into_os_string(),
                "--proof".into(),
                save(w, &reviewed, "proof.json").into_os_string(),
            ],
        )
    }
    fn prepared(&self, request: &Value) -> Value {
        get(&self.preview(request, "distinct-knowledge"), "proposal").clone()
    }
    fn apply(&self, p: &Value, env: &[(OsString, OsString)]) -> (Output, Value) {
        command_fault(
            &self.recall.world,
            "am admission",
            &[
                "apply".into(),
                "--proposal".into(),
                save(&self.recall.world, p, "admission.json").into_os_string(),
            ],
            env,
        )
    }
    fn crash(&self, p: &Value, stage: &str) {
        let env = [
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "exit".into()),
        ];
        assert_eq!(self.apply(p, &env).0.status, Status::Code(86));
    }
    fn unlock(&self) {
        let w = &self.recall.world;
        let p = good(
            w,
            "am writes",
            &[
                "recover".into(),
                "--repo".into(),
                self.recall.repo.as_os_str().to_owned(),
            ],
        );
        let receipt = good(
            w,
            "am writes",
            &[
                "recover".into(),
                "--repo".into(),
                self.recall.repo.as_os_str().to_owned(),
                "--proposal".into(),
                save(w, &p, "unlock.json").into_os_string(),
            ],
        );
        assert!(yes(&receipt, "verified"));
    }
    fn recovery(&self, mode: &str) -> (Output, Value) {
        command(
            &self.recall.world,
            "am transaction",
            &[
                "recover".into(),
                "--repo".into(),
                self.recall.repo.as_os_str().to_owned(),
                "--mode".into(),
                mode.into(),
            ],
        )
    }
    fn recover(&self, mode: &str) -> Value {
        let (output, p) = self.recovery(mode);
        assert_eq!(output.status, Status::Code(0));
        good(
            &self.recall.world,
            "am transaction",
            &[
                "recover".into(),
                "--repo".into(),
                self.recall.repo.as_os_str().to_owned(),
                "--proposal".into(),
                save(&self.recall.world, &p, "recovery.json").into_os_string(),
            ],
        )
    }
    fn state(&self) -> (Value, (Vec<u8>, Vec<u8>, Vec<u8>)) {
        (canonical(&self.recall.bank), self.recall.code())
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_repository_and_reviewed_tool_evidence_are_bound_to_actual_bytes_and_preserved
fn reviewed_evidence_is_bound_to_actual_bytes_and_preserved() {
    for (source, category) in [
        ("verified-repository", "durable-discovery"),
        ("tool", "verified-fix"),
        ("assistant", "failed-approach"),
        ("external", "confirmed-decision"),
    ] {
        let a = Admission::new();
        let request = a.evidence_request(source, category);
        let sha = string(get(&request, "repository_evidence"), "sha256").to_owned();
        let p = a.prepared(&request);
        let (output, done) = a.apply(&p, &[]);
        assert_eq!(output.status, Status::Code(0));
        let id = string(get(get(&request, "candidate"), "metadata"), "id");
        let (_, metadata, _) = agent_memory::records::parse(
            &fs::read_to_string(a.recall.bank.join(format!("topics/{id}.md"))).unwrap(),
            &[],
        )
        .unwrap();
        assert!(
            get(&metadata, "evidence")
                .as_array()
                .unwrap()
                .contains(&text(format!("repo:evidence.md#sha256={sha}")))
        );
        assert_eq!(string(get(&metadata, "source"), "kind"), source);
        assert!(
            string(&metadata, "admission-reason")
                .contains(string(get(&request, "assessment"), "review"))
        );
        assert!(!yes(&done, "source_assertions_verified"));
        assert!(!yes(&done, "semantic_truth_verified"));
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_arrived_learning_tampered_delta_and_changed_evidence_refuse_before_writing
fn arrived_learning_tampered_delta_and_changed_evidence_refuse_before_writing() {
    for change in ["arrived", "tampered", "source"] {
        let a = Admission::new();
        let request = a.evidence_request("verified-repository", "user-correction");
        let mut p = a.prepared(&request);
        match change {
            "arrived" => {
                a.recall.add(
                    "arrived",
                    "# Retry policy\n\nA verified retry probe needs its recorded conditions.\n",
                    &object(Vec::<(String, Value)>::new()),
                );
            }
            "tampered" => {
                let files = p.get_mut("files").unwrap();
                put(
                    files,
                    "projectbrief.md",
                    text(format!(
                        "{}\nAn unrelated unauthorized rewrite.\n",
                        string(files, "projectbrief.md")
                    )),
                );
            }
            _ => fs::write(a.recall.repo.join("evidence.md"), "# Changed evidence\n").unwrap(),
        }
        let before = a.state();
        let (output, value) = a.apply(&p, &[]);
        assert_eq!(output.status, Status::Code(1));
        assert!(!yes(&value, "admission_verified"));
        assert_eq!(a.state(), before);
    }
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_duplicate_learning_defers_to_existing_identity_and_peer_conflict_to_detecting_developer
fn duplicate_defers_to_existing_identity_and_peer_conflict_to_detector() {
    let a = Admission::new();
    let other = "66666666-6666-4666-8666-666666666666";
    let path = a.recall.add(
        "existing",
        "# Attempt cap\n\nA verified retry probe needs its recorded conditions.\n",
        &object(Vec::<(String, Value)>::new()),
    );
    let before = a.recall.code();
    let preview = a.preview(&a.request("user", "user-correction"), "equivalent");
    assert_eq!(string(&preview, "disposition"), "update-existing");
    assert_eq!(get(&preview, "proposal"), &Value::Null);
    let raw = fs::read_to_string(a.recall.bank.join(&path)).unwrap();
    let (_, mut metadata, _) = agent_memory::records::parse(&raw, &[]).unwrap();
    put(
        metadata.get_mut("origin").unwrap(),
        "developer-id",
        text(other),
    );
    let content = agent_memory::codec::frontmatter::write(metadata.as_object().unwrap()).unwrap()
        + "# Attempt cap\n\nThe required cap is five.\n";
    fs::write(a.recall.bank.join(path), content).unwrap();
    let preview = a.preview(&a.request("user", "user-correction"), "contradiction");
    assert_eq!(string(&preview, "disposition"), "clarification-required");
    assert_eq!(string(&preview, "detecting_developer_id"), a.actor);
    assert_eq!(
        get(&preview, "other_developer_ids"),
        &Value::Array(vec![text(other)])
    );
    assert!(!yes(&preview, "messages_sent"));
    assert_eq!(a.recall.code(), before);
    assert!(!a.recall.bank.join("history").exists());
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_terminal_deleted_identity_and_derivations_are_quarantined_not_recaptured
fn terminal_identity_and_derivations_are_quarantined_not_recaptured() {
    let a = Admission::new();
    let path = a.recall.add(
        "old",
        "# Retry lesson\n\nRetain its recorded conditions.\n",
        &object(Vec::<(String, Value)>::new()),
    );
    let (_, metadata, _) =
        agent_memory::records::parse(&fs::read_to_string(a.recall.bank.join(&path)).unwrap(), &[])
            .unwrap();
    let id = string(&metadata, "id");
    let tomb = format!("history/tombstones/{id}.md");
    let tombstone = object([
        ("id", text(id)),
        ("scope", text(PROJECT)),
        ("deleted-at", text("2026-10-22T20:00:00Z")),
        ("deletion-authority", text(&a.actor)),
        ("reason-class", text("user-request")),
    ]);
    fs::create_dir_all(a.recall.bank.join("history/tombstones")).unwrap();
    fs::write(
        a.recall.bank.join(&tomb),
        agent_memory::codec::frontmatter::write(tombstone.as_object().unwrap()).unwrap(),
    )
    .unwrap();
    fs::remove_file(a.recall.bank.join(&path)).unwrap();
    let router = a.recall.bank.join("MEMORY.md");
    let raw = fs::read_to_string(&router).unwrap();
    let raw:String = raw.split_inclusive('\n').map(|line| if line.contains(&format!("[{path}]")) { format!("| [{tomb}](<{tomb}>) | Terminal marker | repository | deleted | Before any reuse |\n") } else {line.to_owned()}).collect();
    fs::write(router, raw).unwrap();
    let before = a.state();
    for (field, value) in [
        ("id", text(id)),
        (
            "derived-from",
            Value::Array(vec![object([
                ("id", text(id)),
                ("version", Value::Int(1.into())),
            ])]),
        ),
    ] {
        let mut request = a.request("user", "user-correction");
        put(
            request
                .get_mut("candidate")
                .unwrap()
                .get_mut("metadata")
                .unwrap(),
            field,
            value,
        );
        let result = a.capture(&request);
        assert_eq!(string(&result, "decision"), "quarantine");
        assert_eq!(get(&result, "capture"), &Value::Null);
        assert!(
            get(&result, "reasons")
                .as_array()
                .unwrap()
                .contains(&text("terminal-deletion"))
        );
    }
    assert_eq!(a.state(), before);
}

#[test]
#[ignore = "needs recovery_frontier and am transaction route registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_interrupted_admission_rolls_back_before_fresh_comparison_even_if_source_changed
fn interrupted_admission_rolls_back_before_new_comparison_even_if_source_changed() {
    let a = Admission::new();
    let before = a.state();
    let request = a.evidence_request("verified-repository", "user-correction");
    let p = a.prepared(&request);
    a.crash(
        &p,
        &format!(
            "transaction-after-write:history/events/{}.md",
            string(&p, "operation_id")
        ),
    );
    assert!(a.recall.bank.join(MARKER).exists());
    a.unlock();
    assert_eq!(a.recovery("finish").0.status, Status::Code(1));
    fs::write(
        a.recall.repo.join("evidence.md"),
        "# Evidence changed while interrupted\n",
    )
    .unwrap();
    let done = a.recover("rollback");
    assert!(yes(&done, "verified"));
    assert_eq!(string(&done, "outcome"), "rolled-back");
    assert_eq!(a.state(), before);
    assert!(!a.recall.bank.join(MARKER).exists());
}

#[test]
#[ignore = "needs recovery_frontier and am transaction route registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_recorded_admission_recovers_lost_response_without_replaying_or_requiring_new_evidence
fn recorded_admission_recovers_lost_response_without_requiring_new_evidence() {
    for stage in [
        "transaction-after-receipt",
        "transaction-after-marker-removal",
    ] {
        for changed in [false, true] {
            let a = Admission::new();
            let request = a.evidence_request("verified-repository", "user-correction");
            let p = a.prepared(&request);
            a.crash(&p, stage);
            let marker = a.recall.bank.join(MARKER);
            assert_eq!(marker.exists(), stage == "transaction-after-receipt");
            if changed {
                fs::write(
                    a.recall.repo.join("evidence.md"),
                    "# Later evidence; do not change the recorded historical outcome\n",
                )
                .unwrap();
            }
            a.unlock();
            let before = a.state();
            let done = if marker.exists() {
                assert_eq!(a.recovery("rollback").0.status, Status::Code(1));
                let done = a.recover("finish");
                assert_eq!(string(&done, "outcome"), "finished");
                assert!(!yes(&done, "writes_performed"));
                done
            } else {
                let (output, done) = a.apply(&p, &[]);
                assert_eq!(output.status, Status::Code(0));
                assert!(yes(&done, "already_applied"));
                done
            };
            assert!(yes(&done, "verified"));
            assert!(!marker.exists());
            assert_eq!(a.state(), before);
            good(
                &a.recall.world,
                "am bank",
                &[
                    "validate".into(),
                    "--repo".into(),
                    a.recall.repo.into_os_string(),
                ],
            );
        }
    }
}

#[test]
#[ignore = "needs recovery_frontier and am transaction route registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_completed_receipt_cleanup_does_not_trust_corrupt_receipts_or_overwrite_external_memory
fn completed_receipt_cleanup_rejects_corruption_and_preserves_external_memory() {
    let a = Admission::new();
    let p = a.prepared(&a.request("user", "user-correction"));
    a.crash(&p, "transaction-after-receipt");
    a.unlock();
    let root = a.recall.world.paths["state"].clone();
    let receipts: Vec<_> = physical(&root)
        .into_iter()
        .filter_map(|(name, raw)| {
            let value = json::decode_bytes(&raw, Default::default()).ok()?;
            (get(&value, "schema").as_str() == Some("agent-memory/transaction-receipt-v1")
                && get(&value, "operation_id") == p.get("operation_id").unwrap())
            .then_some((root.join(name), raw))
        })
        .collect();
    assert_eq!(receipts.len(), 1);
    let (path, raw) = &receipts[0];
    let mut value = json::decode_bytes(raw, Default::default()).unwrap();
    put(&mut value, "verified", Value::Bool(false));
    fs::write(path, json::encode(&value, json::DEFAULT).unwrap()).unwrap();
    let before = a.state();
    assert_eq!(a.recovery("finish").0.status, Status::Code(1));
    assert_eq!(a.state(), before);
    fs::write(path, raw).unwrap();
    let target = a.recall.bank.join("projectbrief.md");
    fs::write(
        &target,
        fs::read_to_string(&target).unwrap() + "\nA foreign edit must be preserved.\n",
    )
    .unwrap();
    let before = a.state();
    assert_eq!(a.recovery("finish").0.status, Status::Code(1));
    assert_eq!(a.state(), before);
    assert!(a.recall.bank.join(MARKER).exists());
}

#[test]
#[ignore = "needs recovery_frontier and am transaction route registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_large_existing_inventory_admits_or_finishes_recovery_without_losing_prior_learning
fn large_existing_inventory_admits_or_finishes_without_losing_prior_learning() {
    for interrupted in [false, true] {
        let a = Admission::new();
        let mut existing = Vec::new();
        for n in 0..48 {
            let path = a.recall.add(
                &format!("archive-storage-evidence-{n}"),
                &format!("# Archive storage\n\nRetain two verified copies for partition {n}.\n"),
                &object(Vec::<(String, Value)>::new()),
            );
            existing.push((path.clone(), fs::read(a.recall.bank.join(path)).unwrap()));
        }
        let before = a.recall.code();
        let request = a.request("user", "user-correction");
        let p = a.prepared(&request);
        let admitted = if interrupted {
            a.crash(&p, "transaction-after-receipt");
            assert!(fs::metadata(a.recall.bank.join(MARKER)).unwrap().len() > 4096);
            a.unlock();
            let recovered = a.recover("finish");
            assert_eq!(string(&recovered, "outcome"), "finished");
            assert!(yes(&recovered, "verified"));
            let (output, replay) = a.apply(&p, &[]);
            assert_eq!(output.status, Status::Code(0));
            assert!(yes(&replay, "already_applied"));
            assert!(!yes(&replay, "writes_performed"));
            replay
        } else {
            let (output, done) = a.apply(&p, &[]);
            assert_eq!(output.status, Status::Code(0));
            done
        };
        assert!(yes(&admitted, "admission_verified"));
        let expected = get(get(&request, "candidate"), "metadata");
        let id = string(expected, "id");
        let (_, metadata, body) = agent_memory::records::parse(
            &fs::read_to_string(a.recall.bank.join(format!("topics/{id}.md"))).unwrap(),
            &[],
        )
        .unwrap();
        assert_eq!(get(&metadata, "origin"), get(expected, "origin"));
        assert_eq!(&text(body), get(get(&request, "candidate"), "body"));
        for (path, raw) in existing {
            assert_eq!(fs::read(a.recall.bank.join(path)).unwrap(), raw);
        }
        assert_eq!(a.recall.code(), before);
        assert!(!a.recall.bank.join(MARKER).exists());
    }
}

#[test]
#[ignore = "needs recovery_frontier and am transaction route registered"]
// obligation: tests/contracts/test_admission.py::AdmissionContract.test_evidence_change_at_final_readback_cannot_get_a_verified_admission_receipt
fn final_readback_evidence_change_never_gets_verified_admission_receipt() {
    let a = Admission::new();
    let before = a.state();
    let request = a.evidence_request("verified-repository", "user-correction");
    let p = a.prepared(&request);
    let w: &World = &a.recall.world;
    let pause: PathBuf = w.inside("admission-rendezvous").unwrap();
    let stage = "transaction-before-receipt";
    let env = [
        ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
        ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
        (
            "AGENT_MEMORY_FAULT_DIR".into(),
            pause.as_os_str().to_owned(),
        ),
    ];
    let child = w
        .spawn_with(
            [
                OsString::from(env!("CARGO_BIN_EXE_am")),
                "--json".into(),
                "admission".into(),
                "apply".into(),
                "--proposal".into(),
                save(w, &p, "paused-admission.json").into_os_string(),
            ],
            RunOptions {
                check: false,
                env: &env,
                ..RunOptions::default()
            },
        )
        .unwrap();
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(8)).unwrap();
    while !pause.join(format!("{stage}.ready")).exists() {
        assert!(!deadline.expired());
        std::thread::sleep(Duration::from_millis(5));
    }
    fs::write(
        a.recall.repo.join("evidence.md"),
        "# Changed during the final source readback\n",
    )
    .unwrap();
    fs::write(pause.join(format!("{stage}.release")), b"").unwrap();
    let output = child.wait().unwrap();
    assert_eq!(output.status, Status::Code(1));
    let value = json::decode_bytes(&output.stdout, Default::default()).unwrap();
    assert!(!yes(&value, "admission_verified"));
    assert!(a.recall.bank.join(MARKER).exists());
    let done = a.recover("rollback");
    assert_eq!(string(&done, "outcome"), "rolled-back");
    assert_eq!(a.state(), before);
}
