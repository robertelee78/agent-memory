//! CLI lane: parser engine against captured baseline argparse vectors, the
//! console presentation, the command table/surface, and owned-child
//! termination through the real `am` binary.

use agent_memory::cli::parser::{Arg, Parser};
use agent_memory::cli::surface::{self, Declared};
use agent_memory::cli::{self, Entry};
use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::console::{self, Channel, Transcript};
use agent_memory::pack_validation::CommandSurface;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, DirBuilder};
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const AM: &str = env!("CARGO_BIN_EXE_am");
const VECTORS: &str = include_str!("vectors/cli-argparse-v1.json");

struct PrivateDir(PathBuf);

impl PrivateDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = PathBuf::from(format!(
                "/tmp/am-t2-cli-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    let root = fs::canonicalize(&path).unwrap();
                    for child in ["bin", "home", "config"] {
                        DirBuilder::new()
                            .mode(0o700)
                            .create(root.join(child))
                            .unwrap();
                    }
                    return Self(root);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("Private test directory: {error}"),
            }
        }
    }

    /// The binary under an installed (or any) command name.
    fn command(&self, name: &str) -> PathBuf {
        let link = self.0.join("bin").join(name);
        if !link.exists() {
            std::os::unix::fs::symlink(AM, &link).unwrap();
        }
        link
    }

    fn base(&self, name: &str, path: &str) -> Command {
        let mut command = Command::new(self.command(name));
        command
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.0.join("home"))
            .env(
                "AGENT_MEMORY_CONFIG_HOME",
                self.0.join("config/agent-memory"),
            )
            .current_dir(&self.0)
            .stdin(Stdio::null());
        command
    }

    fn run(&self, name: &str, args: &[&str]) -> Output {
        self.base(name, "/usr/bin:/bin")
            .args(args)
            .output()
            .unwrap()
    }

    fn run_with_stdin(&self, name: &str, args: &[&str], input: &[u8]) -> Output {
        let mut child = self
            .base(name, "/usr/bin:/bin")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn decode(text: &str) -> Value {
    json::decode(text, DecodePolicy::default()).unwrap()
}

/// Random UUIDs differ per run; everything else must match byte for byte.
fn normalize_uuids(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        let window = bytes.get(index..index + 36);
        let uuid = window.is_some_and(|w| {
            w.iter().enumerate().all(|(i, b)| match i {
                8 | 13 | 18 | 23 => *b == b'-',
                _ => b.is_ascii_hexdigit(),
            })
        });
        if uuid {
            output.push_str("<UUID>");
            index += 36;
        } else {
            let c = text[index..].chars().next().unwrap();
            output.push(c);
            index += c.len_utf8();
        }
    }
    output
}

fn script_command(script: &str) -> String {
    format!("am {}", script.trim_end_matches(".py").replace('_', "-"))
}

fn declared_command_names() -> Vec<String> {
    surface::declaration()
        .get("commands")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|command| {
            command
                .get("route")
                .unwrap_or(command)
                .get("name")
                .and_then(Value::as_str)
                .unwrap()
                .to_owned()
        })
        .collect()
}

// obligation: tests/contracts/test_public_cli.py::PublicCLIContract.test_default_help_version_flags_and_public_parser_names_are_read_only
#[test]
fn baseline_argparse_vectors_reproduce_exit_status_and_messages() {
    let vectors = decode(VECTORS);
    let rows = vectors.get("rows").and_then(Value::as_array).unwrap();
    assert!(rows.len() >= 24);
    for row in rows {
        let entry = row.get("entry").and_then(Value::as_str).unwrap();
        let argv: Vec<String> = row
            .get("argv")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let mut expected_stdout = row
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned();
        let mut expected_stderr = row
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned();
        let expected_exit = row
            .get("exit")
            .and_then(Value::as_int)
            .unwrap()
            .as_i64()
            .unwrap();
        let mut args: Vec<String> = argv.clone();
        if entry == "script" {
            let script = args.remove(0);
            let command = script_command(&script);
            args.insert(0, command.trim_start_matches("am ").to_owned());
            expected_stdout = expected_stdout.replace(&script, &command);
            expected_stderr = expected_stderr.replace(&script, &command);
        }
        let dir = PrivateDir::new();
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = dir.run("am", &borrowed);
        let root = dir.0.to_str().unwrap();
        let stdout = text(&output.stdout).replace(root, "<ROOT>");
        let label = format!("row {:?} {args:?}", row.get("row"));
        if expected_stderr.starts_with("usage: am [-h]\n") {
            // Root alternatives evolve with the public command declaration;
            // usage framing and the captured refusal protocol remain exact.
            let names = declared_command_names();
            let last_expected = expected_stderr.lines().last().unwrap();
            let error = match last_expected.split_once(" (choose from ") {
                Some((message, _)) => format!("{message} (choose from {})\n", names.join(", ")),
                None => format!("{last_expected}\n"),
            };
            assert_eq!(
                text(&output.stderr),
                format!(
                    "usage: am [-h]\n          {{{}}} ...\n{error}",
                    names.join(",")
                ),
                "{label}"
            );
        } else {
            assert_eq!(text(&output.stderr), expected_stderr, "{label}");
        }
        assert_eq!(output.status.code(), Some(expected_exit as i32), "{label}");
        assert_eq!(
            normalize_uuids(&stdout),
            normalize_uuids(&expected_stdout),
            "{label}"
        );
    }
}

