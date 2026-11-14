#![cfg(feature = "dev-tools")]
//! Controlled native protocol only; never model/provider qualification.
#[allow(dead_code)]
#[path = "../src/bin/am_dev/omp_smoke.rs"]
mod smoke;
#[allow(unused_imports)]
mod support;
use agent_memory::{
    codec::json::{self, Value},
    omp_stream,
    omp_transport::{self as transport, Site},
    process::{self, Input, Spec, Status, Target},
};
use smoke::{get, obj, s, set};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{BufRead, Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use support::world::{RunOptions, World};
fn uuid() -> String {
    agent_memory::host::uuid4().unwrap()
}
fn t<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap()
}
fn decode(raw: &[u8]) -> Value {
    json::decode_bytes(raw, Default::default()).unwrap()
}
fn encoded(value: &Value) -> Vec<u8> {
    json::encode(value, json::SORTED_COMPACT_UTF8)
        .unwrap()
        .into_bytes()
}
fn private(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, encoded(value)).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn shell_quote(value: &OsStr) -> String {
    format!("'{}'", value.to_str().unwrap().replace('\'', "'\\''"))
}

// Deliberately copied from tests/t1_recovery_transactions.rs; Main deduplicates.
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
    assert_eq!(output.status, Status::Code(0));
    let receipt = decode(
        &fs::read(
            world
                .inside("state/agent-memory/install/receipt.json")
                .unwrap(),
        )
        .unwrap(),
    );
    let source = get(&receipt, "source");
    let command = agent_memory::installation::source_binary(source).unwrap();
    let pack = Path::new(t(source, "pack"));
    (
        pack.parent().unwrap().parent().unwrap().to_path_buf(),
        command,
    )
}

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    receipt_path: PathBuf,
    receipt: Value,
    runtime: PathBuf,
    am: PathBuf,
    env: smoke::Environment,
}
impl Fixture {
    fn prepare(world: &World, with_bank: bool) {
        let (runtime, _) = installed_runtime(world);
        let agent = world.inside("native smoke root").unwrap();
        fs::create_dir(&agent).unwrap();
        let installed = world
            .am(vec![
                OsString::from("--json"),
                "install".into(),
                "--development-source".into(),
                runtime.clone().into_os_string(),
                "--mode".into(),
                "link".into(),
                "--presets".into(),
                "omp".into(),
                "--omp-agent-dir".into(),
                agent.into_os_string(),
                "--omp-peers-upgraded".into(),
                "--no-guidance".into(),
                "--bin-dir".into(),
                world.inside("bin").unwrap().into_os_string(),
            ])
            .unwrap();
        assert_eq!(installed.status, Status::Code(0));
        let repo = world.repository("repo", false).unwrap();
        if with_bank {
            let mut proposal = json::decode(
                include_str!("../skills/agent-memory/templates/router-proposal.example.json"),
                Default::default(),
            )
            .unwrap();
            set(&mut proposal, "project-id", s(uuid()));
            let rendered = agent_memory::bank_contract::render(&proposal).unwrap();
            fs::create_dir(repo.join("memory-bank")).unwrap();
            for (path, value) in get(&rendered, "files").as_object().unwrap() {
                fs::write(repo.join("memory-bank").join(path), value.as_str().unwrap()).unwrap();
            }
            support::identity_fixtures::bind(world, &repo).unwrap();
        }
        private(
            &world.root.join("smoke-fixture.json"),
            &obj([
                ("repo", s(repo.to_str().unwrap())),
                ("runtime", s(runtime.to_str().unwrap())),
            ]),
        );
    }
    fn load(root: &Path) -> Self {
        agent_memory::cli::register_providers().unwrap();
        let value = smoke::read_json(&root.join("smoke-fixture.json")).unwrap();
        let receipt_path = root.join("state/agent-memory/install/receipt.json");
        let receipt = smoke::read_json(&receipt_path).unwrap();
        let am = Path::new(t(&receipt, "bin_dir")).join("am");
        let fixture = Self {
            root: root.to_path_buf(),
            repo: t(&value, "repo").into(),
            runtime: t(&value, "runtime").into(),
            receipt_path,
            receipt,
            am,
            env: std::env::vars_os().collect(),
        };
        if fixture.repo.join("memory-bank").is_dir() {
            // Controlled source-verifier setup; never native qualification.
            agent_memory::omp_boundary::activate_marker(&fixture.repo, false, &fixture.runtime)
                .unwrap();
        }
        fixture
    }
    fn status(&self) -> Value {
        self.public(&["--json", "status", "--repo", self.repo.to_str().unwrap()])
    }
    fn public(&self, args: &[&str]) -> Value {
        let argv: Vec<OsString> = std::iter::once(self.am.clone().into_os_string())
            .chain(args.iter().map(OsString::from))
            .collect();
        decode(&smoke::run_command(&argv, &self.repo, &self.env, None).unwrap())
    }
    fn site(&self) -> Site<'_> {
        Site::new(&self.repo, &self.runtime)
    }
    fn native_file(
        &self,
        name: &str,
        session: &str,
        humans: &[Value],
        title: bool,
    ) -> (PathBuf, PathBuf) {
        let root = self.root.join(name);
        fs::create_dir_all(root.join("sessions")).unwrap();
        let path = root.join("sessions/current.jsonl");
        let mut records = Vec::new();
        if title {
            records.push(obj([
                ("type", s("title")),
                ("v", Value::Int(1.into())),
                ("title", s("Controlled native session title")),
                ("pad", s("")),
                ("updatedAt", s("2026-11-06T00:00:00.000Z")),
            ]));
        }
        records.push(obj([
            ("type", s("session")),
            ("id", s(session)),
            ("cwd", s(self.repo.to_str().unwrap())),
        ]));
        records.extend_from_slice(humans);
        write_lines(&path, &records);
        (root, path)
    }
    fn observe(&self, session: &str, lifecycle: &str, humans: &[Value]) -> Value {
        observe_installed(
            &self.am,
            &self.repo,
            &self.receipt,
            &self.env,
            session,
            lifecycle,
            humans,
        )
    }
    fn enroll(&self, session: &str) {
        let site = self.site();
        for segment in transport::segments(&site, session).unwrap() {
            let cursor = segment.snapshot().unwrap();
            let directory = segment.directory.join("enrollments");
            for entry in fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                let enrollment = smoke::read_json(&entry.path()).unwrap();
                let (_, faults) =
                    transport::registration(&site, &segment.directory, &cursor, &enrollment, true)
                        .unwrap();
                assert!(faults.is_empty(), "{faults:?}");
            }
        }
    }
    fn binary(&self, version: &str, mode: &str) -> PathBuf {
        let path = self
            .root
            .join(format!("controlled-native-{version}-{mode}"));
        let exe = std::env::current_exe().unwrap();
        let script = format!(
            "#!/bin/sh\nif [ \"${{1-}}\" = --version ]; then printf '%s\\n' 'omp/{version}'; exit 0; fi\nprintf '%s\\n' 'Controlled protocol launch, never model proof.' > {}\nwhile [ \"$#\" -gt 0 ]; do\ncase \"$1\" in\n--session-dir) export AM_SMOKE_SESSION_DIR=\"$2\"; shift 2;;\n--cwd) export AM_SMOKE_REPO=\"$2\"; shift 2;;\n*) shift;;\nesac\ndone\nexport AM_SMOKE_PROTOCOL_MODE='{mode}'\nexec {} --exact controlled_native_protocol --nocapture --quiet\n",
            shell_quote(path.with_extension("launched").as_os_str()),
            shell_quote(exe.as_os_str())
        );
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn smoke(&self, binary: &Path, timeout: u64, expected: i32) -> Value {
        let output = self.root.join(format!("controlled-smoke-{}", uuid()));
        let argv: Vec<OsString> = vec![
            env!("CARGO_BIN_EXE_am-dev").into(),
            "omp-smoke".into(),
            "--binary".into(),
            binary.as_os_str().to_owned(),
            "--am".into(),
            self.am.clone().into_os_string(),
            "--receipt".into(),
            self.receipt_path.clone().into_os_string(),
            "--repo".into(),
            self.repo.clone().into_os_string(),
            "--output".into(),
            output.clone().into_os_string(),
            "--model".into(),
            "controlled/no-provider".into(),
            "--timeout".into(),
            timeout.to_string().into(),
        ];
        let env: Vec<_> = self
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let completed = process::run(&Spec {
            argv: &argv,
            env: &env,
            cwd: Some(&self.repo),
            stdin: Input::Null,
            stdout: Target::Capture { limit: 1024 * 1024 },
            stderr: Target::Capture { limit: 1024 * 1024 },
            search_path: false,
            deadline: agent_memory::clock::Deadline::after(Duration::from_secs(
                (timeout * 4 + 15).max(30),
            ))
            .unwrap(),
            new_session: false,
            file_size_limit: None,
            umask: None,
            pass_fds: &[],
        })
        .unwrap();
        assert_eq!(
            completed.status,
            Status::Code(expected),
            "{}\n{}",
            String::from_utf8_lossy(&completed.stdout),
            String::from_utf8_lossy(&completed.stderr)
        );
        let report = smoke::read_json(&output.join("report.json")).unwrap();
        assert_eq!(get(&report, "qualified"), &Value::Bool(expected == 0));
        assert_eq!(
            get(&report, "full_native_qualification"),
            &Value::Bool(false)
        );
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(output.join("report.json")).unwrap().mode() & 0o777,
            0o600
        );
        report
    }
}
fn isolated(name: &str, with_bank: bool, body: impl FnOnce(Fixture)) {
    if let Ok(root) = support::require_current_world() {
        body(Fixture::load(&root));
        return;
    }
    let world = World::new().unwrap();
    Fixture::prepare(&world, with_bank);
    let output = world
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                name.into(),
                "--include-ignored".into(),
                "--nocapture".into(),
            ],
            RunOptions {
                cwd: Some(&world.root),
                check: false,
                timeout: Duration::from_secs(240),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        output.status,
        Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn human(id: &str, content: &str, parent: Option<&str>) -> Value {
    obj([
        ("type", s("message")),
        ("id", s(id)),
        ("parentId", parent.map_or(Value::Null, s)),
        (
            "message",
            obj([
                ("role", s("user")),
                ("attribution", s("user")),
                ("content", s(content)),
            ]),
        ),
    ])
}
fn write_lines(path: &Path, records: &[Value]) {
    let mut file = fs::File::create(path).unwrap();
    for record in records {
        file.write_all(&encoded(record)).unwrap();
        file.write_all(b"\n").unwrap();
    }
}
fn append_lines(path: &Path, records: &[Value]) {
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    for record in records {
        file.write_all(&encoded(record)).unwrap();
        file.write_all(b"\n").unwrap();
    }
}
fn payload(repo: &Path, session: &str, entries: &[Value]) -> Value {
    let parents = entries
        .iter()
        .map(|e| {
            obj([
                ("id", get(e, "id").clone()),
                ("parent_id", get(e, "parentId").clone()),
                ("timestamp", Value::Null),
            ])
        })
        .collect();
    obj([
        ("entries", Value::Array(entries.to_vec())),
        (
            "branch_ids",
            Value::Array(entries.iter().map(|e| get(e, "id").clone()).collect()),
        ),
        ("event_ordinal", Value::Int(entries.len().into())),
        ("terminal", Value::Bool(false)),
        ("trigger", s("resume")),
        ("inherited_ids", Value::Array(vec![])),
        ("request_wake", Value::Bool(false)),
        (
            "view",
            obj([
                ("session", s(session)),
                ("cwd", s(repo.to_str().unwrap())),
                ("file", Value::Null),
                (
                    "header",
                    obj([
                        ("id", s(session)),
                        ("cwd", s(repo.to_str().unwrap())),
                        ("version", Value::Int(3.into())),
                        ("parent_session", Value::Null),
                        ("timestamp", Value::Null),
                    ]),
                ),
                (
                    "leaf_id",
                    entries.last().map_or(Value::Null, |e| get(e, "id").clone()),
                ),
                ("native_parents", Value::Array(parents)),
                ("source_kind", s("session-manager")),
                ("tail_status", s("available")),
                ("fork_status", s("none")),
            ]),
        ),
    ])
}
fn observe_installed(
    am: &Path,
    repo: &Path,
    receipt: &Value,
    env: &smoke::Environment,
    session: &str,
    lifecycle: &str,
    entries: &[Value],
) -> Value {
    let instance = uuid();
    let payload = payload(repo, session, entries);
    let request = obj([
        ("schema", s("agent-memory/omp-observer-request-v2")),
        ("action", s("seal")),
        ("session", s(session)),
        ("lifecycle", s(lifecycle)),
        ("payload", payload.clone()),
    ]);
    let mut native_ids: Vec<_> = entries.iter().map(|e| get(e, "id").clone()).collect();
    native_ids.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    let identity = obj([
        ("view_id", s(uuid())),
        ("leaf_id", get(get(&payload, "view"), "leaf_id").clone()),
        ("native_ids", Value::Array(native_ids)),
        ("tool_ids", Value::Array(vec![])),
    ]);
    let ordinal = get(&payload, "event_ordinal").clone();
    let mut wire = omp_stream::encode_frame(
        "REQUEST",
        1,
        &obj([
            ("request", request),
            ("event_ordinal", ordinal.clone()),
            ("identity", identity),
        ]),
        None,
    )
    .unwrap();
    wire.extend(
        omp_stream::encode_frame(
            "FINAL",
            2,
            &obj([
                ("last_attempt_seq", Value::Int(1.into())),
                ("refused_count", Value::Int(0.into())),
                ("last_event_ordinal", ordinal),
                ("transition", Value::Null),
            ]),
            None,
        )
        .unwrap(),
    );
    let argv: Vec<OsString> = vec![
        am.as_os_str().to_owned(),
        "omp-boundary".into(),
        "--stream".into(),
        "--repo".into(),
        repo.as_os_str().to_owned(),
        "--session".into(),
        session.into(),
        "--instance".into(),
        instance.clone().into(),
        "--lifecycle".into(),
        lifecycle.into(),
        "--artifact".into(),
        t(get(receipt, "source"), "artifact_digest").into(),
        "--omp-agent-dir".into(),
        t(get(receipt, "omp_binding"), "agent_dir").into(),
    ];
    let env: Vec<_> = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let completed = process::run(&Spec {
        argv: &argv,
        env: &env,
        cwd: Some(repo),
        stdin: Input::Bytes(&wire),
        stdout: Target::Capture { limit: 1024 * 1024 },
        stderr: Target::Capture { limit: 1024 * 1024 },
        search_path: false,
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(60)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    })
    .unwrap();
    assert_eq!(
        completed.status,
        Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&completed.stdout),
        String::from_utf8_lossy(&completed.stderr)
    );
    let mut raw = completed.stdout.as_slice();
    let mut ready = false;
    let mut result = None;
    while !raw.is_empty() {
        let end = raw.iter().position(|b| *b == b'\n').unwrap();
        assert!(end < 256);
        let header = std::str::from_utf8(&raw[..end]).unwrap();
        let fields: Vec<_> = header.split_whitespace().collect();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0], "AM249");
        assert_eq!(fields[4], "-");
        let length = fields[3].parse::<usize>().unwrap();
        assert!(length <= 65536);
        let body = decode(&raw[end + 1..end + 1 + length]);
        assert_eq!(get(&body, "instance_id"), &s(&instance));
        if fields[1] == "READY" {
            ready = true;
        }
        if fields[1] == "OBSERVED" && fields[2] == "1" {
            assert!(result.is_none());
            result = Some(get(&body, "result").clone());
        }
        raw = &raw[end + 1 + length..];
    }
    assert!(ready);
    let result = result.unwrap();
    assert_eq!(get(&result, "verified"), &Value::Bool(true));
    let base = PathBuf::from(t(get(receipt, "source"), "pack"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let inventory =
        omp_stream::inventory(&omp_stream::Installed::new(&base), repo, None, true).unwrap();
    let actual = get(&inventory, "streams")
        .as_array()
        .unwrap()
        .iter()
        .find(|row| get(row, "instance_id").as_str() == Some(&instance))
        .unwrap();
    assert_eq!(get(actual, "closed"), &Value::Bool(true));
    result
}

#[test]
fn controlled_native_protocol() {
    let Ok(mode) = std::env::var("AM_SMOKE_PROTOCOL_MODE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("AM_SMOKE_SESSION_DIR").unwrap());
    fs::create_dir(&root).unwrap();
    let repo = PathBuf::from(std::env::var_os("AM_SMOKE_REPO").unwrap());
    let world = support::require_current_world().unwrap();
    let fixture = Fixture::load(&world);
    let session = uuid();
    let mut lifecycle = uuid();
    let path = root.join("current.jsonl");
    write_lines(
        &path,
        &[obj([
            ("type", s("session")),
            ("id", s(&session)),
            ("cwd", s(repo.to_str().unwrap())),
        ])],
    );
    let emit = |value: Value| {
        println!("{}", String::from_utf8(encoded(&value)).unwrap());
        std::io::stdout().flush().unwrap();
    };
    emit(obj([("type", s("ready"))]));
    let mut entries = Vec::new();
    let mut sentinel = String::new();
    let mut reader = std::io::BufReader::new(std::io::stdin());
    loop {
        let mut line = String::new();
        let count = reader
            .by_ref()
            .take(1024 * 1024 + 1)
            .read_line(&mut line)
            .unwrap();
        if count == 0 {
            break;
        }
        assert!(count <= 1024 * 1024);
        let value = decode(line.as_bytes());
        match get(&value, "type").as_str() {
            Some("prompt") => {
                let prompt = t(&value, "message");
                let parent = entries.last().map(|e| t(e, "id"));
                let entry = human(&uuid(), prompt, parent);
                append_lines(&path, std::slice::from_ref(&entry));
                entries.push(entry);
                if mode != "disabled"
                    && mode != "missing-session"
                    && (mode != "first-only" || entries.len() == 1)
                {
                    let mut retained = entries.clone();
                    if retained.len() == 2 {
                        let last = retained.last_mut().unwrap();
                        if mode == "wrong-text" {
                            set(
                                last.get_mut("message").unwrap(),
                                "content",
                                s("Wrong retained human source text."),
                            );
                        }
                        if mode == "wrong-id" {
                            set(last, "id", s("foreign-native-human"));
                        }
                    }
                    let observed = fixture.observe(&session, &lifecycle, &retained);
                    lifecycle = t(&observed, "lifecycle").to_owned();
                }
                sentinel = prompt
                    .split_whitespace()
                    .find(|word| word.starts_with("OMP_SMOKE_"))
                    .unwrap()
                    .trim_end_matches('.')
                    .to_owned();
                emit(obj([
                    ("type", s("message_end")),
                    (
                        "message",
                        obj([("role", s("assistant")), ("content", s(&sentinel))]),
                    ),
                ]));
                emit(obj([("type", s("session_settled"))]));
            }
            Some("get_state") => {
                if mode == "missing-session" {
                    let _ = fs::remove_file(&path);
                }
                let id = t(&value, "id");
                let late =
                    id.starts_with("observer-progress-") || id.starts_with("observer-final-");
                if mode == "late-duplicate" && late {
                    emit(obj([
                        ("type", s("message_end")),
                        (
                            "message",
                            obj([("role", s("assistant")), ("content", s(&sentinel))]),
                        ),
                    ]));
                }
                emit(obj([
                    ("type", s("response")),
                    ("id", s(id)),
                    (
                        "data",
                        obj([
                            ("sessionId", s(&session)),
                            ("sessionFile", s(path.to_str().unwrap())),
                            (
                                "isSettled",
                                Value::Bool(!(mode == "late-unsettled" && late)),
                            ),
                        ]),
                    ),
                ]));
            }
            _ => {}
        }
    }
}

#[test]
#[ignore = "pack surface: refresh/boundary (Admission)"]
fn test_unsupported_binary_refuses_before_native_launch_or_human_turns() {
    isolated(
        "test_unsupported_binary_refuses_before_native_launch_or_human_turns",
        false,
        |fixture| {
            for version in ["18.14.0", "18.18.9", "18.19.1"] {
                let binary = fixture.binary(version, "disabled");
                let report = fixture.smoke(&binary, 5, 1);
                assert_eq!(get(&report, "native_version"), &s(format!("omp/{version}")));
                assert_eq!(get(&report, "native_requests"), &Value::Array(vec![]));
                assert!(!binary.with_extension("launched").exists());
            }
        },
    );
}

#[test]
#[ignore = "recovery_frontier"]
fn test_historical_observers_cannot_qualify_new_current_session() {
    isolated(
        "test_historical_observers_cannot_qualify_new_current_session",
        true,
        |fixture| {
            let session = uuid();
            let prompts = [
                "First actual current task.".to_owned(),
                "Second actual current task.".to_owned(),
            ];
            let humans = [
                human("current-first", &prompts[0], None),
                human("current-second", &prompts[1], Some("current-first")),
            ];
            // Copied native IDs and prompt bytes in another session are not authority.
            fixture.observe(&uuid(), &uuid(), &humans);
            let (root, path) =
                fixture.native_file("historical-verifier-run", &session, &humans, false);
            let status = fixture.status();
            let actual =
                smoke::current_humans(&fixture.repo, &root, &session, &path, &prompts).unwrap();
            assert_eq!(
                actual.iter().map(|e| t(e, "id")).collect::<Vec<_>>(),
                ["current-first", "current-second"]
            );
            assert!(matches!(
                smoke::current_observations(
                    &fixture.am,
                    &fixture.repo,
                    &root,
                    &session,
                    &path,
                    &prompts,
                    &status,
                    &fixture.env,
                    None
                ),
                Err(smoke::Error::Pending(_))
            ));
            assert!(
                transport::segments(&fixture.site(), &session)
                    .unwrap()
                    .is_empty()
            );
        },
    );
}

#[test]
#[ignore = "recovery_frontier"]
fn test_both_exact_current_human_turns_and_persistent_session_are_required() {
    isolated(
        "test_both_exact_current_human_turns_and_persistent_session_are_required",
        true,
        |fixture| {
            for mode in ["first-only", "wrong-text", "wrong-id", "missing-session"] {
                fixture.smoke(&fixture.binary("18.16.0", mode), 5, 1);
            }
        },
    );
}

#[test]
#[ignore = "recovery_frontier"]
fn test_source_verifier_binds_completed_human_ids_text_roots_and_digests() {
    isolated(
        "test_source_verifier_binds_completed_human_ids_text_roots_and_digests",
        true,
        |fixture| {
            let session = uuid();
            let prompts = [
                "Controlled first human text.".to_owned(),
                "Controlled second human text.".to_owned(),
            ];
            let humans = [
                human("current-first", &prompts[0], None),
                human("current-second", &prompts[1], Some("current-first")),
            ];
            let (root, path) = fixture.native_file("source-verifier-run", &session, &humans, true);
            agent_memory::turn_sources::register(
                &fixture.repo,
                "omp",
                &session,
                &uuid(),
                &"a".repeat(64),
                None,
                None,
                None,
                &fixture.runtime,
            )
            .unwrap();
            let status = fixture.status();
            let verify = |session: &str, prompts: &[String]| {
                smoke::current_observations(
                    &fixture.am,
                    &fixture.repo,
                    &root,
                    session,
                    &path,
                    prompts,
                    &status,
                    &fixture.env,
                    None,
                )
            };
            assert!(verify(&session, &prompts).is_err());
            let first = fixture.observe(&session, &uuid(), &humans[..1]);
            fixture.observe(&session, t(&first, "lifecycle"), &humans);
            fixture.enroll(&session);
            let pending = transport::pending_inventory(&fixture.site()).unwrap().0;
            for row in pending.iter().filter(|row| {
                get(row, "session").as_str() == Some(&session)
                    && get(row, "human_root").as_str().is_some()
            }) {
                let request = obj([
                    ("session", s(&session)),
                    ("lifecycle", get(row, "lifecycle").clone()),
                    ("observation_id", get(row, "observation_id").clone()),
                    ("source_sha256", get(row, "source_sha256").clone()),
                    ("outcome", s("no-learning")),
                    (
                        "assessment",
                        obj([
                            ("source_id", get(row, "observation_id").clone()),
                            (
                                "source_review",
                                s(
                                    "Reviewed the actual ephemeral transport fixture; no durable project learning.",
                                ),
                            ),
                            ("candidates", Value::Array(vec![])),
                        ]),
                    ),
                ]);
                transport::complete(&fixture.repo, &fixture.runtime, &request).unwrap();
            }
            let evidence = verify(&session, &prompts).unwrap();
            assert_eq!(
                evidence
                    .iter()
                    .map(|e| t(e, "native_entry_id"))
                    .collect::<Vec<_>>(),
                ["current-first", "current-second"]
            );
            assert_ne!(
                get(&evidence[0], "human_root"),
                get(&evidence[1], "human_root")
            );
            assert!(verify(&uuid(), &prompts).is_err());
            assert!(verify(&session, &prompts[..1]).is_err());
            let segments = transport::segments(&fixture.site(), &session).unwrap();
            let directory = &segments.last().unwrap().directory;
            let cursor_path = directory.join("cursor.json");
            let original = fs::read(&cursor_path).unwrap();
            let mut cursor = decode(&original);
            set(
                cursor.get_mut("origin").unwrap(),
                "human_root",
                get(&evidence[0], "human_root").clone(),
            );
            private(&cursor_path, &cursor);
            assert!(verify(&session, &prompts).is_err());
            fs::write(&cursor_path, original).unwrap();
            let snapshot = directory
                .join("snapshots")
                .join(format!("{}.json", smoke::sha(b"current-second")));
            fs::write(
                &snapshot,
                fs::read_to_string(&snapshot)
                    .unwrap()
                    .replace(&prompts[1], "Tampered source digest."),
            )
            .unwrap();
            assert!(verify(&session, &prompts).is_err());
        },
    );
}

#[test]
#[ignore = "recovery_frontier"]
fn test_source_verifier_waits_for_fault_free_shared_registration() {
    isolated(
        "test_source_verifier_waits_for_fault_free_shared_registration",
        true,
        |fixture| {
            let session = uuid();
            let prompts = ["Controlled ephemeral source awaiting registration.".to_owned()];
            let human = human("registration-human", &prompts[0], None);
            let lifecycle = transport::uuid5_url(&format!(
                "{}:omp:{session}:registration-human",
                fixture.repo.display()
            ));
            let (root, path) = fixture.native_file(
                "registration-verifier-run",
                &session,
                std::slice::from_ref(&human),
                false,
            );
            let status = fixture.status();
            agent_memory::omp_boundary::verify_marker(&fixture.repo, &fixture.runtime).unwrap();
            let segment = transport::new_segment(
                &fixture.site(),
                &session,
                &lifecycle,
                &lifecycle,
                "human",
                1,
            )
            .unwrap();
            let mut cursor = segment.snapshot().unwrap();
            let captured =
                transport::capture(&segment.directory, &cursor, &human, "verified", None, false)
                    .unwrap()
                    .unwrap();
            set(
                &mut cursor,
                "control_interval",
                obj([("human_entry", s("registration-human"))]),
            );
            set(&mut cursor, "last_native_entry", s("registration-human"));
            transport::replace_cursor(&segment.directory.join("cursor.json"), &cursor).unwrap();
            transport::enroll_admission(&segment.directory, &cursor, Some(&captured)).unwrap();
            let verify = || {
                smoke::current_observations(
                    &fixture.am,
                    &fixture.repo,
                    &root,
                    &session,
                    &path,
                    &prompts,
                    &status,
                    &fixture.env,
                    None,
                )
            };
            assert!(matches!(verify(), Err(smoke::Error::Pending(_))));
            fixture.observe(&session, &lifecycle, std::slice::from_ref(&human));
            let evidence = verify().unwrap();
            assert_eq!(
                evidence
                    .iter()
                    .map(|e| t(e, "native_entry_id"))
                    .collect::<Vec<_>>(),
                ["registration-human"]
            );
            assert_eq!(
                evidence
                    .iter()
                    .map(|e| t(e, "human_root"))
                    .collect::<Vec<_>>(),
                [lifecycle]
            );
        },
    );
}

#[test]
#[ignore = "recovery_frontier"]
fn test_stream_readback_requires_terminal_receipts_and_current_leaf_not_child_quiescence() {
    isolated(
        "test_stream_readback_requires_terminal_receipts_and_current_leaf_not_child_quiescence",
        true,
        |fixture| {
            let session = uuid();
            let human = human("stream-human", "Ephemeral checked source.", None);
            fixture.observe(&session, &uuid(), std::slice::from_ref(&human));
            let (_, native_file) = fixture.native_file(
                "stream-readback",
                &session,
                std::slice::from_ref(&human),
                false,
            );
            let status = fixture.status();
            let verify = |repo: &Path, pending: &[Value]| {
                smoke::current_stream_outcomes(
                    repo,
                    &session,
                    &native_file,
                    &status,
                    &fixture.env,
                    pending,
                )
            };
            let evidence = verify(&fixture.repo, &[]).unwrap();
            assert_eq!(evidence.len(), 1);
            assert_eq!(
                get(&evidence[0], "received_sequence"),
                &Value::Int(1.into())
            );
            assert_eq!(
                get(&evidence[0], "terminal_sequence"),
                &Value::Int(1.into())
            );
            let qualifier = |code: &str| {
                obj([(
                    "qualification",
                    obj([("session", s(&session)), ("code", s(code))]),
                )])
            };
            assert!(matches!(
                verify(&fixture.repo, &[qualifier("missing-outcome")]),
                Err(smoke::Error::Pending(_))
            ));
            assert_eq!(
                verify(&fixture.repo, &[qualifier("open-stream")]).unwrap(),
                evidence
            );
            let mut background = human_entry_assistant("late-background", "Off-branch context.");
            set(&mut background, "activeLeafId", s("stream-human"));
            let late = [
                background,
                obj([
                    ("type", s("model_usage")),
                    ("id", s("late-usage")),
                    ("parentId", s("stream-human")),
                    ("activeLeafId", s("stream-human")),
                ]),
            ];
            append_lines(&native_file, &late);
            assert_eq!(verify(&fixture.repo, &[]).unwrap(), evidence);
            let alias = if let Ok(suffix) = fixture.repo.strip_prefix("/private/tmp") {
                Path::new("/tmp").join(suffix)
            } else {
                fixture.repo.join(".")
            };
            assert_eq!(verify(&alias, &[]).unwrap(), evidence);
            let mut records = vec![human];
            records.extend_from_slice(&late);
            assert_eq!(
                smoke::current_native_leaf(&records).unwrap().as_deref(),
                Some("stream-human")
            );
            let mut null = records.clone();
            null.push(obj([
                ("type", s("custom")),
                ("id", s("explicit-null")),
                ("parentId", Value::Null),
                ("activeLeafId", Value::Null),
            ]));
            assert_eq!(smoke::current_native_leaf(&null).unwrap(), None);
            records.push(obj([
                ("type", s("custom")),
                ("id", s("dangling")),
                ("parentId", Value::Null),
                ("activeLeafId", s("missing")),
            ]));
            assert_eq!(
                smoke::current_native_leaf(&records).unwrap().as_deref(),
                Some("stream-human")
            );
            append_lines(
                &native_file,
                &[human_entry_assistant("uncaptured-leaf", "Later source.")],
            );
            assert!(matches!(
                verify(&fixture.repo, &[]),
                Err(smoke::Error::Pending(_))
            ));
        },
    );
}
fn human_entry_assistant(id: &str, content: &str) -> Value {
    let mut entry = human(id, content, None);
    set(entry.get_mut("message").unwrap(), "role", s("assistant"));
    set(entry.get_mut("message").unwrap(), "attribution", s("agent"));
    entry
}

#[test]
#[ignore = "recovery_frontier"]
fn test_late_native_frames_cannot_reuse_initial_answer_or_settled_proof() {
    isolated(
        "test_late_native_frames_cannot_reuse_initial_answer_or_settled_proof",
        true,
        |fixture| {
            for (version, mode) in [
                ("18.16.0", "complete"),
                ("18.19.0", "complete"),
                ("18.19.0", "late-duplicate"),
                ("18.19.0", "late-unsettled"),
            ] {
                let report = fixture.smoke(
                    &fixture.binary(version, mode),
                    30,
                    if mode == "complete" { 0 } else { 1 },
                );
                if mode == "complete" {
                    let observations = get(&report, "current_human_observations")
                        .as_array()
                        .unwrap();
                    assert_eq!(observations.len(), 2);
                    assert_ne!(
                        get(&observations[0], "human_root"),
                        get(&observations[1], "human_root")
                    );
                } else if mode == "late-duplicate" {
                    assert!(
                        get(&report, "task_sentinels")
                            .as_object()
                            .unwrap()
                            .iter()
                            .any(|(_, n)| n.as_int().unwrap().as_u64().unwrap() > 1)
                    );
                } else {
                    assert_eq!(get(&report, "native_is_settled"), &Value::Bool(false));
                }
            }
        },
    );
}

#[test]
fn leaf_selection_rejects_duplicate_and_malformed_ids_and_preserves_explicit_selection() {
    let first = human("first", "Ephemeral source", None);
    let late = obj([
        ("type", s("model_usage")),
        ("id", s("usage")),
        ("activeLeafId", s("first")),
    ]);
    assert_eq!(
        smoke::current_native_leaf(&[first.clone(), late])
            .unwrap()
            .as_deref(),
        Some("first")
    );
    assert!(smoke::current_native_leaf(&[first.clone(), first]).is_err());
    assert!(
        smoke::current_native_leaf(&[obj([
            ("id", s("entry")),
            ("activeLeafId", Value::Int(3.into()))
        ])])
        .is_err()
    );
    assert!(smoke::current_native_leaf(&[obj([("id", s(""))])]).is_err());
}
#[test]
fn help_preserves_the_native_smoke_argument_surface() {
    let world = World::new().unwrap();
    let output = world
        .run([env!("CARGO_BIN_EXE_am-dev"), "omp-smoke", "--help"])
        .unwrap();
    assert_eq!(output.stdout, smoke::HELP.as_bytes());
    for option in [
        "--binary",
        "--am",
        "--receipt",
        "--repo",
        "--output",
        "--model",
        "--approval-mode",
        "--timeout",
    ] {
        assert!(smoke::HELP.contains(option));
    }
}
#[test]
fn expired_readback_deadline_is_pending_without_launching_a_child() {
    let deadline = agent_memory::clock::Deadline::from_absolute_ns(0);
    assert!(matches!(
        smoke::bounded_timeout(Some(deadline), Duration::from_secs(30)),
        Err(smoke::Error::Pending(_))
    ));
    assert_eq!(
        smoke::bounded_timeout(None, Duration::from_secs(30)).unwrap(),
        Duration::from_secs(30)
    );
}

#[test]
fn stored_current_humans_bind_exact_prompts_header_path_and_distinct_ids() {
    let world = World::new().unwrap();
    let repo = world.repository("repo", false).unwrap();
    let root = world.inside("run").unwrap();
    fs::create_dir_all(root.join("sessions")).unwrap();
    let path = root.join("sessions/current.jsonl");
    let session = uuid();
    let prompts = vec![
        "First exact task.".to_owned(),
        "Second exact task.".to_owned(),
    ];
    let first = human("first", &prompts[0], None);
    let second = human("second", &prompts[1], Some("first"));
    let header = obj([
        ("type", s("session")),
        ("id", s(&session)),
        ("cwd", s(repo.to_str().unwrap())),
    ]);
    let title = obj([("type", s("title")), ("title", s("Native title"))]);
    write_lines(
        &path,
        &[title, header.clone(), first.clone(), second.clone()],
    );
    let actual = smoke::current_humans(&repo, &root, &session, &path, &prompts).unwrap();
    assert_eq!(
        encoded(&Value::Array(actual)),
        encoded(&Value::Array(vec![first.clone(), second.clone()]))
    );
    assert!(smoke::current_humans(&repo, &root, &uuid(), &path, &prompts).is_err());
    assert!(smoke::current_humans(&repo, &root, &session, &path, &prompts[..1]).is_err());
    let outside = world.inside("outside.jsonl").unwrap();
    fs::copy(&path, &outside).unwrap();
    assert!(smoke::current_humans(&repo, &root, &session, &outside, &prompts).is_err());
    let mut duplicate = second.clone();
    set(&mut duplicate, "id", s("first"));
    write_lines(&path, &[header.clone(), first.clone(), duplicate]);
    assert!(smoke::current_humans(&repo, &root, &session, &path, &prompts).is_err());
    let mut fork = header.clone();
    set(&mut fork, "parentSession", s(uuid()));
    write_lines(&path, &[fork, first.clone(), second.clone()]);
    assert!(smoke::current_humans(&repo, &root, &session, &path, &prompts).is_err());
    write_lines(
        &path,
        &[first.clone(), header.clone(), first.clone(), second.clone()],
    );
    assert!(smoke::current_humans(&repo, &root, &session, &path, &prompts).is_err());
    write_lines(&path, &[header.clone(), header, first, second]);
    assert!(smoke::current_humans(&repo, &root, &session, &path, &prompts).is_err());
}

#[test]
fn source_digest_and_content_text_follow_baseline_canonical_bytes() {
    let value = obj([("z", s("é")), ("a", Value::Int(1.into()))]);
    assert_eq!(
        smoke::source_digest(&value).unwrap(),
        smoke::sha("{\"a\":1,\"z\":\"é\"}".as_bytes())
    );
    assert_eq!(
        smoke::content_text(&Value::Array(vec![
            obj([("type", s("text")), ("text", s("one"))]),
            obj([("type", s("text")), ("text", s("two"))]),
        ])),
        Some("onetwo".into())
    );
    assert_eq!(
        smoke::content_text(&Value::Array(vec![obj([
            ("type", s("toolCall")),
            ("text", s("not human text"))
        ])])),
        None
    );
}
