//! Shared literal baseline fixtures for the fanout contract tests.
use super::{Child, RunOptions, World};
use agent_memory::bank_contract;
use agent_memory::codec::json::{self, Value};
use agent_memory::process::{Output, Status};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PROJECT: &str = "11111111-1111-4111-8111-111111111111";
pub const MARKER: &str = ".agent-memory-transaction.md";
pub fn text(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
pub fn object(items: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.get(key).unwrap_or(&Value::Null)
}
pub fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    get(v, key).as_str().unwrap()
}
pub fn yes(v: &Value, key: &str) -> bool {
    get(v, key).as_bool() == Some(true)
}
pub fn put(v: &mut Value, key: &str, value: Value) {
    let Value::Object(items) = v else {
        panic!("fixture object")
    };
    if let Some((_, old)) = items.iter_mut().find(|(k, _)| k == key) {
        *old = value;
    } else {
        items.push((key.to_owned(), value));
    }
}
pub fn save(w: &World, value: &Value, name: &str) -> PathBuf {
    let path = w.inside(name).unwrap();
    fs::write(&path, json::encode(value, json::DEFAULT).unwrap()).unwrap();
    path
}
pub fn command(w: &World, helper: &str, args: &[OsString]) -> (Output, Value) {
    command_fault(w, helper, args, &[])
}
pub fn command_fault(
    w: &World,
    helper: &str,
    args: &[OsString],
    env: &[(OsString, OsString)],
) -> (Output, Value) {
    let translated = super::world::helper_arguments(helper, args).unwrap();
    let argv = std::iter::once(OsString::from("--json")).chain(translated);
    let output = w
        .am_with(
            argv,
            RunOptions {
                check: false,
                env,
                ..RunOptions::default()
            },
        )
        .unwrap();
    let value = if output.stdout.is_empty() {
        Value::Null
    } else {
        json::decode_bytes(&output.stdout, Default::default())
            .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&output.stdout)))
    };
    (output, value)
}
pub fn good(w: &World, helper: &str, args: &[OsString]) -> Value {
    let (output, value) = command(w, helper, args);
    assert_eq!(
        output.status,
        Status::Code(0),
        "{value:?}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    value
}
pub fn refused(w: &World, helper: &str, args: &[OsString]) -> Value {
    let (output, value) = command(w, helper, args);
    assert_eq!(output.status, Status::Code(1), "{value:?}");
    value
}
pub fn render(contexts: &[(&str, &str)], time: &str) -> Value {
    let request = object([
        ("project-id", text(PROJECT)),
        ("router-updated", text(time)),
        (
            "contexts",
            object(contexts.iter().map(|(path, body)| {
                (
                    *path,
                    object([
                        ("body", text(*body)),
                        ("purpose", text("Fixture context")),
                        ("scope", text("repository")),
                        ("lifecycle", text("active")),
                        ("read-when", text("When resuming")),
                    ]),
                )
            })),
        ),
        (
            "omitted",
            object(
                bank_contract::CONTEXTS
                    .iter()
                    .filter(|p| !contexts.iter().any(|(name, _)| name == *p))
                    .map(|p| (*p, text("No distinct evidence"))),
            ),
        ),
    ]);
    get(&bank_contract::render(&request).unwrap(), "files").clone()
}
pub fn write_files(bank: &Path, files: &Value) {
    for (path, body) in files.as_object().unwrap() {
        let target = bank.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, body.as_str().unwrap()).unwrap();
    }
}
pub fn physical(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        if !dir.exists() {
            return;
        }
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let info = fs::symlink_metadata(&path).unwrap();
            if info.is_dir() {
                visit(root, &path, result);
            } else if info.is_file() {
                result.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    fs::read(path).unwrap(),
                );
            } else if info.file_type().is_symlink() {
                result.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    fs::read_link(path)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
pub fn canonical(bank: &Path) -> Value {
    object(
        physical(bank)
            .into_iter()
            .filter(|(p, _)| {
                p.ends_with(".md") && !p.split('/').any(|c| c.starts_with(".agent-memory"))
            })
            .map(|(p, b)| (p, text(String::from_utf8(b).unwrap()))),
    )
}
pub fn same_map(a: &Value, b: &Value) {
    let map = |v: &Value| {
        v.as_object()
            .unwrap()
            .iter()
            .cloned()
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(map(a), map(b));
}
pub fn optional(files: &Value, additions: &[(String, String)]) -> Value {
    let mut result = files.clone();
    let mut rows = String::new();
    for (path, body) in additions {
        let body = if body.starts_with("---\n") {
            body.clone()
        } else {
            super::record_fixtures::record_text(body, PROJECT).unwrap()
        };
        put(&mut result, path, text(body));
        rows.push_str(&format!(
            "| [{path}](<{path}>) | Context | repository | active | When relevant |\n"
        ));
    }
    let router = string(&result, "MEMORY.md").replace(
        "\n## Omitted contexts\n",
        &format!("{rows}\n## Omitted contexts\n"),
    );
    put(&mut result, "MEMORY.md", text(router));
    result
}
pub struct Bank {
    pub world: World,
    pub repo: PathBuf,
    pub bank: PathBuf,
    pub before: Value,
    pub after: Value,
    pub prepared: Value,
}
impl Bank {
    pub fn new() -> Self {
        let world = World::new().unwrap();
        let repo = world.repository("repo", false).unwrap();
        let bank = repo.join("memory-bank");
        fs::create_dir(&bank).unwrap();
        let before = render(
            &[
                ("activeContext.md", "# Focus\n\nOriginal task.\n"),
                (
                    "projectbrief.md",
                    "# Brief\n\nOLD_CONTENT_CANARY to prune.\n",
                ),
                (
                    "techContext.md",
                    "# Technical\n\nUnchanged applicability.\n",
                ),
            ],
            "2026-10-22T18:00:00Z",
        );
        let after = render(
            &[
                ("activeContext.md", "# Focus\n\nNew task and conditions.\n"),
                (
                    "progress.md",
                    "# Progress\n\nVerified result; evidence remains bounded.\n",
                ),
                ("techContext.md", string(&before, "techContext.md")),
            ],
            "2026-10-22T19:00:00Z",
        );
        write_files(&bank, &before);
        super::identity_fixtures::bind(&world, &repo).unwrap();
        let mut value = Self {
            world,
            repo,
            bank,
            before,
            after,
            prepared: Value::Null,
        };
        value.prepared = value.preview(&value.after);
        value
    }
    pub fn preview(&self, files: &Value) -> Value {
        good(
            &self.world,
            "bank_transactions.py",
            &[
                "preview".into(),
                "--repo".into(),
                self.repo.as_os_str().to_owned(),
                "--files".into(),
                save(&self.world, files, "files.json").into_os_string(),
                "--operation".into(),
                "compact".into(),
            ],
        )
    }
    pub fn apply(&self, prepared: &Value) -> (Output, Value) {
        command(
            &self.world,
            "bank_transactions.py",
            &[
                "apply".into(),
                "--proposal".into(),
                save(&self.world, prepared, "proposal.json").into_os_string(),
            ],
        )
    }
    pub fn crash(&self, prepared: &Value, stage: &str) {
        let env = [
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "exit".into()),
        ];
        let (output, _) = command_fault(
            &self.world,
            "bank_transactions.py",
            &[
                "apply".into(),
                "--proposal".into(),
                save(&self.world, prepared, "crash.json").into_os_string(),
            ],
            &env,
        );
        assert_eq!(output.status, Status::Code(86));
    }
    pub fn unlock(&self) {
        let args = [
            "recover".into(),
            "--repo".into(),
            self.repo.as_os_str().to_owned(),
        ];
        let p = good(&self.world, "bank_writes.py", &args);
        let receipt = good(
            &self.world,
            "bank_writes.py",
            &[
                args[0].clone(),
                args[1].clone(),
                args[2].clone(),
                "--proposal".into(),
                save(&self.world, &p, "unlock.json").into_os_string(),
            ],
        );
        assert!(yes(&receipt, "verified"));
    }
    pub fn recovery(&self, mode: &str) -> Value {
        let p = good(
            &self.world,
            "bank_transactions.py",
            &[
                "recover".into(),
                "--repo".into(),
                self.repo.as_os_str().to_owned(),
                "--mode".into(),
                mode.into(),
            ],
        );
        assert!(!yes(&p, "writes_performed"));
        assert!(!self.bank.join(".agent-memory.lock").exists());
        p
    }
    pub fn recover(&self, prepared: &Value, env: &[(OsString, OsString)]) -> (Output, Value) {
        command_fault(
            &self.world,
            "bank_transactions.py",
            &[
                "recover".into(),
                "--repo".into(),
                self.repo.as_os_str().to_owned(),
                "--proposal".into(),
                save(&self.world, prepared, "recover.json").into_os_string(),
            ],
            env,
        )
    }
    pub fn pause(&self, prepared: &Value, stage: &str) -> (Child, PathBuf) {
        let pause = self.world.inside("rendezvous").unwrap();
        let env = [
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
            (
                "AGENT_MEMORY_FAULT_DIR".into(),
                pause.as_os_str().to_owned(),
            ),
        ];
        let child = self
            .world
            .spawn_with(
                [
                    OsString::from(env!("CARGO_BIN_EXE_am")),
                    "--json".into(),
                    "transaction".into(),
                    "apply".into(),
                    "--proposal".into(),
                    save(&self.world, prepared, "paused.json").into_os_string(),
                ],
                RunOptions {
                    check: false,
                    env: &env,
                    ..RunOptions::default()
                },
            )
            .unwrap();
        let leaf = stage.replace('/', "%2F");
        let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(5)).unwrap();
        while !pause.join(format!("{leaf}.ready")).exists() {
            assert!(!deadline.expired(), "missing rendezvous {stage}");
            std::thread::sleep(Duration::from_millis(5));
        }
        (child, pause.join(format!("{leaf}.release")))
    }
    pub fn feature(&self) -> (Vec<u8>, Vec<u8>) {
        fs::write(
            self.repo.join("feature.txt"),
            b"Unfinished feature must remain intact.\n",
        )
        .unwrap();
        self.world
            .git_with(
                ["add", "feature.txt"],
                RunOptions {
                    cwd: Some(&self.repo),
                    ..RunOptions::default()
                },
            )
            .unwrap();
        (
            fs::read(self.repo.join(".git/index")).unwrap(),
            fs::read(self.repo.join(".git/HEAD")).unwrap(),
        )
    }
    pub fn assert_no_transaction_state(&self) {
        let state = self.world.paths["state"].join("agent-memory");
        for path in physical(&state).keys() {
            assert!(
                !path.ends_with("journal.json") && !path.contains("/operations/"),
                "unexpected transaction evidence: {path}"
            );
        }
    }
}
pub fn fifo(path: &Path) {
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_fifo());
}
pub fn private(path: &Path) {
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

pub struct Recall {
    pub world: World,
    pub repo: PathBuf,
    pub bank: PathBuf,
}
impl Recall {
    pub fn new() -> Self {
        let world = World::new().unwrap();
        let repo = world.repository("repo", false).unwrap();
        fs::write(repo.join("app.rs"), "const RETRY_LIMIT: u32 = 3;\n").unwrap();
        fs::write(
            repo.join("README.md"),
            "# Retry project\n\nInvestigate before generalizing.\n",
        )
        .unwrap();
        world
            .git_with(
                ["add", "."],
                RunOptions {
                    cwd: Some(&repo),
                    ..RunOptions::default()
                },
            )
            .unwrap();
        world
            .git_with(
                ["commit", "-m", "Disposable implementation evidence"],
                RunOptions {
                    cwd: Some(&repo),
                    ..RunOptions::default()
                },
            )
            .unwrap();
        let bank = repo.join("memory-bank");
        fs::create_dir(&bank).unwrap();
        write_files(
            &bank,
            &render(
                &[
                    (
                        "projectbrief.md",
                        "# Project brief\n\nGoals and boundaries: [project source](../README.md).\n",
                    ),
                    (
                        "activeContext.md",
                        "# Active context\n\nRetry project orientation; independently check source facts.\n",
                    ),
                ],
                "2026-10-22T17:00:00Z",
            ),
        );
        super::identity_fixtures::bind(&world, &repo).unwrap();
        fs::write(
            repo.join("app.rs"),
            "const RETRY_LIMIT: u32 = 4; // staged unrelated work\n",
        )
        .unwrap();
        world
            .git_with(
                ["add", "app.rs"],
                RunOptions {
                    cwd: Some(&repo),
                    ..RunOptions::default()
                },
            )
            .unwrap();
        Self { world, repo, bank }
    }
    pub fn add(&self, name: &str, body: &str, updates: &Value) -> String {
        let raw = super::record_fixtures::record_text(body, PROJECT).unwrap();
        let (header, _) = raw
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap();
        let mut metadata = object(header.lines().map(|line| {
            let (key, value) = line.split_once(": ").unwrap();
            (key, json::decode(value, Default::default()).unwrap())
        }));
        for (key, value) in updates.as_object().unwrap() {
            put(&mut metadata, key, value.clone());
        }
        let scope = string(&metadata, "scope").to_owned();
        let status = string(&metadata, "status").to_owned();
        let mut rendered = String::from("---\n");
        for (key, value) in metadata.as_object().unwrap() {
            rendered.push_str(&format!(
                "{key}: {}\n",
                json::encode(value, json::DEFAULT).unwrap()
            ));
        }
        rendered.push_str("---\n");
        rendered.push_str(body);
        let path = if name.contains('/') {
            name.to_owned()
        } else {
            format!("topics/{name}.md")
        };
        let target = self.bank.join(&path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, rendered).unwrap();
        let router = self.bank.join("MEMORY.md");
        let row = format!(
            "| [{path}](<{path}>) | Conditional lesson | {scope} | {status} | Before qualified recall |\n"
        );
        fs::write(
            &router,
            fs::read_to_string(&router).unwrap().replace(
                "\n## Omitted contexts",
                &format!("{row}\n## Omitted contexts"),
            ),
        )
        .unwrap();
        path
    }
    pub fn request(&self, paths: &[String], context: Value) -> Value {
        object([
            ("project_id", text(PROJECT)),
            ("query", text("")),
            ("ids", Value::Array(vec![])),
            ("paths", Value::Array(paths.iter().map(text).collect())),
            ("context", context),
            (
                "kinds",
                Value::Array(vec![text("semantic-fact"), text("project-decision")]),
            ),
            ("minimum_trust", text("reviewed")),
            ("limit", Value::Int(8.into())),
            ("include_context", Value::Bool(true)),
        ])
    }
    pub fn retrieve(&self, request: &Value) -> (Output, Value) {
        command(
            &self.world,
            "retrieve.py",
            &[
                "--repo".into(),
                self.repo.as_os_str().to_owned(),
                "--request".into(),
                save(&self.world, request, "recall-request.json").into_os_string(),
            ],
        )
    }
    pub fn code(&self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        (
            fs::read(self.repo.join("app.rs")).unwrap(),
            fs::read(self.repo.join(".git/index")).unwrap(),
            self.world
                .git_with(
                    ["rev-parse", "HEAD"],
                    RunOptions {
                        cwd: Some(&self.repo),
                        ..RunOptions::default()
                    },
                )
                .unwrap()
                .stdout,
        )
    }
    pub fn pause(&self, request: &Value, stage: &str) -> (Child, PathBuf) {
        let directory = self.world.inside("recall-rendezvous").unwrap();
        let env = [
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
            (
                "AGENT_MEMORY_FAULT_DIR".into(),
                directory.as_os_str().to_owned(),
            ),
        ];
        let child = self
            .world
            .spawn_with(
                [
                    OsString::from(env!("CARGO_BIN_EXE_am")),
                    "--json".into(),
                    "search".into(),
                    "--repo".into(),
                    self.repo.as_os_str().to_owned(),
                    "--request".into(),
                    save(&self.world, request, "paused-recall.json").into_os_string(),
                ],
                RunOptions {
                    check: false,
                    env: &env,
                    ..RunOptions::default()
                },
            )
            .unwrap();
        let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(5)).unwrap();
        while !directory.join(format!("{stage}.ready")).exists() {
            assert!(!deadline.expired(), "missing recall rendezvous");
            std::thread::sleep(Duration::from_millis(5));
        }
        (child, directory.join(format!("{stage}.release")))
    }
}