// obligation: tests/contracts/test_public_cli.py::PublicCLIContract.test_default_help_version_flags_and_public_parser_names_are_read_only
#[test]
fn default_help_route_help_and_invalid_option_follow_the_public_contract() {
    let dir = PrivateDir::new();
    for args in [&[][..], &["help"], &["--help"], &["-h"]] {
        let output = dir.run("am", args);
        assert_eq!(output.status.code(), Some(0));
        let stdout = text(&output.stdout);
        assert!(stdout.contains("am update [--check]"));
        assert!(!stdout.contains("installer.py"));
        assert_eq!(stdout, console::help_text("am"));
    }
    for args in [
        &["developer", "--help"][..],
        &["help", "developer"],
        &["developer", "status", "--help"],
        &["scope-policy", "decide", "-h"],
        &["bank", "--help"],
    ] {
        let output = dir.run("am", args);
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        let stdout = text(&output.stdout);
        assert!(stdout.starts_with("usage: am "), "{stdout}");
        assert!(!stdout.contains("am developer developer") && !stdout.contains(".py"));
        assert!(output.stderr.is_empty());
    }
    let invalid = dir.run("am", &["--not-an-option"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(text(&invalid.stderr).contains("am:"));
    assert!(!text(&invalid.stderr).contains("installer.py"));
    let alias = dir.run("agent-memory", &["--help"]);
    assert_eq!(text(&alias.stdout), console::help_text("agent-memory"));
    let unknown = dir.run("am", &["no-such-command"]);
    assert_eq!(unknown.status.code(), Some(2));
    let names = declared_command_names();
    assert_eq!(
        text(&unknown.stderr),
        format!(
            "usage: am [-h]\n          {{{}}} ...\nam: error: argument command: invalid choice: 'no-such-command' (choose from {})\n",
            names.join(","),
            names.join(", ")
        )
    );
}

// obligation: tests/contracts/test_public_cli.py::PublicCLIContract.test_json_is_explicit_and_source_helpers_and_memory_results_remain_structured
#[test]
fn model_commands_are_wired_to_typed_results_and_json_is_only_a_selector() {
    let dir = PrivateDir::new();
    let cases: [(&[&str], agent_memory::lifecycle::CommandResult); 4] = [
        (
            &["scope-policy", "model"],
            agent_memory::scopes::command_model(),
        ),
        (
            &["authority-policy", "model"],
            agent_memory::authority::command_model(),
        ),
        (
            &["record-contract", "model"],
            agent_memory::records::command("model", Ok(None), Ok(None), &[]),
        ),
        (
            &["lifecycle-model", "definition"],
            agent_memory::lifecycle::command("definition", Ok(None)),
        ),
    ];
    for (args, expected) in cases {
        for name in ["am", "agent-memory"] {
            for json in [false, true] {
                let mut argv = args.to_vec();
                if json {
                    argv.insert(0, "--json");
                }
                let output = dir.run(name, &argv);
                assert_eq!(output.status.code(), Some(0), "{argv:?}");
                assert_eq!(text(&output.stdout), expected.stdout, "{argv:?}");
                assert!(output.stderr.is_empty());
            }
        }
    }
    let model = decode(&text(&dir.run("am", &["scope-policy", "model"]).stdout));
    let schema = decode(include_str!(
        "../skills/agent-memory/schemas/scope-policy.json"
    ));
    assert_eq!(model.get("model"), Some(&schema));
    assert_eq!(model.get("writes_performed"), Some(&Value::Bool(false)));
    let guide = dir.run("am", &["lifecycle-model", "guide"]);
    assert_eq!(
        guide.stdout,
        include_bytes!("../skills/agent-memory/model/GUIDE.md").to_vec()
    );
    // `--label=--json` is data, never the selector.
    let labelled = dir.run(
        "am",
        &["developer", "preview", "--create", "--label=--json"],
    );
    assert_eq!(
        labelled.status.code(),
        Some(0),
        "{}",
        text(&labelled.stderr)
    );
    let value = decode(&text(&labelled.stdout));
    let labels = value.get("proposal").and_then(|p| p.get("labels")).unwrap();
    assert_eq!(labels, &Value::Array(vec![Value::String("--json".into())]));
    // After `--` the selector is literal data and argparse refuses it.
    let tail = dir.run("am", &["scope-policy", "model", "--", "--json"]);
    assert_eq!(tail.status.code(), Some(2));
    assert!(
        text(&tail.stderr).ends_with("am scope-policy: error: unrecognized arguments: -- --json\n")
    );
}

#[test]
fn file_and_stdin_inputs_reach_each_module_error_envelope() {
    let dir = PrivateDir::new();
    let missing = dir.run("am", &["scope-policy", "decide", "--input", "missing.json"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(
        text(&missing.stdout),
        "{\"schema\": \"agent-memory/scope-decision-v1\", \"valid\": false, \"writes_performed\": false, \"error\": \"[Errno 2] No such file or directory: 'missing.json'\"}\n"
    );
    let directory = dir.run("am", &["authority-policy", "assess", "--input", "home"]);
    assert_eq!(directory.status.code(), Some(1));
    assert!(text(&directory.stdout).contains("\"error\": \"[Errno 21] Is a directory: 'home'\""));
    let lifecycle = dir.run("am", &["lifecycle-model", "current", "--input", "absent"]);
    assert_eq!(lifecycle.status.code(), Some(1));
    assert_eq!(
        text(&lifecycle.stdout),
        "{\"schema\": \"agent-memory/lifecycle-result-v1\", \"operation\": \"current\", \"error\": \"[Errno 2] No such file or directory: 'absent'\", \"writes_performed\": false}\n"
    );
    let required = dir.run("am", &["lifecycle-model", "transition"]);
    assert_eq!(required.status.code(), Some(1));
    assert!(text(&required.stdout).contains("\"error\": \"--input is required\""));
    let stdin = dir.run_with_stdin("am", &["lifecycle-model", "current", "--input", "-"], b"[]");
    assert_eq!(stdin.status.code(), Some(1));
    assert!(text(&stdin.stdout).contains("list indices must be integers or slices, not str"));
    let duplicate = dir.run_with_stdin(
        "am",
        &["lifecycle-model", "current", "--input", "-"],
        b"{\"a\": 1, \"a\": 2}",
    );
    assert_eq!(duplicate.status.code(), Some(1));
    // Record errors stay generic: malformed deletion markers may hold secrets.
    let record = dir.write("record.md", b"not a record");
    let invalid = dir.run(
        "am",
        &[
            "record-contract",
            "validate",
            "--file",
            record.to_str().unwrap(),
        ],
    );
    assert_eq!(invalid.status.code(), Some(1));
    assert!(text(&invalid.stdout).contains("Invalid record, history event, tombstone"));
    let proposal = dir.run("am", &["developer", "apply", "--proposal", "nope.json"]);
    assert_eq!(proposal.status.code(), Some(1));
    assert!(text(&proposal.stdout).contains("[Errno 2] No such file or directory: 'nope.json'"));
    let writes = dir.run("am", &["writes", "apply", "--proposal", "nope.json"]);
    assert_eq!(writes.status.code(), Some(1));
    let writes = decode(&text(&writes.stdout));
    assert_eq!(writes.get("inspection_required"), Some(&Value::Bool(true)));
}

#[test]
fn pack_default_without_registered_location_is_a_typed_refusal() {
    let dir = PrivateDir::new();
    for (args, prog) in [
        (&["validate-pack"][..], "am validate-pack"),
        (&["artifact", "inspect"], "am artifact"),
    ] {
        let output = dir.run("am", args);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            text(&output.stderr),
            format!(
                "{prog}: No installed Rust runtime receipt owns this executable; pass --pack PATH\n"
            )
        );
    }
    let missing = dir.run("am", &["validate-pack", "--pack", "absent"]);
    assert_eq!(missing.status.code(), Some(1));
    let value = decode(&text(&missing.stdout));
    assert_eq!(value.get("valid"), Some(&Value::Bool(false)));
}

#[test]
fn direct_bootstrap_and_non_utf8_modes() {
    let dir = PrivateDir::new();
    // Not an installed name: the machine mode of the baseline installer.py.
    let direct = dir.run("am-x86_64-unknown-linux-musl", &["--help"]);
    assert_eq!(direct.status.code(), Some(0));
    let stdout = text(&direct.stdout);
    assert!(stdout.starts_with("usage: am [-h]\n"), "{stdout}");
    assert!(stdout.contains("Install a portable runtime"));
    assert!(
        stdout.contains("    developer           Shared memory operation; use developer --help\n")
    );
    let machine = dir.run(
        "am-x86_64-unknown-linux-musl",
        &["scope-policy", "model", "--json"],
    );
    assert_eq!(machine.status.code(), Some(0));
    assert_eq!(
        text(&machine.stdout),
        agent_memory::scopes::command_model().stdout
    );
    // install.sh runs the verified asset as `install --bootstrap-binary PATH …`.
    let bootstrap = dir.run(
        "am-aarch64-apple-darwin",
        &["install", "--bootstrap-binary", "/x/am", "--check"],
    );
    // A missing verified binary refuses before any download.
    assert_eq!(bootstrap.status.code(), Some(1));
    assert!(
        text(&bootstrap.stderr).starts_with(
            "am: Bootstrap binary is unavailable or not an ordinary file; re-run the trusted bootstrap\n"
        ),
        "{}",
        text(&bootstrap.stderr)
    );
    let reconfigure = dir.run(
        "am-aarch64-apple-darwin",
        &[
            "install",
            "--bootstrap-binary",
            "/x/am",
            "--presets",
            "none",
        ],
    );
    assert_eq!(reconfigure.status.code(), Some(1));
    assert!(text(&reconfigure.stderr).contains("Bootstrap binary is unavailable"));
    let output = dir
        .base("am", "/usr/bin:/bin")
        .arg(OsString::from_vec(b"scope-\xffpolicy".to_vec()))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "am: error: Non-UTF-8 argument cannot be recorded; rename it or configure a UTF-8 value\n"
    );
}

#[test]
fn console_selectors_names_and_intent_follow_the_baseline() {
    let strings = |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        console::options(strings(&[
            "-v",
            "status",
            "--json",
            "-v",
            "--",
            "--json",
            "--verbose"
        ])),
        (
            strings(&["-v", "status", "--", "--json", "--verbose"]),
            true,
            true
        )
    );
    assert_eq!(
        cli::arguments(&strings(&["x", "--json", "--label=--json", "--", "--json"])),
        strings(&["x", "--label=--json", "--", "--json"])
    );
    assert_eq!(
        console::installed_name("/usr/local/bin/am".as_ref()),
        Some("am")
    );
    assert_eq!(
        console::installed_name("agent-memory".as_ref()),
        Some("agent-memory")
    );
    assert_eq!(
        console::installed_name("/x/am-aarch64-apple-darwin".as_ref()),
        None
    );
    assert_eq!(
        cli::intent("forget"),
        Some(&["expire", "retention", "delete"][..])
    );
    assert_eq!(cli::intent("correct"), Some(&["correct"][..]));
    assert_eq!(cli::intent("govern"), None);
    assert_eq!(
        console::bootstrap_command(strings(&["install", "--presets", "none"])).unwrap(),
        strings(&["install", "--presets", "none"])
    );
    assert_eq!(
        console::bootstrap_command(strings(&["status"])).unwrap(),
        strings(&["status"])
    );
}

fn rendered(data: &str, command: &str, code: i32) -> (String, String) {
    let mut out = Transcript::default();
    console::render(&decode(data), command, "am", code, &mut out);
    (out.text(Channel::Stdout), out.text(Channel::Stderr))
}

// obligation: tests/contracts/test_public_cli.py::PublicCLIContract.test_refusals_and_preserved_uninstall_are_readable_and_keep_failure_codes
#[test]
fn human_rendering_of_lifecycle_results_is_escaped_and_ordered() {
    let (stdout, _) = rendered(
        r#"{"executing_source": {"version": "1.0.0", "revision": "abc", "dirty": false, "path": "/src"}}"#,
        "version",
        0,
    );
    assert!(stdout.starts_with("am 1.0.0\nrevision "));
    assert!(stdout.contains("(clean)\nsource "));
    assert_eq!(stdout, "am 1.0.0\nrevision abc (clean)\nsource /src\n");
    let (stdout, stderr) = rendered(
        r#"{"error": "bad \u001b[31m path", "repair": "Retry", "failures": [{"detail": "x", "repair": "y"}, "raw"]}"#,
        "install",
        1,
    );
    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        "am: bad \\x1b[31m path\nfailure: x\n  y\nfailure: raw\nRetry\n"
    );
    let (stdout, _) = rendered(
        r#"{"preserved": [{"path": "/b/am"}], "targets": []}"#,
        "uninstall",
        0,
    );
    assert_eq!(
        stdout,
        "Uninstall finished with preserved targets; removal is not complete.\nProject memory and user data preserved.\npreserved /b/am: user changes\n"
    );
    let (stdout, _) = rendered(r#"{"status": "no-pending-transaction"}"#, "recover", 0);
    assert_eq!(stdout, "No pending runtime transaction.\n");
    let (stdout, _) = rendered(
        r#"{"installation": {"state": "installed"}, "repository": "/r", "bank": {"state": "not_located"}, "warnings": [{"detail": {"k": 1}}]}"#,
        "status",
        0,
    );
    assert_eq!(
        stdout,
        "am status\nruntime installed\nrepository /r\nmemory not_located\nwarning: {'k': 1}\n"
    );
    let (stdout, _) = rendered(
        r#"{"check_complete": true, "current": {"version": "1.0.0", "revision": "0123456789abcdef"}}"#,
        "update",
        0,
    );
    assert_eq!(
        stdout,
        "am 1.0.0 is current at 0123456789abcdef.\nPreview only; no installation changes.\n"
    );
}

fn walk(parser: &Parser<'_>, commands: &mut BTreeSet<String>, flags: &mut BTreeSet<String>) {
    for arg in parser.args {
        match arg {
            Arg::Opt(opt) => flags.extend(opt.flags.iter().map(|f| (*f).to_owned())),
            Arg::Pos(pos) if matches!(pos.dest, "operation" | "command") => {
                commands.extend(pos.choices.iter().map(|c| (*c).to_owned()))
            }
            Arg::Pos(_) => {}
        }
    }
    if let Some(subs) = &parser.subcommands {
        for command in subs.commands {
            commands.insert(command.name.to_owned());
            walk(command.parser, commands, flags);
        }
    }
}

// obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_missing_delegated_helper_cannot_be_hidden_behind_the_installed_command
#[test]
fn command_surface_declaration_equals_the_parser_table() {
    let declaration = surface::declaration();
    assert_eq!(declaration, surface::declaration(), "deterministic");
    let declared = Declared::from_value(&declaration).unwrap();
    let encoded = text(&surface::canonical_bytes(&declaration).unwrap());
    assert_eq!(decode(&encoded), {
        // Sorted canonical bytes decode to the same members.
        let mut sorted = declaration.clone();
        fn sort(value: &mut Value) {
            match value {
                Value::Object(pairs) => {
                    pairs.sort_by(|a, b| a.0.cmp(&b.0));
                    pairs.iter_mut().for_each(|(_, v)| sort(v));
                }
                Value::Array(values) => values.iter_mut().for_each(sort),
                _ => {}
            }
        }
        sort(&mut sorted);
        sorted
    });
    // Each helper is named by the native command of its first table route
    // without a fixed prefix (else its first route).
    let routes_of = |module: &'static str| {
        cli::table::ENTRIES
            .iter()
            .filter_map(move |entry| match entry {
                Entry::Route(route) if route.helper.module == module => Some(route),
                _ => None,
            })
    };
    for entry in cli::table::ENTRIES {
        let Entry::Route(route) = entry else { continue };
        let module = route.helper.module;
        let primary = routes_of(module)
            .find(|route| route.prefix.is_empty())
            .or_else(|| routes_of(module).next())
            .unwrap();
        assert_eq!(
            route.helper.command,
            format!("am {}", primary.name),
            "{module}"
        );
    }
    let helper_commands: BTreeSet<&str> = cli::table::ENTRIES
        .iter()
        .filter_map(|entry| match entry {
            Entry::Route(route) => Some(route.helper.command),
            Entry::Native(_) => None,
        })
        .collect();
    let help: BTreeSet<String> = ["-h", "--help"].iter().map(|f| (*f).to_owned()).collect();
    for entry in cli::table::ENTRIES {
        let Entry::Route(route) = entry else { continue };
        let dispatch = declared.dispatch(route.helper.command).unwrap();
        let mut commands = BTreeSet::new();
        let mut flags = help.clone();
        walk(route.helper.parser, &mut commands, &mut flags);
        assert_eq!(dispatch.commands, commands, "{}", route.helper.command);
        assert_eq!(dispatch.flags, flags, "{}", route.helper.command);
        assert_eq!(dispatch.module, route.helper.module);
        assert_eq!(dispatch.routes.len(), route.helper.routes.len());
    }
    let root = declared.dispatch(surface::ROOT_COMMAND).unwrap();
    let names: BTreeSet<String> = cli::table::ENTRIES
        .iter()
        .map(|e| e.name().to_owned())
        .collect();
    assert_eq!(root.commands, names);
    assert_eq!(root.module, surface::ROOT_MODULE);
    for entry in cli::table::ENTRIES {
        if let Entry::Route(route) = entry {
            let declared_route = &root.routes[route.name];
            assert_eq!(declared_route.helper, route.helper.command);
            assert_eq!(declared_route.prefix, route.prefix.to_vec());
        }
    }
    assert_eq!(
        declared.helper_commands().collect::<BTreeSet<_>>(),
        helper_commands
            .iter()
            .copied()
            .chain([surface::ROOT_COMMAND])
            .collect()
    );
    // No declared identifier names a script, module path or interpreter.
    let encoded = text(&surface::canonical_bytes(&declaration).unwrap());
    for forbidden in [".py", "scripts/", "python"] {
        assert!(!encoded.contains(forbidden), "{forbidden}");
    }
    assert!(declared.command("developer").unwrap().is_some());
    assert!(
        declared
            .command("not-a-declared-command")
            .unwrap()
            .is_none()
    );
    for unknown in ["am not-a-declared-helper", "scripts/installer.py"] {
        assert_eq!(declared.dispatch(unknown).unwrap_err().code, "dispatch");
    }
    // A tampered declaration is refused, never partially trusted.
    let mut tampered = declaration.clone();
    if let Value::Object(pairs) = &mut tampered {
        pairs.retain(|(key, _)| key != "helpers");
    }
    assert!(Declared::from_value(&tampered).is_err());
}

