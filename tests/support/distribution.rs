use super::lifecycle::{get, mode, text};
use super::{RunOptions, World};
use agent_memory::codec::json::{self, Value};
use agent_memory::installation::{self as install, Environment, ReleaseAccess, Services};
use agent_memory::release_distribution::{self as release, BinaryIdentity, ReleaseManifest};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub struct Channel {
    pub root: PathBuf,
    pub pack: PathBuf,
}
impl Channel {
    pub fn new(world: &World) -> Self {
        let root = world.root.join("signed channel");
        std::fs::create_dir_all(root.join("latest")).unwrap();
        let key = root.join("key");
        world
            .run([
                "/usr/bin/ssh-keygen",
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-f",
                key.to_str().unwrap(),
            ])
            .unwrap();
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
        let pack = root.join("pack");
        super::lifecycle::copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
            &pack,
        );
        Self { root, pack }
    }
    pub fn access(&self) -> ReleaseAccess {
        ReleaseAccess {
            first_manifest_url: file_url(
                &self
                    .root
                    .join("assets")
                    .join(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .join(release::MANIFEST),
            ),
            latest_manifest_url: file_url(&self.root.join("latest").join(release::MANIFEST)),
            assets: file_url(&self.root.join("assets")),
            options: release::DownloadOptions {
                allow_file: true,
                json_response: false,
            },
            signers: std::fs::read_to_string(self.root.join("allowed-signers")).unwrap(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
    fn sign(&self, raw: &[u8], namespace: &str) -> Vec<u8> {
        let argv = [
            OsString::from("/usr/bin/ssh-keygen"),
            "-Y".into(),
            "sign".into(),
            "-n".into(),
            namespace.into(),
            "-f".into(),
            self.root.join("key").into_os_string(),
        ];
        let output =
            release::run_command(&argv, raw, release::MAX_TAR, None, Duration::from_secs(20))
                .unwrap();
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{output:?}"
        );
        output.stdout
    }
    pub fn publish(&self, version: &str, revision: &str) -> ReleaseManifest {
        let skill = self.pack.join("SKILL.md");
        let raw = std::fs::read_to_string(&skill).unwrap();
        let raw = raw
            .lines()
            .map(|line| {
                if line.starts_with("metadata:") {
                    format!("metadata: {{\"version\": \"{version}\"}}")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(skill, raw).unwrap();
        let files = release::portable_files(&self.pack).unwrap();
        let inventory = release::inventory_value(&files);
        let archive = release::encode_archive(
            &files
                .iter()
                .map(|(name, body)| (format!("{}{name}", release::PREFIX), body.clone()))
                .collect(),
        )
        .unwrap();
        let mut binary = std::fs::read(env!("CARGO_BIN_EXE_am")).unwrap();
        if version != env!("CARGO_PKG_VERSION") {
            binary.extend_from_slice(b"\n");
        }
        let sha256 = release::digest(&binary);
        let identity = BinaryIdentity {
            size: binary.len(),
            artifact_digest: agent_memory::artifact::runtime_digest(&inventory, &sha256, 0o755)
                .unwrap()
                .1,
            sha256,
        };
        let manifest = ReleaseManifest {
            version: version.into(),
            tag: format!("v{version}"),
            source_commit: revision.into(),
            archive_size: archive.len(),
            archive_sha256: release::digest(&archive),
            artifact_digest: release::inventory_digest(&inventory).unwrap(),
            targets: release::TARGETS
                .into_iter()
                .map(|target| (target.into(), identity.clone()))
                .collect(),
            command_surface: agent_memory::cli::surface::declaration(),
        };
        let raw = manifest.encoded().unwrap();
        let assets = self.root.join("assets").join(format!("v{version}"));
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join(release::ARCHIVE), archive).unwrap();
        std::fs::write(assets.join(release::MANIFEST), &raw).unwrap();
        std::fs::write(
            assets.join(release::SIGNATURE),
            self.sign(&raw, release::NAMESPACE),
        )
        .unwrap();
        let binary_name = format!("am-{}", install::TARGET);
        std::fs::write(assets.join(&binary_name), &binary).unwrap();
        mode(&assets.join(&binary_name), 0o755);
        std::fs::write(
            assets.join(format!("{binary_name}.sshsig")),
            self.sign(&binary, release::BINARY_NAMESPACE),
        )
        .unwrap();
        std::fs::write(self.root.join("latest").join(release::MANIFEST), raw).unwrap();
        manifest
    }
    pub fn binary(&self, version: &str) -> PathBuf {
        self.root
            .join("assets")
            .join(format!("v{version}"))
            .join(format!("am-{}", install::TARGET))
    }
    pub fn run(&self, world: &World, operation: &str, options: &[&str], expected: i32) -> Value {
        let mut args = vec![operation.to_owned()];
        args.extend(options.iter().map(|value| (*value).to_owned()));
        let request = Value::Object(vec![
            (
                "channel".into(),
                Value::String(self.root.to_string_lossy().into_owned()),
            ),
            (
                "args".into(),
                Value::Array(args.into_iter().map(Value::String).collect()),
            ),
        ]);
        let overrides = [(
            OsString::from("AGENT_MEMORY_FAULT_FANOUT_REQUEST"),
            OsString::from(json::encode(&request, json::DEFAULT).unwrap()),
        )];
        let output = world
            .run_with(
                [
                    std::env::current_exe().unwrap().as_os_str(),
                    std::ffi::OsStr::new("support::distribution::driver_entry"),
                    std::ffi::OsStr::new("--exact"),
                    std::ffi::OsStr::new("--nocapture"),
                ],
                RunOptions {
                    env: &overrides,
                    check: false,
                    timeout: Duration::from_secs(120),
                    ..Default::default()
                },
            )
            .unwrap();
        if expected == 86 {
            assert_eq!(
                output.status,
                agent_memory::process::Status::Code(86),
                "{output:?}"
            );
            return Value::Null;
        }
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{output:?}"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let row = stdout
            .lines()
            .find_map(|line| line.strip_prefix("FANOUT-RESULT:"))
            .unwrap_or_else(|| panic!("{stdout}"));
        let value = json::decode(row, Default::default()).unwrap();
        assert_eq!(
            get(&value, "code"),
            &Value::Int(expected.into()),
            "{value:?}"
        );
        get(&value, "value").clone()
    }
    pub fn install(&self, world: &World, preset: &str, mode_name: &str) -> Value {
        self.run(world, "install", &[preset, mode_name], 0)
    }
    pub fn fault(&self, world: &mut World, operation: &str, stage: &str) {
        world
            .env
            .insert("AGENT_MEMORY_FAULT_STAGE".into(), stage.into());
        world
            .env
            .insert("AGENT_MEMORY_FAULT_MODE".into(), "exit".into());
        self.run(world, operation, &[], 86);
        world
            .env
            .remove(std::ffi::OsStr::new("AGENT_MEMORY_FAULT_STAGE"));
        world
            .env
            .remove(std::ffi::OsStr::new("AGENT_MEMORY_FAULT_MODE"));
    }
    pub fn spawn(
        &self,
        world: &World,
        operation: &str,
        options: &[&str],
        stage: &str,
    ) -> (super::Child, PathBuf) {
        let rendezvous = world.root.join("lifecycle-rendezvous");
        let mut args = vec![operation.to_owned()];
        args.extend(options.iter().map(|value| (*value).to_owned()));
        let request = Value::Object(vec![
            (
                "channel".into(),
                Value::String(self.root.to_string_lossy().into_owned()),
            ),
            (
                "args".into(),
                Value::Array(args.into_iter().map(Value::String).collect()),
            ),
        ]);
        let overrides = [
            (
                OsString::from("AGENT_MEMORY_FAULT_FANOUT_REQUEST"),
                OsString::from(json::encode(&request, json::DEFAULT).unwrap()),
            ),
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
            (
                "AGENT_MEMORY_FAULT_DIR".into(),
                rendezvous.clone().into_os_string(),
            ),
        ];
        let child = world
            .spawn_with(
                [
                    std::env::current_exe().unwrap().as_os_str(),
                    std::ffi::OsStr::new("support::distribution::driver_entry"),
                    std::ffi::OsStr::new("--exact"),
                    std::ffi::OsStr::new("--nocapture"),
                ],
                RunOptions {
                    env: &overrides,
                    check: false,
                    timeout: Duration::from_secs(120),
                    ..Default::default()
                },
            )
            .unwrap();
        let ready = rendezvous.join(format!("{stage}.ready"));
        let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(30)).unwrap();
        while !ready.exists() {
            assert!(
                !deadline.expired(),
                "lifecycle rendezvous not reached: {stage}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        (child, rendezvous.join(format!("{stage}.release")))
    }
    pub fn collect(child: super::Child, expected: i32) -> Value {
        let output = child.wait().unwrap();
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{output:?}"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let row = stdout
            .lines()
            .find_map(|line| line.strip_prefix("FANOUT-RESULT:"))
            .unwrap_or_else(|| panic!("{stdout}"));
        let value = json::decode(row, Default::default()).unwrap();
        assert_eq!(
            get(&value, "code"),
            &Value::Int(expected.into()),
            "{value:?}"
        );
        get(&value, "value").clone()
    }
}

#[test]
fn driver_entry() {
    let Ok(raw) = std::env::var("AGENT_MEMORY_FAULT_FANOUT_REQUEST") else {
        return;
    };
    agent_memory::cli::register_providers().unwrap();
    let request = json::decode(&raw, Default::default()).unwrap();
    let channel = Channel {
        root: PathBuf::from(text(&request, "channel")),
        pack: PathBuf::new(),
    };
    let args = get(&request, "args").as_array().unwrap();
    let operation = args[0].as_str().unwrap();
    let env = Environment::process();
    let mut services = Services::registered().unwrap();
    services.release = channel.access();
    services.updating = Some(Arc::new(agent_memory::updating::Runtime::new(
        env.clone(),
        services.artifacts.clone(),
        services.release.clone(),
    )));
    if matches!(operation, "doctor" | "diagnostic") {
        services.executable = Some(install::locations(&env).unwrap().runtime_binary());
    }
    if operation == "inventory" {
        let compatibility =
            super::lifecycle::read(&channel.root.join("pack/schemas/runtime-compatibility.json"));
        let repositories = args
            .iter()
            .skip(1)
            .map(|value| value.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let outcome = agent_memory::updating::banks(
            &env,
            &channel.root.join("pack"),
            &repositories,
            &compatibility,
            true,
        );
        let (code, value) = match outcome {
            Ok(value) => (0, value),
            Err(error) => (
                1,
                Value::Object(vec![("error".into(), Value::String(error.to_string()))]),
            ),
        };
        let row = Value::Object(vec![
            ("code".into(), Value::Int(code.into())),
            ("value".into(), value),
        ]);
        println!(
            "FANOUT-RESULT:{}",
            json::encode(&row, json::COMPACT_ASCII).unwrap()
        );
        return;
    }
    if operation == "cli" {
        agent_memory::cli::use_development_pack(Some(channel.root.join("pack")));
        let arguments = args
            .iter()
            .skip(1)
            .map(|arg| arg.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let result = agent_memory::cli::dispatch(&arguments, "am").result;
        let value = json::decode(&result.stdout, Default::default()).unwrap();
        let row = Value::Object(vec![
            ("code".into(), Value::Int(result.exit_code.into())),
            ("value".into(), value),
        ]);
        println!(
            "FANOUT-RESULT:{}",
            json::encode(&row, json::COMPACT_ASCII).unwrap()
        );
        return;
    }
    let result = match operation {
        "install" => install::install_command(
            &env,
            &services,
            &install::InstallRequest {
                check: args.iter().any(|arg| arg.as_str() == Some("--check")),
                presets: Some(args[1].as_str().unwrap().into()),
                mode: Some(if args[2].as_str().unwrap() == "copy" {
                    install::Mode::Copy
                } else {
                    install::Mode::Link
                }),
                target_dir: {
                    let targets = args
                        .iter()
                        .filter_map(|arg| {
                            arg.as_str()
                                .and_then(|arg| arg.strip_prefix("target="))
                                .map(PathBuf::from)
                        })
                        .collect::<Vec<_>>();
                    (!targets.is_empty()).then_some(targets)
                },
                bootstrap_binary: Some(channel.binary(env!("CARGO_PKG_VERSION"))),
                ..Default::default()
            },
        ),
        "update" => agent_memory::updating::update_command(
            &env,
            &services,
            &agent_memory::updating::UpdateRequest {
                check: args.iter().any(|arg| arg.as_str() == Some("--check")),
                repo: args
                    .iter()
                    .skip(1)
                    .filter_map(|arg| {
                        arg.as_str()
                            .and_then(|arg| arg.strip_prefix("repo="))
                            .map(str::to_owned)
                    })
                    .collect(),
                layout_quiescence: args
                    .iter()
                    .filter_map(|arg| {
                        arg.as_str()
                            .and_then(|arg| arg.strip_prefix("layout="))
                            .map(PathBuf::from)
                    })
                    .collect(),
                ..Default::default()
            },
        ),
        "recover" => install::recover_command(&env, &services),
        "doctor" => agent_memory::diagnostics::command(
            &env,
            &services,
            &agent_memory::diagnostics::Request {
                operation: "doctor".into(),
                repo: args
                    .get(1)
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap()),
                projection: None,
                fix: true,
            },
        ),
        "diagnostic" => agent_memory::diagnostics::command(
            &env,
            &services,
            &agent_memory::diagnostics::Request {
                operation: args[1].as_str().unwrap().into(),
                repo: args
                    .get(2)
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap()),
                projection: None,
                fix: false,
            },
        ),
        "uninstall" => {
            agent_memory::uninstallation::uninstall_command(&env, &services, &Default::default())
        }
        _ => panic!("unknown lifecycle operation"),
    };
    let value = json::decode(&result.stdout, Default::default()).unwrap();
    let row = Value::Object(vec![
        ("code".into(), Value::Int(result.exit_code.into())),
        ("value".into(), value),
    ]);
    println!(
        "FANOUT-RESULT:{}",
        json::encode(&row, json::COMPACT_ASCII).unwrap()
    );
}

pub fn file_url(path: &Path) -> String {
    use std::fmt::Write;
    let mut url = String::from("file://");
    for byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            url.push(char::from(*byte));
        } else {
            write!(url, "%{byte:02X}").unwrap();
        }
    }
    url
}

impl Channel {
    pub fn run_with_umask(
        &self,
        world: &World,
        operation: &str,
        options: &[&str],
        expected: i32,
        mask: &str,
    ) -> Value {
        assert!(["002", "022"].contains(&mask));
        let request = Value::Object(vec![
            (
                "channel".into(),
                Value::String(self.root.to_str().unwrap().into()),
            ),
            (
                "args".into(),
                Value::Array(
                    std::iter::once(operation)
                        .chain(options.iter().copied())
                        .map(|arg| Value::String(arg.into()))
                        .collect(),
                ),
            ),
        ]);
        let env = [(
            "AGENT_MEMORY_FAULT_FANOUT_REQUEST".into(),
            json::encode(&request, json::DEFAULT).unwrap().into(),
        )];
        let output = world
            .run_with(
                [
                    OsString::from("/bin/sh"),
                    "-c".into(),
                    "umask \"$1\"; shift; exec \"$@\"".into(),
                    "native-mask".into(),
                    mask.into(),
                    std::env::current_exe().unwrap().into_os_string(),
                    "support::distribution::driver_entry".into(),
                    "--exact".into(),
                    "--nocapture".into(),
                ],
                RunOptions {
                    env: &env,
                    check: false,
                    timeout: Duration::from_secs(120),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{output:?}"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let raw = stdout
            .lines()
            .find_map(|line| line.strip_prefix("FANOUT-RESULT:"))
            .unwrap();
        let result = json::decode(raw, Default::default()).unwrap();
        assert_eq!(
            get(&result, "code"),
            &Value::Int(expected.into()),
            "{result:?}"
        );
        get(&result, "value").clone()
    }
}
