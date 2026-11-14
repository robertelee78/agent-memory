use super::*;
use std::os::unix::fs::PermissionsExt;
fn rows(value: &Value, key: &str) -> Vec<Value> {
    get(value, key).as_array().unwrap().to_vec()
}
fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    text(get(value, key))
}
fn profile<T>(
    w: &mut World,
    name: &str,
    body: impl FnOnce(&mut World) -> TestResult<T>,
) -> TestResult<T> {
    let old = w.env.clone();
    for (key, leaf) in [
        ("AGENT_MEMORY_CONFIG_HOME", "config"),
        ("AGENT_MEMORY_STATE_HOME", "state"),
        ("AGENT_MEMORY_BACKUP_HOME", "backups"),
    ] {
        let path = w.root.join(name).join(leaf);
        fs::create_dir_all(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        w.env.insert(key.into(), path.into_os_string());
    }
    let result = body(w);
    w.env = old;
    result
}
fn actor(w: &World, repo: &Path, adopt: Option<&str>) -> TestResult<String> {
    let status = good(
        w,
        "scope_binding.py",
        &[os("status"), os("--repo"), repo.as_os_str()],
    )?;
    if get(get(&status, "developer"), "setup_required") == &Value::Bool(true) {
        let p = good(
            w,
            "developer_identity.py",
            &[
                os("preview"),
                os(if adopt.is_some() {
                    "--adopt"
                } else {
                    "--create"
                }),
            ]
            .into_iter()
            .chain(adopt.map(os))
            .chain([os("--label"), os("Isolated matrix developer")])
            .collect::<Vec<_>>(),
        )?;
        let path = save(w, &p, "profile-developer.json")?;
        good(
            w,
            "developer_identity.py",
            &[os("apply"), os("--proposal"), path.as_os_str()],
        )?;
    }
    let status = good(
        w,
        "scope_binding.py",
        &[os("status"), os("--repo"), repo.as_os_str()],
    )?;
    if get(&status, "resolved") != &Value::Bool(true) {
        let p = good(
            w,
            "scope_binding.py",
            &[
                os("preview"),
                os("--repo"),
                repo.as_os_str(),
                os("--action"),
                os("bind"),
                os("--reason"),
                os("Explicit same repository profile binding"),
            ],
        )?;
        let path = save(w, &p, "profile-binding.json")?;
        good(
            w,
            "scope_binding.py",
            &[os("apply"), os("--proposal"), path.as_os_str()],
        )?;
    }
    Ok(field(
        &good(
            w,
            "scope_binding.py",
            &[os("status"), os("--repo"), repo.as_os_str()],
        )?,
        "developer_id",
    )
    .to_owned())
}
struct Actors {
    a: PathBuf,
    b: PathBuf,
    c: PathBuf,
    sibling: PathBuf,
    da: String,
    db: String,
    baseline: String,
    policy: PathBuf,
}
fn actors(w: &mut World) -> TestResult<Actors> {
    let (a, bank, seed, _) = fixture(w)?;
    let baseline =
        "# Project\n\n## Build\nUse the isolated build.\n\n## Review\nReview scoped sources.\n"
            .to_owned();
    fs::write(bank.join("projectbrief.md"), &baseline)?;
    git(w, &a, &["add", "memory-bank"])?;
    git(
        w,
        &a,
        &[
            "commit",
            "--only",
            "-m",
            "Shared matrix sections",
            "--",
            "memory-bank",
        ],
    )?;
    git(w, &a, &["push", "origin", "HEAD:main"])?;
    git(w, &seed, &["pull", "--ff-only"])?;
    let policy = github(w, &a, &bank, Some(&seed), false)?;
    let remote = PathBuf::from(field(&read_policy(&policy)?, "remote"));
    git(w, &a, &["fetch", "origin"])?;
    // These bytes are fixture setup, before any preservation observation.
    fs::write(
        bank.join("MEMORY.md"),
        fs::read(seed.join("memory-bank/MEMORY.md"))?,
    )?;
    let authority = git(w, &a, &["rev-parse", "origin/main"])?;
    let b = w.clone(&remote, "machine-B")?;
    let c = w.clone(&remote, "machine-C")?;
    for repo in [&b, &c] {
        git(
            w,
            repo,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/fixture/memory.git",
            ],
        )?;
    }
    let sibling = w.root.join("B-linked-worktree");
    git(
        w,
        &b,
        &[
            "worktree",
            "add",
            "-b",
            "B-worktree",
            sibling.to_str().unwrap(),
            authority.trim(),
        ],
    )?;
    for (repo, branch, value) in [
        (&a, Some("A-feature"), 10),
        (&b, Some("B-feature"), 20),
        (&c, Some("C-feature"), 30),
        (&sibling, None, 40),
    ] {
        if let Some(branch) = branch {
            git(w, repo, &["checkout", "-b", branch])?;
        }
        fs::write(
            repo.join("code.py"),
            format!("VALUE = {value}  # unfinished feature ancestor\n"),
        )?;
        git(w, repo, &["add", "code.py"])?;
        git(
            w,
            repo,
            &[
                "commit",
                "--only",
                "-m",
                "Private feature ancestor",
                "--",
                "code.py",
            ],
        )?;
        fs::write(
            repo.join("code.py"),
            format!("VALUE = {}  # staged unfinished feature\n", value + 1),
        )?;
        git(w, repo, &["add", "code.py"])?;
        fs::write(
            repo.join("unrelated.txt"),
            format!("Keep {value} session note.\n"),
        )?;
    }
    let da = actor(w, &a, None)?;
    let db = profile(w, "profile-B", |w| {
        let bdev = actor(w, &b, None)?;
        assert_eq!(actor(w, &sibling, None)?, bdev);
        Ok(bdev)
    })?;
    profile(w, "profile-C", |w| {
        assert_eq!(actor(w, &c, Some(&da))?, da);
        Ok(())
    })?;
    assert_ne!(da, db);
    Ok(Actors {
        a,
        b,
        c,
        sibling,
        da,
        db,
        baseline,
        policy,
    })
}
fn fact(
    _w: &World,
    repo: &Path,
    name: &str,
    body: &str,
    developer: &str,
    harness: &str,
    conditions: Value,
) -> TestResult<(Value, String)> {
    let bank = repo.join("memory-bank");
    let project =
        support::record_fixtures::project_id(&fs::read_to_string(bank.join("MEMORY.md"))?)?;
    let raw = support::record_fixtures::record_text_with_ids(
        body,
        text(&project),
        Default::default(),
        &agent_memory::host::uuid4()?,
        developer,
    )?;
    let (_, mut metadata, _) = agent_memory::records::parse(&raw, &[])?;
    set(&mut metadata, "conditions", conditions);
    let origin = metadata.get_mut("origin").unwrap();
    set(origin, "harness", s(harness));
    set(origin, "session", s(format!("{name}-session")));
    let path = format!("topics/{name}.md");
    fs::create_dir_all(bank.join("topics"))?;
    fs::write(
        bank.join(&path),
        agent_memory::codec::frontmatter::write(metadata.as_object().unwrap())? + body,
    )?;
    let router = bank.join("MEMORY.md");
    let row = format!(
        "| [{path}](<{path}>) | Scoped reviewed lesson | repository | active | Before reusing the scoped observation |\n"
    );
    fs::write(
        &router,
        fs::read_to_string(&router)?.replace(
            "\n## Omitted contexts",
            &format!("{row}\n## Omitted contexts"),
        ),
    )?;
    Ok((metadata, path))
}
fn protected(w: &World, repo: &Path) -> TestResult<(Vec<u8>, Vec<u8>, String, String, Vec<u8>)> {
    let index = git(
        w,
        repo,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    Ok((
        fs::read(repo.join("code.py"))?,
        fs::read(index.trim())?,
        git(w, repo, &["rev-parse", "HEAD"])?,
        git(w, repo, &["symbolic-ref", "HEAD"])?,
        fs::read(repo.join("unrelated.txt"))?,
    ))
}
fn reviewed(
    w: &World,
    repo: &Path,
    inspection: &Value,
    relation: &str,
    claims: &[String],
    dependencies: Value,
    consolidate: bool,
) -> TestResult<Value> {
    let mut j = judgment(inspection)?;
    let sources = rows(inspection, "sources");
    let path = |key: &Value| {
        field(
            sources.iter().find(|row| get(row, "key") == key).unwrap(),
            "path",
        )
        .to_owned()
    };
    for pair in j.get_mut("pairs").unwrap().as_array_mut().unwrap() {
        let left = path(get(pair, "left"));
        let right = path(get(pair, "right"));
        if claims.contains(&left) && claims.contains(&right) && left != right {
            set(pair, "classification", s(relation));
        } else if relation == "compatible"
            && left == "projectbrief.md"
            && right == "projectbrief.md"
        {
            set(pair, "classification", s("compatible"));
        }
    }
    set(&mut j, "dependencies", dependencies);
    if consolidate {
        let candidates = sources
            .iter()
            .filter(|row| claims.contains(&field(row, "path").to_owned()))
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), 2);
        let primary = candidates
            .iter()
            .find(|row| field(row, "path") == claims[0])
            .unwrap();
        set(
            &mut j,
            "consolidations",
            Value::Array(vec![obj([
                (
                    "sources",
                    Value::Array(
                        candidates
                            .iter()
                            .map(|row| get(row, "key").clone())
                            .collect(),
                    ),
                ),
                ("primary", get(primary, "key").clone()),
                ("body", s("# Retry cap\n\nRetry at most three times.\n")),
                (
                    "reason",
                    s(
                        "Both complete source claims establish the same cap under identical conditions.",
                    ),
                ),
            ])]),
        )
    }
    let input = save(w, &j, "gap-sharing-judgment.json")?;
    good(
        w,
        "reconcile_memory.py",
        &[
            os("review"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
        ],
    )
}
trait ArrayMut {
    fn as_array_mut(&mut self) -> Option<&mut Vec<Value>>;
}
impl ArrayMut for Value {
    fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        if let Value::Array(rows) = self {
            Some(rows)
        } else {
            None
        }
    }
}
fn publication(
    w: &World,
    repo: &Path,
    relation: &str,
    claims: &[String],
    dependencies: Value,
    consolidate: bool,
) -> TestResult<(Value, Value)> {
    let inspection = good(
        w,
        "reconcile_memory.py",
        &[os("inspect"), os("--repo"), repo.as_os_str()],
    )?;
    let review = reviewed(
        w,
        repo,
        &inspection,
        relation,
        claims,
        dependencies,
        consolidate,
    )?;
    let input = save(w, get(&review, "proof"), "gap-sharing-proof.json")?;
    let prepared = good(
        w,
        "publish_memory.py",
        &[
            os("prepare"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
        ],
    )?;
    Ok((get(&prepared, "request").clone(), review))
}
fn recall(project: &Value, paths: Vec<String>, ids: Vec<Value>, limit: i64) -> Value {
    obj([
        ("project_id", project.clone()),
        ("query", s("")),
        ("ids", Value::Array(ids)),
        ("paths", Value::Array(paths.into_iter().map(s).collect())),
        ("context", obj([("component", s("code.py"))])),
        ("kinds", Value::Array(vec![s("semantic-fact")])),
        ("minimum_trust", s("reviewed")),
        ("limit", int(limit)),
        ("include_context", Value::Bool(false)),
    ])
}
fn prepare_refresh(
    w: &World,
    repo: &Path,
    recall: &Value,
    relation: &str,
    claims: &[String],
    dependencies: Value,
) -> TestResult<(Value, Value)> {
    let view = good(
        w,
        "refresh_memory.py",
        &[os("inspect"), os("--repo"), repo.as_os_str()],
    )?;
    let review = reviewed(
        w,
        repo,
        get(&view, "inspection"),
        relation,
        claims,
        dependencies,
        false,
    )?;
    let request = obj([
        ("proof", get(&review, "proof").clone()),
        ("recall", recall.clone()),
    ]);
    let input = save(w, &request, "gap-refresh-input.json")?;
    let prepared = good(
        w,
        "refresh_memory.py",
        &[
            os("prepare"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
        ],
    )?;
    Ok((get(&prepared, "proposal").clone(), review))
}
fn apply_refresh(w: &World, p: &Value) -> TestResult<Value> {
    let input = save(w, p, "gap-refresh-proposal.json")?;
    good(
        w,
        "refresh_memory.py",
        &[os("apply"), os("--input"), input.as_os_str()],
    )
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_multi_machine_compatible.py::MultiMachineCompatible.test_two_developers_three_clones_and_a_worktree_reconcile_without_importing_feature_code
fn compatible_three_clones_and_linked_worktree() -> TestResult {
    let mut w = World::new()?;
    let f = actors(&mut w)?;
    fs::write(
        f.a.join("memory-bank/projectbrief.md"),
        f.baseline.replace(
            "Use the isolated build.",
            "Use the verified isolated build.",
        ),
    )?;
    fs::write(
        f.b.join("memory-bank/projectbrief.md"),
        f.baseline.replace(
            "Review scoped sources.",
            "Review scoped sources and their evidence.",
        ),
    )?;
    let (first, p1) = fact(
        &w,
        &f.a,
        "A-lesson",
        "# Build lesson\n\nKeep the reviewed build observation conditional.\n",
        &f.da,
        "claude",
        obj([("component", s("code.py"))]),
    )?;
    let (second, p2) = fact(
        &w,
        &f.b,
        "B-lesson",
        "# Review lesson\n\nKeep the independent review observation conditional.\n",
        &f.db,
        "codex",
        obj([("component", s("code.py"))]),
    )?;
    let repos = [&f.a, &f.b, &f.c, &f.sibling];
    let protected_before = repos
        .iter()
        .map(|repo| protected(&w, repo))
        .collect::<TestResult<Vec<_>>>()?;
    let banks =
        [&f.a, &f.b, &f.sibling].map(|r| support::cutover::physical(&r.join("memory-bank")));
    let a = publication(&w, &f.a, "related", &[], obj([]), false)?.0;
    let b = profile(&mut w, "profile-B", |w| {
        Ok(publication(w, &f.b, "related", &[], obj([]), false)?.0)
    })?;
    assert_eq!(
        get(&land(&w, &f.a, &a)?, "publication_verified"),
        &Value::Bool(true)
    );
    let (fresh, landed) = profile(&mut w, "profile-B", |w| {
        let stale = land(w, &f.b, &b)?;
        assert_eq!(field(&stale, "sharing_state"), "pending-reconciliation");
        let fresh = publication(w, &f.b, "compatible", &[], obj([]), false)?.0;
        assert_ne!(get(&fresh, "base"), get(&b, "base"));
        let landed = land(w, &f.b, &fresh)?;
        Ok((fresh, landed))
    })?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    let shared = field(&fresh, "candidate");
    let ancestors = git(&w, &f.b, &["rev-list", shared])?;
    for before in &protected_before {
        assert!(!ancestors.lines().any(|id| id == before.2.trim()));
    }
    assert_eq!(
        git(&w, &f.b, &["show", &format!("{shared}:code.py")])?,
        "VALUE = 1\n"
    );
    let brief = git(
        &w,
        &f.b,
        &["show", &format!("{shared}:memory-bank/projectbrief.md")],
    )?;
    assert!(brief.contains("verified isolated build"));
    assert!(brief.contains("sources and their evidence"));
    let request = recall(
        get(&first, "project-id"),
        vec![],
        vec![get(&first, "id").clone(), get(&second, "id").clone()],
        8,
    );
    profile(&mut w, "profile-C", |w| {
        let p = prepare_refresh(w, &f.c, &request, "related", &[], obj([]))?.0;
        let result = apply_refresh(w, &p)?;
        assert_eq!(
            get(&result, "remote_freshness_verified"),
            &Value::Bool(true)
        );
        assert_eq!(
            get(&result, "feature_code_freshness_verified"),
            &Value::Bool(false)
        );
        assert_eq!(
            get(get(get(&result, "recall"), "scope_context"), "developer_id"),
            &s(&f.da)
        );
        let records = rows(get(&result, "recall"), "results");
        assert_eq!(records.len(), 2);
        let b = records
            .iter()
            .find(|row| get(get(row, "record"), "id") == get(&second, "id"))
            .unwrap();
        assert_eq!(
            get(get(get(b, "record"), "origin"), "developer-id"),
            &s(&f.db)
        );
        assert_eq!(get(get(get(b, "record"), "origin"), "harness"), &s("codex"));
        let mut wrong = request.clone();
        set(&mut wrong, "context", obj([("component", s("other.py"))]));
        let path = save(w, &wrong, "wrong-recall.json")?;
        assert!(
            rows(
                &good(
                    w,
                    "retrieve.py",
                    &[
                        os("--repo"),
                        f.c.as_os_str(),
                        os("--request"),
                        path.as_os_str()
                    ]
                )?,
                "results"
            )
            .is_empty()
        );
        assert_eq!(
            get(&apply_refresh(w, &p)?, "canonical_writes_performed"),
            &Value::Bool(false)
        );
        Ok(())
    })?;
    assert_eq!(fs::read_dir(f.c.join("memory-bank/topics"))?.count(), 2);
    for (repo, before) in repos.iter().zip(protected_before) {
        assert_eq!(protected(&w, repo)?, before);
    }
    assert_eq!(
        [&f.a, &f.b, &f.sibling].map(|r| support::cutover::physical(&r.join("memory-bank"))),
        banks
    );
    assert!(f.policy.exists());
    assert_ne!(p1, p2);
    Ok(())
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_multi_machine_conflicts.py::MultiMachineConflicts.test_detecting_developer_sees_both_origins_without_pausing_independent_memory_or_code
fn detecting_developer_retains_both_origins_and_independent_work() -> TestResult {
    let mut w = World::new()?;
    let f = actors(&mut w)?;
    let (yellow, p1) = fact(
        &w,
        &f.a,
        "yellow",
        "# Wall\n\nThe same room wall should be yellow.\n",
        &f.da,
        "claude",
        obj([]),
    )?;
    let (_, child) = fact(
        &w,
        &f.a,
        "paint-order",
        "# Order\n\nThis order depends on the disputed wall color.\n",
        &f.da,
        "claude",
        obj([]),
    )?;
    let (_, p2) = fact(
        &w,
        &f.b,
        "blue",
        "# Wall\n\nThe same room wall should be blue.\n",
        &f.db,
        "codex",
        obj([]),
    )?;
    let (_, independent) = fact(
        &w,
        &f.b,
        "independent",
        "# Independent\n\nAn independent reviewed observation remains useful.\n",
        &f.db,
        "codex",
        obj([("component", s("code.py"))]),
    )?;
    fs::write(
        f.a.join("memory-bank/activeContext.md"),
        "# Current focus\n\nIndependent local work continues while clarification is pending.\n",
    )?;
    let repos = [&f.a, &f.b, &f.c, &f.sibling];
    let before = repos
        .iter()
        .map(|r| protected(&w, r))
        .collect::<TestResult<Vec<_>>>()?;
    profile(&mut w, "profile-B", |w| {
        let p = publication(w, &f.b, "related", &[], obj([]), false)?.0;
        assert_eq!(
            get(&land(w, &f.b, &p)?, "publication_verified"),
            &Value::Bool(true)
        );
        Ok(())
    })?;
    let claims = [p1.clone(), p2.clone()];
    let dependencies = Value::Object(vec![(child.clone(), Value::Array(vec![s(&p1)]))]);
    let request = recall(
        get(&yellow, "project-id"),
        vec![p1.clone(), p2.clone(), child.clone(), independent.clone()],
        vec![],
        1,
    );
    let (p, review) = prepare_refresh(
        &w,
        &f.a,
        &request,
        "contradiction",
        &claims,
        dependencies.clone(),
    )?;
    assert_eq!(
        get(&review, "recommend_other_developer_clarification"),
        &Value::Bool(true)
    );
    assert!(rows(&review, "other_developer_ids").contains(&s(&f.db)));
    assert_eq!(get(&review, "messages_sent"), &Value::Bool(false));
    assert_eq!(
        get(&review, "independent_updates_may_continue"),
        &Value::Bool(true)
    );
    let result = apply_refresh(&w, &p)?;
    assert_eq!(field(&result, "sharing_state"), "refreshed-with-conflicts");
    assert_eq!(get(&result, "conflicts_pending"), &Value::Bool(true));
    assert_eq!(
        rows(get(&result, "recall"), "results")
            .iter()
            .map(|r| field(r, "path"))
            .collect::<Vec<_>>(),
        vec![independent.as_str()]
    );
    assert!(f.a.join("memory-bank").join(&p1).exists());
    assert!(f.a.join("memory-bank").join(&child).exists());
    assert!(!f.a.join("memory-bank").join(&p2).exists());
    let p = publication(&w, &f.a, "contradiction", &claims, dependencies, false)?.0;
    let landed = land(&w, &f.a, &p)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    let candidate = field(&p, "candidate");
    assert!(
        git(
            &w,
            &f.a,
            &["show", &format!("{candidate}:memory-bank/activeContext.md")]
        )?
        .contains("Independent local work continues")
    );
    let paths = git(
        &w,
        &f.a,
        &[
            "ls-tree",
            "-r",
            "--name-only",
            candidate,
            "--",
            "memory-bank",
        ],
    )?;
    assert!(paths.lines().any(|p| p == format!("memory-bank/{p2}")));
    for blocked in [&p1, &child] {
        assert!(!paths.lines().any(|p| p == format!("memory-bank/{blocked}")));
    }
    for (repo, before) in repos.iter().zip(before) {
        assert_eq!(protected(&w, repo)?, before);
    }
    Ok(())
}

fn indexed(w: &World, repo: &Path, claims: &[String]) -> TestResult<(PathBuf, Value, Value)> {
    let initial = w.root.join("indexed-initial.json");
    good(
        w,
        "reconcile_memory.py",
        &[
            os("inspect-v2"),
            os("--repo"),
            repo.as_os_str(),
            os("--output"),
            initial.as_os_str(),
        ],
    )?;
    let initial = decode(&fs::read(initial)?)?;
    let mut request = get(&initial, "request").clone();
    set(
        &mut request,
        "selections",
        Value::Object(
            rows(&initial, "changed")
                .iter()
                .map(|key| {
                    (
                        text(key).to_owned(),
                        obj([
                            ("searches", Value::Array(vec![s("wall"), s("atrium")])),
                            ("paths", Value::Array(claims.iter().map(s).collect())),
                            ("ids", Value::Array(vec![])),
                            ("complete", Value::Bool(true)),
                            ("uncertain", Value::Bool(false)),
                        ]),
                    )
                })
                .collect(),
        ),
    );
    let input = save(w, &request, "indexed-request.json")?;
    let path = w.root.join("indexed-selected.json");
    good(
        w,
        "reconcile_memory.py",
        &[
            os("inspect-v2"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
            os("--output"),
            path.as_os_str(),
        ],
    )?;
    let view = decode(&fs::read(&path)?)?;
    let sources = rows(&view, "sources");
    let decisions = rows(&view, "required_pairs")
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            let p = pair.as_array().unwrap();
            let paths = p
                .iter()
                .map(|key| {
                    field(
                        sources.iter().find(|r| get(r, "key") == key).unwrap(),
                        "path",
                    )
                    .to_owned()
                })
                .collect::<Vec<_>>();
            obj([
                ("index", Value::Int(index.into())),
                (
                    "classification",
                    s(
                        if paths.len() == 2
                            && paths[0] != paths[1]
                            && paths.iter().all(|p| claims.contains(p))
                        {
                            "contradiction"
                        } else {
                            "related"
                        },
                    ),
                ),
                (
                    "reason",
                    s("Actual fixture meanings were compared and retained."),
                ),
            ])
        })
        .collect();
    let decision = obj([
        (
            "pages",
            Value::Array(
                rows(&view, "pages")
                    .iter()
                    .map(|p| get(p, "sha256").clone())
                    .collect(),
            ),
        ),
        ("pairs", Value::Array(decisions)),
        ("consolidations", Value::Array(vec![])),
        (
            "rationale",
            s("Review every actual source pair and preserve disputed wall colors."),
        ),
    ]);
    Ok((path, view, decision))
}
fn indexed_fixture(w: &World) -> TestResult<(PathBuf, PathBuf, PathBuf, Vec<String>)> {
    let (repo, bank, peer, _) = fixture(w)?;
    let dev = field(
        &good(
            w,
            "scope_binding.py",
            &[os("status"), os("--repo"), repo.as_os_str()],
        )?,
        "developer_id",
    )
    .to_owned();
    let (_, left) = fact(
        w,
        &repo,
        "wall-local",
        "# Atrium wall\n\nUse yellow.\n",
        &dev,
        "claude",
        obj([]),
    )?;
    let (_, right) = fact(
        w,
        &peer,
        "wall-peer",
        "# Atrium wall\n\nUse blue.\n",
        &dev,
        "codex",
        obj([]),
    )?;
    git(w, &peer, &["add", "memory-bank"])?;
    git(w, &peer, &["commit", "-m", "Peer wall claim"])?;
    git(w, &peer, &["push", "origin", "HEAD:main"])?;
    Ok((repo, bank, peer, vec![left, right]))
}
fn indexed_review(
    w: &World,
    repo: &Path,
    path: &Path,
    decision: &Value,
    expected: i32,
) -> TestResult<(PathBuf, Value)> {
    let input = save(w, decision, "indexed-decisions.json")?;
    let output = w.root.join("indexed-review.json");
    if output.exists() {
        fs::remove_file(&output)?;
    }
    let (code, value) = call(
        w,
        "reconcile_memory.py",
        &[
            os("review-indexed-v2"),
            os("--repo"),
            repo.as_os_str(),
            os("--inspection"),
            path.as_os_str(),
            os("--input"),
            input.as_os_str(),
            os("--output"),
            output.as_os_str(),
        ],
    )?;
    assert_eq!(code, expected, "{value:?}");
    Ok((output, value))
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_indexed_review_requires_every_pair_and_preserves_conflict
fn indexed_review_requires_every_pair_and_preserves_conflict() -> TestResult {
    let w = World::new()?;
    let (repo, _, _, claims) = indexed_fixture(&w)?;
    let (path, view, decisions) = indexed(&w, &repo, &claims)?;
    for page in rows(&view, "pages") {
        let exposed = good(
            &w,
            "reconcile_memory.py",
            &[
                os("worklist-v2"),
                os("--repo"),
                repo.as_os_str(),
                os("--input"),
                path.as_os_str(),
                os("--page"),
                os(&get(&page, "index").as_int().unwrap().to_string()),
            ],
        )?;
        assert_eq!(get(&exposed, "page_sha256"), get(&page, "sha256"));
        for item in rows(&exposed, "items") {
            if let Some(edge) = item.get("edge") {
                let index = get(&item, "pair_index").as_int().unwrap().as_u64().unwrap() as usize;
                assert_eq!(&rows(&view, "required_pairs")[index], edge);
            }
        }
    }
    for variant in ["missing", "pages", "duplicate"] {
        let mut bad = decisions.clone();
        if variant == "pages" {
            set(&mut bad, "pages", Value::Array(vec![]));
        } else {
            let pairs = bad.get_mut("pairs").unwrap().as_array_mut().unwrap();
            if variant == "missing" {
                pairs.pop();
            } else {
                pairs.push(pairs[0].clone());
            }
        }
        assert!(!indexed_review(&w, &repo, &path, &bad, 1)?.0.exists());
    }
    let output = indexed_review(&w, &repo, &path, &decisions, 0)?.0;
    let result = decode(&fs::read(output)?)?;
    assert_eq!(get(&result, "conflicts_pending"), &Value::Bool(true));
    for claim in claims {
        assert!(rows(&result, "blocked_paths").contains(&s(claim)));
    }
    Ok(())
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_indexed_review_refuses_changed_source_or_authority
fn indexed_review_refuses_changed_source_or_authority() -> TestResult {
    for authority in [false, true] {
        let w = World::new()?;
        let (repo, bank, peer, claims) = indexed_fixture(&w)?;
        let (path, _, decisions) = indexed(&w, &repo, &claims)?;
        let target = if authority {
            peer.join("memory-bank").join(&claims[1])
        } else {
            bank.join(&claims[0])
        };
        fs::write(
            &target,
            fs::read_to_string(&target)?.replace(
                if authority { "blue" } else { "yellow" },
                if authority { "purple" } else { "green" },
            ),
        )?;
        if authority {
            git(&w, &peer, &["add", "memory-bank"])?;
            git(&w, &peer, &["commit", "-m", "Changed authority source"])?;
            git(&w, &peer, &["push", "origin", "HEAD:main"])?;
        }
        let before = state(&w, &repo, &bank)?;
        let (output, result) = indexed_review(&w, &repo, &path, &decisions, 1)?;
        assert!(!output.exists());
        assert_eq!(get(&result, "publication_verified"), &Value::Bool(false));
        assert_eq!(
            get(&result, "canonical_writes_performed"),
            &Value::Bool(false)
        );
        assert_eq!(state(&w, &repo, &bank)?, before);
    }
    Ok(())
}
#[test]
fn stale_hold_worker() {
    let Some(bank) = std::env::var_os("AGENT_MEMORY_FAULT_GAPS_STALE_HOLD") else {
        return;
    };
    let _: agent_memory::error::Result<()> = agent_memory::local_registry::exclusive(
        &PathBuf::from(bank).join(".agent-memory.lock"),
        &agent_memory::host::uuid4().unwrap(),
        "semantic-review-holds",
        Default::default(),
        |_| std::process::exit(86),
    );
    panic!("stale-lock worker returned");
}
#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_wrong_project_recovery_preserves_private_state_and_stale_bank_marker
fn wrong_project_recovery_preserves_private_state_and_stale_marker() -> TestResult {
    let w = World::new()?;
    let (repo, bank, _, _) = fixture(&w)?;
    let project =
        support::record_fixtures::project_id(&fs::read_to_string(bank.join("MEMORY.md"))?)?;
    let env = [(
        "AGENT_MEMORY_FAULT_GAPS_STALE_HOLD".into(),
        bank.clone().into_os_string(),
    )];
    let output = w.run_with(
        [
            std::env::current_exe()?.into_os_string(),
            "--exact".into(),
            "gaps_multi::stale_hold_worker".into(),
            "--nocapture".into(),
        ],
        RunOptions {
            env: &env,
            check: false,
            ..Default::default()
        },
    )?;
    assert_eq!(output.status, agent_memory::process::Status::Code(86));
    let before = support::lifecycle::snapshot(&w.root);
    let (code, _) = call(
        &w,
        "reconcile_memory.py",
        &[
            os("recover-holds"),
            os("--repo"),
            repo.as_os_str(),
            os("--project-id"),
            os(&agent_memory::host::uuid4()?),
        ],
    )?;
    assert_eq!(code, 1);
    assert_eq!(support::lifecycle::snapshot(&w.root), before);
    assert!(bank.join(".agent-memory.lock").exists());
    let result = good(
        &w,
        "reconcile_memory.py",
        &[
            os("recover-holds"),
            os("--repo"),
            repo.as_os_str(),
            os("--project-id"),
            os(text(&project)),
        ],
    )?;
    assert_eq!(get(&result, "verified"), &Value::Bool(true));
    assert!(!bank.join(".agent-memory.lock").exists());
    Ok(())
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_multi_machine_race.py::MultiMachineRace.test_simultaneous_public_publishers_consolidate_duplicate_learning_before_retry_lands
fn simultaneous_public_publishers_consolidate_before_retry_lands() -> TestResult {
    let mut w = World::new()?;
    let f = actors(&mut w)?;
    let (first, p1) = fact(
        &w,
        &f.a,
        "race-A",
        "# Retry cap\n\nRetry at most three times.\n",
        &f.da,
        "claude",
        obj([]),
    )?;
    let (second, p2) = fact(
        &w,
        &f.b,
        "race-B",
        "# Attempt limit\n\nUse no more than three retries.\n",
        &f.db,
        "codex",
        obj([]),
    )?;
    let repos = [&f.a, &f.b, &f.c, &f.sibling];
    let before = repos
        .iter()
        .map(|repo| protected(&w, repo))
        .collect::<TestResult<Vec<_>>>()?;
    let a = publication(&w, &f.a, "related", &[], obj([]), false)?.0;
    let (b, b_environment) = profile(&mut w, "profile-B", |w| {
        Ok((
            publication(w, &f.b, "related", &[], obj([]), false)?.0,
            w.env.clone(),
        ))
    })?;
    assert_eq!(get(&a, "base"), get(&b, "base"));
    let barrier = w.root.join("real-push-rendezvous");
    fs::create_dir(&barrier)?;
    let mut hooks = Vec::new();
    for (index, repo) in [&f.a, &f.b].iter().enumerate() {
        let hook = PathBuf::from(
            git(
                &w,
                repo,
                &[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-path",
                    "hooks/pre-push",
                ],
            )?
            .trim(),
        );
        let script = format!(
            "#!/bin/sh\nset -eu\ncat > '{}/{index}.ready'\nn=0\nwhile [ ! -f '{}/{}.ready' ]; do\n [ \"$n\" -lt 2000 ] || exit 71\n n=$((n+1)); sleep .01\ndone\n",
            barrier.display(),
            barrier.display(),
            1 - index
        );
        fs::write(&hook, script)?;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
        hooks.push(hook);
    }
    let pa = save(&w, &a, "race-A.json")?;
    let pb = save(&w, &b, "race-B.json")?;
    let argv = |repo: &Path, input: &Path| {
        vec![
            OsString::from(env!("CARGO_BIN_EXE_am")),
            "--json".into(),
            "publish".into(),
            "land".into(),
            "--repo".into(),
            repo.as_os_str().to_owned(),
            "--input".into(),
            input.as_os_str().to_owned(),
        ]
    };
    let child_a = w.spawn_with(
        argv(&f.a, &pa),
        RunOptions {
            check: false,
            ..Default::default()
        },
    )?;
    let saved_environment = std::mem::replace(&mut w.env, b_environment);
    let child_b = w.spawn_with(
        argv(&f.b, &pb),
        RunOptions {
            check: false,
            ..Default::default()
        },
    )?;
    w.env = saved_environment;
    let outputs = [child_a.wait()?, child_b.wait()?];
    for hook in hooks {
        fs::remove_file(hook)?;
    }
    let mut results = Vec::new();
    for (index, output) in outputs.iter().enumerate() {
        assert_eq!(
            output.status,
            agent_memory::process::Status::Code(0),
            "{output:?}"
        );
        results.push(decode(&output.stdout)?);
        let command = fs::read_to_string(barrier.join(format!("{index}.ready")))?;
        let fields = command.split_whitespace().collect::<Vec<_>>();
        assert_eq!(fields.len(), 4);
        let candidate = if index == 0 { &a } else { &b };
        assert_eq!(fields[1], field(candidate, "candidate"));
        assert_eq!(fields[2], "refs/heads/main");
        assert_eq!(fields[3], field(candidate, "base"));
    }
    let winners = results
        .iter()
        .enumerate()
        .filter(|(_, r)| get(r, "publication_verified") == &Value::Bool(true))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(winners.len(), 1);
    let loser = 1 - winners[0];
    assert_eq!(
        field(&results[loser], "sharing_state"),
        "pending-reconciliation"
    );
    let claims = [p1.clone(), p2.clone()];
    let retry = |w: &mut World, repo: &Path| -> TestResult<(Value, Value)> {
        let fresh = publication(w, repo, "equivalent", &claims, obj([]), true)?.0;
        assert_ne!(get(&fresh, "base"), get(&a, "base"));
        let landed = land(w, repo, &fresh)?;
        Ok((fresh, landed))
    };
    let (fresh, landed) = if loser == 0 {
        retry(&mut w, &f.a)?
    } else {
        profile(&mut w, "profile-B", |w| retry(w, &f.b))?
    };
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    let actual = field(&landed, "witnessed_commit");
    let repo = if loser == 0 { &f.a } else { &f.b };
    assert_eq!(
        git(&w, repo, &["ls-remote", "origin", "refs/heads/main"])?
            .split_whitespace()
            .next(),
        Some(actual)
    );
    let read = |path: &str| -> TestResult<Value> {
        let raw = git(&w, repo, &["show", &format!("{actual}:memory-bank/{path}")])?;
        Ok(agent_memory::records::parse(&raw, &[])?.1)
    };
    let current = read(&p1)?;
    let historical = read(&p2)?;
    assert_eq!(get(&current, "id"), get(&first, "id"));
    assert_eq!(get(&current, "version"), &int(2));
    assert_eq!(get(&current, "status"), &s("active"));
    assert_eq!(get(&current, "origin"), get(&first, "origin"));
    assert!(rows(&current, "derived-from").contains(&obj([
        ("id", get(&second, "id").clone()),
        ("version", int(1))
    ])));
    assert_eq!(get(&historical, "status"), &s("superseded"));
    assert_eq!(get(&historical, "origin"), get(&second, "origin"));
    assert_eq!(
        get(
            &read(&format!("history/versions/{}/v1.md", field(&first, "id")))?,
            "origin"
        ),
        get(&first, "origin")
    );
    assert_eq!(
        git(
            &w,
            repo,
            &["show", &format!("{}:code.py", field(&fresh, "candidate"))]
        )?,
        "VALUE = 1\n"
    );
    let ancestors = git(&w, repo, &["rev-list", actual])?;
    for (repo, before) in repos.iter().zip(before) {
        assert!(!ancestors.lines().any(|id| id == before.2.trim()));
        assert_eq!(protected(&w, repo)?, before);
    }
    Ok(())
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_multi_machine_merge_replay.py::MultiMachineMergeReplay.test_erasure_survives_an_ordinary_code_merge_and_pre_erasure_proposal_replay
fn erasure_survives_ordinary_code_merge_and_old_refresh_replay() -> TestResult {
    let w = World::new()?;
    let (repo, bank, peer, _) = fixture(&w)?;
    fs::write(
        repo.join("unrelated.txt"),
        "Keep independent session note.\n",
    )?;
    let dev = field(
        &good(
            &w,
            "scope_binding.py",
            &[os("status"), os("--repo"), repo.as_os_str()],
        )?,
        "developer_id",
    )
    .to_owned();
    let (record, path) = fact(
        &w,
        &peer,
        "erased-after-import",
        "# Old lesson\n\nThis explicitly erased lesson must not return.\n",
        &dev,
        "custom",
        obj([]),
    )?;
    git(&w, &peer, &["add", "memory-bank"])?;
    git(&w, &peer, &["commit", "-m", "Existing shared lesson"])?;
    git(&w, &peer, &["push", "origin", "HEAD:main"])?;
    let request = recall(
        get(&record, "project-id"),
        vec![],
        vec![get(&record, "id").clone()],
        8,
    );
    let old = prepare_refresh(&w, &repo, &request, "related", &[], obj([]))?.0;
    let imported = apply_refresh(&w, &old)?;
    assert_eq!(
        get(&imported, "remote_freshness_verified"),
        &Value::Bool(true)
    );
    assert_eq!(rows(get(&imported, "recall"), "results").len(), 1);
    git(&w, &repo, &["add", "memory-bank"])?;
    git(
        &w,
        &repo,
        &[
            "commit",
            "--only",
            "-m",
            "Ordinary memory checkpoint",
            "--",
            "memory-bank",
        ],
    )?;
    let target = bank.join(&path);
    fs::write(
        &target,
        fs::read_to_string(&target)? + "\nAn unpublished old-body edit cannot reverse erasure.\n",
    )?;
    let deletion = obj([
        ("id", get(&record, "id").clone()),
        ("scope", get(&record, "project-id").clone()),
        ("deleted-at", s(agent_memory::host::utc_now_isoformat()?)),
        ("deletion-authority", s(agent_memory::host::uuid4()?)),
        ("reason-class", s("user-request")),
    ]);
    let input = save(&w, &deletion, "deletion.json")?;
    let tomb = good(
        &w,
        "tombstones.py",
        &[os("render"), os("--input"), input.as_os_str()],
    )?;
    let shared = peer.join("memory-bank");
    fs::remove_file(shared.join(&path))?;
    let tombpath = shared.join(field(&tomb, "path"));
    fs::create_dir_all(tombpath.parent().unwrap())?;
    fs::write(&tombpath, field(&tomb, "content"))?;
    let router = shared.join("MEMORY.md");
    let raw = fs::read_to_string(&router)?
        .split_inclusive('\n')
        .filter(|line| !line.contains(&format!("[{path}]")))
        .collect::<String>();
    let p = field(&tomb, "path");
    let row = format!(
        "| [{p}](<{p}>) | Terminal deletion guard | repository | deleted | Before any reuse |\n"
    );
    fs::write(
        &router,
        raw.replace(
            "\n## Omitted contexts",
            &format!("{row}\n## Omitted contexts"),
        ),
    )?;
    fs::write(
        peer.join("code.py"),
        "VALUE = 900  # authority code awaits ordinary integration\n",
    )?;
    git(&w, &peer, &["add", "--all", "--", "memory-bank", "code.py"])?;
    git(
        &w,
        &peer,
        &[
            "commit",
            "-m",
            "Explicit erasure and independent authority code",
        ],
    )?;
    git(&w, &peer, &["push", "origin", "HEAD:main"])?;
    let before = protected(&w, &repo)?;
    let p = prepare_refresh(&w, &repo, &request, "related", &[], obj([]))?.0;
    let erased = apply_refresh(&w, &p)?;
    assert_eq!(
        get(&erased, "remote_freshness_verified"),
        &Value::Bool(true)
    );
    assert!(rows(get(&erased, "recall"), "results").is_empty());
    assert!(!target.exists());
    assert_eq!(
        fs::read_to_string(bank.join(field(&tomb, "path")))?,
        field(&tomb, "content")
    );
    assert_eq!(protected(&w, &repo)?, before);
    git(&w, &repo, &["add", "code.py", "memory-bank"])?;
    git(
        &w,
        &repo,
        &[
            "commit",
            "-m",
            "Ordinary feature checkpoint after memory refresh",
        ],
    )?;
    git(&w, &repo, &["fetch", "origin"])?;
    let merged = w.git_with(
        [
            "merge",
            "--no-ff",
            "origin/main",
            "-m",
            "Ordinary code integration after erasure",
        ],
        RunOptions {
            cwd: Some(&repo),
            check: false,
            ..Default::default()
        },
    )?;
    if merged.status != agent_memory::process::Status::Code(0) {
        assert_eq!(merged.status, agent_memory::process::Status::Code(1));
        assert_eq!(
            git(&w, &repo, &["diff", "--name-only", "--diff-filter=U"])?.trim(),
            "code.py"
        );
        fs::write(
            repo.join("code.py"),
            "VALUE = 900  # explicit ordinary feature-code resolution\n",
        )?;
        git(&w, &repo, &["add", "code.py"])?;
        git(
            &w,
            &repo,
            &["commit", "-m", "Resolve only ordinary feature conflict"],
        )?;
    }
    assert_eq!(
        git(&w, &repo, &["rev-list", "--parents", "-n", "1", "HEAD"])?
            .split_whitespace()
            .count(),
        3
    );
    good(
        &w,
        "bank_contract.py",
        &[os("validate"), os("--repo"), repo.as_os_str()],
    )?;
    let after_merge = state(&w, &repo, &bank)?;
    let input = save(&w, &old, "pre-erasure-proposal.json")?;
    let (code, replayed) = call(
        &w,
        "refresh_memory.py",
        &[os("apply"), os("--input"), input.as_os_str()],
    )?;
    assert!(code == 0 || code == 1);
    if code == 0 {
        assert_eq!(
            get(&replayed, "canonical_writes_performed"),
            &Value::Bool(false)
        );
    }
    assert_eq!(state(&w, &repo, &bank)?, after_merge);
    let p = prepare_refresh(&w, &repo, &request, "related", &[], obj([]))?.0;
    let fresh = apply_refresh(&w, &p)?;
    assert_eq!(get(&fresh, "remote_freshness_verified"), &Value::Bool(true));
    assert_eq!(
        get(&fresh, "feature_code_freshness_verified"),
        &Value::Bool(false)
    );
    assert!(rows(get(&fresh, "recall"), "results").is_empty());
    assert!(!target.exists());
    assert_eq!(
        fs::read_to_string(bank.join(field(&tomb, "path")))?,
        field(&tomb, "content")
    );
    let after = state(&w, &repo, &bank)?;
    assert_eq!(after.1, after_merge.1);
    assert_eq!(after.2, after_merge.2);
    assert!(fs::read_to_string(repo.join("code.py"))?.contains("VALUE = 900"));
    Ok(())
}

#[test]
#[ignore = "needs recovery_frontier registered"]
// tests/contracts/test_shared_learning_loop.py::SharedLearningLoopContract.test_automatic_durable_capture_lands_and_another_actor_recalls_it_without_feature_code
fn automatic_durable_capture_lands_and_another_actor_recalls_without_code() -> TestResult {
    let mut w = World::new()?;
    let f = actors(&mut w)?;
    let project = get(
        &good(
            &w,
            "scope_binding.py",
            &[os("status"), os("--repo"), f.a.as_os_str()],
        )?,
        "project_id",
    )
    .clone();
    let p = prepare_refresh(
        &w,
        &f.a,
        &recall(&project, vec![], vec![], 8),
        "related",
        &[],
        obj([]),
    )?
    .0;
    apply_refresh(&w, &p)?;
    let evidence = f.a.join("evidence.md");
    fs::write(
        &evidence,
        "# Confirmed fixture operator correction\n\nFor the named component use the recorded retry cap.\n",
    )?;
    let mut candidate = decode(include_bytes!(
        "../../skills/agent-memory/templates/record-proposal.example.json"
    ))?;
    let m = candidate.get_mut("metadata").unwrap();
    set(m, "project-id", project.clone());
    set(m, "conditions", obj([("component", s("code.py"))]));
    set(m, "scope", s("repository"));
    set(
        m,
        "source",
        obj([
            ("kind", s("verified-repository")),
            ("actor", s("Explicit reviewed fixture source")),
        ]),
    );
    let origin = m.get_mut("origin").unwrap();
    set(origin, "developer-id", s(&f.da));
    set(origin, "harness", s("claude-fixture"));
    set(origin, "session", s("teacher-feature-session"));
    set(origin, "code-state", s("not-code"));
    let request = obj([
        ("project_id", project.clone()),
        ("candidate", candidate),
        ("source_class", s("verified-repository")),
        (
            "repository_evidence",
            obj([
                ("path", s("evidence.md")),
                (
                    "sha256",
                    s(
                        agent_memory::developer_identity::digest(Some(&fs::read(&evidence)?))
                            .unwrap(),
                    ),
                ),
            ]),
        ),
        (
            "assessment",
            obj([
                ("category", s("durable-discovery")),
                (
                    "utility",
                    s("Retain the confirmed retry cap for the named component."),
                ),
                ("durable", Value::Bool(true)),
                ("evidence_confidence", s("confirmed")),
                (
                    "evidence_reason",
                    s("The exact repository correction was reviewed."),
                ),
                ("confirmed", Value::Bool(true)),
                ("source_redundant", Value::Bool(false)),
                ("work_state_copy", Value::Bool(false)),
                (
                    "review",
                    s("Reviewed actual supporting bytes and their bounded applicability."),
                ),
            ]),
        ),
    ]);
    let input = save(&w, &request, "capture-request.json")?;
    let captured = good(
        &w,
        "admit_learning.py",
        &[
            os("capture"),
            os("--repo"),
            f.a.as_os_str(),
            os("--request"),
            input.as_os_str(),
        ],
    )?;
    let compared = obj([
        ("project_id", project.clone()),
        (
            "candidate",
            get(get(get(&captured, "capture"), "request"), "candidate").clone(),
        ),
        (
            "searches",
            Value::Array(vec![s("retry policy"), s("attempt cap")]),
        ),
        ("read_paths", Value::Array(vec![])),
    ]);
    let input = save(&w, &compared, "capture-comparison.json")?;
    let view = good(
        &w,
        "compare_learning.py",
        &[
            os("inspect"),
            os("--repo"),
            f.a.as_os_str(),
            os("--request"),
            input.as_os_str(),
        ],
    )?;
    let j = obj([
        ("observation", get(&view, "observation").clone()),
        (
            "decisions",
            Value::Array(
                rows(&view, "matches")
                    .iter()
                    .map(|row| {
                        obj([
                            ("path", get(row, "path").clone()),
                            ("classification", s("distinct-knowledge")),
                            (
                                "rationale",
                                s("The authored source differs from this repository correction."),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("pending_decisions", Value::Array(vec![])),
        (
            "rationale",
            s("Reviewed all actual plausible source conditions before durable admission."),
        ),
    ]);
    let judgment = save(&w, &j, "capture-judgment.json")?;
    let proof = good(
        &w,
        "compare_learning.py",
        &[
            os("review"),
            os("--repo"),
            f.a.as_os_str(),
            os("--request"),
            input.as_os_str(),
            os("--judgment"),
            judgment.as_os_str(),
        ],
    )?;
    let capture = save(&w, get(&captured, "capture"), "capture.json")?;
    let proof = save(&w, get(&proof, "proof"), "capture-proof.json")?;
    let prepared = good(
        &w,
        "admit_learning.py",
        &[
            os("preview"),
            os("--repo"),
            f.a.as_os_str(),
            os("--capture"),
            capture.as_os_str(),
            os("--request"),
            input.as_os_str(),
            os("--proof"),
            proof.as_os_str(),
        ],
    )?;
    let proposal = save(&w, get(&prepared, "proposal"), "capture-proposal.json")?;
    let admitted = good(
        &w,
        "admit_learning.py",
        &[os("apply"), os("--proposal"), proposal.as_os_str()],
    )?;
    assert_eq!(get(&admitted, "admission_verified"), &Value::Bool(true));
    let before = state(&w, &f.a, &f.a.join("memory-bank"))?;
    let publication = publication(&w, &f.a, "related", &[], obj([]), false)?.0;
    assert_eq!(
        get(&land(&w, &f.a, &publication)?, "publication_verified"),
        &Value::Bool(true)
    );
    assert_eq!(state(&w, &f.a, &f.a.join("memory-bank"))?, before);
    let candidate = field(&publication, "candidate");
    assert!(
        !git(&w, &f.a, &["rev-list", candidate])?
            .lines()
            .any(|id| id == before.3.trim())
    );
    assert_eq!(
        git(&w, &f.a, &["show", &format!("{candidate}:code.py")])?,
        "VALUE = 1\n"
    );
    let id = get(get(get(&request, "candidate"), "metadata"), "id").clone();
    let path = format!("topics/{}.md", text(&id));
    assert!(!f.b.join("memory-bank").join(&path).exists());
    let protected = protected(&w, &f.b)?;
    let received = profile(&mut w, "profile-B", |w| {
        let request = recall(&project, vec![], vec![id.clone()], 8);
        let p = prepare_refresh(w, &f.b, &request, "related", &[], obj([]))?.0;
        apply_refresh(w, &p)
    })?;
    assert_eq!(
        get(&received, "remote_freshness_verified"),
        &Value::Bool(true)
    );
    assert_eq!(
        get(&received, "feature_code_freshness_verified"),
        &Value::Bool(false)
    );
    assert_eq!(
        get(
            get(get(&received, "recall"), "scope_context"),
            "developer_id"
        ),
        &s(&f.db)
    );
    let rows = rows(get(&received, "recall"), "results");
    let record = get(&rows[0], "record");
    assert_eq!(get(get(record, "origin"), "developer-id"), &s(&f.da));
    assert_eq!(get(get(record, "origin"), "harness"), &s("claude-fixture"));
    assert_eq!(
        get(record, "conditions"),
        &obj([("component", s("code.py"))])
    );
    assert_eq!(
        fs::read(f.a.join("memory-bank").join(&path))?,
        fs::read(f.b.join("memory-bank").join(&path))?
    );
    assert_eq!(self::protected(&w, &f.b)?, protected);
    assert_eq!(state(&w, &f.a, &f.a.join("memory-bank"))?, before);
    Ok(())
}