fn owned_spec<'a>(argv: &'a [OsString]) -> agent_memory::process::Spec<'a> {
    use agent_memory::process::{Input, Spec, Target};
    Spec {
        argv,
        search_path: false,
        env: &[],
        cwd: None,
        stdin: Input::Null,
        stdout: Target::Capture { limit: 4096 },
        stderr: Target::Capture { limit: 4096 },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(60)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    }
}

// Re-exec worker: installs the handler in an otherwise idle test process, so
// the registry is never shared with parallel tests.
#[test]
fn owned_registry_capacity_worker() {
    use agent_memory::process::{self, Group, Refusal, Status};
    let Some(root) = std::env::var_os("AM_T2_CLI_REGISTRY_WORKER") else {
        return;
    };
    process::install_termination_handler().unwrap();
    let marker = PathBuf::from(root).join("marker");
    let sleep: Vec<OsString> = ["/bin/sleep", "30"].iter().map(OsString::from).collect();
    let touch: Vec<OsString> = vec![
        "/bin/sh".into(),
        "-c".into(),
        ": > \"$0\"".into(),
        marker.clone().into(),
    ];
    // The largest baseline Group (3 children) fills the registry exactly.
    let specs = [owned_spec(&sleep), owned_spec(&sleep), owned_spec(&sleep)];
    let group = Group::launch(&specs, Duration::from_secs(60)).unwrap();
    let refused = process::run(&owned_spec(&touch)).unwrap_err();
    assert_eq!(refused.reason, Refusal::OwnedCapacity);
    assert!(
        refused.evidence.is_none(),
        "no child may exist for a refused spawn"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "an untracked child was launched");
    // Dropping the group kills and reaps its children, releasing every slot.
    drop(group);
    for _ in 0..3 {
        let output = process::run(&owned_spec(&touch)).unwrap();
        assert_eq!(output.status, Status::Code(0));
    }
    assert!(marker.exists());
}

