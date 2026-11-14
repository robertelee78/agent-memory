//! Observe actual native CLI results in a private, credential-free process environment.
use agent_memory::clock::Deadline;
use agent_memory::codec::json::{self, Value};
use agent_memory::error::ContractError;
use agent_memory::fs::Descriptor;
use agent_memory::process::{self, Input, Spec, Status, Target};
use agent_memory::release_distribution::{self as release, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_OUTPUT: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn strings(values: impl IntoIterator<Item = String>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}
pub fn capture(am: &Path, pack: &Path) -> Result<Value> {
    let scratch = release::Scratch::new("am-guide-examples-")?;
    let root = &scratch.path;
    let directory = Descriptor::open_directory(root)?;
    for name in ["home", "config", "data", "state", "cache", "tmp", "repo"] {
        directory.create_directory(std::ffi::OsStr::new(name), 0o700)?;
    }
    let mut fixture = json::decode_bytes(
        &release::regular(
            &pack.join("templates/router-proposal.example.json"),
            MAX_OUTPUT,
        )?,
        Default::default(),
    )?;
    if let Value::Object(fields) = &mut fixture {
        let (_, project) = fields
            .iter_mut()
            .find(|(key, _)| key == "project-id")
            .ok_or_else(|| ContractError::new("Example proposal has no project-id"))?;
        *project = string(agent_memory::host::uuid4()?);
    } else {
        return Err(ContractError::new("Example proposal must be an object").into());
    }
    let good = root.join("render.json");
    let bad = root.join("invalid.json");
    release::write_new(
        &good,
        json::encode(&fixture, json::DEFAULT)?.as_bytes(),
        0o600,
    )?;
    release::write_new(&bad, b"{}", 0o600)?;
    let mut environment = Vec::new();
    for (key, directory) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
        ("TMPDIR", "tmp"),
    ] {
        environment.push((OsString::from(key), root.join(directory).into_os_string()));
    }
    for (key, value) in [
        ("PATH", "/usr/bin:/bin"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_ALLOW_PROTOCOL", "file"),
        ("LC_ALL", "C"),
        ("TZ", "UTC"),
    ] {
        environment.push((key.into(), value.into()));
    }
    let cases = [
        (
            "useful-router-render",
            vec![
                "render".to_owned(),
                "--input".into(),
                good.to_string_lossy().into_owned(),
            ],
            vec!["render", "--input", "REVIEWED_INPUT.json"],
        ),
        (
            "missing-render-fields",
            vec![
                "render".to_owned(),
                "--input".into(),
                bad.to_string_lossy().into_owned(),
            ],
            vec!["render", "--input", "EMPTY_INPUT.json"],
        ),
        (
            "unknown-operation",
            vec!["unknown-operation".to_owned()],
            vec!["unknown-operation"],
        ),
    ];
    let mut examples = Vec::new();
    for (name, args, display) in cases {
        let argv: Vec<_> = std::iter::once(am.as_os_str().to_owned())
            .chain([OsString::from("--json"), OsString::from("bank")])
            .chain(args.iter().map(OsString::from))
            .collect();
        let cwd = root.join("repo");
        let output = process::run_for(
            &Spec {
                argv: &argv,
                search_path: false,
                env: &environment,
                cwd: Some(&cwd),
                stdin: Input::Null,
                stdout: Target::Capture { limit: MAX_OUTPUT },
                stderr: Target::Capture { limit: MAX_OUTPUT },
                deadline: Deadline::after(TIMEOUT)?,
                new_session: false,
                file_size_limit: None,
                umask: Some(0o077),
                pass_fds: &[],
            },
            TIMEOUT,
        )?;
        let code = match output.status {
            Status::Code(code) => code,
            _ => return Err(ContractError::new("Guide example did not exit normally").into()),
        };
        let mut item = object([
            ("id", string(name)),
            (
                "observed_argv",
                strings(argv.iter().map(|arg| arg.to_string_lossy().into_owned())),
            ),
            (
                "command",
                strings(
                    ["am", "--json", "bank"]
                        .into_iter()
                        .chain(display)
                        .map(str::to_owned),
                ),
            ),
            ("exit", Value::Int(code.into())),
            ("stdout_sha256", string(release::digest(&output.stdout))),
            ("stderr_sha256", string(release::digest(&output.stderr))),
        ]);
        if let Value::Object(fields) = &mut item {
            if !output.stdout.is_empty() {
                let observed = json::decode_bytes(&output.stdout, Default::default())?;
                fields.push((
                    "observed_fields".into(),
                    Value::Object(
                        ["schema", "operation", "valid", "writes_performed", "error"]
                            .into_iter()
                            .filter_map(|key| {
                                observed.get(key).map(|value| (key.into(), value.clone()))
                            })
                            .collect(),
                    ),
                ));
                if name == "useful-router-render" {
                    let mut paths = observed
                        .get("files")
                        .and_then(Value::as_object)
                        .ok_or_else(|| ContractError::new("Render example has no files"))?
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect::<Vec<_>>();
                    paths.sort();
                    fields.push(("observed_file_paths".into(), strings(paths)));
                }
            }
            if !output.stderr.is_empty() {
                fields.push((
                    "observed_stderr".into(),
                    string(
                        std::str::from_utf8(&output.stderr)
                            .map_err(|_| ContractError::new("Example stderr is not UTF-8"))?,
                    ),
                ));
            }
        }
        examples.push(item);
    }
    let inventory = release::portable_files(pack)?;
    let files = Value::Object(
        inventory
            .iter()
            .filter(|(path, _)| !path.split('/').any(|part| part == "__pycache__"))
            .map(|(path, (raw, _))| (path.clone(), string(release::digest(raw))))
            .collect(),
    );
    let binary = release::regular(am, release::MAX_BINARY)?;
    Ok(object([
        ("schema", string("agent-memory/observed-guide-examples-v1")),
        (
            "runtime",
            object([
                ("version", string(env!("CARGO_PKG_VERSION"))),
                ("implementation", string("rust")),
            ]),
        ),
        ("helper_path", string("am bank")),
        ("helper_sha256", string(release::digest(&binary))),
        ("artifact_files", files),
        ("examples", Value::Array(examples)),
        ("live_credentials_required", Value::Bool(false)),
        ("live_configuration_changed", Value::Bool(false)),
        ("runtime_paths_are_disposable", Value::Bool(true)),
    ]))
}
pub fn run(args: &[String]) -> i32 {
    let result = (|| -> Result<Value> {
        let am = if args.is_empty() {
            std::env::current_exe()?
                .parent()
                .ok_or_else(|| ContractError::new("Developer binary has no parent"))?
                .join("am")
        } else if args.len() == 2 && args.first().is_some_and(|arg| arg == "--am") {
            PathBuf::from(
                args.get(1)
                    .ok_or_else(|| ContractError::new("Missing am binary"))?,
            )
        } else {
            return Err(
                ContractError::new("Usage: am-dev capture-guide-examples [--am PATH]").into(),
            );
        };
        capture(
            &am,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        )
    })();
    match result.and_then(|value| Ok(json::encode(&value, json::INDENT2_ASCII)?)) {
        Ok(raw) => {
            println!("{raw}");
            0
        }
        Err(error) => {
            eprintln!("am-dev capture-guide-examples: {error}");
            1
        }
    }
}
