//! Disposable, credential-free real-process test worlds.
use agent_memory::clock::Deadline;
use agent_memory::codec::json::{self, Value};
use agent_memory::error::ContractError;
use agent_memory::process::{self, Input, Output, Spec, Status, Target};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

pub const CAPTURE_LIMIT: usize = 8 * 1024 * 1024;
pub const MARKER_LIMIT: usize = 4096;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
pub const DIRECTORIES: &[&str] = &[
    "home",
    "config",
    "data",
    "state",
    "cache",
    "backups",
    "tmp",
    "repo",
    "bank",
    "codex",
    "claude",
    "opencode",
    "skills",
    "credentials",
];
pub const ROOT_VARIABLES: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
    "TMPDIR",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "OPENCODE_CONFIG_DIR",
    "GH_CONFIG_DIR",
    "AGENT_MEMORY_CONFIG_HOME",
    "AGENT_MEMORY_STATE_HOME",
    "AGENT_MEMORY_BACKUP_HOME",
];
pub type Environment = BTreeMap<OsString, OsString>;
pub type Result<T, E = WorldError> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum WorldError {
    Invalid(String),
    Io(io::Error),
    Contract(ContractError),
    Process(Box<process::ProcessError>),
    Check(Output),
}

impl WorldError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Io(error) => error.fmt(f),
            Self::Contract(error) => error.fmt(f),
            Self::Process(error) => error.fmt(f),
            Self::Check(output) => write!(
                f,
                "Command failed: {:?}; stdout={:?}; stderr={:?}",
                output.status, output.stdout, output.stderr
            ),
        }
    }
}

impl std::error::Error for WorldError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Contract(error) => Some(error),
            Self::Process(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl From<io::Error> for WorldError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ContractError> for WorldError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<process::ProcessError> for WorldError {
    fn from(error: process::ProcessError) -> Self {
        Self::Process(Box::new(error))
    }
}

pub struct RunOptions<'a> {
    pub cwd: Option<&'a Path>,
    pub env: &'a [(OsString, OsString)],
    pub check: bool,
    pub timeout: Duration,
    pub capture_limit: usize,
    /// Baseline group mode selects the same direct-child ownership policy.
    pub group: bool,
}

impl Default for RunOptions<'_> {
    fn default() -> Self {
        Self {
            cwd: None,
            env: &[],
            check: true,
            timeout: DEFAULT_TIMEOUT,
            capture_limit: CAPTURE_LIMIT,
            group: false,
        }
    }
}

/// Owned launch data; no reference into the World or caller's argv is retained.
pub struct Command {
    argv: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    cwd: PathBuf,
    timeout: Duration,
    capture_limit: usize,
    check: bool,
}

impl Command {
    fn spec(&self) -> Result<Spec<'_>> {
        Ok(Spec {
            argv: &self.argv,
            search_path: true,
            env: &self.env,
            cwd: Some(&self.cwd),
            stdin: Input::Null,
            stdout: Target::Capture {
                limit: self.capture_limit,
            },
            stderr: Target::Capture {
                limit: self.capture_limit,
            },
            deadline: Deadline::after(self.timeout)?,
            new_session: false,
            file_size_limit: None,
            umask: Some(0o077),
            pass_fds: &[],
        })
    }

    pub fn run(&self) -> Result<Output> {
        let output = process::run_for(&self.spec()?, self.timeout)?;
        checked(output, self.check)
    }
}

fn checked(output: Output, check: bool) -> Result<Output> {
    if check && output.status != Status::Code(0) {
        return Err(WorldError::Check(output));
    }
    Ok(output)
}

struct ChildState {
    process: Option<process::OwnedProcess>,
    deadline: Deadline,
    check: bool,
}

/// A concurrent command owned jointly by its handle and its World.
/// Neither borrowed argv nor a borrow into World survives launch.
pub struct Child {
    state: Rc<RefCell<ChildState>>,
}

impl Child {
    pub fn wait(&self) -> Result<Output> {
        let mut state = self.state.borrow_mut();
        let process = state
            .process
            .take()
            .ok_or_else(|| WorldError::invalid("Owned child is already collected or closed"))?;
        let output = process.finish(state.deadline)?;
        checked(output, state.check)
    }

