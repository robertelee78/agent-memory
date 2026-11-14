use agent_memory::codec::{
    frontmatter,
    json::{self, Value},
};
use agent_memory::developer_identity::{self as developer, encoded, load};
use agent_memory::error::{ContractError, Kind, Result};
use agent_memory::scope_binding::{self as scope, RequireOptions};
use agent_memory::{
    bank_contract as bank, bank_layout, bank_transactions as txn, bank_writes as writes, cli,
    clock::Deadline, host, process, project_identity as project, records, tombstones,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const ID: &str = "75b38e17-c36e-4fab-8ba9-5fa51f25de91";
const OTHER: &str = "bdcb692b-2322-4f21-b9dd-a4a4eca21145";
const DEVELOPER: &str = "b3dcae21-f5f3-40ac-bcfe-e845a3d2868b";
fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}
fn obj(v: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(v.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn g<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap()
}
fn st<'a>(v: &'a Value, k: &str) -> &'a str {
    g(v, k).as_str().unwrap()
}
fn set(v: &mut Value, key: &str, value: Value) {
    let Value::Object(pairs) = v else {
        panic!("fixture object")
    };
    if let Some((_, old)) = pairs.iter_mut().find(|(k, _)| k == key) {
        *old = value;
    } else {
        pairs.push((key.into(), value));
    }
}
fn remove(v: &mut Value, key: &str) -> Value {
    let Value::Object(pairs) = v else {
        panic!("fixture object")
    };
    let index = pairs.iter().position(|(k, _)| k == key).unwrap();
    pairs.remove(index).1
}
fn context(initialization: bool) -> Value {
    obj([
        ("developer_id", s(DEVELOPER)),
        ("project_id", s(ID)),
        ("developer_sha256", s("a".repeat(64))),
        ("project_sha256", s("b".repeat(64))),
        ("checkout_key", s("1:2")),
        ("repository_settings_sha256", s("c".repeat(64))),
        ("initialization_only", Value::Bool(initialization)),
    ])
}
fn files(identity: &str, remote: Option<&str>) -> Value {
    let mut request = obj([
        ("project-id", s(identity)),
        ("router-updated", s("2026-10-22T18:00:00Z")),
        (
            "contexts",
            obj([(
                "activeContext.md",
                obj([
                    ("body", s("# Focus\n\nSource-backed fixture context.\n")),
                    ("purpose", s("Current focus")),
                    ("scope", s("repository")),
                    ("lifecycle", s("active")),
                    ("read-when", s("When resuming")),
                ]),
            )]),
        ),
        (
            "omitted",
            obj([
                "projectbrief.md",
                "productContext.md",
                "systemPatterns.md",
                "techContext.md",
                "progress.md",
            ]
            .map(|name| (name, s("No distinct source evidence supplied")))),
        ),
    ]);
    if let Some(remote) = remote {
        set(&mut request, "canonical-remote", s(remote));
    }
    g(&bank::render(&request).unwrap(), "files").clone()
}
fn record(identity: &str) -> String {
    let template = json::decode(
        include_str!("../skills/agent-memory/templates/record-proposal.example.json"),
        json::DecodePolicy::default(),
    )
    .unwrap();
    let mut metadata = g(&template, "metadata").clone();
    set(&mut metadata, "project-id", s(identity));
    set(&mut metadata, "scope", s("branch:feature"));
    set(&mut metadata, "status", s("active"));
    set(&mut metadata, "conditions", obj([] as [(&str, Value); 0]));
    set(&mut metadata, "trust", s("reviewed"));
    set(&mut metadata, "visibility", s("visible"));
    set(
        &mut metadata,
        "admission-reason",
        s("Disposable structural fixture; not real admission evidence"),
    );
    set(
        metadata.get_mut("origin").unwrap(),
        "code-state",
        s("not-code"),
    );
    frontmatter::write(metadata.as_object().unwrap()).unwrap()
        + "# Lesson\n\nConditional source-backed observation.\n"
}
fn extended_files(identity: &str) -> (Value, String, String, String) {
    records::register_tombstone_parser(Some(Arc::new(tombstones::TombstoneParser)));
    let mut files = files(identity, None);
    let record = record(identity);
    let index = bank_layout::render_index(&obj([
        ("path", s("topics/index.md")),
        ("project-id", s(identity)),
        ("updated", s("2026-10-22T18:00:00Z")),
        (
            "routes",
            Value::Array(vec![obj([
                ("path", s("topics/lesson.md")),
                ("purpose", s("Conditional lesson")),
                ("scope", s("branch:feature")),
                ("lifecycle", s("active")),
                ("read-when", s("When on feature")),
            ])]),
        ),
    ]))
    .unwrap();
    let index = st(&index, "content").to_owned();
    let tomb_path = format!("history/tombstones/{OTHER}.md");
    let tomb = frontmatter::write(&[
        ("id".into(), s(OTHER)),
        ("scope".into(), s(identity)),
        ("deleted-at".into(), s("2026-10-22T18:00:00Z")),
        ("deletion-authority".into(), s(DEVELOPER)),
        ("reason-class".into(), s("sensitive")),
    ])
    .unwrap();
    let router = st(&files, "MEMORY.md").replace("\n## Omitted contexts\n", &format!(
        "\n| [topics/index.md](topics/index.md) | Lessons | repository | active | When recalling |\n| [{tomb_path}]({tomb_path}) | Deletion | repository | deleted | During reconciliation |\n\n## Omitted contexts\n"));
    set(&mut files, "MEMORY.md", s(router));
    set(&mut files, "topics/index.md", s(&index));
    set(&mut files, "topics/lesson.md", s(&record));
    set(&mut files, &tomb_path, s(&tomb));
    bank::validate_files(&files, None).unwrap();
    (files, record, index, tomb)
}

