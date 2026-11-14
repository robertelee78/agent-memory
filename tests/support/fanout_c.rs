//! Baseline layout fault matrix; all journeys use real filesystem state and owned children.
use super::*;
use std::os::unix::fs::MetadataExt;

fn directory(f: &Fixture, observed: &Value) -> PathBuf {
    r::operation(&f.repo, t(observed, "operation_id"), &f.runtime)
        .unwrap()
        .directory
}
fn saved_originals(f: &Fixture, observed: &Value) -> BTreeMap<String, Vec<u8>> {
    let dir = directory(f, observed);
    g(g(observed, "manifest"), "sources")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                t(row, "path").to_owned(),
                fs::read(dir.join(t(row, "blob"))).unwrap(),
            )
        })
        .collect()
}
fn modes(bank: &Path) -> BTreeMap<String, u32> {
    snapshot(bank)
        .keys()
        .map(|path| {
            (
                path.clone(),
                fs::metadata(bank.join(path)).unwrap().mode() & 0o7777,
            )
        })
        .collect()
}
fn native_bytes(f: &Fixture) -> BTreeMap<String, Vec<u8>> {
    f.native
        .as_object()
        .unwrap()
        .iter()
        .map(|(p, b)| (p.clone(), b.as_str().unwrap().as_bytes().to_vec()))
        .collect()
}
struct Work {
    index: Vec<u8>,
    head: String,
}
fn work(w: &World, f: &Fixture) -> Work {
    fs::write(f.repo.join("feature.txt"), b"Committed feature\n").unwrap();
    w.git(&f.repo, &["add", "feature.txt"]);
    w.git(&f.repo, &["commit", "-m", "Synthetic feature baseline"]);
    fs::write(f.repo.join("feature.txt"), b"Staged feature\n").unwrap();
    w.git(&f.repo, &["add", "feature.txt"]);
    fs::write(f.repo.join("feature.txt"), b"Unstaged feature\n").unwrap();
    fs::write(f.repo.join("untracked.txt"), b"Untracked feature\n").unwrap();
    Work {
        index: fs::read(f.repo.join(".git/index")).unwrap(),
        head: w.git(&f.repo, &["rev-parse", "HEAD"]),
    }
}
fn unchanged(w: &World, f: &Fixture, work: &Work) {
    assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), work.index);
    assert_eq!(w.git(&f.repo, &["rev-parse", "HEAD"]), work.head);
    assert_eq!(
        fs::read(f.repo.join("feature.txt")).unwrap(),
        b"Unstaged feature\n"
    );
    assert_eq!(
        fs::read(f.repo.join("untracked.txt")).unwrap(),
        b"Untracked feature\n"
    );
}
fn fault(w: &World, f: &Fixture, op: &str, proposal: Option<&Value>, stage: &str, mode: &str) {
    let case = std::env::var("AM_LAYOUT_CASE").unwrap();
    let mut extra = vec![
        ("AM_LAYOUT_CASE", case.clone().into()),
        ("AM_LAYOUT_ROOT", w.root.clone().into_os_string()),
        ("AM_LAYOUT_ACTION", op.into()),
        ("AM_LAYOUT_REPO", f.repo.clone().into_os_string()),
        ("AM_LAYOUT_RUNTIME", f.runtime.clone().into_os_string()),
        (
            "AM_LAYOUT_OUTPUT",
            w.root
                .join(format!("fault-output-{}.json", uuid()))
                .into_os_string(),
        ),
        ("AGENT_MEMORY_TEST_ROOT", w.root.clone().into_os_string()),
        ("AGENT_MEMORY_TEST_TOKEN", "layout-contract".into()),
        ("AGENT_MEMORY_FAULT_STAGE", stage.into()),
        ("AGENT_MEMORY_FAULT_MODE", mode.into()),
    ];
    if let Some(p) = proposal {
        extra.push((
            "AM_LAYOUT_PROPOSAL",
            w.save(p, &format!("fault-{}.json", uuid()))
                .into_os_string(),
        ));
    }
    if op == "retention" {
        extra.push(("AM_LAYOUT_STAGE", "apply".into()));
    }
    let result = w.command(
        &[
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            case.into(),
            "--include-ignored".into(),
            "--nocapture".into(),
        ],
        &extra,
    );
    assert_eq!(
        result.status,
        agent_memory::process::Status::Code(if mode == "exit" { 86 } else { 1 }),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    if mode == "exit" {
        let result = w.command(
            &[
                env!("CARGO_BIN_EXE_am").into(),
                "writes".into(),
                "recover".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
            ],
            &[],
        );
        assert_eq!(
            result.status,
            agent_memory::process::Status::Code(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let preview = agent_memory::developer_identity::load(&result.stdout).unwrap();
        if preview.get("proposal").is_some_and(|v| v != &Value::Null) {
            let path = w.save(&preview, &format!("unlock-{}.json", uuid()));
            let result = w.command(
                &[
                    env!("CARGO_BIN_EXE_am").into(),
                    "writes".into(),
                    "recover".into(),
                    "--repo".into(),
                    f.repo.clone().into_os_string(),
                    "--proposal".into(),
                    path.into_os_string(),
                ],
                &[],
            );
            assert_eq!(result.status, agent_memory::process::Status::Code(0));
        }
    }
}
fn finish(f: &Fixture, id: &str, mode: &str) -> (Value, Value) {
    let preview = r::recover(&f.repo, Some(id), Some(mode), None, &f.runtime).unwrap();
    let result = r::recover(&f.repo, None, None, Some(&preview), &f.runtime).unwrap();
    assert_eq!(g(&result, "verified"), &Value::Bool(true));
    let replay = r::recover(&f.repo, None, None, Some(&preview), &f.runtime).unwrap();
    assert_eq!(g(&replay, "receipt_is_historical"), &Value::Bool(true));
    (preview, result)
}
fn apply_fault(
    w: &World,
    stage: &str,
    mode: &str,
    recovery_mode: &str,
    terminal: bool,
    hold_swap: bool,
) {
    let f = fixture(w, !terminal);
    let work = work(w, &f);
    let original_modes = modes(&f.bank);
    let (observed, proposal) = prepared(&f);
    if hold_swap {
        fault(
            w,
            &f,
            "apply",
            Some(&proposal),
            "layout-recovery-after-receipt",
            "error",
        );
        let preview = r::recover(
            &f.repo,
            Some(t(&observed, "operation_id")),
            Some("finish"),
            None,
            &f.runtime,
        )
        .unwrap();
        fault(w, &f, "recover", Some(&preview), stage, mode);
        assert!(f.bank.join(".agent-memory-transaction.md").is_file());
    } else {
        fault(w, &f, "apply", Some(&proposal), stage, mode);
    }
    assert_eq!(saved_originals(&f, &observed), f.originals);
    unchanged(w, &f, &work);
    let outcome = if stage == "layout-recovery-before-journal" {
        assert!(!directory(&f, &observed).join("journal.json").exists());
        assert_eq!(snapshot(&f.bank), f.originals);
        assert_eq!(modes(&f.bank), original_modes);
        let outcome = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
        assert_eq!(
            g(
                &r::apply(&f.repo, &proposal, &f.runtime).unwrap(),
                "receipt_is_historical"
            ),
            &Value::Bool(true)
        );
        outcome
    } else {
        finish(&f, t(&observed, "operation_id"), recovery_mode).1
    };
    assert_eq!(g(&outcome, "verified"), &Value::Bool(true));
    assert_eq!(
        g(&outcome, "semantic_recovery_complete"),
        &Value::Bool(terminal)
    );
    let wanted = if recovery_mode == "finish" {
        native_bytes(&f)
    } else {
        f.originals.clone()
    };
    assert_eq!(snapshot(&f.bank), wanted);
    for path in wanted.keys() {
        assert_eq!(
            fs::metadata(f.bank.join(path)).unwrap().mode() & 0o7777,
            original_modes[path]
        );
    }
    assert_eq!(saved_originals(&f, &observed), f.originals);
    assert_eq!(
        f.bank.join(".agent-memory-transaction.md").exists(),
        !terminal
    );
    assert_eq!(f.bank.join(".agent-memory-recovery.md").exists(), !terminal);
    if !terminal {
        assert!(
            g(&outcome, "pending_units")
                .as_int()
                .unwrap()
                .as_i128()
                .unwrap()
                > 0
        );
    }
    unchanged(w, &f, &work);
}
fn inspect_fault(w: &World, stage: &str, mode: &str, partial: bool) {
    let f = fixture(w, true);
    let work = work(w, &f);
    let before_modes = modes(&f.bank);
    fault(w, &f, "inspect", None, stage, mode);
    let state = w
        .root
        .join("state/agent-memory/writes")
        .join(sha256_hex(f.bank.to_str().unwrap().as_bytes()))
        .join("layout-recovery");
    let dirs: Vec<_> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(dirs.len(), 1);
    let dir = &dirs[0];
    let retained = snapshot(dir);
    if partial {
        let staging =
            agent_memory::developer_identity::load(&fs::read(dir.join("staging.json")).unwrap())
                .unwrap();
        assert_eq!(g(&staging, "observation_complete"), &Value::Bool(false));
        assert!(!dir.join("manifest.json").exists());
        let blobs: Vec<_> = fs::read_dir(dir.join("originals")).unwrap().collect();
        assert_eq!(blobs.len(), 1);
        let status = r::status(
            &f.repo,
            Some(dir.file_name().unwrap().to_str().unwrap()),
            &f.runtime,
        )
        .unwrap();
        assert_eq!(t(&status, "state"), "observation-incomplete");
    } else {
        assert!(dir.join("manifest.json").is_file());
    }
    let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
    assert_ne!(directory(&f, &observed), *dir);
    assert_eq!(saved_originals(&f, &observed), f.originals);
    assert_eq!(snapshot(dir), retained);
    assert_eq!(snapshot(&f.bank), f.originals);
    assert_eq!(modes(&f.bank), before_modes);
    unchanged(w, &f, &work);
}
fn restore_fault(w: &World, stage: &str, mode: &str) {
    let f = fixture(w, true);
    let work = work(w, &f);
    let original_modes = modes(&f.bank);
    let (observed, proposal) = prepared(&f);
    r::apply(&f.repo, &proposal, &f.runtime).unwrap();
    let parent = directory(&f, &observed);
    let receipt = fs::read(parent.join("receipt.json")).unwrap();
    let child =
        agent_memory::layout_restore::preview(&f.repo, t(&observed, "operation_id"), &f.runtime)
            .unwrap();
    let child_id = t(g(&child, "proposal"), "operation_id").to_owned();
    fault(w, &f, "rollback", Some(&child), stage, mode);
    let dir = r::operation(&f.repo, &child_id, &f.runtime)
        .unwrap()
        .directory;
    let journal = fs::read(dir.join("journal.json")).unwrap();
    finish(&f, &child_id, "finish");
    assert_eq!(fs::read(dir.join("journal.json")).unwrap(), journal);
    assert_eq!(fs::read(parent.join("receipt.json")).unwrap(), receipt);
    assert_eq!(snapshot(&f.bank), f.originals);
    assert_eq!(modes(&f.bank), original_modes);
    assert_eq!(saved_originals(&f, &observed), f.originals);
    unchanged(w, &f, &work);
}
fn retention_fault(w: &World, stage: &str, mode: &str) {
    let f = fixture(w, false);
    let work = work(w, &f);
    let (observed, proposal) = prepared(&f);
    r::apply(&f.repo, &proposal, &f.runtime).unwrap();
    let dir = directory(&f, &observed);
    let receipt = fs::read(dir.join("receipt.json")).unwrap();
    let bank = snapshot(&f.bank);
    let bank_modes = modes(&f.bank);
    let reviewed = cleanup_review(
        &f,
        t(&observed, "operation_id"),
        g(g(&proposal, "proposal"), "author"),
    );
    let mut ordered: Vec<_> = g(g(&reviewed, "proposal"), "delete")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    ordered.sort();
    assert!(!ordered.is_empty());
    let before: BTreeMap<_, _> = ordered
        .iter()
        .map(|p| (p.clone(), fs::read(dir.join(p)).unwrap()))
        .collect();
    fault(w, &f, "retention", Some(&reviewed), stage, mode);
    let deleted = if stage.ends_with("retention-intent") {
        0
    } else if stage.ends_with("retention-after-progress") {
        1
    } else if stage.contains("retention-after-delete-") {
        stage.rsplit('-').next().unwrap().parse::<usize>().unwrap() + 1
    } else {
        ordered.len()
    };
    assert!(deleted <= ordered.len());
    for (i, path) in ordered.iter().enumerate() {
        if i < deleted {
            assert!(!dir.join(path).exists());
        } else {
            assert_eq!(fs::read(dir.join(path)).unwrap(), before[path]);
        }
    }
    retention::apply(&f.repo, &reviewed, &f.runtime).unwrap();
    assert_eq!(
        g(
            &retention::apply(&f.repo, &reviewed, &f.runtime).unwrap(),
            "receipt_is_historical"
        ),
        &Value::Bool(true)
    );
    assert_eq!(
        g(
            &r::apply(&f.repo, &proposal, &f.runtime).unwrap(),
            "receipt_is_historical"
        ),
        &Value::Bool(true)
    );
    assert!(ordered.iter().all(|p| !dir.join(p).exists()));
    assert_eq!(fs::read(dir.join("receipt.json")).unwrap(), receipt);
    assert_eq!(snapshot(&f.bank), bank);
    assert_eq!(modes(&f.bank), bank_modes);
    unchanged(w, &f, &work);
}

fn subcase(key: &str, value: &str) {
    let root = Root::new();
    fs::create_dir(root.join("home")).unwrap();
    private(
        &root.join(".test-world.json"),
        b"{\"token\":\"layout-contract\"}",
    );
    let w = World {
        root: root.0.clone(),
    };
    let name = std::env::var("AM_LAYOUT_CASE").unwrap();
    let result = w.command(
        &[
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            name.clone().into(),
            "--include-ignored".into(),
            "--nocapture".into(),
        ],
        &[
            ("AM_LAYOUT_CASE", name.into()),
            ("AM_LAYOUT_ROOT", root.0.clone().into_os_string()),
            (key, value.into()),
        ],
    );
    assert_eq!(
        result.status,
        agent_memory::process::Status::Code(0),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
fn retention_partition(w: &World, partition: usize, mode: &str) {
    if let Ok(sequence) = std::env::var("AM_RETENTION_SEQUENCE") {
        retention_fault(
            w,
            &format!("layout-recovery-retention-after-delete-{sequence}"),
            mode,
        );
        return;
    }
    let f = fixture(w, false);
    let (observed, proposal) = prepared(&f);
    r::apply(&f.repo, &proposal, &f.runtime).unwrap();
    let reviewed = cleanup_review(
        &f,
        t(&observed, "operation_id"),
        g(g(&proposal, "proposal"), "author"),
    );
    let count = g(g(&reviewed, "proposal"), "delete")
        .as_array()
        .unwrap()
        .len();
    let sequences: Vec<_> = (0..count)
        .filter(|n| n % 4 == partition && !(*n == 0 && mode == "error"))
        .collect();
    assert!(!sequences.is_empty());
    for sequence in sequences {
        subcase("AM_RETENTION_SEQUENCE", &sequence.to_string());
    }
}

fn metadata_snapshot(root: &Path) -> BTreeMap<String, Value> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<String, Value>) {
        let m = fs::symlink_metadata(path).unwrap();
        let d = if m.is_dir() {
            agent_memory::fs::Descriptor::open_directory(path).unwrap()
        } else {
            agent_memory::fs::Descriptor::open_directory(path.parent().unwrap())
                .unwrap()
                .open_at(
                    path.file_name().unwrap(),
                    libc::O_RDONLY | libc::O_NOFOLLOW,
                    0,
                )
                .unwrap()
        };
        let attrs = agent_memory::fs::metadata::list_xattrs(&d)
            .unwrap()
            .iter()
            .map(|name| {
                let bytes = agent_memory::fs::metadata::read_xattr(&d, name).unwrap();
                (
                    String::from_utf8(name.clone()).unwrap(),
                    s(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()),
                )
            })
            .collect();
        out.insert(
            path.strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
            obj([
                ("uid", n(m.uid() as usize)),
                ("gid", n(m.gid() as usize)),
                ("mode", n((m.mode() & 0o7777) as usize)),
                (
                    "flags",
                    n(agent_memory::fs::metadata::inode_flags(&d).unwrap() as usize),
                ),
                ("xattrs", Value::Object(attrs)),
            ]),
        );
        if m.is_dir() {
            for e in fs::read_dir(path).unwrap() {
                visit(root, &e.unwrap().path(), out);
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn move_prepared(f: &mut Fixture) -> (Value, Value, String, String) {
    let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
    let mut req = request(f, &observed);
    let old = f
        .native
        .as_object()
        .unwrap()
        .iter()
        .find(|(p, _)| p.starts_with("topics/"))
        .unwrap()
        .0
        .clone();
    let new = format!(
        "topics/probe-nested/{}",
        Path::new(&old).file_name().unwrap().to_str().unwrap()
    );
    let body = g(&f.native, &old).clone();
    if let Value::Object(rows) = &mut f.native {
        rows.retain(|(p, _)| p != &old);
        rows.push((new.clone(), body));
    }
    let router = t(&f.native, "MEMORY.md").replace(&old, &new);
    set(&mut f.native, "MEMORY.md", s(router));
    set(&mut req, "intended_files", f.native.clone());
    for unit in req
        .get_mut("unit_map")
        .unwrap()
        .get_mut("units")
        .unwrap()
        .as_array_mut()
        .unwrap()
    {
        for target in unit.get_mut("targets").unwrap().as_array_mut().unwrap() {
            if t(target, "path") == old {
                set(target, "path", s(&new));
            }
            if [new.as_str(), "MEMORY.md"].contains(&t(target, "path")) {
                set(
                    target,
                    "sha256",
                    s(sha256_hex(t(&f.native, t(target, "path")).as_bytes())),
                );
            }
        }
    }
    let plan = req.get_mut("metadata_plan").unwrap();
    let facts = g(g(plan, "files"), &old).clone();
    if let Value::Object(files) = plan.get_mut("files").unwrap() {
        files.retain(|(p, _)| p != &old);
        files.push((new.clone(), facts));
    }
    let parent = metadata_snapshot(&f.bank).get("topics").unwrap().clone();
    let mut facts = parent.as_object().unwrap().to_vec();
    facts.retain(|(k, _)| k != "flags");
    if let Value::Object(dirs) = plan.get_mut("directories").unwrap() {
        dirs.push(("topics/probe-nested".into(), Value::Object(facts)));
    }
    for (key, field) in [
        ("unit_map", "unit_map_sha256"),
        ("intended_files", "intended_files_sha256"),
        ("metadata_plan", "metadata_plan_sha256"),
    ] {
        let hash = sha(g(&req, key));
        set(req.get_mut("review").unwrap(), field, s(hash));
    }
    let proposal = r::prepare(&f.repo, t(&observed, "operation_id"), &req, &f.runtime).unwrap();
    (observed, proposal, old, new)
}
fn read_json(path: &Path) -> Value {
    agent_memory::developer_identity::load(&fs::read(path).unwrap()).unwrap()
}
fn all_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(path).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(
                    p.strip_prefix(root).unwrap().to_str().unwrap().to_owned(),
                    fs::read(p).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn probe_templates(f: &Fixture, observed: &Value) -> Vec<(Value, bool)> {
    let journal = read_json(&directory(f, observed).join("journal.json"));
    let mut nodes = BTreeMap::new();
    for task in g(g(&journal, "metadata_probes"), "tasks")
        .as_array()
        .unwrap()
    {
        for route in g(task, "routes").as_array().unwrap() {
            let route = route.as_array().unwrap();
            assert!(!route.is_empty());
            let reachable = f.bank.join(t(&route[0], "path")).parent().unwrap().is_dir();
            for node in route {
                let path = t(node, "path");
                assert!(!Path::new(path).is_absolute());
                assert!(
                    !Path::new(path)
                        .components()
                        .any(|c| c == std::path::Component::ParentDir)
                );
                if let Some((old, _)) = nodes.insert(path.to_owned(), (node.clone(), reachable)) {
                    assert_eq!(old, *node);
                }
            }
        }
    }
    let mut rows: Vec<_> = nodes.into_values().collect();
    rows.sort_by_key(|(node, _)| {
        (
            Path::new(t(node, "path")).components().count(),
            t(node, "path").to_owned(),
        )
    });
    rows
}
fn attempts(dir: &Path) -> Vec<PathBuf> {
    let p = dir.join("metadata-probe-attempts");
    if !p.exists() {
        return vec![];
    }
    fs::read_dir(p)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect()
}
fn probe_finish(w: &World, f: &Fixture, observed: &Value, work: &Work, probes: &[String]) {
    let (_, outcome) = finish(f, t(observed, "operation_id"), "finish");
    assert_eq!(
        g(&outcome, "semantic_recovery_complete"),
        &Value::Bool(false)
    );
    assert_eq!(snapshot(&f.bank), native_bytes(f));
    assert_eq!(saved_originals(f, observed), f.originals);
    assert!(probes.iter().all(|p| !f.bank.join(p).exists()));
    unchanged(w, f, work);
}
fn probe_journey(w: &World, stage: &str, mode: &str) {
    let mut f = fixture(w, true);
    let work = work(w, &f);
    let before = metadata_snapshot(&f.bank);
    let (observed, proposal, _, _) = move_prepared(&mut f);
    fault(w, &f, "apply", Some(&proposal), stage, mode);
    assert!(directory(&f, &observed).join("journal.json").is_file());
    assert_eq!(snapshot(&f.bank), f.originals);
    assert_eq!(metadata_snapshot(&f.bank), before);
    if stage.ends_with("probe-directories-removed") {
        assert!(!f.bank.join("topics/probe-nested").exists());
    }
    probe_finish(w, &f, &observed, &work, &[]);
    let actual = metadata_snapshot(&f.bank);
    for (path, expected) in g(g(&proposal, "proposal"), "metadata_plan")
        .get("files")
        .unwrap()
        .as_object()
        .unwrap()
    {
        for key in ["uid", "gid", "mode", "xattrs"] {
            assert_eq!(g(actual.get(path).unwrap(), key), g(expected, key));
        }
    }
}
fn probe_partition(w: &World, kind: &str, partition: usize, mode: &str, late: bool) {
    let mut f = fixture(w, true);
    let work = work(w, &f);
    let (observed, proposal, old, new) = move_prepared(&mut f);
    fault(
        w,
        &f,
        "apply",
        Some(&proposal),
        "layout-recovery-probe-journal",
        "error",
    );
    let templates = probe_templates(&f, &observed);
    let mut candidates: Vec<_> = templates
        .iter()
        .enumerate()
        .filter(|(_, (_, reachable))| *reachable != late)
        .filter(|(_, (node, _))| !late || kind == "created" || t(node, "type") == "file")
        .map(|(i, (node, _))| {
            let role = if t(node, "target") == old {
                "original-record"
            } else if t(node, "target") == new {
                "moved-record"
            } else {
                t(node, "target")
            };
            let shadow = Path::new(t(node, "path"))
                .parent()
                .unwrap()
                .components()
                .any(|p| {
                    p.as_os_str()
                        .to_str()
                        .unwrap()
                        .starts_with(".agent-memory-probe-")
                });
            ((t(node, "type").to_owned(), role.to_owned(), shadow), i)
        })
        .collect();
    candidates.sort();
    assert!(!candidates.is_empty());
    let selected = std::env::var("AM_PROBE_INDEX")
        .ok()
        .map(|v| v.parse::<usize>().unwrap());
    if selected.is_none() {
        for i in (partition..candidates.len()).step_by(if late { 1 } else { 4 }) {
            subcase("AM_PROBE_INDEX", &i.to_string());
        }
        return;
    }
    let (_, index) = &candidates[selected.unwrap()];
    let preview = if late {
        fault(
            w,
            &f,
            "apply",
            Some(&proposal),
            &format!("layout-recovery-before-replace:{new}"),
            "error",
        );
        assert!(f.bank.join("topics/probe-nested").is_dir());
        assert!(!f.bank.join(&new).exists());
        Some(
            r::recover(
                &f.repo,
                Some(t(&observed, "operation_id")),
                Some("finish"),
                None,
                &f.runtime,
            )
            .unwrap(),
        )
    } else {
        None
    };
    let before = snapshot(&f.bank);
    let dir = directory(&f, &observed);
    let previous = attempts(&dir);
    fault(
        w,
        &f,
        if late { "recover" } else { "apply" },
        Some(preview.as_ref().unwrap_or(&proposal)),
        &format!(
            "layout-recovery-probe-{}:{index}",
            if kind == "created" {
                "after-pin"
            } else {
                "removed"
            }
        ),
        mode,
    );
    let fresh: Vec<_> = attempts(&dir)
        .into_iter()
        .filter(|p| !previous.contains(p))
        .collect();
    assert_eq!(fresh.len(), 1);
    let plan = read_json(&fresh[0].join("plan.json"));
    let nodes = g(&plan, "nodes").as_array().unwrap();
    let node = nodes
        .iter()
        .find(|r| g(r, "index").as_int().unwrap().as_i128().unwrap() == *index as i128)
        .unwrap();
    assert_eq!(t(node, "template"), t(&templates[*index].0, "path"));
    if kind == "created" && mode == "exit" {
        assert!(f.bank.join(t(node, "path")).exists());
    }
    if kind == "removed" {
        assert!(!f.bank.join(t(node, "path")).exists());
    }
    assert_eq!(snapshot(&f.bank), before);
    assert_eq!(f.bank.join(".agent-memory-transaction.md").exists(), late);
    probe_finish(
        w,
        &f,
        &observed,
        &work,
        &nodes
            .iter()
            .map(|n| t(n, "path").to_owned())
            .collect::<Vec<_>>(),
    );
}

fn damaged_evidence(w: &World, evidence: &str) {
    let f = fixture(w, true);
    let work = work(w, &f);
    let (observed, proposal) = prepared(&f);
    fault(
        w,
        &f,
        "apply",
        Some(&proposal),
        "layout-recovery-after-marker",
        "error",
    );
    let dir = directory(&f, &observed);
    let path = if evidence == "original" {
        dir.join(t(
            &g(g(&observed, "manifest"), "sources").as_array().unwrap()[0],
            "blob",
        ))
    } else {
        dir.join("reviews")
            .join(format!("{}.json", t(g(&proposal, "proposal"), "review_id")))
    };
    let bytes = fs::read(&path).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions();
    let before = all_bytes(&f.bank);
    for missing in [false, true] {
        if missing {
            fs::remove_file(&path).unwrap();
        } else {
            let mut changed = bytes.clone();
            changed.extend_from_slice(b"changed");
            fs::write(&path, &changed).unwrap();
        }
        assert!(
            r::recover(
                &f.repo,
                Some(t(&observed, "operation_id")),
                Some("finish"),
                None,
                &f.runtime
            )
            .is_err()
        );
        if missing {
            assert!(!path.exists());
        } else {
            assert_eq!(
                fs::read(&path).unwrap(),
                [bytes.as_slice(), b"changed"].concat()
            );
        }
        assert_eq!(all_bytes(&f.bank), before);
        unchanged(w, &f, &work);
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, mode.clone()).unwrap();
    }
    finish(&f, t(&observed, "operation_id"), "finish");
    assert_eq!(saved_originals(&f, &observed), f.originals);
    assert_eq!(snapshot(&f.bank), native_bytes(&f));
    unchanged(w, &f, &work);
}
fn external_change(w: &World, metadata: bool, validate: bool) {
    let f = fixture(w, true);
    let work = work(w, &f);
    let (observed, proposal) = prepared(&f);
    fault(
        w,
        &f,
        "apply",
        Some(&proposal),
        "layout-recovery-after-rename:activeContext.md",
        "exit",
    );
    let before = all_bytes(&f.bank);
    if validate {
        let out = w.command(
            &[
                env!("CARGO_BIN_EXE_am").into(),
                "bank".into(),
                "validate".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
            ],
            &[],
        );
        assert_ne!(out.status, agent_memory::process::Status::Code(0));
        assert_eq!(all_bytes(&f.bank), before);
    }
    let path = f.bank.join(if metadata {
        "projectbrief.md"
    } else {
        "foreign-occupant.md"
    });
    if metadata {
        let bits = fs::metadata(&path).unwrap().mode() & 0o7777;
        fs::set_permissions(&path, fs::Permissions::from_mode(bits ^ 0o100)).unwrap();
    } else {
        fs::write(&path, b"Independent external file stays intact.\n").unwrap();
    }
    let before = all_bytes(&f.bank);
    let meta = metadata_snapshot(&f.bank);
    let journal = fs::read(directory(&f, &observed).join("journal.json")).unwrap();
    for mode in ["finish", "rollback"] {
        assert!(
            r::recover(
                &f.repo,
                Some(t(&observed, "operation_id")),
                Some(mode),
                None,
                &f.runtime
            )
            .is_err()
        );
        assert_eq!(all_bytes(&f.bank), before);
        assert_eq!(metadata_snapshot(&f.bank), meta);
        assert_eq!(
            fs::read(directory(&f, &observed).join("journal.json")).unwrap(),
            journal
        );
        unchanged(w, &f, &work);
    }
    assert_eq!(saved_originals(&f, &observed), f.originals);
    if validate {
        fs::remove_file(path).unwrap();
        assert!(
            r::recover(
                &f.repo,
                Some(t(&observed, "operation_id")),
                Some("finish"),
                None,
                &f.runtime
            )
            .is_ok()
        );
    }
}
fn unsafe_original(w: &World) {
    if std::env::var("AM_UNSAFE_ORIGINAL").is_err() {
        for kind in ["secret", "oversized"] {
            subcase("AM_UNSAFE_ORIGINAL", kind);
        }
        return;
    }
    let f = fixture(w, true);
    let work = work(w, &f);
    let (observed, proposal) = prepared(&f);
    let dir = directory(&f, &observed);
    let row = g(g(&observed, "manifest"), "sources")
        .as_array()
        .unwrap()
        .iter()
        .find(|r| t(r, "path") == "handoff-prompt.md")
        .unwrap();
    let secret = b"password=PRIVATE_FRONTIER_SECRET_987654321\n";
    fs::write(
        dir.join(t(row, "blob")),
        if std::env::var("AM_UNSAFE_ORIGINAL").unwrap() == "secret" {
            secret.to_vec()
        } else {
            vec![b'x'; 16385]
        },
    )
    .unwrap();
    let bank = all_bytes(&f.bank);
    let private = all_bytes(&dir);
    let error = r::apply(&f.repo, &proposal, &f.runtime).unwrap_err();
    assert!(
        !error
            .to_string()
            .contains("PRIVATE_FRONTIER_SECRET_987654321")
    );
    assert_eq!(all_bytes(&f.bank), bank);
    assert_eq!(all_bytes(&dir), private);
    unchanged(w, &f, &work);
}

fn probe_ambiguity(w: &World, node_type: &str, foreign: &str, mode: &str) {
    let mut f = fixture(w, true);
    let work = work(w, &f);
    let (observed, proposal, _, _) = move_prepared(&mut f);
    fault(
        w,
        &f,
        "apply",
        Some(&proposal),
        "layout-recovery-probe-journal",
        "error",
    );
    let templates = probe_templates(&f, &observed);
    let index = templates
        .iter()
        .position(|(node, reachable)| *reachable && t(node, "type") == node_type)
        .unwrap();
    let dir = directory(&f, &observed);
    let previous = attempts(&dir);
    fault(
        w,
        &f,
        "apply",
        Some(&proposal),
        &format!(
            "layout-recovery-probe-{}:{index}",
            if foreign.is_empty() {
                "before-pin"
            } else {
                "after-pin"
            }
        ),
        mode,
    );
    let fresh: Vec<_> = attempts(&dir)
        .into_iter()
        .filter(|p| !previous.contains(p))
        .collect();
    assert_eq!(fresh.len(), 1);
    let attempt = &fresh[0];
    let plan = read_json(&attempt.join("plan.json"));
    let node = g(&plan, "nodes")
        .as_array()
        .unwrap()
        .iter()
        .find(|r| g(r, "index").as_int().unwrap().as_i128().unwrap() == index as i128)
        .unwrap();
    let target = f.bank.join(t(node, "path"));
    let pin = attempt.join("pins").join(format!(
        "{}.json",
        target.file_name().unwrap().to_str().unwrap()
    ));
    assert_eq!(pin.exists(), !foreign.is_empty());
    let paths = g(&plan, "nodes")
        .as_array()
        .unwrap()
        .iter()
        .map(|r| t(r, "path").to_owned())
        .collect::<Vec<_>>();
    if mode == "error" {
        assert!(!target.exists());
        probe_finish(w, &f, &observed, &work, &paths);
        return;
    }
    assert!(target.exists());
    match foreign {
        "inode" => {
            let inode = fs::metadata(&target).unwrap().ino();
            fs::rename(&target, w.root.join("independently-inspected-probe")).unwrap();
            fs::write(
                &target,
                b"Foreign fixture occupant; preserve this exact file.\n",
            )
            .unwrap();
            assert_ne!(fs::metadata(&target).unwrap().ino(), inode);
        }
        "nonce" => {
            let inode = fs::metadata(&target).unwrap().ino();
            fs::write(
                &target,
                b"Foreign fixture occupant; preserve this exact file.\n",
            )
            .unwrap();
            assert_eq!(fs::metadata(&target).unwrap().ino(), inode);
        }
        "pin-flags" => {
            let before = fs::metadata(&target).unwrap();
            let mut value = read_json(&pin);
            let flags = g(&value, "flags").as_int().unwrap().as_i128().unwrap();
            set(
                &mut value,
                "flags",
                Value::Int((flags ^ if cfg!(target_os = "macos") { 0x1 } else { 0x40 }).into()),
            );
            private(&pin, &encoded(&value).unwrap());
            assert_eq!(fs::metadata(&target).unwrap().ino(), before.ino());
        }
        "flags" => {
            let output = if cfg!(target_os = "macos") {
                w.command(
                    &[
                        "/usr/bin/chflags".into(),
                        "nodump".into(),
                        target.clone().into_os_string(),
                    ],
                    &[],
                )
            } else {
                w.command(
                    &[
                        "chattr".into(),
                        "+d".into(),
                        target.clone().into_os_string(),
                    ],
                    &[],
                )
            };
            assert_eq!(output.status, agent_memory::process::Status::Code(0));
        }
        "" => {}
        _ => panic!("Unknown fixture mutation"),
    }
    let bank = all_bytes(&f.bank);
    let meta = metadata_snapshot(&f.bank);
    let attempt_bytes = all_bytes(attempt);
    let journal = fs::read(dir.join("journal.json")).unwrap();
    for operation in ["recover", "status"] {
        let result = r::command(
            r::Command {
                operation,
                stage: None,
                repo: &f.repo,
                operation_id: Some(t(&observed, "operation_id")),
                input: None,
                proposal: None,
                output: Some(&w.root.join(format!("ambiguous-{operation}.json"))),
                page: None,
                identity_review: None,
                mode: if operation == "recover" {
                    Some("finish")
                } else {
                    None
                },
                nested: None,
                unit: None,
            },
            &f.runtime,
        );
        assert_eq!(result.exit_code, 1);
        let value = agent_memory::developer_identity::load(result.stdout.as_bytes()).unwrap();
        assert_eq!(t(&value, "state"), "pending");
        assert_eq!(g(&value, "verified"), &Value::Bool(false));
        assert_eq!(
            t(&value, "failure_code"),
            "pending-ambiguous-probe-ownership"
        );
        assert_eq!(t(&value, "probe_path"), t(node, "path"));
        assert!(!g(&value, "required_actions").as_array().unwrap().is_empty());
        assert_eq!(g(&value, "writes_performed"), &s("unknown"));
        assert_eq!(g(&value, "canonical_writes_performed"), &s("unknown"));
        assert_eq!(
            g(&value, "canonical_mutation_may_have_occurred"),
            &Value::Bool(true)
        );
        for node in g(&plan, "nodes").as_array().unwrap() {
            if let Some(nonce) = node.get("nonce").and_then(Value::as_str) {
                assert!(!result.stdout.contains(nonce));
            }
        }
        assert!(!w.root.join(format!("ambiguous-{operation}.json")).exists());
    }
    assert_eq!(all_bytes(&f.bank), bank);
    assert_eq!(metadata_snapshot(&f.bank), meta);
    assert_eq!(all_bytes(attempt), attempt_bytes);
    assert_eq!(fs::read(dir.join("journal.json")).unwrap(), journal);
    assert!(!f.bank.join(".agent-memory-transaction.md").exists());
    unchanged(w, &f, &work);
    if node_type == "directory" {
        fs::remove_dir(target).unwrap();
    } else {
        fs::remove_file(target).unwrap();
    }
    probe_finish(w, &f, &observed, &work, &paths);
}
fn unowned_probe(w: &World) {
    let f = fixture(w, true);
    let work = work(w, &f);
    let path = f
        .bank
        .join(".agent-memory-probe-00000000-0000-4000-8000-000000000000.tmp");
    fs::write(
        &path,
        b"Unowned interrupted metadata probe; preserve this evidence.\n",
    )
    .unwrap();
    let before = all_bytes(&f.bank);
    assert!(r::inspect(&f.repo, None, &f.runtime).is_err());
    assert_eq!(all_bytes(&f.bank), before);
    unchanged(w, &f, &work);
}
fn metadata_retention(w: &World, mutation: &str) {
    let f = fixture(w, false);
    let work = work(w, &f);
    let (observed, proposal) = prepared(&f);
    r::apply(&f.repo, &proposal, &f.runtime).unwrap();
    let dir = directory(&f, &observed);
    let attempts = attempts(&dir);
    assert!(!attempts.is_empty());
    let attempt = &attempts[0];
    let mut plan = read_json(&attempt.join("plan.json"));
    assert!(attempt.join("completion.json").exists());
    match mutation {
        "missing" => fs::remove_file(attempt.join("completion.json")).unwrap(),
        "plan" => {
            set(&mut plan, "plan_sha256", s("0".repeat(64)));
            private(&attempt.join("plan.json"), &encoded(&plan).unwrap());
        }
        "pin" => {
            let pins = attempt.join("pins");
            fs::create_dir_all(&pins).unwrap();
            private(
                &pins.join(format!(".agent-memory-probe-{}.json", uuid())),
                b"{\"unowned\":true}\n",
            );
        }
        "foreign" => {
            let node = g(&plan, "nodes")
                .as_array()
                .unwrap()
                .iter()
                .find(|node| {
                    t(node, "type") == "file"
                        && f.bank.join(t(node, "path")).parent().unwrap().is_dir()
                })
                .unwrap();
            let path = f.bank.join(t(node, "path"));
            assert!(!path.exists());
            fs::write(path, b"External fixture occupant must remain.\n").unwrap();
        }
        "" => {}
        _ => panic!("Unknown fixture mutation"),
    }
    let bank = all_bytes(&f.bank);
    let meta = metadata_snapshot(&f.bank);
    let private_before = all_bytes(&dir);
    if mutation.is_empty() {
        let journal = fs::read(dir.join("journal.json")).unwrap();
        let attempts = all_bytes(&dir.join("metadata-probe-attempts"));
        let reviewed = cleanup_review(
            &f,
            t(&observed, "operation_id"),
            g(g(&proposal, "proposal"), "author"),
        );
        let retain = g(g(&reviewed, "proposal"), "retain").as_array().unwrap();
        assert!(
            retain
                .iter()
                .any(|p| p.as_str().unwrap().starts_with("metadata-probe-attempts/"))
        );
        assert!(retain.contains(&s("journal.json")));
        let rendered = String::from_utf8(encoded(&reviewed).unwrap()).unwrap();
        for node in g(&plan, "nodes").as_array().unwrap() {
            if let Some(nonce) = node.get("nonce").and_then(Value::as_str) {
                assert!(!rendered.contains(nonce));
            }
        }
        retention::apply(&f.repo, &reviewed, &f.runtime).unwrap();
        assert_eq!(all_bytes(&dir.join("metadata-probe-attempts")), attempts);
        assert_eq!(fs::read(dir.join("journal.json")).unwrap(), journal);
        assert_eq!(
            g(
                &r::apply(&f.repo, &proposal, &f.runtime).unwrap(),
                "receipt_is_historical"
            ),
            &Value::Bool(true)
        );
    } else {
        assert!(
            retention::inspect(&f.repo, t(&observed, "operation_id"), None, &f.runtime).is_err()
        );
        assert_eq!(all_bytes(&dir), private_before);
    }
    assert_eq!(all_bytes(&f.bank), bank);
    assert_eq!(metadata_snapshot(&f.bank), meta);
    unchanged(w, &f, &work);
}

fn apply_changed_binding(w: &World, kind: &str) {
    if ["head-index", "settings-parent"].contains(&kind)
        && std::env::var("AM_BINDING_VARIANT").is_err()
    {
        for variant in if kind == "head-index" {
            ["head", "index"]
        } else {
            ["settings", "parent"]
        } {
            subcase("AM_BINDING_VARIANT", variant);
        }
        return;
    }
    let kind = std::env::var("AM_BINDING_VARIANT").unwrap_or_else(|_| kind.into());
    let f = fixture(w, true);
    let work_before = work(w, &f);
    let (observed, proposal) = prepared(&f);
    let registry = w.root.join("config/projects.json");
    match kind.as_str() {
        "settings" => {
            w.git(
                &f.repo,
                &[
                    "config",
                    "--local",
                    "agent-memory.authorityBranch",
                    "another-branch",
                ],
            );
        }
        "parent" => {
            let mode = fs::metadata(&f.bank).unwrap().mode() & 0o777;
            fs::set_permissions(&f.bank, fs::Permissions::from_mode(mode ^ 0o020)).unwrap();
        }
        "head" => {
            let commit = w.git(
                &f.repo,
                &[
                    "commit-tree",
                    "HEAD^{tree}",
                    "-p",
                    "HEAD",
                    "-m",
                    "New feature history",
                ],
            );
            w.git(&f.repo, &["update-ref", "HEAD", commit.trim()]);
            assert_eq!(
                fs::read(f.repo.join(".git/index")).unwrap(),
                work_before.index
            );
        }
        "index" => {
            fs::write(
                f.repo.join("feature.txt"),
                b"Changed independent feature.\n",
            )
            .unwrap();
            w.git(&f.repo, &["add", "feature.txt"]);
            assert_eq!(w.git(&f.repo, &["rev-parse", "HEAD"]), work_before.head);
        }
        "registry" => {
            let mut value = read_json(&registry);
            let key = t(
                g(g(&observed, "manifest"), "identity")
                    .get("scope_context")
                    .unwrap(),
                "checkout_key",
            );
            value
                .get_mut("checkouts")
                .unwrap()
                .get_mut(key)
                .unwrap()
                .get_mut("locations")
                .unwrap()
                .as_array_mut()
                .unwrap()
                .push(s(w
                    .root
                    .join("additional-checkout-location")
                    .to_str()
                    .unwrap()));
            private(&registry, &encoded(&value).unwrap());
        }
        _ => panic!("Unknown binding fixture"),
    }
    let bank = all_bytes(&f.bank);
    let metadata = metadata_snapshot(&f.bank);
    let registry_before = fs::read(&registry).unwrap();
    let index = fs::read(f.repo.join(".git/index")).unwrap();
    let head = w.git(&f.repo, &["rev-parse", "HEAD"]);
    let feature = fs::read(f.repo.join("feature.txt")).unwrap();
    assert!(r::apply(&f.repo, &proposal, &f.runtime).is_err());
    assert_eq!(all_bytes(&f.bank), bank);
    assert_eq!(metadata_snapshot(&f.bank), metadata);
    assert_eq!(fs::read(registry).unwrap(), registry_before);
    assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), index);
    assert_eq!(w.git(&f.repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(f.repo.join("feature.txt")).unwrap(), feature);
}
fn quiescence_change(w: &World) {
    let f = fixture(w, true);
    let work = work(w, &f);
    let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
    let original = request(&f, &observed);
    for routes in [false, true] {
        let mut req = original.clone();
        if routes {
            set(
                req.get_mut("quiescence_attestation").unwrap(),
                "declared_invocation_routes",
                Value::Array(vec![]),
            );
        } else {
            set(&mut req, "quiescence_attestation", Value::Null);
        }
        let before = all_bytes(&f.bank);
        assert!(r::prepare(&f.repo, t(&observed, "operation_id"), &req, &f.runtime).is_err());
        assert_eq!(all_bytes(&f.bank), before);
    }
    let mut proposal =
        r::prepare(&f.repo, t(&observed, "operation_id"), &original, &f.runtime).unwrap();
    set(
        proposal
            .get_mut("proposal")
            .unwrap()
            .get_mut("quiescence_attestation")
            .unwrap(),
        "declared_invocation_routes",
        Value::Array(vec![]),
    );
    let before = all_bytes(&f.bank);
    assert!(r::apply(&f.repo, &proposal, &f.runtime).is_err());
    assert_eq!(all_bytes(&f.bank), before);
    unchanged(w, &f, &work);
}
fn inspect_identity_refusal(w: &World, kind: &str) {
    let f = fixture(w, false);
    let work = work(w, &f);
    let lock = if kind == "lock" {
        let owner = obj([
            ("operation_id", s(uuid())),
            ("operation", s("bank-capture")),
            ("host", s(host::hostname().unwrap())),
            ("pid", n(std::process::id() as usize)),
            ("start", s("2026-10-22T18:00:00Z")),
        ]);
        let path = f.bank.join(".agent-memory.lock");
        private(&path, &encoded(&owner).unwrap());
        let d = agent_memory::fs::Descriptor::open_directory(&f.bank)
            .unwrap()
            .open_at(
                path.file_name().unwrap(),
                libc::O_RDWR | libc::O_NOFOLLOW,
                0,
            )
            .unwrap();
        Some(
            agent_memory::fs::lock::FlockGuard::try_acquire(
                d,
                agent_memory::fs::lock::LockMode::Exclusive,
            )
            .unwrap(),
        )
    } else {
        None
    };
    match kind {
        "uuid" => {
            w.git(&f.repo, &["add", "memory-bank"]);
            let raw = "Malformed identity prose: 75b38e17-c36e-4fab-8ba9-5fa51f25de91\nRepeated: 75b38e17-c36e-4fab-8ba9-5fa51f25de91\n";
            fs::write(f.bank.join("MEMORY.md"), raw).unwrap();
        }
        "authority" => {
            w.git(
                &f.repo,
                &[
                    "remote",
                    "add",
                    "origin",
                    "https://github.com/owner/project.git",
                ],
            );
            w.git(
                &f.repo,
                &[
                    "remote",
                    "set-url",
                    "--push",
                    "origin",
                    "https://github.com/other/project.git",
                ],
            );
            w.git(&f.repo, &["add", "memory-bank"]);
        }
        "xattr" => {
            let p = f.bank.join("projectbrief.md");
            let output = if cfg!(target_os = "macos") {
                w.command(
                    &[
                        "xattr".into(),
                        "-w".into(),
                        "com.agent-memory.test".into(),
                        "preserve".into(),
                        p.into_os_string(),
                    ],
                    &[],
                )
            } else {
                w.command(
                    &[
                        "setfattr".into(),
                        "-n".into(),
                        "user.agent-memory-test".into(),
                        "-v".into(),
                        "preserve".into(),
                        p.into_os_string(),
                    ],
                    &[],
                )
            };
            assert_eq!(output.status, agent_memory::process::Status::Code(0));
        }
        "acl" => {
            let p = f.bank.join("projectbrief.md");
            let output = if cfg!(target_os = "macos") {
                w.command(
                    &[
                        "chmod".into(),
                        "+a".into(),
                        "everyone deny delete".into(),
                        p.into_os_string(),
                    ],
                    &[],
                )
            } else {
                w.command(
                    &[
                        "setfacl".into(),
                        "-m".into(),
                        format!("u:{}:r--", agent_memory::host::effective_uid()).into(),
                        p.into_os_string(),
                    ],
                    &[],
                )
            };
            assert_eq!(output.status, agent_memory::process::Status::Code(0));
        }
        "lock" => {}
        _ => panic!("Unknown identity fixture"),
    }
    let before = all_bytes(&f.bank);
    let metadata = metadata_snapshot(&f.bank);
    let index = fs::read(f.repo.join(".git/index")).unwrap();
    assert!(r::inspect(&f.repo, None, &f.runtime).is_err());
    assert_eq!(all_bytes(&f.bank), before);
    assert_eq!(metadata_snapshot(&f.bank), metadata);
    assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), index);
    assert_eq!(w.git(&f.repo, &["rev-parse", "HEAD"]), work.head);
    drop(lock);
}

fn rebind_review(request: &mut Value) {
    for (key, field) in [
        ("unit_map", "unit_map_sha256"),
        ("intended_files", "intended_files_sha256"),
        ("metadata_plan", "metadata_plan_sha256"),
    ] {
        let hash = sha(g(request, key));
        set(request.get_mut("review").unwrap(), field, s(hash));
    }
    let rows = g(g(request, "unit_map"), "units")
        .as_array()
        .unwrap()
        .iter()
        .map(|u| {
            obj([
                ("unit_id", g(u, "unit_id").clone()),
                ("disposition", g(u, "disposition").clone()),
                ("rationale", g(u, "reason").clone()),
            ])
        })
        .collect();
    set(
        request.get_mut("review").unwrap(),
        "unit_decisions",
        Value::Array(rows),
    );
}
fn add_native(f: &mut Fixture, path: &str, body: String, lifecycle: &str) {
    fs::create_dir_all(f.bank.join(path).parent().unwrap()).unwrap();
    fs::write(f.bank.join(path), &body).unwrap();
    if let Value::Object(files) = &mut f.native {
        files.push((path.into(), s(body)));
    }
    let (metadata, mut routes, omitted) =
        agent_memory::bank_contract::parse_router(t(&f.native, "MEMORY.md")).unwrap();
    routes.push(obj([
        ("path", s(path)),
        ("purpose", s("Controlled prior-guidance fixture")),
        ("scope", s("repository")),
        ("lifecycle", s(lifecycle)),
        ("read-when", s("When reviewing synthetic conditions")),
    ]));
    let router = agent_memory::bank_contract::router_text(&metadata, &routes, &omitted).unwrap();
    set(&mut f.native, "MEMORY.md", s(&router));
    fs::write(f.bank.join("MEMORY.md"), router).unwrap();
}
fn history_fixture(w: &World) -> (Fixture, String, String) {
    let mut f = fixture(w, false);
    let record = f
        .native
        .as_object()
        .unwrap()
        .iter()
        .find(|(p, _)| p.starts_with("topics/"))
        .unwrap()
        .1
        .as_str()
        .unwrap();
    let (_, mut metadata, body) = agent_memory::records::parse(record, &[]).unwrap();
    set(&mut metadata, "status", s("superseded"));
    if let Value::Object(pairs) = &mut metadata {
        pairs.push((
            "state-reason".into(),
            s("Independently reviewed correction retains this historical version."),
        ));
    }
    let history = format!("history/sources/{}/{}.md", uuid(), t(&metadata, "id"));
    let raw =
        agent_memory::codec::frontmatter::write(metadata.as_object().unwrap()).unwrap() + &body;
    add_native(&mut f, &history, raw, "superseded");
    let id = uuid();
    let tomb = format!("history/tombstones/{id}.md");
    let metadata = obj([
        ("id", s(id)),
        ("scope", s("75b38e17-c36e-4fab-8ba9-5fa51f25de91")),
        ("deleted-at", s("2026-10-22T20:00:00Z")),
        ("deletion-authority", s(&f.dev)),
        ("reason-class", s("user-request")),
    ]);
    add_native(
        &mut f,
        &tomb,
        agent_memory::codec::frontmatter::write(metadata.as_object().unwrap()).unwrap(),
        "deleted",
    );
    f.originals = snapshot(&f.bank);
    (f, history, tomb)
}
fn root_semantics(w: &World, kind: &str) {
    let (f, history, tomb) = history_fixture(w);
    let work = work(w, &f);
    let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
    let control = request(&f, &observed);
    let mut req = control.clone();
    if kind == "provenance" {
        let (_, mut metadata, body) =
            agent_memory::records::parse(t(&f.native, &history), &[]).unwrap();
        set(
            metadata.get_mut("origin").unwrap(),
            "session",
            s("different-unverified-session"),
        );
        let raw =
            agent_memory::codec::frontmatter::write(metadata.as_object().unwrap()).unwrap() + &body;
        set(req.get_mut("intended_files").unwrap(), &history, s(&raw));
        for unit in req
            .get_mut("unit_map")
            .unwrap()
            .get_mut("units")
            .unwrap()
            .as_array_mut()
            .unwrap()
        {
            for target in unit.get_mut("targets").unwrap().as_array_mut().unwrap() {
                if t(target, "path") == history {
                    set(target, "sha256", s(sha256_hex(raw.as_bytes())));
                }
            }
        }
    } else if kind == "discard" || kind == "backup" {
        let path = if kind == "discard" { &tomb } else { &history };
        for unit in req
            .get_mut("unit_map")
            .unwrap()
            .get_mut("units")
            .unwrap()
            .as_array_mut()
            .unwrap()
        {
            if g(unit, "targets")
                .as_array()
                .unwrap()
                .iter()
                .any(|row| t(row, "path") == path)
            {
                set(unit, "targets", Value::Array(vec![]));
                if kind == "discard" {
                    set(unit, "semantic_kind", s("non-guidance"));
                    set(unit, "disposition", s("non-guidance-discard"));
                    set(unit, "reason", s("A private backup exists."));
                }
            }
        }
        if kind == "discard" {
            if let Value::Object(files) = req.get_mut("intended_files").unwrap() {
                files.retain(|(p, _)| p != &tomb);
            }
            if let Value::Object(files) = req
                .get_mut("metadata_plan")
                .unwrap()
                .get_mut("files")
                .unwrap()
            {
                files.retain(|(p, _)| p != &tomb);
            }
            let router = t(g(&req, "intended_files"), "MEMORY.md")
                .lines()
                .filter(|line| !line.contains(&tomb))
                .map(|line| format!("{line}\n"))
                .collect::<String>();
            set(
                req.get_mut("intended_files").unwrap(),
                "MEMORY.md",
                s(&router),
            );
            for unit in req
                .get_mut("unit_map")
                .unwrap()
                .get_mut("units")
                .unwrap()
                .as_array_mut()
                .unwrap()
            {
                for target in unit.get_mut("targets").unwrap().as_array_mut().unwrap() {
                    if t(target, "path") == "MEMORY.md" {
                        set(target, "sha256", s(sha256_hex(router.as_bytes())));
                    }
                }
            }
        }
    }
    rebind_review(&mut req);
    if kind == "discard" {
        let author = g(&req, "author").clone();
        for decision in req
            .get_mut("review")
            .unwrap()
            .get_mut("unit_decisions")
            .unwrap()
            .as_array_mut()
            .unwrap()
        {
            if t(decision, "disposition") == "non-guidance-discard" {
                let mut authority = author.clone();
                if let Value::Object(rows) = &mut authority {
                    rows.push(("statement".into(),s("Explicitly authorize this source-bound non-guidance archive/removal; byte retention alone is not the reason.")));
                }
                if let Value::Object(rows) = decision {
                    rows.push(("authority".into(), authority));
                }
            }
        }
    }
    let before = all_bytes(&f.bank);
    let private = all_bytes(&directory(&f, &observed));
    if kind != "preserve" {
        assert!(r::prepare(&f.repo, t(&observed, "operation_id"), &req, &f.runtime).is_err());
        assert_eq!(all_bytes(&f.bank), before);
        assert_eq!(all_bytes(&directory(&f, &observed)), private);
        unchanged(w, &f, &work);
        if kind == "discard" {
            assert_eq!(saved_originals(&f, &observed), f.originals);
            return;
        }
    }
    let proposal = r::prepare(&f.repo, t(&observed, "operation_id"), &control, &f.runtime).unwrap();
    if kind == "preserve" || kind == "backup" {
        let result = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
        assert_eq!(g(&result, "semantic_recovery_complete"), &Value::Bool(true));
        assert_eq!(
            fs::read(f.bank.join(&history)).unwrap(),
            t(&f.native, &history).as_bytes()
        );
        assert_eq!(
            fs::read(f.bank.join(&tomb)).unwrap(),
            t(&f.native, &tomb).as_bytes()
        );
    }
    assert_eq!(saved_originals(&f, &observed), f.originals);
    unchanged(w, &f, &work);
}

fn add_xattr(w: &World, path: &Path, name: &str, value: &str) {
    let output = if cfg!(target_os = "macos") {
        w.command(
            &[
                "xattr".into(),
                "-w".into(),
                name.into(),
                value.into(),
                path.as_os_str().to_owned(),
            ],
            &[],
        )
    } else {
        w.command(
            &[
                "setfattr".into(),
                "-n".into(),
                format!("user.{name}").into(),
                "-v".into(),
                value.into(),
                path.as_os_str().to_owned(),
            ],
            &[],
        )
    };
    assert_eq!(output.status, agent_memory::process::Status::Code(0));
}
fn provenance(w: &World, kind: &str) {
    let f = fixture(w, false);
    let before = metadata_snapshot(&f.bank);
    let source = f.originals.clone();
    let target = f.bank.join("activeContext.md");
    if kind == "unknown" || kind == "flags" {
        if kind == "unknown" {
            add_xattr(
                w,
                &target,
                "org.agent-memory.contract-control",
                "controlled",
            );
        } else {
            let output = if cfg!(target_os = "macos") {
                w.command(
                    &[
                        "/usr/bin/chflags".into(),
                        "nodump".into(),
                        target.clone().into_os_string(),
                    ],
                    &[],
                )
            } else {
                w.command(
                    &[
                        "chattr".into(),
                        "+d".into(),
                        target.clone().into_os_string(),
                    ],
                    &[],
                )
            };
            assert_eq!(output.status, agent_memory::process::Status::Code(0));
        }
        let metadata = metadata_snapshot(&f.bank);
        assert!(r::inspect(&f.repo, None, &f.runtime).is_err());
        assert_eq!(metadata_snapshot(&f.bank), metadata);
        assert_eq!(snapshot(&f.bank), source);
        return;
    }
    let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
    let mut req = request(&f, &observed);
    if kind == "wrong" {
        let attrs = req
            .get_mut("metadata_plan")
            .unwrap()
            .get_mut("files")
            .unwrap()
            .get_mut("activeContext.md")
            .unwrap()
            .get_mut("xattrs")
            .unwrap();
        let old = attrs
            .get("com.apple.provenance")
            .and_then(Value::as_str)
            .unwrap_or("");
        let value = format!("{old}00");
        if let Value::Object(rows) = attrs {
            if let Some((_, v)) = rows.iter_mut().find(|(k, _)| k == "com.apple.provenance") {
                *v = s(value);
            } else {
                rows.push(("com.apple.provenance".into(), s(value)));
            }
        }
        rebind_review(&mut req);
        assert!(r::prepare(&f.repo, t(&observed, "operation_id"), &req, &f.runtime).is_err());
        assert_eq!(metadata_snapshot(&f.bank), before);
        assert_eq!(snapshot(&f.bank), source);
        return;
    }
    let proposal = r::prepare(&f.repo, t(&observed, "operation_id"), &req, &f.runtime).unwrap();
    if kind == "changed" {
        add_xattr(
            w,
            &target,
            "org.agent-memory.drift-control",
            "controlled drift",
        );
        let metadata = metadata_snapshot(&f.bank);
        assert!(r::apply(&f.repo, &proposal, &f.runtime).is_err());
        assert_eq!(metadata_snapshot(&f.bank), metadata);
        assert_eq!(snapshot(&f.bank), source);
        return;
    }
    for row in g(g(&observed, "manifest"), "physical_entries")
        .as_array()
        .unwrap()
    {
        if let Some(expected) = before.get(t(row, "path")) {
            assert_eq!(g(row, "xattrs"), g(expected, "xattrs"));
        }
    }
    for (path, row) in g(g(g(&proposal, "proposal"), "metadata_plan"), "files")
        .as_object()
        .unwrap()
    {
        assert_eq!(g(row, "xattrs"), g(before.get(path).unwrap(), "xattrs"));
    }
    r::apply(&f.repo, &proposal, &f.runtime).unwrap();
    let after = metadata_snapshot(&f.bank);
    for path in f.originals.keys() {
        assert_eq!(after[path], before[path]);
    }
    assert_eq!(snapshot(&f.bank), native_bytes(&f));
    let child = r::rollback(
        &f.repo,
        Some(t(&observed, "operation_id")),
        None,
        &f.runtime,
    )
    .unwrap();
    r::rollback(&f.repo, None, Some(&child), &f.runtime).unwrap();
    assert_eq!(snapshot(&f.bank), source);
    assert_eq!(metadata_snapshot(&f.bank), before);
}
fn unsafe_pending(w: &World, transcript: bool) {
    if std::env::var("AM_UNSAFE_PENDING").is_err() {
        for variant in if transcript {
            ["intended", "pending"]
        } else {
            ["secret", "oversized"]
        } {
            subcase("AM_UNSAFE_PENDING", variant);
        }
        return;
    }
    let variant = std::env::var("AM_UNSAFE_PENDING").unwrap();
    let mut f = fixture(w, false);
    let body = if transcript {
        "User: Private fixture question.\nAssistant: Private fixture response.\n".to_owned()
    } else if variant == "secret" {
        "# Unsafe\n\npassword=PRIVATE_FRONTIER_SECRET_987654321\n".to_owned()
    } else {
        "x".repeat(16385)
    };
    fs::write(f.bank.join("handoff-prompt.md"), &body).unwrap();
    f.originals = snapshot(&f.bank);
    let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
    let mut req = request(&f, &observed);
    if transcript && variant == "intended" {
        let intended = t(g(&req, "intended_files"), "activeContext.md").to_owned() + "\n" + &body;
        set(
            req.get_mut("intended_files").unwrap(),
            "activeContext.md",
            s(&intended),
        );
        for unit in req
            .get_mut("unit_map")
            .unwrap()
            .get_mut("units")
            .unwrap()
            .as_array_mut()
            .unwrap()
        {
            if t(unit, "disposition") == "pending-admission" {
                set(unit, "disposition", s("preserve"));
                set(unit, "required_actions", Value::Array(vec![]));
                set(
                    unit,
                    "targets",
                    Value::Array(vec![obj([
                        ("path", s("activeContext.md")),
                        ("sha256", s(sha256_hex(intended.as_bytes()))),
                    ])]),
                );
            } else {
                for target in unit.get_mut("targets").unwrap().as_array_mut().unwrap() {
                    if t(target, "path") == "activeContext.md" {
                        set(target, "sha256", s(sha256_hex(intended.as_bytes())));
                    }
                }
            }
        }
    }
    rebind_review(&mut req);
    let before = all_bytes(&f.bank);
    let error = r::prepare(&f.repo, t(&observed, "operation_id"), &req, &f.runtime).unwrap_err();
    assert!(
        !error
            .to_string()
            .contains("PRIVATE_FRONTIER_SECRET_987654321")
    );
    assert_eq!(all_bytes(&f.bank), before);
    assert_eq!(saved_originals(&f, &observed), f.originals);
    assert!(!f.bank.join(".agent-memory-transaction.md").exists());
    assert!(!f.bank.join(".agent-memory-recovery.md").exists());
}

fn review_pin(w: &World) {
    let f = fixture(w, true);
    let work = work(w, &f);
    let (observed, proposal) = prepared(&f);
    fault(
        w,
        &f,
        "apply",
        Some(&proposal),
        "layout-recovery-before-receipt",
        "error",
    );
    let preview = r::recover(
        &f.repo,
        Some(t(&observed, "operation_id")),
        Some("finish"),
        None,
        &f.runtime,
    )
    .unwrap();
    let dir = directory(&f, &observed);
    let review = dir
        .join("reviews")
        .join(format!("{}.json", t(g(&proposal, "proposal"), "review_id")));
    let original = fs::read(&review).unwrap();
    let mut altered = read_json(&review);
    let evidence = t(&altered, "evidence").to_owned() + " Changed after review.";
    set(&mut altered, "evidence", s(evidence));
    private(&review, &encoded(&altered).unwrap());
    let before = all_bytes(&f.bank);
    let retained = all_bytes(&dir);
    assert!(r::recover(&f.repo, None, None, Some(&preview), &f.runtime).is_err());
    assert_eq!(all_bytes(&f.bank), before);
    assert_eq!(all_bytes(&dir), retained);
    unchanged(w, &f, &work);
    fs::write(review, original).unwrap();
    let result = r::recover(&f.repo, None, None, Some(&preview), &f.runtime).unwrap();
    assert_eq!(g(&result, "verified"), &Value::Bool(true));
    unchanged(w, &f, &work);
}
fn child_marker(w: &World) {
    let f = fixture(w, true);
    let (observed, proposal) = prepared(&f);
    r::apply(&f.repo, &proposal, &f.runtime).unwrap();
    let child = r::rollback(
        &f.repo,
        Some(t(&observed, "operation_id")),
        None,
        &f.runtime,
    )
    .unwrap();
    let id = t(g(&child, "proposal"), "operation_id");
    fault(
        w,
        &f,
        "rollback",
        Some(&child),
        "layout-recovery-after-marker-swap",
        "error",
    );
    let marker = read_json(&f.bank.join(".agent-memory-transaction.md"));
    assert_ne!(id, t(&observed, "operation_id"));
    assert_eq!(t(&marker, "operation_id"), id);
    assert_eq!(t(&marker, "transaction_kind"), "rollback");
    finish(&f, id, "finish");
    assert_eq!(
        t(
            &read_json(&f.bank.join(".agent-memory-transaction.md")),
            "operation_id"
        ),
        t(&observed, "operation_id")
    );
}
fn setgid_bank(w: &World) {
    let f = fixture(w, false);
    fs::set_permissions(&f.bank, fs::Permissions::from_mode(0o2770)).unwrap();
    let before = fs::metadata(&f.bank).unwrap();
    assert_eq!(before.mode() & 0o7777, 0o2770);
    let (_, proposal) = prepared(&f);
    let result = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
    assert_eq!(g(&result, "canonical_eligibility"), &Value::Bool(true));
    let after = fs::metadata(&f.bank).unwrap();
    assert_eq!(after.mode() & 0o7777, 0o2770);
    assert_eq!(after.gid(), before.gid());
}

include!("fanout_c_cases.rs");

fn historical_checkout(w: &World) {
    let f = fixture(w, false);
    w.git(
        &f.repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/memory-fixture.git",
        ],
    );
    let path = w.root.join("config/projects.json");
    let mut data = read_json(&path);
    let info = fs::metadata(f.repo.join(".git")).unwrap();
    let key = format!("{}:{}", info.dev(), info.ino());
    let entry = g(g(&data, "checkouts"), &key).clone();
    if let Value::Object(rows) = data.get_mut("checkouts").unwrap() {
        rows.retain(|(k, _)| k != &key);
        rows.push((format!("{}:{}", info.dev() + 1, info.ino()), entry.clone()));
    }
    private(&path, &encoded(&data).unwrap());
    let out = w.command(
        &[
            env!("CARGO_BIN_EXE_am").into(),
            "identity".into(),
            "status".into(),
            "--repo".into(),
            f.repo.clone().into_os_string(),
        ],
        &[],
    );
    assert_eq!(out.status, agent_memory::process::Status::Code(0));
    let positive = agent_memory::developer_identity::load(&out.stdout).unwrap();
    assert_eq!(g(&positive, "resolved"), &Value::Bool(true));
    assert_eq!(
        t(g(g(&positive, "project"), "evidence"), "binding_source"),
        "historical-device-change"
    );
    let mut other = entry;
    set(&mut other, "authority_name", s("different-authority"));
    if let Value::Object(rows) = data.get_mut("checkouts").unwrap() {
        rows.push((format!("{}:{}", info.dev() + 2, info.ino()), other));
    }
    private(&path, &encoded(&data).unwrap());
    let before = all_bytes(&f.bank);
    let registry = fs::read(&path).unwrap();
    assert!(r::inspect(&f.repo, None, &f.runtime).is_err());
    assert_eq!(all_bytes(&f.bank), before);
    assert_eq!(fs::read(path).unwrap(), registry);
}
fn raw_router(w: &World) {
    let f = fixture(w, false);
    let observed = r::inspect(&f.repo, None, &f.runtime).unwrap();
    let committed = fs::read(f.bank.join("MEMORY.md")).unwrap();
    w.git(&f.repo, &["add", "."]);
    w.git(
        &f.repo,
        &["commit", "-m", "Controlled existing router evidence"],
    );
    let head = w.git(&f.repo, &["rev-parse", "HEAD"]);
    let identity = g(g(g(&observed, "manifest"), "identity"), "scope_context");
    let mut review = Value::Null;
    for raw in [
        [committed.as_slice(), b"\n", &vec![b' '; 17000]].concat(),
        b"malformed router with ambiguous UUID-looking prose\n".to_vec(),
    ] {
        fs::write(f.bank.join("MEMORY.md"), &raw).unwrap();
        review = obj([
            ("schema", s("agent-memory/layout-identity-review-v1")),
            ("review_id", s(uuid())),
            ("project_id", g(identity, "project_id").clone()),
            ("repo_root", s(f.repo.to_str().unwrap())),
            ("checkout_key", g(identity, "checkout_key").clone()),
            ("router_sha256", s(sha256_hex(&raw))),
            (
                "committed_router",
                s(format!("{}:memory-bank/MEMORY.md", head.trim())),
            ),
            ("committed_router_sha256", s(sha256_hex(&committed))),
            ("reviewer_developer_id", g(identity, "developer_id").clone()),
            ("reviewer_harness", s("controlled-reviewer")),
            ("reviewer_session", s("router-review")),
            ("author_session", s("router-author")),
            (
                "reviewed_at",
                g(g(&observed, "manifest"), "inspected_at").clone(),
            ),
            (
                "rationale",
                s("Independently checked existing registry/checkout and exact committed router."),
            ),
            ("decision", s("accept")),
        ]);
        let before = all_bytes(&f.bank);
        let inspected = r::inspect(&f.repo, Some(&review), &f.runtime).unwrap();
        assert_eq!(g(&inspected, "observation_complete"), &Value::Bool(true));
        assert_eq!(
            g(g(&inspected, "manifest"), "project_id"),
            g(identity, "project_id")
        );
        assert_eq!(all_bytes(&f.bank), before);
        assert_eq!(saved_originals(&f, &inspected), snapshot(&f.bank));
        let out = w.command(
            &[
                env!("CARGO_BIN_EXE_am").into(),
                "bank".into(),
                "validate".into(),
                "--repo".into(),
                f.repo.clone().into_os_string(),
            ],
            &[],
        );
        assert_ne!(out.status, agent_memory::process::Status::Code(0));
        if raw.starts_with(b"malformed") {
            let mut wrong = review.clone();
            set(&mut wrong, "project_id", s(uuid()));
            assert!(r::inspect(&f.repo, Some(&wrong), &f.runtime).is_err());
            assert_eq!(all_bytes(&f.bank), before);
        }
    }
    let text = String::from_utf8(committed)
        .unwrap()
        .replace(t(identity, "project_id"), &uuid());
    fs::write(f.bank.join("MEMORY.md"), &text).unwrap();
    set(&mut review, "router_sha256", s(sha256_hex(text.as_bytes())));
    let before = all_bytes(&f.bank);
    let index = fs::read(f.repo.join(".git/index")).unwrap();
    for supplied in [None, Some(&review)] {
        assert!(r::inspect(&f.repo, supplied, &f.runtime).is_err());
        assert_eq!(all_bytes(&f.bank), before);
        assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), index);
        assert_eq!(w.git(&f.repo, &["rev-parse", "HEAD"]), head);
    }
}

fn incapable_runtime(w: &World) {
    let f = fixture(w, true);
    let (observed, proposal) = prepared(&f);
    let compatibility = f
        .runtime
        .join("skills/agent-memory/schemas/runtime-compatibility.json");
    let mut value = read_json(&compatibility);
    value
        .get_mut("private_state_capabilities")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .retain(|v| v != &s("layout-frontier-v1"));
    fs::write(&compatibility, encoded(&value).unwrap()).unwrap();
    let bank = all_bytes(&f.bank);
    let dir = directory(&f, &observed);
    let retained = all_bytes(&dir);
    let config = all_bytes(&w.root.join("config"));
    let state = all_bytes(&w.root.join("state"));
    let path = w.save(&proposal, "incapable-proposal.json");
    let output = r::command(
        r::Command {
            operation: "apply",
            stage: None,
            repo: &f.repo,
            operation_id: None,
            input: None,
            proposal: Some(&path),
            output: None,
            page: None,
            identity_review: None,
            mode: None,
            nested: None,
            unit: None,
        },
        &f.runtime,
    );
    assert_ne!(output.exit_code, 0);
    let result = agent_memory::developer_identity::load(output.stdout.as_bytes()).unwrap();
    assert_eq!(t(&result, "failure_code"), "pending-runtime-compatibility");
    assert_eq!(g(&result, "writes_performed"), &Value::Bool(false));
    assert_eq!(
        g(&result, "canonical_writes_performed"),
        &Value::Bool(false)
    );
    assert_eq!(all_bytes(&f.bank), bank);
    assert_eq!(all_bytes(&dir), retained);
    assert_eq!(all_bytes(&w.root.join("config")), config);
    assert_eq!(all_bytes(&w.root.join("state")), state);
}
fn continued_request(f: &Fixture, observed: &Value) -> Value {
    let mut req = request(f, observed);
    let base = directory(f, observed).parent().unwrap().to_path_buf();
    let index = read_json(&base.join("index.json"));
    let mut inherited = vec![];
    let mut rows = g(&index, "operations").as_object().unwrap().to_vec();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, row) in rows {
        if t(&row, "state") != "rolled-back" {
            continue;
        }
        let dir = base.join(&id);
        let manifest = read_json(&dir.join("manifest.json"));
        let head = read_json(&dir.join("ledger-head.json"));
        let generation = g(&head, "generation").as_int().unwrap().as_u64().unwrap();
        let ledger = read_json(&dir.join("ledger").join(format!("{generation:08}.json")));
        for unit in g(&ledger, "units").as_array().unwrap() {
            if t(unit, "state") == "pending-admission" {
                let mut body = vec![];
                for span in g(unit, "source_ranges").as_array().unwrap() {
                    let source = g(&manifest, "sources")
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|r| g(r, "source_id") == g(span, "source_id"))
                        .unwrap();
                    let bytes = fs::read(dir.join(t(source, "blob"))).unwrap();
                    let start = g(span, "start").as_int().unwrap().as_u64().unwrap() as usize;
                    let end = g(span, "end").as_int().unwrap().as_u64().unwrap() as usize;
                    body.extend_from_slice(&bytes[start..end]);
                }
                inherited.push(obj([
                    ("operation_id", s(&id)),
                    ("unit_id", g(unit, "unit_id").clone()),
                    ("body", s(String::from_utf8(body).unwrap())),
                    ("source_ranges", g(unit, "source_ranges").clone()),
                    ("provenance", g(unit, "provenance").clone()),
                    ("reason", g(unit, "reason").clone()),
                    ("selection_class", s("comparison-only-not-current-guidance")),
                    ("source_assertions_verified", Value::Bool(false)),
                ]));
            }
        }
    }
    let hash = sha(&Value::Array(inherited.clone()));
    set(
        req.get_mut("review").unwrap(),
        "inherited_frontier_sha256",
        s(hash),
    );
    let decisions=inherited.iter().map(|row|obj([("unit_id",g(row,"unit_id").clone()),("disposition",s("retain-pending")),("rationale",s("Retain original reviewed obligation and stable identity; no admission by this repair"))])).collect();
    set(
        req.get_mut("review").unwrap(),
        "inherited_unit_decisions",
        Value::Array(decisions),
    );
    req
}