#[test]
fn full_owned_registry_refuses_before_fork_and_frees_on_reap() {
    let dir = PrivateDir::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "owned_registry_capacity_worker",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AM_T2_CLI_REGISTRY_WORKER", &dir.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert!(text(&output.stdout).contains("1 passed"));
}

fn wait_for_pid(path: &Path) -> i32 {
    let start = Instant::now();
    loop {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(pid) = raw.trim().parse() {
                return pid;
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "child never started"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

fn errno_location() -> *mut libc::c_int {
    #[cfg(target_os = "macos")]
    let location = unsafe { libc::__error() };
    #[cfg(target_os = "linux")]
    let location = unsafe { libc::__errno_location() };
    location
}

/// This process can no longer collect `pid`'s status (it was already reaped
/// by this process) and the PID names no process.
fn reaped_by_this_process(pid: i32) -> bool {
    let mut status = 0;
    let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    let wait_errno = std::io::Error::last_os_error().raw_os_error();
    let signalled = unsafe { libc::kill(pid, 0) };
    let kill_errno = std::io::Error::last_os_error().raw_os_error();
    waited == -1
        && wait_errno == Some(libc::ECHILD)
        && signalled == -1
        && kill_errno == Some(libc::ESRCH)
}

/// A fake `git` that records its PID, then sleeps as an owned child.
fn hanging_git(dir: &PrivateDir) -> PathBuf {
    let fake = dir.0.join("fake");
    DirBuilder::new().mode(0o700).create(&fake).unwrap();
    let git = fake.join("git");
    fs::write(
        &git,
        "#!/bin/sh\necho $$ > \"$AM_TEST_CHILD_PID\"\nexec /bin/sleep 30\n",
    )
    .unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
    fake
}

/// `am developer status` blocked on an owned git child, optionally started
/// with `ignored` set to SIG_IGN as a non-interactive shell does for `&` jobs.
fn status_with_owned_child(
    dir: &PrivateDir,
    pid_file: &Path,
    ignored: Option<libc::c_int>,
) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let fake = hanging_git(dir);
    let mut command = dir.base("am", fake.to_str().unwrap());
    command
        .env("AM_TEST_CHILD_PID", pid_file)
        .args(["developer", "status", "--repo", "."])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(signal) = ignored {
        // Only async-signal-safe calls between fork and exec.
        unsafe {
            command.pre_exec(move || {
                if libc::signal(signal, libc::SIG_IGN) == libc::SIG_ERR {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    command.spawn().unwrap()
}

fn terminate_with(signal: libc::c_int) {
    use std::os::unix::process::ExitStatusExt;
    let dir = PrivateDir::new();
    let pid_file = dir.0.join("child.pid");
    // A non-owned sibling in the same process group must survive.
    let mut sentinel = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let mut binary = status_with_owned_child(&dir, &pid_file, None);
    let child = wait_for_pid(&pid_file);
    assert!(
        alive(child),
        "owned child must be running before the signal"
    );
    assert_eq!(unsafe { libc::kill(binary.id() as i32, signal) }, 0);
    let output = binary.wait().unwrap();
    // The binary reaped before exiting: an unreaped zombie would still answer.
    assert!(!alive(child), "owned child survived or was left unreaped");
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    // Death by the signal itself (as the baseline interpreter), not exit 128+n.
    assert_eq!(output.signal(), Some(signal), "{output:?}");
    assert_eq!(output.code(), None);
    let mut stdout = String::new();
    std::io::Read::read_to_string(&mut binary.stdout.take().unwrap(), &mut stdout).unwrap();
    assert!(
        stdout.is_empty(),
        "no further domain output after the signal"
    );
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut binary.stderr.take().unwrap(), &mut stderr).unwrap();
    // SIGINT: the baseline's interrupt-cleanup note, with nothing left unreaped.
    let note = "Owned child cleanup: 0 killed-but-unreaped or cancellation-unproved children handed to process exit; no further signals\n";
    let expected = if signal == libc::SIGINT { note } else { "" };
    assert_eq!(stderr, expected);
    assert!(
        sentinel.try_wait().unwrap().is_none(),
        "non-owned process signalled"
    );
    sentinel.kill().unwrap();
    sentinel.wait().unwrap();
}

#[test]
fn sigterm_kills_and_reaps_owned_children_then_dies_by_sigterm() {
    terminate_with(libc::SIGTERM);
}

#[test]
fn sigint_kills_and_reaps_owned_children_then_dies_by_sigint() {
    terminate_with(libc::SIGINT);
}

#[test]
fn a_termination_signal_inherited_as_ignored_stays_ignored() {
    use std::os::unix::process::ExitStatusExt;
    for (ignored, other) in [(libc::SIGINT, libc::SIGTERM), (libc::SIGTERM, libc::SIGINT)] {
        let dir = PrivateDir::new();
        let pid_file = dir.0.join("child.pid");
        let mut binary = status_with_owned_child(&dir, &pid_file, Some(ignored));
        let child = wait_for_pid(&pid_file);
        assert_eq!(unsafe { libc::kill(binary.id() as i32, ignored) }, 0);
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            binary.try_wait().unwrap().is_none(),
            "am must keep running when signal {ignored} was inherited as ignored"
        );
        assert!(alive(child), "owned child must keep running too");
        // The other signal still has its handler.
        assert_eq!(unsafe { libc::kill(binary.id() as i32, other) }, 0);
        let status = binary.wait().unwrap();
        assert_eq!(status.signal(), Some(other), "{status:?}");
        assert!(!alive(child), "owned child survived or was left unreaped");
    }
}

fn interrupted_status(status: libc::c_int) -> bool {
    libc::WIFSIGNALED(status) && libc::WTERMSIG(status) == libc::SIGINT
}

/// Stops `am developer apply` while its `.developers.lock` exists, delivers
/// SIGINT inside that window and resumes it. The test is the binary's parent
/// and collects its status with waitpid only (never through `Child`).
#[test]
fn sigint_during_developer_apply_releases_lock_and_temp_then_dies_by_sigint() {
    const ATTEMPTS: usize = 400;
    let dir = PrivateDir::new();
    let config = dir.0.join("config/agent-memory");
    let lock = config.join(".developers.lock");
    let mut interrupted_in_window = 0;
    for _ in 0..ATTEMPTS {
        if interrupted_in_window == 3 {
            break;
        }
        let _ = fs::remove_dir_all(&config);
        let preview = dir.run("am", &["developer", "preview", "--create"]);
        assert_eq!(preview.status.code(), Some(0), "{}", text(&preview.stderr));
        let proposal = dir.write("proposal.json", &preview.stdout);
        let mut binary = dir
            .base("am", "/usr/bin:/bin")
            .args(["developer", "apply", "--proposal"])
            .arg(&proposal)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = binary.id() as i32;
        let mut status = 0;
        let mut exited = false;
        while fs::symlink_metadata(&lock).is_err() {
            if unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) } == pid {
                exited = true;
                break;
            }
        }
        if exited {
            continue;
        }
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        assert_eq!(
            unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) },
            pid
        );
        if !libc::WIFSTOPPED(status) {
            continue;
        }
        let in_window = fs::symlink_metadata(&lock).is_ok();
        if in_window {
            assert_eq!(unsafe { libc::kill(pid, libc::SIGINT) }, 0);
        }
        assert_eq!(unsafe { libc::kill(pid, libc::SIGCONT) }, 0);
        assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
        if !in_window {
            continue;
        }
        interrupted_in_window += 1;
        assert!(interrupted_status(status), "wait status {status:#x}");
        let mut stdout = Vec::new();
        std::io::Read::read_to_end(&mut binary.stdout.take().unwrap(), &mut stdout).unwrap();
        assert!(stdout.is_empty(), "no result after SIGINT");
        let leftovers: Vec<String> = fs::read_dir(&config)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".developers"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let next = dir.run("am", &["developer", "preview", "--create"]);
        assert_eq!(next.status.code(), Some(0), "{}", text(&next.stdout));
    }
    assert!(
        interrupted_in_window > 0,
        "no attempt stopped am while it held the registry lock"
    );
}

// Re-exec worker: the termination handler in an otherwise idle test process.
// The supervising test thread is the only one SIGINT is directed at.
#[test]
fn interrupt_handler_worker() {
    use agent_memory::process::{self, Group, Refusal};
    let Some(root) = std::env::var_os("AM_T2_CLI_INTERRUPT_WORKER") else {
        return;
    };
    let blocked = std::env::var_os("AM_T2_CLI_INTERRUPT_MODE").as_deref()
        == Some(std::ffi::OsStr::new("blocked"));
    process::install_termination_handler().unwrap();
    let root = PathBuf::from(root);
    let argv: Vec<Vec<OsString>> = (0..3)
        .map(|index| {
            vec![
                "/bin/sh".into(),
                "-c".into(),
                "echo $$ > \"$0\"; exec /bin/sleep 30".into(),
                root.join(format!("child-{index}")).into(),
            ]
        })
        .collect();
    let specs: Vec<_> = argv.iter().map(|argv| owned_spec(argv)).collect();
    // The largest baseline Group: three live owned children.
    let mut group = Group::launch(&specs, Duration::from_secs(60)).unwrap();
    let pids: Vec<i32> = (0..3)
        .map(|index| wait_for_pid(&root.join(format!("child-{index}"))))
        .collect();
    let mut first = 0;
    if blocked {
        // SIGINT lands while this thread is blocked in the group's poll.
        let target = unsafe { libc::pthread_self() } as usize;
        let signaller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            unsafe { libc::pthread_kill(target as libc::pthread_t, libc::SIGINT) }
        });
        let start = Instant::now();
        let error = group.value(0).unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(10));
        assert_eq!(signaller.join().unwrap(), 0);
        assert_eq!(error.reason, Refusal::Interrupted);
        assert!(error.cleanup_error.is_none(), "{error}");
        assert!(error.evidence.is_some_and(|evidence| evidence.reaped));
        first = 1;
    } else {
        // A returning handler restores the interrupted code's errno.
        unsafe { *errno_location() = libc::EDOM };
        assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
        assert_eq!(unsafe { *errno_location() }, libc::EDOM);
    }
    assert!(process::interrupted());
    // This process, their parent, already collected all three: in idle mode
    // nothing but the handler ran between raise() and this check.
    for pid in &pids {
        assert!(reaped_by_this_process(*pid), "child {pid} not reaped");
    }
    for index in first..3 {
        let error = group.value(index).unwrap_err();
        assert_eq!(error.reason, Refusal::Interrupted);
        assert!(error.cleanup_error.is_none(), "{error}");
    }
    group.finish().unwrap();
    // No further child is launched once interrupted.
    let marker = root.join("marker");
    let touch: Vec<OsString> = vec![
        "/bin/sh".into(),
        "-c".into(),
        ": > \"$0\"".into(),
        marker.clone().into(),
    ];
    let refused = process::run(&owned_spec(&touch)).unwrap_err();
    assert_eq!(refused.reason, Refusal::Interrupted);
    assert!(refused.evidence.is_none());
    std::thread::sleep(Duration::from_millis(200));
    assert!(!marker.exists(), "a child was launched after SIGINT");
}

