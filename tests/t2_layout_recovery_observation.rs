use agent_memory::codec::json::Value;
use agent_memory::developer_identity::{digest, encoded};
use agent_memory::layout_recovery_observation as raw;
use agent_memory::{bank_observation, cli, host, project_identity};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn n(value: usize) -> Value {
    Value::Int(value.into())
}
fn object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn st<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap()
}
fn run(command: &mut Command, inherited: bool) -> agent_memory::process::Output {
    let argv: Vec<OsString> = std::iter::once(command.get_program().to_owned())
        .chain(command.get_args().map(ToOwned::to_owned))
        .collect();
    let mut environment: BTreeMap<OsString, OsString> = if inherited {
        std::env::vars_os().collect()
    } else {
        BTreeMap::new()
    };
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => {
                environment.insert(key.to_owned(), value.to_owned());
            }
            None => {
                environment.remove(key);
            }
        }
    }
    let env: Vec<_> = environment.into_iter().collect();
    let spec = agent_memory::process::Spec {
        argv: &argv,
        env: &env,
        cwd: command.get_current_dir(),
        search_path: true,
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture {
            limit: 4 * 1024 * 1024,
        },
        stderr: agent_memory::process::Target::Capture {
            limit: 4 * 1024 * 1024,
        },
        deadline: agent_memory::clock::Deadline::from_absolute_ns(u64::MAX),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    agent_memory::process::run_for(&spec, Duration::from_secs(120)).unwrap()
}
fn checked(command: &mut Command) -> Vec<u8> {
    let output = run(command, true);
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("am-layoutobs-{}", host::uuid4().unwrap()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn join(&self, path: &str) -> PathBuf {
        self.0.join(path)
    }
    fn bank(&self) -> PathBuf {
        let path = self.join("bank");
        fs::create_dir(&path).unwrap();
        path
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn capacity(
    error: &agent_memory::error::ContractError,
    dimension: &str,
    limit: usize,
    measured: usize,
    measurement: &str,
) {
    assert_eq!(error.to_string(), "pending-recovery-capacity");
    let expected = object([
        ("dimension", s(dimension)),
        ("limit", n(limit)),
        ("measured", n(measured)),
        ("measurement", s(measurement)),
        ("observation_complete", Value::Bool(false)),
    ]);
    assert_eq!(
        encoded(&raw::capacity_details(error).unwrap()).unwrap(),
        encoded(&expected).unwrap()
    );
}
fn hashes(sources: &[Value]) -> BTreeMap<String, String> {
    sources
        .iter()
        .map(|row| (st(row, "path").to_owned(), st(row, "sha256").to_owned()))
        .collect()
}
fn physical_hashes(bank: &Path) -> BTreeMap<String, String> {
    fs::read_dir(bank)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                digest(Some(&fs::read(path).unwrap())).unwrap(),
            )
        })
        .collect()
}

#[test]
fn raw_paths_and_provenance_hex_are_canonical_and_bounded() {
    for path in [
        "MEMORY.md",
        "nested/file with spaces",
        "é/内容",
        ".agent-memory-transaction.md",
    ] {
        assert_eq!(raw::path_value(path).unwrap(), path);
    }
    for path in [
        "",
        "/file",
        "a/",
        "a//b",
        ".",
        "..",
        "a/./b",
        "a/../b",
        "a\\b",
        "x\n",
        "x\u{7f}",
        "x\u{85}",
        "x\u{2028}",
        "x\u{2029}",
    ] {
        assert_eq!(
            raw::path_value(path).unwrap_err().to_string(),
            "Unsafe raw-source path; preserve the physical bank"
        );
    }
    assert!(raw::path_value(&"x".repeat(4096)).is_ok());
    assert!(raw::path_value(&"x".repeat(4097)).is_err());
    assert!(raw::path_value(&"é".repeat(2049)).is_err());
    assert!(raw::valid_xattrs(&object([])));
    for body in ["", "00ff", &"ab".repeat(16384)] {
        assert!(raw::valid_xattrs(&object([(
            "com.apple.provenance",
            s(body)
        )])));
    }
    for body in ["0", "FF", "a b", "gg", &"00".repeat(16385)] {
        assert!(!raw::valid_xattrs(&object([(
            "com.apple.provenance",
            s(body)
        )])));
    }
    assert!(!raw::valid_xattrs(&object([("foreign", s(""))])));
    assert!(!raw::valid_xattrs(&object([(
        "com.apple.provenance",
        n(1)
    )])));
    assert!(!raw::valid_xattrs(&Value::Null));
}

