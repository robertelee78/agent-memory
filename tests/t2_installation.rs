//! Installation lane: runtime faults, lifecycle lock and records, fresh
//! installs, journaled recovery and Python-era adoption in isolated Worlds.
//! Fault-injection scenarios run in a child copy of this test binary so the
//! process environment of parallel tests is never mutated.
use agent_memory::cache_recovery::Qualification;
use agent_memory::codec::json::{self, Value};
use agent_memory::installation::{
    self as install, ArtifactInspection, Environment, InstallRequest, Mode, PrivateState,
    ReleaseAccess, Services, Updating,
};
use agent_memory::release_distribution::{self as release, BinaryIdentity, ReleaseManifest};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

const STAND_IN: &[u8] = b"#!/bin/sh\nprintf 'stand-in am %s\\n' \"$*\"\n";

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
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            root.join(".test-world.json"),
            format!("{{\"token\": \"{token}\"}}"),
        )
        .unwrap();
        for name in [
            "home", "config", "data", "state", "codex", "claude", "opencode", "repo", "tmp",
        ] {
            std::fs::create_dir(root.join(name)).unwrap();
            std::fs::set_permissions(root.join(name), std::fs::Permissions::from_mode(0o700))
                .unwrap();
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
    fn bin(&self) -> PathBuf {
        self.path("home/.local/bin")
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

struct FixtureUpdating;
impl Updating for FixtureUpdating {
    fn banks(&self, repositories: &[String], _: &Value, _: bool) -> install::Result<Value> {
        Ok(Value::Object(vec![(
            "observed_repositories".into(),
            Value::Array(repositories.iter().cloned().map(Value::String).collect()),
        )]))
    }
    fn qualified_current(&self, _: &Path, _: &Path) -> install::Result<Option<Qualification>> {
        Ok(None)
    }
    fn current(&self, _: &Path, _: &Path, _: Option<&Qualification>) -> install::Result<()> {
        Ok(())
    }
    fn release_transition(&self, _: &Value, _: &Value, _: &Path) -> install::Result<()> {
        Ok(())
    }
    fn compatible_surface(
        &self,
        _: &Value,
        _: &Value,
        _: Option<&Qualification>,
    ) -> install::Result<()> {
        Ok(())
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
fn services() -> Services {
    Services {
        updating: Some(Arc::new(FixtureUpdating)),
        private_state: Some(Arc::new(FixturePrivateState)),
        omp_boundary: None,
        artifacts: Arc::new(FixtureArtifacts),
        release: ReleaseAccess::default(),
        executable: None,
    }
}

// ----- fixtures -----

const COMPATIBILITY: &str = r#"{"schema":"agent-memory/runtime-compatibility-v1","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":[],"python_minimum":"3.11","installed_commands":["am","agent-memory"],"installation_receipt_schemas":["agent-memory/install-receipt-v1","agent-memory/install-receipt-v2","agent-memory/install-receipt-v3"],"git_required_for":["git-sharing"],"required_python_packages":[],"required_agent_clients":[],"optional_adapters":[]}"#;

fn write(path: &Path, body: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
fn mode(path: &Path, bits: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap();
}

/// A minimal valid portable pack plus a stand-in `am`, as a development source.
fn development_source(world: &World, name: &str) -> PathBuf {
    let root = world.path(name);
    let pack = root.join("skills/agent-memory");
    write(
        &pack.join("SKILL.md"),
        "---\nname: agent-memory\ndescription: Bounded portable contract fixture\nmetadata: {\"version\":\"0.0.15\"}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n",
    );
    write(
        &pack.join("bank/GUIDE.md"),
        "---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n",
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
    write(&pack.join("inspect/GUIDE.md"), operation);
    write(
        &pack.join("schemas/runtime-compatibility.json"),
        COMPATIBILITY,
    );
    write(
        &pack.join("schemas/bank-router.json"),
        r#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#,
    );
    write(
        &pack.join("templates/activation-markdown.example.txt"),
        install::SHIM,
    );
    write(
        &pack.join("templates/activation-omp-markdown.example.txt"),
        include_str!("../skills/agent-memory/templates/activation-omp-markdown.example.txt"),
    );
    write(&pack.join("hooks/opencode.js"), "export default {}\n");
    write(
        &pack.join("hooks/omp.ts"),
        "export default function () {}\n",
    );
    let binary = root.join("target/release/am");
    write(&binary, STAND_IN);
    mode(&binary, 0o755);
    for entry in walk(&pack) {
        if entry.is_file() {
            mode(&entry, 0o644);
        } else if entry.is_dir() {
            mode(&entry, 0o755);
        }
    }
    root
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
/// Every node below `root`: link target, directory mode, or sha256 + mode.
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for path in walk(root).into_iter().skip(1) {
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
                agent_memory::release_distribution::digest(&std::fs::read(&path).unwrap()),
                meta.mode() & 0o7777
            )
        };
        result.insert(key, value);
    }
    result
}
fn without_lock(mut snapshot: BTreeMap<String, String>) -> BTreeMap<String, String> {
    snapshot.retain(|key, _| {
        let name = key.rsplit('/').next().unwrap_or(key);
        !(name == "lock" || name == "owner.json")
    });
    snapshot
}
fn request(source: &Path) -> InstallRequest {
    InstallRequest {
        development_source: Some(source.to_path_buf()),
        ..InstallRequest::default()
    }
}
fn run(world: &World, request: &InstallRequest) -> (i32, Value) {
    let result = install::install_command(&world.env(), &services(), request);
    assert!(result.stderr.is_empty(), "{}", result.stderr);
    (result.exit_code, result.value.unwrap())
}
fn ok(world: &World, request: &InstallRequest) -> Value {
    let (code, value) = run(world, request);
    assert_eq!(
        code,
        0,
        "{}",
        json::encode(&value, json::INDENT2_ASCII).unwrap()
    );
    value
}
fn recover(world: &World) -> (i32, Value) {
    let result = install::recover_command(&world.env(), &services());
    (result.exit_code, result.value.unwrap())
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
fn target<'a>(receipt: &'a Value, path: &Path) -> Value {
    targets(receipt)
        .into_iter()
        .find(|t| text(t, "path") == path.to_str().unwrap())
        .unwrap_or_else(|| panic!("no target {}", path.display()))
}
fn decode(path: &Path) -> Value {
    json::decode_bytes(&std::fs::read(path).unwrap(), Default::default()).unwrap()
}
fn error_text(value: &Value) -> String {
    text(value, "error").to_owned()
}

/// Publish a complete release-v2 fixture using a throwaway signing key. Only
/// file:// transport is enabled; no released key, network or executable is used.
fn signed_release(world: &World, name: &str, binary: &[u8]) -> PathBuf {
    let root = development_source(world, name);
    let pack = root.join(install::RELEASE_PACK);
    write(&pack.join("keys/allowed-signers"), release::ALLOWED_SIGNERS);
    mode(&pack.join("keys"), 0o755);
    mode(&pack.join("keys/allowed-signers"), 0o644);
    let files: release::Inventory = walk(&pack)
        .into_iter()
        .filter(|path| path.is_file())
        .map(|path| {
            (
                path.strip_prefix(&pack)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                (std::fs::read(&path).unwrap(), 0o644),
            )
        })
        .collect();
    let archive = release::encode_archive(
        &files
            .iter()
            .map(|(name, value)| (format!("{}{name}", release::PREFIX), value.clone()))
            .collect(),
    )
    .unwrap();
    let inventory = release::inventory_value(&files);
    let sha256 = release::digest(binary);
    let identity = BinaryIdentity {
        size: binary.len(),
        artifact_digest: agent_memory::artifact::runtime_digest(&inventory, &sha256, 0o755)
            .unwrap()
            .1,
        sha256,
    };
    let manifest = ReleaseManifest {
        version: env!("CARGO_PKG_VERSION").into(),
        tag: format!("v{}", env!("CARGO_PKG_VERSION")),
        source_commit: "a".repeat(40),
        archive_size: archive.len(),
        archive_sha256: release::digest(&archive),
        artifact_digest: release::inventory_digest(&inventory).unwrap(),
        targets: release::TARGETS
            .into_iter()
            .map(|target| (target.into(), identity.clone()))
            .collect(),
        command_surface: Value::Object(vec![(
            "commands".into(),
            Value::Array(vec![
                Value::String("install".into()),
                Value::String("update".into()),
            ]),
        )]),
    };
    let key = root.join("key");
    let command = |args: &[OsString], input: &[u8]| {
        let output = release::run_command(
            args,
            input,
            release::MAX_METADATA,
            None,
            std::time::Duration::from_secs(20),
        )
        .unwrap();
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
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
    write(
        &root.join("signers"),
        format!(
            "{} namespaces=\"{}\" {}\n",
            release::PRINCIPAL,
            release::NAMESPACE,
            fields,
        ),
    );
    let raw = manifest.encoded().unwrap();
    let signature = command(
        &[
            "ssh-keygen".into(),
            "-Y".into(),
            "sign".into(),
            "-n".into(),
            release::NAMESPACE.into(),
            "-f".into(),
            key.as_os_str().to_owned(),
        ],
        &raw,
    );
    let tag = root.join(format!("assets/v{}", env!("CARGO_PKG_VERSION")));
    write(&tag.join(release::MANIFEST), raw);
    write(&tag.join(release::SIGNATURE), signature);
    write(&tag.join(release::ARCHIVE), archive);
    write(&root.join("bootstrap"), binary);
    mode(&root.join("bootstrap"), 0o755);
    root
}

fn release_services(root: &Path) -> Services {
    let mut services = services();
    let url_root = root.to_str().unwrap().replace(' ', "%20");
    services.release = ReleaseAccess {
        first_manifest_url: format!(
            "file://{}/assets/v{}/{}",
            url_root,
            env!("CARGO_PKG_VERSION"),
            release::MANIFEST
        ),
        assets: format!("file://{}/assets", url_root),
        options: release::DownloadOptions {
            allow_file: true,
            json_response: false,
        },
        signers: std::fs::read_to_string(root.join("signers")).unwrap(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    services
}

fn release_request(root: &Path) -> InstallRequest {
    InstallRequest {
        bootstrap_binary: Some(root.join("bootstrap")),
        ..Default::default()
    }
}

fn release_ok(world: &World, root: &Path, request: &InstallRequest) -> Value {
    let result = install::install_command(&world.env(), &release_services(root), request);
    assert_eq!(result.exit_code, 0, "{}", result.stdout);
    result.value.unwrap()
}

fn release_recover(world: &World, root: &Path) -> Value {
    let result = install::recover_command(&world.env(), &release_services(root));
    assert_eq!(result.exit_code, 0, "{}", result.stdout);
    result.value.unwrap()
}

/// Compare actual target nodes, modes and bytes across relocated Worlds, not
/// merely the identities claimed by their receipts.
fn physical_targets(world: &World, receipt: &Value) -> Vec<Value> {
    targets(receipt)
        .iter()
        .map(|target| {
            let path = PathBuf::from(text(target, "path"));
            let nodes = if path.symlink_metadata().is_ok() {
                walk(&path)
            } else {
                vec![]
            };
            Value::Array(
                nodes
                    .iter()
                    .map(|node| {
                        let relative = node.strip_prefix(&world.root).unwrap().to_str().unwrap();
                        let meta = node.symlink_metadata().unwrap();
                        let identity = if meta.file_type().is_symlink() {
                            format!("link {}", std::fs::read_link(node).unwrap().display())
                        } else if meta.is_dir() {
                            format!("directory {:o}", meta.mode() & 0o7777)
                        } else {
                            let raw = std::fs::read(node).unwrap();
                            let normalized = String::from_utf8_lossy(&raw)
                                .replace(world.root.to_str().unwrap(), "<W>");
                            format!(
                                "file {:o} {}",
                                meta.mode() & 0o7777,
                                release::digest(normalized.as_bytes())
                            )
                        };
                        Value::Array(vec![
                            Value::String(relative.into()),
                            Value::String(identity.replace(world.root.to_str().unwrap(), "<W>")),
                        ])
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn bootstrap_manifest_and_following_assets_are_version_pinned() {
    let access = ReleaseAccess::default();
    assert_eq!(
        access.first_manifest_url,
        format!(
            "{}/v{}/{}",
            release::ASSETS,
            env!("CARGO_PKG_VERSION"),
            release::MANIFEST,
        )
    );
    assert!(!access.first_manifest_url.contains("/latest/"));
    let world = World::new();
    let root = signed_release(&world, "release", STAND_IN);
    let access = release_services(&root).release;
    let observed = release::probe_with(
        &access.first_manifest_url,
        &access.assets,
        access.options,
        &access.signers,
    )
    .unwrap();
    assert_eq!(
        observed.base,
        format!("{}/v{}/", access.assets, env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn signed_bootstrap_installs_bound_bytes_and_exchanges_the_runtime_root() {
    for mode in [Mode::Copy, Mode::Link] {
        let world = World::new();
        let first = signed_release(&world, "release one", STAND_IN);
        let request = InstallRequest {
            mode: Some(mode),
            presets: Some("codex".into()),
            ..release_request(&first)
        };
        let installed = release_ok(&world, &first, &request);
        let receipt = get(&installed, "receipt");
        let runtime = world.path("data/agent-memory/runtime");
        assert_eq!(std::fs::read(runtime.join("bin/am")).unwrap(), STAND_IN);
        assert_eq!(text(get(receipt, "source"), "kind"), "release");
        assert_eq!(text(get(receipt, "source"), "target"), install::TARGET);
        assert_eq!(
            text(get(get(receipt, "artifact_files"), "bin/am"), "sha256"),
            release::digest(STAND_IN)
        );
        let original = install::fingerprint(&runtime).unwrap();
        let replacement = [STAND_IN, b"# replacement\n"].concat();
        let second = signed_release(&world, "release two", &replacement);
        let replaced = release_ok(&world, &second, &release_request(&second));
        assert_eq!(get(&replaced, "changed"), &Value::Bool(true));
        assert_eq!(std::fs::read(runtime.join("bin/am")).unwrap(), replacement);
        let journal = decode(&world.state().join("journal.json"));
        let source = get(&journal, "actions")
            .as_array()
            .unwrap()
            .iter()
            .find(|action| text(action, "kind") == "source")
            .unwrap();
        assert!(install::same(get(source, "before"), &original));
        assert!(install::same(
            &install::fingerprint(Path::new(text(source, "stage"))).unwrap(),
            &original
        ));
        assert!(install::same(
            &install::fingerprint(Path::new(text(source, "backup"))).unwrap(),
            &original
        ));
    }
}

#[test]
fn bootstrap_manifest_binding_refuses_version_digest_and_size_before_writes() {
    for case in ["version", "sha256", "size"] {
        let world = World::new();
        let root = signed_release(&world, "release", STAND_IN);
        let mut services = release_services(&root);
        match case {
            "version" => services.release.version = "0.0.16".into(),
            "sha256" => {
                let mut raw = STAND_IN.to_vec();
                raw[0] = b'X';
                std::fs::write(root.join("bootstrap"), raw).unwrap();
            }
            "size" => write(&root.join("bootstrap"), [STAND_IN, b"extra"].concat()),
            _ => unreachable!(),
        }
        let before = snapshot(&world.root);
        let result = install::install_command(&world.env(), &services, &release_request(&root));
        assert_eq!(result.exit_code, 1, "{case}: {}", result.stdout);
        assert!(
            error_text(result.value.as_ref().unwrap()).contains("Bootstrap binary"),
            "{case}"
        );
        assert_eq!(snapshot(&world.root), before, "{case}");
    }
}

#[test]
fn staged_release_is_reverified_under_the_lifecycle_lock() {
    for case in ["binary", "manifest", "signature"] {
        let world = World::new();
        let root = signed_release(&world, "release", STAND_IN);
        let services = release_services(&root);
        let (_scratch, staged) =
            install::stage_release(&services, &root.join("bootstrap")).unwrap();
        let tampered = match case {
            "binary" => staged.join("bin/am"),
            "manifest" => staged.join(release::MANIFEST),
            _ => staged.join(release::SIGNATURE),
        };
        std::fs::write(tampered, b"tampered staged bytes\n").unwrap();
        let env = world.env();
        let locations = install::locations(&env).unwrap();
        let result = install::locked(&env, &locations.state, || {
            install::prepare(
                &env,
                &services,
                &release_request(&root),
                &locations,
                None,
                install::PrepareOptions {
                    bootstrap_root: Some(&staged),
                    ..Default::default()
                },
            )
        });
        assert!(result.is_err(), "{case}");
        assert!(!world.path("data/agent-memory/runtime").exists());
        assert!(!world.state().join("receipt.json").exists());
    }
}

#[test]
fn managed_channel_options_refuse_before_bootstrap_downloads() {
    for (remote, reference) in [
        (Some("https://example.invalid/repo"), None),
        (None, Some("main")),
        (Some("https://example.invalid/repo"), Some("main")),
    ] {
        for legacy in [false, true] {
            let world = World::new();
            if legacy {
                materialize(&world, "all-copy");
            }
            let binary = world.path("bootstrap");
            write(&binary, STAND_IN);
            let before = snapshot(&world.root);
            let mut services = services();
            services.release.first_manifest_url = "file:///nonexistent/manifest".into();
            let result = install::install_command(
                &world.env(),
                &services,
                &InstallRequest {
                    bootstrap_binary: Some(binary),
                    remote: remote.map(str::to_owned),
                    reference: reference.map(str::to_owned),
                    ..Default::default()
                },
            );
            assert_eq!(result.exit_code, 1);
            assert_eq!(
                error_text(result.value.as_ref().unwrap()),
                install::MANAGED_GUIDANCE
            );
            assert_eq!(snapshot(&world.root), before);
        }
    }
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
    let world = World::reopen(&root, &token);
    let release_root = std::env::var_os("AM_T2_RELEASE").map(PathBuf::from);
    let services = release_root
        .as_deref()
        .map(release_services)
        .unwrap_or_else(services);
    let result = if command == "recover" {
        install::recover_command(&world.env(), &services)
    } else {
        let mut request = if let Some(root) = &release_root {
            release_request(root)
        } else {
            request(&PathBuf::from(std::env::var("AM_T2_SOURCE").unwrap()))
        };
        if let Ok(value) = std::env::var("AM_T2_MODE") {
            request.mode = Some(Mode::parse(&value).unwrap());
        }
        if let Ok(value) = std::env::var("AM_T2_PRESETS") {
            request.presets = Some(value);
        }
        if let Ok(value) = std::env::var("AM_T2_BIN_DIR") {
            request.bin_dir = Some(PathBuf::from(value));
        }
        install::install_command(&world.env(), &services, &request)
    };
    let value = result.value.unwrap_or(Value::Null);
    println!(
        "AM-RESULT:{}",
        json::encode(&value, json::COMPACT_ASCII).unwrap()
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
        .env("PATH", "/usr/bin:/bin");
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
fn trace_rows(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| json::decode(line, Default::default()).unwrap())
        .collect()
}

// ======================================================================
// Checkpoint 1: runtime_faults, lifecycle lock/owner, receipt/journal codecs
// ======================================================================

#[test]
fn production_without_fault_environment_creates_no_trace_and_numbers_nothing() {
    let paths = vec![PathBuf::from("/nonexistent/outside/any/world")];
    let value = agent_memory::runtime_faults::operation("install", &paths, || {
        agent_memory::runtime_faults::mutate("file-stage", || {
            Ok::<_, agent_memory::error::ContractError>(7)
        })
    })
    .unwrap();
    assert_eq!(value, 7);
}

// obligation: tests/contracts/test_lifecycle_lost_responses.py::LifecycleLostResponses.test_fault_trace_refuses_another_world_target_before_bootstrap_writes
#[test]
fn fault_environment_requires_an_explicit_disposable_world_before_any_write() {
    let world = World::new();
    let source = development_source(&world, "dev source");
    let before = snapshot(&world.root);
    // A forged token: the marker does not match, so nothing may run.
    let forged = child(
        &world,
        "install",
        &[
            ("AM_T2_SOURCE", source.to_string_lossy().into_owned()),
            ("AM_T2_PRESETS", "none".into()),
            ("AGENT_MEMORY_FAULT_STAGE", "install-before-receipt".into()),
            ("AGENT_MEMORY_TEST_TOKEN", "forged".into()),
        ],
    );
    let value = forged.value.unwrap();
    assert_eq!(forged.code, Some(0));
    assert_eq!(get(&value, "verified"), &Value::Bool(false));
    assert_eq!(
        error_text(&value),
        "Lifecycle fault tracing requires an explicit disposable World"
    );
    assert_eq!(snapshot(&world.root), before);
    // A write set outside the World is refused before any write.
    let outside = std::fs::canonicalize("/tmp")
        .unwrap()
        .join(format!("am-outside-{}", world.token));
    let escaped = child(
        &world,
        "install",
        &[
            ("AM_T2_SOURCE", source.to_string_lossy().into_owned()),
            ("AM_T2_PRESETS", "none".into()),
            ("AM_T2_BIN_DIR", outside.to_string_lossy().into_owned()),
            ("AGENT_MEMORY_FAULT_STAGE", "install-before-receipt".into()),
        ],
    );
    assert_eq!(
        error_text(&escaped.value.unwrap()),
        "Lifecycle fault tests must not mutate paths outside their World"
    );
    assert!(!outside.exists());
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_lifecycle_lost_responses.py::LifecycleLostResponses.test_fault_trace_itself_cannot_escape_into_another_world
#[test]
fn trace_must_stay_inside_the_world() {
    let world = World::new();
    let other = World::new();
    let source = development_source(&world, "dev");
    let before = (snapshot(&world.root), snapshot(&other.root));
    let trace = other.path("trace.jsonl");
    let result = child(
        &world,
        "install",
        &[
            ("AM_T2_SOURCE", source.to_string_lossy().into_owned()),
            ("AM_T2_PRESETS", "none".into()),
            (
                "AGENT_MEMORY_FAULT_TRACE",
                trace.to_string_lossy().into_owned(),
            ),
        ],
    );
    assert_eq!(
        error_text(&result.value.unwrap()),
        "Lifecycle trace must remain inside its disposable World"
    );
    assert_eq!((snapshot(&world.root), snapshot(&other.root)), before);
}

#[test]
fn records_use_sorted_indent_two_ascii_with_one_newline() {
    let value = json::decode(
        r#"{"b": [1, {"z": "\u00e9", "a": null}], "a": true}"#,
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(install::encoded(&value).unwrap()).unwrap(),
        "{\n  \"a\": true,\n  \"b\": [\n    1,\n    {\n      \"a\": null,\n      \"z\": \"\\u00e9\"\n    }\n  ]\n}\n"
    );
}

#[test]
fn load_refuses_links_oversize_and_duplicate_keys() {
    let world = World::new();
    let dir = world.path("records");
    std::fs::create_dir(&dir).unwrap();
    mode(&dir, 0o700);
    write(&dir.join("dup.json"), "{\"a\": 1, \"a\": 2}");
    mode(&dir.join("dup.json"), 0o600);
    assert!(
        install::load(&dir.join("dup.json"))
            .unwrap_err()
            .to_string()
            .contains("Duplicate")
    );
    write(&dir.join("big.json"), vec![b' '; install::MAX_RECORD + 1]);
    mode(&dir.join("big.json"), 0o600);
    assert_eq!(
        install::load(&dir.join("big.json"))
            .unwrap_err()
            .to_string(),
        format!(
            "Missing, linked or oversized installation record: {}",
            dir.join("big.json").display()
        )
    );
    symlink(dir.join("dup.json"), dir.join("link.json")).unwrap();
    assert!(install::load(&dir.join("link.json")).is_err());
}

fn lock_bytes() -> &'static str {
    "{\n  \"schema\": \"agent-memory/lifecycle-lock-v1\"\n}\n"
}

#[test]
fn lock_seeds_private_state_and_publishes_owner_metadata() {
    let world = World::new();
    let state = world.state();
    let pid = install::locked(&world.env(), &state, || Ok(std::process::id())).unwrap();
    assert_eq!(std::fs::metadata(&state).unwrap().mode() & 0o7777, 0o700);
    assert_eq!(
        std::fs::read_to_string(state.join("lock")).unwrap(),
        lock_bytes()
    );
    assert_eq!(
        std::fs::metadata(state.join("lock")).unwrap().mode() & 0o7777,
        0o600
    );
    let owner = decode(&state.join("owner.json"));
    assert_eq!(text(&owner, "schema"), "agent-memory/lifecycle-owner-v1");
    assert_eq!(
        text(&owner, "lock_path"),
        state.join("lock").to_str().unwrap()
    );
    assert_eq!(
        get(&owner, "pid").as_int().unwrap().as_u64(),
        Some(u64::from(pid))
    );
    let keys: Vec<_> = owner
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(keys, ["lock_path", "pid", "schema", "started_at"]);
    assert!(
        std::fs::read_dir(&state).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.'))
    );
}

#[test]
fn a_held_flock_is_busy_and_never_stolen() {
    let world = World::new();
    let state = world.state();
    install::locked(&world.env(), &state, || Ok(())).unwrap();
    let holder = agent_memory::fs::Descriptor::open_directory(&state)
        .unwrap()
        .open_at(std::ffi::OsStr::new("lock"), libc::O_RDWR, 0)
        .unwrap();
    let _held =
        agent_memory::fs::FlockGuard::try_acquire(holder, agent_memory::fs::LockMode::Exclusive)
            .unwrap();
    let owner = std::fs::read(state.join("owner.json")).unwrap();
    let error = install::locked(&world.env(), &state, || Ok(())).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Another lifecycle process holds the lock; retry after it finishes"
    );
    assert_eq!(std::fs::read(state.join("owner.json")).unwrap(), owner);
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_unrelated_lock_and_linked_state_are_never_adopted
#[test]
fn unrelated_lock_and_linked_state_are_never_adopted() {
    let world = World::new();
    let source = development_source(&world, "developer source with spaces");
    let state = world.state();
    std::fs::create_dir_all(&state).unwrap();
    mode(&state, 0o700);
    write(&state.join("lock"), "Unrelated file I own.\n");
    mode(&state.join("lock"), 0o600);
    let (code, value) = run(&world, &request(&source));
    assert_eq!(code, 1);
    assert_eq!(
        error_text(&value),
        "Unrelated lifecycle lock file; preserve it for reconciliation"
    );
    assert_eq!(
        std::fs::read_to_string(state.join("lock")).unwrap(),
        "Unrelated file I own.\n"
    );
    std::fs::remove_file(state.join("lock")).unwrap();
    std::fs::remove_dir(&state).unwrap();
    let bank = world.path("bank");
    std::fs::create_dir(&bank).unwrap();
    write(&bank.join("keep.md"), "Keep.\n");
    symlink(&bank, &state).unwrap();
    let before = snapshot(&bank);
    let (code, _) = run(&world, &request(&source));
    assert_eq!(code, 1);
    assert_eq!(snapshot(&bank), before);
}

fn receipt_fixture(world: &World, schema: &str, extra_target: Option<Value>) -> Value {
    let path = world
        .path("codex/skills/agent-memory")
        .to_string_lossy()
        .into_owned();
    let mut targets = vec![Value::Object(vec![
        ("path".into(), Value::String(path)),
        ("kind".into(), Value::String("copy".into())),
        ("identity".into(), Value::Null),
    ])];
    targets.extend(extra_target);
    let mut source = vec![("kind".to_owned(), Value::String("development".into()))];
    if schema.ends_with("v3") {
        source.push(("target".into(), Value::String(install::TARGET.into())));
    }
    Value::Object(vec![
        ("schema".into(), Value::String(schema.into())),
        ("verified".into(), Value::Bool(true)),
        ("installation_id".into(), Value::String("0".repeat(32))),
        ("presets".into(), Value::String("codex".into())),
        (
            "enabled_adapters".into(),
            Value::Array(vec![Value::String("codex".into())]),
        ),
        ("source".into(), Value::Object(source)),
        ("targets".into(), Value::Array(targets)),
    ])
}
fn put_receipt(world: &World, value: &Value) {
    let state = world.state();
    std::fs::create_dir_all(&state).unwrap();
    mode(&state, 0o700);
    let path = state.join("receipt.json");
    let _ = std::fs::remove_file(&path);
    write(&path, install::encoded(value).unwrap());
    mode(&path, 0o600);
}

#[test]
fn receipt_codec_reads_v1_v2_v3_and_refuses_unknown_or_malformed() {
    let world = World::new();
    let env = world.env();
    for schema in [
        "agent-memory/install-receipt-v1",
        "agent-memory/install-receipt-v3",
    ] {
        put_receipt(&world, &receipt_fixture(&world, schema, None));
        assert!(
            install::read_receipt(&env, &world.state())
                .unwrap()
                .is_some(),
            "{schema}"
        );
    }
    put_receipt(
        &world,
        &receipt_fixture(&world, "agent-memory/install-receipt-v9", None),
    );
    assert!(
        install::read_receipt(&env, &world.state())
            .unwrap_err()
            .to_string()
            .starts_with("Invalid installation ownership receipt")
    );
    // command-link targets exist only in Rust-era receipts.
    let link = Value::Object(vec![
        (
            "path".into(),
            Value::String(world.bin().join("am").to_string_lossy().into_owned()),
        ),
        ("kind".into(), Value::String("command-link".into())),
        ("identity".into(), Value::Null),
    ]);
    put_receipt(
        &world,
        &receipt_fixture(
            &world,
            "agent-memory/install-receipt-v1",
            Some(link.clone()),
        ),
    );
    assert_eq!(
        install::read_receipt(&env, &world.state())
            .unwrap_err()
            .to_string(),
        "Malformed target ownership receipt"
    );
    put_receipt(
        &world,
        &receipt_fixture(&world, "agent-memory/install-receipt-v3", Some(link)),
    );
    assert!(
        install::read_receipt(&env, &world.state())
            .unwrap()
            .is_some()
    );
    // A v3 receipt must name its compiled runtime target.
    let mut missing = receipt_fixture(&world, "agent-memory/install-receipt-v3", None);
    if let Some(Value::Object(source)) = missing.get_mut("source") {
        source.retain(|(k, _)| k != "target");
    }
    put_receipt(&world, &missing);
    assert_eq!(
        install::read_receipt(&env, &world.state())
            .unwrap_err()
            .to_string(),
        "Rust-era installation receipt lacks its runtime target"
    );
    let mut uninstalled = receipt_fixture(&world, "agent-memory/install-receipt-v3", None);
    if let Value::Object(fields) = &mut uninstalled {
        fields.push(("uninstalled".into(), Value::Bool(true)));
    }
    put_receipt(&world, &uninstalled);
    assert!(
        install::read_receipt(&env, &world.state())
            .unwrap()
            .is_none()
    );
}

#[test]
fn journal_codec_refuses_unknown_status_and_foreign_staging_paths() {
    let world = World::new();
    let state = world.state();
    std::fs::create_dir_all(&state).unwrap();
    mode(&state, 0o700);
    let target = world.path("codex/AGENTS.md");
    let journal = |stage: &Path, status: &str, kind: &str| {
        Value::Object(vec![
            (
                "schema".into(),
                Value::String(install::JOURNAL_SCHEMA.into()),
            ),
            ("status".into(), Value::String(status.into())),
            ("transaction".into(), Value::String("abc".into())),
            ("receipt".into(), Value::Object(Vec::new())),
            (
                "actions".into(),
                Value::Array(vec![Value::Object(vec![
                    (
                        "path".into(),
                        Value::String(target.to_string_lossy().into_owned()),
                    ),
                    (
                        "stage".into(),
                        Value::String(stage.to_string_lossy().into_owned()),
                    ),
                    ("kind".into(), Value::String(kind.into())),
                    ("before".into(), Value::Null),
                    ("after".into(), Value::Null),
                ])]),
            ),
        ])
    };
    let good = world.path("codex/.agent-memory-abc-0");
    for (stage, status, kind, ok) in [
        (good.clone(), "activating", "activation", true),
        (good.clone(), "unknown", "activation", false),
        (
            world.path("codex/elsewhere"),
            "staging",
            "activation",
            false,
        ),
        (good.clone(), "activating", "mystery-step", false),
    ] {
        let path = state.join("journal.json");
        let _ = std::fs::remove_file(&path);
        write(
            &path,
            install::encoded(&journal(&stage, status, kind)).unwrap(),
        );
        mode(&path, 0o600);
        assert_eq!(
            install::read_journal(&world.env(), &state).is_ok(),
            ok,
            "{status}"
        );
    }
}

// ======================================================================
// Checkpoint 2: fresh install, copy and link modes, journaled recovery
// ======================================================================

#[test]
fn executing_runtime_selects_the_owned_development_receipt_without_writes() {
    // The existing opaque executable fixture proves locator selection, not
    // qualification of a native CLI or of the production installation guards.
    let world = World::new();
    let source = development_source(&world, "executing development source");
    let binary = source.join("target/release/am");
    let env = world.env();
    let before = snapshot(&world.root);
    let refusal = install::executing_runtime(&env, &binary).unwrap_err();
    assert!(matches!(&refusal, install::Error::Contract(_)));
    assert!(refusal.to_string().contains("trusted bootstrap installer"));
    assert!(!refusal.to_string().contains("pass --pack"));
    assert_eq!(snapshot(&world.root), before);

    let installed = ok(&world, &request(&source));
    let receipt = get(&installed, "receipt");
    let expected = (
        PathBuf::from(text(get(receipt, "source"), "pack")),
        install::source_binary(get(receipt, "source")).unwrap(),
    );
    assert_eq!(expected.1, binary);
    let before = snapshot(&world.root);
    assert_eq!(
        install::executing_runtime(&env, &world.bin().join("am")).unwrap(),
        expected
    );
    assert_eq!(
        install::pack_root(&env, &world.bin().join("agent-memory")).unwrap(),
        expected.0
    );
    let locations = install::omp_locations(&env, &world.bin().join("am")).unwrap();
    assert_eq!(locations.state, world.state());
    assert_eq!(locations.pack, expected.0);
    assert_eq!(locations.binary, expected.1);
    assert_eq!(snapshot(&world.root), before);

    let unrelated = world.path("unowned-am");
    write(&unrelated, STAND_IN);
    mode(&unrelated, 0o755);
    let before = snapshot(&world.root);
    let refusal = install::executing_runtime(&env, &unrelated).unwrap_err();
    assert!(matches!(&refusal, install::Error::Contract(_)));
    assert!(refusal.to_string().contains("trusted bootstrap installer"));
    assert!(!refusal.to_string().contains("pass --pack"));
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_fresh_plan_link_install_repeat_and_installed_command_preserve_source_and_user_config
#[test]
fn fresh_plan_link_install_repeat_preserve_source_and_user_config() {
    let world = World::new();
    let source = development_source(&world, "developer source with spaces");
    let source_before = snapshot(&source);
    write(&world.path("codex/AGENTS.md"), "My existing instructions\n");
    let before = snapshot(&world.root);
    let plan = ok(
        &world,
        &InstallRequest {
            check: true,
            ..request(&source)
        },
    );
    assert_eq!(text(&plan, "schema"), "agent-memory/install-plan-v1");
    assert_eq!(
        get(&plan, "installation_writes_performed"),
        &Value::Bool(false)
    );
    assert_eq!(snapshot(&world.root), before);
    let result = ok(&world, &request(&source));
    let receipt = get(&result, "receipt").clone();
    assert_eq!(text(&receipt, "schema"), "agent-memory/install-receipt-v3");
    assert_eq!(text(get(&receipt, "source"), "revision"), "unversioned");
    assert_eq!(text(get(&receipt, "source"), "target"), install::TARGET);
    assert_eq!(
        get(&receipt, "enabled_adapters").as_array().unwrap().len(),
        4
    );
    let agents = std::fs::read_to_string(world.path("codex/AGENTS.md")).unwrap();
    assert!(agents.starts_with("My existing instructions\n"));
    assert_eq!(agents.matches(install::BEGIN).count(), 1);
    assert_eq!(snapshot(&source), source_before);
    let pack = source.join("skills/agent-memory");
    let binary = source.join("target/release/am");
    let inspected = FixtureArtifacts.inspect_runtime(&pack, &binary).unwrap();
    assert_eq!(
        get(get(&receipt, "source"), "artifact_digest"),
        get(&inspected, "artifact_digest")
    );
    assert_eq!(get(&receipt, "artifact_files"), get(&inspected, "files"));
    assert!(get(&receipt, "artifact_files").get("bin/am").is_some());
    for t in targets(&receipt) {
        if text(&t, "kind") == "link" {
            assert_eq!(std::fs::read_link(text(&t, "path")).unwrap(), pack);
            assert_eq!(
                get(&t, "artifact_digest"),
                get(&inspected, "artifact_digest")
            );
        }
    }
    for name in ["am", "agent-memory"] {
        let link = world.bin().join(name);
        assert_eq!(std::fs::read_link(&link).unwrap(), binary);
        assert_eq!(text(&target(&receipt, &link), "kind"), "command-link");
        let output = Command::new(&link).arg("version").output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "stand-in am version\n"
        );
    }
    let hooks = decode(&world.path("codex/hooks.json"));
    let command = |event: &str| {
        text(
            &get(get(&hooks, "hooks"), event).as_array().unwrap()[0]
                .get("hooks")
                .unwrap()
                .as_array()
                .unwrap()[0],
            "command",
        )
        .to_owned()
    };
    assert_eq!(
        command("UserPromptSubmit"),
        format!("'{}' hook codex-prompt", binary.display())
    );
    assert_eq!(
        command("Stop"),
        format!("'{}' hook codex-stop", binary.display())
    );
    let receipt_path = world.state().join("receipt.json");
    assert_eq!(
        std::fs::read(&receipt_path).unwrap(),
        install::encoded(&receipt).unwrap()
    );
    assert_eq!(
        std::fs::metadata(&receipt_path).unwrap().mode() & 0o7777,
        0o600
    );
    let records = without_lock(snapshot(&world.state()));
    let repeat = ok(&world, &InstallRequest::default());
    assert_eq!(get(&repeat, "changed"), &Value::Bool(false));
    assert_eq!(get(&repeat, "restart_clients"), &Value::Array(vec![]));
    assert_eq!(without_lock(snapshot(&world.state())), records);
    let command_plan = ok(
        &world,
        &InstallRequest {
            check: true,
            ..InstallRequest::default()
        },
    );
    assert!(
        get(&command_plan, "targets")
            .as_array()
            .unwrap()
            .iter()
            .all(|t| get(t, "changed") == &Value::Bool(false))
    );
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_copy_replacement_backup_and_conversion_have_identical_artifact_identity
#[test]
fn copy_replacement_backup_and_conversion_have_identical_artifact_identity() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let custom = world.path("custom clients with spaces");
    let first = ok(
        &world,
        &InstallRequest {
            mode: Some(Mode::Copy),
            presets: Some("none".into()),
            target_dir: Some(vec![custom.clone()]),
            ..request(&source)
        },
    );
    let receipt = get(&first, "receipt").clone();
    let client = custom.join("agent-memory");
    assert!(
        !std::fs::symlink_metadata(&client)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::metadata(&client).unwrap().mode() & 0o7777, 0o700);
    let old_files = snapshot(&client);
    write(
        &source.join("skills/agent-memory/templates/new-evidence.txt"),
        "A valid additional artifact asset.\n",
    );
    mode(
        &source.join("skills/agent-memory/templates/new-evidence.txt"),
        0o644,
    );
    let second = ok(&world, &InstallRequest::default());
    let second_receipt = get(&second, "receipt");
    assert_ne!(
        get(get(second_receipt, "source"), "artifact_digest"),
        get(get(&receipt, "source"), "artifact_digest")
    );
    let backup = get(second_receipt, "backups")
        .as_array()
        .unwrap()
        .iter()
        .find(|b| text(b, "original_path") == client.to_str().unwrap())
        .unwrap()
        .clone();
    assert_eq!(snapshot(Path::new(text(&backup, "path"))), old_files);
    let as_link = ok(
        &world,
        &InstallRequest {
            mode: Some(Mode::Link),
            ..InstallRequest::default()
        },
    );
    assert!(
        std::fs::symlink_metadata(&client)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(get(&as_link, "restart_clients"), &Value::Array(vec![]));
    assert_eq!(
        get(get(get(&as_link, "receipt"), "source"), "artifact_digest"),
        get(get(second_receipt, "source"), "artifact_digest")
    );
    assert!(Path::new(text(&backup, "path")).is_dir());
    assert_eq!(
        get(&ok(&world, &InstallRequest::default()), "restart_clients"),
        &Value::Array(vec![])
    );
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_all_targets_preflight_before_activation_and_unrelated_types_are_preserved
#[test]
fn all_targets_preflight_before_activation_and_unrelated_types_are_preserved() {
    for kind in ["file", "directory", "symlink"] {
        let world = World::new();
        let source = development_source(&world, "dev");
        let collision = world.path("claude/skills/agent-memory");
        std::fs::create_dir_all(collision.parent().unwrap()).unwrap();
        match kind {
            "file" => write(&collision, "keep this"),
            "directory" => write(&collision.join("mine"), "keep this"),
            _ => symlink(world.path("repo"), &collision).unwrap(),
        }
        let before = snapshot(&world.root);
        let (code, value) = run(&world, &request(&source));
        assert_eq!(code, 1, "{kind}");
        assert!(
            error_text(&value).contains("preserve"),
            "{kind}: {}",
            error_text(&value)
        );
        let after = snapshot(&world.root);
        let outside_state = |s: &BTreeMap<String, String>| -> BTreeMap<String, String> {
            s.iter()
                .filter(|(k, _)| !k.starts_with("state"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        assert_eq!(outside_state(&after), outside_state(&before), "{kind}");
        assert!(!world.state().join("receipt.json").exists());
    }
}

// obligation: tests/contracts/test_installer.py::MacOSCommandContract.test_unrelated_am_file_directory_or_link_refuses_without_activating_other_targets
#[test]
fn unrelated_am_file_directory_or_link_refuses_without_activating_other_targets() {
    for kind in ["file", "directory", "link"] {
        let world = World::new();
        let source = development_source(&world, "dev");
        let target = world.bin().join("am");
        std::fs::create_dir_all(world.bin()).unwrap();
        match kind {
            "file" => write(&target, "Unrelated am.\n"),
            "directory" => write(&target.join("mine"), "Keep.\n"),
            _ => symlink(world.path("repo"), &target).unwrap(),
        }
        let before = snapshot(&world.bin());
        let (code, value) = run(
            &world,
            &InstallRequest {
                presets: Some("none".into()),
                ..request(&source)
            },
        );
        assert_eq!(code, 1, "{kind}");
        assert_eq!(snapshot(&world.bin()), before);
        assert!(!world.state().join("receipt.json").exists());
        assert!(!text(&value, "repair").contains("with recover before"));
    }
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_activation_outside_edits_survive_and_inside_edits_refuse
#[test]
fn activation_outside_edits_survive_and_inside_edits_refuse() {
    let world = World::new();
    let source = development_source(&world, "dev");
    ok(
        &world,
        &InstallRequest {
            presets: Some("codex".into()),
            ..request(&source)
        },
    );
    let config = world.path("codex/AGENTS.md");
    let body = std::fs::read_to_string(&config).unwrap();
    std::fs::write(
        &config,
        format!("Before user rules\n{body}After user rules\n"),
    )
    .unwrap();
    let outside = std::fs::read(&config).unwrap();
    let refreshed = ok(&world, &InstallRequest::default());
    assert_eq!(get(&refreshed, "restart_clients"), &Value::Array(vec![]));
    assert_eq!(std::fs::read(&config).unwrap(), outside);
    let changed = String::from_utf8(outside)
        .unwrap()
        .replace("load the agent-memory skill", "my custom changed block");
    std::fs::write(&config, &changed).unwrap();
    let before = snapshot(&world.root);
    let (code, denied) = run(&world, &InstallRequest::default());
    assert_eq!(code, 1);
    assert!(error_text(&denied).contains("modified"));
    assert_eq!(without_lock(snapshot(&world.root)), without_lock(before));
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_owned_client_hook_lifecycle_preserves_foreign_configuration
#[test]
fn owned_client_hook_lifecycle_preserves_foreign_configuration() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let foreign = r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"foreign-hook"}]}]},"other":{"kept":true}}"#;
    for client in ["codex/hooks.json", "claude/settings.json"] {
        write(&world.path(client), format!("{foreign}\n"));
    }
    ok(
        &world,
        &InstallRequest {
            presets: Some("codex,claude".into()),
            ..request(&source)
        },
    );
    for client in ["codex/hooks.json", "claude/settings.json"] {
        let value = decode(&world.path(client));
        let groups = get(get(&value, "hooks"), "UserPromptSubmit")
            .as_array()
            .unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            text(
                &groups[0].get("hooks").unwrap().as_array().unwrap()[0],
                "command"
            ),
            "foreign-hook"
        );
        let raw = std::fs::read_to_string(world.path(client)).unwrap();
        std::fs::write(
            world.path(client),
            raw.replacen('{', "{\"user-edit\": \"preserve later changes\", ", 1),
        )
        .unwrap();
    }
    ok(&world, &InstallRequest::default());
    for client in ["codex/hooks.json", "claude/settings.json"] {
        let value = decode(&world.path(client));
        assert_eq!(
            get(get(&value, "hooks"), "UserPromptSubmit")
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(text(&value, "user-edit"), "preserve later changes");
    }
    let claude = decode(&world.path("claude/settings.json"));
    let stop = &get(get(&claude, "hooks"), "Stop").as_array().unwrap()[0];
    assert_eq!(
        get(
            &stop.get("hooks").unwrap().as_array().unwrap()[0],
            "asyncRewake"
        ),
        &Value::Bool(true)
    );
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_explicit_development_link_refresh_still_revalidates_new_content
#[test]
fn explicit_development_link_refresh_still_revalidates_new_content() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let first = ok(
        &world,
        &InstallRequest {
            mode: Some(Mode::Link),
            presets: Some("codex".into()),
            ..request(&source)
        },
    );
    let added = source.join("skills/agent-memory/templates/new-local-source-asset.txt");
    write(&added, "Explicit new development artifact.\n");
    mode(&added, 0o644);
    let result = ok(&world, &request(&source));
    assert_ne!(
        get(get(get(&result, "receipt"), "source"), "artifact_digest"),
        get(get(get(&first, "receipt"), "source"), "artifact_digest")
    );
    assert_eq!(
        get(&result, "restart_clients"),
        &Value::Array(vec![Value::String("codex".into())])
    );
    assert_eq!(
        get(&ok(&world, &request(&source)), "restart_clients"),
        &Value::Array(vec![])
    );
}

#[test]
fn swapped_runtime_binary_changes_the_artifact_digest_and_restarts_clients() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let first = ok(
        &world,
        &InstallRequest {
            presets: Some("codex".into()),
            ..request(&source)
        },
    );
    std::fs::write(
        source.join("target/release/am"),
        b"#!/bin/sh\necho rebuilt\n",
    )
    .unwrap();
    let second = ok(&world, &request(&source));
    assert_ne!(
        get(get(get(&second, "receipt"), "source"), "artifact_digest"),
        get(get(get(&first, "receipt"), "source"), "artifact_digest")
    );
    assert_eq!(
        get(&second, "restart_clients"),
        &Value::Array(vec![Value::String("codex".into())])
    );
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_recover_with_no_pending_transaction_is_read_only
#[test]
fn recover_with_no_pending_transaction_is_read_only() {
    let world = World::new();
    let before = snapshot(&world.root);
    let (code, value) = recover(&world);
    assert_eq!(code, 0);
    assert_eq!(text(&value, "status"), "no-pending-transaction");
    assert_eq!(get(&value, "writes_performed"), &Value::Bool(false));
    assert_eq!(snapshot(&world.root), before);
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_state_inside_development_source_refuses_before_creating_a_lock
#[test]
fn state_inside_development_source_refuses_before_creating_a_lock() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let mut pairs = world.pairs();
    for (key, value) in &mut pairs {
        if key == "XDG_STATE_HOME" {
            *value = source.join("runtime state").to_string_lossy().into_owned();
        }
    }
    let env = Environment::from_pairs(
        pairs
            .into_iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    );
    let before = snapshot(&world.root);
    for check in [false, true] {
        let result = install::install_command(
            &env,
            &services(),
            &InstallRequest {
                check,
                presets: Some("none".into()),
                ..request(&source)
            },
        );
        let value = result.value.unwrap();
        assert_eq!(result.exit_code, 1);
        assert!(
            error_text(&value).contains("development source"),
            "{}",
            error_text(&value)
        );
        assert!(text(&value, "repair").contains("Do not run recovery"));
        assert_eq!(snapshot(&world.root), before);
    }
}

#[test]
fn unregistered_lazy_callees_refuse_without_writes() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let mut services = services();
    services.updating = None;
    let before = snapshot(&world.root);
    let result = install::install_command(
        &world.env(),
        &services,
        &InstallRequest {
            presets: Some("none".into()),
            ..request(&source)
        },
    );
    let value = result.value.unwrap();
    assert_eq!(result.exit_code, 1);
    assert!(
        error_text(&value).contains("Updating (bank compatibility) is not registered"),
        "{}",
        error_text(&value)
    );
    assert!(!world.state().join("receipt.json").exists());
    let after = without_lock(snapshot(&world.root));
    let before = without_lock(before);
    assert!(
        after
            .keys()
            .filter(|k| !k.starts_with("state"))
            .eq(before.keys().filter(|k| !k.starts_with("state")))
    );
}

#[test]
fn managed_remote_channels_and_release_without_bootstrap_refuse_with_guidance() {
    let world = World::new();
    let (code, value) = run(
        &world,
        &InstallRequest {
            remote: Some("https://example.invalid/x.git".into()),
            ..InstallRequest::default()
        },
    );
    assert_eq!(code, 1);
    assert_eq!(error_text(&value), install::MANAGED_GUIDANCE);
    let (code, value) = run(&world, &InstallRequest::default());
    assert_eq!(code, 1);
    assert_eq!(error_text(&value), install::RELEASE_GUIDANCE);
    let source = development_source(&world, "dev");
    std::fs::remove_file(source.join("target/release/am")).unwrap();
    let (_, value) = run(&world, &request(&source));
    assert!(error_text(&value).starts_with("Development source requires a prebuilt am at "));
    assert!(!world.state().join("receipt.json").exists());
}

// ----- interruptions through runtime_faults at the baseline journal steps -----

fn child_install(world: &World, source: &Path, extra: &[(&str, String)]) -> Child {
    let mut all = vec![("AM_T2_SOURCE", source.to_string_lossy().into_owned())];
    if source.join("signers").is_file() {
        all.push(("AM_T2_RELEASE", source.to_string_lossy().into_owned()));
    }
    all.extend(extra.iter().cloned());
    child(world, "install", &all)
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_partial_activation_failure_rolls_back_and_retry_converges
#[test]
fn partial_activation_failure_rolls_back_and_retry_converges() {
    let world = World::new();
    let source = development_source(&world, "dev");
    write(&world.path("codex/AGENTS.md"), "Keep existing config\n");
    let failed = child_install(
        &world,
        &source,
        &[
            ("AM_T2_PRESETS", "codex".into()),
            (
                "AGENT_MEMORY_FAULT_STAGE",
                "install-after-activate-1".into(),
            ),
        ],
    );
    let value = failed.value.unwrap();
    assert_eq!(get(&value, "verified"), &Value::Bool(false));
    assert!(error_text(&value).contains("Injected write failure"));
    assert_eq!(
        std::fs::read_to_string(world.path("codex/AGENTS.md")).unwrap(),
        "Keep existing config\n"
    );
    assert!(!world.path("codex/skills/agent-memory").exists());
    assert!(!world.state().join("receipt.json").exists());
    assert_eq!(
        text(&decode(&world.state().join("journal.json")), "status"),
        "rolled-back"
    );
    let retry = ok(
        &world,
        &InstallRequest {
            presets: Some("codex".into()),
            ..request(&source)
        },
    );
    assert_eq!(get(&retry, "verified"), &Value::Bool(true));
    assert_eq!(
        std::fs::read_to_string(world.path("codex/AGENTS.md"))
            .unwrap()
            .matches(install::BEGIN)
            .count(),
        1
    );
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_bootstrap_recovers_process_exit_before_wrapper_and_lost_commit_response
#[test]
fn process_exit_before_commit_rolls_back_and_lost_commit_response_completes() {
    for stage in [
        "install-after-activate-0",
        "install-after-receipt",
        "install-response-lost",
    ] {
        let world = World::new();
        let source = development_source(&world, "dev");
        let exited = child_install(
            &world,
            &source,
            &[
                ("AM_T2_PRESETS", "codex".into()),
                ("AGENT_MEMORY_FAULT_STAGE", stage.into()),
                ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
            ],
        );
        assert_eq!(exited.code, Some(86), "{stage}");
        let (code, value) = recover(&world);
        assert_eq!(code, 0, "{stage}: {value:?}");
        let expected = if stage == "install-after-activate-0" {
            "rolled-back"
        } else {
            "complete"
        };
        assert_eq!(text(&value, "status"), expected, "{stage}");
        let retry = ok(
            &world,
            &InstallRequest {
                presets: Some("codex".into()),
                ..request(&source)
            },
        );
        assert_eq!(get(&retry, "verified"), &Value::Bool(true));
        assert_eq!(
            std::fs::read_to_string(world.path("codex/AGENTS.md"))
                .unwrap()
                .matches(install::BEGIN)
                .count(),
            1
        );
        assert_eq!(
            get(&ok(&world, &request(&source)), "restart_clients"),
            &Value::Array(vec![])
        );
    }
}

// obligation: tests/contracts/test_installer.py::MacOSCommandContract.test_interrupted_activation_recovers_both_owned_command_names
#[test]
fn interrupted_activation_recovers_both_owned_command_names() {
    for stage in ["install-after-activate-0", "install-before-receipt"] {
        let world = World::new();
        let source = development_source(&world, "dev");
        let exited = child_install(
            &world,
            &source,
            &[
                ("AM_T2_PRESETS", "none".into()),
                ("AGENT_MEMORY_FAULT_STAGE", stage.into()),
                ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
            ],
        );
        assert_eq!(exited.code, Some(86));
        assert!(world.bin().join("am").symlink_metadata().is_ok());
        assert_eq!(
            world.bin().join("agent-memory").symlink_metadata().is_ok(),
            stage == "install-before-receipt"
        );
        let (code, value) = recover(&world);
        assert_eq!(code, 0);
        assert_eq!(text(&value, "status"), "rolled-back");
        assert!(world.bin().join("am").symlink_metadata().is_err());
        assert!(world.bin().join("agent-memory").symlink_metadata().is_err());
        ok(
            &world,
            &InstallRequest {
                presets: Some("none".into()),
                ..request(&source)
            },
        );
    }
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_failed_replacement_restores_existing_copies_and_the_exact_prior_receipt
#[test]
fn failed_replacement_restores_existing_copies_and_the_exact_prior_receipt() {
    let world = World::new();
    let source = development_source(&world, "dev");
    ok(
        &world,
        &InstallRequest {
            mode: Some(Mode::Copy),
            presets: Some("codex".into()),
            target_dir: Some(vec![world.path("another client")]),
            ..request(&source)
        },
    );
    let owned = snapshot(&world.path("codex"));
    let receipt_bytes = std::fs::read(world.state().join("receipt.json")).unwrap();
    let added = source.join("skills/agent-memory/templates/another-useful-asset.txt");
    write(&added, "Changed source content.\n");
    mode(&added, 0o644);
    let denied = child_install(
        &world,
        &source,
        &[(
            "AGENT_MEMORY_FAULT_STAGE",
            "install-after-activate-1".into(),
        )],
    );
    assert!(error_text(&denied.value.unwrap()).contains("Injected write failure"));
    let mut restored = snapshot(&world.path("codex"));
    restored.retain(|k, _| !k.contains(".agent-memory-"));
    assert_eq!(restored, owned);
    assert_eq!(
        std::fs::read(world.state().join("receipt.json")).unwrap(),
        receipt_bytes
    );
    ok(&world, &request(&source));
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_recovery_preserves_edits_made_after_an_interruption
#[test]
fn recovery_preserves_edits_made_after_an_interruption() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let exited = child_install(
        &world,
        &source,
        &[
            ("AM_T2_MODE", "copy".into()),
            ("AM_T2_PRESETS", "codex".into()),
            (
                "AGENT_MEMORY_FAULT_STAGE",
                "install-after-activate-0".into(),
            ),
            ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
        ],
    );
    assert_eq!(exited.code, Some(86));
    let edited = world.path("codex/skills/agent-memory/user-note.txt");
    write(&edited, "Preserve my post-interruption edit.\n");
    let (code, value) = recover(&world);
    assert_eq!(code, 1);
    assert_eq!(get(&value, "verified"), &Value::Bool(false));
    assert_eq!(
        std::fs::read_to_string(&edited).unwrap(),
        "Preserve my post-interruption edit.\n"
    );
    assert_eq!(
        text(&decode(&world.state().join("journal.json")), "status"),
        "rollback-conflict"
    );
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_completed_recovery_checks_the_actual_receipt_and_owned_targets
#[test]
fn completed_recovery_checks_the_actual_receipt_and_owned_targets() {
    for case in ["missing-receipt", "edited-copy", "missing-copy", "healthy"] {
        let world = World::new();
        let source = development_source(&world, "dev");
        ok(
            &world,
            &InstallRequest {
                mode: Some(Mode::Copy),
                presets: Some("codex".into()),
                ..request(&source)
            },
        );
        let client = world.path("codex/skills/agent-memory");
        match case {
            "missing-receipt" => std::fs::remove_file(world.state().join("receipt.json")).unwrap(),
            "edited-copy" => write(
                &client.join("user-note.txt"),
                "Preserve my edited installed copy.\n",
            ),
            "missing-copy" => std::fs::remove_dir_all(&client).unwrap(),
            _ => {}
        }
        let before = without_lock(snapshot(&world.root));
        let (code, value) = recover(&world);
        assert_eq!(
            code,
            if case == "healthy" { 0 } else { 1 },
            "{case}: {value:?}"
        );
        assert_eq!(get(&value, "verified"), &Value::Bool(case == "healthy"));
        assert_eq!(without_lock(snapshot(&world.root)), before, "{case}");
    }
}

/// Port of `lifecycle_fault_fixtures.LifecycleWriteCases.replay` for install:
/// every observed logical write, in copy and link modes and both phases, is
/// interrupted, recovered and retried to the clean semantics.
fn replay_install(install_mode: &str, phase: &str) {
    let world = World::new();
    let source = signed_release(&world, "fault channel with spaces", STAND_IN);
    write(
        &world.path("codex/AGENTS.md"),
        "Keep original developer instructions.\n",
    );
    let trace = world.path("clean-trace.jsonl");
    let clean = child_install(
        &world,
        &source,
        &[
            ("AM_T2_MODE", install_mode.into()),
            ("AM_T2_PRESETS", "codex".into()),
            (
                "AGENT_MEMORY_FAULT_TRACE",
                trace.to_string_lossy().into_owned(),
            ),
        ],
    );
    assert_eq!(
        get(clean.value.as_ref().unwrap(), "verified"),
        &Value::Bool(true)
    );
    let rows = trace_rows(&trace);
    assert!(!rows.is_empty() && rows.len() % 2 == 0);
    for (step, pair) in rows.chunks(2).enumerate() {
        assert_eq!(
            get(&pair[0], "step").as_int().unwrap().as_u64(),
            Some(step as u64)
        );
        assert_eq!(text(&pair[0], "phase"), "before");
        assert_eq!(text(&pair[1], "phase"), "after");
        assert_eq!(text(&pair[0], "kind"), text(&pair[1], "kind"));
    }
    let kinds: std::collections::BTreeSet<_> =
        rows.iter().map(|r| text(r, "kind").to_owned()).collect();
    for kind in [
        "file-stage",
        "record-publication",
        "atomic-node-switch",
        "state-parent-directory",
        "target-parent-directory",
    ] {
        assert!(kinds.contains(kind), "{kind}: {kinds:?}");
    }
    assert!(kinds.contains("payload-snapshot"));
    let expected = decode(&world.state().join("receipt.json"));
    let expected_files = physical_targets(&world, &expected);
    let semantics = |receipt: &Value| -> Value {
        let mut source = get(receipt, "source").clone();
        if let Value::Object(fields) = &mut source {
            fields.retain(|(k, _)| k != "compatibility");
        }
        Value::Array(vec![
            source,
            get(receipt, "mode").clone(),
            get(receipt, "presets").clone(),
            // Hook configuration bytes embed the World's binary path; compare entries instead.
            Value::Array(
                targets(receipt)
                    .iter()
                    .map(|t| {
                        let identity = if text(t, "kind") == "hook-config" {
                            get(t, "hook_entry").clone()
                        } else {
                            get(t, "identity").clone()
                        };
                        Value::Array(vec![
                            get(t, "kind").clone(),
                            get(t, "path").clone(),
                            identity,
                        ])
                    })
                    .collect(),
            ),
        ])
    };
    let selected: Vec<&Value> = rows
        .iter()
        .filter(|row| text(row, "phase") == phase)
        .collect();
    assert!(!selected.is_empty());
    let mut executed = Vec::new();
    for row in selected.iter().copied() {
        let point = text(row, "point").to_owned();
        let interrupted = World::new();
        write(
            &interrupted.path("codex/AGENTS.md"),
            "Keep original developer instructions.\n",
        );
        let actual = interrupted.path("actual-trace.jsonl");
        let exited = child_install(
            &interrupted,
            &source,
            &[
                ("AM_T2_MODE", install_mode.into()),
                ("AM_T2_PRESETS", "codex".into()),
                ("AGENT_MEMORY_FAULT_STAGE", point.clone()),
                ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
                (
                    "AGENT_MEMORY_FAULT_TRACE",
                    actual.to_string_lossy().into_owned(),
                ),
            ],
        );
        assert_eq!(exited.code, Some(86), "{point}");
        assert_eq!(text(trace_rows(&actual).last().unwrap(), "point"), point);
        let recovered = release_recover(&interrupted, &source);
        if text(&recovered, "status") == "complete" {
            let receipt = decode(&interrupted.state().join("receipt.json"));
            assert_eq!(
                physical_targets(&interrupted, &receipt),
                expected_files,
                "{point}"
            );
        } else {
            assert!(
                !interrupted.path("data/agent-memory/runtime").exists(),
                "{point}"
            );
            assert!(
                !interrupted.path("codex/skills/agent-memory").exists(),
                "{point}"
            );
            assert_eq!(
                std::fs::read_to_string(interrupted.path("codex/AGENTS.md")).unwrap(),
                "Keep original developer instructions.\n",
                "{point}"
            );
            assert!(
                interrupted.bin().join("am").symlink_metadata().is_err(),
                "{point}"
            );
            assert!(
                interrupted
                    .bin()
                    .join("agent-memory")
                    .symlink_metadata()
                    .is_err(),
                "{point}"
            );
        }
        let retried = release_ok(
            &interrupted,
            &source,
            &InstallRequest {
                mode: Some(Mode::parse(install_mode).unwrap()),
                presets: Some("codex".into()),
                ..release_request(&source)
            },
        );
        assert_eq!(get(&retried, "verified"), &Value::Bool(true));
        let current = decode(&interrupted.state().join("receipt.json"));
        assert_eq!(
            physical_targets(&interrupted, &current),
            expected_files,
            "{point}"
        );
        let relocate = |value: &Value| {
            json::encode(&semantics(value), json::SORTED_COMPACT_UTF8)
                .unwrap()
                .replace(world.root.to_str().unwrap(), "<W>")
                .replace(interrupted.root.to_str().unwrap(), "<W>")
        };
        assert_eq!(relocate(&current), relocate(&expected), "{point}");
        let stable = std::fs::read(interrupted.state().join("receipt.json")).unwrap();
        assert_eq!(
            get(
                &release_ok(&interrupted, &source, &InstallRequest::default()),
                "changed"
            ),
            &Value::Bool(false)
        );
        assert_eq!(
            std::fs::read(interrupted.state().join("receipt.json")).unwrap(),
            stable
        );
        executed.push(point);
    }
    // Every partition (step % 8) of this phase's observed inventory was executed.
    let expected_points: Vec<String> = selected
        .iter()
        .map(|row| text(row, "point").to_owned())
        .collect();
    assert_eq!(executed, expected_points);
    for partition in 0..8u64 {
        assert!(
            selected.iter().any(|row| get(row, "step")
                .as_int()
                .and_then(|s| s.as_u64())
                .is_some_and(|s| s % 8 == partition)),
            "partition {partition} of {install_mode}-{phase} is empty"
        );
    }
}

// obligation: tests/contracts/test_install_write_copy_before.py::InstallCopyBeforeContract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_install_write_copy_before.py::InstallCopyBeforeContract.test_actual_logical_writes_in_partition_4
// obligation: tests/contracts/test_install_write_copy_before_part_1.py::InstallCopyBeforePart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_install_write_copy_before_part_1.py::InstallCopyBeforePart1Contract.test_actual_logical_writes_in_partition_5
// obligation: tests/contracts/test_install_write_copy_before_part_2.py::InstallCopyBeforePart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_install_write_copy_before_part_2.py::InstallCopyBeforePart2Contract.test_actual_logical_writes_in_partition_6
// obligation: tests/contracts/test_install_write_copy_before_part_3.py::InstallCopyBeforePart3Contract.test_actual_logical_writes_in_partition_3
// obligation: tests/contracts/test_install_write_copy_before_part_3.py::InstallCopyBeforePart3Contract.test_actual_logical_writes_in_partition_7
#[test]
fn every_install_write_before_phase_interrupts_recovers_and_retries_in_copy_mode() {
    replay_install("copy", "before");
}

// obligation: tests/contracts/test_install_write_copy_after.py::LifecycleWriteContract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_install_write_copy_after.py::LifecycleWriteContract.test_actual_logical_writes_in_partition_4
// obligation: tests/contracts/test_install_write_copy_after_part_1.py::LifecycleWritePart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_install_write_copy_after_part_1.py::LifecycleWritePart1Contract.test_actual_logical_writes_in_partition_5
// obligation: tests/contracts/test_install_write_copy_after_part_2.py::LifecycleWritePart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_install_write_copy_after_part_2.py::LifecycleWritePart2Contract.test_actual_logical_writes_in_partition_6
// obligation: tests/contracts/test_install_write_copy_after_part_3.py::LifecycleWritePart3Contract.test_actual_logical_writes_in_partition_3
// obligation: tests/contracts/test_install_write_copy_after_part_3.py::LifecycleWritePart3Contract.test_actual_logical_writes_in_partition_7
#[test]
fn every_install_write_after_phase_interrupts_recovers_and_retries_in_copy_mode() {
    replay_install("copy", "after");
}

// obligation: tests/contracts/test_install_write_link_before.py::LifecycleWriteContract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_install_write_link_before.py::LifecycleWriteContract.test_actual_logical_writes_in_partition_4
// obligation: tests/contracts/test_install_write_link_before_part_1.py::LifecycleWritePart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_install_write_link_before_part_1.py::LifecycleWritePart1Contract.test_actual_logical_writes_in_partition_5
// obligation: tests/contracts/test_install_write_link_before_part_2.py::LifecycleWritePart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_install_write_link_before_part_2.py::LifecycleWritePart2Contract.test_actual_logical_writes_in_partition_6
// obligation: tests/contracts/test_install_write_link_before_part_3.py::LifecycleWritePart3Contract.test_actual_logical_writes_in_partition_3
// obligation: tests/contracts/test_install_write_link_before_part_3.py::LifecycleWritePart3Contract.test_actual_logical_writes_in_partition_7
#[test]
fn every_install_write_before_phase_interrupts_recovers_and_retries_in_link_mode() {
    replay_install("link", "before");
}

// obligation: tests/contracts/test_install_write_link_after.py::LifecycleWriteContract.test_actual_logical_writes_in_partition_0
// obligation: tests/contracts/test_install_write_link_after.py::LifecycleWriteContract.test_actual_logical_writes_in_partition_4
// obligation: tests/contracts/test_install_write_link_after_part_1.py::LifecycleWritePart1Contract.test_actual_logical_writes_in_partition_1
// obligation: tests/contracts/test_install_write_link_after_part_1.py::LifecycleWritePart1Contract.test_actual_logical_writes_in_partition_5
// obligation: tests/contracts/test_install_write_link_after_part_2.py::LifecycleWritePart2Contract.test_actual_logical_writes_in_partition_2
// obligation: tests/contracts/test_install_write_link_after_part_2.py::LifecycleWritePart2Contract.test_actual_logical_writes_in_partition_6
// obligation: tests/contracts/test_install_write_link_after_part_3.py::LifecycleWritePart3Contract.test_actual_logical_writes_in_partition_3
// obligation: tests/contracts/test_install_write_link_after_part_3.py::LifecycleWritePart3Contract.test_actual_logical_writes_in_partition_7
#[test]
fn every_install_write_after_phase_interrupts_recovers_and_retries_in_link_mode() {
    replay_install("link", "after");
}

// obligation: tests/contracts/test_lifecycle_lost_responses.py::LifecycleLostResponses.test_install_response_is_lost_after_real_receipt_and_targets_commit
#[test]
fn install_response_is_lost_after_real_receipt_and_targets_commit() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let lost = child_install(
        &world,
        &source,
        &[
            ("AM_T2_MODE", "copy".into()),
            ("AM_T2_PRESETS", "codex".into()),
            ("AGENT_MEMORY_FAULT_STAGE", "install-response-lost".into()),
            ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
        ],
    );
    assert_eq!(lost.code, Some(86));
    let committed = std::fs::read(world.state().join("receipt.json")).unwrap();
    assert_eq!(
        text(&decode(&world.state().join("journal.json")), "status"),
        "complete"
    );
    let repeated = ok(
        &world,
        &InstallRequest {
            mode: Some(Mode::Copy),
            presets: Some("codex".into()),
            ..request(&source)
        },
    );
    assert_eq!(get(&repeated, "changed"), &Value::Bool(false));
    assert_eq!(
        std::fs::read(world.state().join("receipt.json")).unwrap(),
        committed
    );
}

// ----- console bootstrap rewrite: only a Rust-era receipt selects update -----

static BOOTSTRAP_RECEIPT: std::sync::Mutex<Option<Result<Option<String>, String>>> =
    std::sync::Mutex::new(None);
struct FixtureReceipt;
impl agent_memory::console::InstalledReceipt for FixtureReceipt {
    fn receipt(&self) -> agent_memory::error::Result<Option<Value>> {
        match BOOTSTRAP_RECEIPT.lock().unwrap().clone().unwrap() {
            Ok(Some(raw)) => Ok(Some(json::decode(&raw, Default::default()).unwrap())),
            Ok(None) => Ok(None),
            Err(message) => Err(agent_memory::error::ContractError::new(message)),
        }
    }
}

#[test]
fn bootstrap_rewrites_to_update_only_for_rust_era_receipts() {
    agent_memory::console::register_installed_receipt(Some(Arc::new(FixtureReceipt)));
    let args = |tail: &[&str]| -> Vec<String> {
        ["install", "--bootstrap-binary", "/x/am"]
            .iter()
            .chain(tail)
            .map(|s| (*s).to_owned())
            .collect()
    };
    let receipt = |schema: &str, kind: &str| {
        format!(
            r#"{{"schema":"{schema}","source":{{"kind":"{kind}","remote":"https://github.com/robertelee78/agent-memory.git","ref":"releases/latest"}}}}"#
        )
    };
    let cases: Vec<(Result<Option<String>, String>, &[&str], Vec<String>)> = vec![
        (
            Ok(Some(receipt("agent-memory/install-receipt-v1", "managed"))),
            &[],
            args(&[]),
        ),
        (
            Ok(Some(receipt("agent-memory/install-receipt-v2", "managed"))),
            &["--check"],
            args(&["--check"]),
        ),
        (
            Ok(Some(receipt("agent-memory/install-receipt-v2", "release"))),
            &[],
            args(&[]),
        ),
        (
            Ok(Some(receipt("agent-memory/install-receipt-v3", "release"))),
            &[],
            vec!["update".into(), "--bootstrap-binary".into(), "/x/am".into()],
        ),
        (
            Ok(Some(receipt("agent-memory/install-receipt-v3", "release"))),
            &["--check"],
            vec![
                "update".into(),
                "--bootstrap-binary".into(),
                "/x/am".into(),
                "--check".into(),
            ],
        ),
        (
            Ok(Some(receipt("agent-memory/install-receipt-v3", "managed"))),
            &[],
            vec!["update".into(), "--bootstrap-binary".into(), "/x/am".into()],
        ),
        (
            Ok(Some(receipt("agent-memory/install-receipt-v9", "release"))),
            &[],
            args(&[]),
        ),
        (Ok(None), &[], args(&[])),
        (
            Err("Invalid installation ownership receipt".into()),
            &[],
            args(&[]),
        ),
    ];
    for (provided, tail, expected) in cases {
        *BOOTSTRAP_RECEIPT.lock().unwrap() = Some(provided.clone());
        assert_eq!(
            agent_memory::console::bootstrap_command(args(tail)).unwrap(),
            expected,
            "{provided:?}"
        );
    }
    // A different channel on a v3 receipt keeps the explicit install.
    *BOOTSTRAP_RECEIPT.lock().unwrap() = Some(Ok(Some(receipt(
        "agent-memory/install-receipt-v3",
        "release",
    ))));
    let other = args(&["--ref", "main"]);
    assert_eq!(
        agent_memory::console::bootstrap_command(other.clone()).unwrap(),
        other
    );
}

// ======================================================================
// Checkpoint 3: Python-era (v0.0.14) adoption from derived capture fixtures
// ======================================================================

fn fixture_root(scenario: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/v014-state")
        .join(scenario)
}
fn relocate(world: &World, text: &str) -> String {
    text.replace("@WORLD@", world.root.to_str().unwrap())
}

/// Materialize a derived capture in `world`: state files with their recorded
/// modes, owned links, opaque owned trees. Genuine captured identities are kept
/// for unchanged files; only relocated bytes/links and opaque trees are rebound.
fn materialize(world: &World, scenario: &str) -> Value {
    let base = fixture_root(scenario);
    let modes = std::fs::read_to_string(base.join("modes.txt")).unwrap();
    for line in modes.lines() {
        let (bits, relative) = line.split_once(' ').unwrap();
        let raw = std::fs::read_to_string(base.join(relative)).unwrap();
        let path = world.path(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, relocate(world, &raw)).unwrap();
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
    let receipt_path = world.state().join("receipt.json");
    let mut receipt = decode(&receipt_path);
    let captured = decode(&base.join("identities.json"));
    if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
        for target in targets.iter_mut() {
            let path = PathBuf::from(text(target, "path"));
            let identity = install::fingerprint(&path).unwrap();
            let relative = path.strip_prefix(&world.root).unwrap().to_str().unwrap();
            if let Some(captured) = captured.get(relative) {
                assert!(install::same(&identity, captured), "{scenario}: {relative}");
                assert!(
                    install::same(get(target, "identity"), captured),
                    "{scenario}: {relative}"
                );
                continue;
            }
            if let Value::Object(fields) = target {
                for (key, value) in fields.iter_mut() {
                    if key == "identity" {
                        *value = identity.clone();
                    }
                }
            }
        }
    }
    store_receipt(world, &receipt);
    receipt
}
/// Write a Python-era receipt and keep its committed journal consistent.
fn store_receipt(world: &World, receipt: &Value) {
    let receipt_path = world.state().join("receipt.json");
    std::fs::remove_file(&receipt_path).unwrap();
    write(&receipt_path, install::encoded(receipt).unwrap());
    mode(&receipt_path, 0o600);
    let journal_path = world.state().join("journal.json");
    let mut journal = decode(&journal_path);
    let identity = install::fingerprint(&receipt_path).unwrap();
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
fn old_pack(receipt: &Value) -> PathBuf {
    PathBuf::from(text(get(receipt, "source"), "pack"))
}
fn adopt(world: &World, source: &Path) -> (i32, Value) {
    run(world, &request(source))
}

#[test]
fn derived_fixtures_hold_no_python_source_or_archive() {
    for path in walk(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v014-state")) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            !name.ends_with(".py") && !name.ends_with(".gz") && !name.ends_with(".tar"),
            "{name}"
        );
    }
}

#[test]
fn captured_wrappers_and_hook_commands_are_exactly_the_recognised_python_era_bytes() {
    for scenario in [
        "all-copy",
        "all-link",
        "claude-opencode-copy",
        "codex-copy-pending-source",
    ] {
        let world = World::new();
        let receipt = materialize(&world, scenario);
        let pack = old_pack(&receipt);
        let wrapper = install::legacy_wrapper(&pack).unwrap();
        for name in ["am", "agent-memory"] {
            assert_eq!(
                std::fs::read(world.bin().join(name)).unwrap(),
                wrapper,
                "{scenario} {name}"
            );
        }
        for target in targets(&receipt)
            .iter()
            .filter(|t| text(t, "kind") == "hook-config")
        {
            let client = if text(target, "path").ends_with("hooks.json") {
                "codex"
            } else {
                "claude"
            };
            let (prompt, stop) =
                agent_memory::client_hooks::python_era_entries(client, &pack).unwrap();
            assert!(
                install::same(get(target, "hook_entry"), &prompt),
                "{scenario} {client}"
            );
            assert!(
                install::same(get(target, "stop_entry"), &stop),
                "{scenario} {client}"
            );
        }
    }
}

// adoption row: install receipt v1, wrappers, client hooks, activation blocks,
// lifecycle owner/lock, committed journal history, managed source clone.
#[test]
fn adoption_takes_over_every_python_era_target_and_preserves_history() {
    let world = World::new();
    let receipt = materialize(&world, "all-copy");
    let source = development_source(&world, "dev");
    let state = world.state();
    // Committed Python journal history (a prior transaction's journal) is preserved.
    let history = state.join("journal-5593988c255c41b1b37d1e9d2aebd7d4.json");
    std::fs::copy(state.join("journal.json"), &history).unwrap();
    mode(&history, 0o600);
    let history_bytes = std::fs::read(&history).unwrap();
    let lock_before = std::fs::metadata(state.join("lock")).unwrap();
    let lock_bytes = std::fs::read(state.join("lock")).unwrap();
    let legacy_tree = world.path("data/agent-memory/source");
    let legacy_before = snapshot(&legacy_tree);
    let settings_before = decode(&world.path("claude/settings.json"));
    let result = ok(&world, &request(&source));
    let adopted = get(&result, "receipt").clone();
    assert_eq!(text(&adopted, "schema"), "agent-memory/install-receipt-v3");
    assert_eq!(
        text(&adopted, "installation_id"),
        text(&receipt, "installation_id")
    );
    assert_eq!(
        text(get(&result, "adoption"), "from_schema"),
        "agent-memory/install-receipt-v1"
    );
    assert_eq!(
        get(get(&result, "adoption"), "rollback_to_python_after_commit"),
        &Value::Bool(false)
    );
    assert!(text(&result, "restart_notice").contains("restart"));
    // Same lock inode and bytes; owner metadata re-published for this process.
    let lock_after = std::fs::metadata(state.join("lock")).unwrap();
    assert_eq!(
        (lock_after.dev(), lock_after.ino()),
        (lock_before.dev(), lock_before.ino())
    );
    assert_eq!(std::fs::read(state.join("lock")).unwrap(), lock_bytes);
    let owner = decode(&state.join("owner.json"));
    assert_eq!(
        get(&owner, "pid").as_int().unwrap().as_u64(),
        Some(u64::from(std::process::id()))
    );
    assert_eq!(std::fs::read(&history).unwrap(), history_bytes);
    // The Python managed clone stays in place, inert, as a legacy-source target.
    assert_eq!(snapshot(&legacy_tree), legacy_before);
    assert_eq!(
        text(&target(&adopted, &legacy_tree), "kind"),
        "legacy-source"
    );
    // Wrappers became owned links to the Rust binary.
    for name in ["am", "agent-memory"] {
        let link = world.bin().join(name);
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            source.join("target/release/am")
        );
    }
    // Only the owned hook groups were rewritten; their shapes keep the Python record.
    let settings = decode(&world.path("claude/settings.json"));
    let prompt = &get(get(&settings, "hooks"), "UserPromptSubmit")
        .as_array()
        .unwrap()[0];
    assert!(
        text(
            &prompt.get("hooks").unwrap().as_array().unwrap()[0],
            "command"
        )
        .ends_with(" hook turn-hook")
    );
    assert!(
        !json::encode(&settings, json::DEFAULT)
            .unwrap()
            .contains("python3.sh")
    );
    assert_eq!(
        settings.as_object().unwrap().len(),
        settings_before.as_object().unwrap().len()
    );
    let hook_target = target(&adopted, &world.path("claude/settings.json"));
    assert_eq!(get(&hook_target, "originally_absent"), &Value::Bool(true));
    // Activation blocks were replaced in place, never duplicated.
    let agents = std::fs::read_to_string(world.path("codex/AGENTS.md")).unwrap();
    assert_eq!(agents.matches(install::BEGIN).count(), 1);
    // The previous receipt is retained as a backup; the new one is committed.
    let backups = get(&adopted, "backups").as_array().unwrap();
    assert!(
        backups
            .iter()
            .any(|b| text(b, "original_path") == state.join("receipt.json").to_str().unwrap())
    );
    assert_eq!(
        text(&decode(&state.join("journal.json")), "status"),
        "complete"
    );
    // A repeat is a verified no-op on the Rust receipt.
    assert_eq!(
        get(&ok(&world, &InstallRequest::default()), "changed"),
        &Value::Bool(false)
    );
}

#[test]
fn adoption_succeeds_for_every_captured_scenario() {
    for scenario in [
        "all-copy",
        "all-link",
        "claude-opencode-copy",
        "codex-copy-pending-source",
    ] {
        let world = World::new();
        let receipt = materialize(&world, scenario);
        let source = development_source(&world, "dev");
        let (code, value) = adopt(&world, &source);
        assert_eq!(code, 0, "{scenario}: {}", error_text(&value));
        let adopted = get(&value, "receipt");
        assert_eq!(text(adopted, "mode"), text(&receipt, "mode"), "{scenario}");
        assert_eq!(
            text(adopted, "presets"),
            text(&receipt, "presets"),
            "{scenario}"
        );
        let restart: Vec<_> = get(&value, "restart_clients").as_array().unwrap().to_vec();
        assert_eq!(
            restart,
            *get(&receipt, "enabled_adapters").as_array().unwrap(),
            "{scenario}"
        );
    }
}

fn refuses_without_mutation(world: &World, source: &Path, expected: &str) {
    let before = without_lock(snapshot(&world.root));
    let (code, value) = adopt(world, source);
    assert_eq!(code, 1, "{}", error_text(&value));
    assert!(
        error_text(&value).contains(expected),
        "{} / {expected}",
        error_text(&value)
    );
    assert_eq!(without_lock(snapshot(&world.root)), before);
    assert_eq!(
        text(&decode(&world.state().join("receipt.json")), "schema"),
        "agent-memory/install-receipt-v1"
    );
}

// adoption row: receipt v1/v2 with a drifted owned target refuses and names it.
#[test]
fn adoption_refuses_a_drifted_owned_target_naming_it() {
    let world = World::new();
    materialize(&world, "all-copy");
    let source = development_source(&world, "dev");
    let drifted = world.path("codex/skills/agent-memory");
    write(&drifted.join("user-note.txt"), "My edit.\n");
    refuses_without_mutation(
        &world,
        &source,
        &format!(
            "Unrelated or user-modified target; preserve: {}",
            drifted.display()
        ),
    );
}

// adoption row: unknown receipt schema refuses without mutation.
#[test]
fn adoption_refuses_an_unknown_receipt_schema() {
    let world = World::new();
    let mut receipt = materialize(&world, "all-copy");
    if let Value::Object(fields) = &mut receipt {
        for (key, value) in fields.iter_mut() {
            if key == "schema" {
                *value = Value::String("agent-memory/install-receipt-v0".into());
            }
        }
    }
    store_receipt(&world, &receipt);
    let source = development_source(&world, "dev");
    let before = without_lock(snapshot(&world.root));
    let (code, value) = adopt(&world, &source);
    assert_eq!(code, 1);
    assert!(
        error_text(&value).starts_with("Invalid installation ownership receipt"),
        "{}",
        error_text(&value)
    );
    assert_eq!(without_lock(snapshot(&world.root)), before);
}

// adoption row: wrapper bytes must equal the exact recorded Python wrapper.
#[test]
fn adoption_refuses_a_wrapper_that_is_not_the_exact_python_wrapper() {
    let world = World::new();
    let mut receipt = materialize(&world, "all-copy");
    let wrapper = world.bin().join("am");
    std::fs::write(&wrapper, b"#!/bin/sh\necho my own am\n").unwrap();
    // Even with the receipt claiming the edited bytes, they are not the installer's wrapper.
    let identity = install::fingerprint(&wrapper).unwrap();
    if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
        for target in targets.iter_mut() {
            if text(target, "path") == wrapper.to_str().unwrap() {
                if let Value::Object(fields) = target {
                    for (key, value) in fields.iter_mut() {
                        if key == "identity" {
                            *value = identity.clone();
                        }
                    }
                }
            }
        }
    }
    store_receipt(&world, &receipt);
    let source = development_source(&world, "dev");
    refuses_without_mutation(
        &world,
        &source,
        "Python-era wrapper differs from the exact recorded wrapper; preserve as foreign",
    );
    // Without the receipt edit the wrapper is drifted and refused as well.
    let world = World::new();
    materialize(&world, "all-copy");
    std::fs::write(world.bin().join("agent-memory"), b"#!/bin/sh\necho mine\n").unwrap();
    let source = development_source(&world, "dev");
    refuses_without_mutation(
        &world,
        &source,
        "Unrelated or user-modified target; preserve",
    );
}

// adoption row: only the exact prior client hook command strings are owned.
#[test]
fn adoption_refuses_hook_entries_that_are_not_the_exact_python_commands() {
    let world = World::new();
    let mut receipt = materialize(&world, "all-copy");
    let codex = world.path("codex/hooks.json");
    let raw = std::fs::read_to_string(&codex)
        .unwrap()
        .replace("codex_prompt.py", "my_prompt.py");
    std::fs::write(&codex, &raw).unwrap();
    let identity = install::fingerprint(&codex).unwrap();
    if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
        for target in targets.iter_mut() {
            if text(target, "path") == codex.to_str().unwrap() {
                let rewritten = json::encode(target, json::DEFAULT)
                    .unwrap()
                    .replace("codex_prompt.py", "my_prompt.py");
                *target = json::decode(&rewritten, Default::default()).unwrap();
                if let Value::Object(fields) = target {
                    for (key, value) in fields.iter_mut() {
                        if key == "identity" {
                            *value = identity.clone();
                        }
                    }
                }
            }
        }
    }
    store_receipt(&world, &receipt);
    let source = development_source(&world, "dev");
    refuses_without_mutation(
        &world,
        &source,
        "Python-era client hook command is not the exact installer entry; preserve",
    );
}

// adoption row: a modified activation block is a refusal.
#[test]
fn adoption_refuses_a_modified_activation_block() {
    let world = World::new();
    let mut receipt = materialize(&world, "all-copy");
    let agents = world.path("codex/AGENTS.md");
    let raw = std::fs::read_to_string(&agents)
        .unwrap()
        .replace("load the agent-memory skill", "my changed block");
    std::fs::write(&agents, raw).unwrap();
    let identity = install::fingerprint(&agents).unwrap();
    if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
        for target in targets.iter_mut() {
            if text(target, "path") == agents.to_str().unwrap() {
                if let Value::Object(fields) = target {
                    for (key, value) in fields.iter_mut() {
                        if key == "identity" {
                            *value = identity.clone();
                        }
                    }
                }
            }
        }
    }
    store_receipt(&world, &receipt);
    let source = development_source(&world, "dev");
    refuses_without_mutation(
        &world,
        &source,
        "Activation block is unrelated or modified; preserve and reconcile manually",
    );
}

// adoption row: a live Python owner holding the lifecycle flock blocks adoption.
#[test]
fn adoption_is_busy_while_a_python_owner_holds_the_lock() {
    let world = World::new();
    materialize(&world, "all-copy");
    let source = development_source(&world, "dev");
    let lock = agent_memory::fs::Descriptor::open_directory(&world.state())
        .unwrap()
        .open_at(std::ffi::OsStr::new("lock"), libc::O_RDWR, 0)
        .unwrap();
    let held =
        agent_memory::fs::FlockGuard::try_acquire(lock, agent_memory::fs::LockMode::Exclusive)
            .unwrap();
    let before = snapshot(&world.root);
    let (code, value) = adopt(&world, &source);
    assert_eq!(code, 1);
    assert_eq!(
        error_text(&value),
        "Another lifecycle process holds the lock; retry after it finishes"
    );
    assert_eq!(snapshot(&world.root), before);
    drop(held);
    // A dead owner (the captured PID's process is gone) follows normal recovery.
    assert_eq!(adopt(&world, &source).0, 0);
}

fn in_flight(world: &World, kind: &str) -> (PathBuf, Vec<u8>) {
    // A Python update interrupted mid-activation of codex/AGENTS.md: the new
    // bytes are active, the original sits in its stage, the receipt is the old one.
    let state = world.state();
    let agents = world.path("codex/AGENTS.md");
    let original = std::fs::read(&agents).unwrap();
    let before = install::fingerprint(&agents).unwrap();
    let transaction = "0123456789abcdef0123456789abcdef";
    let stage = world.path(&format!("codex/.agent-memory-{transaction}-0"));
    std::fs::copy(&agents, &stage).unwrap();
    mode(&stage, 0o644);
    std::fs::write(
        &agents,
        [original.as_slice(), b"Interrupted Python update.\n"].concat(),
    )
    .unwrap();
    let after = install::fingerprint(&agents).unwrap();
    let mut journal = decode(&state.join("journal.json"));
    if let Value::Object(fields) = &mut journal {
        for (key, value) in fields.iter_mut() {
            match key.as_str() {
                "status" => *value = Value::String("activating".into()),
                "transaction" => *value = Value::String(transaction.into()),
                "receipt_identity" => *value = Value::Null,
                "activated" => *value = Value::Int(1u32.into()),
                "actions" => {
                    *value = Value::Array(vec![Value::Object(vec![
                        (
                            "path".into(),
                            Value::String(agents.to_string_lossy().into_owned()),
                        ),
                        ("kind".into(), Value::String(kind.into())),
                        ("before".into(), before.clone()),
                        ("after".into(), after.clone()),
                        (
                            "stage".into(),
                            Value::String(stage.to_string_lossy().into_owned()),
                        ),
                        ("backup".into(), Value::Null),
                    ])])
                }
                _ => {}
            }
        }
    }
    std::fs::remove_file(state.join("journal.json")).unwrap();
    write(
        &state.join("journal.json"),
        install::encoded(&journal).unwrap(),
    );
    mode(&state.join("journal.json"), 0o600);
    (agents, original)
}

// adoption row: an in-flight Python journal with known step kinds is finished
// (rolled back from its recorded steps) before adoption ever starts.
#[test]
fn adoption_rolls_back_an_in_flight_python_journal_before_starting() {
    let world = World::new();
    materialize(&world, "all-copy");
    let (agents, original) = in_flight(&world, "activation");
    let source = development_source(&world, "dev");
    let result = ok(&world, &request(&source));
    assert_eq!(
        text(get(&result, "receipt"), "schema"),
        "agent-memory/install-receipt-v3"
    );
    let text_now = std::fs::read_to_string(&agents).unwrap();
    assert!(!text_now.contains("Interrupted Python update."));
    assert!(
        text_now.starts_with(
            std::str::from_utf8(&original)
                .unwrap()
                .split(install::BEGIN)
                .next()
                .unwrap()
        )
    );
    // The interrupted transaction's journal is retained as history.
    let kept: Vec<_> = std::fs::read_dir(world.state())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("journal-"))
        .collect();
    assert_eq!(kept.len(), 1);
    let retained = decode(&world.state().join(&kept[0]));
    assert_eq!(text(&retained, "status"), "rolled-back");
}

// adoption row: an in-flight journal with an unknown step kind refuses and
// directs to the retained evidence, without mutation.
#[test]
fn adoption_refuses_an_in_flight_journal_with_unknown_step_kinds() {
    let world = World::new();
    materialize(&world, "all-copy");
    in_flight(&world, "python-only-step");
    let source = development_source(&world, "dev");
    let before = without_lock(snapshot(&world.root));
    let (code, value) = adopt(&world, &source);
    assert_eq!(code, 1);
    assert!(
        error_text(&value).starts_with("Lifecycle journal step kind is unknown"),
        "{}",
        error_text(&value)
    );
    assert_eq!(without_lock(snapshot(&world.root)), before);
}

// adoption row: journaled finish/rollback of an interrupted adoption; after
// commit, Python-era targets are never re-activated.
#[test]
fn interrupted_adoption_rolls_back_before_commit_and_never_after() {
    let world = World::new();
    materialize(&world, "all-copy");
    let source = signed_release(&world, "release", STAND_IN);
    let python = without_lock(snapshot(&world.root));
    let failed = child_install(
        &world,
        &source,
        &[
            ("AGENT_MEMORY_FAULT_STAGE", "install-before-receipt".into()),
            ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
        ],
    );
    assert_eq!(failed.code, Some(86));
    let value = release_recover(&world, &source);
    assert_eq!(text(&value, "status"), "rolled-back");
    assert!(!world.path("data/agent-memory/runtime").exists());
    let mut restored = without_lock(snapshot(&world.root));
    restored.retain(|k, _| !k.contains(".agent-memory-") && !k.starts_with("state/"));
    let mut expected = python.clone();
    expected.retain(|k, _| !k.starts_with("state/"));
    assert_eq!(restored, expected);
    assert_eq!(
        text(&decode(&world.state().join("receipt.json")), "schema"),
        "agent-memory/install-receipt-v1"
    );
    // Committed adoption: a lost response completes, never re-activating Python bytes.
    let committed = child_install(
        &world,
        &source,
        &[
            ("AGENT_MEMORY_FAULT_STAGE", "install-after-receipt".into()),
            ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
        ],
    );
    assert_eq!(committed.code, Some(86));
    let value = release_recover(&world, &source);
    assert_eq!(text(&value, "status"), "complete");
    assert_eq!(
        text(&decode(&world.state().join("receipt.json")), "schema"),
        "agent-memory/install-receipt-v3"
    );
    assert!(std::fs::read_link(world.bin().join("am")).is_ok());
    assert_eq!(
        std::fs::read(world.path("data/agent-memory/runtime/bin/am")).unwrap(),
        STAND_IN
    );
}

#[test]
fn python_era_receipt_without_a_verified_runtime_source_refuses_with_bootstrap_guidance() {
    let world = World::new();
    materialize(&world, "all-copy");
    let before = without_lock(snapshot(&world.root));
    let (code, value) = run(&world, &InstallRequest::default());
    assert_eq!(code, 1);
    assert_eq!(error_text(&value), install::ADOPTION_GUIDANCE);
    // Managed --remote/--ref on the adopted channel still refuses updating it.
    let (_, value) = run(
        &world,
        &InstallRequest {
            remote: Some("https://example.invalid/other.git".into()),
            ..InstallRequest::default()
        },
    );
    assert_eq!(error_text(&value), install::MANAGED_GUIDANCE);
    assert_eq!(without_lock(snapshot(&world.root)), before);
}

// adoption row: OMP binding (receipt v2) — re-issued for the Rust artifact
// digest; rollout attestation must be supplied again, never inherited.
struct FixtureOmp;
impl install::OmpBoundary for FixtureOmp {
    fn capability(&self) -> &str {
        agent_memory::omp_installation::READER_CAPABILITY
    }
    fn native_transition(&self, _: &Value, _: bool) -> install::Result<()> {
        Ok(())
    }
    fn native_activation(&self, _: &Value, _: bool) -> install::Result<Value> {
        Ok(Value::Object(Vec::new()))
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
        Ok(Value::String("verified".into()))
    }
}

fn python_v2_omp_world() -> (World, PathBuf, Value) {
    let world = World::new();
    let mut receipt = materialize(&world, "codex-copy-pending-source");
    let agent = world.path("omp agent");
    std::fs::create_dir_all(&agent).unwrap();
    let binding = agent_memory::omp_installation::binding(
        agent.to_str().unwrap(),
        false,
        agent_memory::omp_installation::READER_CAPABILITY,
    )
    .unwrap();
    let pack = old_pack(&receipt);
    let digest = text(get(&receipt, "source"), "artifact_digest").to_owned();
    let skill = agent.join("skills/agent-memory");
    write(&skill.join("opaque"), "opaque omp copy\n");
    mode(&skill, 0o700);
    let extension = agent.join("extensions/agent-memory.ts");
    write(
        &extension,
        install::legacy_extension_wrapper(&pack, &digest, &binding).unwrap(),
    );
    mode(&extension, 0o644);
    let template =
        include_str!("../skills/agent-memory/templates/activation-omp-markdown.example.txt");
    let activation = agent.join("AGENTS.md");
    write(&activation, format!("Unrelated omp rules.\n{template}\n"));
    mode(&activation, 0o644);
    let target = |path: &Path, kind: &str, extra: Vec<(String, Value)>| {
        let mut fields = vec![
            (
                "path".to_owned(),
                Value::String(path.to_string_lossy().into_owned()),
            ),
            ("kind".to_owned(), Value::String(kind.into())),
            ("identity".to_owned(), install::fingerprint(path).unwrap()),
            ("pre_change_identity".to_owned(), Value::Null),
        ];
        fields.extend(extra);
        Value::Object(fields)
    };
    let digest_field = || vec![("artifact_digest".to_owned(), Value::String(digest.clone()))];
    if let Value::Object(fields) = &mut receipt {
        for (key, value) in fields.iter_mut() {
            match key.as_str() {
                "schema" => *value = Value::String("agent-memory/install-receipt-v2".into()),
                "presets" => *value = Value::String("codex,omp".into()),
                "enabled_adapters" => {
                    *value = Value::Array(vec![
                        Value::String("codex".into()),
                        Value::String("omp".into()),
                    ])
                }
                "targets" => {
                    if let Value::Array(items) = value {
                        items.push(target(&skill, "copy", digest_field()));
                        items.push(target(
                            &activation,
                            "activation",
                            vec![
                                (
                                    "block_hash".into(),
                                    Value::String(agent_memory::release_distribution::digest(
                                        template.as_bytes(),
                                    )),
                                ),
                                ("originally_absent".into(), Value::Bool(false)),
                            ],
                        ));
                        items.push(target(&extension, "wrapper", digest_field()));
                    }
                }
                _ => {}
            }
        }
        fields.push(("omp_binding".into(), binding));
    }
    store_receipt(&world, &receipt);
    (world, agent, receipt)
}

#[test]
fn adoption_reissues_the_omp_binding_only_with_renewed_rollout_attestation() {
    let (world, agent, old) = python_v2_omp_world();
    assert!(
        install::read_receipt(&world.env(), &world.state())
            .unwrap()
            .is_some()
    );
    let source = development_source(&world, "dev");
    let mut services = services();
    services.omp_boundary = Some(Arc::new(FixtureOmp));
    let before = without_lock(snapshot(&world.root));
    let refused = install::install_command(&world.env(), &services, &request(&source));
    let value = refused.value.unwrap();
    assert_eq!(refused.exit_code, 1);
    assert!(
        error_text(&value).contains("--omp-peers-upgraded"),
        "{}",
        error_text(&value)
    );
    assert_eq!(without_lock(snapshot(&world.root)), before);
    let renewed = InstallRequest {
        omp_peers_upgraded: true,
        ..request(&source)
    };
    let result = install::install_command(&world.env(), &services, &renewed);
    let value = result.value.unwrap();
    assert_eq!(result.exit_code, 0, "{}", error_text(&value));
    let adopted = get(&value, "receipt");
    assert_eq!(text(adopted, "schema"), "agent-memory/install-receipt-v3");
    assert_eq!(
        get(get(adopted, "omp_binding"), "rollout_attested"),
        &Value::Bool(true)
    );
    let digest = text(get(adopted, "source"), "artifact_digest");
    assert_ne!(digest, text(get(&old, "source"), "artifact_digest"));
    let extension = std::fs::read_to_string(agent.join("extensions/agent-memory.ts")).unwrap();
    assert!(extension.contains(&format!("\"artifact\":\"{digest}\"")));
    assert!(extension.contains(&format!(
        "\"executable\":\"{}\"",
        source.join("target/release/am").display()
    )));
    assert_eq!(
        get(
            &target(adopted, &agent.join("extensions/agent-memory.ts")),
            "artifact_digest"
        )
        .as_str(),
        Some(digest)
    );
    assert!(
        get(&value, "restart_clients")
            .as_array()
            .unwrap()
            .contains(&Value::String("omp".into()))
    );
    assert_eq!(text(&value, "stored_source_activation"), "verified");
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_other_clients_keep_shared_activation_without_omp_template
#[test]
fn other_clients_keep_shared_activation_without_omp_template() {
    let world = World::new();
    let source = development_source(&world, "dev");
    std::fs::remove_file(
        source.join("skills/agent-memory/templates/activation-omp-markdown.example.txt"),
    )
    .unwrap();
    let installed = ok(
        &world,
        &InstallRequest {
            presets: Some("codex,claude,opencode".into()),
            ..request(&source)
        },
    );
    let common = install::SHIM.as_bytes();
    let mut seen = 0;
    for target in targets(get(&installed, "receipt")) {
        if text(&target, "kind") == "activation" {
            seen += 1;
            assert_eq!(
                std::fs::read(text(&target, "path")).unwrap(),
                [common, b"\n"].concat()
            );
            assert_eq!(
                text(&target, "block_hash"),
                agent_memory::release_distribution::digest(common)
            );
        }
    }
    assert_eq!(seen, 3);
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_interrupted_claude_stop_install_restores_original_settings
#[test]
fn interrupted_claude_stop_install_restores_original_settings() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let settings = world.path("claude/settings.json");
    let original =
        b"{\"hooks\":{\"Stop\":[{\"hooks\":[{\"type\":\"command\",\"command\":\"foreign\"}]}]}}\n";
    write(&settings, original);
    let exited = child_install(
        &world,
        &source,
        &[
            ("AM_T2_PRESETS", "claude".into()),
            ("AGENT_MEMORY_FAULT_STAGE", "install-before-receipt".into()),
            ("AGENT_MEMORY_FAULT_MODE", "exit".into()),
        ],
    );
    assert_eq!(exited.code, Some(86));
    let (code, value) = recover(&world);
    assert_eq!(code, 0, "{}", error_text(&value));
    assert_eq!(std::fs::read(&settings).unwrap(), original);
}

// obligation: tests/contracts/test_installer.py::InstallerContract.test_bank_state_overlap_is_refused_before_any_bank_or_runtime_write
#[test]
fn bank_state_overlap_is_refused_before_any_bank_or_runtime_write() {
    let world = World::new();
    let source = development_source(&world, "dev");
    let bank = world.path("repo/memory-bank");
    write(&bank.join("MEMORY.md"), "# Memory\n");
    write(
        &bank.join("projectbrief.md"),
        "# Project\n\nPreserve shared project learning.\n",
    );
    let mut pairs = world.pairs();
    for (key, value) in &mut pairs {
        if key == "XDG_STATE_HOME" {
            *value = bank.to_string_lossy().into_owned();
        }
    }
    let env = Environment::from_pairs(
        pairs
            .into_iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    );
    let before = snapshot(&world.root);
    for check in [false, true] {
        let result = install::install_command(
            &env,
            &services(),
            &InstallRequest {
                check,
                ..request(&source)
            },
        );
        assert_eq!(result.exit_code, 1);
        assert!(error_text(&result.value.unwrap()).contains("Memory Bank"));
        assert_eq!(snapshot(&world.root), before);
    }
    let recovered = install::recover_command(&env, &services());
    assert_eq!(recovered.exit_code, 1);
    assert_eq!(snapshot(&world.root), before);
}

fn set(value: &mut Value, key: &str, replacement: Value) {
    let fields = match value {
        Value::Object(fields) => fields,
        _ => panic!("test object required"),
    };
    if let Some((_, value)) = fields.iter_mut().find(|(name, _)| name == key) {
        *value = replacement;
    } else {
        fields.push((key.into(), replacement));
    }
}

/// A captured Python journal shape with several activated node kinds, not just
/// an activation-file step. The prior receipt remains byte-for-byte committed.
fn in_flight_nodes(world: &World, kinds: &[&str]) -> Vec<(PathBuf, Value)> {
    let receipt = decode(&world.state().join("receipt.json"));
    let transaction = "0123456789abcdef0123456789abcdef";
    let mut actions = Vec::new();
    let mut originals = Vec::new();
    for (index, kind) in kinds.iter().enumerate() {
        let target_kind = if *kind == "command-link" {
            "wrapper"
        } else {
            kind
        };
        let target = targets(&receipt)
            .into_iter()
            .find(|target| text(target, "kind") == target_kind)
            .unwrap();
        let path = PathBuf::from(text(&target, "path"));
        let before = install::fingerprint(&path).unwrap();
        let stage = path
            .parent()
            .unwrap()
            .join(format!(".agent-memory-{transaction}-{index}"));
        std::fs::rename(&path, &stage).unwrap();
        if *kind == "command-link" {
            symlink(world.path("future-runtime/bin/am"), &path).unwrap();
        } else if text(&before, "kind") == "directory" {
            std::fs::create_dir(&path).unwrap();
            write(&path.join("interrupted"), "Interrupted Python update.\n");
            mode(
                &path,
                get(&before, "mode").as_int().unwrap().as_u64().unwrap() as u32,
            );
        } else {
            let original = std::fs::read(&stage).unwrap();
            write(
                &path,
                [original.as_slice(), b"Interrupted Python update.\n"].concat(),
            );
            mode(
                &path,
                get(&before, "mode").as_int().unwrap().as_u64().unwrap() as u32,
            );
        }
        actions.push(Value::Object(vec![
            ("path".into(), Value::String(path.to_str().unwrap().into())),
            (
                "stage".into(),
                Value::String(stage.to_str().unwrap().into()),
            ),
            ("kind".into(), Value::String((*kind).into())),
            ("before".into(), before.clone()),
            ("after".into(), install::fingerprint(&path).unwrap()),
            ("backup".into(), Value::Null),
        ]));
        originals.push((path, before));
    }
    let mut journal = decode(&world.state().join("journal.json"));
    set(&mut journal, "status", Value::String("activating".into()));
    set(
        &mut journal,
        "transaction",
        Value::String(transaction.into()),
    );
    set(&mut journal, "receipt_identity", Value::Null);
    set(&mut journal, "activated", Value::Int(kinds.len().into()));
    set(&mut journal, "actions", Value::Array(actions));
    write(
        &world.state().join("journal.json"),
        install::encoded(&journal).unwrap(),
    );
    originals
}

#[test]
fn in_flight_python_journal_restores_source_copy_wrapper_and_activation_before_adoption() {
    let world = World::new();
    materialize(&world, "all-copy");
    let receipt = std::fs::read(world.state().join("receipt.json")).unwrap();
    let originals = in_flight_nodes(&world, &["source", "copy", "wrapper", "activation"]);
    let (code, result) = recover(&world);
    assert_eq!(code, 0, "{result:?}");
    assert_eq!(text(&result, "status"), "rolled-back");
    for (path, identity) in originals {
        assert!(
            install::same(&install::fingerprint(&path).unwrap(), &identity),
            "{}",
            path.display()
        );
    }
    assert_eq!(
        std::fs::read(world.state().join("receipt.json")).unwrap(),
        receipt
    );
    let source = development_source(&world, "dev");
    let result = ok(&world, &request(&source));
    assert_eq!(
        text(get(&result, "receipt"), "schema"),
        install::RECEIPT_SCHEMA
    );
}

struct RecheckingUpdating(std::sync::atomic::AtomicUsize);
impl Updating for RecheckingUpdating {
    fn banks(
        &self,
        repositories: &[String],
        compatibility: &Value,
        readable: bool,
    ) -> install::Result<Value> {
        if readable && self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            return Err(
                agent_memory::error::ContractError::new("Concurrent bank migration").into(),
            );
        }
        FixtureUpdating.banks(repositories, compatibility, readable)
    }
    fn qualified_current(
        &self,
        state: &Path,
        code: &Path,
    ) -> install::Result<Option<Qualification>> {
        FixtureUpdating.qualified_current(state, code)
    }
    fn current(
        &self,
        state: &Path,
        code: &Path,
        cache: Option<&Qualification>,
    ) -> install::Result<()> {
        FixtureUpdating.current(state, code, cache)
    }
    fn release_transition(&self, old: &Value, new: &Value, root: &Path) -> install::Result<()> {
        FixtureUpdating.release_transition(old, new, root)
    }
    fn compatible_surface(
        &self,
        old: &Value,
        new: &Value,
        cache: Option<&Qualification>,
    ) -> install::Result<()> {
        FixtureUpdating.compatible_surface(old, new, cache)
    }
}

#[test]
fn command_link_rollback_rechecks_bank_compatibility_before_restoring_python() {
    let world = World::new();
    materialize(&world, "all-copy");
    in_flight_nodes(&world, &["command-link"]);
    let active = install::fingerprint(&world.bin().join("am")).unwrap();
    let updating = Arc::new(RecheckingUpdating(std::sync::atomic::AtomicUsize::new(0)));
    let mut services = services();
    services.updating = Some(updating.clone());
    let result = install::recover_command(&world.env(), &services);
    assert_eq!(result.exit_code, 1, "{}", result.stdout);
    assert!(error_text(result.value.as_ref().unwrap()).contains("Concurrent bank migration"));
    assert_eq!(updating.0.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert!(install::same(
        &install::fingerprint(&world.bin().join("am")).unwrap(),
        &active
    ));
}

#[test]
fn adoption_refuses_an_opencode_hook_link_near_miss_even_when_receipted() {
    let world = World::new();
    let mut receipt = materialize(&world, "claude-opencode-copy");
    let path = world.path("config/opencode/plugins/agent-memory.js");
    std::fs::remove_file(&path).unwrap();
    symlink(
        old_pack(&receipt).join("hooks/opencode-near-miss.js"),
        &path,
    )
    .unwrap();
    if let Some(Value::Array(targets)) = receipt.get_mut("targets") {
        let target = targets
            .iter_mut()
            .find(|target| text(target, "kind") == "hook-link")
            .unwrap();
        set(target, "identity", install::fingerprint(&path).unwrap());
    }
    store_receipt(&world, &receipt);
    let source = development_source(&world, "dev");
    refuses_without_mutation(&world, &source, "Python-era OpenCode hook link");
}

#[test]
fn captured_python_state_with_release_source_is_adopted_by_the_signed_bootstrap() {
    let world = World::new();
    // No genuine release-kind capture exists in the capture corpus. Vary only
    // the channel metadata on a genuine all-copy state capture; owned targets,
    // commands and unchanged activation identities remain captured v0.0.14 data.
    let mut receipt = materialize(&world, "all-copy");
    let source = receipt.get_mut("source").unwrap();
    set(source, "kind", Value::String("release".into()));
    set(
        source,
        "remote",
        Value::String(install::DEFAULT_REMOTE.into()),
    );
    set(source, "ref", Value::String("releases/latest".into()));
    set(
        source,
        "release_manifest_sha256",
        Value::String("a".repeat(64)),
    );
    set(
        source,
        "signature_namespace",
        Value::String(release::NAMESPACE.into()),
    );
    set(
        source,
        "signature_principal",
        Value::String(release::PRINCIPAL.into()),
    );
    store_receipt(&world, &receipt);
    let legacy = snapshot(&world.path("data/agent-memory/source"));
    let root = signed_release(&world, "release", STAND_IN);
    let result = release_ok(&world, &root, &release_request(&root));
    assert_eq!(
        text(get(&result, "receipt"), "schema"),
        install::RECEIPT_SCHEMA
    );
    assert_eq!(
        text(get(get(&result, "receipt"), "source"), "kind"),
        "release"
    );
    assert_eq!(snapshot(&world.path("data/agent-memory/source")), legacy);
}

#[test]
fn python_development_artifact_readback_refuses_a_drifted_link_pack() {
    for status in ["complete", "activating"] {
        let world = World::new();
        let root = development_source(&world, "python development");
        let pack = root.join(install::RELEASE_PACK);
        let link = world.path("codex/skills/agent-memory");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&pack, &link).unwrap();
        let mut receipt = materialize(&world, "claude-opencode-copy");
        let digest = get(&FixtureArtifacts.inspect(&pack).unwrap(), "artifact_digest").clone();
        set(
            &mut receipt,
            "source",
            Value::Object(vec![
                ("kind".into(), Value::String("development".into())),
                ("path".into(), Value::String(root.to_str().unwrap().into())),
                ("pack".into(), Value::String(pack.to_str().unwrap().into())),
            ]),
        );
        set(
            &mut receipt,
            "targets",
            Value::Array(vec![Value::Object(vec![
                ("kind".into(), Value::String("link".into())),
                ("path".into(), Value::String(link.to_str().unwrap().into())),
                ("identity".into(), install::fingerprint(&link).unwrap()),
                ("artifact_digest".into(), digest),
            ])]),
        );
        store_receipt(&world, &receipt);
        let mut journal = decode(&world.state().join("journal.json"));
        set(&mut journal, "status", Value::String(status.into()));
        // Use no stale captured actions: this is a committed recovery readback.
        set(&mut journal, "actions", Value::Array(vec![]));
        write(
            &world.state().join("journal.json"),
            install::encoded(&journal).unwrap(),
        );
        let result =
            install::recover(&world.env(), &services(), &world.state(), false, false).unwrap();
        assert_eq!(text(&result, "status"), "complete");
        set(&mut journal, "status", Value::String(status.into()));
        write(
            &world.state().join("journal.json"),
            install::encoded(&journal).unwrap(),
        );
        write(
            &pack.join("templates/drift.txt"),
            "Drifted unowned development source.\n",
        );
        mode(&pack.join("templates/drift.txt"), 0o644);
        let before = snapshot(&world.root);
        let error =
            install::recover(&world.env(), &services(), &world.state(), false, false).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Client-visible artifact readback failed"),
            "{error}"
        );
        assert_eq!(snapshot(&world.root), before);
    }
}

#[test]
fn repeated_development_bootstrap_does_not_stage_an_ignored_release() {
    let world = World::new();
    let source = development_source(&world, "development");
    let installed = ok(&world, &request(&source));
    let receipt = get(&installed, "receipt").clone();
    let mut services = services();
    services.release.first_manifest_url = "file:///nonexistent/manifest".into();
    let result = install::install_command(
        &world.env(),
        &services,
        &InstallRequest {
            bootstrap_binary: Some(world.path("missing-bootstrap")),
            ..Default::default()
        },
    );
    assert_eq!(result.exit_code, 0, "{}", result.stdout);
    assert_eq!(
        get(result.value.as_ref().unwrap(), "changed"),
        &Value::Bool(false)
    );
    assert!(install::same(
        get(result.value.as_ref().unwrap(), "receipt"),
        &receipt
    ));
}