fn run_interrupt_worker(mode: &str) {
    let dir = PrivateDir::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "interrupt_handler_worker",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AM_T2_CLI_INTERRUPT_WORKER", &dir.0)
        .env("AM_T2_CLI_INTERRUPT_MODE", mode)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert!(text(&output.stdout).contains("1 passed"));
}

#[test]
fn sigint_handler_reaps_every_owned_child_itself_and_restores_errno() {
    run_interrupt_worker("idle");
}

#[test]
fn sigint_while_a_three_child_group_is_supervised_refuses_as_interrupted() {
    run_interrupt_worker("blocked");
}

#[test]
fn content_files_are_read_with_universal_newlines_for_writes_preview_only() {
    let dir = PrivateDir::new();
    let crlf = dir.write("crlf.md", b"---\r\ntype: decision\r\n---\r\nbody\r\n");
    let lone = dir.write("lone.md", b"---\ntype: decision\n---\nbo\rdy\r\r\n");
    let crlf = crlf.to_str().unwrap();
    let lone = lone.to_str().unwrap();
    assert_eq!(
        cli::input::read_text_universal(crlf).unwrap(),
        "---\ntype: decision\n---\nbody\n"
    );
    assert_eq!(
        cli::input::read_text_universal(lone).unwrap(),
        "---\ntype: decision\n---\nbo\ndy\n\n"
    );
    // `read_bytes().decode()` call sites keep the bytes.
    assert_eq!(
        cli::input::read_text(crlf).unwrap(),
        "---\r\ntype: decision\r\n---\r\nbody\r\n"
    );
}