#[test]
fn raw_read_retains_arbitrary_bytes_exact_stat_and_streamed_hash() {
    let root = Root::new();
    let bank = root.bank();
    fs::create_dir(bank.join("nested")).unwrap();
    let body = b"\0\xff\xfeprivate\r\n";
    fs::write(bank.join("nested/source"), body).unwrap();
    let (read, facts) = raw::read_source(&bank, "nested/source", body.len()).unwrap();
    assert_eq!(read, body);
    let info = fs::metadata(bank.join("nested/source")).unwrap();
    assert_eq!(get(&facts, "dev"), &Value::Int(info.dev().into()));
    assert_eq!(get(&facts, "ino"), &Value::Int(info.ino().into()));
    assert_eq!(get(&facts, "mode"), &Value::Int(info.mode().into()));
    assert_eq!(get(&facts, "uid"), &Value::Int(info.uid().into()));
    assert_eq!(get(&facts, "gid"), &Value::Int(info.gid().into()));
    assert_eq!(get(&facts, "nlink"), &Value::Int(info.nlink().into()));
    assert_eq!(get(&facts, "size"), &Value::Int(info.size().into()));
    assert_eq!(
        get(&facts, "mtime_ns"),
        &Value::Int((info.mtime() as i128 * 1_000_000_000 + info.mtime_nsec() as i128).into())
    );
    assert_eq!(
        get(&facts, "ctime_ns"),
        &Value::Int((info.ctime() as i128 * 1_000_000_000 + info.ctime_nsec() as i128).into())
    );
    assert_eq!(facts.as_object().unwrap().len(), 9);
    let (hash, hash_facts) = raw::read_source_hash(&bank, "nested/source", body.len()).unwrap();
    assert_eq!(Some(hash), digest(Some(body)));
    assert_eq!(facts, hash_facts);
    fs::write(bank.join("empty"), b"").unwrap();
    assert_eq!(raw::read_source(&bank, "empty", 0).unwrap().0, b"");
    assert_eq!(
        raw::read_source_hash(&bank, "empty", 0).unwrap().0,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_raw_file_limit_is_inclusive_and_unsupported_aliases_never_hang_or_mutate
#[test]
fn inclusive_source_limit_and_unsupported_aliases_preserve_every_byte() {
    let root = Root::new();
    let bank = root.bank();
    let path = bank.join("opaque.bin");
    let body = vec![b'x'; raw::MAX_FILE];
    fs::write(&path, &body).unwrap();
    let observed = raw::inventory(&bank).unwrap();
    assert_eq!(get(&observed.sources[0], "byte_count"), &n(raw::MAX_FILE));
    fs::write(&path, vec![b'x'; raw::MAX_FILE + 1]).unwrap();
    capacity(
        &raw::inventory(&bank).unwrap_err(),
        "source-bytes",
        raw::MAX_FILE,
        raw::MAX_FILE + 1,
        "exact",
    );
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        (raw::MAX_FILE + 1) as u64
    );
    fs::remove_file(&path).unwrap();
    fs::write(bank.join("MEMORY.md"), b"original").unwrap();
    symlink(bank.join("MEMORY.md"), &path).unwrap();
    assert_eq!(
        raw::inventory(&bank).unwrap_err().to_string(),
        "Unsupported raw source type/alias; preserve the bank"
    );
    assert!(raw::read_source(&bank, "opaque.bin", raw::MAX_FILE).is_err());
    fs::remove_file(&path).unwrap();
    fs::hard_link(bank.join("MEMORY.md"), &path).unwrap();
    assert!(raw::inventory(&bank).is_err());
    assert_eq!(
        raw::read_source(&bank, "opaque.bin", raw::MAX_FILE)
            .unwrap_err()
            .to_string(),
        "Recovery requires ordinary single-link sources"
    );
    fs::remove_file(&path).unwrap();
    checked(Command::new("/usr/bin/mkfifo").arg(&path));
    assert_eq!(
        raw::read_source(&bank, "opaque.bin", raw::MAX_FILE)
            .unwrap_err()
            .to_string(),
        "Recovery requires ordinary single-link sources"
    );
    assert!(raw::inventory(&bank).is_err());
    assert_eq!(fs::read(bank.join("MEMORY.md")).unwrap(), b"original");
}

// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryInventoryBounds.test_actual_aggregate_boundary_preserves_all_sources_on_overflow
#[test]
fn actual_aggregate_boundary_preserves_all_sources_on_overflow() {
    let root = Root::new();
    let bank = root.bank();
    let mut body = vec![b'x'; raw::MAX_FILE];
    body[..18].copy_from_slice(b"raw-source-canary\n");
    let hash = digest(Some(&body)).unwrap();
    let expected: BTreeMap<_, _> = (0..64)
        .map(|number| {
            let name = format!("{number:02}.bin");
            fs::write(bank.join(&name), &body).unwrap();
            (name, hash.clone())
        })
        .collect();
    let result = raw::inventory(&bank).unwrap();
    assert_eq!(get(&result.totals, "source_bytes"), &n(raw::MAX_BYTES));
    assert_eq!(hashes(&result.sources), expected);
    fs::write(bank.join("overflow.bin"), b"!").unwrap();
    capacity(
        &raw::inventory(&bank).unwrap_err(),
        "aggregate-bytes",
        raw::MAX_BYTES,
        raw::MAX_BYTES + 1,
        "lower-bound",
    );
    let mut expected = expected;
    expected.insert("overflow.bin".into(), digest(Some(b"!")).unwrap());
    assert_eq!(physical_hashes(&bank), expected);
}

// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryInventoryBounds.test_actual_source_count_boundary_does_not_drop_or_change_excess_source
#[test]
fn actual_source_count_boundary_preserves_every_excess_source() {
    let root = Root::new();
    let bank = root.bank();
    let expected: BTreeMap<_, _> = (0..65536)
        .map(|number| {
            let name = format!("{number:05}.bin");
            let body = format!("raw-source-{number}");
            fs::write(bank.join(&name), body.as_bytes()).unwrap();
            (name, digest(Some(body.as_bytes())).unwrap())
        })
        .collect();
    let result = raw::inventory(&bank).unwrap();
    assert_eq!(get(&result.totals, "source_files"), &n(65536));
    assert_eq!(hashes(&result.sources), expected);
    fs::write(bank.join("overflow.bin"), b"!").unwrap();
    capacity(
        &raw::inventory(&bank).unwrap_err(),
        "source-files",
        65536,
        65537,
        "lower-bound",
    );
    let mut expected = expected;
    expected.insert("overflow.bin".into(), digest(Some(b"!")).unwrap());
    assert_eq!(physical_hashes(&bank), expected);
}

// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryInventoryBounds.test_actual_physical_entry_boundary_preserves_every_directory_on_overflow
#[test]
fn actual_physical_entry_boundary_preserves_all_directories_on_overflow() {
    let root = Root::new();
    let bank = root.bank();
    let expected: BTreeSet<_> = (0..131072)
        .map(|number| {
            let name = format!("d{number:06}");
            fs::create_dir(bank.join(&name)).unwrap();
            name
        })
        .collect();
    let result = raw::inventory(&bank).unwrap();
    assert_eq!(get(&result.totals, "physical_entries"), &n(131072));
    assert_eq!(get(&result.totals, "source_files"), &n(0));
    assert!(result.sources.is_empty());
    assert_eq!(
        result
            .physical_entries
            .iter()
            .map(|row| {
                assert_eq!(st(row, "type"), "directory");
                st(row, "path").to_owned()
            })
            .collect::<BTreeSet<_>>(),
        expected
    );
    fs::create_dir(bank.join("overflow")).unwrap();
    capacity(
        &raw::inventory(&bank).unwrap_err(),
        "physical-entries",
        131072,
        131073,
        "lower-bound",
    );
    let mut expected = expected;
    expected.insert("overflow".into());
    assert_eq!(
        fs::read_dir(&bank)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                assert!(path.is_dir());
                assert!(fs::read_dir(&path).unwrap().next().is_none());
                path.file_name().unwrap().to_str().unwrap().to_owned()
            })
            .collect::<BTreeSet<_>>(),
        expected
    );
}

