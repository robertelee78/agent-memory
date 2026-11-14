use agent_memory::codec::json::{self, Value};
use agent_memory::developer_identity::{encoded, load};
use agent_memory::error::Result;
use agent_memory::host::{self, Sources};
use agent_memory::project_identity::{self as project, DiscoveryOptions};
use agent_memory::{bank_contract, clock::Deadline, process, repository_config};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn obj(values: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(values.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn g<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn st<'a>(value: &'a Value, key: &str) -> &'a str {
    g(value, key).as_str().unwrap()
}
fn set(value: &mut Value, key: &str, item: Value) {
    let Value::Object(items) = value else {
        panic!("fixture object")
    };
    if let Some((_, old)) = items.iter_mut().find(|(k, _)| k == key) {
        *old = item;
    } else {
        items.push((key.into(), item));
    }
}
fn remove(value: &mut Value, key: &str) -> Value {
    let Value::Object(items) = value else {
        panic!("fixture object")
    };
    let index = items.iter().position(|(k, _)| k == key).unwrap();
    items.remove(index).1
}
fn push(value: &mut Value, item: Value) {
    let Value::Array(items) = value else {
        panic!("fixture array")
    };
    items.push(item);
}
fn fixed_time() -> Result<String> {
    Ok("2026-10-22T18:00:00+00:00".into())
}
fn sources() -> Sources {
    Sources {
        uuid4: host::uuid4,
        utc_now: fixed_time,
    }
}