#[test]
fn owned_children_complete_normally_with_the_handler_installed() {
    let dir = PrivateDir::new();
    let fake = dir.0.join("fake");
    DirBuilder::new().mode(0o700).create(&fake).unwrap();
    let git = fake.join("git");
    fs::write(&git, "#!/bin/sh\nprintf 'user.name\\nAda\\0'\n").unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
    for _ in 0..3 {
        let output = dir
            .base("am", fake.to_str().unwrap())
            .args(["developer", "status"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        let value = decode(&text(&output.stdout));
        let observed = value.get("observations").and_then(Value::as_array).unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].get("value"), Some(&Value::String("Ada".into())));
    }
}

/// The binary registers the migration contract, so artifact inspection fails
/// or succeeds on the pack itself, never on a missing provider. Commands whose
/// modules are not yet registered remain explicit pack validation failures.
#[test]
fn artifact_inspection_has_its_migration_provider() {
    let dir = PrivateDir::new();
    let pack = concat!(env!("CARGO_MANIFEST_DIR"), "/skills/agent-memory");
    let output = dir.run("am", &["artifact", "inspect", "--pack", pack]);
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains("\"schema\": \"agent-memory/artifact-inspection-v1\""),
        "{stdout}"
    );
    assert!(!stdout.contains("provider is not registered"), "{stdout}");
}