// obligation: tests/contracts/test_layout_recovery.py::LayoutRecoveryContract.test_raw_depth_boundary_and_unknown_marker_precedence
#[test]
fn depth_is_inclusive_and_unknown_owner_precedes_source_disclosure() {
    let root = Root::new();
    let bank = root.bank();
    let mut deepest = bank.clone();
    for _ in 0..63 {
        deepest = deepest.join("d");
        fs::create_dir(&deepest).unwrap();
    }
    fs::write(deepest.join("leaf.bin"), b"opaque private fixture").unwrap();
    assert_eq!(get(&raw::inventory(&bank).unwrap().totals, "depth"), &n(64));
    fs::create_dir(deepest.join("too-deep")).unwrap();
    fs::write(deepest.join("too-deep/leaf.bin"), b"x").unwrap();
    capacity(
        &raw::inventory(&bank).unwrap_err(),
        "depth",
        64,
        65,
        "lower-bound",
    );
    let marker = bank.join(".agent-memory-transaction.md");
    fs::write(&marker, b"unknown-owner").unwrap();
    fs::set_permissions(&marker, fs::Permissions::from_mode(0o600)).unwrap();
    let error = bank_observation::markers(&bank, None, None).unwrap_err();
    assert!(raw::capacity_details(&error).is_none());
    assert_eq!(fs::read(marker).unwrap(), b"unknown-owner");
}