#[test]
fn pinned_context_rejects_every_missing_extra_wrong_identity_and_initialization_mode() {
    scope::context_value(&Value::Null, false).unwrap();
    scope::context_value(&Value::Null, true).unwrap();
    for initialize in [false, true] {
        let good = context(initialize);
        scope::context_value(&good, initialize).unwrap();
        assert_eq!(
            scope::context_value(&good, !initialize)
                .unwrap_err()
                .to_string(),
            "Invalid pinned scope context"
        );
        for field in scope::CONTEXT_FIELDS {
            let mut bad = good.clone();
            remove(&mut bad, field);
            assert_eq!(
                scope::context_value(&bad, initialize)
                    .unwrap_err()
                    .to_string(),
                "Invalid pinned scope context"
            );
        }
        for field in ["project_id", "developer_id"] {
            let mut bad = good.clone();
            set(&mut bad, field, s("not-a-uuid"));
            assert!(scope::context_value(&bad, initialize).is_err());
        }
        let mut bad = good.clone();
        set(&mut bad, "initialization_only", Value::Int(1.into()));
        assert!(scope::context_value(&bad, initialize).is_err());
        let mut bad = good;
        set(&mut bad, "hold", Value::Null);
        assert!(scope::context_value(&bad, initialize).is_err());
    }
    for bad in [Value::Bool(false), Value::Array(Vec::new()), s("scope")] {
        assert_eq!(
            scope::context_value(&bad, false).unwrap_err().to_string(),
            "Invalid pinned scope context"
        );
    }
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_fork_changes_every_namespace_and_preserves_record_provenance_conditions_and_tombstone
#[test]
fn namespace_rewrite_preserves_records_indexes_tombstones_and_context_bytes() {
    let (original, record, index, tomb) = extended_files(ID);
    let desired = scope::identity_files(
        &original,
        &obj([
            ("project-id", s(DEVELOPER)),
            ("canonical-remote", Value::Null),
        ]),
    )
    .unwrap();
    assert_eq!(
        st(&desired, "topics/lesson.md"),
        record.replace(ID, DEVELOPER)
    );
    assert_eq!(
        st(&desired, "topics/index.md"),
        index.replace(ID, DEVELOPER)
    );
    assert_eq!(
        st(&desired, &format!("history/tombstones/{OTHER}.md")),
        tomb.replace(ID, DEVELOPER)
    );
    assert_eq!(
        g(&original, "activeContext.md"),
        g(&desired, "activeContext.md")
    );
    assert_eq!(
        g(&bank::validate_files(&desired, None).unwrap(), "project_id"),
        &s(DEVELOPER)
    );
    assert_eq!(st(&original, "topics/lesson.md"), record);
    let bound = scope::identity_files(
        &original,
        &obj([
            ("project-id", s(ID)),
            ("canonical-remote", s("repo://github.com/owner/project")),
        ]),
    )
    .unwrap();
    assert!(
        st(&bound, "MEMORY.md").contains("canonical-remote: \"repo://github.com/owner/project\"")
    );
    let unbound = scope::identity_files(
        &bound,
        &obj([("project-id", s(ID)), ("canonical-remote", Value::Null)]),
    )
    .unwrap();
    assert_eq!(encoded(&original).unwrap(), encoded(&unbound).unwrap());
}

#[test]
fn binding_bundle_refuses_unknown_fields_unnecessary_transactions_and_wrong_preview_base() {
    let base = files(ID, None);
    let project = obj([
        ("operation_id", s(OTHER)),
        ("project_id", s(ID)),
        ("action", s("bind")),
        (
            "bank_identity",
            obj([("project-id", s(ID)), ("canonical-remote", Value::Null)]),
        ),
        (
            "evidence",
            obj([
                ("bank_project_id", s(ID)),
                (
                    "router_sha256",
                    s(agent_memory::developer_identity::digest(Some(
                        st(&base, "MEMORY.md").as_bytes(),
                    ))
                    .unwrap()),
                ),
            ]),
        ),
    ]);
    let good = obj([
        ("schema", s(scope::SCHEMA)),
        ("operation_id", s(OTHER)),
        ("developer_id", s(DEVELOPER)),
        ("developer_sha256", s("a".repeat(64))),
        ("project", project),
        ("base_files", base),
        ("transaction", Value::Null),
    ]);
    scope::bundle_value(&good).unwrap();
    scope::bundle_value(&obj([("proposal", good.clone())])).unwrap();
    for field in scope::FIELDS {
        let mut bad = good.clone();
        remove(&mut bad, field);
        assert_eq!(
            scope::bundle_value(&bad).unwrap_err().to_string(),
            "Invalid scope binding bundle"
        );
    }
    let mut bad = good.clone();
    set(&mut bad, "unexpected", Value::Null);
    assert!(scope::bundle_value(&bad).is_err());
    let mut bad = good.clone();
    set(&mut bad, "transaction", obj([] as [(&str, Value); 0]));
    assert_eq!(
        scope::bundle_value(&bad).unwrap_err().to_string(),
        "Unnecessary or unknown bank transaction in binding"
    );
    let mut bad = good.clone();
    set(
        bad.get_mut("project").unwrap().get_mut("evidence").unwrap(),
        "router_sha256",
        s("0".repeat(64)),
    );
    assert_eq!(
        scope::bundle_value(&bad).unwrap_err().to_string(),
        "Binding base differs from the project preview"
    );
    let mut create = good;
    set(&mut create, "base_files", Value::Null);
    let p = create.get_mut("project").unwrap();
    set(p, "action", s("create"));
    set(
        p.get_mut("evidence").unwrap(),
        "bank_project_id",
        Value::Null,
    );
    scope::bundle_value(&create).unwrap();
    set(create.get_mut("project").unwrap(), "action", s("bind"));
    assert_eq!(
        scope::bundle_value(&create).unwrap_err().to_string(),
        "Only explicit create can precede bank initialization"
    );
}

#[test]
fn command_receipts_use_baseline_codecs_and_capacity_free_refusals() {
    let value = scope::result(
        "status",
        [("label", s("é")), ("writes_performed", Value::Bool(false))],
    );
    let success = scope::command_result("status", Ok(value.clone()));
    assert_eq!(success.exit_code, 0);
    assert_eq!(
        success.stdout,
        json::encode(&value, json::INDENT2_UTF8).unwrap() + "\n"
    );
    assert!(success.stderr.is_empty());
    let failure = scope::command_result("require", Err(ContractError::new("Synthetic refusal")));
    assert_eq!(failure.exit_code, 1);
    assert!(failure.stdout.contains("\"inspection_required\": true"));
    let failure_value = failure.value.as_ref().unwrap();
    assert_eq!(
        failure.stdout,
        json::encode(failure_value, json::DEFAULT).unwrap() + "\n"
    );
    assert_eq!(
        g(failure_value, "identity_authenticated"),
        &Value::Bool(false)
    );
}

struct World {
    root: PathBuf,
}
impl World {
    fn env(&self) -> Vec<(OsString, OsString)> {
        vec![
            ("PATH".into(), "/usr/bin:/bin:/usr/local/bin".into()),
            ("HOME".into(), self.root.join("home").into_os_string()),
            (
                "AGENT_MEMORY_CONFIG_HOME".into(),
                self.root.join("config").into_os_string(),
            ),
            (
                "AGENT_MEMORY_STATE_HOME".into(),
                self.root.join("state/agent-memory").into_os_string(),
            ),
            (
                "XDG_CONFIG_HOME".into(),
                self.root.join("xdg-config").into_os_string(),
            ),
            (
                "XDG_STATE_HOME".into(),
                self.root.join("state").into_os_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("LC_ALL".into(), "C".into()),
        ]
    }
    fn command(&self, argv: &[OsString], extra: &[(&str, OsString)]) -> process::Output {
        let mut env = self.env();
        env.extend(extra.iter().map(|(k, v)| ((*k).into(), v.clone())));
        let spec = process::Spec {
            argv,
            env: &env,
            cwd: Some(&self.root),
            search_path: true,
            stdin: process::Input::Null,
            stdout: process::Target::Capture {
                limit: 16 * 1024 * 1024,
            },
            stderr: process::Target::Capture {
                limit: 16 * 1024 * 1024,
            },
            deadline: Deadline::from_absolute_ns(u64::MAX),
            new_session: false,
            file_size_limit: None,
            umask: None,
            pass_fds: &[],
        };
        process::run_for(&spec, Duration::from_secs(120)).unwrap()
    }
    fn git(&self, repo: &Path, args: &[&str]) -> String {
        let mut argv = vec![
            OsString::from("git"),
            "-C".into(),
            repo.as_os_str().to_owned(),
        ];
        argv.extend(args.iter().map(OsString::from));
        let output = self.command(&argv, &[]);
        assert_eq!(
            output.status,
            process::Status::Code(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
    fn repo(&self, name: &str) -> PathBuf {
        let repo = self.root.join(name);
        std::fs::create_dir(&repo).unwrap();
        self.git(&repo, &["init", "--initial-branch=main"]);
        self.git(&repo, &["config", "user.name", "Fixture"]);
        self.git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        repo
    }
    fn bank(&self, repo: &Path, identity: &str, remote: Option<&str>) -> PathBuf {
        self.write_files(repo, &files(identity, remote))
    }
    fn write_files(&self, repo: &Path, files: &Value) -> PathBuf {
        let bank = repo.join("memory-bank");
        std::fs::create_dir_all(&bank).unwrap();
        for (path, body) in files.as_object().unwrap() {
            let path = bank.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body.as_str().unwrap()).unwrap();
        }
        bank
    }
    fn developer(&self, label: &str) -> String {
        let p = developer::preview(
            "create",
            None,
            &Value::Array(vec![s(label)]),
            &Value::Array(Vec::new()),
        )
        .unwrap();
        st(&developer::apply(&p).unwrap(), "developer_id").to_owned()
    }
    fn preview(&self, repo: &Path, action: &str) -> Value {
        scope::preview(
            repo,
            action,
            None,
            "Explicit fixture binding decision",
            None,
        )
        .unwrap()
    }
    fn bind(&self, repo: &Path) -> Value {
        scope::apply(&self.preview(repo, "bind")).unwrap()
    }
    fn status(&self, repo: &Path) -> Value {
        scope::status(repo, &[], None, None).unwrap()
    }
    fn registry(&self) -> Value {
        load(&std::fs::read(self.root.join("config/projects.json")).unwrap()).unwrap()
    }
    fn save_registry(&self, value: &Value) {
        std::fs::write(
            self.root.join("config/projects.json"),
            encoded(value).unwrap(),
        )
        .unwrap();
    }
    fn save(&self, value: &Value, name: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, encoded(value).unwrap()).unwrap();
        path
    }
    fn native(&self, args: &[OsString], extra: &[(&str, OsString)]) -> process::Output {
        let mut argv = vec![OsString::from(env!("CARGO_BIN_EXE_am")), "--json".into()];
        argv.extend_from_slice(args);
        self.command(&argv, extra)
    }
    fn canonical(&self, bank: &Path) -> BTreeMap<String, Vec<u8>> {
        fn visit(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            if !path.exists() {
                return;
            }
            for item in std::fs::read_dir(path).unwrap() {
                let item = item.unwrap();
                let path = item.path();
                if item.file_type().unwrap().is_dir() {
                    visit(root, &path, out);
                } else if path.extension().is_some_and(|v| v == "md")
                    && !item
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".agent-memory")
                {
                    out.insert(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        std::fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        visit(bank, bank, &mut out);
        out
    }
    fn historical(&self, repo: &Path) -> (String, String) {
        let current = st(
            g(&project::status(repo).unwrap(), "evidence"),
            "checkout_key",
        )
        .to_owned();
        let (device, inode) = current.split_once(':').unwrap();
        let old = format!("{}:{inode}", device.parse::<u64>().unwrap() + 100);
        let mut data = self.registry();
        let entry = remove(data.get_mut("checkouts").unwrap(), &current);
        set(data.get_mut("checkouts").unwrap(), &old, entry);
        self.save_registry(&data);
        (current, old)
    }
}
fn in_world(name: &str, body: impl FnOnce(&World)) {
    if std::env::var("AM_SCOPE_CASE").ok().as_deref() == Some(name) {
        // Only production registrations. No scanner/frontier/hold/security mock.
        cli::register_providers().unwrap();
        scope::register_providers();
        let world = World {
            root: PathBuf::from(std::env::var_os("AM_SCOPE_ROOT").unwrap()),
        };
        body(&world);
        return;
    }
    let root = std::env::temp_dir().join(format!("am-scope-{}", host::uuid4().unwrap()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::create_dir(root.join("home")).unwrap();
    let world = World { root };
    let output = world.command(
        &[
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            name.into(),
            "--include-ignored".into(),
            "--nocapture".into(),
        ],
        &[
            ("AM_SCOPE_CASE", name.into()),
            ("AM_SCOPE_ROOT", world.root.as_os_str().to_owned()),
        ],
    );
    std::fs::remove_dir_all(&world.root).unwrap();
    assert_eq!(
        output.status,
        process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_combined_status_readonly_missing_conflicting_and_known_attribution
#[test]
#[ignore = "needs security, recovery_frontier and project dependency adapters registered"]
fn combined_status_readonly_missing_conflicting_and_known_attribution() {
    in_world(
        "combined_status_readonly_missing_conflicting_and_known_attribution",
        |w| {
            let repo = w.repo("repository");
            let bank = w.bank(&repo, ID, None);
            let before = w.canonical(&bank);
            let missing = w.status(&repo);
            assert_eq!(g(&missing, "resolved"), &Value::Bool(false));
            assert_eq!(g(&missing, "project_id"), &s(ID));
            assert_eq!(g(&missing, "developer_id"), &Value::Null);
            assert!(!w.root.join("config").exists());
            assert_eq!(w.canonical(&bank), before);
            let a = w.developer("Fixture developer");
            let p = w.preview(&repo, "bind");
            assert_eq!(g(&p, "changed_memory_paths"), &Value::Array(Vec::new()));
            assert!(!w.root.join("state/agent-memory").exists());
            assert_eq!(w.canonical(&bank), before);
            let done = scope::apply(&p).unwrap();
            assert_eq!(g(&done, "verified"), &Value::Bool(true));
            assert_eq!(g(g(&done, "status"), "developer_id"), &s(&a));
            assert_eq!(g(&done, "identity_authenticated"), &Value::Bool(false));
            let b = w.developer("Second developer");
            let aliases =
                developer::declared(&["git-author-email=first@example.invalid".into()]).unwrap();
            let adoption =
                developer::preview("adopt", Some(&a), &Value::Array(Vec::new()), &aliases).unwrap();
            developer::apply(&adoption).unwrap();
            let select = developer::preview(
                "select",
                Some(&b),
                &Value::Array(Vec::new()),
                &Value::Array(Vec::new()),
            )
            .unwrap();
            developer::apply(&select).unwrap();
            w.git(&repo, &["config", "user.email", "first@example.invalid"]);
            let conflict = w.status(&repo);
            assert_eq!(g(&conflict, "resolved"), &Value::Bool(false));
            assert_eq!(g(&conflict, "developer_id"), &s(&b));
            let conflicts = g(g(&conflict, "developer"), "attribution_conflicts")
                .as_array()
                .unwrap();
            assert_eq!(
                g(conflicts.first().unwrap(), "matching_developer_ids"),
                &Value::Array(vec![s(a)])
            );
            assert!(scope::require(&repo, None, RequireOptions::default()).is_err());
            assert_eq!(w.canonical(&bank), before);
        },
    );
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_create_has_initialization_only_handoff_without_chicken_and_egg
#[test]
#[ignore = "needs security and project dependency adapters registered"]
fn create_has_initialization_only_handoff_without_chicken_and_egg() {
    in_world(
        "create_has_initialization_only_handoff_without_chicken_and_egg",
        |w| {
            let repo = w.repo("repository");
            w.developer("Fixture developer");
            let p = w.preview(&repo, "create");
            assert!(!repo.join("memory-bank").exists());
            let done = scope::apply(&p).unwrap();
            assert_eq!(g(&done, "verified"), &Value::Bool(false));
            assert_eq!(g(&done, "registry_verified"), &Value::Bool(true));
            assert_eq!(g(&done, "initialization_pending"), &Value::Bool(true));
            assert_eq!(
                g(g(&done, "initialization_context"), "initialization_only"),
                &Value::Bool(true)
            );
            assert!(!repo.join("memory-bank").exists());
            assert!(scope::require(&repo, None, RequireOptions::default()).is_err());
            let scope = scope::require(
                &repo,
                None,
                RequireOptions {
                    initialize: true,
                    ..Default::default()
                },
            )
            .unwrap();
            w.bank(&repo, st(&scope, "project_id"), None);
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
            assert!(
                scope::require(
                    &repo,
                    None,
                    RequireOptions {
                        initialize: true,
                        ..Default::default()
                    }
                )
                .is_err()
            );
            let replay = scope::apply(&p).unwrap();
            assert_eq!(g(&replay, "verified"), &Value::Bool(true));
            assert_eq!(g(&replay, "initialization_pending"), &Value::Bool(false));
        },
    );
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_rebind_previews_bank_change_reads_both_stores_and_preserves_aliases_and_code
#[test]
#[ignore = "needs security, recovery_frontier, turn_sources and project dependency adapters registered"]
fn rebind_preserves_aliases_context_code_index_and_head() {
    in_world(
        "rebind_preserves_aliases_context_code_index_and_head",
        |w| {
            let repo = w.repo("repository");
            w.git(
                &repo,
                &["remote", "add", "origin", "git@github.com:old/project.git"],
            );
            let bank = w.bank(&repo, ID, Some("repo://github.com/old/project"));
            w.developer("Fixture developer");
            w.bind(&repo);
            std::fs::write(repo.join("feature.txt"), "Unfinished feature\n").unwrap();
            w.git(&repo, &["add", "feature.txt"]);
            let index = std::fs::read(repo.join(".git/index")).unwrap();
            let head = std::fs::read(repo.join(".git/HEAD")).unwrap();
            w.git(
                &repo,
                &[
                    "remote",
                    "set-url",
                    "origin",
                    "https://user:secret@github.com/new/project.git?secret=secret",
                ],
            );
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(false));
            let before = w.canonical(&bank);
            let p = w.preview(&repo, "rebind");
            assert_eq!(
                g(&p, "changed_memory_paths"),
                &Value::Array(vec![s("memory-bank/MEMORY.md")])
            );
            assert_eq!(w.canonical(&bank), before);
            assert!(!json::encode(&p, json::DEFAULT).unwrap().contains("secret"));
            let done = scope::apply(&p).unwrap();
            assert_eq!(g(&done, "verified"), &Value::Bool(true));
            assert_eq!(g(&done, "project_id"), &s(ID));
            assert_eq!(
                g(
                    g(g(g(&done, "status"), "project"), "project_entry"),
                    "aliases"
                ),
                &Value::Array(vec![s("repo://github.com/old/project")])
            );
            assert_eq!(
                std::fs::read(bank.join("activeContext.md")).unwrap(),
                *before.get("activeContext.md").unwrap()
            );
            assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
            assert_eq!(std::fs::read(repo.join(".git/HEAD")).unwrap(), head);
            assert_eq!(
                std::fs::read_to_string(repo.join("feature.txt")).unwrap(),
                "Unfinished feature\n"
            );
        },
    );
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_fork_changes_every_namespace_and_preserves_record_provenance_conditions_and_tombstone
#[test]
#[ignore = "needs security, recovery_frontier, turn_sources and project dependency adapters registered"]
fn fork_changes_every_namespace_through_real_two_store_binding() {
    in_world(
        "fork_changes_every_namespace_through_real_two_store_binding",
        |w| {
            let repo = w.repo("repository");
            w.bank(&repo, ID, None);
            w.developer("Fixture developer");
            w.bind(&repo);
            let (all, record, index, tomb) = extended_files(ID);
            let bank = w.write_files(&repo, &all);
            w.git(&repo, &["add", "."]);
            w.git(
                &repo,
                &["commit", "-m", "Conditional lesson and deletion fixture"],
            );
            let other = w.root.join("copy");
            w.git(
                &w.root,
                &["clone", repo.to_str().unwrap(), other.to_str().unwrap()],
            );
            w.git(&other, &["remote", "remove", "origin"]);
            assert_eq!(g(&w.status(&other), "resolved"), &Value::Bool(false));
            let done = scope::apply(&w.preview(&other, "fork")).unwrap();
            assert_eq!(g(&done, "verified"), &Value::Bool(true));
            let target = st(&done, "project_id");
            assert_ne!(target, ID);
            let forked = other.join("memory-bank");
            assert_eq!(
                std::fs::read_to_string(forked.join("topics/lesson.md")).unwrap(),
                record.replace(ID, target)
            );
            assert_eq!(
                std::fs::read_to_string(forked.join("topics/index.md")).unwrap(),
                index.replace(ID, target)
            );
            assert_eq!(
                std::fs::read_to_string(forked.join(format!("history/tombstones/{OTHER}.md")))
                    .unwrap(),
                tomb.replace(ID, target)
            );
            assert_eq!(
                std::fs::read(forked.join("activeContext.md")).unwrap(),
                std::fs::read(bank.join("activeContext.md")).unwrap()
            );
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
            assert_eq!(g(&w.status(&other), "resolved"), &Value::Bool(true));
            bank::validate(&other, None, None).unwrap();
        },
    );
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_stale_developer_registry_bank_and_remote_proposals_never_overwrite
#[test]
#[ignore = "needs security, recovery_frontier and project dependency adapters registered"]
fn stale_developer_registry_bank_and_remote_never_overwrite() {
    in_world(
        "stale_developer_registry_bank_and_remote_never_overwrite",
        |w| {
            let repo = w.repo("repository");
            let bank = w.bank(&repo, ID, None);
            w.developer("Fixture developer");
            let p = w.preview(&repo, "bind");
            w.developer("Different actor");
            let before = w.canonical(&bank);
            assert!(scope::apply(&p).is_err());
            assert!(!w.root.join("config/projects.json").exists());
            let p = w.preview(&repo, "bind");
            std::fs::write(
                bank.join("activeContext.md"),
                "# Focus\n\nNewer external evidence.\n",
            )
            .unwrap();
            assert!(scope::apply(&p).is_err());
            assert_ne!(w.canonical(&bank), before);
            let p = w.preview(&repo, "bind");
            w.git(
                &repo,
                &[
                    "remote",
                    "add",
                    "origin",
                    "https://github.com/owner/project.git",
                ],
            );
            assert!(scope::apply(&p).is_err());
            assert!(!w.root.join("config/projects.json").exists());
            let p = w.preview(&repo, "bind");
            let registry_only = project::preview(
                &repo,
                "bind",
                None,
                "Another explicit registry decision",
                None,
            )
            .unwrap();
            project::apply(&registry_only).unwrap();
            let newer = std::fs::read(w.root.join("config/projects.json")).unwrap();
            assert!(scope::apply(&p).is_err());
            assert_eq!(
                std::fs::read(w.root.join("config/projects.json")).unwrap(),
                newer
            );
            assert_eq!(
                std::fs::read_to_string(bank.join("activeContext.md")).unwrap(),
                "# Focus\n\nNewer external evidence.\n"
            );
        },
    );
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_registry_only_interruption_and_torn_bank_resume_with_original_bundle
#[test]
#[ignore = "needs security, recovery_frontier, turn_sources and project dependency adapters registered"]
fn registry_only_and_torn_bank_interruptions_resume_original_bundle() {
    in_world(
        "registry_only_and_torn_bank_interruptions_resume_original_bundle",
        |w| {
            for (index, stage) in [
                "binding-after-registry",
                "transaction-after-write:MEMORY.md",
            ]
            .iter()
            .enumerate()
            {
                let repo = w.repo(&format!("repository-{index}"));
                w.git(
                    &repo,
                    &[
                        "remote",
                        "add",
                        "origin",
                        "https://github.com/owner/project.git",
                    ],
                );
                let bank = w.bank(&repo, ID, None);
                w.developer("Fixture developer");
                let p = w.preview(&repo, "bind");
                let file = w.save(&p, "binding-proposal.json");
                let output = w.native(
                    &[
                        "identity".into(),
                        "apply".into(),
                        "--proposal".into(),
                        file.into_os_string(),
                    ],
                    &[
                        ("AGENT_MEMORY_FAULT_STAGE", (*stage).into()),
                        ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
                    ],
                );
                assert_eq!(output.status, process::Status::Code(86));
                let tx = g(g(&p, "proposal"), "transaction");
                let (journal, _) = txn::locations(&repo, &bank, st(tx, "operation_id")).unwrap();
                let bundle = journal.parent().unwrap().join("binding.json");
                assert_eq!(
                    encoded(&load(&std::fs::read(&bundle).unwrap()).unwrap()).unwrap(),
                    encoded(g(&p, "proposal")).unwrap()
                );
                assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(false));
                if bank.join(".agent-memory.lock").exists() {
                    let recovery = writes::recover(&repo, None).unwrap();
                    writes::recover(&repo, Some(&recovery)).unwrap();
                }
                let done = scope::apply(&p).unwrap();
                assert_eq!(g(&done, "verified"), &Value::Bool(true));
                assert!(!bank.join(txn::MARKER).exists());
                assert!(!bundle.exists());
                std::fs::write(
                    bank.join("activeContext.md"),
                    "# Focus\n\nA consequential later correction.\n",
                )
                .unwrap();
                let before = w.canonical(&bank);
                assert!(scope::apply(&p).is_err());
                assert_eq!(w.canonical(&bank), before);
            }
        },
    );
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_identity_is_pinned_from_preview_and_rechecked_under_the_bank_lock
#[test]
#[ignore = "needs security, recovery_frontier, turn_sources and project dependency adapters registered"]
fn identity_pins_are_rechecked_under_real_bank_lock() {
    in_world("identity_pins_are_rechecked_under_real_bank_lock", |w| {
        let repo = w.repo("repository");
        let bank = w.bank(&repo, ID, None);
        w.developer("Fixture developer");
        w.bind(&repo);
        let p = writes::preview(
            &repo,
            "activeContext.md",
            "# Focus\n\nCaptured by the original developer.\n",
            "replace",
        )
        .unwrap();
        let original = w.canonical(&bank);
        let file = w.save(&p, "single-proposal.json");
        let directory = w.root.join("identity-rendezvous");
        let mut environment = w.env();
        environment.extend([
            (
                "AGENT_MEMORY_FAULT_STAGE".into(),
                "locked-before-inventory".into(),
            ),
            ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
            (
                "AGENT_MEMORY_FAULT_DIR".into(),
                directory.clone().into_os_string(),
            ),
        ]);
        let argv = [
            OsString::from(env!("CARGO_BIN_EXE_am")),
            "--json".into(),
            "writes".into(),
            "apply".into(),
            "--proposal".into(),
            file.into_os_string(),
        ];
        let child = std::thread::spawn(move || {
            let spec = process::Spec {
                argv: &argv,
                env: &environment,
                cwd: None,
                search_path: true,
                stdin: process::Input::Null,
                stdout: process::Target::Capture {
                    limit: 4 * 1024 * 1024,
                },
                stderr: process::Target::Capture {
                    limit: 4 * 1024 * 1024,
                },
                deadline: Deadline::from_absolute_ns(u64::MAX),
                new_session: false,
                file_size_limit: None,
                umask: None,
                pass_fds: &[],
            };
            process::run_for(&spec, Duration::from_secs(20)).unwrap()
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !directory.join("locked-before-inventory.ready").exists()
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(directory.join("locked-before-inventory.ready").exists());
        w.developer("New explicitly selected developer");
        std::fs::write(directory.join("locked-before-inventory.release"), b"").unwrap();
        let output = child.join().unwrap();
        assert!(output.stderr.is_empty());
        assert_eq!(output.status, process::Status::Code(1));
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("identity changed")
        );
        assert_eq!(w.canonical(&bank), original);
        assert!(writes::apply(&p).is_err());
        assert_eq!(w.canonical(&bank), original);
    });
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_all_public_mediated_writers_refuse_unresolved_identity_and_raw_reads_remain_available
#[test]
#[ignore = "needs security, recovery_frontier, turn_sources and project dependency adapters registered"]
fn every_mediated_writer_refuses_unresolved_identity_but_raw_validation_remains_available() {
    in_world(
        "every_mediated_writer_refuses_unresolved_identity_but_raw_validation_remains_available",
        |w| {
            let repo = w.repo("repository");
            let bank = w.bank(&repo, ID, None);
            let content = "# Focus\n\nProposed mediated write.\n";
            let single = writes::preview(&repo, "activeContext.md", content, "replace").unwrap();
            let mut desired = files(ID, None);
            set(&mut desired, "activeContext.md", s(content));
            let tx = txn::preview(&repo, &desired, "compact").unwrap();
            let before = w.canonical(&bank);
            assert!(writes::apply(&single).is_err());
            assert_eq!(w.canonical(&bank), before);
            assert!(txn::apply(&tx).is_err());
            assert_eq!(w.canonical(&bank), before);
            assert!(!bank.join(".agent-memory.lock").exists());
            bank::validate(&repo, None, None).unwrap();
            assert!(!w.root.join("state/agent-memory").exists());
            w.developer("Fixture developer");
            w.bind(&repo);
            assert!(writes::apply(&single).is_err());
            let single = writes::preview(&repo, "activeContext.md", content, "replace").unwrap();
            let receipt = writes::apply(&single).unwrap();
            assert_eq!(
                g(g(&receipt, "resolved_scope"), "project_id"),
                g(&w.status(&repo), "project_id")
            );
            let mut forged = tx;
            set(forged.get_mut("proposal").unwrap(), "operation", s("bind"));
            assert!(txn::apply(&forged).is_err());
        },
    );
}

// obligation: tests/contracts/test_scope_binding.py::ScopeBindingContract.test_portable_status_uses_selected_repo_and_preserves_whitespace_paths_despite_ambient_git
#[test]
#[ignore = "needs security, recovery_frontier and project dependency adapters registered"]
fn public_status_preserves_whitespace_paths_and_ignores_ambient_git() {
    in_world(
        "public_status_preserves_whitespace_paths_and_ignores_ambient_git",
        |w| {
            let repo = w.repo("repository with trailing space ");
            w.bank(&repo, ID, None);
            w.developer("Fixture developer");
            w.bind(&repo);
            w.git(&repo, &["config", "user.email", "selected@example.invalid"]);
            let other = w.repo("different");
            w.git(&other, &["config", "user.email", "wrong@example.invalid"]);
            let output = w.native(
                &[
                    "identity".into(),
                    "status".into(),
                    "--repo".into(),
                    repo.clone().into_os_string(),
                    "--harness".into(),
                    "custom".into(),
                    "--session".into(),
                    "session-1".into(),
                ],
                &[
                    ("GIT_DIR", other.join(".git").into_os_string()),
                    ("GIT_CONFIG_COUNT", "1".into()),
                    ("GIT_CONFIG_KEY_0", "user.email".into()),
                    ("GIT_CONFIG_VALUE_0", "injected@example.invalid".into()),
                ],
            );
            assert_eq!(output.status, process::Status::Code(0));
            let current = load(&output.stdout).unwrap();
            assert_eq!(g(&current, "resolved"), &Value::Bool(true));
            assert_eq!(g(&current, "project_id"), &s(ID));
            assert_eq!(
                g(g(g(&current, "project"), "evidence"), "repo_root"),
                &s(repo.to_str().unwrap())
            );
            let observations = g(g(&current, "developer"), "observations")
                .as_array()
                .unwrap();
            assert!(
                observations
                    .iter()
                    .any(|row| g(row, "value") == &s("selected@example.invalid"))
            );
            assert!(!observations.iter().any(|row| {
                [s("wrong@example.invalid"), s("injected@example.invalid")]
                    .contains(g(row, "value"))
            }));
            assert_eq!(
                g(g(&current, "developer"), "session_context"),
                &obj([("harness", s("custom")), ("session", s("session-1"))])
            );
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_scope_link_repair_preserves_bank_and_verifies_current_alias
#[test]
#[ignore = "needs security, recovery_frontier and project dependency adapters registered"]
fn historical_device_scope_link_verifies_current_alias_without_bank_changes() {
    in_world(
        "historical_device_scope_link_verifies_current_alias_without_bank_changes",
        |w| {
            let repo = w.repo("repository");
            w.git(
                &repo,
                &[
                    "remote",
                    "add",
                    "origin",
                    "https://github.com/example/project.git",
                ],
            );
            let bank = w.bank(&repo, ID, Some("repo://github.com/example/project"));
            w.developer("Fixture developer");
            w.bind(&repo);
            let (current, old) = w.historical(&repo);
            let before = w.canonical(&bank);
            let done = scope::apply(&w.preview(&repo, "link")).unwrap();
            assert_eq!(g(&done, "verified"), &Value::Bool(true));
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
            assert_eq!(w.canonical(&bank), before);
            assert_eq!(
                g(
                    g(&project::status(&repo).unwrap(), "evidence"),
                    "checkout_key"
                ),
                &s(&current)
            );
            assert!(g(&w.registry(), "checkouts").get(&old).is_some());
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_conflicting_historical_aliases_allow_explicit_scope_link
#[test]
#[ignore = "needs security, recovery_frontier and project dependency adapters registered"]
fn conflicting_historical_aliases_allow_explicit_scope_link() {
    in_world(
        "conflicting_historical_aliases_allow_explicit_scope_link",
        |w| {
            let repo = w.repo("repository");
            w.git(
                &repo,
                &[
                    "remote",
                    "add",
                    "origin",
                    "https://github.com/example/project.git",
                ],
            );
            let bank = w.bank(&repo, ID, Some("repo://github.com/example/project"));
            w.developer("Fixture developer");
            w.bind(&repo);
            let (_, old) = w.historical(&repo);
            let mut data = w.registry();
            let alias = format!("999999999:{}", old.split_once(':').unwrap().1);
            let mut entry = g(g(&data, "checkouts"), &old).clone();
            set(&mut entry, "authority_name", s("other"));
            set(data.get_mut("checkouts").unwrap(), &alias, entry);
            w.save_registry(&data);
            assert_eq!(
                g(&project::status(&repo).unwrap(), "resolved"),
                &Value::Bool(false)
            );
            let before = w.canonical(&bank);
            let done = scope::apply(&w.preview(&repo, "link")).unwrap();
            assert_eq!(g(&done, "verified"), &Value::Bool(true));
            assert_eq!(
                g(&project::status(&repo).unwrap(), "resolved"),
                &Value::Bool(true)
            );
            assert_eq!(w.canonical(&bank), before);
            let final_registry = w.registry();
            for key in [&old, &alias] {
                assert_eq!(
                    encoded(g(g(&final_registry, "checkouts"), key)).unwrap(),
                    encoded(g(g(&data, "checkouts"), key)).unwrap()
                );
            }
        },
    );
}

#[test]
fn typed_enrollment_and_raw_identity_are_not_serializable_request_values() {
    // These function signatures are the capability boundary: neither function
    // accepts Value as an enrollment context or raw identity witness.
    let _: fn(&scope::PrivateEnrollmentContext) -> Value = scope::PrivateEnrollmentContext::scope;
    let _: fn(&Path, &project::RawIdentityEvidence, Option<&[u8]>, Option<&[u8]>) -> Result<Value> =
        scope::raw_recovery_context;
    let _: fn(&Value, bool) -> Result<()> = scope::context_value;
    assert!(
        scope::context_value(
            &obj([("scope", context(false)), ("held", Value::Bool(true))]),
            false
        )
        .is_err()
    );
}

#[test]
fn observer_locator_without_live_context_cannot_grant_scope_authority() {
    let root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(
        scope::observe_enrollment_location(&root, None, &root)
            .unwrap()
            .is_none()
    );
    scope::fresh_enrollment_locations(|| {
        assert!(scope::observe_enrollment_location(&root, None, &root)?.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn unresolved_principal_refuses_before_any_optional_provider_or_write() {
    in_world(
        "unresolved_principal_refuses_before_any_optional_provider_or_write",
        |w| {
            let repo = w.repo("repository");
            let bank = w.bank(&repo, ID, None);
            let before = w.canonical(&bank);
            let error = scope::require(&repo, None, RequireOptions::default()).unwrap_err();
            assert_eq!(error.kind(), &Kind::IdentityUnresolved);
            assert_eq!(
                error.to_string(),
                "Developer attribution is missing, conflicting or locked; explicitly create/adopt/select or repair it"
            );
            assert_eq!(scope::capture(&repo).unwrap(), Value::Null);
            assert!(!w.root.join("config").exists());
            assert!(!w.root.join("state/agent-memory").exists());
            assert_eq!(w.canonical(&bank), before);
            assert!(!bank.join(".agent-memory.lock").exists());
        },
    );
}

#[test]
fn migration_cannot_initialize_or_rebind_identity_and_never_calls_reader() {
    in_world(
        "migration_cannot_initialize_or_rebind_identity_and_never_calls_reader",
        |w| {
            let repo = w.repo("repository");
            w.developer("Fixture developer");
            let called = std::cell::Cell::new(false);
            let reader = |_: &[u8]| -> Result<Option<Value>> {
                called.set(true);
                Ok(None)
            };
            let error = scope::require(
                &repo,
                None,
                RequireOptions {
                    initialize: true,
                    migration_reader: Some(&reader),
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "Migration cannot initialize or rebind project identity"
            );
            assert!(!called.get());
            assert!(!repo.join("memory-bank").exists());
            assert!(!w.root.join("config/projects.json").exists());
        },
    );
}

#[test]
fn production_scope_adapter_preserves_initialization_and_live_locator_boundaries() {
    use writes::ScopeBinding;
    let provider = scope::ScopeBinding;
    provider.context_value(&context(false)).unwrap();
    assert!(provider.context_value(&context(true)).is_err());
    provider
        .context_value_for_initialization(&context(true), true)
        .unwrap();
    assert!(
        provider
            .context_value_for_initialization(&context(false), true)
            .is_err()
    );
    let root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(
        provider
            .observe_enrollment_location(&root, &root)
            .unwrap()
            .is_none()
    );
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_config_and_remote_mutation_between_enrollment_entry_exit_are_fresh
#[test]
#[ignore = "needs security, recovery_frontier, turn_sources and project dependency adapters registered"]
fn enrollment_entry_and_exit_independently_recheck_repository_configuration() {
    in_world(
        "enrollment_entry_and_exit_independently_recheck_repository_configuration",
        |w| {
            for mutation in ["config", "remote"] {
                let repo = w.repo(mutation);
                w.git(
                    &repo,
                    &[
                        "remote",
                        "add",
                        "origin",
                        "https://github.com/example/project.git",
                    ],
                );
                let bank = w.bank(&repo, ID, Some("repo://github.com/example/project"));
                w.developer("Fixture developer");
                w.bind(&repo);
                let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory");
                let context = scope::private_enrollment_context(&repo, &runtime).unwrap();
                assert!(!context.held());
                assert_eq!(context.project_id(), &s(ID));
                let original = w.canonical(&bank);
                let change = || {
                    if mutation == "config" {
                        w.git(
                            &repo,
                            &["config", "agent-memory.bankRoot", "different-bank"],
                        );
                    } else {
                        w.git(
                            &repo,
                            &[
                                "remote",
                                "set-url",
                                "origin",
                                "https://github.com/example/changed.git",
                            ],
                        );
                    }
                };
                // A successful operation body cannot escape the fresh exit check.
                assert!(
                    context
                        .within(|_| {
                            change();
                            Ok(())
                        })
                        .is_err()
                );
                assert_eq!(w.canonical(&bank), original);
                // Entry is independently checked; an already stale context cannot
                // authorize a body, even when its private namespace was pinned.
                let entered = std::cell::Cell::new(false);
                assert!(
                    context
                        .within(|_| {
                            entered.set(true);
                            Ok(())
                        })
                        .is_err()
                );
                assert!(!entered.get());
                assert!(scope::private_enrollment_context(&repo, &runtime).is_err());
                assert_eq!(w.canonical(&bank), original);
            }
        },
    );
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_subdirectory_still_refuses_repository_root_authority
#[test]
#[ignore = "needs security, recovery_frontier, turn_sources and project dependency adapters registered"]
fn enrollment_never_grants_repository_root_authority_to_subdirectories() {
    in_world(
        "enrollment_never_grants_repository_root_authority_to_subdirectories",
        |w| {
            let repo = w.repo("parent");
            let bank = w.bank(&repo, ID, None);
            w.developer("Fixture developer");
            w.bind(&repo);
            w.git(&repo, &["add", "."]);
            w.git(&repo, &["commit", "-m", "Fixture bank"]);
            let child = repo.join("ordinary source directory");
            std::fs::create_dir(&child).unwrap();
            let before = w.canonical(&bank);
            let index = std::fs::read(repo.join(".git/index")).unwrap();
            let head = std::fs::read(repo.join(".git/HEAD")).unwrap();
            let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory");
            assert!(agent_memory::repository_config::settings(&child).is_err());
            assert!(scope::private_enrollment_context(&child, &runtime).is_err());
            assert_eq!(w.canonical(&bank), before);
            assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
            assert_eq!(std::fs::read(repo.join(".git/HEAD")).unwrap(), head);
        },
    );
}
