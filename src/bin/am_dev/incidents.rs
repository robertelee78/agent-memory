//! Run retained incidents through their obligation-bound native contract tests.
use agent_memory::clock::Deadline;
use agent_memory::codec::json::{self, Value};
use agent_memory::error::ContractError;
use agent_memory::fs::{self, DirectoryChain};
use agent_memory::process::{self, Input, Spec, Status, Target};
use agent_memory::release_distribution::{self as release, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
const MAX_SOURCE: usize = 2 * 1024 * 1024;
const MAX_TEST_FILES: usize = 4096;
const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(300);
fn error(detail: &str) -> release::Error {
    ContractError::new(detail).into()
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| error("Incomplete incident catalog"))
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?
        .as_str()
        .ok_or_else(|| error("Invalid incident catalog text"))
}
fn number(value: usize) -> Value {
    Value::Int(value.into())
}
fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn native_test(root: &Path, obligation: &str) -> Result<(String, String)> {
    let directory = DirectoryChain::open(&root.join("tests"))?;
    let names = directory
        .directory()
        .entries_bounded(MAX_TEST_FILES)?
        .ok_or_else(|| error("Test source inventory exceeds its bound"))?;
    let mut found = Vec::new();
    for name in names {
        let path = Path::new(&name);
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let raw = release::regular(&root.join("tests").join(path), MAX_SOURCE)?;
        let source =
            std::str::from_utf8(&raw).map_err(|_| error("Native test source is not UTF-8"))?;
        let annotation = format!("// obligation: {obligation}");
        let mut selected = false;
        for line in source.lines() {
            let line = line.trim();
            if line == annotation {
                selected = true;
            } else if selected {
                if let Some(rest) = line.strip_prefix("fn ") {
                    let function = rest
                        .split_once('(')
                        .map(|(name, _)| name)
                        .ok_or_else(|| error("Invalid native test declaration"))?;
                    if function.is_empty()
                        || !function
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    {
                        return Err(error("Invalid native test name"));
                    }
                    let target = path
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .ok_or_else(|| error("Invalid native test target"))?;
                    found.push((target.to_owned(), function.to_owned()));
                    selected = false;
                } else if !line.is_empty() && !line.starts_with("//") && !line.starts_with("#[") {
                    return Err(error("Obligation is not directly bound to a native test"));
                }
            }
        }
    }
    directory.recheck()?;
    if found.len() != 1 {
        return Err(error(
            "Incident must bind to exactly one native contract test",
        ));
    }
    found
        .pop()
        .ok_or_else(|| error("Missing native incident test"))
}
fn test_binary(target: &str) -> Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let root = executable
        .parent()
        .ok_or_else(|| error("Developer binary has no parent"))?
        .join("deps");
    let directory = DirectoryChain::open(&root)?;
    let entries = directory
        .directory()
        .entries_bounded(MAX_TEST_FILES)?
        .ok_or_else(|| error("Native test binary inventory exceeds its bound"))?;
    let prefix = format!("{target}-");
    let mut candidates = Vec::new();
    for name in entries {
        let Some(name_text) = name.to_str() else {
            continue;
        };
        let Some(hash) = name_text.strip_prefix(&prefix) else {
            continue;
        };
        if hash.len() != 16 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let identity = directory.directory().stat_at(&name)?;
        if identity.is_regular() && identity.mode & 0o111 != 0 {
            candidates.push(root.join(name));
        }
    }
    directory.recheck()?;
    if candidates.len() != 1 {
        return Err(error(
            "Build the incident's native test target into this am-dev profile; preserve exactly one executable per target",
        ));
    }
    candidates
        .pop()
        .ok_or_else(|| error("Missing native test executable"))
}
fn run(root: &Path, id: &str) -> Result<Value> {
    let catalog = json::decode_bytes(
        &release::regular(&root.join("tests/incidents.json"), MAX_SOURCE)?,
        Default::default(),
    )?;
    let entries = field(&catalog, "incidents")?
        .as_array()
        .ok_or_else(|| error("Invalid incident catalog"))?;
    let entries = entries
        .iter()
        .filter(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err(error(
            "Select one known incident ID from tests/incidents.json",
        ));
    }
    let item = entries.first().ok_or_else(|| error("Missing incident"))?;
    let cases = field(item, "tests")?
        .as_array()
        .ok_or_else(|| error("Invalid incident tests"))?;
    if cases.is_empty() {
        return Err(error("Incident must contain actual tests"));
    }
    let scratch = release::Scratch::new("am-incident-")?;
    let private = &scratch.path;
    let directory = fs::Descriptor::open_directory(private)?;
    for name in ["home", "config", "data", "state", "cache", "tmp"] {
        directory.create_directory(std::ffi::OsStr::new(name), 0o700)?;
    }
    let mut environment = Vec::new();
    for (key, name) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
        ("TMPDIR", "tmp"),
    ] {
        environment.push((OsString::from(key), private.join(name).into_os_string()));
    }
    environment.push((
        "PATH".into(),
        std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
    ));
    for (key, value) in [
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_ALLOW_PROTOCOL", "file"),
        ("LC_ALL", "C"),
        ("TZ", "UTC"),
    ] {
        environment.push((key.into(), value.into()));
    }
    let mut tests = 0;
    let mut failures = 0;
    let mut errors = 0;
    let mut skipped = 0;
    for case in cases {
        let name = case
            .as_str()
            .ok_or_else(|| error("Invalid incident selector"))?;
        let obligation = format!("{}::{}.{name}", text(item, "suite")?, text(item, "class")?);
        let (target, function) = native_test(root, &obligation)?;
        let binary = test_binary(&target)?;
        let argv: Vec<OsString> = [
            binary.as_os_str().to_owned(),
            function.into(),
            "--exact".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ]
        .into();
        let output = process::run_for(
            &Spec {
                argv: &argv,
                search_path: false,
                env: &environment,
                cwd: Some(root),
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
        let stdout = std::str::from_utf8(&output.stdout)
            .map_err(|_| error("Native incident output is not UTF-8"))?;
        eprint!("{}{}", stdout, String::from_utf8_lossy(&output.stderr));
        let mut observed = None;
        for line in stdout
            .lines()
            .filter(|line| line.starts_with("test result:"))
        {
            let words: Vec<_> = line.split_whitespace().collect();
            let count = |label: &str| -> Option<usize> {
                words
                    .windows(2)
                    .find(|pair| {
                        pair.get(1)
                            .is_some_and(|word| word.trim_end_matches(';') == label)
                    })
                    .and_then(|pair| pair.first())
                    .and_then(|value| value.parse().ok())
            };
            if let (Some(passed), Some(failed), Some(ignored)) =
                (count("passed"), count("failed"), count("ignored"))
            {
                observed = Some((passed, failed, ignored));
            }
        }
        match observed {
            Some((passed, failed, ignored)) if passed + failed + ignored == 1 => {
                tests += 1;
                failures += failed;
                skipped += ignored;
                if output.status != Status::Code(0) && failed == 0 {
                    errors += 1;
                }
            }
            _ => errors += 1,
        }
    }
    let passed = tests == cases.len() && failures == 0 && errors == 0 && skipped == 0;
    Ok(object([
        (
            "schema",
            Value::String("agent-memory/incident-result-v1".into()),
        ),
        ("incident", Value::String(id.into())),
        ("work_key", field(item, "work_key")?.clone()),
        ("passed", Value::Bool(passed)),
        ("tests", number(tests)),
        ("errors", number(errors)),
        ("failures", number(failures)),
        ("skipped", number(skipped)),
        ("expected_failures", number(0)),
        ("unexpected_successes", number(0)),
        ("live_credentials_required", Value::Bool(false)),
        ("live_configuration_changed", Value::Bool(false)),
    ]))
}
pub fn entry(args: &[String]) -> i32 {
    let result = if args.len() == 1 {
        run(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            args.first().map(String::as_str).unwrap_or(""),
        )
    } else {
        Err(error("Usage: am-dev run-incident INCIDENT"))
    };
    match result {
        Ok(value) => match json::encode(&value, json::SORTED_ASCII) {
            Ok(raw) => {
                println!("{raw}");
                if value.get("passed").and_then(Value::as_bool) == Some(true) {
                    0
                } else {
                    1
                }
            }
            Err(error) => {
                eprintln!("am-dev run-incident: {error}");
                1
            }
        },
        Err(error) => {
            eprintln!("am-dev run-incident: {error}");
            1
        }
    }
}