// Each case executes in its own real process with a private HOME/config. No
// process-global environment mutation or dependence on the user's Git config.
fn command(argv: &[OsString], env: &[(OsString, OsString)], cwd: Option<&Path>) -> process::Output {
    let spec = process::Spec {
        argv,
        env,
        cwd,
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
    process::run_for(&spec, Duration::from_secs(120)).unwrap()
}
struct LabelScan;
impl repository_config::SecurityScan for LabelScan {
    fn has_reasons(&self, value: &str) -> Result<bool> {
        // Real deterministic scan for the boundary labels used by these tests.
        Ok(value.to_ascii_lowercase().contains("password=")
            || value.contains("PRIVATE KEY")
            || value.contains("token="))
    }
    fn scan(&self, _body: &str) -> Result<agent_memory::codec::json::Value> {
        // Body scanning belongs to the security module; this label fixture refuses it.
        Err(agent_memory::error::ContractError::new(
            "Security body scan is not available in this label fixture",
        ))
    }
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
                "XDG_CONFIG_HOME".into(),
                self.root.join("xdg-config").into_os_string(),
            ),
            (
                "XDG_STATE_HOME".into(),
                self.root.join("state").into_os_string(),
            ),
            (
                "AGENT_MEMORY_CONFIG_HOME".into(),
                self.root.join("config with spaces").into_os_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("LC_ALL".into(), "C".into()),
        ]
    }
    fn git(&self, repo: &Path, args: &[&OsStr]) -> String {
        let mut argv = vec![
            OsString::from("git"),
            "-C".into(),
            repo.as_os_str().to_owned(),
        ];
        argv.extend(args.iter().map(|v| v.to_os_string()));
        let output = command(&argv, &self.env(), None);
        assert_eq!(
            output.status,
            process::Status::Code(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
    fn run(&self, repo: &Path, args: &[&str]) -> String {
        self.git(repo, &args.iter().map(OsStr::new).collect::<Vec<_>>())
    }
    fn repo(&self, name: &str) -> PathBuf {
        let repo = self.root.join(name);
        std::fs::create_dir(&repo).unwrap();
        self.run(&repo, &["init", "--initial-branch=main"]);
        self.run(&repo, &["config", "user.name", "Fixture"]);
        self.run(&repo, &["config", "user.email", "fixture@example.invalid"]);
        repo
    }
    fn remote(&self, repo: &Path, name: &str, url: &str) {
        self.run(repo, &["remote", "add", name, url]);
    }
    fn status(&self, repo: &Path) -> Value {
        project::status(repo).unwrap()
    }
    fn preview(&self, repo: &Path, action: &str, authority: Option<&str>) -> Value {
        project::preview_with_sources(
            repo,
            action,
            None,
            "Explicit fixture decision",
            authority,
            sources(),
        )
        .unwrap()
    }
    fn apply(&self, proposal: &Value) -> Value {
        project::apply_with_sources(proposal, sources()).unwrap()
    }
    fn registry_path(&self) -> PathBuf {
        self.root.join("config with spaces/projects.json")
    }
    fn registry(&self) -> Value {
        load(&std::fs::read(self.registry_path()).unwrap()).unwrap()
    }
    fn save_registry(&self, data: &Value) {
        std::fs::write(self.registry_path(), encoded(data).unwrap()).unwrap();
    }
    fn bank(&self, repo: &Path, identity: &str, remote: Option<&str>) {
        self.bank_at(repo, "memory-bank", identity, remote);
    }
    fn bank_at(&self, repo: &Path, relative: &str, identity: &str, remote: Option<&str>) {
        let proposal = obj([
            ("project-id", s(identity)),
            ("router-updated", s("2026-10-22T18:00:00Z")),
            (
                "contexts",
                obj([(
                    "projectbrief.md",
                    obj([
                        (
                            "body",
                            s("# Project\n\nFixture context with [source](../README.md)."),
                        ),
                        ("purpose", s("Goals")),
                        ("scope", s("repository")),
                        ("lifecycle", s("active")),
                        ("read-when", s("When starting")),
                    ]),
                )]),
            ),
            (
                "omitted",
                obj([
                    "productContext.md",
                    "activeContext.md",
                    "systemPatterns.md",
                    "techContext.md",
                    "progress.md",
                ]
                .map(|name| (name, s("No relevant evidence supplied")))),
            ),
        ]);
        let mut proposal = proposal;
        if let Some(remote) = remote {
            set(&mut proposal, "canonical-remote", s(remote));
        }
        let files = bank_contract::render(&proposal).unwrap();
        let bank = repo.join(relative);
        std::fs::create_dir_all(&bank).unwrap();
        for (name, body) in g(&files, "files").as_object().unwrap() {
            std::fs::write(bank.join(name), body.as_str().unwrap()).unwrap();
        }
    }
    fn create(&self, repo: &Path, authority: Option<&str>) -> (String, Value) {
        let proposal = self.preview(repo, "create", authority);
        let receipt = self.apply(&proposal);
        assert_eq!(g(&receipt, "verified"), &Value::Bool(true));
        assert_eq!(
            g(g(&receipt, "binding_status"), "resolved"),
            &Value::Bool(false)
        );
        let identity = st(&proposal, "project_id").to_owned();
        self.bank(
            repo,
            &identity,
            g(
                g(g(&proposal, "proposal"), "bank_identity"),
                "canonical-remote",
            )
            .as_str(),
        );
        (identity, proposal)
    }
    fn commit(&self, repo: &Path) {
        self.run(repo, &["add", "."]);
        self.run(repo, &["commit", "-m", "Fixture bank"]);
    }
    fn historical(&self, repo: &Path) -> (String, String) {
        let current = st(g(&self.status(repo), "evidence"), "checkout_key").to_owned();
        let (device, inode) = current.split_once(':').unwrap();
        let old = format!("{}:{inode}", device.parse::<u64>().unwrap() + 100);
        let mut data = self.registry();
        let entry = remove(data.get_mut("checkouts").unwrap(), &current);
        set(data.get_mut("checkouts").unwrap(), &old, entry);
        self.save_registry(&data);
        (current, old)
    }
    fn snapshot(&self, repo: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn visit(path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            if !path.exists() {
                return;
            }
            for item in std::fs::read_dir(path).unwrap() {
                let item = item.unwrap();
                let kind = item.file_type().unwrap();
                if kind.is_dir() {
                    visit(&item.path(), files);
                } else if kind.is_file() {
                    files.insert(item.path(), std::fs::read(item.path()).unwrap());
                }
            }
        }
        let mut result = BTreeMap::new();
        visit(repo, &mut result);
        visit(&self.root.join("config with spaces"), &mut result);
        visit(&self.root.join("state"), &mut result);
        result
    }
}
static OUTER_LAUNCH: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn in_world(name: &str, body: impl FnOnce(&World)) {
    if std::env::var("AM_PROJECT_CASE").ok().as_deref() == Some(name) {
        repository_config::register_security_scan(Some(Arc::new(LabelScan))).unwrap();
        let world = World {
            root: PathBuf::from(std::env::var_os("AM_PROJECT_ROOT").unwrap()),
        };
        body(&world);
        return;
    }
    // T0's macOS pre-fork FD snapshot can overflow under outer test fanout.
    // Keep fixture launches serialized; the registration race inside its child
    // still runs concurrently with independent flock descriptors.
    let _launch = OUTER_LAUNCH.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::env::temp_dir().join(format!("am-project-{}", host::uuid4().unwrap()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap();
    let world = World {
        root: std::fs::canonicalize(&path).unwrap(),
    };
    std::fs::create_dir(world.root.join("home")).unwrap();
    let shims = world.root.join("shims");
    std::fs::create_dir(&shims).unwrap();
    let shim = shims.join("git");
    std::fs::write(&shim, "#!/bin/sh\ncase \"$*\" in\n  *' rev-parse --show-toplevel --verify --quiet HEAD')\n    if [ -f \"$AM_PROJECT_ROOT/head-output\" ]; then\n      /bin/cat \"$AM_PROJECT_ROOT/head-output\"\n      code=$(/bin/cat \"$AM_PROJECT_ROOT/head-code\")\n      case \"$code\" in signal*) kill -\"${code#signal}\" $$ ;; *) exit \"$code\" ;; esac\n    fi ;;\nesac\nexec /usr/bin/git \"$@\"\n").unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut env = world.env();
    if let Some((_, path)) = env.iter_mut().find(|(key, _)| key == "PATH") {
        *path = OsString::from(format!("{}:/usr/bin:/bin", shims.display()));
    }
    if name == "ambient_git_authority_and_fsmonitor_are_ignored" {
        env.extend([
            (
                "GIT_DIR".into(),
                world.root.join("foreign/.git").into_os_string(),
            ),
            (
                "GIT_WORK_TREE".into(),
                world.root.join("foreign").into_os_string(),
            ),
            (
                "GIT_INDEX_FILE".into(),
                world.root.join("unwanted-index").into_os_string(),
            ),
            ("GIT_CONFIG_COUNT".into(), "1".into()),
            ("GIT_CONFIG_KEY_0".into(), "agent-memory.bankRoot".into()),
            ("GIT_CONFIG_VALUE_0".into(), "foreign-bank".into()),
            (
                "GIT_TRACE2_EVENT".into(),
                world.root.join("unwanted-trace").into_os_string(),
            ),
        ]);
    }
    env.push(("AM_PROJECT_CASE".into(), name.into()));
    env.push(("AM_PROJECT_ROOT".into(), world.root.as_os_str().to_owned()));
    let argv = [
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        name.into(),
        "--nocapture".into(),
    ];
    let output = command(&argv, &env, None);
    std::fs::remove_dir_all(&world.root).unwrap();
    assert_eq!(
        output.status,
        process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_bootstrap_is_read_only_until_explicit_local_registration_and_bank_initialization
#[test]
fn bootstrap_readonly_then_explicit_registration() {
    in_world("bootstrap_readonly_then_explicit_registration", |w| {
        let repo = w.repo("repository");
        let status = w.status(&repo);
        assert_eq!(g(&status, "project_id"), &Value::Null);
        assert_eq!(g(&status, "resolved"), &Value::Bool(false));
        assert!(!w.registry_path().parent().unwrap().exists());
        let proposal = w.preview(&repo, "create", None);
        assert!(!w.registry_path().parent().unwrap().exists());
        assert!(agent_memory::developer_identity::uuid_valid(g(
            &proposal,
            "project_id"
        )));
        let receipt = w.apply(&proposal);
        assert_eq!(
            g(&receipt, "changed_memory_paths"),
            &Value::Array(Vec::new())
        );
        assert!(!repo.join("memory-bank").exists());
        assert!(
            json::encode(g(&receipt, "binding_status"), json::DEFAULT)
                .unwrap()
                .contains("pending")
        );
        w.bank(&repo, st(&proposal, "project_id"), None);
        assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
        assert!(!w.registry_path().with_file_name("developers.json").exists());
        assert_eq!(
            std::fs::metadata(w.registry_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(w.registry_path().parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_device_change_recovers_readonly_and_explicit_link_persists_alias
#[test]
fn device_change_recovers_and_link_persists_alias() {
    in_world("device_change_recovers_and_link_persists_alias", |w| {
        let repo = w.repo("repository");
        w.remote(&repo, "origin", "https://github.com/example/project.git");
        let (id, _) = w.create(&repo, None);
        let (current, old) = w.historical(&repo);
        let before = std::fs::read(w.registry_path()).unwrap();
        let status = w.status(&repo);
        assert_eq!(g(&status, "resolved"), &Value::Bool(true));
        assert_eq!(st(&status, "project_id"), id);
        assert_eq!(st(g(&status, "evidence"), "checkout_key"), old);
        assert_eq!(st(g(&status, "evidence"), "observed_checkout_key"), current);
        assert_eq!(
            st(g(&status, "evidence"), "binding_source"),
            "historical-device-change"
        );
        assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
        let history = g(g(&status, "checkout_entry"), "history")
            .as_array()
            .unwrap();
        let receipt = w.apply(&w.preview(&repo, "link", None));
        assert_eq!(
            g(g(&receipt, "binding_status"), "resolved"),
            &Value::Bool(true)
        );
        assert_eq!(
            st(
                g(g(&receipt, "binding_status"), "evidence"),
                "binding_source"
            ),
            "exact"
        );
        let data = w.registry();
        let now = g(g(g(&data, "checkouts"), &current), "history")
            .as_array()
            .unwrap();
        assert_eq!(now.len(), history.len() + 1);
        assert_eq!(now.get(..history.len()).unwrap(), history);
        assert!(g(&data, "checkouts").get(&old).is_some());
        w.commit(&repo);
        let linked = w.root.join("recovered worktree");
        w.git(
            &repo,
            &[
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("-b"),
                OsStr::new("recovered"),
                linked.as_os_str(),
            ],
        );
        assert_eq!(g(&w.status(&linked), "resolved"), &Value::Bool(true));
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_historical_fork_and_rebind_require_explicit_alias_link
#[test]
fn historical_fork_and_rebind_require_link() {
    in_world("historical_fork_and_rebind_require_link", |w| {
        let repo = w.repo("repository");
        w.remote(&repo, "origin", "https://github.com/example/project.git");
        w.create(&repo, None);
        w.historical(&repo);
        let before = std::fs::read(w.registry_path()).unwrap();
        for action in ["rebind", "fork"] {
            assert!(
                project::preview(&repo, action, None, "Explicit fixture decision", None)
                    .unwrap_err()
                    .to_string()
                    .contains("explicit link")
            );
            assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
        }
        w.apply(&w.preview(&repo, "link", None));
        w.preview(&repo, "rebind", None);
        w.run(
            &repo,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/example/fork.git",
            ],
        );
        w.preview(&repo, "fork", None);
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_observed_alias_registered_after_preview_refuses_and_retains_history
#[test]
fn concurrent_alias_registration_refuses_stale_preview() {
    in_world("concurrent_alias_registration_refuses_stale_preview", |w| {
        let repo = w.repo("repository");
        w.remote(&repo, "origin", "https://github.com/example/project.git");
        w.create(&repo, None);
        let (current, old) = w.historical(&repo);
        let proposal = w.preview(&repo, "link", None);
        let mut data = w.registry();
        let mut entry = g(g(&data, "checkouts"), &old).clone();
        push(
            entry.get_mut("locations").unwrap(),
            s(w.root.join("concurrent-known-location").to_str().unwrap()),
        );
        set(data.get_mut("checkouts").unwrap(), &current, entry);
        w.save_registry(&data);
        let before = std::fs::read(w.registry_path()).unwrap();
        assert!(
            project::apply(&proposal)
                .unwrap_err()
                .to_string()
                .contains("changed after preview")
        );
        assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_old_pending_proposal_evidence_requires_fresh_preview
#[test]
fn old_pending_proposal_requires_fresh_evidence() {
    in_world("old_pending_proposal_requires_fresh_evidence", |w| {
        let repo = w.repo("repository");
        w.create(&repo, None);
        let mut proposal = w.preview(&repo, "link", None);
        let evidence = proposal
            .get_mut("proposal")
            .unwrap()
            .get_mut("evidence")
            .unwrap();
        for key in [
            "binding_source",
            "observed_checkout_key",
            "continuity_issues",
        ] {
            remove(evidence, key);
        }
        let before = std::fs::read(w.registry_path()).unwrap();
        assert!(
            project::apply(&proposal)
                .unwrap_err()
                .to_string()
                .contains("evidence changed")
        );
        assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_historical_candidates_are_deterministic_and_conflicts_refused
#[test]
fn historical_candidates_sort_numerically_and_conflicts_refuse() {
    in_world(
        "historical_candidates_sort_numerically_and_conflicts_refuse",
        |w| {
            let repo = w.repo("repository");
            w.remote(&repo, "origin", "https://github.com/example/project.git");
            w.create(&repo, None);
            let (_, old) = w.historical(&repo);
            let inode = old.split_once(':').unwrap().1;
            let mut data = w.registry();
            let entry = remove(data.get_mut("checkouts").unwrap(), &old);
            let old = format!("100:{inode}");
            let alternate = format!("9:{inode}");
            set(data.get_mut("checkouts").unwrap(), &old, entry.clone());
            set(data.get_mut("checkouts").unwrap(), &alternate, entry);
            w.save_registry(&data);
            assert_eq!(
                st(g(&w.status(&repo), "evidence"), "checkout_key"),
                alternate
            );
            set(
                data.get_mut("checkouts")
                    .unwrap()
                    .get_mut(&alternate)
                    .unwrap(),
                "authority_name",
                s("other"),
            );
            w.save_registry(&data);
            let status = w.status(&repo);
            assert_eq!(g(&status, "resolved"), &Value::Bool(false));
            assert!(
                json::encode(g(&status, "issues"), json::DEFAULT)
                    .unwrap()
                    .contains("Conflicting historical")
            );
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_rebound_aliases_after_later_device_change_require_explicit_selection
#[test]
fn rebound_historical_aliases_require_explicit_selection() {
    in_world(
        "rebound_historical_aliases_require_explicit_selection",
        |w| {
            let repo = w.repo("repository");
            w.remote(&repo, "origin", "https://github.com/example/project.git");
            w.remote(&repo, "upstream", "https://github.com/example/new.git");
            let (id, _) = w.create(&repo, Some("origin"));
            let (current, _) = w.historical(&repo);
            w.apply(&w.preview(&repo, "link", Some("origin")));
            w.apply(&w.preview(&repo, "rebind", Some("upstream")));
            w.bank(&repo, &id, Some("repo://github.com/example/new"));
            let mut data = w.registry();
            let (device, inode) = current.split_once(':').unwrap();
            let later = format!("{}:{inode}", device.parse::<u64>().unwrap() + 200);
            let entry = remove(data.get_mut("checkouts").unwrap(), &current);
            set(data.get_mut("checkouts").unwrap(), &later, entry);
            w.save_registry(&data);
            let status = w.status(&repo);
            assert_eq!(g(&status, "resolved"), &Value::Bool(false));
            assert!(
                json::encode(g(&status, "issues"), json::DEFAULT)
                    .unwrap()
                    .contains("Conflicting historical")
            );
            w.apply(&w.preview(&repo, "link", Some("upstream")));
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
            assert_eq!(st(&w.status(&repo), "project_id"), id);
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_historical_recovery_requires_bank_authority_location_and_inode
#[test]
fn historical_recovery_requires_all_continuity_evidence() {
    in_world(
        "historical_recovery_requires_all_continuity_evidence",
        |w| {
            for damage in ["remote", "bank", "inode", "location", "missing-remote"] {
                let repo = w.repo(damage);
                w.remote(
                    &repo,
                    "origin",
                    &format!("https://github.com/example/{damage}.git"),
                );
                w.create(&repo, None);
                let (_, old) = w.historical(&repo);
                let mut data = w.registry();
                match damage {
                    "remote" => {
                        w.run(
                            &repo,
                            &[
                                "remote",
                                "set-url",
                                "origin",
                                "https://github.com/example/other.git",
                            ],
                        );
                    }
                    "bank" => w.bank(
                        &repo,
                        &host::uuid4().unwrap(),
                        Some("repo://github.com/example/bank"),
                    ),
                    "inode" => {
                        let entry = remove(data.get_mut("checkouts").unwrap(), &old);
                        set(
                            data.get_mut("checkouts").unwrap(),
                            &format!("{}:999999999999", old.split_once(':').unwrap().0),
                            entry,
                        );
                    }
                    "location" => set(
                        data.get_mut("checkouts").unwrap().get_mut(&old).unwrap(),
                        "locations",
                        Value::Array(vec![s(w.root.join("another/.git").to_str().unwrap())]),
                    ),
                    _ => {
                        w.run(&repo, &["remote", "remove", "origin"]);
                    }
                }
                w.save_registry(&data);
                assert_eq!(
                    g(&w.status(&repo), "resolved"),
                    &Value::Bool(false),
                    "{damage}"
                );
            }
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_historical_recovery_preserves_explicit_authority
#[test]
fn historical_recovery_preserves_explicit_authority() {
    in_world("historical_recovery_preserves_explicit_authority", |w| {
        let repo = w.repo("repository");
        w.remote(&repo, "origin", "https://github.com/example/project.git");
        w.remote(&repo, "upstream", "https://github.com/example/upstream.git");
        w.create(&repo, Some("origin"));
        w.historical(&repo);
        let status = w.status(&repo);
        assert_eq!(g(&status, "resolved"), &Value::Bool(true));
        assert_eq!(st(g(&status, "evidence"), "authority_name"), "origin");
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_normalization_strips_secrets_and_unifies_only_documented_equivalence
#[test]
fn normalization_strips_secrets_and_preserves_distinctions() {
    in_world(
        "normalization_strips_secrets_and_preserves_distinctions",
        |_| {
            let expected = "repo://github.com/owner/repo";
            for remote in [
                "https://alice:secret@GITHUB.COM/Owner/Repo.git/?token=secret#secret",
                "git@github.com:Owner/Repo.git",
                "ssh://git@GITHUB.com:22/OWNER/REPO.git/",
                "https://github.com:443/owner/repo",
                "http://github.com:80/owner/repo.git",
                expected,
                "https://github.com/%6fwner/repo.git",
            ] {
                let receipt = project::normalize(&s(remote), None).unwrap();
                assert_eq!(st(&receipt, "canonical_remote"), expected);
                assert!(
                    !json::encode(&receipt, json::DEFAULT)
                        .unwrap()
                        .contains("secret")
                );
            }
            let mut seen = std::collections::BTreeSet::from([expected.to_owned()]);
            for remote in [
                "https://other.example/owner/repo.git",
                "https://github.com/another/repo.git",
                "https://github.com/owner/fork.git",
                "ssh://git@github.com:2222/owner/repo.git",
                "https://Example.com/Owner/Repo.git",
                "https://example.com/owner/repo.git",
            ] {
                assert!(
                    seen.insert(
                        st(
                            &project::normalize(&s(remote), None).unwrap(),
                            "canonical_remote"
                        )
                        .to_owned()
                    )
                );
            }
            for remote in [
                "https://secret@host/owner/%2Frepo",
                "ext::secret",
                "https://secret@host/a/../b",
                "ssh://secret@host:wrong/repo",
            ] {
                assert!(
                    !project::normalize(&s(remote), None)
                        .unwrap_err()
                        .to_string()
                        .contains("secret")
                );
            }
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_linked_worktrees_join_via_the_real_common_git_directory
#[test]
fn linked_worktrees_share_common_directory_identity() {
    in_world("linked_worktrees_share_common_directory_identity", |w| {
        let repo = w.repo("repository");
        let (id, _) = w.create(&repo, None);
        w.commit(&repo);
        let linked = w.root.join("feature worktree");
        w.git(
            &repo,
            &[
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("-b"),
                OsStr::new("feature"),
                linked.as_os_str(),
            ],
        );
        let a = w.status(&repo);
        let b = w.status(&linked);
        assert_eq!(g(&b, "resolved"), &Value::Bool(true));
        assert_eq!(st(&b, "project_id"), id);
        assert_eq!(
            g(g(&a, "evidence"), "checkout_key"),
            g(g(&b, "evidence"), "checkout_key")
        );
        assert_ne!(
            g(g(&a, "evidence"), "repo_root"),
            g(g(&b, "evidence"), "repo_root")
        );
        assert_eq!(g(&w.registry(), "checkouts").as_object().unwrap().len(), 1);
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_no_remote_move_keeps_id_and_preserves_location_history
#[test]
fn moved_repository_preserves_location_history_and_old_replay() {
    in_world(
        "moved_repository_preserves_location_history_and_old_replay",
        |w| {
            let repo = w.repo("repository");
            let (id, original) = w.create(&repo, None);
            let old = repo.join(".git");
            let moved = w.root.join("moved repository");
            std::fs::rename(&repo, &moved).unwrap();
            assert_eq!(g(&w.status(&moved), "resolved"), &Value::Bool(true));
            assert_eq!(st(&w.status(&moved), "project_id"), id);
            w.apply(&w.preview(&moved, "link", None));
            let status = w.status(&moved);
            let entry = g(&status, "checkout_entry");
            assert_eq!(
                g(entry, "locations"),
                &Value::Array(vec![
                    s(old.to_str().unwrap()),
                    s(moved.join(".git").to_str().unwrap())
                ])
            );
            assert_eq!(
                st(
                    g(entry, "history").as_array().unwrap().last().unwrap(),
                    "action"
                ),
                "link"
            );
            let replay = w.apply(&original);
            assert_eq!(g(&replay, "already_applied"), &Value::Bool(true));
            assert_eq!(
                g(g(&replay, "binding_status"), "resolved"),
                &Value::Bool(false)
            );
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_copied_bank_requires_explicit_link_or_fork
#[test]
fn copied_bank_requires_link_or_fork() {
    in_world("copied_bank_requires_link_or_fork", |w| {
        let repo = w.repo("repository");
        let (id, _) = w.create(&repo, None);
        w.commit(&repo);
        let copied = w.root.join("independent-copy");
        w.git(
            &w.root,
            &[OsStr::new("clone"), repo.as_os_str(), copied.as_os_str()],
        );
        w.run(&copied, &["remote", "remove", "origin"]);
        assert_eq!(g(&w.status(&copied), "resolved"), &Value::Bool(false));
        assert!(project::preview(&copied, "bind", None, "Copy", None).is_err());
        w.apply(&w.preview(&copied, "link", None));
        assert_eq!(g(&w.status(&copied), "resolved"), &Value::Bool(true));
        assert_eq!(st(&w.status(&copied), "project_id"), id);
        let fork = w.preview(&copied, "fork", None);
        let receipt = w.apply(&fork);
        assert_eq!(
            g(g(&receipt, "binding_status"), "resolved"),
            &Value::Bool(false)
        );
        let new_id = st(&fork, "project_id");
        assert_ne!(new_id, id);
        w.bank(&copied, new_id, None);
        assert_eq!(g(&w.status(&copied), "resolved"), &Value::Bool(true));
        assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
        assert_eq!(g(&w.registry(), "projects").as_object().unwrap().len(), 2);
        assert_eq!(
            st(
                g(g(&w.status(&copied), "checkout_entry"), "history")
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap(),
                "previous_project_id"
            ),
            id
        );
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_fork_on_fresh_machine_is_caught_by_shared_authority_evidence
#[test]
fn fresh_machine_fork_requires_shared_authority_decision() {
    in_world(
        "fresh_machine_fork_requires_shared_authority_decision",
        |w| {
            let repo = w.repo("repository");
            let old = host::uuid4().unwrap();
            w.bank(&repo, &old, Some("repo://github.com/owner/project"));
            w.remote(
                &repo,
                "origin",
                "git@github.com:other-developer/project.git",
            );
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(false));
            assert!(
                project::preview(&repo, "bind", None, "Automatic adoption", None)
                    .unwrap_err()
                    .to_string()
                    .contains("conflicts")
            );
            let fork = w.preview(&repo, "fork", None);
            w.apply(&fork);
            let target = g(g(&fork, "proposal"), "bank_identity");
            w.bank(
                &repo,
                st(target, "project-id"),
                g(target, "canonical-remote").as_str(),
            );
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
            assert_ne!(st(&w.status(&repo), "project_id"), old);
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_same_remote_on_fresh_machine_binds_stable_bank_id
#[test]
fn fresh_machine_matching_remote_binds_stable_id_without_secrets() {
    in_world(
        "fresh_machine_matching_remote_binds_stable_id_without_secrets",
        |w| {
            let repo = w.repo("repository");
            let id = host::uuid4().unwrap();
            w.bank(&repo, &id, Some("repo://github.com/owner/project"));
            w.remote(
                &repo,
                "origin",
                "https://credentials:secret@GITHUB.COM/Owner/Project.git?secret=secret",
            );
            let receipt = w.apply(&w.preview(&repo, "bind", None));
            assert_eq!(
                g(g(&receipt, "binding_status"), "resolved"),
                &Value::Bool(true)
            );
            assert_eq!(st(&receipt, "project_id"), id);
            assert!(
                !String::from_utf8(std::fs::read(w.registry_path()).unwrap())
                    .unwrap()
                    .contains("secret")
            );
            assert!(
                !json::encode(&receipt, json::DEFAULT)
                    .unwrap()
                    .contains("secret")
            );
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_multiple_remotes_require_authority_and_configured_selection_wins
#[test]
fn configured_authority_resolves_multiple_remotes() {
    in_world("configured_authority_resolves_multiple_remotes", |w| {
        let repo = w.repo("repository");
        w.remote(&repo, "origin", "https://github.com/owner/project.git");
        w.remote(&repo, "upstream", "git@github.com:other/project.git");
        assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(false));
        assert!(project::preview(&repo, "create", None, "No authority", None).is_err());
        w.run(
            &repo,
            &["config", "agent-memory.authorityRemote", "upstream"],
        );
        let (id, _) = w.create(&repo, None);
        let status = w.status(&repo);
        assert_eq!(g(&status, "resolved"), &Value::Bool(true));
        assert_eq!(st(&status, "project_id"), id);
        assert_eq!(
            st(g(&status, "evidence"), "canonical_remote"),
            "repo://github.com/other/project"
        );
        w.run(&repo, &["config", "agent-memory.authorityRemote", "origin"]);
        assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(false));
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_explicit_authority_is_retained_and_conflicting_push_url_refuses
#[test]
fn explicit_authority_persists_and_conflicting_push_refuses() {
    in_world(
        "explicit_authority_persists_and_conflicting_push_refuses",
        |w| {
            let repo = w.repo("repository");
            w.remote(&repo, "origin", "https://github.com/owner/project.git");
            w.remote(&repo, "other", "https://github.com/other/project.git");
            w.create(&repo, Some("origin"));
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
            w.run(
                &repo,
                &[
                    "remote",
                    "set-url",
                    "--push",
                    "origin",
                    "https://github.com/other/project.git",
                ],
            );
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(false));
            assert!(
                project::preview(
                    &repo,
                    "rebind",
                    None,
                    "Conflicting publication destination",
                    None
                )
                .is_err()
            );
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_remote_change_stops_until_explicit_rebind_and_retains_alias_history
#[test]
fn rebind_retains_alias_history_and_never_reapplies_old_decision() {
    in_world(
        "rebind_retains_alias_history_and_never_reapplies_old_decision",
        |w| {
            let repo = w.repo("repository");
            w.remote(&repo, "origin", "https://github.com/old/project.git");
            let (id, old) = w.create(&repo, None);
            let before_bank = std::fs::read(repo.join("memory-bank/MEMORY.md")).unwrap();
            w.run(
                &repo,
                &[
                    "remote",
                    "set-url",
                    "origin",
                    "git@github.com:new/project.git",
                ],
            );
            let status = w.status(&repo);
            assert_eq!(g(&status, "resolved"), &Value::Bool(false));
            assert!(
                json::encode(g(&status, "issues"), json::DEFAULT)
                    .unwrap()
                    .contains("Unexpected authority")
            );
            let rebind = w.preview(&repo, "rebind", None);
            let receipt = w.apply(&rebind);
            assert_eq!(
                g(g(&receipt, "binding_status"), "resolved"),
                &Value::Bool(false)
            );
            assert_eq!(
                std::fs::read(repo.join("memory-bank/MEMORY.md")).unwrap(),
                before_bank
            );
            w.bank(&repo, &id, Some("repo://github.com/new/project"));
            let status = w.status(&repo);
            assert_eq!(g(&status, "resolved"), &Value::Bool(true));
            assert_eq!(
                g(g(&status, "project_entry"), "aliases"),
                &Value::Array(vec![s("repo://github.com/old/project")])
            );
            assert_eq!(
                st(
                    g(g(&status, "project_entry"), "history")
                        .as_array()
                        .unwrap()
                        .last()
                        .unwrap(),
                    "action"
                ),
                "rebind"
            );
            let before = std::fs::read(w.registry_path()).unwrap();
            assert_eq!(g(&w.apply(&old), "already_applied"), &Value::Bool(true));
            assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
            assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_changed_registry_bank_remote_or_authority_refuses_stale_proposals
#[test]
fn every_changed_source_refuses_stale_proposals() {
    in_world("every_changed_source_refuses_stale_proposals", |w| {
        let repo = w.repo("repository");
        w.remote(&repo, "origin", "https://github.com/owner/project.git");
        let proposal = w.preview(&repo, "create", None);
        w.run(
            &repo,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/other/project.git",
            ],
        );
        assert!(project::apply(&proposal).is_err());
        assert!(!w.registry_path().exists());
        let proposal = w.preview(&repo, "create", None);
        w.bank(
            &repo,
            &host::uuid4().unwrap(),
            Some("repo://github.com/other/project"),
        );
        assert!(project::apply(&proposal).is_err());
        let proposal = w.preview(&repo, "bind", None);
        w.run(&repo, &["config", "agent-memory.authorityRemote", "origin"]);
        assert!(project::apply(&proposal).is_err());
        w.apply(&w.preview(&repo, "bind", None));
        let stale = w.preview(&repo, "link", None);
        let mut data = w.registry();
        let id = st(&w.status(&repo), "project_id").to_owned();
        push(
            data.get_mut("projects")
                .unwrap()
                .get_mut(&id)
                .unwrap()
                .get_mut("aliases")
                .unwrap(),
            s("repo://github.com/historical/project"),
        );
        w.save_registry(&data);
        let before = std::fs::read(w.registry_path()).unwrap();
        assert!(project::apply(&stale).is_err());
        assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_parallel_registrations_preserve_winner_and_replay_is_idempotent
#[test]
fn parallel_registrations_preserve_winner_and_replay() {
    in_world("parallel_registrations_preserve_winner_and_replay", |w| {
        let repo = w.repo("repository");
        let a = w.preview(&repo, "create", None);
        let b = w.preview(&repo, "create", None);
        let gate = Arc::new(std::sync::Barrier::new(3));
        let children = [a.clone(), b.clone()].map(|p| {
            let gate = gate.clone();
            std::thread::spawn(move || {
                gate.wait();
                project::apply_with_sources(&p, sources())
            })
        });
        gate.wait();
        let results = children.map(|c| c.join().unwrap());
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "{results:?}"
        );
        let data = w.registry();
        assert_eq!(g(&data, "projects").as_object().unwrap().len(), 1);
        assert_eq!(g(&data, "checkouts").as_object().unwrap().len(), 1);
        let winner = if g(&data, "projects").get(st(&a, "project_id")).is_some() {
            a
        } else {
            b
        };
        let before = std::fs::read(w.registry_path()).unwrap();
        assert_eq!(g(&w.apply(&winner), "already_applied"), &Value::Bool(true));
        assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_canonical_remote_does_not_merge_different_bank_ids
#[test]
fn canonical_remote_never_merges_different_bank_ids() {
    in_world("canonical_remote_never_merges_different_bank_ids", |w| {
        let repo = w.repo("repository");
        w.remote(&repo, "origin", "https://github.com/owner/project.git");
        let (id, _) = w.create(&repo, None);
        let other = w.repo("independent");
        w.remote(&other, "origin", "git@github.com:owner/project.git");
        w.bank(
            &other,
            &host::uuid4().unwrap(),
            Some("repo://github.com/owner/project"),
        );
        assert!(
            project::preview(&other, "bind", None, "Conflicting bank IDs", None)
                .unwrap_err()
                .to_string()
                .contains("another project")
        );
        assert_eq!(st(&w.status(&repo), "project_id"), id);
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_copied_pack_spaces_non_git_project_and_unrelated_registry_preserved
#[test]
fn non_git_spaces_and_unrelated_registry_preserved() {
    in_world("non_git_spaces_and_unrelated_registry_preserved", |w| {
        let repo = w.root.join("non git project");
        std::fs::create_dir(&repo).unwrap();
        let proposal = w.preview(&repo, "create", None);
        w.apply(&proposal);
        w.bank(&repo, st(&proposal, "project_id"), None);
        assert_eq!(g(&w.status(&repo), "resolved"), &Value::Bool(true));
        std::fs::write(w.registry_path(), b"Unrelated file; preserve").unwrap();
        let before = std::fs::read(w.registry_path()).unwrap();
        assert!(project::status(&repo).is_err());
        assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
    });
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_unknown_bank_symlinks_and_busy_state_refuse_without_mutation
#[test]
fn unknown_bank_symlinks_and_busy_state_preserve_sources() {
    in_world(
        "unknown_bank_symlinks_and_busy_state_preserve_sources",
        |w| {
            let repo = w.repo("repository");
            w.create(&repo, None);
            let router = repo.join("memory-bank/MEMORY.md");
            let original = std::fs::read_to_string(&router).unwrap();
            std::fs::write(
                &router,
                original.replace("agent-memory/v1", "agent-memory/v999"),
            )
            .unwrap();
            assert!(project::status(&repo).is_err());
            std::fs::write(&router, &original).unwrap();
            let lock = router.parent().unwrap().join(".agent-memory.lock");
            std::fs::write(&lock, b"Writer").unwrap();
            assert!(
                project::status(&repo)
                    .unwrap_err()
                    .to_string()
                    .contains("provider is not registered")
            );
            std::fs::remove_file(&lock).unwrap();
            let original_registry = std::fs::read(w.registry_path()).unwrap();
            std::fs::remove_file(w.registry_path()).unwrap();
            let unrelated = w.root.join("unrelated.json");
            std::fs::write(&unrelated, &original_registry).unwrap();
            std::os::unix::fs::symlink(&unrelated, w.registry_path()).unwrap();
            assert!(project::status(&repo).is_err());
            assert!(w.registry_path().is_symlink());
            assert_eq!(std::fs::read(&unrelated).unwrap(), original_registry);
        },
    );
}

// obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_project_registry_stale_lock_recovery_is_bounded_and_read_back
#[test]
fn stale_lock_recovery_proves_dead_owner_and_readback() {
    in_world("stale_lock_recovery_proves_dead_owner_and_readback", |w| {
        let repo = w.repo("repository");
        w.create(&repo, None);
        let lock = w.registry_path().with_file_name(".projects.lock");
        let mut owner = obj([
            ("operation_id", s(host::uuid4().unwrap())),
            ("operation", s("project-create")),
            ("host", s(host::hostname().unwrap())),
            ("pid", Value::Int(host::pid().into())),
            ("start", s("2026-10-22T18:00:00Z")),
        ]);
        std::fs::write(&lock, encoded(&owner).unwrap()).unwrap();
        assert!(
            project::recover(None)
                .unwrap_err()
                .to_string()
                .contains("still alive")
        );
        let output = command(&["/usr/bin/true".into()], &w.env(), None);
        set(&mut owner, "pid", Value::Int(output.pid.into()));
        std::fs::write(&lock, encoded(&owner).unwrap()).unwrap();
        let proposal = project::recover(None).unwrap();
        assert!(lock.exists());
        assert_eq!(
            st(g(&proposal, "proposal"), "schema"),
            "agent-memory/project-lock-recovery-v1"
        );
        let before = std::fs::read(w.registry_path()).unwrap();
        let receipt = project::recover(Some(&proposal)).unwrap();
        assert_eq!(g(&receipt, "verified"), &Value::Bool(true));
        assert!(!lock.exists());
        assert_eq!(std::fs::read(w.registry_path()).unwrap(), before);
    });
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_real_root_spellings_and_linked_worktrees_preserve_public_identity
#[test]
fn real_root_spellings_and_linked_worktrees_preserve_identity() {
    in_world(
        "real_root_spellings_and_linked_worktrees_preserve_identity",
        |w| {
            for (i, name) in [
                "normal root",
                "embedded\nnewline root",
                "trailing LF root\n\n",
            ]
            .iter()
            .enumerate()
            {
                let repo = w.repo(name);
                let (id, _) = w.create(&repo, None);
                std::fs::rename(repo.join("memory-bank"), repo.join("project-notes")).unwrap();
                w.run(&repo, &["config", "agent-memory.bankRoot", "project-notes"]);
                w.commit(&repo);
                let linked = w.root.join(format!("linked-{i}\n"));
                w.git(
                    &repo,
                    &[
                        OsStr::new("worktree"),
                        OsStr::new("add"),
                        OsStr::new("--detach"),
                        linked.as_os_str(),
                        OsStr::new("HEAD"),
                    ],
                );
                for root in [&repo, &linked] {
                    let before = w.snapshot(root);
                    assert_eq!(
                        repository_config::settings(root).unwrap().bank_root,
                        "project-notes"
                    );
                    assert_eq!(
                        repository_config::recovery_settings(root)
                            .unwrap()
                            .bank_root,
                        "project-notes"
                    );
                    let status = w.status(root);
                    assert_eq!(g(&status, "resolved"), &Value::Bool(true));
                    assert_eq!(st(&status, "project_id"), id);
                    assert_eq!(w.snapshot(root), before);
                }
            }
        },
    );
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_sha256_head_retains_committed_locator_after_staged_deletion
#[test]
fn sha256_committed_locator_survives_staged_deletion() {
    in_world("sha256_committed_locator_survives_staged_deletion", |w| {
        let repo = w.root.join("sha256 repository\n");
        std::fs::create_dir(&repo).unwrap();
        w.run(
            &repo,
            &["init", "--initial-branch=main", "--object-format=sha256"],
        );
        w.run(&repo, &["config", "user.name", "Fixture"]);
        w.run(&repo, &["config", "user.email", "fixture@example.invalid"]);
        let (id, _) = w.create(&repo, None);
        std::fs::rename(repo.join("memory-bank"), repo.join("project-notes")).unwrap();
        w.run(&repo, &["config", "agent-memory.bankRoot", "project-notes"]);
        w.commit(&repo);
        w.run(&repo, &["rm", "-r", "--", "project-notes"]);
        let before = w.snapshot(&repo);
        assert_eq!(
            repository_config::settings(&repo).unwrap().bank_root,
            "project-notes"
        );
        let status = w.status(&repo);
        assert_eq!(g(&status, "resolved"), &Value::Bool(false));
        assert_eq!(st(&status, "project_id"), id);
        assert!(
            g(&status, "issues")
                .as_array()
                .unwrap()
                .contains(&s("Bank initialization or identity update is pending"))
        );
        assert_eq!(w.snapshot(&repo), before);
    });
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_unborn_root_preserves_local_settings_and_indexed_bank_identity
#[test]
fn unborn_root_preserves_indexed_bank_identity() {
    in_world("unborn_root_preserves_indexed_bank_identity", |w| {
        for name in [
            "unborn normal",
            "unborn embedded\nroot",
            "unborn trailing\n\n",
        ] {
            let repo = w.repo(name);
            let (id, _) = w.create(&repo, None);
            std::fs::rename(repo.join("memory-bank"), repo.join("project-notes")).unwrap();
            w.run(&repo, &["config", "agent-memory.bankRoot", "project-notes"]);
            w.run(&repo, &["add", "--", "project-notes"]);
            let before = w.snapshot(&repo);
            assert_eq!(
                repository_config::settings(&repo).unwrap().bank_root,
                "project-notes"
            );
            let status = w.status(&repo);
            assert_eq!(g(&status, "resolved"), &Value::Bool(true));
            assert_eq!(st(&status, "project_id"), id);
            assert_eq!(w.snapshot(&repo), before);
        }
    });
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_non_git_directory_remains_readable_without_inventing_identity
#[test]
fn non_git_directory_never_invents_identity() {
    in_world("non_git_directory_never_invents_identity", |w| {
        let repo = w.root.join("plain directory\n\n");
        std::fs::create_dir(&repo).unwrap();
        let before = w.snapshot(&repo);
        assert_eq!(
            repository_config::settings(&repo).unwrap().bank_root,
            "memory-bank"
        );
        assert_eq!(
            repository_config::recovery_settings(&repo).unwrap(),
            repository_config::settings(&repo).unwrap()
        );
        let status = w.status(&repo);
        assert_eq!(g(&status, "resolved"), &Value::Bool(false));
        assert_eq!(g(&status, "project_id"), &Value::Null);
        assert_eq!(w.snapshot(&repo), before);
        assert!(!repo.join("memory-bank").exists());
    });
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_grouped_discovery_keeps_effective_includes_rewrites_and_remote_conflicts
#[test]
fn grouped_discovery_preserves_includes_rewrites_and_conflicts() {
    in_world(
        "grouped_discovery_preserves_includes_rewrites_and_conflicts",
        |w| {
            let repo = w.repo("repository");
            w.create(&repo, None);
            w.commit(&repo);
            let included = w.root.join("included-authority.gitconfig");
            std::fs::write(&included, "[agent-memory]\n authorityRemote = Upstream\n").unwrap();
            w.git(
                &repo,
                &[
                    OsStr::new("config"),
                    OsStr::new("include.path"),
                    included.as_os_str(),
                ],
            );
            w.run(
                &repo,
                &["config", "url.https://github.com/.insteadOf", "fixture:"],
            );
            w.remote(&repo, "Upstream", "fixture:example/project.git");
            w.run(
                &repo,
                &[
                    "remote",
                    "set-url",
                    "--push",
                    "Upstream",
                    "https://github.com/example/project.git",
                ],
            );
            let before = w.snapshot(&repo);
            let found = project::discover(&repo, None, DiscoveryOptions::default()).unwrap();
            assert_eq!(
                g(&found, "configured_authority"),
                &Value::Array(vec![s("Upstream")])
            );
            assert_eq!(st(&found, "authority_name"), "Upstream");
            assert_eq!(
                st(&found, "canonical_remote"),
                "repo://github.com/example/project"
            );
            assert_eq!(w.snapshot(&repo), before);
            w.remote(&repo, "Other", "https://github.com/example/other.git");
            w.run(
                &repo,
                &["config", "--add", "agent-memory.authorityRemote", "Other"],
            );
            let before = w.snapshot(&repo);
            let found = project::discover(&repo, None, DiscoveryOptions::default()).unwrap();
            assert!(g(&found, "issues").as_array().unwrap().contains(&s(
                "Conflicting configured authority remotes; choose one explicitly"
            )));
            assert_eq!(
                st(
                    &project::discover(&repo, Some("Upstream"), DiscoveryOptions::default())
                        .unwrap(),
                    "canonical_remote"
                ),
                "repo://github.com/example/project"
            );
            assert_eq!(w.snapshot(&repo), before);
            w.run(
                &repo,
                &[
                    "remote",
                    "set-url",
                    "--push",
                    "Upstream",
                    "https://github.com/example/different.git",
                ],
            );
            let before = w.snapshot(&repo);
            let found =
                project::discover(&repo, Some("Upstream"), DiscoveryOptions::default()).unwrap();
            assert_eq!(g(&found, "canonical_remote"), &Value::Null);
            assert_eq!(
                g(g(g(&found, "remotes"), "Upstream"), "consistent"),
                &Value::Bool(false)
            );
            assert_eq!(w.snapshot(&repo), before);
        },
    );
}

#[test]
fn registry_bytes_validation_and_unordered_proposal_maps() {
    in_world(
        "registry_bytes_validation_and_unordered_proposal_maps",
        |w| {
            assert_eq!(encoded(&obj([("schema",s("agent-memory/projects-v1")),("projects",obj([] as [(&str,Value);0])),("checkouts",obj([] as [(&str,Value);0])),("operations",obj([] as [(&str,Value);0]))])).unwrap(),b"{\n  \"checkouts\": {},\n  \"operations\": {},\n  \"projects\": {},\n  \"schema\": \"agent-memory/projects-v1\"\n}\n");
            let repo = w.repo("repository");
            let proposal = w.preview(&repo, "create", None);
            // Encoding sorted keys emulates loading a Python-era persisted proposal;
            // structural evidence comparison must not compare insertion order.
            let sorted = load(&encoded(&proposal).unwrap()).unwrap();
            let receipt = w.apply(&sorted);
            assert_eq!(g(&receipt, "verified"), &Value::Bool(true));
            let mut changed = sorted.clone();
            set(
                changed.get_mut("proposal").unwrap(),
                "reason",
                s("Changed request"),
            );
            assert_eq!(
                project::apply(&changed).unwrap_err().to_string(),
                "Operation ID already used for a different proposal"
            );
            let mut data = w.registry();
            set(&mut data, "unknown", Value::Null);
            assert!(project::validate(&data).is_err());
        },
    );
}

struct RecoveryReader;
impl project::LayoutObservation for RecoveryReader {
    fn read_source(&self, bank: &Path, relative: &Path, limit: usize) -> Result<Vec<u8>> {
        agent_memory::fs::read_source(bank, relative, limit)
            .map(|v| v.bytes)
            .map_err(|e| agent_memory::error::ContractError::new(e.to_string()))
    }
}
struct MarkerObservation;
impl project::BankObservation for MarkerObservation {
    fn markers(&self, bank: &Path, lock: Option<&[u8]>, transaction: Option<&[u8]>) -> Result<()> {
        for (name, owned) in [
            (".agent-memory.lock", lock),
            (".agent-memory-transaction.md", transaction),
        ] {
            match agent_memory::fs::read_source(bank, Path::new(name), project::MARKER_MAX_BYTES) {
                Ok(value) if owned == Some(value.bytes.as_slice()) => (),
                Err(agent_memory::fs::ReadError::Io(e))
                    if e.kind() == std::io::ErrorKind::NotFound =>
                {
                    ()
                }
                _ => {
                    return Err(agent_memory::error::ContractError::new(
                        "Bank busy or interrupted; inspect/recover before resolving identity",
                    ));
                }
            }
        }
        Ok(())
    }
}
#[test]
fn raw_identity_requires_independent_typed_evidence_and_providers() {
    in_world(
        "raw_identity_requires_independent_typed_evidence_and_providers",
        |w| {
            let repo = w.repo("repository");
            w.create(&repo, None);
            let router = repo.join("memory-bank/MEMORY.md");
            let raw = std::fs::read(&router).unwrap();
            let evidence = project::RawIdentityEvidence::observe(&repo, &raw, None).unwrap();
            let options = DiscoveryOptions {
                raw_identity: Some(&evidence),
                ..DiscoveryOptions::default()
            };
            assert!(
                project::discover(&repo, None, options)
                    .unwrap_err()
                    .to_string()
                    .contains("bank observation provider is not registered")
            );
            project::register_bank_observation(Some(Arc::new(MarkerObservation))).unwrap();
            assert!(
                project::discover(&repo, None, options)
                    .unwrap_err()
                    .to_string()
                    .contains("layout observation provider is not registered")
            );
            project::register_layout_observation(Some(Arc::new(RecoveryReader))).unwrap();
            assert_eq!(
                g(
                    &project::status_with_options(&repo, None, options).unwrap(),
                    "resolved"
                ),
                &Value::Bool(true)
            );
            assert_eq!(
                evidence.metadata().get("project-id"),
                g(&w.status(&repo), "project_id").into()
            );
            assert!(
                evidence
                    .checked(&repo, &repo.join("other-bank"), &raw)
                    .is_err()
            );
            assert!(
                evidence
                    .checked(&repo, &repo.join("memory-bank"), b"changed")
                    .is_err()
            );
            assert_eq!(
                project::RawIdentityEvidence::observe(&repo, b"not a router", None)
                    .unwrap_err()
                    .to_string(),
                "pending-identity-review"
            );
            let reader = |_: &[u8]| Ok(None);
            assert!(
                project::discover(
                    &repo,
                    None,
                    DiscoveryOptions {
                        migration_reader: Some(&reader),
                        ..options
                    }
                )
                .unwrap_err()
                .to_string()
                .contains("internally observed typed evidence")
            );
        },
    );
}

#[test]
fn committed_identity_review_establishes_existing_checkout_only() {
    in_world(
        "committed_identity_review_establishes_existing_checkout_only",
        |w| {
            let repo = w.repo("repository");
            let (id, _) = w.create(&repo, None);
            w.commit(&repo);
            let committed = std::fs::read(repo.join("memory-bank/MEMORY.md")).unwrap();
            let raw = b"# Damaged raw router retained verbatim.\n";
            std::fs::write(repo.join("memory-bank/MEMORY.md"), raw).unwrap();
            let oid = w.run(&repo, &["rev-parse", "HEAD"]).trim_end().to_owned();
            let (_, registry, _) = agent_memory::local_registry::paths("projects").unwrap();
            let data = load(&std::fs::read(registry).unwrap()).unwrap();
            let key = g(&data, "checkouts")
                .as_object()
                .unwrap()
                .first()
                .unwrap()
                .0
                .clone();
            let mut review = obj([
                ("schema", s("agent-memory/layout-identity-review-v1")),
                ("review_id", s(host::uuid4().unwrap())),
                ("project_id", s(&id)),
                ("repo_root", s(repo.to_str().unwrap())),
                ("checkout_key", s(key)),
                (
                    "router_sha256",
                    s(agent_memory::developer_identity::digest(Some(raw)).unwrap()),
                ),
                (
                    "committed_router",
                    s(format!("{oid}:memory-bank/MEMORY.md")),
                ),
                (
                    "committed_router_sha256",
                    s(agent_memory::developer_identity::digest(Some(&committed)).unwrap()),
                ),
                ("reviewer_developer_id", s(host::uuid4().unwrap())),
                ("reviewer_harness", s("fixture")),
                ("reviewer_session", s("independent-review")),
                ("author_session", s("author")),
                ("reviewed_at", s("2026-10-22T18:00:00Z")),
                (
                    "rationale",
                    s("Reviewed immutable original and retained raw bytes"),
                ),
                ("decision", s("accept")),
            ]);
            let evidence =
                project::RawIdentityEvidence::observe(&repo, raw, Some(&review)).unwrap();
            assert!(evidence.review().is_some());
            assert_eq!(st(evidence.metadata(), "project-id"), id);
            set(
                &mut review,
                "committed_router",
                s("HEAD:memory-bank/MEMORY.md"),
            );
            assert!(
                project::RawIdentityEvidence::observe(&repo, raw, Some(&review))
                    .unwrap_err()
                    .to_string()
                    .contains("exact immutable")
            );
            set(
                &mut review,
                "committed_router",
                s(format!("{oid}:memory-bank/MEMORY.md")),
            );
            set(&mut review, "project_id", s(host::uuid4().unwrap()));
            assert!(
                project::RawIdentityEvidence::observe(&repo, raw, Some(&review))
                    .unwrap_err()
                    .to_string()
                    .contains("existing checkout/project")
            );
        },
    );
}

#[test]
fn owned_markers_and_migration_reader_preserve_authority() {
    in_world(
        "owned_markers_and_migration_reader_preserve_authority",
        |w| {
            let repo = w.repo("repository");
            let (id, _) = w.create(&repo, None);
            let bank = repo.join("memory-bank");
            std::fs::write(bank.join(".agent-memory.lock"), b"Owned lock").unwrap();
            std::fs::write(
                bank.join(".agent-memory-transaction.md"),
                b"Owned transaction",
            )
            .unwrap();
            let options = DiscoveryOptions {
                owned_lock: Some(b"Owned lock"),
                owned_transaction: Some(b"Owned transaction"),
                ..DiscoveryOptions::default()
            };
            assert_eq!(
                g(
                    &project::status_with_options(&repo, None, options).unwrap(),
                    "resolved"
                ),
                &Value::Bool(true)
            );
            assert!(
                project::discover(
                    &repo,
                    None,
                    DiscoveryOptions {
                        owned_transaction: Some(b"changed"),
                        ..options
                    }
                )
                .is_err()
            );
            let reader = |raw: &[u8]| {
                Ok(Some(
                    bank_contract::parse_router(std::str::from_utf8(raw).unwrap())?.0,
                ))
            };
            let found = project::discover(
                &repo,
                None,
                DiscoveryOptions {
                    migration_reader: Some(&reader),
                    ..options
                },
            )
            .unwrap();
            assert_eq!(st(&found, "bank_project_id"), id);
            let unsupported = |_: &[u8]| Ok(None);
            assert_eq!(
                project::discover(
                    &repo,
                    None,
                    DiscoveryOptions {
                        migration_reader: Some(&unsupported),
                        ..options
                    }
                )
                .unwrap_err()
                .to_string(),
                "Existing bank router is not a supported migration source"
            );
        },
    );
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_corrupt_missing_and_dangling_head_never_establish_empty_repository
#[test]
fn damaged_head_never_establishes_an_empty_repository() {
    in_world("damaged_head_never_establishes_an_empty_repository", |w| {
        for damage in ["corrupt", "missing", "dangling"] {
            let repo = w.repo(damage);
            w.create(&repo, None);
            w.commit(&repo);
            let head = repo.join(".git/HEAD");
            match damage {
                "missing" => std::fs::remove_file(&head).unwrap(),
                "corrupt" => std::fs::write(&head, "not a valid reference\n").unwrap(),
                _ => std::fs::write(&head, format!("{}\n", "f".repeat(40))).unwrap(),
            }
            let before = w.snapshot(&repo);
            assert!(project::status(&repo).is_err());
            assert_eq!(w.snapshot(&repo), before);
        }
    });
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_unreadable_or_malformed_metadata_preserves_refusal_and_original_bank
#[test]
fn malformed_git_metadata_preserves_bank_and_refusal() {
    in_world("malformed_git_metadata_preserves_bank_and_refusal", |w| {
        for damage in ["config-directory", "malformed-config", "invalid-gitfile"] {
            let repo = w.repo(damage);
            w.create(&repo, None);
            w.commit(&repo);
            let config = repo.join(".git/config");
            match damage {
                "config-directory" => {
                    std::fs::rename(&config, w.root.join("retained-config")).unwrap();
                    std::fs::create_dir(&config).unwrap();
                }
                "malformed-config" => {
                    std::fs::write(&config, "[unterminated fixture section\n").unwrap()
                }
                _ => {
                    std::fs::rename(repo.join(".git"), w.root.join("retained-git")).unwrap();
                    std::fs::write(repo.join(".git"), "unreadable metadata locator\n").unwrap();
                }
            }
            let before = w.snapshot(&repo);
            assert!(project::status(&repo).is_err());
            assert_eq!(w.snapshot(&repo), before);
        }
    });
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_malformed_successful_observation_never_resolves_existing_identity
#[test]
fn malformed_successful_root_observation_never_resolves() {
    in_world(
        "malformed_successful_root_observation_never_resolves",
        |w| {
            let repo = w.repo("repository");
            w.create(&repo, None);
            w.commit(&repo);
            let root = repo.to_str().unwrap();
            let oid = w.run(&repo, &["rev-parse", "HEAD"]).trim_end().to_owned();
            let truncated = oid.get(..oid.len() - 1).unwrap();
            let outputs = [
                String::new(),
                format!("{root}\n"),
                format!("{root}\n{oid}"),
                format!("{root}\n{truncated}\n"),
                format!("{root}\nHEAD\n"),
                format!("\n{oid}\n"),
                format!("{root}\n{oid}\n\n"),
            ];
            let before = w.snapshot(&repo);
            for output in outputs {
                std::fs::write(w.root.join("head-output"), output).unwrap();
                std::fs::write(w.root.join("head-code"), "0").unwrap();
                assert!(project::status(&repo).is_err());
                assert_eq!(w.snapshot(&repo), before);
            }
        },
    );
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_signaled_or_incomplete_failed_observation_never_adopts_partial_root
#[test]
fn failed_root_observation_never_accepts_partial_output() {
    in_world(
        "failed_root_observation_never_accepts_partial_output",
        |w| {
            let repo = w.repo("repository");
            w.create(&repo, None);
            w.commit(&repo);
            let root = repo.to_str().unwrap();
            let failures = [
                ("1", String::new()),
                ("1", root.to_owned()),
                ("1", "\n".into()),
                ("signal15", format!("{root}\n")),
                ("signal9", format!("{root}\n{}\n", "a".repeat(40))),
            ];
            let before = w.snapshot(&repo);
            for (code, output) in failures {
                std::fs::write(w.root.join("head-output"), output).unwrap();
                std::fs::write(w.root.join("head-code"), code).unwrap();
                assert!(project::status(&repo).is_err());
                assert_eq!(w.snapshot(&repo), before);
            }
        },
    );
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_unavailable_commit_or_locator_blob_never_becomes_absent_memory
#[test]
fn unavailable_commit_or_locator_blob_never_means_absent_memory() {
    in_world(
        "unavailable_commit_or_locator_blob_never_means_absent_memory",
        |w| {
            for missing in ["commit", "locator-blob"] {
                let repo = w.repo(missing);
                w.create(&repo, None);
                w.commit(&repo);
                let expression = if missing == "commit" {
                    "HEAD"
                } else {
                    "HEAD:memory-bank/MEMORY.md"
                };
                let oid = w
                    .run(&repo, &["rev-parse", expression])
                    .trim_end()
                    .to_owned();
                std::fs::remove_file(
                    repo.join(".git/objects")
                        .join(oid.get(..2).unwrap())
                        .join(oid.get(2..).unwrap()),
                )
                .unwrap();
                let before = w.snapshot(&repo);
                assert!(project::status(&repo).is_err());
                assert_eq!(w.snapshot(&repo), before);
            }
        },
    );
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_coalesced_observation_ignores_ambient_authority_and_fsmonitor
#[test]
fn ambient_git_authority_and_fsmonitor_are_ignored() {
    in_world("ambient_git_authority_and_fsmonitor_are_ignored", |w| {
        let repo = w.repo("repository");
        let (id, _) = w.create(&repo, None);
        w.commit(&repo);
        w.repo("foreign");
        let trap = w.root.join("fsmonitor-trap");
        let canary = w.root.join("fsmonitor-executed");
        std::fs::write(
            &trap,
            format!("#!/bin/sh\nprintf executed > '{}'\n", canary.display()),
        )
        .unwrap();
        std::fs::set_permissions(&trap, std::fs::Permissions::from_mode(0o700)).unwrap();
        w.git(
            &repo,
            &[
                OsStr::new("config"),
                OsStr::new("core.fsmonitor"),
                trap.as_os_str(),
            ],
        );
        let before = w.snapshot(&repo);
        let status = w.status(&repo);
        assert_eq!(g(&status, "resolved"), &Value::Bool(true));
        assert_eq!(st(&status, "project_id"), id);
        assert_eq!(w.snapshot(&repo), before);
        assert!(!canary.exists());
        assert!(!w.root.join("unwanted-trace").exists());
        assert!(!w.root.join("unwanted-index").exists());
    });
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_invalid_config_refuses_before_speculative_real_git_inventory_acceptance
#[test]
fn invalid_local_configuration_preserves_bank_and_sanitizes_refusal() {
    in_world(
        "invalid_local_configuration_preserves_bank_and_sanitizes_refusal",
        |w| {
            for invalid in ["duplicate", "empty", "unsafe-root", "unsafe-label"] {
                let repo = w.repo(invalid);
                w.create(&repo, None);
                w.commit(&repo);
                if invalid == "duplicate" {
                    w.run(
                        &repo,
                        &["config", "--add", "agent-memory.bankRoot", "memory-bank"],
                    );
                    w.run(
                        &repo,
                        &["config", "--add", "agent-memory.bankRoot", "memory-bank"],
                    );
                } else {
                    let value = match invalid {
                        "empty" => "",
                        "unsafe-root" => "../outside",
                        _ => "docs/password=BOUNDARY_ROOT_SECRET",
                    };
                    w.run(&repo, &["config", "agent-memory.bankRoot", value]);
                }
                let before = w.snapshot(&repo);
                let error = project::status(&repo).unwrap_err();
                assert!(!error.to_string().contains("BOUNDARY_ROOT_SECRET"));
                assert_eq!(w.snapshot(&repo), before);
                assert!(!w.root.join("outside").exists());
            }
        },
    );
}

// obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_branch_validation_remains_after_competing_router_refusal
#[test]
fn competing_router_refusal_precedes_branch_validation() {
    in_world("competing_router_refusal_precedes_branch_validation", |w| {
        let repo = w.repo("repository");
        w.create(&repo, None);
        w.commit(&repo);
        w.run(
            &repo,
            &["config", "agent-memory.authorityBranch", "../unsafe"],
        );
        std::fs::create_dir(repo.join("competing")).unwrap();
        std::fs::copy(
            repo.join("memory-bank/MEMORY.md"),
            repo.join("competing/MEMORY.md"),
        )
        .unwrap();
        let before = w.snapshot(&repo);
        assert!(
            project::status(&repo)
                .unwrap_err()
                .to_string()
                .contains("Competing memory bank locations")
        );
        assert_eq!(w.snapshot(&repo), before);
    });
}

struct WriterOperations;
impl project::BankWrites for WriterOperations {
    fn operations(&self) -> &[&str] {
        agent_memory::bank_writes::OPERATIONS
    }
}
#[test]
fn canonical_writer_busy_and_initializing_remain_distinct_typed_refusals() {
    in_world(
        "canonical_writer_busy_and_initializing_remain_distinct_typed_refusals",
        |w| {
            let repo = w.repo("repository");
            w.create(&repo, None);
            project::register_bank_writes(Some(Arc::new(WriterOperations))).unwrap();
            let bank = repo.join("memory-bank");
            let lock = bank.join(".agent-memory.lock");
            let chain = agent_memory::fs::DirectoryChain::open(&bank).unwrap();
            let file = chain
                .directory()
                .create_exclusive(OsStr::new(".agent-memory.lock"), libc::O_WRONLY, 0o600)
                .unwrap();
            let guard = agent_memory::fs::FlockGuard::try_acquire(
                file,
                agent_memory::fs::LockMode::Exclusive,
            )
            .unwrap();
            assert_eq!(
                project::discover(&repo, None, DiscoveryOptions::default())
                    .unwrap_err()
                    .kind(),
                &agent_memory::error::Kind::CanonicalWriterInitializing
            );
            assert_eq!(
                project::status(&repo).unwrap_err().kind(),
                &agent_memory::error::Kind::CanonicalWriterInitializing
            );
            drop(guard);
            std::fs::remove_file(&lock).unwrap();
            agent_memory::local_registry::exclusive(
                &lock,
                &host::uuid4().unwrap(),
                "bank-replace",
                sources(),
                |_| {
                    assert_eq!(
                        project::discover(&repo, None, DiscoveryOptions::default())
                            .unwrap_err()
                            .kind(),
                        &agent_memory::error::Kind::CanonicalWriterBusy
                    );
                    assert_eq!(
                        project::status(&repo).unwrap_err().kind(),
                        &agent_memory::error::Kind::CanonicalWriterBusy
                    );
                    Ok(())
                },
            )
            .unwrap();
            assert!(!lock.exists());
        },
    );
}
