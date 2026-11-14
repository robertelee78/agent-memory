//! Lifecycle lane: `am update` (Rust-to-Rust from a locally signed
//! release-v2 channel), `am uninstall`, `am purge-data` and doctor repair,
//! in isolated Worlds. Fault-injection scenarios run in a child copy of this
//! test binary so the process environment of parallel tests is never mutated.
use agent_memory::cache_recovery::Qualification;
use agent_memory::codec::json::{self, Value};
use agent_memory::installation::{
    self as install, ArtifactInspection, Environment, InstallRequest, Mode, PrivateState,
    ReleaseAccess, Services, Updating,
};
use agent_memory::release_distribution::{self as release, BinaryIdentity, ReleaseManifest};
use agent_memory::uninstallation::{self, UninstallRequest};
use agent_memory::updating::{self, UpdateRequest};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

const STAND_IN: &[u8] = b"#!/bin/sh\nprintf 'stand-in am %s\\n' \"$*\"\n";
const BASE: &str = "0.0.15";
const NEXT: &str = "0.0.16";

// ----- worlds -----

struct World {
    root: PathBuf,
    token: String,
}
impl World {
    fn new() -> Self {
        let tmp = std::fs::canonicalize("/tmp").unwrap();
        let token = agent_memory::host::uuid4().unwrap().replace('-', "");
        let root = tmp.join(format!("agent-memory-test-{}", &token[..16]));
        std::fs::create_dir(&root).unwrap();
        mode(&root, 0o700);
        std::fs::write(
            root.join(".test-world.json"),
            format!("{{\"token\": \"{token}\"}}"),
        )
        .unwrap();
        for name in [
            "home", "config", "data", "state", "codex", "claude", "opencode", "repo", "tmp",
        ] {
            std::fs::create_dir(root.join(name)).unwrap();
            mode(&root.join(name), 0o700);
        }
        Self { root, token }
    }
    fn reopen(root: &Path, token: &str) -> Self {
        Self {
            root: root.to_path_buf(),
            token: token.to_owned(),
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn pairs(&self) -> Vec<(String, String)> {
        let p = |name: &str| self.path(name).to_string_lossy().into_owned();
        vec![
            ("HOME".into(), p("home")),
            ("XDG_CONFIG_HOME".into(), p("config")),
            ("XDG_DATA_HOME".into(), p("data")),
            ("XDG_STATE_HOME".into(), p("state")),
            ("CODEX_HOME".into(), p("codex")),
            ("CLAUDE_CONFIG_DIR".into(), p("claude")),
            ("OPENCODE_CONFIG_DIR".into(), p("opencode")),
            ("TMPDIR".into(), p("tmp")),
            ("PATH".into(), "/usr/bin:/bin".into()),
            (
                "AGENT_MEMORY_TEST_ROOT".into(),
                self.root.to_string_lossy().into_owned(),
            ),
            ("AGENT_MEMORY_TEST_TOKEN".into(), self.token.clone()),
        ]
    }
    fn env(&self) -> Environment {
        Environment::from_pairs(
            self.pairs()
                .into_iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v))),
        )
    }
    fn state(&self) -> PathBuf {
        self.path("state/agent-memory/install")
    }
    fn runtime(&self) -> PathBuf {
        self.path("data/agent-memory/runtime")
    }
    fn bin(&self) -> PathBuf {
        self.path("home/.local/bin")
    }
    fn receipt_path(&self) -> PathBuf {
        self.state().join("receipt.json")
    }
}
impl Drop for World {
    fn drop(&mut self) {
        if std::env::var_os("AM_T2_CHILD").is_none() {
            let _ = make_writable(&self.root);
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
fn make_writable(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        for entry in std::fs::read_dir(path)? {
            make_writable(&entry?.path())?;
        }
    }
    Ok(())
}

// ----- seams: honest test implementers of unported callees -----

/// The real update checks; only the bank scan (private_state hooks, waiting
/// for installation/omp_boundary/admission registration) is a fixture.
struct LifecycleUpdating(updating::Runtime);
impl Updating for LifecycleUpdating {
    fn banks(&self, repositories: &[String], _: &Value, _: bool) -> install::Result<Value> {
        Ok(Value::Object(vec![(
            "observed_repositories".into(),
            Value::Array(repositories.iter().cloned().map(Value::String).collect()),
        )]))
    }
    fn qualified_current(
        &self,
        state: &Path,
        runtime: &Path,
    ) -> install::Result<Option<Qualification>> {
        self.0.qualified_current(state, runtime)
    }
    fn current(
        &self,
        state: &Path,
        runtime: &Path,
        cache: Option<&Qualification>,
    ) -> install::Result<()> {
        self.0.current(state, runtime, cache)
    }
    fn release_transition(
        &self,
        saved: &Value,
        source: &Value,
        root: &Path,
    ) -> install::Result<()> {
        self.0.release_transition(saved, source, root)
    }
    fn compatible_surface(
        &self,
        saved: &Value,
        source: &Value,
        cache: Option<&Qualification>,
    ) -> install::Result<()> {
        self.0.compatible_surface(saved, source, cache)
    }
}
struct FixturePrivateState;
impl PrivateState for FixturePrivateState {
    fn activation_guard(
        &self,
        _: &Value,
        _: Option<&Value>,
        _: &str,
        transition: &mut dyn FnMut() -> install::Result<Option<Value>>,
        body: &mut dyn FnMut(&Value) -> install::Result<Value>,
    ) -> install::Result<Value> {
        transition()?;
        body(&Value::Object(vec![("fixture".into(), Value::Bool(true))]))
    }
    fn prepare_lifecycle(&self, _: &Value, _: bool) -> install::Result<Value> {
        Ok(Value::Object(Vec::new()))
    }
    fn require_compatible(&self, _: &Value, _: &Value, _: &str) -> install::Result<Value> {
        Ok(Value::Object(Vec::new()))
    }
    fn public_summary(&self, value: &Value) -> install::Result<Value> {
        Ok(value.clone())
    }
}
struct FixtureCatalog;
impl agent_memory::artifact::MigrationContract for FixtureCatalog {
    fn inspect(&self, _: &Path, _: &Value, _: &Value) -> agent_memory::error::Result<Value> {
        Ok(Value::Array(vec![]))
    }
}
struct FixtureArtifacts;
impl ArtifactInspection for FixtureArtifacts {
    fn inspect_runtime(&self, pack: &Path, binary: &Path) -> install::Result<Value> {
        Ok(agent_memory::artifact::inspect_runtime_with(
            pack,
            binary,
            None,
            Some(&FixtureCatalog),
        )?)
    }
    fn inspect(&self, pack: &Path) -> install::Result<Value> {
        Ok(agent_memory::artifact::inspect_with(
            pack,
            None,
            Some(&FixtureCatalog),
        )?)
    }
}
fn services_with(world: &World, release: ReleaseAccess) -> Services {
    let runtime = updating::Runtime::new(world.env(), Arc::new(FixtureArtifacts), release.clone());
    Services {
        updating: Some(Arc::new(LifecycleUpdating(runtime))),
        private_state: Some(Arc::new(FixturePrivateState)),
        omp_boundary: None,
        artifacts: Arc::new(FixtureArtifacts),
        release,
        executable: None,
    }
}

// ----- small helpers -----

const COMPATIBILITY: &str = r#"{"schema":"agent-memory/runtime-compatibility-v1","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":[],"python_minimum":"3.11","installed_commands":["am","agent-memory"],"installation_receipt_schemas":["agent-memory/install-receipt-v1","agent-memory/install-receipt-v2","agent-memory/install-receipt-v3"],"git_required_for":["git-sharing"],"required_python_packages":[],"required_agent_clients":[],"optional_adapters":[]}"#;

fn write(path: &Path, body: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
fn mode(path: &Path, bits: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap();
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap_or_default()
}
fn targets(receipt: &Value) -> Vec<Value> {
    get(receipt, "targets").as_array().unwrap_or(&[]).to_vec()
}
fn decode(path: &Path) -> Value {
    json::decode_bytes(&std::fs::read(path).unwrap(), Default::default()).unwrap()
}
fn flag(value: &Value, key: &str) -> bool {
    get(value, key) == &Value::Bool(true)
}
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut result = vec![root.to_path_buf()];
    if std::fs::symlink_metadata(root).is_ok_and(|m| m.is_dir()) {
        let mut entries: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for entry in entries {
            result.extend(walk(&entry));
        }
    }
    result
}
/// Every node at and below `root`: link target, directory mode, or sha256 + mode.
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    if std::fs::symlink_metadata(root).is_err() {
        return result;
    }
    for path in walk(root) {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        let key = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let value = if meta.file_type().is_symlink() {
            format!("link {}", std::fs::read_link(&path).unwrap().display())
        } else if meta.is_dir() {
            format!("dir {:o}", meta.mode() & 0o7777)
        } else {
            format!(
                "{} {:o}",
                release::digest(&std::fs::read(&path).unwrap()),
                meta.mode() & 0o7777
            )
        };
        result.insert(key, value);
    }
    result
}
/// Lock owner diagnostics may refresh; ownership, targets and data do not.
fn without_owner(mut snapshot: BTreeMap<String, String>) -> BTreeMap<String, String> {
    snapshot.retain(|key, _| !key.ends_with("owner.json") && !key.ends_with("/lock"));
    snapshot
}
fn owned(receipt: &Value) -> BTreeMap<String, BTreeMap<String, String>> {
    targets(receipt)
        .iter()
        .map(|t| {
            (
                text(t, "path").to_owned(),
                snapshot(Path::new(text(t, "path"))),
            )
        })
        .collect()
}

// ----- a locally signed release-v2 channel (throwaway key, file:// assets) -----

fn command(args: &[OsString], input: &[u8]) -> Vec<u8> {
    let out =
        release::run_command(args, input, release::MAX_TAR, None, Duration::from_secs(20)).unwrap();
    assert_eq!(
        out.status,
        agent_memory::process::Status::Code(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

struct Channel {
    root: PathBuf,
}
impl Channel {
    fn new(world: &World) -> Self {
        let root = world.path("channel");
        std::fs::create_dir_all(root.join("latest")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let key = root.join("key");
        command(
            &[
                "ssh-keygen".into(),
                "-q".into(),
                "-t".into(),
                "ed25519".into(),
                "-N".into(),
                "".into(),
                "-f".into(),
                key.as_os_str().to_owned(),
            ],
            b"",
        );
        let public = std::fs::read_to_string(key.with_extension("pub")).unwrap();
        let fields = public
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        std::fs::write(
            root.join("allowed-signers"),
            format!(
                "{} namespaces=\"{},{}\" {fields}\n",
                release::PRINCIPAL,
                release::NAMESPACE,
                release::BINARY_NAMESPACE
            ),
        )
        .unwrap();
        Self { root }
    }
    fn open(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
    fn sign(&self, raw: &[u8], namespace: &str) -> Vec<u8> {
        command(
            &[
                "ssh-keygen".into(),
                "-Y".into(),
                "sign".into(),
                "-n".into(),
                namespace.into(),
                "-f".into(),
                self.root.join("key").into_os_string(),
            ],
            raw,
        )
    }
    fn access(&self) -> ReleaseAccess {
        ReleaseAccess {
            first_manifest_url: format!(
                "file://{}",
                self.root.join("latest").join(release::MANIFEST).display()
            ),
            assets: format!("file://{}", self.root.join("assets").display()),
            options: release::DownloadOptions {
                allow_file: true,
                json_response: false,
            },
            signers: std::fs::read_to_string(self.root.join("allowed-signers")).unwrap(),
            version: BASE.into(),
        }
    }
    fn assets(&self, version: &str) -> PathBuf {
        self.root.join("assets").join(format!("v{version}"))
    }
    fn binary(&self, version: &str) -> PathBuf {
        self.assets(version).join(format!("am-{}", install::TARGET))
    }
    /// Publish one signed release and make it the latest manifest.
    fn publish(&self, release: &Release) -> ReleaseManifest {
        let files = pack(&release.version, &release.extra);
        let inventory = release::inventory_value(&files);
        // encode_archive emits the deterministic gzip-compressed USTAR stream.
        let archive = release::encode_archive(
            &files
                .iter()
                .map(|(k, v)| (format!("{}{k}", release::PREFIX), v.clone()))
                .collect(),
        )
        .unwrap();
        let sha256 = release::digest(&release.binary);
        let identity = BinaryIdentity {
            size: release.binary.len(),
            artifact_digest: agent_memory::artifact::runtime_digest(&inventory, &sha256, 0o755)
                .unwrap()
                .1,
            sha256,
        };
        let manifest = ReleaseManifest {
            version: release.version.clone(),
            tag: format!("v{}", release.version),
            source_commit: release.commit.repeat(40),
            archive_size: archive.len(),
            archive_sha256: release::digest(&archive),
            artifact_digest: release::inventory_digest(&inventory).unwrap(),
            targets: release::TARGETS
                .into_iter()
                .map(|t| (t.into(), identity.clone()))
                .collect(),
            command_surface: release.surface.clone(),
        };
        let raw = manifest.encoded().unwrap();
        self.put_manifest(&release.version, &raw, &archive);
        let directory = self.assets(&release.version);
        let shipped = release.shipped.as_deref().unwrap_or(&release.binary);
        std::fs::write(self.binary(&release.version), shipped).unwrap();
        mode(&self.binary(&release.version), 0o755);
        std::fs::write(
            directory.join(format!("am-{}.sshsig", install::TARGET)),
            self.sign(shipped, release::BINARY_NAMESPACE),
        )
        .unwrap();
        manifest
    }
    fn put_manifest(&self, version: &str, raw: &[u8], archive: &[u8]) {
        let directory = self.assets(version);
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(release::ARCHIVE), archive).unwrap();
        std::fs::write(directory.join(release::MANIFEST), raw).unwrap();
        std::fs::write(
            directory.join(release::SIGNATURE),
            self.sign(raw, release::NAMESPACE),
        )
        .unwrap();
        std::fs::write(self.root.join("latest").join(release::MANIFEST), raw).unwrap();
    }
}

struct Release {
    version: String,
    commit: String,
    binary: Vec<u8>,
    /// Bytes actually served as the binary asset (tampering control).
    shipped: Option<Vec<u8>>,
    extra: Vec<(String, Vec<u8>)>,
    surface: Value,
}
impl Release {
    fn new(version: &str, commit: &str, binary: &[u8]) -> Self {
        Self {
            version: version.into(),
            commit: commit.into(),
            binary: binary.to_vec(),
            shipped: None,
            extra: Vec::new(),
            surface: agent_memory::cli::surface::declaration(),
        }
    }
    fn next(binary: &[u8]) -> Self {
        let mut value = Self::new(NEXT, "b", binary);
        value.extra.push((
            "templates/new-runtime-asset.txt".into(),
            b"Useful new runtime asset.\n".to_vec(),
        ));
        value
    }
}

/// The portable pack of a fixture release (a valid bounded portable contract).
fn pack(version: &str, extra: &[(String, Vec<u8>)]) -> release::Inventory {
    let mut files = release::Inventory::new();
    let mut put = |name: &str, raw: Vec<u8>| {
        files.insert(name.to_owned(), (raw, 0o644));
    };
    put(
        "SKILL.md",
        format!("---\nname: agent-memory\ndescription: Bounded portable contract fixture\nmetadata: {{\"version\":\"{version}\"}}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n").into_bytes(),
    );
    put(
        "bank/GUIDE.md",
        b"---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n".to_vec(),
    );
    let mut operation =
        String::from("---\nkind: operation\ndomain: bank\nfile-operations: [\"inspect\"]\n---\n\n");
    for section in agent_memory::pack_validation::SECTIONS {
        operation.push_str(&format!("## {section}\n\n"));
        if section == "Required context" {
            operation.push_str(
                "A session MUST first load and follow the [bank domain](../bank/GUIDE.md).\n\n",
            );
        } else {
            operation.push_str("Explicit operation context.\n\n");
        }
    }
    put("inspect/GUIDE.md", operation.into_bytes());
    put(
        "schemas/runtime-compatibility.json",
        COMPATIBILITY.as_bytes().to_vec(),
    );
    put(
        "schemas/bank-router.json",
        br#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#.to_vec(),
    );
    put(
        "templates/activation-markdown.example.txt",
        install::SHIM.as_bytes().to_vec(),
    );
    put(
        "templates/activation-omp-markdown.example.txt",
        include_bytes!("../skills/agent-memory/templates/activation-omp-markdown.example.txt")
            .to_vec(),
    );
    put("hooks/opencode.js", b"export default {}\n".to_vec());
    put("hooks/omp.ts", b"export default function () {}\n".to_vec());
    put(
        "keys/allowed-signers",
        release::ALLOWED_SIGNERS.as_bytes().to_vec(),
    );
    for (name, raw) in extra {
        put(name, raw.clone());
    }
    files
}

/// One installed signed release with the codex preset and a custom target.
struct Installed {
    world: World,
    channel: Channel,
}
impl Installed {
    fn new(install_mode: Mode) -> Self {
        Self::with_binary(install_mode, STAND_IN)
    }
    fn with_binary(install_mode: Mode, binary: &[u8]) -> Self {
        let world = World::new();
        let channel = Channel::new(&world);
        channel.publish(&Release::new(BASE, "a", binary));
        write(
            &world.path("codex/AGENTS.md"),
            "User instructions stay here.\n",
        );
        let installed = Self { world, channel };
        let result = installed.install(install_mode);
        assert_eq!(
            get(&result, "verified"),
            &Value::Bool(true),
            "{}",
            json::encode(&result, json::INDENT2_ASCII).unwrap()
        );
        installed
    }
    fn services(&self) -> Services {
        services_with(&self.world, self.channel.access())
    }
    fn install_request(&self, install_mode: Mode) -> InstallRequest {
        InstallRequest {
            bootstrap_binary: Some(self.channel.binary(BASE)),
            mode: Some(install_mode),
            presets: Some("codex".into()),
            target_dir: Some(vec![self.world.path("custom discovery with spaces")]),
            ..InstallRequest::default()
        }
    }
    fn install(&self, install_mode: Mode) -> Value {
        install::install_command(
            &self.world.env(),
            &self.services(),
            &self.install_request(install_mode),
        )
        .value
        .unwrap()
    }
    fn update(&self, request: &UpdateRequest) -> (i32, Value) {
        let result = updating::update_command(&self.world.env(), &self.services(), request);
        assert!(result.stderr.is_empty(), "{}", result.stderr);
        (result.exit_code, result.value.unwrap())
    }
    fn uninstall(&self, request: &UninstallRequest) -> (i32, Value) {
        let result =
            uninstallation::uninstall_command(&self.world.env(), &self.services(), request);
        assert!(result.stderr.is_empty(), "{}", result.stderr);
        (result.exit_code, result.value.unwrap())
    }
    fn recover(&self) -> (i32, Value) {
        let result = install::recover_command(&self.world.env(), &self.services());
        (result.exit_code, result.value.unwrap())
    }
    fn receipt(&self) -> Value {
        decode(&self.world.receipt_path())
    }
}
fn verified(code: i32, value: &Value) {
    assert_eq!(
        code,
        0,
        "{}",
        json::encode(value, json::INDENT2_ASCII).unwrap()
    );
}
fn refused(code: i32, value: &Value) -> String {
    assert_eq!(
        code,
        1,
        "{}",
        json::encode(value, json::INDENT2_ASCII).unwrap()
    );
    assert_eq!(get(value, "verified"), &Value::Bool(false));
    text(value, "error").to_owned()
}
fn check() -> UpdateRequest {
    UpdateRequest {
        check: true,
        ..UpdateRequest::default()
    }
}
fn begins(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .unwrap()
        .matches(install::BEGIN)
        .count()
}

// ----- child-process harness for environment-driven fault injection -----

/// Child entry: runs one lifecycle command in the World named by the
/// environment and prints its JSON result after a marker. Inert in parents.
#[test]
fn child_entry() {
    let Ok(command) = std::env::var("AM_T2_CHILD") else {
        return;
    };
    let root = PathBuf::from(std::env::var("AGENT_MEMORY_TEST_ROOT").unwrap());
    let token = std::env::var("AGENT_MEMORY_TEST_TOKEN").unwrap();
    let installed = Installed {
        world: World::reopen(&root, &token),
        channel: Channel::open(&root.join("channel")),
    };
    let result = match command.as_str() {
        "recover" => install::recover_command(&installed.world.env(), &installed.services()),
        "update" => updating::update_command(
            &installed.world.env(),
            &installed.services(),
            &UpdateRequest::default(),
        ),
        "uninstall" => uninstallation::uninstall_command(
            &installed.world.env(),
            &installed.services(),
            &UninstallRequest::default(),
        ),
        "purge" => agent_memory::purging::purge_command(
            &installed.world.env(),
            &installed.services(),
            &agent_memory::purging::PurgeRequest {
                repo: installed.world.path("repo"),
                path: installed.world.path("repo/memory-bank"),
                check: false,
                confirm: std::env::var("AM_T2_CONFIRM").ok(),
            },
        ),
        other => panic!("unknown child command {other}"),
    };
    println!(
        "AM-RESULT:{}",
        json::encode(&result.value.unwrap_or(Value::Null), json::COMPACT_ASCII).unwrap()
    );
}

struct Child {
    code: Option<i32>,
    value: Option<Value>,
}
fn child(world: &World, command: &str, extra: &[(&str, String)]) -> Child {
    let mut process = Command::new(std::env::current_exe().unwrap());
    process
        .args([
            "child_entry",
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "-q",
        ])
        .env_clear()
        .env("AM_T2_CHILD", command)
        .current_dir(world.path("repo"));
    for (key, value) in world.pairs() {
        process.env(key, value);
    }
    for (key, value) in extra {
        process.env(key, value);
    }
    let output = process.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = stdout
        .lines()
        .find_map(|line| line.strip_prefix("AM-RESULT:"))
        .map(|line| json::decode(line, Default::default()).unwrap());
    Child {
        code: output.status.code(),
        value,
    }
}
fn fault(stage: &str) -> Vec<(&'static str, String)> {
    vec![
        ("AGENT_MEMORY_FAULT_STAGE", stage.to_owned()),
        ("AGENT_MEMORY_FAULT_MODE", "exit".to_owned()),
    ]
}
fn trace_rows(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| json::decode(line, Default::default()).unwrap())
        .collect()
}

/// `cp -pPR`: the saved World parts keep modes, bytes and symlinks.
fn copy_tree(from: &Path, to: &Path) {
    let status = Command::new("/bin/cp")
        .arg("-pPR")
        .arg(from)
        .arg(to)
        .status()
        .unwrap();
    assert!(status.success());
}
const PARTS: [&str; 6] = [
    "data",
    "state",
    "home",
    "codex",
    "custom discovery with spaces",
    "repo",
];
fn save_parts(world: &World) -> PathBuf {
    let saved = world.path("saved-parts");
    std::fs::create_dir(&saved).unwrap();
    for part in PARTS {
        if world.path(part).exists() {
            copy_tree(&world.path(part), &saved.join(part));
        }
    }
    saved
}
fn restore_parts(world: &World, saved: &Path) {
    for part in PARTS {
        let path = world.path(part);
        if std::fs::symlink_metadata(&path).is_ok() {
            make_writable(&path).unwrap();
            std::fs::remove_dir_all(&path).unwrap();
        }
        if saved.join(part).exists() {
            copy_tree(&saved.join(part), &path);
        }
    }
}

// ======================================================================
// Checkpoint 1: am update (signed release-v2, Rust-to-Rust)
// ======================================================================

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_current_update_checks_ownership_without_scanning_banks
// obligation: tests/contracts/test_signed_release_lifecycle.py::SignedReleaseLifecycle.test_current_signed_update_checks_manifest_without_archive_or_bank_scan
#[test]
fn current_update_checks_ownership_and_manifest_without_archive_or_bank_scan() {
    let installed = Installed::new(Mode::Copy);
    let before = std::fs::read(installed.world.receipt_path()).unwrap();
    // A verified exact manifest needs no archive fetch.
    std::fs::remove_file(installed.channel.assets(BASE).join(release::ARCHIVE)).unwrap();
    std::fs::remove_file(installed.channel.binary(BASE)).unwrap();
    let (code, result) = installed.update(&UpdateRequest::default());
    verified(code, &result);
    assert_eq!(get(&result, "changed"), &Value::Bool(false));
    assert_eq!(get(&result, "update_available"), &Value::Bool(false));
    assert_eq!(get(&result, "restart_clients"), &Value::Array(vec![]));
    assert_eq!(
        text(get(&result, "compatibility"), "bank_scan"),
        "not-needed-no-changes"
    );
    assert_eq!(
        std::fs::read(installed.world.receipt_path()).unwrap(),
        before
    );
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_read_only_check_valid_update_and_repeat_preserve_user_text_and_source_channel
#[test]
fn read_only_check_valid_update_and_repeat_preserve_user_text() {
    for install_mode in [Mode::Link, Mode::Copy] {
        let installed = Installed::new(install_mode);
        let agents = installed.world.path("codex/AGENTS.md");
        let current = std::fs::read_to_string(&agents).unwrap();
        std::fs::write(
            &agents,
            format!("Outside prefix\n{current}Outside suffix\n"),
        )
        .unwrap();
        let user = std::fs::read(&agents).unwrap();
        let mut next = Release::next(b"#!/bin/sh\nprintf 'stand-in am 2 %s\\n' \"$*\"\n");
        next.commit = "c".into();
        let manifest = installed.channel.publish(&next);
        let before = snapshot(&installed.world.root);
        let (code, preview) = installed.update(&check());
        assert_eq!(code, 0, "{preview:?}");
        assert_eq!(get(&preview, "check_complete"), &Value::Bool(true));
        assert_eq!(get(&preview, "verified"), &Value::Bool(false));
        assert_eq!(get(&preview, "update_available"), &Value::Bool(true));
        assert_eq!(snapshot(&installed.world.root), before);
        let (code, result) = installed.update(&UpdateRequest::default());
        verified(code, &result);
        let source = get(get(&result, "receipt"), "source");
        assert_eq!(text(source, "version"), NEXT);
        assert_eq!(text(source, "revision"), "c".repeat(40));
        assert_eq!(
            text(source, "artifact_digest"),
            manifest.targets[install::TARGET].artifact_digest
        );
        let now = std::fs::read_to_string(&agents).unwrap();
        assert!(now.contains("Outside prefix") && now.contains("Outside suffix"));
        assert_eq!(begins(&agents), 1);
        assert_ne!(get(&result, "restart_clients"), &Value::Array(vec![]));
        assert!(installed.runtime_asset_present());
        let receipt = std::fs::read(installed.world.receipt_path()).unwrap();
        let targets_now = owned(&installed.receipt());
        let (code, repeated) = installed.update(&UpdateRequest::default());
        verified(code, &repeated);
        assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
        assert_eq!(get(&repeated, "restart_clients"), &Value::Array(vec![]));
        assert_eq!(
            std::fs::read(installed.world.receipt_path()).unwrap(),
            receipt
        );
        assert_eq!(owned(&installed.receipt()), targets_now);
        assert_eq!(std::fs::read(&agents).unwrap(), user);
    }
}
impl Installed {
    fn runtime_asset_present(&self) -> bool {
        self.world
            .runtime()
            .join("skills/agent-memory/templates/new-runtime-asset.txt")
            .exists()
    }
}

/// The built `am`, and a second build whose bytes (digest and size) differ.
fn built_binaries() -> (Vec<u8>, Vec<u8>) {
    let first = std::fs::read(env!("CARGO_BIN_EXE_am")).unwrap();
    let mut second = first.clone();
    second.push(b'\n');
    (first, second)
}

#[test]
fn rust_to_rust_update_swaps_the_runtime_while_a_running_am_keeps_its_inode() {
    let (first, second) = built_binaries();
    let installed = Installed::with_binary(Mode::Copy, &first);
    let binary = installed.world.runtime().join("bin/am");
    assert_eq!(std::fs::read(&binary).unwrap(), first);
    // A running am holds its executable open: this handle stands for it.
    let mut running = std::fs::File::open(&binary).unwrap();
    let old_inode = std::fs::metadata(&binary).unwrap().ino();
    let old_receipt = installed.receipt();
    let manifest = installed.channel.publish(&Release::next(&second));
    let (code, result) = installed.update(&UpdateRequest::default());
    verified(code, &result);
    let receipt = installed.receipt();
    let source = get(&receipt, "source");
    assert_eq!(text(source, "version"), NEXT);
    assert_eq!(text(source, "target"), install::TARGET);
    assert_eq!(
        text(source, "artifact_digest"),
        manifest.targets[install::TARGET].artifact_digest
    );
    assert_ne!(
        text(source, "artifact_digest"),
        text(get(&old_receipt, "source"), "artifact_digest")
    );
    assert_eq!(std::fs::read(&binary).unwrap(), second);
    assert_ne!(std::fs::metadata(&binary).unwrap().ino(), old_inode);
    let mut kept = Vec::new();
    std::io::Read::read_to_end(&mut running, &mut kept).unwrap();
    assert_eq!(kept, first);
    // The owned command links resolve to the new runtime and dispatch.
    for name in ["am", "agent-memory"] {
        let link = installed.world.bin().join(name);
        assert_eq!(std::fs::read_link(&link).unwrap(), binary);
        let output = Command::new(&link)
            .arg("--help")
            .env_clear()
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    // The old runtime is retained as rollback evidence.
    let retained = get(&receipt, "backups")
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| text(b, "original_path") == installed.world.runtime().to_str().unwrap())
        .any(|b| {
            std::fs::read(Path::new(text(b, "path")).join("bin/am")).is_ok_and(|raw| raw == first)
        });
    assert!(retained);
    let (code, repeated) = installed.update(&UpdateRequest::default());
    verified(code, &repeated);
    assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_downgrade_non_fast_forward_and_invalid_candidate_keep_the_previous_runtime
#[test]
fn downgrade_identity_change_surface_loss_and_unsigned_binary_keep_the_previous_runtime() {
    let installed = Installed::new(Mode::Copy);
    let cases: Vec<(Release, &str)> = vec![
        (
            Release::new("0.0.14", "d", STAND_IN),
            "Runtime downgrade refused",
        ),
        (
            Release::new(BASE, "a", b"#!/bin/sh\nexit 0\n"),
            "Published release version changed identity",
        ),
        (
            {
                let mut value = Release::next(STAND_IN);
                let mut surface = value.surface.clone();
                if let Some(Value::Array(commands)) = surface.get_mut("commands") {
                    commands.retain(|c| text(c, "name") != "uninstall");
                }
                value.surface = surface;
                value
            },
            "Candidate would remove the owned lifecycle command surface",
        ),
        (
            {
                let mut value = Release::next(STAND_IN);
                value.shipped = Some(b"#!/bin/sh\nexit 3\n".to_vec());
                value
            },
            "Release binary differs from its signed manifest",
        ),
    ];
    for (candidate, expected) in cases {
        let before = owned(&installed.receipt());
        let raw = std::fs::read(installed.world.receipt_path()).unwrap();
        installed.channel.publish(&candidate);
        let world_before = without_owner(snapshot(&installed.world.root));
        let (code, preview) = installed.update(&check());
        assert_eq!(refused(code, &preview), expected);
        let (code, denied) = installed.update(&UpdateRequest::default());
        assert_eq!(refused(code, &denied), expected);
        assert_eq!(owned(&installed.receipt()), before, "{expected}");
        assert_eq!(std::fs::read(installed.world.receipt_path()).unwrap(), raw);
        assert_eq!(without_owner(snapshot(&installed.world.root)), world_before);
    }
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_same_version_still_reconciles_changed_owned_receipt
#[test]
fn same_version_still_reconciles_changed_owned_receipt() {
    let installed = Installed::new(Mode::Copy);
    let agents = installed.world.path("codex/AGENTS.md");
    let mut current = std::fs::read_to_string(&agents).unwrap();
    current.push_str("User text added outside the managed block.\n");
    std::fs::write(&agents, &current).unwrap();
    let old = std::fs::read(installed.world.receipt_path()).unwrap();
    let (code, result) = installed.update(&UpdateRequest::default());
    verified(code, &result);
    assert_eq!(get(&result, "changed"), &Value::Bool(true));
    assert_eq!(get(&result, "update_available"), &Value::Bool(false));
    assert_ne!(std::fs::read(installed.world.receipt_path()).unwrap(), old);
    assert!(
        std::fs::read_to_string(&agents)
            .unwrap()
            .contains("User text added outside the managed block.")
    );
    let (code, next) = installed.update(&UpdateRequest::default());
    verified(code, &next);
    assert_eq!(get(&next, "changed"), &Value::Bool(false));
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_dirty_remote_ref_and_unowned_target_changes_refuse_without_activation
#[test]
fn unowned_target_and_runtime_changes_refuse_without_activation() {
    for case in ["runtime-edit", "copy-edit", "block-edit"] {
        let installed = Installed::new(Mode::Copy);
        installed.channel.publish(&Release::next(STAND_IN));
        match case {
            "runtime-edit" => write(
                &installed.world.runtime().join("user-note.txt"),
                "Keep this local edit.\n",
            ),
            "copy-edit" => write(
                &installed
                    .world
                    .path("codex/skills/agent-memory/user-note.txt"),
                "Keep my copy edit.\n",
            ),
            _ => {
                let path = installed.world.path("codex/AGENTS.md");
                let value = std::fs::read_to_string(&path)
                    .unwrap()
                    .replace("agent-memory", "my custom block");
                std::fs::write(&path, value).unwrap();
            }
        }
        let before = owned(&installed.receipt());
        let raw = std::fs::read(installed.world.receipt_path()).unwrap();
        let (code, denied) = installed.update(&UpdateRequest::default());
        refused(code, &denied);
        assert_eq!(owned(&installed.receipt()), before, "{case}");
        assert_eq!(std::fs::read(installed.world.receipt_path()).unwrap(), raw);
        let check_before = without_owner(snapshot(&installed.world.root));
        let (code, preview) = installed.update(&check());
        refused(code, &preview);
        assert_eq!(without_owner(snapshot(&installed.world.root)), check_before);
    }
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_dangling_lifecycle_records_are_preserved_and_refuse_update
#[test]
fn dangling_lifecycle_records_are_preserved_and_refuse_update() {
    for name in ["owner.json", "journal.json"] {
        let installed = Installed::new(Mode::Copy);
        installed.channel.publish(&Release::next(STAND_IN));
        let path = installed.world.state().join(name);
        let _ = std::fs::remove_file(&path);
        symlink(installed.world.path("unrelated missing record"), &path).unwrap();
        let link = std::fs::read_link(&path).unwrap();
        let targets_before = owned(&installed.receipt());
        let raw = std::fs::read(installed.world.receipt_path()).unwrap();
        if name == "journal.json" {
            let all = snapshot(&installed.world.root);
            let (code, preview) = installed.update(&check());
            refused(code, &preview);
            assert_eq!(snapshot(&installed.world.root), all);
        }
        let (code, result) = installed.update(&UpdateRequest::default());
        refused(code, &result);
        assert_eq!(std::fs::read_link(&path).unwrap(), link);
        assert_eq!(owned(&installed.receipt()), targets_before);
        assert_eq!(std::fs::read(installed.world.receipt_path()).unwrap(), raw);
    }
}

fn rewrite_receipt(world: &World, edit: impl FnOnce(&mut Value)) {
    let path = world.receipt_path();
    let mut receipt = decode(&path);
    edit(&mut receipt);
    std::fs::remove_file(&path).unwrap();
    write(&path, install::encoded(&receipt).unwrap());
    mode(&path, 0o600);
    // Keep the committed journal consistent with the edited receipt.
    let journal_path = world.state().join("journal.json");
    if journal_path.exists() {
        let mut journal = decode(&journal_path);
        let identity = install::fingerprint(&path).unwrap();
        if let Value::Object(fields) = &mut journal {
            for (key, value) in fields.iter_mut() {
                match key.as_str() {
                    "receipt" => *value = receipt.clone(),
                    "receipt_identity" => *value = identity.clone(),
                    _ => {}
                }
            }
        }
        std::fs::remove_file(&journal_path).unwrap();
        write(&journal_path, install::encoded(&journal).unwrap());
        mode(&journal_path, 0o600);
    }
}
fn set_field(value: &mut Value, key: &str, field: Value) {
    if let Value::Object(fields) = value {
        match fields.iter_mut().find(|(name, _)| name == key) {
            Some((_, slot)) => *slot = field,
            None => fields.push((key.to_owned(), field)),
        }
    }
}

#[test]
fn python_era_receipts_and_managed_channels_refuse_with_rebootstrap_guidance() {
    for (schema, kind, expected) in [
        (
            "agent-memory/install-receipt-v1",
            "release",
            install::ADOPTION_GUIDANCE,
        ),
        (
            // v2 additionally requires an OMP binding; v1 covers the managed row.
            "agent-memory/install-receipt-v1",
            "managed",
            install::ADOPTION_GUIDANCE,
        ),
        (
            "agent-memory/install-receipt-v3",
            "managed",
            install::MANAGED_GUIDANCE,
        ),
    ] {
        let installed = Installed::new(Mode::Copy);
        installed.channel.publish(&Release::next(STAND_IN));
        rewrite_receipt(&installed.world, |receipt| {
            set_field(receipt, "schema", Value::String(schema.into()));
            if let Some(source) = receipt.get_mut("source") {
                set_field(source, "kind", Value::String(kind.into()));
            }
            if schema != "agent-memory/install-receipt-v3" {
                if let Some(Value::Array(items)) = receipt.get_mut("targets") {
                    items.retain(|t| text(t, "kind") != "command-link");
                }
            }
        });
        let before = without_owner(snapshot(&installed.world.root));
        for request in [check(), UpdateRequest::default()] {
            let (code, result) = installed.update(&request);
            assert_eq!(refused(code, &result), expected, "{schema} {kind}");
        }
        assert_eq!(without_owner(snapshot(&installed.world.root)), before);
    }
}

#[test]
fn a_release_v1_manifest_refuses_with_the_rebootstrap_guidance() {
    let installed = Installed::new(Mode::Copy);
    let legacy = format!(
        "{{\"schema\":\"agent-memory/release-v1\",\"repository\":\"{}\",\"tag\":\"v{NEXT}\",\"version\":\"{NEXT}\",\"python_minimum\":\"3.11\"}}\n",
        release::REPOSITORY
    );
    installed
        .channel
        .put_manifest(NEXT, legacy.as_bytes(), b"not an archive");
    let before = without_owner(snapshot(&installed.world.root));
    for request in [check(), UpdateRequest::default()] {
        let (code, result) = installed.update(&request);
        assert_eq!(refused(code, &result), release::LEGACY_GUIDANCE);
    }
    assert_eq!(without_owner(snapshot(&installed.world.root)), before);
}

// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_development_sources_are_never_mutated_by_update
#[test]
fn development_sources_are_never_mutated_by_update() {
    let world = World::new();
    let channel = Channel::new(&world);
    let source = development_source(&world, "developer source");
    let services = services_with(&world, channel.access());
    let installed = install::install_command(
        &world.env(),
        &services,
        &InstallRequest {
            development_source: Some(source.clone()),
            presets: Some("codex".into()),
            ..InstallRequest::default()
        },
    );
    assert_eq!(installed.exit_code, 0, "{}", installed.stdout);
    channel.publish(&Release::next(STAND_IN));
    let before = without_owner(snapshot(&world.root));
    let result = updating::update_command(&world.env(), &services, &UpdateRequest::default());
    assert_eq!(
        refused(result.exit_code, result.value.as_ref().unwrap()),
        updating::RECORDED_GUIDANCE
    );
    assert_eq!(without_owner(snapshot(&world.root)), before);
}

/// A development source: the release fixture pack plus a prebuilt stand-in am.
fn development_source(world: &World, name: &str) -> PathBuf {
    let root = world.path(name);
    for (relative, (raw, bits)) in pack(BASE, &[]) {
        let path = root.join("skills/agent-memory").join(relative);
        write(&path, raw);
        mode(&path, bits);
    }
    for entry in walk(&root) {
        if entry.is_dir() {
            mode(&entry, 0o755);
        }
    }
    let binary = root.join("target/release/am");
    write(&binary, STAND_IN);
    mode(&binary, 0o755);
    root
}

// obligation: tests/contracts/test_lifecycle_lost_responses.py::LifecycleLostResponses.test_update_response_is_lost_after_real_new_revision_is_active
// obligation: tests/contracts/test_runtime_update.py::RuntimeUpdateContract.test_lost_response_and_partial_activation_recover_then_retry_without_duplicates
#[test]
fn update_response_lost_and_partial_activation_recover_then_retry_without_duplicates() {
    let installed = Installed::new(Mode::Copy);
    installed.channel.publish(&Release::next(STAND_IN));
    let lost = child(&installed.world, "update", &fault("update-response-lost"));
    assert_eq!(lost.code, Some(86));
    let committed = std::fs::read(installed.world.receipt_path()).unwrap();
    assert_eq!(
        text(
            get(&decode(&installed.world.receipt_path()), "source"),
            "version"
        ),
        NEXT
    );
    let (code, recovered) = installed.recover();
    verified(code, &recovered);
    let (code, repeated) = installed.update(&UpdateRequest::default());
    verified(code, &repeated);
    assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
    assert_eq!(
        std::fs::read(installed.world.receipt_path()).unwrap(),
        committed
    );
    // Partial activation: interrupted after the first exchanged target.
    let partial = Installed::new(Mode::Copy);
    partial.channel.publish(&Release::next(STAND_IN));
    let previous = owned(&partial.receipt());
    let killed = child(&partial.world, "update", &fault("update-after-activate-0"));
    assert_eq!(killed.code, Some(86));
    let (code, recovered) = partial.recover();
    verified(code, &recovered);
    assert_eq!(owned(&partial.receipt()), previous);
    let (code, retried) = partial.update(&UpdateRequest::default());
    verified(code, &retried);
    assert_eq!(
        text(get(get(&retried, "receipt"), "source"), "version"),
        NEXT
    );
    assert_eq!(begins(&partial.world.path("codex/AGENTS.md")), 1);
}

/// Every observed logical update write of `phase`, interrupted (exit 86),
/// recovered and retried to the clean outcome; banks are never written.
fn replay_update(phase: &str) {
    let installed = Installed::new(Mode::Copy);
    let world = &installed.world;
    write(
        &world.path("repo/memory-bank/MEMORY.md"),
        "---\nschema: agent-memory/v1\n---\nBank bytes are never touched.\n",
    );
    let memory = snapshot(&world.path("repo/memory-bank"));
    let next = installed.channel.publish(&Release::next(STAND_IN));
    let digest = next.targets[install::TARGET].artifact_digest.clone();
    let previous = owned(&installed.receipt());
    let saved = save_parts(world);
    let trace = world.path("clean-trace.jsonl");
    let clean = child(
        world,
        "update",
        &[(
            "AGENT_MEMORY_FAULT_TRACE",
            trace.to_string_lossy().into_owned(),
        )],
    );
    assert_eq!(
        get(clean.value.as_ref().unwrap(), "verified"),
        &Value::Bool(true),
        "{:?}",
        clean.value
    );
    let rows = trace_rows(&trace);
    let steps: BTreeSet<u64> = rows
        .iter()
        .map(|r| get(r, "step").as_int().unwrap().as_u64().unwrap())
        .collect();
    assert_eq!(steps, (0..steps.len() as u64).collect());
    assert_eq!(rows.len(), 2 * steps.len());
    let kinds: BTreeSet<String> = rows.iter().map(|r| text(r, "kind").to_owned()).collect();
    for kind in [
        "file-stage",
        "record-publication",
        "payload-snapshot",
        "atomic-node-switch",
    ] {
        assert!(kinds.contains(kind), "{kind}: {kinds:?}");
    }
    let selected: Vec<&Value> = rows.iter().filter(|r| text(r, "phase") == phase).collect();
    assert_eq!(selected.len(), steps.len());
    for partition in 0..8u64 {
        assert!(
            selected
                .iter()
                .any(|r| get(r, "step").as_int().unwrap().as_u64().unwrap() % 8 == partition),
            "partition {partition} of update-{phase} is empty"
        );
    }
    let mut executed = Vec::new();
    for row in &selected {
        let point = text(row, "point").to_owned();
        restore_parts(world, &saved);
        let actual = world.path("actual-trace.jsonl");
        let _ = std::fs::remove_file(&actual);
        let mut extra = fault(&point);
        extra.push((
            "AGENT_MEMORY_FAULT_TRACE",
            actual.to_string_lossy().into_owned(),
        ));
        let exited = child(world, "update", &extra);
        assert_eq!(exited.code, Some(86), "{point}");
        assert_eq!(text(trace_rows(&actual).last().unwrap(), "point"), point);
        let (code, recovered) = installed.recover();
        verified(code, &recovered);
        let current = installed.receipt();
        if text(get(&current, "source"), "version") != NEXT {
            assert_eq!(owned(&current), previous, "{point}");
        }
        assert_eq!(snapshot(&world.path("repo/memory-bank")), memory);
        let (code, retried) = installed.update(&UpdateRequest::default());
        verified(code, &retried);
        let source = get(get(&retried, "receipt"), "source");
        assert_eq!(text(source, "version"), NEXT, "{point}");
        assert_eq!(text(source, "artifact_digest"), digest);
        assert_eq!(snapshot(&world.path("repo/memory-bank")), memory);
        assert_eq!(begins(&world.path("codex/AGENTS.md")), 1);
        executed.push(point);
    }
    let expected: Vec<String> = selected
        .iter()
        .map(|r| text(r, "point").to_owned())
        .collect();
    assert_eq!(executed, expected);
}

// obligation: tests/contracts/test_update_write_before.py::UpdateWriteBeforeContract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_update_write_before.py::UpdateWriteBeforeContract.test_actual_logical_writes_in_partition_4
// obligation: tests/contracts/test_update_write_before_part_1.py::UpdateWriteBeforePart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_update_write_before_part_1.py::UpdateWriteBeforePart1Contract.test_actual_logical_writes_in_partition_5
// obligation: tests/contracts/test_update_write_before_part_2.py::UpdateWriteBeforePart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_update_write_before_part_2.py::UpdateWriteBeforePart2Contract.test_actual_logical_writes_in_partition_6
// obligation: tests/contracts/test_update_write_before_part_3.py::UpdateWriteBeforePart3Contract.test_actual_logical_writes_in_partition_3
// obligation: tests/contracts/test_update_write_before_part_3.py::UpdateWriteBeforePart3Contract.test_actual_logical_writes_in_partition_7
#[test]
fn every_update_write_before_phase_interrupts_recovers_and_retries() {
    replay_update("before");
}

// obligation: tests/contracts/test_update_write_after.py::UpdateWriteAfterContract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_update_write_after.py::UpdateWriteAfterContract.test_actual_logical_writes_in_partition_4
// obligation: tests/contracts/test_update_write_after_part_1.py::UpdateWriteAfterPart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_update_write_after_part_1.py::UpdateWriteAfterPart1Contract.test_actual_logical_writes_in_partition_5
// obligation: tests/contracts/test_update_write_after_part_2.py::UpdateWriteAfterPart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_update_write_after_part_2.py::UpdateWriteAfterPart2Contract.test_actual_logical_writes_in_partition_6
// obligation: tests/contracts/test_update_write_after_part_3.py::UpdateWriteAfterPart3Contract.test_actual_logical_writes_in_partition_3
// obligation: tests/contracts/test_update_write_after_part_3.py::UpdateWriteAfterPart3Contract.test_actual_logical_writes_in_partition_7
#[test]
fn every_update_write_after_phase_interrupts_recovers_and_retries() {
    replay_update("after");
}

impl Installed {
    /// Services whose compiled version is `version` (a re-run bootstrap binary).
    fn bootstrap_services(&self, version: &str) -> Services {
        let mut access = self.channel.access();
        access.version = version.into();
        services_with(&self.world, access)
    }
    fn bootstrap_update(&self, binary: &Path, check: bool) -> (i32, Value) {
        let request = UpdateRequest {
            check,
            bootstrap_binary: Some(binary.to_path_buf()),
            ..UpdateRequest::default()
        };
        let result =
            updating::update_command(&self.world.env(), &self.bootstrap_services(NEXT), &request);
        assert!(result.stderr.is_empty(), "{}", result.stderr);
        (result.exit_code, result.value.unwrap())
    }
}

#[test]
fn rerun_bootstrap_update_installs_exactly_the_bound_binary_bytes_without_redownload() {
    let installed = Installed::new(Mode::Copy);
    let next_binary = b"#!/bin/sh\nprintf 'bootstrap am %s\\n' \"$*\"\n";
    let manifest = installed.channel.publish(&Release::next(next_binary));
    // The re-run bootstrap verified and kept its own copy; the published
    // binary asset is gone, so any re-download would refuse.
    let bootstrap = installed.world.path("bootstrap/am");
    write(&bootstrap, next_binary);
    mode(&bootstrap, 0o755);
    std::fs::remove_file(installed.channel.binary(NEXT)).unwrap();
    let before = without_owner(snapshot(&installed.world.root));
    let (code, preview) = installed.bootstrap_update(&bootstrap, true);
    assert_eq!(code, 0, "{preview:?}");
    assert_eq!(get(&preview, "update_available"), &Value::Bool(true));
    assert_eq!(without_owner(snapshot(&installed.world.root)), before);
    let (code, result) = installed.bootstrap_update(&bootstrap, false);
    verified(code, &result);
    let source = get(get(&result, "receipt"), "source");
    assert_eq!(text(source, "version"), NEXT);
    assert_eq!(
        text(source, "artifact_digest"),
        manifest.targets[install::TARGET].artifact_digest
    );
    assert_eq!(
        std::fs::read(installed.world.runtime().join("bin/am")).unwrap(),
        next_binary
    );
}

#[test]
fn rerun_bootstrap_update_refuses_unbound_or_missing_binary_bytes_before_writes() {
    let installed = Installed::new(Mode::Copy);
    installed.channel.publish(&Release::next(STAND_IN));
    let other = installed.world.path("bootstrap/am");
    write(&other, b"#!/bin/sh\nexit 0\n");
    let before = without_owner(snapshot(&installed.world.root));
    for (binary, expected) in [
        (
            other.clone(),
            "Bootstrap binary differs from its signed release; re-run the trusted bootstrap",
        ),
        (
            installed.world.path("bootstrap/missing"),
            "Bootstrap binary is unavailable or not an ordinary file; re-run the trusted bootstrap",
        ),
    ] {
        for check in [true, false] {
            let (code, result) = installed.bootstrap_update(&binary, check);
            assert_eq!(refused(code, &result), expected);
        }
    }
    // A bootstrap binary for another compiled version is never bound.
    let request = UpdateRequest {
        bootstrap_binary: Some(installed.channel.binary(NEXT)),
        ..UpdateRequest::default()
    };
    let result = updating::update_command(
        &installed.world.env(),
        &installed.bootstrap_services(BASE),
        &request,
    );
    assert_eq!(
        refused(result.exit_code, result.value.as_ref().unwrap()),
        "Bootstrap binary version differs from its signed release; re-run the trusted bootstrap"
    );
    assert_eq!(without_owner(snapshot(&installed.world.root)), before);
}

#[test]
fn version_order_follows_the_baseline_version_key() {
    let key = |v: &str| updating::version_key(v).unwrap();
    assert!(key("1.0.1") > key("1.0.0"));
    assert!(key("1.0.0") > key("1.0.0-rc.1"));
    assert!(key("1.0.0-rc.2") > key("1.0.0-rc.1"));
    assert!(key("1.0.0-rc.1") > key("1.0.0-11"));
    assert!(key("1.10.0") > key("1.9.9"));
    assert!(key("1.0.0+build.7") == key("1.0.0"));
    assert!(updating::version_key("1.0").is_err());
}

#[test]
fn router_metadata_locates_only_explicit_format_headers() {
    assert!(
        updating::router_metadata(b"plain notes\n")
            .unwrap()
            .is_none()
    );
    let valid = b"---\nschema: agent-memory/v1\nproject-id: 8e0b66aa-2be5-4fa3-b47f-1245474aa7dd\n---\nBody\n";
    let metadata = updating::router_metadata(valid).unwrap().unwrap();
    assert_eq!(text(&metadata, "schema"), "agent-memory/v1");
    for invalid in [
        &b"intro\nschema: agent-memory/v1\n"[..],
        &b"---\nschema: agent-memory/v1\n"[..],
        &b"---\nschema: agent-memory/v1\nproject-id: nope\n---\n"[..],
    ] {
        assert!(updating::router_metadata(invalid).is_err());
    }
}

// ======================================================================
// Checkpoint 1: am uninstall
// ======================================================================

fn uninstall_request(check: bool) -> UninstallRequest {
    UninstallRequest {
        check,
        ..UninstallRequest::default()
    }
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_preview_remove_repeat_and_reinstall_preserve_bank_original_settings_and_backups
#[test]
fn preview_remove_repeat_and_reinstall_preserve_bank_original_settings_and_backups() {
    for install_mode in [Mode::Link, Mode::Copy] {
        let installed = Installed::new(install_mode);
        let world = &installed.world;
        write(
            &world.path("repo/memory-bank/MEMORY.md"),
            "---\nschema: agent-memory/v1\n---\nBank bytes are never touched.\n",
        );
        let bank = snapshot(&world.path("repo/memory-bank"));
        let receipt = installed.receipt();
        let backups: BTreeMap<String, BTreeMap<String, String>> = get(&receipt, "backups")
            .as_array()
            .unwrap()
            .iter()
            .map(|b| {
                (
                    text(b, "path").to_owned(),
                    snapshot(Path::new(text(b, "path"))),
                )
            })
            .collect();
        let before = snapshot(&world.root);
        let (code, preview) = installed.uninstall(&uninstall_request(true));
        assert_eq!(code, 0, "{preview:?}");
        assert_eq!(get(&preview, "check_complete"), &Value::Bool(true));
        assert_eq!(get(&preview, "verified"), &Value::Bool(false));
        assert_eq!(snapshot(&world.root), before);
        let (code, result) = installed.uninstall(&uninstall_request(false));
        verified(code, &result);
        assert_eq!(get(&result, "uninstalled"), &Value::Bool(true));
        assert_eq!(get(&result, "preserved"), &Value::Array(vec![]));
        assert_eq!(
            get(&result, "project_memory_writes_performed"),
            &Value::Bool(false)
        );
        for target in targets(&receipt) {
            let path = PathBuf::from(text(&target, "path"));
            if text(&target, "kind") == "activation" {
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    b"User instructions stay here.\n"
                );
            } else {
                assert!(
                    std::fs::symlink_metadata(&path).is_err(),
                    "{}",
                    path.display()
                );
            }
        }
        assert!(!world.runtime().exists());
        assert_eq!(snapshot(&world.path("repo/memory-bank")), bank);
        for (path, value) in &backups {
            assert_eq!(&snapshot(Path::new(path)), value);
        }
        assert!(flag(&installed.receipt(), "uninstalled"));
        let reinstall = get(&result, "reinstall");
        assert_eq!(
            get(reinstall, "argv").as_array().unwrap()[..2],
            [Value::String("am".into()), Value::String("install".into())]
        );
        let (code, repeated) = installed.uninstall(&uninstall_request(false));
        verified(code, &repeated);
        assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
        assert_eq!(snapshot(&world.path("repo/memory-bank")), bank);
        let restored = installed.install(install_mode);
        assert_eq!(
            get(&restored, "verified"),
            &Value::Bool(true),
            "{restored:?}"
        );
        assert_eq!(snapshot(&world.path("repo/memory-bank")), bank);
    }
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_modified_copy_and_block_are_preserved_and_source_is_retained
#[test]
fn modified_copy_and_block_are_preserved_and_runtime_is_retained() {
    let installed = Installed::new(Mode::Copy);
    let world = &installed.world;
    let note = world.path("codex/skills/agent-memory/user.txt");
    write(&note, "Keep this user edit.\n");
    let settings = world.path("codex/AGENTS.md");
    let edited = std::fs::read_to_string(&settings)
        .unwrap()
        .replace("agent-memory", "my custom instructions");
    std::fs::write(&settings, &edited).unwrap();
    let runtime = snapshot(&world.runtime());
    let (code, result) = installed.uninstall(&uninstall_request(false));
    verified(code, &result);
    let preserved = get(&result, "preserved").as_array().unwrap();
    assert!(!preserved.is_empty());
    assert!(preserved.iter().any(|p| text(p, "kind") == "source"));
    assert_eq!(
        std::fs::read_to_string(&note).unwrap(),
        "Keep this user edit.\n"
    );
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), edited);
    assert_eq!(snapshot(&world.runtime()), runtime);
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_user_added_blank_lines_outside_the_owned_block_are_not_removed
#[test]
fn user_added_blank_lines_outside_the_owned_block_are_not_removed() {
    for position in ["before", "after"] {
        let installed = Installed::new(Mode::Copy);
        let path = installed.world.path("codex/AGENTS.md");
        let mut raw = std::fs::read_to_string(&path).unwrap();
        if position == "before" {
            raw = raw.replacen(install::BEGIN, &format!("\n{}", install::BEGIN), 1);
        } else {
            raw.push('\n');
        }
        std::fs::write(&path, &raw).unwrap();
        let start = raw.find(install::BEGIN).unwrap();
        let stop = raw.find(install::END).unwrap() + install::END.len();
        let expected = format!("{}{}", &raw[..start], &raw[stop..]);
        let (code, result) = installed.uninstall(&uninstall_request(false));
        verified(code, &result);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            expected,
            "{position}"
        );
    }
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_allowed_outside_edits_survive_uninstall_even_after_update_captures_them
#[test]
fn allowed_outside_edits_survive_uninstall_even_after_update_captures_them() {
    let installed = Installed::new(Mode::Copy);
    let settings = installed.world.path("codex/AGENTS.md");
    let current = std::fs::read_to_string(&settings).unwrap();
    std::fs::write(
        &settings,
        format!("Additional prefix.\n{current}Additional suffix.\n"),
    )
    .unwrap();
    let (code, updated) = installed.update(&UpdateRequest::default());
    verified(code, &updated);
    let (code, result) = installed.uninstall(&uninstall_request(false));
    verified(code, &result);
    let text_now = std::fs::read_to_string(&settings).unwrap();
    assert!(text_now.contains("Additional prefix.\nUser instructions stay here.\n"));
    assert!(text_now.ends_with("Additional suffix.\n"));
    assert!(!text_now.contains("agent-memory:activation"));
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_missing_managed_source_is_removed_from_ownership_using_trusted_helper
#[test]
fn missing_runtime_is_removed_from_ownership() {
    let installed = Installed::new(Mode::Link);
    make_writable(&installed.world.runtime()).unwrap();
    std::fs::remove_dir_all(installed.world.runtime()).unwrap();
    let (code, result) = installed.uninstall(&uninstall_request(false));
    verified(code, &result);
    assert_eq!(get(&result, "uninstalled"), &Value::Bool(true));
    assert!(std::fs::symlink_metadata(installed.world.bin().join("agent-memory")).is_err());
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_interrupted_removal_recovers_and_retry_converges
#[test]
fn interrupted_removal_recovers_and_retry_converges() {
    let installed = Installed::new(Mode::Copy);
    let before = owned(&installed.receipt());
    let killed = child(
        &installed.world,
        "uninstall",
        &fault("uninstall-after-activate-0"),
    );
    assert_eq!(killed.code, Some(86));
    let (code, recovered) = installed.recover();
    verified(code, &recovered);
    assert_eq!(owned(&installed.receipt()), before);
    let (code, result) = installed.uninstall(&uninstall_request(false));
    verified(code, &result);
    assert!(!installed.world.runtime().exists());
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_lost_uninstall_success_response_is_recovered_as_committed
// obligation: tests/contracts/test_lifecycle_lost_responses.py::LifecycleLostResponses.test_uninstall_response_is_lost_after_runtime_is_removed_and_data_retained
#[test]
fn lost_uninstall_responses_are_recovered_as_committed() {
    for stage in ["uninstall-after-receipt", "uninstall-response-lost"] {
        let installed = Installed::new(Mode::Copy);
        write(
            &installed.world.path("repo/memory-bank/MEMORY.md"),
            "---\nschema: agent-memory/v1\n---\nRetained.\n",
        );
        let bank = snapshot(&installed.world.path("repo/memory-bank"));
        let killed = child(&installed.world, "uninstall", &fault(stage));
        assert_eq!(killed.code, Some(86), "{stage}");
        assert!(!installed.world.runtime().exists());
        let (code, recovered) = installed.recover();
        verified(code, &recovered);
        let (code, repeated) = installed.uninstall(&uninstall_request(false));
        verified(code, &repeated);
        assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
        assert_eq!(snapshot(&installed.world.path("repo/memory-bank")), bank);
    }
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_missing_receipt_refuses_without_guessing_ownership
#[test]
fn missing_receipt_refuses_without_guessing_ownership() {
    let installed = Installed::new(Mode::Copy);
    let receipt = installed.receipt();
    std::fs::remove_file(installed.world.receipt_path()).unwrap();
    let targets_before = owned(&receipt);
    let all = snapshot(&installed.world.root);
    let (code, preview) = installed.uninstall(&uninstall_request(true));
    refused(code, &preview);
    assert_eq!(snapshot(&installed.world.root), all);
    let (code, result) = installed.uninstall(&uninstall_request(false));
    refused(code, &result);
    assert_eq!(owned(&receipt), targets_before);
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_preview_refuses_a_receipt_that_conflicts_with_its_completed_journal
#[test]
fn preview_refuses_a_receipt_that_conflicts_with_its_completed_journal() {
    for case in ["inactive-flag", "active-receipt"] {
        let installed = Installed::new(Mode::Copy);
        let path = installed.world.receipt_path();
        let mut value = decode(&path);
        if case == "inactive-flag" {
            set_field(&mut value, "uninstalled", Value::Bool(true));
            set_field(&mut value, "preserved", Value::Array(vec![]));
            set_field(
                &mut value,
                "reinstall",
                Value::Object(vec![(
                    "argv".into(),
                    Value::Array(vec![Value::String("example".into())]),
                )]),
            );
        } else {
            set_field(
                &mut value,
                "installed_at",
                Value::String("externally changed".into()),
            );
        }
        std::fs::remove_file(&path).unwrap();
        write(&path, install::encoded(&value).unwrap());
        mode(&path, 0o600);
        let before = snapshot(&installed.world.root);
        let (code, denied) = installed.uninstall(&uninstall_request(true));
        refused(code, &denied);
        assert_eq!(snapshot(&installed.world.root), before, "{case}");
    }
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_development_source_is_preserved
#[test]
fn development_source_is_preserved() {
    let world = World::new();
    let channel = Channel::new(&world);
    let source = development_source(&world, "developer source");
    write(
        &source.join("developer-notes.md"),
        "Preserve this developer edit.\n",
    );
    let services = services_with(&world, channel.access());
    let result = install::install_command(
        &world.env(),
        &services,
        &InstallRequest {
            development_source: Some(source.clone()),
            presets: Some("codex".into()),
            ..InstallRequest::default()
        },
    );
    assert_eq!(result.exit_code, 0, "{}", result.stdout);
    let before = snapshot(&source);
    let removed =
        uninstallation::uninstall_command(&world.env(), &services, &uninstall_request(false));
    let value = removed.value.unwrap();
    verified(removed.exit_code, &value);
    assert_eq!(snapshot(&source), before);
    let argv = get(get(&value, "reinstall"), "argv").as_array().unwrap();
    assert!(
        argv.iter()
            .any(|a| a.as_str() == Some("--development-source"))
    );
}

// obligation: tests/contracts/test_runtime_uninstall.py::RuntimeUninstallContract.test_shared_hooks_actual_uninstall_and_committed_recovery
#[test]
fn shared_hooks_actual_uninstall_and_committed_recovery() {
    for interrupted in [false, true] {
        let installed = Installed::new(Mode::Copy);
        let path = installed.world.path("codex/hooks.json");
        let mut value = decode(&path);
        let foreign = json::decode(
            r#"{"hooks":[{"type":"command","command":"printf unrelated"}]}"#,
            Default::default(),
        )
        .unwrap();
        set_field(
            &mut value,
            "otherToolSetting",
            json::decode(r#"{"preserve":true}"#, Default::default()).unwrap(),
        );
        for event in ["UserPromptSubmit", "Stop"] {
            if let Some(Value::Array(groups)) = value
                .get_mut("hooks")
                .and_then(|hooks| hooks.get_mut(event))
            {
                groups.push(foreign.clone());
            }
        }
        std::fs::write(&path, json::encode(&value, json::COMPACT_ASCII).unwrap()).unwrap();
        let file_mode = std::fs::metadata(&path).unwrap().mode() & 0o777;
        let before = snapshot(&installed.world.root);
        let (code, preview) = installed.uninstall(&uninstall_request(true));
        assert_eq!(code, 0, "{preview:?}");
        assert_eq!(get(&preview, "preserved"), &Value::Array(vec![]));
        assert_eq!(snapshot(&installed.world.root), before);
        if interrupted {
            let killed = child(
                &installed.world,
                "uninstall",
                &fault("uninstall-after-receipt"),
            );
            assert_eq!(killed.code, Some(86));
            let (code, recovered) = installed.recover();
            verified(code, &recovered);
        } else {
            let (code, result) = installed.uninstall(&uninstall_request(false));
            verified(code, &result);
            assert_eq!(get(&result, "preserved"), &Value::Array(vec![]));
        }
        let remaining = decode(&path);
        assert_eq!(
            get(&remaining, "otherToolSetting"),
            &json::decode(r#"{"preserve":true}"#, Default::default()).unwrap()
        );
        for event in ["UserPromptSubmit", "Stop"] {
            assert_eq!(
                get(get(&remaining, "hooks"), event),
                &Value::Array(vec![foreign.clone()])
            );
        }
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, file_mode);
        assert!(flag(&installed.receipt(), "uninstalled"));
    }
}

#[test]
fn python_era_receipts_refuse_uninstall_with_rebootstrap_guidance() {
    let installed = Installed::new(Mode::Copy);
    rewrite_receipt(&installed.world, |receipt| {
        set_field(
            receipt,
            "schema",
            Value::String("agent-memory/install-receipt-v1".into()),
        );
        if let Some(Value::Array(items)) = receipt.get_mut("targets") {
            items.retain(|t| text(t, "kind") != "command-link");
        }
    });
    let before = without_owner(snapshot(&installed.world.root));
    for check in [true, false] {
        let (code, result) = installed.uninstall(&uninstall_request(check));
        assert_eq!(refused(code, &result), install::ADOPTION_GUIDANCE);
    }
    assert_eq!(without_owner(snapshot(&installed.world.root)), before);
}

/// Receipt semantics a retried uninstall must reproduce.
fn uninstall_semantics(receipt: &Value) -> String {
    let rows: Vec<Value> = targets(receipt)
        .iter()
        .map(|t| {
            Value::Array(vec![
                get(t, "kind").clone(),
                get(t, "path").clone(),
                get(t, "identity").clone(),
            ])
        })
        .collect();
    json::encode(
        &Value::Array(vec![
            get(receipt, "uninstalled").clone(),
            get(receipt, "preserved").clone(),
            Value::Array(rows),
        ]),
        json::SORTED_COMPACT_UTF8,
    )
    .unwrap()
}

fn replay_uninstall(install_mode: Mode, phase: &str) {
    let installed = Installed::new(install_mode);
    let world = &installed.world;
    write(
        &world.path("repo/memory-bank/MEMORY.md"),
        "---\nschema: agent-memory/v1\n---\nBank bytes are never touched.\n",
    );
    let memory = snapshot(&world.path("repo/memory-bank"));
    let installation_id = text(&installed.receipt(), "installation_id").to_owned();
    let saved = save_parts(world);
    let trace = world.path("clean-trace.jsonl");
    let clean = child(
        world,
        "uninstall",
        &[(
            "AGENT_MEMORY_FAULT_TRACE",
            trace.to_string_lossy().into_owned(),
        )],
    );
    assert_eq!(
        get(clean.value.as_ref().unwrap(), "verified"),
        &Value::Bool(true),
        "{:?}",
        clean.value
    );
    let expected_receipt = installed.receipt();
    let expected = uninstall_semantics(&expected_receipt);
    let expected_nodes: BTreeMap<String, BTreeMap<String, String>> = owned(&expected_receipt);
    let rows = trace_rows(&trace);
    let kinds: BTreeSet<String> = rows.iter().map(|r| text(r, "kind").to_owned()).collect();
    for kind in ["file-stage", "record-publication", "atomic-node-switch"] {
        assert!(kinds.contains(kind), "{kind}: {kinds:?}");
    }
    let selected: Vec<&Value> = rows.iter().filter(|r| text(r, "phase") == phase).collect();
    // The baseline splits uninstall interruptions into four partitions (step % 4).
    for partition in 0..4u64 {
        assert!(
            selected
                .iter()
                .any(|r| get(r, "step").as_int().unwrap().as_u64().unwrap() % 4 == partition),
            "partition {partition} of uninstall-{phase} is empty"
        );
    }
    let mut executed = Vec::new();
    for row in &selected {
        let point = text(row, "point").to_owned();
        restore_parts(world, &saved);
        let actual = world.path("actual-trace.jsonl");
        let _ = std::fs::remove_file(&actual);
        let mut extra = fault(&point);
        extra.push((
            "AGENT_MEMORY_FAULT_TRACE",
            actual.to_string_lossy().into_owned(),
        ));
        let exited = child(world, "uninstall", &extra);
        assert_eq!(exited.code, Some(86), "{point}");
        assert_eq!(text(trace_rows(&actual).last().unwrap(), "point"), point);
        let (code, recovered) = installed.recover();
        verified(code, &recovered);
        let (code, retried) = installed.uninstall(&uninstall_request(false));
        verified(code, &retried);
        let current = installed.receipt();
        assert_eq!(uninstall_semantics(&current), expected, "{point}");
        assert_eq!(owned(&current), expected_nodes, "{point}");
        assert_eq!(text(&current, "installation_id"), installation_id);
        assert_eq!(snapshot(&world.path("repo/memory-bank")), memory);
        let stable = std::fs::read(world.receipt_path()).unwrap();
        let (code, repeated) = installed.uninstall(&uninstall_request(false));
        verified(code, &repeated);
        assert_eq!(std::fs::read(world.receipt_path()).unwrap(), stable);
        executed.push(point);
    }
    let all: Vec<String> = selected
        .iter()
        .map(|r| text(r, "point").to_owned())
        .collect();
    assert_eq!(executed, all);
}

// obligation: tests/contracts/test_uninstall_write_copy_before.py::UninstallCopyBeforePart0Contract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_uninstall_write_copy_before_part_1.py::UninstallCopyBeforePart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_uninstall_write_copy_before_part_2.py::UninstallCopyBeforePart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_uninstall_write_copy_before_part_3.py::UninstallCopyBeforePart3Contract.test_actual_logical_writes_in_partition_3
#[test]
fn every_uninstall_write_before_phase_interrupts_recovers_and_retries_in_copy_mode() {
    replay_uninstall(Mode::Copy, "before");
}

// obligation: tests/contracts/test_uninstall_write_copy_after.py::UninstallCopyAfterPart0Contract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_uninstall_write_copy_after_part_1.py::UninstallCopyAfterPart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_uninstall_write_copy_after_part_2.py::UninstallCopyAfterPart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_uninstall_write_copy_after_part_3.py::UninstallCopyAfterPart3Contract.test_actual_logical_writes_in_partition_3
#[test]
fn every_uninstall_write_after_phase_interrupts_recovers_and_retries_in_copy_mode() {
    replay_uninstall(Mode::Copy, "after");
}

// obligation: tests/contracts/test_uninstall_write_link_before.py::UninstallLinkBeforePart0Contract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_uninstall_write_link_before_part_1.py::UninstallLinkBeforePart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_uninstall_write_link_before_part_2.py::UninstallLinkBeforePart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_uninstall_write_link_before_part_3.py::UninstallLinkBeforePart3Contract.test_actual_logical_writes_in_partition_3
#[test]
fn every_uninstall_write_before_phase_interrupts_recovers_and_retries_in_link_mode() {
    replay_uninstall(Mode::Link, "before");
}

// obligation: tests/contracts/test_uninstall_write_link_after.py::UninstallLinkAfterPart0Contract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_uninstall_write_link_after_part_1.py::UninstallLinkAfterPart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_uninstall_write_link_after_part_2.py::UninstallLinkAfterPart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_uninstall_write_link_after_part_3.py::UninstallLinkAfterPart3Contract.test_actual_logical_writes_in_partition_3
#[test]
fn every_uninstall_write_after_phase_interrupts_recovers_and_retries_in_link_mode() {
    replay_uninstall(Mode::Link, "after");
}

// ----- uninstall of an adopted Python-era legacy source tree -----

fn fixture_root(scenario: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/v014-state")
        .join(scenario)
}
fn relocate(world: &World, text: &str) -> String {
    text.replace("@WORLD@", world.root.to_str().unwrap())
}

/// Materialize a derived v0.0.14 capture (as `t2_installation` does): state
/// files with their recorded modes, owned links, opaque owned trees, then
/// recompute every recorded target identity and the committed journal.
fn materialize(world: &World, scenario: &str) {
    let base = fixture_root(scenario);
    for line in std::fs::read_to_string(base.join("modes.txt"))
        .unwrap()
        .lines()
    {
        let (bits, relative) = line.split_once(' ').unwrap();
        let raw = std::fs::read_to_string(base.join(relative)).unwrap();
        let path = world.path(relative);
        write(&path, relocate(world, &raw));
        mode(&path, u32::from_str_radix(bits, 8).unwrap());
    }
    mode(&world.state(), 0o700);
    for line in std::fs::read_to_string(base.join("trees.txt"))
        .unwrap()
        .lines()
    {
        let (kind, relative) = line.split_once(' ').unwrap();
        let path = world.path(relative);
        std::fs::create_dir_all(&path).unwrap();
        write(&path.join("opaque"), format!("opaque {kind} {relative}\n"));
        mode(&path.join("opaque"), 0o644);
        mode(&path, 0o700);
    }
    for line in std::fs::read_to_string(base.join("links.txt"))
        .unwrap()
        .lines()
    {
        let (relative, target) = line.split_once(" -> ").unwrap();
        let path = world.path(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(relocate(world, target), &path).unwrap();
    }
    rewrite_receipt(world, |receipt| {
        if let Some(Value::Array(items)) = receipt.get_mut("targets") {
            for target in items.iter_mut() {
                let identity = install::fingerprint(Path::new(text(target, "path"))).unwrap();
                set_field(target, "identity", identity);
            }
        }
    });
}

/// An adopted Python-era installation (managed legacy tree kept inert).
fn adopted() -> (World, PathBuf) {
    let world = World::new();
    materialize(&world, "all-copy");
    let source = development_source(&world, "dev");
    let services = services_with(&world, ReleaseAccess::default());
    let result = install::install_command(
        &world.env(),
        &services,
        &InstallRequest {
            development_source: Some(source),
            ..InstallRequest::default()
        },
    );
    assert_eq!(result.exit_code, 0, "{}", result.stdout);
    let legacy = world.path("data/agent-memory/source");
    let receipt = decode(&world.receipt_path());
    assert!(
        targets(&receipt)
            .iter()
            .any(|t| text(t, "kind") == "legacy-source"
                && text(t, "path") == legacy.to_str().unwrap())
    );
    (world, legacy)
}

// obligation: tests/contracts/test_legacy_uninstall_metadata.py::LegacyUninstallMetadataContract.test_metadata_drift_removes_verified_owned_source
#[test]
fn uninstall_removes_the_recorded_legacy_source_tree() {
    let (world, legacy) = adopted();
    let services = services_with(&world, ReleaseAccess::default());
    let preview =
        uninstallation::uninstall_command(&world.env(), &services, &uninstall_request(true));
    let preview = preview.value.unwrap();
    assert_eq!(
        get(&preview, "preserved"),
        &Value::Array(vec![]),
        "{preview:?}"
    );
    let removed =
        uninstallation::uninstall_command(&world.env(), &services, &uninstall_request(false));
    let value = removed.value.unwrap();
    verified(removed.exit_code, &value);
    assert!(std::fs::symlink_metadata(&legacy).is_err());
}

// obligation: tests/contracts/test_legacy_uninstall_metadata.py::LegacyUninstallMetadataContract.test_changed_payload_is_preserved
// obligation: tests/contracts/test_legacy_uninstall_metadata.py::LegacyUninstallMetadataContract.test_untracked_payload_is_preserved
#[test]
fn changed_or_added_legacy_payload_is_preserved() {
    for case in ["changed", "untracked"] {
        let (world, legacy) = adopted();
        let file = if case == "changed" {
            legacy.join("opaque")
        } else {
            legacy.join("user-file.txt")
        };
        write(&file, "do not remove\n");
        let services = services_with(&world, ReleaseAccess::default());
        let removed =
            uninstallation::uninstall_command(&world.env(), &services, &uninstall_request(false));
        let value = removed.value.unwrap();
        verified(removed.exit_code, &value);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "do not remove\n");
        assert!(
            get(&value, "preserved")
                .as_array()
                .unwrap()
                .iter()
                .any(|p| text(p, "kind") == "legacy-source")
        );
    }
}

// ======================================================================
// Checkpoint 2: am purge-data
// ======================================================================

fn purge(
    world: &World,
    repo: &Path,
    path: &Path,
    check: bool,
    confirm: Option<&str>,
) -> (i32, Value) {
    let request = agent_memory::purging::PurgeRequest {
        repo: repo.to_path_buf(),
        path: path.to_path_buf(),
        check,
        confirm: confirm.map(str::to_owned),
    };
    let result = agent_memory::purging::purge_command(
        &world.env(),
        &services_with(world, ReleaseAccess::default()),
        &request,
    );
    assert!(result.stderr.is_empty(), "{}", result.stderr);
    (result.exit_code, result.value.unwrap())
}
fn small_bank(world: &World) -> PathBuf {
    let bank = world.path("repo/memory-bank");
    write(
        &bank.join("MEMORY.md"),
        "---\nschema: agent-memory/v1\nproject-id: 8e0b66aa-2be5-4fa3-b47f-1245474aa7dd\n---\nBank.\n",
    );
    bank
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_broad_home_root_and_symlink_inputs_are_refused_without_writes
#[test]
fn broad_home_root_and_symlink_inputs_are_refused_without_writes() {
    // The `broad` and `inside-link` cases need the bank locator (security
    // scan) and the inventory (recovery frontier): see the ignored test below.
    for case in ["home", "root", "alias"] {
        let world = World::new();
        let bank = small_bank(&world);
        let (repo, path) = match case {
            "broad" => (world.path("repo"), world.path("repo")),
            "home" => (world.path("home"), world.path("home")),
            "root" => (PathBuf::from("/"), PathBuf::from("/")),
            _ => {
                let alias = world.path("bank alias");
                symlink(&bank, &alias).unwrap();
                (world.path("repo"), alias)
            }
        };
        let before = snapshot(&world.root);
        let (code, denied) = purge(&world, &repo, &path, true, None);
        let error = refused(code, &denied);
        let expected = match case {
            "broad" => {
                "Purge path must be exactly this repository's resolved Memory Bank, not a broad directory"
            }
            "alias" => "Purge paths must not cross symlinks",
            _ => "A bounded project repository root is required",
        };
        assert_eq!(error, expected, "{case}");
        assert_eq!(text(&denied, "operation"), "purge-data");
        assert_eq!(text(&denied, "repair"), agent_memory::purging::REPAIR);
        assert_eq!(snapshot(&world.root), before, "{case}");
    }
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_exact_preview_confirmation_and_repeat_preserve_runtime_backups_code_and_git
#[test]
#[ignore = "needs security, recovery_frontier registered"]
fn broad_path_preview_confirm_and_repeat_purge_only_the_confirmed_bank() {
    let world = World::new();
    let bank = small_bank(&world);
    let repo = world.path("repo");
    let before = snapshot(&world.root);
    let (code, denied) = purge(&world, &repo, &repo, true, None);
    assert_eq!(
        refused(code, &denied),
        "Purge path must be exactly this repository's resolved Memory Bank, not a broad directory"
    );
    assert_eq!(snapshot(&world.root), before);
    let (code, preview) = purge(&world, &repo, &bank, true, None);
    assert_eq!(code, 0, "{preview:?}");
    assert_eq!(snapshot(&world.root), before);
    let token = text(get(&preview, "preview"), "confirmation").to_owned();
    let (code, done) = purge(&world, &repo, &bank, false, Some(&token));
    verified(code, &done);
    assert!(!bank.exists());
    let (code, repeated) = purge(&world, &repo, &bank, false, Some(&token));
    verified(code, &repeated);
    assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
}

#[test]
fn purge_refuses_with_typed_missing_providers_before_writes() {
    let world = World::new();
    let bank = small_bank(&world);
    let repo = world.path("repo");
    let before = snapshot(&world.root);
    // The bank locator needs the repository security scan and the exact
    // preview the private recovery frontier: both are typed refusals until
    // `security` and `recovery_frontier` are registered.
    for (check, confirm) in [(true, None), (false, Some("0".repeat(64)))] {
        let (code, refusal) = purge(&world, &repo, &bank, check, confirm.as_deref());
        let error = refused(code, &refusal);
        assert!(error.ends_with("provider is not registered"), "{error}");
    }
    assert_eq!(snapshot(&world.root), before);
}

// ======================================================================
// Checkpoint 2: doctor --fix (diagnostics is a pending seam)
// ======================================================================

impl Installed {
    /// Services whose running executable is the installed runtime binary.
    fn running_services(&self) -> Services {
        let mut services = self.services();
        services.executable = Some(self.world.runtime().join("bin/am"));
        services
    }
    fn locations(&self) -> install::Locations {
        install::locations(&self.world.env()).unwrap()
    }
}

// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_missing_owned_runtime_targets_are_recreated
// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_noop_and_restore_missing_owned_hook
#[test]
fn reconcile_recreates_missing_owned_targets_from_the_receipt() {
    let installed = Installed::new(Mode::Copy);
    let services = installed.running_services();
    let locations = installed.locations();
    let old = agent_memory::doctor_fix::trusted_receipt(
        &installed.world.env(),
        &services,
        &locations,
        None,
    )
    .unwrap();
    // Nothing missing: reconciliation is a verified no-op.
    let noop = agent_memory::doctor_fix::reconcile_hooks(
        &installed.world.env(),
        &services,
        &locations,
        &old,
    )
    .unwrap();
    assert_eq!(get(&noop, "changed"), &Value::Bool(false));
    let hook = installed.world.path("codex/hooks.json");
    let link = installed.world.bin().join("am");
    std::fs::remove_file(&hook).unwrap();
    std::fs::remove_file(&link).unwrap();
    let repaired = agent_memory::doctor_fix::reconcile_hooks(
        &installed.world.env(),
        &services,
        &locations,
        &old,
    )
    .unwrap();
    assert_eq!(get(&repaired, "verified"), &Value::Bool(true));
    assert_eq!(get(&repaired, "changed"), &Value::Bool(true));
    assert!(hook.is_file());
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        installed.world.runtime().join("bin/am")
    );
    assert!(
        get(&repaired, "restart_clients")
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str() == Some("codex"))
    );
}

// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_modified_owned_hook_is_preserved_and_not_replaced
#[test]
fn reconcile_preserves_a_modified_owned_hook_and_refuses() {
    let installed = Installed::new(Mode::Copy);
    let services = installed.running_services();
    let locations = installed.locations();
    let hook = installed.world.path("codex/hooks.json");
    let mut value = decode(&hook);
    if let Some(Value::Array(groups)) = value
        .get_mut("hooks")
        .and_then(|hooks| hooks.get_mut("UserPromptSubmit"))
    {
        groups.clear();
    }
    std::fs::write(&hook, json::encode(&value, json::COMPACT_ASCII).unwrap()).unwrap();
    let before = std::fs::read(&hook).unwrap();
    let old = installed.receipt();
    let outcome = agent_memory::doctor_fix::reconcile_hooks(
        &installed.world.env(),
        &services,
        &locations,
        &old,
    );
    assert!(outcome.is_err());
    assert_eq!(std::fs::read(&hook).unwrap(), before);
}

#[test]
fn trusted_receipt_refuses_another_executing_package_and_python_era_receipts() {
    let installed = Installed::new(Mode::Copy);
    let locations = installed.locations();
    let mut services = installed.services();
    services.executable = Some(installed.channel.binary(BASE));
    let error = agent_memory::doctor_fix::trusted_receipt(
        &installed.world.env(),
        &services,
        &locations,
        None,
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Executing package differs from the installed receipt"
    );
    let mut legacy = installed.receipt();
    set_field(
        &mut legacy,
        "schema",
        Value::String("agent-memory/install-receipt-v1".into()),
    );
    let error = agent_memory::doctor_fix::trusted_receipt(
        &installed.world.env(),
        &installed.running_services(),
        &locations,
        Some(legacy),
    )
    .unwrap_err();
    assert_eq!(error.to_string(), install::ADOPTION_GUIDANCE);
}

#[test]
fn doctor_fix_refuses_while_diagnostics_is_unregistered() {
    let installed = Installed::new(Mode::Copy);
    let before = snapshot(&installed.world.root);
    let error = agent_memory::doctor_fix::run(
        &installed.world.env(),
        &installed.running_services(),
        &installed.world.path("repo"),
        None,
    )
    .unwrap_err();
    assert!(matches!(error, install::Error::Unregistered(_)), "{error}");
    assert_eq!(snapshot(&installed.world.root), before);
}

// obligation: tests/contracts/test_runtime_purge.py::RuntimePurgeContract.test_interruptions_before_journal_detach_partial_delete_and_lost_response_converge
// obligation: tests/contracts/test_lifecycle_lost_responses.py::LifecycleLostResponses.test_purge_response_loss_reuses_the_exact_original_confirmation
#[test]
#[ignore = "needs security, recovery_frontier registered"]
fn purge_interruptions_and_lost_response_converge_on_the_same_confirmation() {
    for point in [
        "purge-bank-lock-ready",
        "purge-after-detach",
        "purge-after-entry-0",
        "purge-after-root-remove",
        "purge-after-receipt",
        "purge-response-lost",
    ] {
        let world = World::new();
        let bank = small_bank(&world);
        write(&bank.join("projectbrief.md"), "# Project\n");
        let repo = world.path("repo");
        let (code, preview) = purge(&world, &repo, &bank, true, None);
        assert_eq!(code, 0, "{preview:?}");
        let token = text(get(&preview, "preview"), "confirmation").to_owned();
        let mut extra = fault(point);
        extra.push(("AM_T2_CONFIRM", token.clone()));
        assert_eq!(child(&world, "purge", &extra).code, Some(86), "{point}");
        let (code, completed) = purge(&world, &repo, &bank, false, Some(&token));
        verified(code, &completed);
        let receipt = get(&completed, "receipt");
        assert!(install::same(
            get(receipt, "preview"),
            get(&preview, "preview")
        ));
        assert_eq!(
            get(receipt, "deletion_readback_verified"),
            &Value::Bool(true)
        );
        assert!(!bank.exists());
        let (code, repeated) = purge(&world, &repo, &bank, false, Some(&token));
        verified(code, &repeated);
        assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
    }
}

// obligation: tests/contracts/test_doctor_fix.py::DoctorFixContract.test_healthy_unrelated_hook_configuration_needs_no_repair
#[test]
#[ignore = "needs diagnostics registered"]
fn healthy_installation_needs_no_doctor_repair() {
    let installed = Installed::new(Mode::Copy);
    let before = snapshot(&installed.world.root);
    let value = agent_memory::doctor_fix::run(
        &installed.world.env(),
        &installed.running_services(),
        &installed.world.path("repo"),
        None,
    )
    .unwrap();
    assert_eq!(get(&value, "repairs_performed"), &Value::Bool(false));
    assert_eq!(get(&value, "writes_performed"), &Value::Bool(false));
    assert_eq!(get(&value, "restart_clients"), &Value::Array(vec![]));
    assert_eq!(snapshot(&installed.world.root), before);
}

/// Records each plan the lifecycle hands to the native boundary seam.
#[derive(Default)]
struct RecordingOmp(std::sync::Mutex<Vec<(Value, bool)>>);
fn planned_transition() -> Value {
    json::decode(
        r#"{"binding_disable_planned": true, "drains": [], "installation_writes_performed": false, "canonical_writes_performed": false}"#,
        json::DecodePolicy::default(),
    )
    .unwrap()
}
impl install::OmpBoundary for RecordingOmp {
    fn capability(&self) -> &str {
        agent_memory::omp_boundary::CAPABILITY
    }
    fn native_transition(&self, plan: &mut Value, check: bool) -> install::Result<Value> {
        self.0.lock().unwrap().push((plan.clone(), check));
        Ok(planned_transition())
    }
    fn native_activation(&self, _: &mut Value, _: bool) -> install::Result<Value> {
        Ok(Value::Array(vec![]))
    }
    fn finish_native_transition(&self, _: &Value) -> install::Result<()> {
        Ok(())
    }
    fn recover_native_binding(&self, _: &Path) -> install::Result<()> {
        Ok(())
    }
    fn native_recovery_expected(&self, _: &Path, _: &str) -> install::Result<Option<Value>> {
        Ok(None)
    }
    fn complete_stored_source_activation(&self, _: &Path) -> install::Result<Value> {
        Ok(Value::Object(vec![]))
    }
}

/// `--omp-writers-quiesced` is the plan's own `omp_transition`; update check
/// records the native transition's result, uninstall check passes the plan.
#[test]
fn writers_quiesced_checks_hand_the_plan_transition_to_the_native_boundary() {
    let installed = Installed::new(Mode::Copy);
    let omp = Arc::new(RecordingOmp::default());
    let mut services = installed.services();
    services.omp_boundary = Some(omp.clone());
    let before = snapshot(&installed.world.root);
    let request = UpdateRequest {
        check: true,
        omp_writers_quiesced: true,
        ..UpdateRequest::default()
    };
    let result = updating::update_command(&installed.world.env(), &services, &request);
    let value = result.value.unwrap();
    assert_eq!(result.exit_code, 0, "{value:?}");
    assert_eq!(get(&value, "native_transition"), &planned_transition());
    let quiesced = json::decode(
        r#"{"writers_quiesced": true}"#,
        json::DecodePolicy::default(),
    )
    .unwrap();
    {
        let calls = omp.0.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let (plan, check) = &calls[0];
        assert!(*check);
        assert_eq!(get(plan, "omp_transition"), &quiesced);
        assert_eq!(text(plan, "operation"), "update");
        assert!(plan.get("preserved").is_none() && plan.get("reinstall").is_none());
    }
    let request = UninstallRequest {
        check: true,
        omp_writers_quiesced: true,
        ..UninstallRequest::default()
    };
    let result = uninstallation::uninstall_command(&installed.world.env(), &services, &request);
    let value = result.value.unwrap();
    assert_eq!(result.exit_code, 0, "{value:?}");
    let calls = omp.0.lock().unwrap();
    assert_eq!(calls.len(), 2);
    let (plan, check) = &calls[1];
    assert!(*check);
    assert_eq!(get(plan, "omp_transition"), &quiesced);
    assert_eq!(text(plan, "operation"), "uninstall");
    assert!(plan.get("preserved").is_some());
    assert_eq!(snapshot(&installed.world.root), before);
}