fn discard_source(f: &Fixture, observed: &Value, id: &str) -> Value {
    let dir = directory(f, observed);
    let head = read_json(&dir.join("ledger-head.json"));
    let generation = g(&head, "generation").as_int().unwrap().as_u64().unwrap();
    let ledger = read_json(&dir.join("ledger").join(format!("{generation:08}.json")));
    let unit = g(&ledger, "units")
        .as_array()
        .unwrap()
        .iter()
        .find(|u| t(u, "unit_id") == id)
        .unwrap();
    let authority = obj([
        ("schema", s("agent-memory/layout-nonguidance-discard-v1")),
        ("review_id", s(uuid())),
        ("operation_id", g(observed, "operation_id").clone()),
        ("unit_id", s(id)),
        ("manifest_sha256", g(&ledger, "manifest_sha256").clone()),
        ("unit_map_sha256", g(&ledger, "unit_map_sha256").clone()),
        ("ledger_head_sha256", g(&head, "sha256").clone()),
        ("source_ranges", g(unit, "source_ranges").clone()),
        ("author_harness", s("controlled-source-author")),
        ("author_session", s("scratch-author")),
        ("reviewer_developer_id", s(&f.dev)),
        ("reviewer_harness", s("controlled-independent-reviewer")),
        ("reviewer_session", s("scratch-reviewer")),
        (
            "review_time",
            g(g(observed, "manifest"), "inspected_at").clone(),
        ),
        ("non_guidance", Value::Bool(true)),
        (
            "reason",
            s(
                "Explicit controlled authority for disposable scratch work-state; not guidance or an age-based discard.",
            ),
        ),
        ("decision", s("discard")),
    ]);
    agent_memory::layout_resolution::resolve(
        &f.repo,
        Some(t(observed, "operation_id")),
        Some(id),
        Some(&obj([("discard_authority", authority)])),
        None,
        &f.runtime,
    )
    .unwrap()
}
fn retained_handoff(w: &World, unfinished: bool) {
    if !unfinished && std::env::var("AM_HANDOFF_COMPLETED").is_err() {
        for variant in ["false", "true"] {
            subcase("AM_HANDOFF_COMPLETED", variant);
        }
        return;
    }
    let mut f = fixture(w, false);
    let scratch=b"# Disposable scratch\n\nSynthetic temporary work-state; explicitly not durable guidance.\n";
    let path = if unfinished {
        "handoff-notes.txt"
    } else {
        "handoff-prompt.md"
    };
    fs::write(f.bank.join(path), scratch).unwrap();
    f.originals = snapshot(&f.bank);
    let (observed, proposal) = prepared(&f);
    if std::env::var("AM_HANDOFF_COMPLETED").ok().as_deref() == Some("true") {
        r::apply(&f.repo, &proposal, &f.runtime).unwrap();
        let child = r::rollback(
            &f.repo,
            Some(t(&observed, "operation_id")),
            None,
            &f.runtime,
        )
        .unwrap();
        r::rollback(&f.repo, None, Some(&child), &f.runtime).unwrap();
    } else {
        fault(
            w,
            &f,
            "apply",
            Some(&proposal),
            "layout-recovery-before-receipt",
            "error",
        );
        finish(&f, t(&observed, "operation_id"), "rollback");
    }
    let old_status = r::status(&f.repo, Some(t(&observed, "operation_id")), &f.runtime).unwrap();
    let old_ids = g(&old_status, "pending_unit_ids").as_array().unwrap();
    assert_eq!(old_ids.len(), 1);
    assert_eq!(snapshot(&f.bank), f.originals);
    let old_marker = fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap();
    let next = r::inspect(&f.repo, None, &f.runtime).unwrap();
    assert_eq!(
        g(g(g(&next, "manifest"), "predecessor"), "operation_id"),
        g(&observed, "operation_id")
    );
    assert_eq!(
        fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
        old_marker
    );
    let request = continued_request(&f, &next);
    let proposal = r::prepare(&f.repo, t(&next, "operation_id"), &request, &f.runtime).unwrap();
    if unfinished {
        let resolution = discard_source(&f, &observed, old_ids[0].as_str().unwrap());
        fault(
            w,
            &f,
            "resolve",
            Some(&resolution),
            "layout-recovery-resolution-after-receipt",
            "error",
        );
        let retained = all_bytes(directory(&f, &observed).parent().unwrap());
        let bank = all_bytes(&f.bank);
        assert!(r::apply(&f.repo, &proposal, &f.runtime).is_err());
        assert!(r::inspect(&f.repo, None, &f.runtime).is_err());
        assert!(r::prepare(&f.repo, t(&next, "operation_id"), &request, &f.runtime).is_err());
        assert_eq!(all_bytes(&f.bank), bank);
        assert_eq!(
            all_bytes(directory(&f, &observed).parent().unwrap()),
            retained
        );
        let completed = agent_memory::layout_resolution::resolve(
            &f.repo,
            None,
            None,
            None,
            Some(&resolution),
            &f.runtime,
        )
        .unwrap();
        assert_eq!(
            g(&completed, "semantic_recovery_complete"),
            &Value::Bool(true)
        );
        assert!(r::apply(&f.repo, &proposal, &f.runtime).is_err());
        r::inspect(&f.repo, None, &f.runtime).unwrap();
        assert_eq!(snapshot(&f.bank), f.originals);
        return;
    }
    let applied = r::apply(&f.repo, &proposal, &f.runtime).unwrap();
    let ids = g(&applied, "pending_unit_ids").as_array().unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&old_ids[0]));
    let new_id = ids
        .iter()
        .find(|id| !old_ids.contains(id))
        .unwrap()
        .as_str()
        .unwrap();
    let marker = fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap();
    assert_ne!(marker, old_marker);
    assert_eq!(
        t(
            &read_json(&f.bank.join(".agent-memory-transaction.md")),
            "operation_id"
        ),
        t(&next, "operation_id")
    );
    let resolution = discard_source(&f, &next, new_id);
    agent_memory::layout_resolution::resolve(
        &f.repo,
        None,
        None,
        None,
        Some(&resolution),
        &f.runtime,
    )
    .unwrap();
    let after = r::status(&f.repo, Some(t(&next, "operation_id")), &f.runtime).unwrap();
    assert_eq!(
        g(&after, "pending_unit_ids"),
        g(&old_status, "pending_unit_ids")
    );
    assert_eq!(g(&after, "semantic_recovery_complete"), &Value::Bool(false));
    assert_eq!(
        fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
        marker
    );
    assert!(f.bank.join(".agent-memory-recovery.md").exists());
    assert_eq!(saved_originals(&f, &observed), f.originals);
    let source = g(g(&observed, "manifest"), "sources")
        .as_array()
        .unwrap()
        .iter()
        .find(|r| t(r, "path") == path)
        .unwrap();
    let blob = directory(&f, &observed).join(t(source, "blob"));
    let mut bytes = fs::read(&blob).unwrap();
    bytes.extend_from_slice(b"changed");
    fs::write(blob, bytes).unwrap();
    assert!(r::status(&f.repo, Some(t(&next, "operation_id")), &f.runtime).is_err());
    assert_eq!(
        fs::read(f.bank.join(".agent-memory-transaction.md")).unwrap(),
        marker
    );
}
