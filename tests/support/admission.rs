//! Real private filesystem/process fixture shared by admission and refresh contracts.
use agent_memory::{clock::Deadline, codec::json::Value, developer_identity, process};
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "mod.rs"]
mod runtime_support;

pub struct World {
    pub root: PathBuf,
}
impl World {
    fn env(&self) -> Vec<(OsString, OsString)> {
        if runtime_support::require_current_world().ok().as_ref() == Some(&self.root) {
            return std::env::vars_os().collect();
        }
        [
            ("PATH", OsString::from("/usr/bin:/bin:/usr/local/bin")),
            ("HOME", self.root.join("home").into_os_string()),
            (
                "AGENT_MEMORY_CONFIG_HOME",
                self.root.join("config").into_os_string(),
            ),
            (
                "AGENT_MEMORY_STATE_HOME",
                self.root.join("state/agent-memory").into_os_string(),
            ),
            ("XDG_CONFIG_HOME", self.root.join("config").into_os_string()),
            ("XDG_STATE_HOME", self.root.join("state").into_os_string()),
            ("GIT_CONFIG_NOSYSTEM", OsString::from("1")),
            ("LC_ALL", OsString::from("C")),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect()
    }
    pub fn command(&self, argv: &[OsString], extra: &[(&str, OsString)]) -> process::Output {
        let mut env = self.env();
        for (key, value) in extra {
            let key = OsString::from(*key);
            if let Some((_, current)) = env.iter_mut().find(|(name, _)| name == &key) {
                *current = value.clone();
            } else {
                env.push((key, value.clone()));
            }
        }
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
    pub fn git(&self, repo: &Path, args: &[&str]) -> String {
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
    pub fn fixture(&self) -> (PathBuf, PathBuf, String) {
        self.fixture_with_origin(None)
    }
    /// `fixture()` whose repository already names `origin` when it is bound
    /// (baseline `World.repository` wrapped to add the remote at creation).
    pub fn fixture_with_origin(&self, origin: Option<&str>) -> (PathBuf, PathBuf, String) {
        let repo = self.root.join("repository");
        std::fs::create_dir(&repo).unwrap();
        self.git(&repo, &["init", "--initial-branch=main"]);
        self.git(&repo, &["config", "user.name", "Fixture"]);
        self.git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        if let Some(origin) = origin {
            self.git(&repo, &["remote", "add", "origin", origin]);
        }
        std::fs::write(
            repo.join("evidence.md"),
            b"# Observed result\n\nA disposable measured fixture, not actual project admission.\n",
        )
        .unwrap();
        self.git(&repo, &["add", "."]);
        self.git(&repo, &["commit", "-m", "Source evidence fixture"]);
        let mut proposal = agent_memory::codec::json::decode(
            include_str!("../../skills/agent-memory/templates/router-proposal.example.json"),
            Default::default(),
        )
        .unwrap();
        *proposal.get_mut("project-id").unwrap() =
            Value::String("75b38e17-c36e-4fab-8ba9-5fa51f25de91".into());
        let rendered = agent_memory::bank_contract::render(&proposal).unwrap();
        let bank = repo.join("memory-bank");
        for (path, body) in rendered.get("files").unwrap().as_object().unwrap() {
            let path = bank.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body.as_str().unwrap()).unwrap();
        }
        let p = developer_identity::preview(
            "create",
            None,
            &Value::Array(vec![Value::String("Fixture observer".into())]),
            &Value::Array(vec![]),
        )
        .unwrap();
        let dev = developer_identity::apply(&p)
            .unwrap()
            .get("developer_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let bind = agent_memory::scope_binding::preview(
            &repo,
            "bind",
            None,
            "Explicit fixture binding decision",
            None,
        )
        .unwrap();
        agent_memory::scope_binding::apply(&bind).unwrap();
        (repo, bank, dev)
    }
    pub fn save(&self, value: &Value, name: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, developer_identity::encoded(value).unwrap()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path
    }
    pub fn state(&self, repo: &Path, bank: &Path) -> (Value, String, String) {
        (
            agent_memory::bank_transactions::files_at(bank).unwrap(),
            self.git(repo, &["rev-parse", "HEAD"]),
            self.git(repo, &["diff", "--cached", "--binary"]),
        )
    }
}
pub fn in_world(name: &str, body: impl FnOnce(&World)) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    const COMPLETED: &str = "admission-consumer-completed";
    if std::env::var("AM_ADMISSION_CASE").ok().as_deref() == Some(name) {
        let root = runtime_support::require_current_world().unwrap();
        assert_eq!(
            std::env::var_os("AM_ADMISSION_ROOT").unwrap(),
            root.as_os_str()
        );
        let world = World { root };
        let runtime = runtime_support::installed_runtime::install(&world.root, |argv| {
            world.command(argv, &[])
        });
        agent_memory::cli::register_providers().unwrap();
        runtime_support::installed_runtime::register_component_runtime();
        body(&world);
        let token = std::env::var("AGENT_MEMORY_TEST_TOKEN").unwrap();
        let mut completed = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(world.root.join(COMPLETED))
            .unwrap();
        completed
            .write_all(format!("{name}\n{token}\n").as_bytes())
            .unwrap();
        completed.sync_all().unwrap();
        std::fs::File::open(&world.root)
            .unwrap()
            .sync_all()
            .unwrap();
        drop(runtime);
        return;
    }
    let mut world = runtime_support::World::new().unwrap();
    std::fs::remove_dir(world.root.join("repo")).unwrap();
    world.env.insert(
        "AGENT_MEMORY_CONFIG_HOME".into(),
        world.root.join("config").into_os_string(),
    );
    world.env.insert("AM_ADMISSION_CASE".into(), name.into());
    world.env.insert(
        "AM_ADMISSION_ROOT".into(),
        world.root.as_os_str().to_owned(),
    );
    let expected = format!("{name}\n{}\n", world.token);
    world
        .run_with(
            [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                name.into(),
                "--include-ignored".into(),
                "--nocapture".into(),
            ],
            runtime_support::RunOptions {
                cwd: Some(&world.root),
                timeout: Duration::from_secs(120),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        std::fs::read(world.root.join(COMPLETED)).unwrap(),
        expected.as_bytes()
    );
}