    pub fn close(&self) -> Result<()> {
        if let Some(mut process) = self.state.borrow_mut().process.take() {
            process.close()?;
        }
        Ok(())
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub struct World {
    pub root: PathBuf,
    pub token: String,
    pub paths: BTreeMap<&'static str, PathBuf>,
    pub env: Environment,
    git_binary: PathBuf,
    closed: RefCell<bool>,
    children: RefCell<Vec<Rc<RefCell<ChildState>>>>,
}

impl World {
    pub fn new() -> Result<Self> {
        let temporary = agent_memory::fs::canonical_path(Path::new("/tmp"), true)?;
        let token = random_token()?;
        let root = temporary.join(format!("agent-memory-test-{token}"));
        fs::DirBuilder::new().mode(0o700).create(&root)?;
        // Construct the guard before any fallible setup so partial setup is removed.
        let mut world = Self {
            root,
            token,
            paths: BTreeMap::new(),
            env: Environment::new(),
            git_binary: PathBuf::new(),
            closed: RefCell::new(false),
            children: RefCell::new(Vec::new()),
        };
        for &name in DIRECTORIES {
            let path = world.root.join(name);
            fs::DirBuilder::new().mode(0o700).create(&path)?;
            world.paths.insert(name, path);
        }
        fs::write(
            world.root.join(".test-world.json"),
            json::encode(
                &Value::Object(vec![(
                    "token".to_owned(),
                    Value::String(world.token.clone()),
                )]),
                json::DEFAULT,
            )?,
        )?;
        world.git_binary = executable("git")
            .ok_or_else(|| WorldError::invalid("The offline contract gate requires Git"))?;
        let am_dir = Path::new(env!("CARGO_BIN_EXE_am"))
            .parent()
            .ok_or_else(|| WorldError::invalid("am binary has no parent directory"))?;
        let git_dir = world
            .git_binary
            .parent()
            .ok_or_else(|| WorldError::invalid("Git binary has no parent directory"))?;
        let mut search = vec![am_dir.to_path_buf(), git_dir.to_path_buf()];
        if let Some(node) = executable("node") {
            let tools = world.root.join("node-tools");
            fs::DirBuilder::new().mode(0o700).create(&tools)?;
            symlink(fs::canonicalize(node)?, tools.join("node"))?;
            search.push(tools);
        }
        search.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
        let mut unique = Vec::new();
        for directory in search {
            if !unique.contains(&directory) {
                unique.push(directory);
            }
        }
        let path =
            std::env::join_paths(unique).map_err(|error| WorldError::invalid(error.to_string()))?;
        world.env.insert("PATH".into(), path);
        for (key, directory) in [
            ("HOME", "home"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_CACHE_HOME", "cache"),
            ("TMPDIR", "tmp"),
            ("CODEX_HOME", "codex"),
            ("CLAUDE_CONFIG_DIR", "claude"),
            ("OPENCODE_CONFIG_DIR", "opencode"),
            ("AGENT_MEMORY_BACKUP_HOME", "backups"),
            ("GH_CONFIG_DIR", "credentials"),
        ] {
            world
                .env
                .insert(key.into(), world.root.join(directory).into_os_string());
        }
        for (key, directory) in [
            ("AGENT_MEMORY_CONFIG_HOME", "config"),
            ("AGENT_MEMORY_STATE_HOME", "state"),
        ] {
            world.env.insert(
                key.into(),
                world
                    .root
                    .join(directory)
                    .join("agent-memory")
                    .into_os_string(),
            );
        }
        for (key, value) in [
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_ASKPASS", "/usr/bin/false"),
            ("SSH_ASKPASS", "/usr/bin/false"),
            ("GIT_ALLOW_PROTOCOL", "file"),
            ("GIT_AUTHOR_NAME", "Test Developer"),
            ("GIT_AUTHOR_EMAIL", "test@example.invalid"),
            ("GIT_COMMITTER_NAME", "Test Developer"),
            ("GIT_COMMITTER_EMAIL", "test@example.invalid"),
            ("LC_ALL", "C"),
            ("TZ", "UTC"),
            ("NO_COLOR", "1"),
        ] {
            world.env.insert(key.into(), value.into());
        }
        for (key, name) in [
            ("GIT_CONFIG_SYSTEM", "git-system.config"),
            ("GIT_CONFIG_GLOBAL", "git-global.config"),
        ] {
            let path = world.root.join("credentials").join(name);
            fs::write(&path, b"")?;
            world.env.insert(key.into(), path.into_os_string());
        }
        world.env.insert(
            "AGENT_MEMORY_TEST_ROOT".into(),
            world.root.clone().into_os_string(),
        );
        world
            .env
            .insert("AGENT_MEMORY_TEST_TOKEN".into(), world.token.clone().into());
        Ok(world)
    }

    pub fn inside(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        let resolved = agent_memory::fs::canonical_path(
            &if path.is_absolute() {
                path.to_path_buf()
            } else {
                self.root.join(path)
            },
            false,
        )?;
        if !resolved.starts_with(&self.root) {
            return Err(WorldError::invalid(
                "Test path escapes its disposable world",
            ));
        }
        Ok(resolved)
    }

    pub fn command<I, S>(&self, args: I, options: RunOptions<'_>) -> Result<Command>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command_argv(
            args.into_iter()
                .map(|arg| arg.as_ref().to_owned())
                .collect(),
            options,
        )
    }

    fn command_argv(&self, argv: Vec<OsString>, options: RunOptions<'_>) -> Result<Command> {
        if *self.closed.borrow() {
            return Err(WorldError::invalid("Test world is closed"));
        }
        if argv.is_empty() {
            return Err(WorldError::invalid(
                "Commands require a nonempty argument list; shell execution is disabled",
            ));
        }
        if options
            .env
            .iter()
            .any(|(key, _)| !key.to_string_lossy().starts_with("AGENT_MEMORY_FAULT_"))
        {
            return Err(WorldError::invalid(
                "Only explicit fault-injection overrides are accepted",
            ));
        }
        if options.capture_limit > CAPTURE_LIMIT {
            return Err(WorldError::invalid(
                "Capture limit exceeds the test-world 8 MiB per-stream bound",
            ));
        }
        let mut env: Vec<_> = self
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        for (key, value) in options.env {
            if let Some((_, previous)) = env.iter_mut().find(|(existing, _)| existing == key) {
                *previous = value.clone();
            } else {
                env.push((key.clone(), value.clone()));
            }
        }
        let command = Command {
            argv,
            env,
            cwd: self.inside(options.cwd.unwrap_or(&self.root.join("repo")))?,
            timeout: options.timeout,
            capture_limit: options.capture_limit,
            check: options.check,
        };
        // Validate deadline overflow before launching any process.
        let _ = Deadline::after(command.timeout)?;
        Ok(command)
    }

    pub fn run<I, S>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.run_with(args, RunOptions::default())
    }

    pub fn run_with<I, S>(&self, args: I, options: RunOptions<'_>) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command(args, options)?.run()
    }

    pub fn am<I, S>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.am_with(args, RunOptions::default())
    }