#[test]
fn inventory_retains_exact_private_blobs_ids_and_runtime_classification() {
    let root = Root::new();
    let bank = root.bank();
    fs::create_dir(bank.join("nested")).unwrap();
    fs::write(bank.join("nested/é.bin"), b"\xff\0\r\n").unwrap();
    fs::write(bank.join("MEMORY.md"), b"raw router").unwrap();
    fs::write(bank.join(".agent-memory.lock"), b"not a source").unwrap();
    fs::write(bank.join(".agent-memory-transaction.md"), b"owned marker").unwrap();
    let retained = root.join("private");
    let result = raw::inventory_with_options(
        &bank,
        raw::InventoryOptions {
            retain_directory: Some(&retained),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.physical_entries.len(), 4);
    assert_eq!(result.sources.len(), 2);
    assert_eq!(get(&result.totals, "physical_entries"), &n(4));
    for row in &result.sources {
        assert!(agent_memory::developer_identity::uuid_valid(get(
            row,
            "source_id"
        )));
        let path = retained.join(st(row, "blob"));
        assert_eq!(
            fs::read(&path).unwrap(),
            fs::read(bank.join(st(row, "path"))).unwrap()
        );
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o7777, 0o600);
        assert_eq!(
            fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o7777,
            0o700
        );
    }
    let second = raw::inventory_with_options(
        &bank,
        raw::InventoryOptions {
            prior_sources: Some(&result.sources),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.sources, second.sources);
    assert_eq!(result.physical_entries, second.physical_entries);
    assert_eq!(result.totals, second.totals);
    assert!(
        result
            .physical_entries
            .iter()
            .find(|row| st(row, "path") == ".agent-memory-transaction.md")
            .unwrap()
            .get("runtime")
            .unwrap()
            .as_bool()
            .unwrap()
    );
}

#[test]
fn staging_artifacts_unknown_runtime_portable_aliases_and_ignored_paths() {
    let root = Root::new();
    let bank = root.bank();
    fs::write(bank.join(".agent-memory-layout-foreign"), b"staged").unwrap();
    assert_eq!(
        raw::inventory(&bank).unwrap_err().to_string(),
        "Unknown layout staging artifact requires its known operation recovery"
    );
    let result = raw::inventory_with_options(
        &bank,
        raw::InventoryOptions {
            allow_temporaries: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(result.sources.is_empty());
    let ignored = raw::inventory_with_options(
        &bank,
        raw::InventoryOptions {
            ignored_paths: &[".agent-memory-layout-foreign"],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(ignored.physical_entries.is_empty());
    assert_eq!(get(&ignored.totals, "physical_entries"), &n(1));
    assert_eq!(get(&ignored.totals, "depth"), &n(0));
    fs::remove_file(bank.join(".agent-memory-layout-foreign")).unwrap();
    fs::write(bank.join(".agent-memory-unknown-recovery"), b"foreign").unwrap();
    assert_eq!(
        raw::inventory(&bank).unwrap_err().to_string(),
        "Unknown runtime recovery artifact"
    );
    fs::remove_file(bank.join(".agent-memory-unknown-recovery")).unwrap();
    fs::write(bank.join("Case.md"), b"a").unwrap();
    // Portable case aliases cannot coexist on the default case-insensitive macOS volume;
    // Unicode-normalized aliases are covered by the same foundation contract on every host.
    let error = agent_memory::bank_contract::portable_aliases(["Case.md", "case.md"]).unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(fs::read(bank.join("Case.md")).unwrap(), b"a");
}

#[test]
fn retention_never_publishes_excess_source_and_preserves_partial_staging() {
    let root = Root::new();
    let bank = root.bank();
    fs::write(bank.join("a"), b"first private source").unwrap();
    fs::write(bank.join("z"), vec![b'z'; raw::MAX_FILE + 1]).unwrap();
    let retained = root.join("evidence");
    capacity(
        &raw::inventory_with_options(
            &bank,
            raw::InventoryOptions {
                retain_directory: Some(&retained),
                ..Default::default()
            },
        )
        .unwrap_err(),
        "source-bytes",
        raw::MAX_FILE,
        raw::MAX_FILE + 1,
        "exact",
    );
    let blobs: Vec<_> = fs::read_dir(retained.join("originals"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(blobs.len(), 1);
    assert_eq!(fs::read(&blobs[0]).unwrap(), b"first private source");
    assert!(!retained.join("manifest.json").exists());
    assert_eq!(fs::read(bank.join("a")).unwrap(), b"first private source");
}

#[test]
fn manifest_accounting_matches_literal_nested_encoder_bytes() {
    let envelope = object([
        ("schema", s("fixture")),
        ("identity", object([("text", s("é\n\""))])),
    ]);
    let totals = raw::Totals::default().value();
    let mut budget = raw::ManifestBudget::new(&envelope, &totals).unwrap();
    let mut physical = vec![];
    let mut sources = vec![];
    for (kind, row) in [
        (
            "physical_entries",
            object([("path", s("é/one")), ("xattrs", object([]))]),
        ),
        (
            "physical_entries",
            object([("path", s("two")), ("bytes", n(10))]),
        ),
        (
            "sources",
            object([("path", s("one")), ("source_id", s("fixture"))]),
        ),
    ] {
        budget.add(kind, &row, &totals).unwrap();
        if kind == "physical_entries" {
            physical.push(row);
        } else {
            sources.push(row);
        }
        let mut pairs = envelope.as_object().unwrap().to_vec();
        pairs.extend([
            ("physical_entries".into(), Value::Array(physical.clone())),
            ("sources".into(), Value::Array(sources.clone())),
            ("totals".into(), totals.clone()),
        ]);
        assert_eq!(
            budget.measured(&totals).unwrap(),
            encoded(&Value::Object(pairs)).unwrap().len()
        );
    }
    let row = object([("body", s("x".repeat(raw::MAX_FILE)))]);
    let mut last = 0;
    loop {
        match budget.add("sources", &row, &totals) {
            Ok(()) => {
                last = budget.measured(&totals).unwrap();
            }
            Err(error) => {
                let measured = budget.measured(&totals).unwrap();
                assert!(last <= agent_memory::sharing_limits::MAX_PRIVATE);
                assert!(measured > agent_memory::sharing_limits::MAX_PRIVATE);
                capacity(
                    &error,
                    "encoded-manifest-bytes",
                    agent_memory::sharing_limits::MAX_PRIVATE,
                    measured,
                    "lower-bound",
                );
                break;
            }
        }
    }
}

#[test]
fn ordinary_reads_keep_the_16384_gate_separate_from_raw_recovery() {
    let root = Root::new();
    let bank = root.bank();
    let body = vec![b'x'; 16385];
    fs::write(bank.join("activeContext.md"), &body).unwrap();
    assert!(bank_observation::read_source(&bank, "activeContext.md", 16384).is_err());
    assert_eq!(
        raw::read_source(&bank, "activeContext.md", raw::MAX_FILE)
            .unwrap()
            .0,
        body
    );
    assert_eq!(raw::inventory(&bank).unwrap().sources.len(), 1);
}

#[test]
fn no_follow_parent_bindings_and_project_provider_use_the_real_raw_reader() {
    let root = Root::new();
    let bank = root.bank();
    fs::create_dir(bank.join("nested")).unwrap();
    fs::write(bank.join("nested/file"), b"raw").unwrap();
    symlink(bank.join("nested"), bank.join("redirected")).unwrap();
    assert_eq!(
        raw::read_source(&bank, "redirected/file", raw::MAX_FILE)
            .unwrap_err()
            .to_string(),
        "Raw source binding/type/permissions changed; preserve evidence"
    );
    assert_eq!(raw::read_source(&bank, "nested/file", 3).unwrap().0, b"raw");
    capacity(
        &raw::read_source(&bank, "nested/file", 2).unwrap_err(),
        "source-bytes",
        2,
        3,
        "exact",
    );
    cli::register_providers().unwrap();
}

#[test]
fn malformed_prior_ids_cannot_escape_private_retention() {
    let root = Root::new();
    let bank = root.bank();
    fs::write(bank.join("source"), b"retained").unwrap();
    let prior = [object([
        ("path", s("source")),
        ("source_id", s("../../escape")),
    ])];
    let evidence = root.join("evidence");
    assert_eq!(
        raw::inventory_with_options(
            &bank,
            raw::InventoryOptions {
                prior_sources: Some(&prior),
                retain_directory: Some(&evidence),
                ..Default::default()
            }
        )
        .unwrap_err()
        .to_string(),
        "Invalid prior raw source identifier"
    );
    assert!(!evidence.exists());
    assert!(!root.join("escape.bin").exists());
}

static CHILD: Mutex<()> = Mutex::new(());
fn isolated(name: &str, body: impl FnOnce(&Path)) {
    if std::env::var("AM_LAYOUTOBS_CASE").ok().as_deref() == Some(name) {
        body(Path::new(&std::env::var_os("AM_LAYOUTOBS_ROOT").unwrap()));
        return;
    }
    let _guard = CHILD.lock().unwrap();
    let root = Root::new();
    fs::create_dir(root.join("home")).unwrap();
    let output = run(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--include-ignored", "--nocapture"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/local/bin")
            .env("HOME", root.join("home"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env("AGENT_MEMORY_CONFIG_HOME", root.join("am-config"))
            .env("AGENT_MEMORY_STATE_HOME", root.join("am-state"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("LC_ALL", "C")
            .env("AM_LAYOUTOBS_CASE", name)
            .env("AM_LAYOUTOBS_ROOT", &root.0),
        false,
    );
    assert!(
        output.status == agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    checked(Command::new("git").arg("-C").arg(root).args(args))
}
#[test]
fn git_pins_capture_exact_large_index_and_reject_redirected_bindings() {
    isolated(
        "git_pins_capture_exact_large_index_and_reject_redirected_bindings",
        |world| {
            let repo = world.join("repo");
            fs::create_dir(&repo).unwrap();
            assert!(get(&raw::git_pins(&repo).unwrap(), "index") == &Value::Null);
            git(&repo, &["init", "--initial-branch=main"]);
            git(&repo, &["config", "user.name", "Fixture"]);
            git(&repo, &["config", "user.email", "fixture@example.invalid"]);
            let absent = raw::git_pins(&repo).unwrap();
            assert_eq!(get(&absent, "head"), &Value::Null);
            assert_eq!(get(get(&absent, "index"), "sha256"), &Value::Null);
            fs::write(repo.join("work"), b"committed work\n").unwrap();
            git(&repo, &["add", "work"]);
            git(&repo, &["commit", "-qm", "fixture"]);
            let index = repo.join(".git/index");
            let index_bytes = fs::read(&index).unwrap();
            // Raw index pinning deliberately hashes bytes, not a Git-parsed memory source.
            fs::write(&index, vec![b'i'; raw::MAX_FILE + 123]).unwrap();
            let pins = raw::git_pins(&repo).unwrap();
            assert_eq!(
                get(get(&pins, "index"), "sha256"),
                &s(digest(Some(&vec![b'i'; raw::MAX_FILE + 123])).unwrap())
            );
            assert_eq!(get(&pins, "head").as_str().unwrap().len(), 40);
            assert_eq!(get(&pins, "branch"), &s("refs/heads/main"));
            assert_eq!(st(get(&pins, "index"), "path"), index.to_str().unwrap());
            assert_eq!(
                get(get(get(&pins, "index"), "facts"), "size"),
                &n(raw::MAX_FILE + 123)
            );
            let mut configuration = Command::new("git");
            configuration.arg("-C").arg(&repo).args([
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.untrackedCache=false",
                "config",
                "--null",
                "--list",
            ]);
            // Match baseline authority isolation: even GIT_CONFIG_NOSYSTEM is
            // removed; HOME is already private in this isolated child.
            for (key, _) in
                std::env::vars_os().filter(|(key, _)| key.as_encoded_bytes().starts_with(b"GIT_"))
            {
                configuration.env_remove(key);
            }
            assert_eq!(
                get(&pins, "configuration"),
                &s(digest(Some(&checked(&mut configuration))).unwrap())
            );
            assert_eq!(fs::read(&index).unwrap(), vec![b'i'; raw::MAX_FILE + 123]);
            fs::write(&index, &index_bytes).unwrap();
            fs::rename(&index, repo.join("real-index")).unwrap();
            symlink(repo.join("real-index"), &index).unwrap();
            assert_eq!(
                raw::git_pins(&repo).unwrap_err().to_string(),
                "Original Git index must not follow redirected bindings"
            );
        },
    );
}

#[test]
fn invalid_operation_refuses_before_repository_or_runtime_access() {
    let root = Root::new();
    assert_eq!(
        raw::observe(
            &root.join("missing"),
            "not-an-operation",
            &root.join("runtime"),
            &root.join("binary"),
            raw::ObserveOptions::default()
        )
        .unwrap_err()
        .to_string(),
        "Recovery operation requires a canonical UUIDv4"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_provenance_is_exact_and_foreign_xattrs_acl_and_flags_refuse() {
    let root = Root::new();
    let bank = root.bank();
    let path = bank.join("source");
    fs::write(&path, b"original").unwrap();
    checked(
        Command::new("/usr/bin/xattr")
            .args(["-wx", "com.apple.provenance", "0001aaff"])
            .arg(&path),
    );
    // macOS may retain a kernel-issued provenance value instead of the supplied one.
    // Independently read the exact inode metadata with the platform tool.
    let observed = checked(
        Command::new("/usr/bin/xattr")
            .args(["-px", "com.apple.provenance"])
            .arg(&path),
    );
    let expected: String = String::from_utf8(observed)
        .unwrap()
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    assert_eq!(
        raw::metadata_supported(&path).unwrap(),
        object([("com.apple.provenance", s(expected))])
    );
    checked(
        Command::new("/usr/bin/xattr")
            .args(["-w", "com.example.unsupported", "private"])
            .arg(&path),
    );
    assert_eq!(
        raw::inventory(&bank).unwrap_err().to_string(),
        "pending-metadata-preservation"
    );
    assert_eq!(fs::read(&path).unwrap(), b"original");
    checked(
        Command::new("/usr/bin/xattr")
            .args(["-d", "com.example.unsupported"])
            .arg(&path),
    );
    checked(
        Command::new("/bin/chmod")
            .args(["+a", "everyone deny delete"])
            .arg(&path),
    );
    assert_eq!(
        raw::metadata_supported(&path).unwrap_err().to_string(),
        "pending-metadata-preservation"
    );
    checked(Command::new("/bin/chmod").arg("-N").arg(&path));
    checked(Command::new("/usr/bin/chflags").arg("hidden").arg(&path));
    assert_eq!(
        raw::metadata_supported(&path).unwrap_err().to_string(),
        "pending-metadata-preservation"
    );
    checked(Command::new("/usr/bin/chflags").arg("nohidden").arg(&path));
    assert_eq!(fs::read(path).unwrap(), b"original");
}

#[test]

fn complete_observation_retains_private_evidence_and_rechecks_every_pin() {
    isolated(
        "complete_observation_retains_private_evidence_and_rechecks_every_pin",
        |world| {
            cli::register_providers().unwrap();
            let developer = agent_memory::developer_identity::preview(
                "create",
                None,
                &Value::Array(vec![s("Recovery reviewer")]),
                &Value::Array(vec![]),
            )
            .unwrap();
            agent_memory::developer_identity::apply(&developer).unwrap();
            let repo = world.join("repo");
            fs::create_dir(&repo).unwrap();
            git(&repo, &["init", "--initial-branch=main"]);
            git(&repo, &["config", "user.name", "Fixture"]);
            git(&repo, &["config", "user.email", "fixture@example.invalid"]);
            let project =
                project_identity::preview(&repo, "create", None, "Explicit recovery fixture", None)
                    .unwrap();
            project_identity::apply(&project).unwrap();
            let request = object([
                ("project-id", get(&project, "project_id").clone()),
                ("router-updated", s("2026-10-22T18:00:00Z")),
                (
                    "contexts",
                    object([(
                        "projectbrief.md",
                        object([
                            ("body", s("# Project\n\nRecovery fixture context.\n")),
                            ("purpose", s("Goals")),
                            ("scope", s("repository")),
                            ("lifecycle", s("active")),
                            ("read-when", s("When starting")),
                        ]),
                    )]),
                ),
                (
                    "omitted",
                    object([
                        ("productContext.md", s("No supplied evidence")),
                        ("activeContext.md", s("No supplied evidence")),
                        ("systemPatterns.md", s("No supplied evidence")),
                        ("techContext.md", s("No supplied evidence")),
                        ("progress.md", s("No supplied evidence")),
                    ]),
                ),
            ]);
            let bank = repo.join("memory-bank");
            fs::create_dir(&bank).unwrap();
            let rendered = agent_memory::bank_contract::render(&request).unwrap();
            for (path, body) in get(&rendered, "files").as_object().unwrap() {
                fs::write(bank.join(path), body.as_str().unwrap()).unwrap();
            }
            fs::write(bank.join("opaque.bin"), b"\xff\0private source\r\n").unwrap();
            fs::write(bank.join("activeContext.md"), vec![b'x'; 79054]).unwrap();
            git(&repo, &["add", "."]);
            git(&repo, &["commit", "-qm", "original evidence"]);
            let original_index = fs::read(repo.join(".git/index")).unwrap();
            let original_sources = physical_hashes(&bank);
            let operation = host::uuid4().unwrap();
            let runtime = Path::new(env!("CARGO_MANIFEST_DIR"));
            let manifest = raw::observe(
                &repo,
                &operation,
                runtime,
                Path::new(env!("CARGO_BIN_EXE_am")),
                raw::ObserveOptions::default(),
            )
            .unwrap();
            assert_eq!(get(&manifest, "observation_complete"), &Value::Bool(true));
            assert_eq!(st(&manifest, "metadata_policy"), raw::METADATA_POLICY);
            let sources = get(&manifest, "sources").as_array().unwrap();
            assert_eq!(hashes(sources), original_sources);
            let directory =
                agent_memory::layout_recovery_state::paths(&repo, Some(&operation), runtime)
                    .unwrap()
                    .3
                    .unwrap();
            let staging = agent_memory::layout_recovery_state::read(
                &directory.join("staging.json"),
                agent_memory::sharing_limits::MAX_PRIVATE,
                false,
            )
            .unwrap()
            .unwrap();
            assert_eq!(get(&staging, "observation_complete"), &Value::Bool(false));
            for source in sources {
                assert_eq!(
                    fs::read(directory.join(st(source, "blob"))).unwrap(),
                    fs::read(bank.join(st(source, "path"))).unwrap()
                );
            }
            assert!(raw::recheck(&repo, &manifest, None, None).unwrap());
            // Persisted sorted-key manifests retain the same object semantics.
            let persisted =
                agent_memory::developer_identity::load(&encoded(&manifest).unwrap()).unwrap();
            assert!(raw::recheck(&repo, &persisted, None, None).unwrap());
            let mut changed = manifest.clone();
            *changed.get_mut("observation_complete").unwrap() = Value::Bool(false);
            assert_eq!(
                raw::recheck(&repo, &changed, None, None)
                    .unwrap_err()
                    .to_string(),
                "Recovery manifest location/completeness changed"
            );
            fs::write(bank.join("opaque.bin"), b"changed source").unwrap();
            assert_eq!(
                raw::recheck(&repo, &manifest, None, None)
                    .unwrap_err()
                    .to_string(),
                "Recovery source/metadata/identity/Git pins changed"
            );
            assert_eq!(fs::read(repo.join(".git/index")).unwrap(), original_index);
            assert!(!directory.join("manifest.json").exists());
        },
    );
}