/// Re-executed `am` library dispatch with the source checkout's pack.
/// Tests using this helper define `cutover_dispatch_worker` and delegate to it.
pub fn dispatch_worker() {
    let Some(input) = std::env::var_os("AGENT_MEMORY_FAULT_DISPATCH_INPUT") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    agent_memory::cli::use_development_pack(Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
    ));
    let request = json::decode_bytes(&fs::read(input).unwrap(), Default::default()).unwrap();
    let args: Vec<_> = get(&request, "args")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    let output = if let Some(name) = args
        .first()
        .and_then(|first| (first == "hook").then(|| args.get(1)))
        .flatten()
    {
        let raw = json::encode(get(&request, "event"), json::DEFAULT).unwrap();
        let result =
            agent_memory::hooks::run(name, raw.as_bytes(), &agent_memory::cli::pack_root).unwrap();
        object([
            ("exit_code", Value::Int(result.exit_code.into())),
            ("stdout", text(result.stdout)),
            ("stderr", text(result.stderr)),
        ])
    } else {
        let result = agent_memory::cli::dispatch(&args, "am").result;
        object([
            ("exit_code", Value::Int(result.exit_code.into())),
            ("stdout", text(result.stdout)),
            ("stderr", text(result.stderr)),
        ])
    };
    println!(
        "AM-FANOUT-OUTPUT:{}",
        json::encode(&output, json::COMPACT_ASCII).unwrap()
    );
}