fn native_guide_command(
    surface: &Declared,
    command: &str,
) -> agent_memory::pack_validation::Result<Value> {
    let dir = PrivateDir::new();
    let pack = dir.0.join("pack");
    DirBuilder::new().mode(0o700).create(&pack).unwrap();
    fs::write(
        pack.join("SKILL.md"),
        format!(
            "---\nname: agent-memory\ndescription: Native command contract\nmetadata: {{\"version\":\"0.0.15\"}}\n---\n\n## Route\n\n```sh\n{command}\n```\n"
        ),
    )
    .unwrap();
    agent_memory::pack_validation::validate_with_surface(&pack, Some(surface))
}

fn declaration_field_mut<'a>(value: &'a mut Value, key: &str) -> &'a mut Value {
    let Value::Object(pairs) = value else {
        panic!("Declaration object");
    };
    &mut pairs.iter_mut().find(|(name, _)| name == key).unwrap().1
}

fn native_guide_surface() -> Declared {
    let mut declaration = surface::declaration();
    let Value::Array(helpers) = declaration_field_mut(&mut declaration, "helpers") else {
        panic!("Declaration helpers");
    };
    let bank = helpers
        .iter_mut()
        .find(|helper| helper.get("command").and_then(Value::as_str) == Some("am bank"))
        .unwrap();
    // A nested route exercises child operation/flag checks independently of
    // the top-level bank parser; both parsers come from the actual table.
    *declaration_field_mut(bank, "routes") =
        decode(r#"[{"name":"render","helper":"am authority-policy","prefix":[]}]"#);
    let Value::Array(commands) = declaration_field_mut(&mut declaration, "commands") else {
        panic!("Declaration commands");
    };
    commands.push(decode(
        r#"{"kind":"route","route":{"name":"status-prefix","helper":"am developer","prefix":["status"]}}"#,
    ));
    Declared::from_value(&declaration).unwrap()
}

#[test]
fn native_guides_resolve_routes_helpers_prefixes_aliases_and_help() {
    let declared = native_guide_surface();
    for command in [
        "am developer status --repo /path/to/repo",
        "am authority-policy model",
        "agent-memory authority-policy assess --input /private/request.json",
        "am status-prefix --repo /path/to/repo --json",
        "am bank render model",
        "am bank render assess --input /private/request.json --json",
        "am developer --help",
        "am bank render --help",
        "am authority-policy -h",
        "am --help",
        "am --json --help",
        "am -h",
    ] {
        let result = native_guide_command(&declared, command);
        assert!(result.is_ok(), "{command}: {result:?}");
    }
}

#[test]
fn native_guides_refuse_unknown_commands_operations_flags_and_python_forms() {
    let declared = native_guide_surface();
    for (command, detail) in [
        (
            "am not-a-declared-command --help",
            "Command am not-a-declared-command is not declared by the command surface",
        ),
        (
            "am authority-policy unknown",
            "Unknown documented operation for am authority-policy",
        ),
        (
            "am authority-policy model --not-a-flag",
            "Unknown documented option for am authority-policy",
        ),
        (
            "am bank render unknown",
            "Unknown documented operation for am bank",
        ),
        (
            "am bank render model --repo REPO",
            "Unknown documented option for am bank",
        ),
        (
            "python3 -I \"$SKILL_DIR/scripts/checkpoint.py\" inspect",
            "Python helper invocations are not supported; document the am command",
        ),
        (
            "/bin/sh \"$SKILL_DIR/hooks/python3.sh\" \"$SKILL_DIR/scripts/checkpoint.py\" inspect",
            "Python helper invocations are not supported; document the am command",
        ),
    ] {
        let issue = native_guide_command(&declared, command).unwrap_err();
        assert_eq!(issue.code, "dispatch", "{command}: {issue:?}");
        assert_eq!(issue.detail, detail, "{command}");
        assert_eq!(issue.path, "SKILL.md", "{command}");
    }
}

#[test]
fn native_surface_resolves_only_commands_in_the_table() {
    let declared = Declared::current().unwrap();
    for entry in cli::table::ENTRIES {
        let (dispatch, prefix) = declared.command(entry.name()).unwrap().unwrap();
        match entry {
            Entry::Route(route) => {
                assert_eq!(prefix, route.prefix);
                assert_eq!(dispatch, declared.dispatch(route.helper.command).unwrap());
            }
            Entry::Native(native) => {
                let mut operations = BTreeSet::new();
                let mut flags = ["-h", "--help"].into_iter().map(str::to_owned).collect();
                walk(native.parser, &mut operations, &mut flags);
                assert!(prefix.is_empty());
                assert_eq!(dispatch.commands, operations);
                assert_eq!(dispatch.flags, flags);
            }
        }
    }
    assert!(
        declared
            .command("not-a-declared-command")
            .unwrap()
            .is_none()
    );
}