    pub fn am_with<I, S>(&self, args: I, options: RunOptions<'_>) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let argv = std::iter::once(OsString::from(env!("CARGO_BIN_EXE_am")))
            .chain(args.into_iter().map(|arg| arg.as_ref().to_owned()))
            .collect();
        self.command_argv(argv, options)?.run()
    }

    /// Run `am --json <command path> <args>`, e.g. `am_json("am writes", …)`.
    pub fn am_json<I, S>(&self, command: &str, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = native_arguments(command, args)?;
        let argv = [
            OsString::from(env!("CARGO_BIN_EXE_am")),
            OsString::from("--json"),
        ]
        .into_iter()
        .chain(args)
        .collect();
        self.command_argv(argv, RunOptions::default())?.run()
    }

    pub fn git<I, S>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.git_with(args, RunOptions::default())
    }

    pub fn git_with<I, S>(&self, args: I, options: RunOptions<'_>) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let argv = std::iter::once(self.git_binary.clone().into_os_string())
            .chain(args.into_iter().map(|arg| arg.as_ref().to_owned()))
            .collect();
        self.command_argv(argv, options)?.run()
    }

    pub fn repository(&self, name: impl AsRef<Path>, bare: bool) -> Result<PathBuf> {
        let path = self.inside(name)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)?;
        let mut args = vec![OsString::from("init"), "--initial-branch=main".into()];
        if bare {
            args.push("--bare".into());
        }
        args.push(path.clone().into_os_string());
        self.git(args)?;
        Ok(path)
    }

    pub fn clone(&self, remote: impl AsRef<Path>, name: impl AsRef<Path>) -> Result<PathBuf> {
        let remote = self.inside(remote)?;
        let target = self.inside(name)?;
        self.git([
            OsStr::new("clone"),
            OsStr::new("--"),
            remote.as_os_str(),
            target.as_os_str(),
        ])?;
        Ok(target)
    }
}

fn random_token() -> Result<String> {
    let mut token = String::with_capacity(48);
    while token.len() < 48 {
        let uuid = agent_memory::host::uuid4()?;
        for (index, digit) in uuid.chars().filter(|digit| *digit != '-').enumerate() {
            // Omit UUID version/variant nibbles so the token has 192 random bits.
            if index != 12 && index != 16 && token.len() < 48 {
                token.push(digit);
            }
        }
    }
    Ok(token)
}