pub fn dispatch(w: &World, args: &[OsString], event: Option<&Value>) -> (i32, String, String) {
    let request = object([
        (
            "args",
            Value::Array(args.iter().map(|s| text(s.to_str().unwrap())).collect()),
        ),
        ("event", event.cloned().unwrap_or(Value::Null)),
    ]);
    let input = save(w, &request, "dispatch-input.json");
    let env = [(
        "AGENT_MEMORY_FAULT_DISPATCH_INPUT".into(),
        input.into_os_string(),
    )];
    let output = w
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "cutover_dispatch_worker".into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            RunOptions {
                env: &env,
                ..RunOptions::default()
            },
        )
        .unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let raw = stdout
        .lines()
        .find_map(|line| line.split_once("AM-FANOUT-OUTPUT:").map(|(_, raw)| raw))
        .unwrap_or_else(|| panic!("dispatch worker produced no receipt: {stdout}"));
    let result = json::decode(raw, Default::default()).unwrap();
    (
        get(&result, "exit_code")
            .as_int()
            .unwrap()
            .as_i64()
            .unwrap() as i32,
        string(&result, "stdout").to_owned(),
        string(&result, "stderr").to_owned(),
    )
}

pub fn discovery_trace(r: &Recall, paths: &[String], intent: &str) -> Value {
    let value = good(
        &r.world,
        "discover_memory.py",
        &[
            "start".into(),
            "--repo".into(),
            r.repo.as_os_str().to_owned(),
            "--intent".into(),
            intent.into(),
            "--task".into(),
            "Determine the permitted number of attempts after a temporary failure.".into(),
        ],
    );
    let mut trace = get(&value, "discovery").clone();
    assert!(!yes(&trace, "complete"));
    assert!(!yes(&value, "usable_as_current_guidance"));
    let sources = paths
        .iter()
        .map(|path| {
            let raw = fs::read(r.bank.join(path)).unwrap();
            object([
                ("path", text(path)),
                (
                    "sha256",
                    text(agent_memory::developer_identity::digest(Some(&raw)).unwrap()),
                ),
            ])
        })
        .collect();
    put(&mut trace, "sources", Value::Array(sources));
    put(&mut trace, "complete", Value::Bool(true));
    put(
        &mut trace,
        "rationale",
        text(
            "Selected plausible meaning-bearing routes; inspect all actual source conditions and evidence.",
        ),
    );
    trace
}

pub fn development_source(w: &World, name: &str) -> PathBuf {
    let source = w.inside(name).unwrap();
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        &source.join("skills/agent-memory"),
    );
    fs::create_dir_all(source.join("target/release")).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_am"), source.join("target/release/am")).unwrap();
    source
}
pub fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    fs::set_permissions(target, fs::Permissions::from_mode(0o755)).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap();
        if name == "__pycache__" || path.extension().is_some_and(|e| e == "pyc" || e == "pyo") {
            continue;
        }
        let info = fs::symlink_metadata(&path).unwrap();
        if info.is_dir() {
            copy_tree(&path, &target.join(name));
        } else if info.is_file() {
            fs::copy(&path, target.join(name)).unwrap();
        } else {
            panic!("portable fixture unexpectedly contains a link");
        }
    }
}