fn executable(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(name))
        .find(|path| {
            fs::metadata(path)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

pub fn require_world(env: &Environment, cwd: &Path) -> Result<PathBuf> {
    let root = env
        .get(OsStr::new("AGENT_MEMORY_TEST_ROOT"))
        .map(PathBuf::from)
        .ok_or_else(|| WorldError::invalid("Invalid test world"))?;
    if !root.is_absolute() || fs::symlink_metadata(&root)?.file_type().is_symlink() {
        return Err(WorldError::invalid("Invalid test world"));
    }
    let root = agent_memory::fs::canonical_path(&root, true)?;
    let temporary = agent_memory::fs::canonical_path(Path::new("/tmp"), true)?;
    if root.parent() != Some(temporary.as_path())
        || !root
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("agent-memory-test-"))
    {
        return Err(WorldError::invalid("Refusing a live path as a test world"));
    }
    let token = env
        .get(OsStr::new("AGENT_MEMORY_TEST_TOKEN"))
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| WorldError::invalid("Missing or mismatched test-world marker"))?;
    let (bytes, _) =
        agent_memory::fs::read_regular(&root, Path::new(".test-world.json"), MARKER_LIMIT)
            .map_err(|error| WorldError::invalid(error.to_string()))?;
    let marker = json::decode_bytes(
        &bytes,
        json::DecodePolicy {
            max_bytes: MARKER_LIMIT,
            ..json::DecodePolicy::default()
        },
    )?;
    if marker.get("token").and_then(Value::as_str) != Some(token) {
        return Err(WorldError::invalid(
            "Missing or mismatched test-world marker",
        ));
    }
    for &key in ROOT_VARIABLES {
        let value = env
            .get(OsStr::new(key))
            .map(PathBuf::from)
            .ok_or_else(|| WorldError::invalid(format!("Refusing non-isolated {key}")))?;
        let value = agent_memory::fs::canonical_path(&value, false)?;
        if value == root || !value.starts_with(&root) {
            return Err(WorldError::invalid(format!("Refusing non-isolated {key}")));
        }
    }
    if !agent_memory::fs::canonical_path(cwd, true)?.starts_with(&root) {
        return Err(WorldError::invalid(
            "Test worker must run inside its disposable world",
        ));
    }
    Ok(root)
}

/// The argv of one native invocation: `command` is the literal `am …` command
/// path (`am writes`, `am bank recover-layout`, `am install`), followed by
/// `args`. Nothing is translated, discovered or executed by name.
pub fn native_arguments<I, S>(command: &str, args: I) -> Result<Vec<OsString>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let path = command
        .strip_prefix("am")
        .filter(|rest| rest.is_empty() || rest.starts_with(' '))
        .ok_or_else(|| {
            WorldError::invalid(format!(
                "Fixture command must be a native am invocation: {command}"
            ))
        })?;
    Ok(path
        .split_whitespace()
        .map(OsString::from)
        .chain(args.into_iter().map(|arg| arg.as_ref().to_owned()))
        .collect())
}

impl World {
    pub fn spawn<I, S>(&self, args: I) -> Result<Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.spawn_with(
            args,
            RunOptions {
                check: false,
                ..RunOptions::default()
            },
        )
    }

    pub fn spawn_with<I, S>(&self, args: I, options: RunOptions<'_>) -> Result<Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let command = self.command(args, options)?;
        let mut children = self.children.borrow_mut();
        children.retain(|child| child.borrow().process.is_some());
        if children.len() >= 3 {
            return Err(WorldError::invalid(
                "Test world owns three active children; collect one before spawning another",
            ));
        }
        let spec = command.spec()?;
        let deadline = spec.deadline;
        let process = process::spawn_owned(&spec)?;
        let state = Rc::new(RefCell::new(ChildState {
            process: Some(process),
            deadline,
            check: command.check,
        }));
        children.push(Rc::clone(&state));
        Ok(Child { state })
    }

    pub fn close(&self) -> Result<()> {
        if self.closed.replace(true) {
            return Ok(());
        }
        let mut first = None;
        for child in self.children.borrow_mut().drain(..) {
            if let Some(mut process) = child.borrow_mut().process.take() {
                if let Err(error) = process.close() {
                    if first.is_none() {
                        first = Some(WorldError::from(error));
                    }
                }
            }
        }
        if let Err(error) = fs::remove_dir_all(&self.root) {
            if error.kind() != io::ErrorKind::NotFound && first.is_none() {
                first = Some(WorldError::Io(error));
            }
        }
        match first {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub fn require_current_world() -> Result<PathBuf> {
    require_world(&std::env::vars_os().collect(), &std::env::current_dir()?)
}
