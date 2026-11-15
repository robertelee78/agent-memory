//! Memory publication and source-preserving layout publication contracts.
//!
//! The offline GitHub protocol fixture (`publication_github.py`) is ported as
//! two re-executed workers of this test binary: `gh` and the bare remote's
//! pre-receive guard are POSIX shell shims that hand their arguments and stdin
//! to `github_fixture_worker` / `github_guard_worker`. No credentials, sockets
//! or interpreters are involved; Git transport stays restricted to `file`.
#[allow(unused_imports)]
mod support;

use agent_memory::codec::json::{self, DecodePolicy, Value};
use agent_memory::layout_recovery_publication as layout;
use agent_memory::memory_publication as publication;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::world::native_arguments;
use support::{RunOptions, World, WorldError};

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const USER_ID: i64 = 7;

// ---------------------------------------------------------------- values

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn int(value: i64) -> Value {
    Value::Int(value.into())
}
fn obj<const N: usize>(values: [(&str, Value); N]) -> Value {
    Value::Object(values.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}
fn set(value: &mut Value, key: &str, new: Value) {
    let Value::Object(pairs) = value else {
        panic!("test object required")
    };
    if let Some((_, old)) = pairs.iter_mut().find(|(k, _)| k == key) {
        *old = new;
    } else {
        pairs.push((key.into(), new));
    }
}
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
fn decode(raw: &[u8]) -> TestResult<Value> {
    Ok(json::decode_bytes(raw, DecodePolicy::default())?)
}
fn encode(value: &Value) -> TestResult<String> {
    Ok(json::encode(value, json::DEFAULT)?)
}
fn user(id: i64, login: &str) -> Value {
    obj([("id", int(id)), ("login", s(login)), ("type", s("User"))])
}

// ---------------------------------------------------------------- world helpers

fn save(w: &World, value: &Value, name: &str) -> TestResult<PathBuf> {
    let path = w.root.join(name);
    fs::write(&path, agent_memory::developer_identity::encoded(value)?)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    Ok(path)
}
fn git(w: &World, cwd: &Path, args: &[&str]) -> TestResult<String> {
    let output = w.run_with(
        std::iter::once(os("git")).chain(args.iter().map(|arg| os(arg))),
        RunOptions {
            cwd: Some(cwd),
            ..RunOptions::default()
        },
    )?;
    Ok(String::from_utf8(output.stdout)?)
}
fn call(w: &World, command: &str, args: &[&OsStr]) -> TestResult<(i32, Value)> {
    let argv: Vec<OsString> = [
        w.env[OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone(),
        OsString::from("--json"),
    ]
    .into_iter()
    .chain(native_arguments(command, args)?)
    .collect();
    let output = w.run_with(
        argv,
        RunOptions {
            check: false,
            timeout: Duration::from_secs(120),
            ..RunOptions::default()
        },
    )?;
    let code = match output.status {
        agent_memory::process::Status::Code(code) => code,
        agent_memory::process::Status::Signal(signal) => -signal,
    };
    let value = decode(&output.stdout).map_err(|error| {
        WorldError::invalid(format!(
            "{command} produced no JSON ({error}): stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    })?;
    Ok((code, value))
}
fn good(w: &World, command: &str, args: &[&OsStr]) -> TestResult<Value> {
    let (code, value) = call(w, command, args)?;
    assert_eq!(code, 0, "{command}: {}", encode(&value)?);
    Ok(value)
}
fn install_runtime(w: &mut World) {
    // Baseline layout_recovery_fixtures.installed:74-76 chooses a group
    // reproducible by this principal before any original bank exists.
    let mut directories = vec![w.root.clone()];
    while let Some(directory) = directories.pop() {
        std::os::unix::fs::chown(&directory, None, Some(agent_memory::host::effective_gid()))
            .unwrap();
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                directories.push(entry.path());
            }
        }
    }
    let runtime = support::installed_runtime::install(&w.root, |argv| {
        w.run_with(
            argv,
            RunOptions {
                check: false,
                timeout: Duration::from_secs(120),
                ..Default::default()
            },
        )
        .unwrap()
    });
    w.env.insert(
        "AGENT_MEMORY_TEST_OWNED_COMMAND".into(),
        runtime.command.into_os_string(),
    );
}

/// Execute the receipt-owned native command, including hooks' bounded stdin.
fn native_dispatch(w: &World, args: &[OsString], event: Option<&Value>) -> (i32, String, String) {
    use agent_memory::process::{self, Input, Spec, Status, Target};
    let cwd = w.root.join("repo");
    support::require_world(&w.env, &cwd).unwrap();
    let argv: Vec<_> =
        std::iter::once(w.env[OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone())
            .chain(args.iter().cloned())
            .collect();
    let env: Vec<_> = w
        .env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let payload = event.map(|value| json::encode(value, json::DEFAULT).unwrap().into_bytes());
    let timeout = support::world::DEFAULT_TIMEOUT;
    let spec = Spec {
        argv: &argv,
        search_path: false,
        env: &env,
        cwd: Some(&cwd),
        stdin: payload.as_deref().map_or(Input::Null, Input::Bytes),
        stdout: Target::Capture {
            limit: support::world::CAPTURE_LIMIT,
        },
        stderr: Target::Capture {
            limit: support::world::CAPTURE_LIMIT,
        },
        deadline: agent_memory::clock::Deadline::after(timeout).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    };
    let output = process::run_for(&spec, timeout).unwrap();
    let code = match output.status {
        Status::Code(code) => code,
        Status::Signal(signal) => -signal,
    };
    (
        code,
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}
fn os(value: &str) -> &OsStr {
    OsStr::new(value)
}

/// Bank bytes, staged feature code, original index, HEAD and branch.
type State = (BTreeMap<String, Vec<u8>>, Vec<u8>, Vec<u8>, String, String);
fn files_below(root: &Path, prefix: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> TestResult {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            files_below(&path, prefix, out)?;
        } else if kind.is_file() {
            let name = path.strip_prefix(prefix)?.to_string_lossy().into_owned();
            out.insert(name, fs::read(&path)?);
        }
    }
    Ok(())
}
fn state(w: &World, repo: &Path, bank: &Path) -> TestResult<State> {
    let index = git(
        w,
        repo,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let mut files = BTreeMap::new();
    files_below(bank, bank, &mut files)?;
    Ok((
        files,
        fs::read(repo.join("code.py"))?,
        fs::read(index.trim())?,
        git(w, repo, &["rev-parse", "HEAD"])?,
        git(w, repo, &["symbolic-ref", "HEAD"])?,
    ))
}
fn remote_head(w: &World, repo: &Path) -> TestResult<String> {
    git(w, repo, &["ls-remote", "origin", "refs/heads/main"])
}

// ---------------------------------------------------------------- repository fixtures

/// Public identity setup; a refused command reports its JSON, not raw bytes.
fn bind(w: &World, repo: &Path) -> TestResult {
    support::identity_fixtures::bind_with_command(
        w,
        repo,
        Path::new(&w.env[OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")]),
    )
    .map_err(|error| match error {
        WorldError::Check(output) => WorldError::invalid(format!(
            "identity setup refused: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )),
        other => other,
    })?;
    Ok(())
}
fn router_proposal() -> TestResult<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skills/agent-memory/templates/router-proposal.example.json");
    let mut proposal = decode(&fs::read(path)?)?;
    set(&mut proposal, "project-id", s(agent_memory::host::uuid4()?));
    Ok(proposal)
}
fn render_bank(w: &World, bank: &Path) -> TestResult {
    let input = save(w, &router_proposal()?, "router-proposal.json")?;
    let rendered = good(
        w,
        "am bank",
        &[os("render"), os("--input"), input.as_os_str()],
    )?;
    fs::create_dir_all(bank)?;
    for (path, body) in get(&rendered, "files").as_object().unwrap_or(&[]) {
        let target = bank.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, text(body))?;
    }
    Ok(())
}

/// Baseline `SharingBoundaryContract.fixture`.
fn boundary_fixture(w: &mut World, bank_root: &str) -> TestResult<(PathBuf, PathBuf)> {
    install_runtime(w);
    let repo = w.repository("repo", false)?;
    let remote = w.repository("remote.git", true)?;
    let remote_text = remote.to_string_lossy().into_owned();
    git(w, &repo, &["remote", "add", "origin", &remote_text])?;
    git(
        w,
        &repo,
        &["config", "--local", "agent-memory.authorityBranch", "main"],
    )?;
    if bank_root != "memory-bank" {
        git(
            w,
            &repo,
            &["config", "--local", "agent-memory.bankRoot", bank_root],
        )?;
    }
    let bank = repo.join(bank_root);
    render_bank(w, &bank)?;
    fs::write(repo.join("code.py"), "VALUE = 1\n")?;
    bind(w, &repo)?;
    git(w, &repo, &["add", bank_root, "code.py"])?;
    git(w, &repo, &["commit", "-m", "Fixture canonical memory"])?;
    fs::write(repo.join("code.py"), "VALUE = 2  # staged work\n")?;
    git(w, &repo, &["add", "code.py"])?;
    Ok((repo, bank))
}

/// Baseline `SharingRecoveryContract.fixture`: `(repo, bank, peer, base)`.
fn fixture(w: &mut World) -> TestResult<(PathBuf, PathBuf, PathBuf, String)> {
    let (repo, bank) = boundary_fixture(w, "memory-bank")?;
    git(w, &repo, &["reset", "--", "code.py"])?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nA useful isolated session focus.\n",
    )?;
    let router = bank.join("MEMORY.md");
    let original = fs::read_to_string(&router)?;
    let kept: String = original
        .split_inclusive('\n')
        .filter(|line| !line.starts_with("- `activeContext.md`:"))
        .collect();
    let routed = kept.replace(
        "\n## Omitted contexts\n",
        "| [activeContext.md](<activeContext.md>) | Session focus | repository | active | At a boundary |\n\n## Omitted contexts\n",
    );
    fs::write(&router, routed)?;
    git(w, &repo, &["add", "memory-bank"])?;
    git(w, &repo, &["commit", "-m", "Useful fixture context"])?;
    git(w, &repo, &["add", "code.py"])?;
    git(w, &repo, &["push", "origin", "HEAD:main"])?;
    let remote = git(w, &repo, &["remote", "get-url", "origin"])?;
    let peer = w.clone(remote.trim(), "peer")?;
    let base = git(w, &repo, &["rev-parse", "HEAD"])?.trim().to_owned();
    Ok((repo, bank, peer, base))
}

/// Baseline `PublicationFixture.bootstrap`: first bank, unfinished feature ancestry.
fn bootstrap(w: &mut World) -> TestResult<(PathBuf, PathBuf, String)> {
    install_runtime(w);
    let repo = w.repository("repo", false)?;
    let remote = w.repository("remote.git", true)?;
    git(
        w,
        &repo,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    )?;
    git(
        w,
        &repo,
        &["config", "--local", "agent-memory.authorityBranch", "main"],
    )?;
    fs::write(repo.join("code.py"), "VALUE = 1\n")?;
    git(w, &repo, &["add", "code.py"])?;
    git(w, &repo, &["commit", "-m", "Initial authority code only"])?;
    git(w, &repo, &["push", "origin", "HEAD:main"])?;
    let base = git(w, &repo, &["rev-parse", "HEAD"])?.trim().to_owned();
    let bank = repo.join("memory-bank");
    render_bank(w, &bank)?;
    bind(w, &repo)?;
    git(w, &repo, &["checkout", "-b", "feature"])?;
    fs::write(
        repo.join("code.py"),
        "VALUE = 7  # unfinished feature ancestor\n",
    )?;
    git(w, &repo, &["add", "code.py"])?;
    git(w, &repo, &["commit", "-m", "Unfinished feature code"])?;
    fs::write(repo.join("code.py"), "VALUE = 8  # staged feature work\n")?;
    git(w, &repo, &["add", "code.py"])?;
    fs::write(repo.join("unrelated.txt"), "Unstaged independent work\n")?;
    Ok((repo, bank, base))
}

/// Baseline `SemanticSharingContract.judgment` with every pair related.
fn judgment(inspection: &Value) -> TestResult<Value> {
    let pairs = get(inspection, "required_pairs")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .map(|pair| {
            let row = pair.as_array().unwrap_or(&[]);
            obj([
                ("left", row.first().cloned().unwrap_or(Value::Null)),
                ("right", row.get(1).cloned().unwrap_or(Value::Null)),
                ("classification", s("related")),
                (
                    "reason",
                    s("Actual complete source meaning reviewed for this isolated case."),
                ),
            ])
        })
        .collect();
    Ok(obj([
        ("binding", get(inspection, "binding").clone()),
        ("pairs", Value::Array(pairs)),
        ("dependencies", obj([])),
        ("consolidations", Value::Array(vec![])),
        (
            "rationale",
            s("Preserve actual source conditions and every origin; no landing-order truth."),
        ),
        ("recorded_at", s(agent_memory::host::utc_now_isoformat()?)),
    ]))
}
fn proof(w: &World, repo: &Path) -> TestResult<Value> {
    let inspection = good(
        w,
        "am semantic",
        &[os("inspect"), os("--repo"), repo.as_os_str()],
    )?;
    let input = save(w, &judgment(&inspection)?, "judgment.json")?;
    let reviewed = good(
        w,
        "am semantic",
        &[
            os("review"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
        ],
    )?;
    Ok(get(&reviewed, "proof").clone())
}
fn prepared(w: &World, repo: &Path) -> TestResult<Value> {
    let input = save(w, &proof(w, repo)?, "proof.json")?;
    good(
        w,
        "am publish",
        &[
            os("prepare"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
        ],
    )
}
fn publish(w: &World, repo: &Path, operation: &str, value: &Value) -> TestResult<(i32, Value)> {
    let input = save(w, value, "publication-request.json")?;
    call(
        w,
        "am publish",
        &[
            os(operation),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
        ],
    )
}
fn land(w: &World, repo: &Path, request: &Value) -> TestResult<Value> {
    let (code, value) = publish(w, repo, "land", request)?;
    assert_eq!(code, 0, "{}", encode(&value)?);
    Ok(value)
}

// ---------------------------------------------------------------- GitHub protocol fixture

const GH_SHIM: &str = r#"#!/bin/sh
call=$(mktemp -d "$AGENT_MEMORY_TEST_ROOT/gh-call.XXXXXX") || exit 70
cat > "$call/stdin"
i=0
for arg do
  printf '%s' "$arg" > "$call/arg.$i"
  i=$((i+1))
done
printf '%s' "$i" > "$call/argc"
AM_GITHUB_FIXTURE_CALL="$call" "$AM_PUBLICATION_TEST_EXE" --exact github_fixture_worker --nocapture --test-threads=1 >/dev/null 2>&1
cat "$call/response" 2>/dev/null
code=$(cat "$call/code" 2>/dev/null || echo 70)
rm -rf "$call"
exit "$code"
"#;
const GUARD_SHIM: &str = r#"#!/bin/sh
input=$(mktemp "$AGENT_MEMORY_TEST_ROOT/guard.XXXXXX") || exit 1
cat > "$input"
AM_GITHUB_GUARD_INPUT="$input" "$AM_PUBLICATION_TEST_EXE" --exact github_guard_worker --nocapture --test-threads=1 >/dev/null 2>&1
code=$?
rm -f "$input"
exit $code
"#;

fn executable(path: &Path, body: &str) -> TestResult {
    fs::write(path, body)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}
fn real_git(w: &World) -> TestResult<PathBuf> {
    let search = w.env.get(OsStr::new("PATH")).cloned().unwrap_or_default();
    std::env::split_paths(&search)
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .ok_or_else(|| WorldError::invalid("Git is required").into())
}
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Baseline `publication_github.install`: policy state, `gh` and a Git transport rewrite.
fn install(w: &mut World, repo: &Path, remote: &Path, protected: bool) -> TestResult<PathBuf> {
    let git_binary = real_git(w)?;
    let policy = w.root.join("github-policy.json");
    let initial = obj([
        ("repo", s(repo.to_string_lossy())),
        ("remote", s(remote.to_string_lossy())),
        ("protected", Value::Bool(protected)),
        (
            "review",
            if protected {
                s("REVIEW_REQUIRED")
            } else {
                Value::Null
            },
        ),
        ("merge_state", s("CLEAN")),
        ("checks", s("SUCCESS")),
        ("rules", Value::Array(vec![])),
        ("push", Value::Bool(true)),
        ("pulls", Value::Array(vec![])),
        ("events", Value::Array(vec![])),
        ("real_git", s(git_binary.to_string_lossy())),
    ]);
    fs::write(&policy, encode(&initial)?)?;
    let binary = w.root.join("fixture-bin");
    fs::create_dir_all(&binary)?;
    executable(&binary.join("gh"), GH_SHIM)?;
    let rewrite = format!(
        "url.{}.insteadOf=https://github.com/fixture/memory.git",
        remote.to_string_lossy()
    );
    let transport = format!(
        "#!/bin/sh\nfor arg do\n case \"$arg\" in\n fetch|pull|push|ls-remote) exec {} -c {} \"$@\" ;;\n esac\ndone\nexec {} \"$@\"\n",
        shell_quote(&git_binary.to_string_lossy()),
        shell_quote(&rewrite),
        shell_quote(&git_binary.to_string_lossy()),
    );
    executable(&binary.join("git"), &transport)?;
    let mut search = vec![binary];
    if let Some(path) = w.env.get(OsStr::new("PATH")) {
        search.extend(std::env::split_paths(path));
    }
    w.env.insert("PATH".into(), std::env::join_paths(search)?);
    w.env.insert(
        "AM_PUBLICATION_TEST_EXE".into(),
        std::env::current_exe()?.into_os_string(),
    );
    Ok(policy)
}
fn enforce(remote: &Path) -> TestResult {
    executable(&remote.join("hooks/pre-receive"), GUARD_SHIM)
}
fn read_policy(policy: &Path) -> TestResult<Value> {
    decode(&fs::read(policy)?)
}
fn edit_policy(policy: &Path, values: &[(&str, Value)]) -> TestResult {
    let mut current = read_policy(policy)?;
    for (key, value) in values {
        set(&mut current, key, value.clone());
    }
    fs::write(policy, encode(&current)?)?;
    Ok(())
}

/// Baseline `PublicationFixture.github`.
fn github(
    w: &mut World,
    repo: &Path,
    bank: &Path,
    peer: Option<&Path>,
    protected: bool,
) -> TestResult<PathBuf> {
    let remote = PathBuf::from(git(w, repo, &["remote", "get-url", "origin"])?.trim());
    let policy = install(w, repo, &remote, protected)?;
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
    let proposal = good(
        w,
        "am identity",
        &[
            os("preview"),
            os("--repo"),
            repo.as_os_str(),
            os("--action"),
            os("rebind"),
            os("--reason"),
            os("Explicit isolated transport fixture for the same project GitHub authority"),
        ],
    )?;
    let path = save(w, &proposal, "rebind-proposal.json")?;
    good(
        w,
        "am identity",
        &[os("apply"), os("--proposal"), path.as_os_str()],
    )?;
    if let Some(peer) = peer {
        fs::write(
            peer.join("memory-bank/MEMORY.md"),
            fs::read(bank.join("MEMORY.md"))?,
        )?;
        git(w, peer, &["add", "memory-bank/MEMORY.md"])?;
        git(
            w,
            peer,
            &[
                "commit",
                "-m",
                "Explicit canonical authority rebind fixture",
            ],
        )?;
        git(w, peer, &["push", "origin", "HEAD:main"])?;
    }
    enforce(&remote)?;
    Ok(policy)
}

/// Holds the fixture state lock until dropped (the descriptor owns the flock).
struct StateLock(#[allow(dead_code)] fs::File);
impl StateLock {
    fn acquire(path: &Path) -> TestResult<Self> {
        use std::os::fd::AsRawFd;
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        // The fixture file descriptor is owned and open for the whole call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(file))
    }
}

fn fixture_git(d: &Value, args: &[&str]) -> TestResult<String> {
    let output = std::process::Command::new(text(get(d, "real_git")))
        .arg("-C")
        .arg(text(get(d, "remote")))
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(WorldError::invalid(format!("fixture git failed: {args:?}")).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn counter(d: &mut Value, key: &str) -> i64 {
    let next = get(d, key).as_int().and_then(|n| n.as_i64()).unwrap_or(0) + 1;
    set(d, key, int(next));
    next
}
fn page_of(endpoint: &str) -> i64 {
    endpoint
        .rsplit("page=")
        .next()
        .and_then(|page| page.parse().ok())
        .unwrap_or(0)
}
fn page_slice(rows: &[Value], page: i64) -> Value {
    let start = ((page - 1).max(0) * 100) as usize;
    let end = (page.max(0) * 100) as usize;
    Value::Array(
        rows.iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .cloned()
            .collect(),
    )
}

/// One GitHub response: `(status, body, extra header lines)`.
type Response = (i64, Value, String);

/// Baseline `publication_github.SERVER`, one call against the shared state.
fn github_server(d: &mut Value, args: &[String], stdin: &[u8]) -> TestResult<Response> {
    let position = |needle: &str| args.iter().position(|arg| arg == needle);
    let endpoint = position("X-GitHub-Api-Version: 2026-03-10")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_default();
    let method = position("--method")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_default();
    let body = if position("--input").is_some() {
        decode(stdin)?
    } else {
        Value::Null
    };
    assert_eq!(
        args.get(..3),
        Some(
            &[
                "api".to_owned(),
                "--hostname".to_owned(),
                "github.com".to_owned()
            ][..]
        )
    );
    let mut events = get(d, "events").as_array().unwrap_or(&[]).to_vec();
    events.push(obj([
        ("endpoint", s(endpoint.clone())),
        ("method", s(method.clone())),
        ("body", body.clone()),
    ]));
    let event_count = events.len();
    set(d, "events", Value::Array(events));
    let repo = obj([
        ("full_name", s("fixture/memory")),
        ("default_branch", s("main")),
        (
            "permissions",
            obj([
                ("push", get(d, "push").clone()),
                ("admin", Value::Bool(true)),
            ]),
        ),
        ("archived", Value::Bool(false)),
        ("disabled", Value::Bool(false)),
        ("allow_merge_commit", Value::Bool(true)),
        ("pushed_at", int(event_count as i64)),
    ]);
    let truthy = |value: &Value| !matches!(value, Value::Null | Value::Bool(false));
    if truthy(get(d, "unavailable")) {
        return Ok((
            403,
            obj([("message", s("Fixture permission failure"))]),
            String::new(),
        ));
    }
    if endpoint == "user" {
        let reads = counter(d, "actor_reads");
        if get(d, "change_actor_at").as_int().and_then(|n| n.as_i64()) == Some(reads) {
            set(d, "actor", user(8, "other-user"));
        }
        if truthy(get(d, "actor_unavailable")) {
            return Ok((
                403,
                obj([("message", s("Actor unavailable"))]),
                String::new(),
            ));
        }
        let status = get(d, "actor_status")
            .as_int()
            .and_then(|n| n.as_i64())
            .unwrap_or(200);
        let actor = d
            .get("actor")
            .cloned()
            .unwrap_or_else(|| user(USER_ID, "allowed-user"));
        return Ok((status, actor, String::new()));
    }
    let allowlist = "repos/fixture/memory/branches/main/protection/restrictions/users?";
    if endpoint.starts_with(allowlist) {
        let page = page_of(&endpoint);
        if get(d, "allowlist_failed_page")
            .as_int()
            .and_then(|n| n.as_i64())
            == Some(page)
        {
            return Ok((
                403,
                obj([("message", s("Allowlist unavailable"))]),
                String::new(),
            ));
        }
        let rows = d
            .get("allowed_users")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        let Value::Array(rows) = rows else {
            return Ok((200, rows, String::new()));
        };
        let mut link = String::new();
        if rows.len() as i64 > page * 100 {
            let mut target = format!(
                "https://api.github.com/repos/fixture/memory/branches/main/protection/restrictions/users?per_page=100&page={}",
                page + 1
            );
            if truthy(get(d, "broken_next")) {
                target = "https://api.github.com/repos/fixture/memory/branches/main/protection/restrictions/users?per_page=100&page=99".to_owned();
            }
            link = format!("Link: <{target}>; rel=\"next\"\n");
        }
        let status = get(get(d, "allowlist_status"), &page.to_string())
            .as_int()
            .and_then(|n| n.as_i64())
            .unwrap_or(200);
        return Ok((status, page_slice(&rows, page), link));
    }
    if endpoint == "repos/fixture/memory" {
        return Ok((200, repo, String::new()));
    }
    if endpoint == "repos/fixture/memory/branches/main" {
        let sha = fixture_git(d, &["rev-parse", "refs/heads/main"])?;
        return Ok((
            200,
            obj([
                ("name", s("main")),
                ("protected", get(d, "protected").clone()),
                ("commit", obj([("sha", s(sha))])),
            ]),
            String::new(),
        ));
    }
    if endpoint.starts_with("repos/fixture/memory/rules/branches/main?") {
        let page = page_of(&endpoint);
        let rules = get(d, "rules").as_array().unwrap_or(&[]).to_vec();
        let link = if rules.len() as i64 > page * 100 {
            "Link: <fixture>; rel=\"next\"\n".to_owned()
        } else {
            String::new()
        };
        return Ok((200, page_slice(&rules, page), link));
    }
    if endpoint == "repos/fixture/memory/branches/main/protection" {
        let reads = counter(d, "protection_reads");
        if get(d, "remove_restrictions_at")
            .as_int()
            .and_then(|n| n.as_i64())
            == Some(reads)
        {
            set(d, "restrictions", Value::Null);
        }
        if get(d, "change_allowlist_at")
            .as_int()
            .and_then(|n| n.as_i64())
            == Some(reads)
        {
            set(d, "allowed_users", Value::Array(vec![]));
        }
        if !truthy(get(d, "protected")) {
            return Ok((
                404,
                obj([("message", s("Branch not protected"))]),
                String::new(),
            ));
        }
        return Ok((
            200,
            obj([
                (
                    "required_pull_request_reviews",
                    obj([("required_approving_review_count", int(1))]),
                ),
                ("enforce_admins", obj([("enabled", Value::Bool(false))])),
                (
                    "required_status_checks",
                    obj([("contexts", Value::Array(vec![s("fixture-ci")]))]),
                ),
                ("restrictions", get(d, "restrictions").clone()),
            ]),
            String::new(),
        ));
    }
    if endpoint.starts_with("repos/fixture/memory/pulls?") {
        return Ok((200, get(d, "pulls").clone(), String::new()));
    }
    if endpoint == "repos/fixture/memory/pulls" && method == "POST" {
        let head = fixture_git(
            d,
            &[
                "rev-parse",
                &format!("refs/heads/{}", text(get(&body, "head"))),
            ],
        )?;
        let base = fixture_git(
            d,
            &[
                "rev-parse",
                &format!("refs/heads/{}", text(get(&body, "base"))),
            ],
        )?;
        let repo_name = || obj([("full_name", s("fixture/memory"))]);
        let pull = obj([
            ("number", int(1)),
            ("state", s("open")),
            ("html_url", s("https://github.com/fixture/memory/pull/1")),
            (
                "head",
                obj([
                    ("sha", s(head)),
                    ("ref", get(&body, "head").clone()),
                    ("repo", repo_name()),
                ]),
            ),
            (
                "base",
                obj([
                    ("sha", s(base)),
                    ("ref", get(&body, "base").clone()),
                    ("repo", repo_name()),
                ]),
            ),
        ]);
        set(d, "pulls", Value::Array(vec![pull.clone()]));
        return Ok((201, pull, String::new()));
    }
    if endpoint == "graphql" {
        let pull = get(d, "pulls")
            .as_array()
            .and_then(|p| p.first())
            .cloned()
            .unwrap_or(Value::Null);
        let candidate = text(get(get(&pull, "head"), "sha")).to_owned();
        let base = fixture_git(d, &["rev-parse", "refs/heads/main"])?;
        let tree = fixture_git(d, &["rev-parse", &format!("{candidate}^{{tree}}")])?;
        let merged = fixture_git(
            d,
            &[
                "commit-tree",
                &tree,
                "-p",
                &base,
                "-p",
                &candidate,
                "-m",
                "Actual offline GitHub generated merge",
            ],
        )?;
        fixture_git(d, &["update-ref", "refs/pull/1/merge", &merged])?;
        let pull_request = obj([
            ("state", s("OPEN")),
            ("isDraft", Value::Bool(false)),
            ("headRefOid", s(candidate.clone())),
            ("baseRefOid", s(base)),
            ("reviewDecision", get(d, "review").clone()),
            ("mergeStateStatus", get(d, "merge_state").clone()),
            (
                "commits",
                obj([(
                    "nodes",
                    Value::Array(vec![obj([(
                        "commit",
                        obj([
                            ("oid", s(candidate)),
                            (
                                "statusCheckRollup",
                                obj([("state", get(d, "checks").clone())]),
                            ),
                        ]),
                    )])]),
                )]),
            ),
            (
                "reviewThreads",
                obj([
                    ("pageInfo", obj([("hasNextPage", Value::Bool(false))])),
                    ("nodes", Value::Array(vec![])),
                ]),
            ),
        ]);
        return Ok((
            200,
            obj([(
                "data",
                obj([("repository", obj([("pullRequest", pull_request)]))]),
            )]),
            String::new(),
        ));
    }
    if endpoint.starts_with("repos/fixture/memory/commits/") {
        let sha = endpoint.split('/').nth(4).unwrap_or("").to_owned();
        if endpoint.contains("/statuses?") {
            return Ok((200, Value::Array(vec![]), String::new()));
        }
        if endpoint.contains("/check-runs?") {
            let head = get(d, "pulls")
                .as_array()
                .and_then(|p| p.first())
                .map(|p| text(get(get(p, "head"), "sha")).to_owned())
                .unwrap_or_default();
            let mut runs = Vec::new();
            if sha == head && !truthy(get(d, "missing_required_check")) {
                let success = text(get(d, "checks")) == "SUCCESS";
                runs.push(obj([
                    ("id", int(1)),
                    ("head_sha", s(sha.clone())),
                    ("name", s("fixture-ci")),
                    (
                        "app",
                        obj([("id", d.get("check_app").cloned().unwrap_or(int(1)))]),
                    ),
                    (
                        "status",
                        s(if success { "completed" } else { "in_progress" }),
                    ),
                    (
                        "conclusion",
                        if success { s("success") } else { Value::Null },
                    ),
                    ("completed_at", s(agent_memory::host::utc_now_isoformat()?)),
                ]));
            }
            let count = runs.len() as i64;
            return Ok((
                200,
                obj([
                    ("check_runs", Value::Array(runs)),
                    ("total_count", int(count)),
                ]),
                String::new(),
            ));
        }
    }
    Ok((
        404,
        obj([("message", s("Unknown fixture route"))]),
        String::new(),
    ))
}

fn serve_call(call: &Path) -> TestResult {
    let root = PathBuf::from(std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap_or_default());
    let policy = root.join("github-policy.json");
    let _lock = StateLock::acquire(&root.join("github-policy.lock"))?;
    let mut d = read_policy(&policy)?;
    let count: usize = fs::read_to_string(call.join("argc"))?.parse()?;
    let args = (0..count)
        .map(|i| fs::read_to_string(call.join(format!("arg.{i}"))))
        .collect::<std::io::Result<Vec<_>>>()?;
    let stdin = fs::read(call.join("stdin"))?;
    let (status, value, extra) = github_server(&mut d, &args, &stdin)?;
    let pending = root.join("github-policy.next");
    fs::write(&pending, encode(&d)?)?;
    fs::rename(&pending, &policy)?;
    let mut response = fs::File::create(call.join("response"))?;
    write!(
        response,
        "HTTP/2.0 {status} Fixture\nContent-Type: application/json\n{extra}\n{}",
        encode(&value)?
    )?;
    fs::write(call.join("code"), if status < 400 { "0" } else { "1" })?;
    Ok(())
}

/// Re-executed `gh api` worker; a no-op in ordinary test runs.
#[test]
fn github_fixture_worker() {
    let Some(call) = std::env::var_os("AM_GITHUB_FIXTURE_CALL") else {
        return;
    };
    serve_call(Path::new(&call)).expect("fixture GitHub response");
}

fn guard(input: &Path) -> TestResult<bool> {
    let root = PathBuf::from(std::env::var_os("AGENT_MEMORY_TEST_ROOT").unwrap_or_default());
    let d = read_policy(&root.join("github-policy.json"))?;
    for line in fs::read_to_string(input)?.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [old, new, reference] = fields.as_slice() else {
            return Ok(false);
        };
        if *reference != "refs/heads/main" {
            continue;
        }
        if !matches!(get(&d, "restrictions"), Value::Null) {
            let actor = get(&d, "transport_actor_id")
                .as_int()
                .and_then(|n| n.as_i64())
                .unwrap_or(USER_ID);
            let Some(users) = get(&d, "allowed_users").as_array() else {
                return Ok(false);
            };
            if !users
                .iter()
                .any(|u| get(u, "id").as_int().and_then(|n| n.as_i64()) == Some(actor))
            {
                return Ok(false);
            }
        }
        if get(&d, "protected") == &Value::Bool(true) {
            let pulls = get(&d, "pulls").as_array().unwrap_or(&[]);
            if text(get(&d, "review")) != "APPROVED"
                || text(get(&d, "checks")) != "SUCCESS"
                || text(get(&d, "merge_state")) != "CLEAN"
                || pulls.is_empty()
            {
                return Ok(false);
            }
            let candidate = pulls
                .first()
                .map(|p| text(get(get(p, "head"), "sha")).to_owned())
                .unwrap_or_default();
            let parents = fixture_git(&d, &["rev-list", "--parents", "-n", "1", new])?;
            let parents: Vec<&str> = parents.split_whitespace().skip(1).collect();
            if parents != [*old, candidate.as_str()] {
                return Ok(false);
            }
            let tree =
                |revision: &str| fixture_git(&d, &["rev-parse", &format!("{revision}^{{tree}}")]);
            if tree(new)? != tree(&candidate)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Re-executed pre-receive guard; panics (non-zero exit) to reject a push.
#[test]
fn github_guard_worker() {
    let Some(input) = std::env::var_os("AM_GITHUB_GUARD_INPUT") else {
        return;
    };
    assert!(
        guard(Path::new(&input)).unwrap_or(false),
        "push rejected by fixture rules"
    );
}

// ---------------------------------------------------------------- policy worker

fn worker_outcome<T>(result: agent_memory::error::Result<T>, ok: impl FnOnce(T) -> Value) -> Value {
    match result {
        Ok(value) => ok(value),
        Err(error) => obj([("error", s(error.message()))]),
    }
}

/// Runs `memory_publication::policy`, or `protected_candidate` when the case
/// carries a request, in a child that sees the fixture `gh`.
#[test]
fn policy_worker() {
    let Some(case) = std::env::var_os("AM_PUBLICATION_POLICY_CASE") else {
        return;
    };
    let case = PathBuf::from(case);
    agent_memory::cli::register_providers().expect("providers");
    let repo = fs::read_to_string(case.join("repo")).expect("repo");
    let repo = Path::new(&repo);
    if let Ok(raw) = fs::read(case.join("tree.json")) {
        let input = decode(&raw).expect("tree input");
        let runtime = case.join("runtime");
        fs::create_dir_all(&runtime).expect("runtime root");
        let retained = case.join("retained");
        let observe = |directory: Option<&Path>, expected: Option<&Value>| {
            layout::observe_tree(
                repo,
                text(get(&input, "revision")),
                "memory-bank",
                directory,
                expected,
                &runtime,
            )
        };
        let first = observe(Some(&retained), None);
        let outcome = match first {
            Err(error) => obj([("error", s(error.message()))]),
            Ok(manifest) => {
                let again = observe(None, Some(&manifest)).expect("same inventory");
                let mut tampered = manifest.clone();
                set(&mut tampered, "source_bytes", int(0));
                let changed = observe(None, Some(&tampered)).unwrap_err();
                let mut retained_bytes = Vec::new();
                for source in get(&manifest, "sources").as_array().unwrap_or(&[]) {
                    let bytes = fs::read(retained.join(text(get(source, "blob")))).expect("blob");
                    retained_bytes.push(s(String::from_utf8_lossy(&bytes)));
                }
                obj([
                    ("manifest", manifest.clone()),
                    ("again", again),
                    ("changed", s(changed.message())),
                    ("retained", Value::Array(retained_bytes)),
                    (
                        "normal_refused",
                        Value::Bool(
                            agent_memory::sharing_recovery::tree(
                                repo,
                                text(get(&input, "revision")),
                                "memory-bank",
                                false,
                            )
                            .is_err(),
                        ),
                    ),
                ])
            }
        };
        fs::write(case.join("outcome.json"), encode(&outcome).expect("encode")).expect("outcome");
        return;
    }
    let outcome = match fs::read(case.join("request.json")) {
        Ok(raw) => {
            let request = decode(&raw).expect("request");
            let policy =
                decode(&fs::read(case.join("policy.json")).expect("policy")).expect("policy");
            worker_outcome(
                publication::protected_candidate(repo, &request, &policy, None),
                |(merged, url, reason)| {
                    obj([
                        ("merged", merged.map_or(Value::Null, s)),
                        ("url", s(url)),
                        ("reason", reason.map_or(Value::Null, s)),
                    ])
                },
            )
        }
        Err(_) => {
            let binding = obj([("authority_branch", s("main"))]);
            worker_outcome(publication::policy(repo, &binding), |value| {
                obj([("policy", value)])
            })
        }
    };
    fs::write(case.join("outcome.json"), encode(&outcome).expect("encode")).expect("outcome");
}

fn run_policy(w: &World, repo: &Path) -> TestResult<Value> {
    run_case(w, repo, &[])
}

fn run_case(w: &World, repo: &Path, inputs: &[(&str, &Value)]) -> TestResult<Value> {
    let case = w
        .root
        .join(format!("policy-case-{}", agent_memory::host::uuid4()?));
    fs::create_dir(&case)?;
    fs::write(case.join("repo"), repo.to_string_lossy().as_bytes())?;
    for (name, value) in inputs {
        fs::write(case.join(name), encode(value)?)?;
    }
    let mut env = w.env.clone();
    env.insert(
        "AM_PUBLICATION_POLICY_CASE".into(),
        case.clone().into_os_string(),
    );
    let argv = [
        std::env::current_exe()?.into_os_string(),
        "--exact".into(),
        "policy_worker".into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ];
    let env: Vec<(OsString, OsString)> = env.into_iter().collect();
    let output = agent_memory::process::run(&agent_memory::process::Spec {
        argv: &argv,
        search_path: false,
        env: &env,
        cwd: Some(repo),
        stdin: agent_memory::process::Input::Null,
        stdout: agent_memory::process::Target::Capture { limit: 1 << 20 },
        stderr: agent_memory::process::Target::Capture { limit: 1 << 20 },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(120))?,
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    })?;
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(0),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    decode(&fs::read(case.join("outcome.json"))?)
}

/// A repository with a rendered bank and pushed authority, not yet bound
/// (identity binding needs recovery_frontier): `(repo, bank, remote)`.
fn plain_repository(w: &mut World) -> TestResult<(PathBuf, PathBuf, PathBuf)> {
    install_runtime(w);
    let repo = w.repository("repo", false)?;
    let remote = w.repository("remote.git", true)?;
    git(
        w,
        &repo,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    )?;
    git(
        w,
        &repo,
        &["config", "--local", "agent-memory.authorityBranch", "main"],
    )?;
    let bank = repo.join("memory-bank");
    render_bank(w, &bank)?;
    fs::write(repo.join("code.py"), "VALUE = 1\n")?;
    git(w, &repo, &["add", "memory-bank", "code.py"])?;
    git(w, &repo, &["commit", "-m", "Fixture canonical memory"])?;
    git(w, &repo, &["push", "origin", "HEAD:main"])?;
    fs::write(repo.join("code.py"), "VALUE = 2  # staged work\n")?;
    git(w, &repo, &["add", "code.py"])?;
    Ok((repo, bank, remote))
}

/// The GitHub protocol fixture as the repository's observable authority.
fn policy_world(protected: bool) -> TestResult<(World, PathBuf, PathBuf)> {
    let mut w = World::new()?;
    let (repo, _, remote) = plain_repository(&mut w)?;
    let policy = install(&mut w, &repo, &remote, protected)?;
    git(
        &w,
        &repo,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/fixture/memory.git",
        ],
    )?;
    enforce(&remote)?;
    Ok((w, repo, policy))
}
fn unsupported(outcome: &Value) -> Vec<String> {
    get(get(outcome, "policy"), "unsupported_rules")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .map(|v| text(v).to_owned())
        .collect()
}
fn only_reads(policy: &Path) -> TestResult<bool> {
    let d = read_policy(policy)?;
    Ok(get(&d, "events")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .all(|event| text(get(event, "method")) == "GET"))
}

// ---------------------------------------------------------------- active tests

#[test]
fn publication_prepare_refuses_incomplete_input_and_keeps_local_state() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _) = plain_repository(&mut w)?;
    let before = state(&w, &repo, &bank)?;
    let (code, value) = publish(&w, &repo, "prepare", &obj([]))?;
    assert_eq!(code, 1);
    assert_eq!(
        text(get(&value, "schema")),
        "agent-memory/publication-result-v1"
    );
    assert_eq!(text(get(&value, "operation")), "prepare");
    assert_eq!(text(get(&value, "sharing_state")), "pending-inspection");
    assert_eq!(text(get(&value, "failure_code")), "publication-unverified");
    assert_eq!(get(&value, "publication_verified"), &Value::Bool(false));
    assert_eq!(get(&value, "local_memory_available"), &Value::Bool(true));
    assert_eq!(get(&value, "git_metadata_writes_performed"), &Value::Null);
    assert_eq!(
        get(&value, "remote_freshness_verified"),
        &Value::Bool(false)
    );
    assert!(value.get("request").is_none());
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

#[test]
fn publish_parser_requires_an_operation_and_both_paths() -> TestResult {
    let w = World::new()?;
    let output = w.am_with(
        ["--json", "publish", "deploy", "--repo", ".", "--input", "x"],
        RunOptions {
            check: false,
            ..RunOptions::default()
        },
    )?;
    assert_eq!(output.status, agent_memory::process::Status::Code(2));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("invalid choice: 'deploy'"), "{stderr}");
    let output = w.am_with(
        ["--json", "publish", "land", "--repo", "."],
        RunOptions {
            check: false,
            ..RunOptions::default()
        },
    )?;
    assert_eq!(output.status, agent_memory::process::Status::Code(2));
    assert!(String::from_utf8(output.stderr)?.contains("--input"));
    Ok(())
}

#[test]
fn request_value_refuses_inexact_or_rebound_requests_before_git() -> TestResult {
    let w = World::new()?;
    let root = &w.root;
    let root_text = root.to_string_lossy().into_owned();
    let binding = obj([
        ("repo_root", s(root_text.clone())),
        ("project_id", s("8e0b66aa-2be5-4fa3-b47f-1245474aa7dd")),
        ("authority_head", s("a".repeat(40))),
        ("bank_root", s("memory-bank")),
    ]);
    let request = obj([
        ("schema", s(publication::SCHEMA)),
        ("repo_root", s(root_text.clone())),
        ("project_id", get(&binding, "project_id").clone()),
        ("base", s("a".repeat(40))),
        ("candidate", s("b".repeat(40))),
        (
            "proof",
            obj([("judgment", obj([("binding", binding.clone())]))]),
        ),
    ]);
    let exact = "Use the exact private publication request in its original repository";
    let mut extra = request.clone();
    set(&mut extra, "extra", Value::Bool(true));
    let mut schema = request.clone();
    set(
        &mut schema,
        "schema",
        s("agent-memory/publication-request-v0"),
    );
    let mut elsewhere = request.clone();
    set(&mut elsewhere, "repo_root", s("/elsewhere"));
    for value in [extra, schema, elsewhere, Value::Array(vec![])] {
        let error = publication::request_value(root, &value).unwrap_err();
        assert_eq!(error.message(), exact);
    }
    let shared = "Publication request and source proof do not share the same project/base";
    let mut project = request.clone();
    set(
        &mut project,
        "project_id",
        s("9e0b66aa-2be5-4fa3-b47f-1245474aa7dd"),
    );
    let mut base = request.clone();
    set(&mut base, "base", s("c".repeat(40)));
    let mut moved = request.clone();
    let mut other = binding.clone();
    set(&mut other, "repo_root", s("/elsewhere"));
    set(
        &mut moved,
        "proof",
        obj([("judgment", obj([("binding", other)]))]),
    );
    for value in [project, base, moved] {
        let error = publication::request_value(root, &value).unwrap_err();
        assert_eq!(error.message(), shared);
    }
    Ok(())
}

#[test]
fn recovery_publication_artifacts_are_exact_bounded_and_typed() -> TestResult {
    let value = obj([("schema", s(layout::REVIEW)), ("decision", s("approve"))]);
    assert!(layout::exact(&value, &["schema", "decision"], Some(layout::REVIEW)).is_ok());
    for (keys, schema) in [
        (&["schema"][..], Some(layout::REVIEW)),
        (&["schema", "decision", "extra"][..], Some(layout::REVIEW)),
        (&["schema", "decision"][..], Some(layout::REQUEST)),
    ] {
        let error = layout::exact(&value, keys, schema).unwrap_err();
        assert_eq!(
            error.message(),
            "Use the exact typed private recovery publication artifact"
        );
    }
    assert!(layout::exact(&Value::Array(vec![]), &[], None).is_err());
    assert_eq!(
        layout::text(&s("A reviewed judgment"), 4096)?,
        "A reviewed judgment"
    );
    for rejected in [s("  \n"), s("x".repeat(4097)), int(1), Value::Null] {
        let error = layout::text(&rejected, 4096).unwrap_err();
        assert_eq!(
            error.message(),
            "A bounded explicit source-preservation judgment is required"
        );
    }
    let pending = layout::result(
        "publish-land",
        "a9abac6f-c33e-44c9-b40f-7b2ebafda860",
        "pending-policy",
        obj([
            ("publication_verified", Value::Bool(true)),
            ("pull_request", Value::Null),
        ]),
    )?;
    assert_eq!(get(&pending, "verified"), &Value::Bool(false));
    let keys: Vec<&str> = pending
        .as_object()
        .unwrap_or(&[])
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(keys.first(), Some(&"schema"));
    assert_eq!(keys.get(4), Some(&"publication_verified"));
    assert_eq!(keys.last(), Some(&"pull_request"));
    assert_eq!(get(&pending, "publication_verified"), &Value::Bool(true));
    let landed = layout::result("publish-land", "x", "landed", obj([]))?;
    assert_eq!(get(&landed, "verified"), &Value::Bool(true));
    assert_eq!(text(get(&landed, "canonical_eligibility")), "unavailable");
    Ok(())
}

#[test]
fn policy_reads_every_rule_page_before_allowing_automation() -> TestResult {
    let (w, repo, policy) = policy_world(false)?;
    let clean = run_policy(&w, &repo)?;
    assert!(
        get(&clean, "error").as_str().is_none(),
        "{}",
        encode(&clean)?
    );
    let observed = get(&clean, "policy");
    assert_eq!(text(get(observed, "repository")), "fixture/memory");
    assert_eq!(get(observed, "protected"), &Value::Bool(false));
    assert_eq!(get(observed, "push_actor"), &Value::Null);
    assert!(unsupported(&clean).is_empty());
    let mut rules: Vec<Value> = (0..100)
        .map(|_| obj([("type", s("non_fast_forward"))]))
        .collect();
    rules.push(obj([("type", s("required_deployments"))]));
    edit_policy(
        &policy,
        &[
            ("rules", Value::Array(rules)),
            ("events", Value::Array(vec![])),
        ],
    )?;
    let outcome = run_policy(&w, &repo)?;
    assert_eq!(unsupported(&outcome), ["required_deployments"]);
    let d = read_policy(&policy)?;
    let paged = get(&d, "events")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .any(|event| {
            text(get(event, "endpoint"))
                .rsplit("repos/fixture/memory/")
                .next()
                == Some("rules/branches/main?per_page=100&page=2")
        });
    assert!(paged);
    assert!(only_reads(&policy)?);
    Ok(())
}

#[test]
fn policy_reports_protected_review_checks_and_permission_refusals() -> TestResult {
    let (w, repo, policy) = policy_world(true)?;
    let protected = run_policy(&w, &repo)?;
    let observed = get(&protected, "policy");
    assert_eq!(get(observed, "protected"), &Value::Bool(true));
    assert_eq!(get(observed, "review_required"), &Value::Bool(true));
    assert_eq!(
        get(observed, "required_checks"),
        &Value::Array(vec![obj([
            ("context", s("fixture-ci")),
            ("app_id", Value::Null)
        ])])
    );
    assert!(unsupported(&protected).is_empty());
    edit_policy(
        &policy,
        &[(
            "rules",
            Value::Array(vec![obj([
                ("type", s("required_status_checks")),
                (
                    "parameters",
                    obj([(
                        "required_status_checks",
                        Value::Array(vec![obj([
                            ("context", s("fixture-ci")),
                            ("integration_id", int(2)),
                        ])]),
                    )]),
                ),
            ])]),
        )],
    )?;
    let checks = run_policy(&w, &repo)?;
    assert_eq!(
        get(get(&checks, "policy"), "required_checks"),
        &Value::Array(vec![
            obj([("context", s("fixture-ci")), ("app_id", Value::Null)]),
            obj([("context", s("fixture-ci")), ("app_id", int(2))]),
        ])
    );
    assert_ne!(
        get(get(&checks, "policy"), "observation_sha256"),
        get(observed, "observation_sha256")
    );
    edit_policy(&policy, &[("push", Value::Bool(false))])?;
    let refused = run_policy(&w, &repo)?;
    assert_eq!(
        text(get(&refused, "error")),
        "Current repository identity/default/permission does not permit publication"
    );
    edit_policy(
        &policy,
        &[
            ("push", Value::Bool(true)),
            ("unavailable", Value::Bool(true)),
        ],
    )?;
    let offline = run_policy(&w, &repo)?;
    assert_eq!(
        text(get(&offline, "error")),
        "GitHub rules or permission observation unavailable; publication remains pending"
    );
    assert!(only_reads(&policy)?);
    Ok(())
}

fn restricted_world() -> TestResult<(World, PathBuf, PathBuf, Value)> {
    let (w, repo, policy) = policy_world(true)?;
    let allowed = user(USER_ID, "allowed-user");
    edit_policy(
        &policy,
        &[
            (
                "restrictions",
                obj([
                    ("users", Value::Array(vec![allowed.clone()])),
                    ("teams", Value::Array(vec![])),
                    ("apps", Value::Array(vec![])),
                ]),
            ),
            ("allowed_users", Value::Array(vec![allowed])),
        ],
    )?;
    let baseline = read_policy(&policy)?;
    Ok((w, repo, policy, baseline))
}

#[test]
fn restricted_push_actor_requires_an_explicit_complete_user_grant() -> TestResult {
    let (w, repo, policy, baseline) = restricted_world()?;
    let eligible = run_policy(&w, &repo)?;
    let actor = get(get(&eligible, "policy"), "push_actor");
    assert_eq!(
        get(actor, "eligible"),
        &Value::Bool(true),
        "{}",
        encode(&eligible)?
    );
    assert_eq!(get(actor, "actor"), &user(USER_ID, "allowed-user"));
    assert!(unsupported(&eligible).is_empty());
    let allowed = user(USER_ID, "allowed-user");
    let cases: Vec<Vec<(&str, Value)>> = vec![
        vec![("allowed_users", Value::Array(vec![]))],
        vec![("actor", user(8, "allowed-user"))],
        vec![("actor", user(USER_ID, "different-user"))],
        vec![("actor_unavailable", Value::Bool(true))],
        vec![("actor_status", int(201))],
        vec![(
            "allowed_users",
            Value::Array(vec![allowed.clone(), allowed.clone()]),
        )],
        vec![(
            "allowed_users",
            Value::Array(vec![obj([
                ("id", Value::Bool(true)),
                ("login", s("allowed-user")),
                ("type", s("User")),
            ])]),
        )],
        vec![(
            "allowed_users",
            obj([("users", Value::Array(vec![allowed.clone()]))]),
        )],
        vec![(
            "restrictions",
            obj([("users", Value::Array(vec![allowed.clone()]))]),
        )],
        vec![
            ("allowed_users", Value::Array(vec![])),
            (
                "restrictions",
                obj([
                    ("users", Value::Array(vec![])),
                    ("teams", Value::Array(vec![obj([("id", int(7))])])),
                    ("apps", Value::Array(vec![])),
                ]),
            ),
        ],
        vec![
            ("allowed_users", Value::Array(vec![])),
            (
                "restrictions",
                obj([
                    ("users", Value::Array(vec![])),
                    ("teams", Value::Array(vec![])),
                    ("apps", Value::Array(vec![obj([("id", int(7))])])),
                ]),
            ),
        ],
        vec![("allowlist_failed_page", int(1))],
    ];
    for case in cases {
        let mut d = baseline.clone();
        for (key, value) in &case {
            set(&mut d, key, value.clone());
        }
        set(&mut d, "events", Value::Array(vec![]));
        fs::write(&policy, encode(&d)?)?;
        let outcome = run_policy(&w, &repo)?;
        let actor = get(get(&outcome, "policy"), "push_actor");
        assert!(
            unsupported(&outcome).contains(&"restricted-push-actor-unverified".to_owned()),
            "{}",
            encode(&outcome)?
        );
        assert_eq!(get(actor, "eligible"), &Value::Bool(false));
        assert!(text(get(actor, "repair")).contains("Retain local learning"));
        assert!(only_reads(&policy)?);
    }
    Ok(())
}

#[test]
fn restricted_push_actor_pagination_must_be_a_complete_exact_chain() -> TestResult {
    let (w, repo, policy, baseline) = restricted_world()?;
    let users: Vec<Value> = (0..400)
        .map(|n| user(n + 100, &format!("user-{n}")))
        .collect();
    let mut first = users.get(..100).unwrap_or(&[]).to_vec();
    first.push(user(USER_ID, "Allowed-User"));
    let mut d = baseline.clone();
    set(&mut d, "allowed_users", Value::Array(first.clone()));
    fs::write(&policy, encode(&d)?)?;
    let later = run_policy(&w, &repo)?;
    assert!(unsupported(&later).is_empty(), "{}", encode(&later)?);
    let actor = get(get(&later, "policy"), "push_actor");
    assert_eq!(get(actor, "eligible"), &Value::Bool(true));
    assert_eq!(
        get(actor, "users").as_array().map(<[Value]>::len),
        Some(101)
    );
    let mut all = users.clone();
    all.push(user(USER_ID, "allowed-user"));
    let mut duplicate = users.get(..100).unwrap_or(&[]).to_vec();
    duplicate.push(users.first().cloned().unwrap_or(Value::Null));
    let changes: Vec<(&str, Value)> = vec![
        ("allowlist_failed_page", int(2)),
        ("allowlist_status", obj([("2", int(201))])),
        ("broken_next", Value::Bool(true)),
        ("allowed_users", Value::Array(all)),
        ("allowed_users", Value::Array(duplicate)),
    ];
    for (key, value) in changes {
        let mut d = baseline.clone();
        set(&mut d, "allowed_users", Value::Array(first.clone()));
        set(&mut d, key, value);
        fs::write(&policy, encode(&d)?)?;
        let outcome = run_policy(&w, &repo)?;
        assert!(
            unsupported(&outcome).contains(&"restricted-push-actor-unverified".to_owned()),
            "{key}: {}",
            encode(&outcome)?
        );
    }
    Ok(())
}

/// A memory-only candidate over the pushed authority head, built in a private index.
fn memory_candidate(w: &mut World, repo: &Path, base: &str) -> TestResult<String> {
    let index = w.root.join("candidate-index");
    let body = w.root.join("candidate-body.md");
    fs::write(&body, "# Current focus\n\nIndependent reviewed learning.\n")?;
    w.env
        .insert("GIT_INDEX_FILE".into(), index.into_os_string());
    let built = (|| -> TestResult<String> {
        git(w, repo, &["read-tree", base])?;
        let blob = git(w, repo, &["hash-object", "-w", &body.to_string_lossy()])?;
        let info = format!("100644,{},memory-bank/activeContext.md", blob.trim());
        git(w, repo, &["update-index", "--add", "--cacheinfo", &info])?;
        let tree = git(w, repo, &["write-tree"])?;
        Ok(git(
            w,
            repo,
            &[
                "commit-tree",
                tree.trim(),
                "-p",
                base,
                "-m",
                "Memory-only candidate",
            ],
        )?
        .trim()
        .to_owned())
    })();
    w.env.remove(OsStr::new("GIT_INDEX_FILE"));
    built
}

#[test]
fn protected_landing_waits_for_review_then_exact_checks_then_uses_the_generated_merge() -> TestResult
{
    let (mut w, repo, policy) = policy_world(true)?;
    let base = git(&w, &repo, &["rev-parse", "HEAD"])?.trim().to_owned();
    let candidate = memory_candidate(&mut w, &repo, &base)?;
    let project = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd";
    let branch = format!("agent-memory/{project}/{candidate}");
    let remote = text(get(&read_policy(&policy)?, "remote")).to_owned();
    let refspec = format!("{candidate}:refs/heads/{branch}");
    git(&w, &repo, &["push", &remote, &refspec])?;
    // The PR already exists (a retried landing): no new push or source guard.
    let side = |sha: &str, reference: &str| {
        obj([
            ("sha", s(sha)),
            ("ref", s(reference)),
            ("repo", obj([("full_name", s("fixture/memory"))])),
        ])
    };
    let pull = obj([
        ("number", int(1)),
        ("state", s("open")),
        ("head", side(&candidate, &branch)),
        ("base", side(&base, "main")),
    ]);
    edit_policy(&policy, &[("pulls", Value::Array(vec![pull]))])?;
    let observed = get(&run_policy(&w, &repo)?, "policy").clone();
    let request = obj([
        ("base", s(base.clone())),
        ("candidate", s(candidate.clone())),
        ("project_id", s(project)),
        (
            "proof",
            obj([(
                "judgment",
                obj([(
                    "binding",
                    obj([
                        ("authority_remote", s("origin")),
                        ("authority_branch", s("main")),
                    ]),
                )]),
            )]),
        ),
    ]);
    let inputs = [("request.json", &request), ("policy.json", &observed)];
    let review = run_case(&w, &repo, &inputs)?;
    assert_eq!(get(&review, "merged"), &Value::Null, "{}", encode(&review)?);
    assert_eq!(
        text(get(&review, "url")),
        "https://github.com/fixture/memory/pull/1"
    );
    assert!(text(get(&review, "reason")).contains("Required review remains pending"));
    edit_policy(
        &policy,
        &[("review", s("APPROVED")), ("checks", s("PENDING"))],
    )?;
    let checks = run_case(&w, &repo, &inputs)?;
    assert_eq!(get(&checks, "merged"), &Value::Null, "{}", encode(&checks)?);
    assert!(text(get(&checks, "reason")).contains("Exact required status/check results"));
    edit_policy(
        &policy,
        &[
            ("checks", s("SUCCESS")),
            ("missing_required_check", Value::Bool(true)),
        ],
    )?;
    let missing = run_case(&w, &repo, &inputs)?;
    assert_eq!(get(&missing, "merged"), &Value::Null);
    edit_policy(&policy, &[("missing_required_check", Value::Bool(false))])?;
    let ready = run_case(&w, &repo, &inputs)?;
    let merged = text(get(&ready, "merged")).to_owned();
    assert_eq!(get(&ready, "reason"), &Value::Null, "{}", encode(&ready)?);
    assert_eq!(
        rev_parents(&w, &repo, &merged)?,
        [merged.clone(), base, candidate.clone()]
    );
    assert_eq!(
        git(&w, &repo, &["rev-parse", &format!("{merged}^{{tree}}")])?,
        git(&w, &repo, &["rev-parse", &format!("{candidate}^{{tree}}")])?
    );
    // Disposable observation refs never survive the landing check.
    assert_eq!(git(&w, &repo, &["for-each-ref", "refs/agent-memory"])?, "");
    assert_eq!(
        get(&read_policy(&policy)?, "pulls")
            .as_array()
            .map(<[Value]>::len),
        Some(1)
    );
    Ok(())
}

#[test]
fn publication_request_and_replay_refuse_inexact_inputs() -> TestResult {
    let mut w = World::new()?;
    let (repo, _, _) = plain_repository(&mut w)?;
    agent_memory::cli::register_providers()?;
    let error = publication::request_value(&repo, &obj([])).unwrap_err();
    assert_eq!(
        error.message(),
        "Use the exact private publication request in its original repository"
    );
    let inexact =
        agent_memory::sharing_recovery::replay(&repo, &obj([("base", s("a"))]), false).unwrap_err();
    assert_eq!(
        inexact.message(),
        "Replay requires exact base and memory-only candidate commit IDs"
    );
    Ok(())
}

fn observe_tree_case(w: &World, repo: &Path) -> TestResult<Value> {
    let revision = git(w, repo, &["rev-parse", "HEAD"])?.trim().to_owned();
    let input = obj([("revision", s(revision))]);
    run_case(w, repo, &[("tree.json", &input)])
}

#[test]
fn raw_tree_inventory_verifies_every_object_and_retains_exact_bytes() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _) = plain_repository(&mut w)?;
    let opaque = "opaque\u{0}unsupported source\n";
    fs::write(bank.join("notes.txt"), opaque)?;
    git(&w, &repo, &["add", "memory-bank/notes.txt"])?;
    git(&w, &repo, &["commit", "-m", "Opaque raw source"])?;
    let outcome = observe_tree_case(&w, &repo)?;
    assert!(
        get(&outcome, "error").as_str().is_none(),
        "{}",
        encode(&outcome)?
    );
    let manifest = get(&outcome, "manifest");
    assert_eq!(
        text(get(manifest, "schema")),
        "agent-memory/layout-publication-tree-v1"
    );
    assert_eq!(text(get(manifest, "bank_root")), "memory-bank");
    assert_eq!(get(manifest, "observation_complete"), &Value::Bool(true));
    let sources = get(manifest, "sources").as_array().unwrap_or(&[]);
    let mut files = BTreeMap::new();
    files_below(&bank, &bank, &mut files)?;
    let committed: Vec<&String> = files.keys().collect();
    let listed: Vec<&str> = sources.iter().map(|row| text(get(row, "path"))).collect();
    assert_eq!(listed, committed);
    let mut total = 0;
    for (row, retained) in sources
        .iter()
        .zip(get(&outcome, "retained").as_array().unwrap_or(&[]))
    {
        let path = text(get(row, "path"));
        let raw = files.get(path).cloned().unwrap_or_default();
        total += raw.len();
        assert_eq!(get(row, "byte_count"), &int(raw.len() as i64));
        assert_eq!(text(get(row, "sha256")), {
            use sha2::Digest;
            sha2::Sha256::digest(&raw)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        });
        let blob = git(
            &w,
            &repo,
            &["rev-parse", &format!("HEAD:memory-bank/{path}")],
        )?;
        assert_eq!(text(get(row, "git_blob")), blob.trim());
        let id = text(get(row, "source_id"));
        assert_eq!(text(get(row, "blob")), format!("originals/{id}.bin"));
        assert_eq!(text(retained).as_bytes(), raw.as_slice());
    }
    assert_eq!(get(manifest, "source_bytes"), &int(total as i64));
    assert!(
        get(manifest, "physical_entries")
            .as_array()
            .unwrap_or(&[])
            .iter()
            .all(|row| matches!(
                (text(get(row, "type")), text(get(row, "mode"))),
                ("blob", "100644") | ("tree", "040000")
            ))
    );
    // Re-observation with the reviewed inventory keeps every source identity.
    assert_eq!(get(&outcome, "again"), manifest);
    assert_eq!(
        text(get(&outcome, "changed")),
        "Raw authority source inventory changed; obtain a new independent review"
    );
    Ok(())
}

#[test]
fn raw_tree_inventory_refuses_nonordinary_objects_and_runtime_markers() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _) = plain_repository(&mut w)?;
    fs::write(bank.join("tool.sh"), "#!/bin/sh\n")?;
    fs::set_permissions(bank.join("tool.sh"), fs::Permissions::from_mode(0o755))?;
    git(&w, &repo, &["add", "memory-bank/tool.sh"])?;
    git(&w, &repo, &["commit", "-m", "Executable bank object"])?;
    let executable = observe_tree_case(&w, &repo)?;
    assert_eq!(
        text(get(&executable, "error")),
        "Redirect, executable, submodule or nonordinary raw bank object requires review"
    );
    git(&w, &repo, &["rm", "-q", "memory-bank/tool.sh"])?;
    fs::write(bank.join(".agent-memory-transaction.md"), "marker\n")?;
    git(
        &w,
        &repo,
        &["add", "-f", "memory-bank/.agent-memory-transaction.md"],
    )?;
    git(&w, &repo, &["commit", "-m", "Committed runtime marker"])?;
    let marker = observe_tree_case(&w, &repo)?;
    assert_eq!(
        text(get(&marker, "error")),
        "A committed recovery/runtime marker is not portable publication evidence"
    );
    Ok(())
}

// ---------------------------------------------------------------- memory publication contracts

fn rev_parents(w: &World, repo: &Path, commit: &str) -> TestResult<Vec<String>> {
    Ok(git(w, repo, &["rev-list", "--parents", "-n", "1", commit])?
        .split_whitespace()
        .map(str::to_owned)
        .collect())
}

// obligation: tests/contracts/test_memory_publication.py::MemoryPublicationContract.test_first_publication_excludes_feature_ancestry_and_preserves_original_files_index
#[test]
fn first_publication_excludes_feature_ancestry_and_preserves_original_files_index() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, base) = bootstrap(&mut w)?;
    github(&mut w, &repo, &bank, None, false)?;
    let before = state(&w, &repo, &bank)?;
    let unrelated = fs::read(repo.join("unrelated.txt"))?;
    let prepared = prepared(&w, &repo)?;
    assert_eq!(text(get(&prepared, "sharing_state")), "prepared");
    let request = get(&prepared, "request").clone();
    let candidate = text(get(&request, "candidate"));
    assert_eq!(
        rev_parents(&w, &repo, candidate)?,
        [candidate.to_owned(), base]
    );
    assert_eq!(
        git(&w, &repo, &["show", &format!("{candidate}:code.py")])?,
        "VALUE = 1\n"
    );
    let landed = land(&w, &repo, &request)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    assert_eq!(text(get(&landed, "sharing_state")), "landed");
    assert_eq!(
        remote_head(&w, &repo)?.split_whitespace().next(),
        Some(candidate)
    );
    assert_eq!(before, state(&w, &repo, &bank)?);
    assert_eq!(unrelated, fs::read(repo.join("unrelated.txt"))?);
    let again = land(&w, &repo, &request)?;
    assert_eq!(text(get(&again, "sharing_state")), "already-landed");
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_memory_publication.py::MemoryPublicationContract.test_required_human_review_and_checks_remain_pending_even_for_an_admin
#[test]
fn required_human_review_and_checks_remain_pending_even_for_an_admin() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    let policy = github(&mut w, &repo, &bank, Some(&peer), true)?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nKeep independently useful learning conditional on unfinished work.\n",
    )?;
    let before = state(&w, &repo, &bank)?;
    let head = remote_head(&w, &repo)?;
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let pending = land(&w, &repo, &request)?;
    assert_eq!(get(&pending, "publication_verified"), &Value::Bool(false));
    assert_eq!(
        text(get(&pending, "sharing_state")),
        "pending-repository-rules"
    );
    assert!(
        text(get(&pending, "reason"))
            .to_lowercase()
            .contains("review")
    );
    assert_eq!(head, remote_head(&w, &repo)?);
    assert_eq!(
        get(&read_policy(&policy)?, "pulls")
            .as_array()
            .map(<[Value]>::len),
        Some(1)
    );
    edit_policy(
        &policy,
        &[("review", s("APPROVED")), ("checks", s("PENDING"))],
    )?;
    let checks = land(&w, &repo, &request)?;
    assert_eq!(get(&checks, "publication_verified"), &Value::Bool(false));
    assert_eq!(head, remote_head(&w, &repo)?);
    edit_policy(&policy, &[("checks", s("SUCCESS"))])?;
    let landed = land(&w, &repo, &request)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    let actual = remote_head(&w, &repo)?;
    let actual = actual.split_whitespace().next().unwrap_or("");
    assert_eq!(text(get(&landed, "witnessed_commit")), actual);
    let parents = rev_parents(&w, &repo, actual)?;
    assert_eq!(
        parents.get(1..),
        Some(
            &[
                text(get(&request, "base")).to_owned(),
                text(get(&request, "candidate")).to_owned()
            ][..]
        )
    );
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_memory_publication.py::MemoryPublicationContract.test_first_bank_compares_real_absence_without_a_second_starter
#[test]
fn first_bank_compares_real_absence_without_a_second_starter() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, base) = bootstrap(&mut w)?;
    let before = state(&w, &repo, &bank)?;
    let proof = proof(&w, &repo)?;
    let binding = get(get(&proof, "judgment"), "binding");
    assert_eq!(text(get(binding, "base")), base);
    assert_eq!(get(binding, "shared_bank_present"), &Value::Bool(false));
    assert_eq!(get(get(binding, "source_hashes"), "base"), &obj([]));
    assert_eq!(get(get(binding, "source_hashes"), "landed"), &obj([]));
    let input = save(&w, &proof, "proof.json")?;
    let checked = good(
        &w,
        "am semantic",
        &[
            os("check"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
        ],
    )?;
    assert_eq!(
        get(&checked, "publish_files"),
        get(&checked, "refresh_files")
    );
    let mut published: Vec<String> = get(&checked, "publish_files")
        .as_object()
        .unwrap_or(&[])
        .iter()
        .map(|(p, _)| p.clone())
        .collect();
    published.sort();
    let mut expected: Vec<String> = before
        .0
        .keys()
        .filter(|p| p.ends_with(".md"))
        .cloned()
        .collect();
    expected.sort();
    assert_eq!(published, expected);
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_memory_publication.py::MemoryPublicationContract.test_unstaged_canonical_deletion_needs_real_prior_ownership_not_an_index_edit
#[test]
fn unstaged_canonical_deletion_needs_real_prior_ownership_not_an_index_edit() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _, _) = fixture(&mut w)?;
    fs::remove_file(bank.join("activeContext.md"))?;
    let router = bank.join("MEMORY.md");
    let kept: String = fs::read_to_string(&router)?
        .split_inclusive('\n')
        .filter(|line| !line.starts_with("| [activeContext.md]"))
        .collect();
    let edited = kept.replace(
        "## Omitted contexts\n\n",
        "## Omitted contexts\n\n- `activeContext.md`: Finished session; no active focus remains.\n",
    );
    fs::write(&router, edited)?;
    let before = state(&w, &repo, &bank)?;
    let boundary = good(&w, "am sharing", &[os("--repo"), repo.as_os_str()])?;
    assert_eq!(get(&boundary, "local_boundary_ready"), &Value::Bool(true));
    assert_eq!(
        get(&boundary, "proven_deleted_memory_paths"),
        &Value::Array(vec![s("memory-bank/activeContext.md")])
    );
    proof(&w, &repo)?;
    assert_eq!(before, state(&w, &repo, &bank)?);
    fs::write(bank.join("cache.json"), "{}\n")?;
    git(&w, &repo, &["add", "memory-bank/cache.json"])?;
    fs::remove_file(bank.join("cache.json"))?;
    let index = fs::read(repo.join(".git/index"))?;
    let blocked = good(&w, "am sharing", &[os("--repo"), repo.as_os_str()])?;
    assert!(
        get(&blocked, "blockers")
            .as_array()
            .unwrap_or(&[])
            .contains(&s("excluded-bank-artifacts-are-tracked"))
    );
    assert_eq!(index, fs::read(repo.join(".git/index"))?);
    Ok(())
}

// obligation: tests/contracts/test_publication_policy.py::PublicationPolicyContract.test_rule_inventory_reads_the_next_page_before_deciding_automation_is_allowed
#[test]
fn rule_inventory_reads_the_next_page_before_deciding_automation_is_allowed() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    let policy = github(&mut w, &repo, &bank, Some(&peer), false)?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nA durable independent update.\n",
    )?;
    let before = state(&w, &repo, &bank)?;
    let mut rules: Vec<Value> = (0..100)
        .map(|_| obj([("type", s("non_fast_forward"))]))
        .collect();
    rules.push(obj([("type", s("required_deployments"))]));
    edit_policy(&policy, &[("rules", Value::Array(rules))])?;
    let prepared = prepared(&w, &repo)?;
    assert_eq!(text(get(&prepared, "sharing_state")), "pending-policy");
    assert!(
        get(&prepared, "unsupported_rules")
            .as_array()
            .unwrap_or(&[])
            .contains(&s("required_deployments"))
    );
    let d = read_policy(&policy)?;
    assert!(get(&d, "events").as_array().unwrap_or(&[]).iter().any(|e| {
        text(get(e, "endpoint"))
            .rsplit("repos/fixture/memory/")
            .next()
            == Some("rules/branches/main?per_page=100&page=2")
    }));
    assert!(only_reads(&policy)?);
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_publication_policy.py::PublicationPolicyContract.test_clean_rollup_cannot_substitute_for_missing_or_wrong_app_required_checks
#[test]
fn clean_rollup_cannot_substitute_for_missing_or_wrong_app_required_checks() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    let policy = github(&mut w, &repo, &bank, Some(&peer), true)?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nA source-linked update requiring the configured CI.\n",
    )?;
    edit_policy(
        &policy,
        &[
            ("review", s("APPROVED")),
            ("missing_required_check", Value::Bool(true)),
        ],
    )?;
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let before = state(&w, &repo, &bank)?;
    let head = remote_head(&w, &repo)?;
    let missing = land(&w, &repo, &request)?;
    assert_eq!(get(&missing, "publication_verified"), &Value::Bool(false));
    assert!(
        text(get(&missing, "reason"))
            .to_lowercase()
            .contains("required status")
    );
    edit_policy(
        &policy,
        &[
            ("missing_required_check", Value::Bool(false)),
            (
                "rules",
                Value::Array(vec![obj([
                    ("type", s("required_status_checks")),
                    (
                        "parameters",
                        obj([(
                            "required_status_checks",
                            Value::Array(vec![obj([
                                ("context", s("fixture-ci")),
                                ("integration_id", int(2)),
                            ])]),
                        )]),
                    ),
                ])]),
            ),
        ],
    )?;
    let wrong = land(&w, &repo, &request)?;
    assert_eq!(get(&wrong, "publication_verified"), &Value::Bool(false));
    assert_eq!(head, remote_head(&w, &repo)?);
    edit_policy(&policy, &[("check_app", int(2))])?;
    let landed = land(&w, &repo, &request)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

fn restricted_publication(w: &mut World) -> TestResult<(PathBuf, PathBuf, PathBuf)> {
    let (repo, bank, peer, _) = fixture(w)?;
    let policy = github(w, &repo, &bank, Some(&peer), true)?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nIndependent reviewed learning.\n",
    )?;
    let allowed = user(USER_ID, "allowed-user");
    edit_policy(
        &policy,
        &[
            (
                "restrictions",
                obj([
                    ("users", Value::Array(vec![allowed.clone()])),
                    ("teams", Value::Array(vec![])),
                    ("apps", Value::Array(vec![])),
                ]),
            ),
            ("allowed_users", Value::Array(vec![allowed])),
        ],
    )?;
    Ok((repo, bank, policy))
}

// obligation: tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_allowed_user_still_waits_for_reviews_and_checks_then_lands
#[test]
fn allowed_user_still_waits_for_reviews_and_checks_then_lands() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, policy) = restricted_publication(&mut w)?;
    let before = state(&w, &repo, &bank)?;
    let prepared = prepared(&w, &repo)?;
    assert_eq!(
        get(
            get(get(&prepared, "repository_policy"), "push_actor"),
            "eligible"
        ),
        &Value::Bool(true)
    );
    let request = get(&prepared, "request").clone();
    let pending = land(&w, &repo, &request)?;
    assert_eq!(
        text(get(&pending, "sharing_state")),
        "pending-repository-rules"
    );
    edit_policy(
        &policy,
        &[("review", s("APPROVED")), ("checks", s("PENDING"))],
    )?;
    assert_eq!(
        get(&land(&w, &repo, &request)?, "publication_verified"),
        &Value::Bool(false)
    );
    edit_policy(&policy, &[("checks", s("SUCCESS"))])?;
    assert_eq!(
        get(&land(&w, &repo, &request)?, "publication_verified"),
        &Value::Bool(true)
    );
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_all_negative_identity_and_inventory_cases_refuse_prepare
#[test]
fn all_negative_identity_and_inventory_cases_refuse_prepare() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, policy) = restricted_publication(&mut w)?;
    let baseline = read_policy(&policy)?;
    let proof = proof(&w, &repo)?;
    let before = state(&w, &repo, &bank)?;
    let allowed = user(USER_ID, "allowed-user");
    let cases: Vec<Vec<(&str, Value)>> = vec![
        vec![("allowed_users", Value::Array(vec![]))],
        vec![("actor", user(8, "allowed-user"))],
        vec![("actor", user(USER_ID, "different-user"))],
        vec![("actor_unavailable", Value::Bool(true))],
        vec![("actor_status", int(201))],
        vec![(
            "allowed_users",
            Value::Array(vec![allowed.clone(), allowed.clone()]),
        )],
        vec![(
            "allowed_users",
            Value::Array(vec![obj([
                ("id", Value::Bool(true)),
                ("login", s("allowed-user")),
                ("type", s("User")),
            ])]),
        )],
        vec![(
            "allowed_users",
            obj([("users", Value::Array(vec![allowed.clone()]))]),
        )],
        vec![(
            "restrictions",
            obj([("users", Value::Array(vec![allowed.clone()]))]),
        )],
        vec![
            ("allowed_users", Value::Array(vec![])),
            (
                "restrictions",
                obj([
                    ("users", Value::Array(vec![])),
                    ("teams", Value::Array(vec![obj([("id", int(7))])])),
                    ("apps", Value::Array(vec![])),
                ]),
            ),
        ],
        vec![
            ("allowed_users", Value::Array(vec![])),
            (
                "restrictions",
                obj([
                    ("users", Value::Array(vec![])),
                    ("teams", Value::Array(vec![])),
                    ("apps", Value::Array(vec![obj([("id", int(7))])])),
                ]),
            ),
        ],
        vec![("allowlist_failed_page", int(1))],
    ];
    for case in cases {
        let mut d = baseline.clone();
        for (key, value) in &case {
            set(&mut d, key, value.clone());
        }
        fs::write(&policy, encode(&d)?)?;
        let (_, result) = publish(&w, &repo, "prepare", &proof)?;
        assert_eq!(text(get(&result, "sharing_state")), "pending-policy");
        assert_eq!(get(&result, "publication_verified"), &Value::Bool(false));
        assert!(result.get("request").is_none());
        assert!(
            get(&result, "unsupported_rules")
                .as_array()
                .unwrap_or(&[])
                .contains(&s("restricted-push-actor-unverified"))
        );
        assert!(get(&result, "policy_failure").get("repair").is_some());
        assert!(only_reads(&policy)?);
        assert_eq!(before, state(&w, &repo, &bank)?);
    }
    Ok(())
}

// obligation: tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_complete_later_page_succeeds_but_incomplete_chains_refuse
#[test]
fn complete_later_page_succeeds_but_incomplete_chains_refuse() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, policy) = restricted_publication(&mut w)?;
    let users: Vec<Value> = (0..400)
        .map(|n| user(n + 100, &format!("user-{n}")))
        .collect();
    let mut first = users.get(..100).unwrap_or(&[]).to_vec();
    first.push(user(USER_ID, "Allowed-User"));
    edit_policy(&policy, &[("allowed_users", Value::Array(first))])?;
    let prepared = prepared(&w, &repo)?;
    assert_eq!(text(get(&prepared, "sharing_state")), "prepared");
    let proof = get(get(&prepared, "request"), "proof").clone();
    let before = state(&w, &repo, &bank)?;
    let baseline = read_policy(&policy)?;
    let mut all = users.clone();
    all.push(user(USER_ID, "allowed-user"));
    let mut duplicate = users.get(..100).unwrap_or(&[]).to_vec();
    duplicate.push(users.first().cloned().unwrap_or(Value::Null));
    for (key, value) in [
        ("allowlist_failed_page", int(2)),
        ("allowlist_status", obj([("2", int(201))])),
        ("broken_next", Value::Bool(true)),
        ("allowed_users", Value::Array(all)),
        ("allowed_users", Value::Array(duplicate)),
    ] {
        let mut d = baseline.clone();
        set(&mut d, key, value);
        fs::write(&policy, encode(&d)?)?;
        let (_, result) = publish(&w, &repo, "prepare", &proof)?;
        assert_eq!(text(get(&result, "sharing_state")), "pending-policy");
        assert_eq!(get(&result, "publication_verified"), &Value::Bool(false));
        assert_eq!(before, state(&w, &repo, &bank)?);
    }
    Ok(())
}

// obligation: tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_actor_revoked_after_prepare_and_transport_rejection_stay_unpublished
#[test]
fn actor_revoked_after_prepare_and_transport_rejection_stay_unpublished() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, policy) = restricted_publication(&mut w)?;
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let before = state(&w, &repo, &bank)?;
    let head = remote_head(&w, &repo)?;
    edit_policy(&policy, &[("allowed_users", Value::Array(vec![]))])?;
    assert_eq!(
        text(get(&land(&w, &repo, &request)?, "sharing_state")),
        "pending-policy"
    );
    edit_policy(
        &policy,
        &[
            (
                "allowed_users",
                Value::Array(vec![user(USER_ID, "allowed-user")]),
            ),
            ("review", s("APPROVED")),
            ("transport_actor_id", int(8)),
        ],
    )?;
    let rejected = land(&w, &repo, &request)?;
    assert_eq!(get(&rejected, "publication_verified"), &Value::Bool(false));
    assert!(text(get(&rejected, "reason")).contains("different credentials"));
    assert_eq!(head, remote_head(&w, &repo)?);
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_publication_restricted_actor.py::RestrictedActorContract.test_immediate_final_recheck_refuses_actor_allowlist_or_restriction_change
#[test]
fn immediate_final_recheck_refuses_actor_allowlist_or_restriction_change() -> TestResult {
    for field in [
        "change_actor_at",
        "change_allowlist_at",
        "remove_restrictions_at",
    ] {
        let mut w = World::new()?;
        let (repo, bank, policy) = restricted_publication(&mut w)?;
        let request = get(&prepared(&w, &repo)?, "request").clone();
        let before = state(&w, &repo, &bank)?;
        let head = remote_head(&w, &repo)?;
        let d = read_policy(&policy)?;
        let counter = if field == "change_actor_at" {
            "actor_reads"
        } else {
            "protection_reads"
        };
        let reads = get(&d, counter)
            .as_int()
            .and_then(|n| n.as_i64())
            .unwrap_or(0);
        edit_policy(
            &policy,
            &[("review", s("APPROVED")), (field, int(reads + 2))],
        )?;
        let (code, result) = publish(&w, &repo, "land", &request)?;
        assert_eq!(code, 1, "{field}");
        assert_eq!(get(&result, "publication_verified"), &Value::Bool(false));
        assert_eq!(head, remote_head(&w, &repo)?);
        assert_eq!(before, state(&w, &repo, &bank)?);
    }
    Ok(())
}

// obligation: tests/contracts/test_publication_pending.py::PublicationPendingContract.test_peer_bank_at_another_root_and_its_later_removal_do_not_authorize_a_second_bank
#[test]
fn peer_bank_at_another_root_and_its_later_removal_do_not_authorize_a_second_bank() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _) = bootstrap(&mut w)?;
    let policy = github(&mut w, &repo, &bank, None, false)?;
    let proof = proof(&w, &repo)?;
    let before = state(&w, &repo, &bank)?;
    let remote = text(get(&read_policy(&policy)?, "remote")).to_owned();
    let peer = w.clone(&remote, "other-bank-peer")?;
    let other = peer.join("docs/shared memory");
    let mut files = BTreeMap::new();
    files_below(&bank, &bank, &mut files)?;
    for (path, raw) in files.iter().filter(|(p, _)| p.ends_with(".md")) {
        let target = other.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = String::from_utf8(raw.clone())?;
        let body = if path == "MEMORY.md" {
            body.split_inclusive('\n')
                .map(|line| {
                    if line.starts_with("project-id: ") {
                        format!(
                            "project-id: \"{}\"\n",
                            agent_memory::host::uuid4().unwrap_or_default()
                        )
                    } else {
                        line.to_owned()
                    }
                })
                .collect()
        } else {
            body
        };
        fs::write(target, body)?;
    }
    git(&w, &peer, &["add", "docs/shared memory"])?;
    git(
        &w,
        &peer,
        &[
            "commit",
            "-m",
            "Peer explicitly initialized shared custom bank",
        ],
    )?;
    git(&w, &peer, &["push", "origin", "HEAD:main"])?;
    for removed in [false, true] {
        if removed {
            git(&w, &peer, &["rm", "-r", "docs/shared memory"])?;
            git(
                &w,
                &peer,
                &[
                    "commit",
                    "-m",
                    "Retired custom bank remains historical ownership",
                ],
            )?;
            git(&w, &peer, &["push", "origin", "HEAD:main"])?;
        }
        let (code, value) = publish(&w, &repo, "prepare", &proof)?;
        assert_eq!(code, 1);
        assert!(value.get("request").is_none());
        assert_eq!(get(&value, "publication_verified"), &Value::Bool(false));
        assert_eq!(before, state(&w, &repo, &bank)?);
    }
    Ok(())
}

// obligation: tests/contracts/test_publication_pending.py::PublicationPendingContract.test_discarded_side_bank_is_not_invented_as_historical_authority_ownership
#[test]
fn discarded_side_bank_is_not_invented_as_historical_authority_ownership() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _) = bootstrap(&mut w)?;
    let policy = github(&mut w, &repo, &bank, None, false)?;
    let remote = text(get(&read_policy(&policy)?, "remote")).to_owned();
    let peer = w.clone(&remote, "discarded-bank-peer")?;
    git(&w, &peer, &["checkout", "-b", "unlanded-bank"])?;
    let other = peer.join("docs/shared memory");
    let mut files = BTreeMap::new();
    files_below(&bank, &bank, &mut files)?;
    for (path, raw) in &files {
        let target = other.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, raw)?;
    }
    git(&w, &peer, &["add", "docs/shared memory"])?;
    git(
        &w,
        &peer,
        &["commit", "-m", "Proposed bank only on side branch"],
    )?;
    git(&w, &peer, &["checkout", "main"])?;
    git(
        &w,
        &peer,
        &[
            "merge",
            "--no-ff",
            "--strategy=ours",
            "unlanded-bank",
            "-m",
            "Discard proposed bank from actual authority",
        ],
    )?;
    git(&w, &peer, &["push", "origin", "HEAD:main"])?;
    let before = state(&w, &repo, &bank)?;
    let prepared = prepared(&w, &repo)?;
    assert_eq!(text(get(&prepared, "sharing_state")), "prepared");
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_publication_pending.py::PublicationPendingContract.test_offline_permission_source_and_unsupported_policy_failures_keep_local_memory_pending
#[test]
fn offline_permission_source_and_unsupported_policy_failures_keep_local_memory_pending()
-> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    let policy = github(&mut w, &repo, &bank, Some(&peer), false)?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nRetain this admitted local learning while offline.\n",
    )?;
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let before = state(&w, &repo, &bank)?;
    let remote = PathBuf::from(text(get(&read_policy(&policy)?, "remote")));
    let away = remote.with_file_name("offline.git");
    let head = remote_head(&w, &repo)?;
    fs::rename(&remote, &away)?;
    let (code, value) = publish(&w, &repo, "land", &request)?;
    assert_eq!(code, 1);
    assert_eq!(get(&value, "publication_verified"), &Value::Bool(false));
    assert_eq!(get(&value, "local_memory_available"), &Value::Bool(true));
    fs::rename(&away, &remote)?;
    assert_eq!(before, state(&w, &repo, &bank)?);
    edit_policy(&policy, &[("push", Value::Bool(false))])?;
    let (code, value) = publish(&w, &repo, "land", &request)?;
    assert_eq!(code, 1);
    assert_eq!(get(&value, "publication_verified"), &Value::Bool(false));
    assert_eq!(before, state(&w, &repo, &bank)?);
    edit_policy(&policy, &[("push", Value::Bool(true))])?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nNew source evidence requires a fresh semantic review.\n",
    )?;
    let pending = land(&w, &repo, &request)?;
    assert_eq!(
        text(get(&pending, "sharing_state")),
        "pending-reconciliation"
    );
    edit_policy(
        &policy,
        &[(
            "rules",
            Value::Array(vec![obj([("type", s("required_deployments"))])]),
        )],
    )?;
    let unsupported = prepared(&w, &repo)?;
    assert_eq!(text(get(&unsupported, "sharing_state")), "pending-policy");
    assert_eq!(
        get(&unsupported, "publication_verified"),
        &Value::Bool(false)
    );
    assert_eq!(head, remote_head(&w, &repo)?);
    assert!(fs::read_to_string(bank.join("activeContext.md"))?.contains("New source evidence"));
    let after = state(&w, &repo, &bank)?;
    assert_eq!(
        (&before.1, &before.2, &before.3, &before.4),
        (&after.1, &after.2, &after.3, &after.4)
    );
    Ok(())
}

// obligation: tests/contracts/test_publication_pending.py::PublicationPendingContract.test_removed_authority_bank_is_not_treated_as_first_initialization
#[test]
fn removed_authority_bank_is_not_treated_as_first_initialization() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    let before = state(&w, &repo, &bank)?;
    git(&w, &peer, &["rm", "-r", "memory-bank"])?;
    git(
        &w,
        &peer,
        &["commit", "-m", "Authority bank removal requires inspection"],
    )?;
    git(&w, &peer, &["push", "origin", "HEAD:main"])?;
    let (code, value) = call(
        &w,
        "am semantic",
        &[os("inspect"), os("--repo"), repo.as_os_str()],
    )?;
    assert_eq!(code, 1);
    assert_eq!(get(&value, "publication_verified"), &Value::Bool(false));
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_publication_pending.py::PublicationPendingContract.test_tampered_candidate_with_identical_memory_but_feature_code_is_refused
#[test]
fn tampered_candidate_with_identical_memory_but_feature_code_is_refused() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nPublish this memory but never feature code.\n",
    )?;
    let mut request = get(&prepared(&w, &repo)?, "request").clone();
    let before = state(&w, &repo, &bank)?;
    let private_index = w.root.join("attacker-fixture-index");
    let code = w.root.join("unrelated-feature-content");
    fs::write(
        &code,
        "VALUE = 12345  # unrelated feature code must not land\n",
    )?;
    w.env
        .insert("GIT_INDEX_FILE".into(), private_index.into_os_string());
    let invalid = (|| -> TestResult<String> {
        git(&w, &repo, &["read-tree", text(get(&request, "candidate"))])?;
        let blob = git(&w, &repo, &["hash-object", "-w", &code.to_string_lossy()])?;
        let info = format!("100644,{},code.py", blob.trim());
        git(&w, &repo, &["update-index", "--add", "--cacheinfo", &info])?;
        let tree = git(&w, &repo, &["write-tree"])?;
        Ok(git(
            &w,
            &repo,
            &[
                "commit-tree",
                tree.trim(),
                "-p",
                text(get(&request, "base")),
                "-m",
                "Same memory with unrelated feature code",
            ],
        )?
        .trim()
        .to_owned())
    })();
    w.env.remove(OsStr::new("GIT_INDEX_FILE"));
    set(&mut request, "candidate", s(invalid?));
    let (status, value) = publish(&w, &repo, "land", &request)?;
    assert_eq!(status, 1);
    assert_eq!(get(&value, "publication_verified"), &Value::Bool(false));
    assert_eq!(before, state(&w, &repo, &bank)?);
    assert_eq!(
        remote_head(&w, &repo)?.split_whitespace().next(),
        Some(text(get(&request, "base")))
    );
    Ok(())
}

// obligation: tests/contracts/test_publication_recovery.py::PublicationRecoveryContract.test_lost_response_inventory_does_not_duplicate_or_reverse_a_later_correction
#[test]
fn lost_response_inventory_does_not_duplicate_or_reverse_a_later_correction() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nA reviewed discovery independent of unfinished code.\n",
    )?;
    let before = state(&w, &repo, &bank)?;
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let path = save(&w, &request, "lost-response-request.json")?;
    let argv: Vec<OsString> = [
        w.env[OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone(),
        OsString::from("--json"),
    ]
    .into_iter()
    .chain(native_arguments(
        "am publish",
        [
            os("land"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            path.as_os_str(),
        ],
    )?)
    .collect();
    let failed = w.run_with(
        argv,
        RunOptions {
            env: &[
                (
                    "AGENT_MEMORY_FAULT_STAGE".into(),
                    "publication-after-push".into(),
                ),
                ("AGENT_MEMORY_FAULT_MODE".into(), "exit".into()),
            ],
            check: false,
            timeout: Duration::from_secs(120),
            ..RunOptions::default()
        },
    )?;
    assert_eq!(failed.status, agent_memory::process::Status::Code(86));
    assert_eq!(
        remote_head(&w, &repo)?.split_whitespace().next(),
        Some(text(get(&request, "candidate")))
    );
    let state_home = PathBuf::from(
        w.env
            .get(OsStr::new("AGENT_MEMORY_STATE_HOME"))
            .cloned()
            .unwrap_or_default(),
    );
    fs::remove_dir_all(state_home)?;
    // Whole-state loss also removes installation metadata. Reinstall the real
    // retained source before ordinary readback; never restore a copied receipt.
    let source = w.root.join("dev-source");
    let bin_dir = w.root.join("home/.local/bin");
    let binary = source.join("target/release/am");
    for name in agent_memory::console::NAMES {
        let command = bin_dir.join(name);
        assert!(fs::symlink_metadata(&command)?.file_type().is_symlink());
        assert_eq!(fs::canonicalize(&command)?, binary);
        fs::remove_file(command)?;
    }
    let installed = w.run_with(
        [
            binary.as_os_str(),
            os("--json"),
            os("install"),
            os("--development-source"),
            source.as_os_str(),
            os("--mode"),
            os("link"),
            os("--presets"),
            os("none"),
            os("--no-guidance"),
            os("--bin-dir"),
            bin_dir.as_os_str(),
        ],
        RunOptions {
            check: false,
            timeout: Duration::from_secs(120),
            ..RunOptions::default()
        },
    )?;
    assert_eq!(installed.status, agent_memory::process::Status::Code(0));
    assert_eq!(
        get(&decode(&installed.stdout)?, "verified"),
        &Value::Bool(true)
    );
    let first = land(&w, &repo, &request)?;
    assert_eq!(text(get(&first, "sharing_state")), "already-landed");
    assert_eq!(get(&first, "publication_verified"), &Value::Bool(true));
    git(&w, &peer, &["pull", "--ff-only"])?;
    fs::write(
        peer.join("memory-bank/activeContext.md"),
        "# Current focus\n\nA later evidence-backed correction stays authoritative.\n",
    )?;
    git(&w, &peer, &["add", "memory-bank/activeContext.md"])?;
    git(&w, &peer, &["commit", "-m", "Later actual correction"])?;
    git(&w, &peer, &["push", "origin", "HEAD:main"])?;
    let actual = remote_head(&w, &repo)?;
    let again = land(&w, &repo, &request)?;
    assert_eq!(get(&again, "publication_verified"), &Value::Bool(true));
    assert_eq!(get(&again, "intended_delta_current"), &Value::Bool(false));
    assert_eq!(get(&again, "witnessed_commit"), get(&request, "candidate"));
    assert_eq!(actual, remote_head(&w, &repo)?);
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_publication_recovery.py::PublicationRecoveryContract.test_competing_main_during_push_is_rejected_then_fresh_reconciliation_preserves_authority_code
#[test]
fn competing_main_during_push_is_rejected_then_fresh_reconciliation_preserves_authority_code()
-> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    let policy = github(&mut w, &repo, &bank, Some(&peer), false)?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nIndependent local learning is ready.\n",
    )?;
    fs::write(
        peer.join("memory-bank/projectbrief.md"),
        "# Project\n\nCompatible independently landed project learning.\n",
    )?;
    fs::write(
        peer.join("code.py"),
        "VALUE = 900  # newly landed authority code\n",
    )?;
    git(
        &w,
        &peer,
        &["add", "memory-bank/projectbrief.md", "code.py"],
    )?;
    git(&w, &peer, &["commit", "-m", "Competing actual authority"])?;
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let before = state(&w, &repo, &bank)?;
    let flag = w.root.join("competing-main-ran");
    let real = text(get(&read_policy(&policy)?, "real_git")).to_owned();
    // A local pre-push hook pushes the peer's competing main exactly once.
    let hook = format!(
        "#!/bin/sh\nwhile read local_ref local_sha remote_ref remote_sha; do\n if [ \"$remote_ref\" = refs/heads/main ] && [ ! -e {flag} ]; then\n  : > {flag}\n  env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE -u GIT_PREFIX {git} -C {peer} push origin HEAD:main >/dev/null 2>&1 || exit 1\n fi\ndone\nexit 0\n",
        flag = shell_quote(&flag.to_string_lossy()),
        git = shell_quote(&real),
        peer = shell_quote(&peer.to_string_lossy()),
    );
    executable(&repo.join(".git/hooks/pre-push"), &hook)?;
    let pending = land(&w, &repo, &request)?;
    assert!(flag.exists());
    assert_eq!(get(&pending, "publication_verified"), &Value::Bool(false));
    assert_eq!(
        text(get(&pending, "sharing_state")),
        "pending-reconciliation"
    );
    assert_eq!(before, state(&w, &repo, &bank)?);
    let fresh = get(&prepared(&w, &repo)?, "request").clone();
    assert_ne!(get(&fresh, "base"), get(&request, "base"));
    let landed = land(&w, &repo, &fresh)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    let actual = text(get(&landed, "witnessed_commit"));
    assert!(git(&w, &repo, &["show", &format!("{actual}:code.py")])?.contains("VALUE = 900"));
    assert!(
        git(
            &w,
            &repo,
            &["show", &format!("{actual}:memory-bank/activeContext.md")]
        )?
        .contains("Independent local learning")
    );
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_publication_worktree.py::PublicationWorktreeContract.test_linked_worktree_uses_a_private_authority_index_and_does_not_push_follow_tags
#[test]
fn linked_worktree_uses_a_private_authority_index_and_does_not_push_follow_tags() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, base) = bootstrap(&mut w)?;
    github(&mut w, &repo, &bank, None, false)?;
    let sibling = w.root.join("separate worktree");
    git(
        &w,
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "session-two",
            &sibling.to_string_lossy(),
            &base,
        ],
    )?;
    let other_bank = sibling.join("memory-bank");
    let mut files = BTreeMap::new();
    files_below(&bank, &bank, &mut files)?;
    for (path, raw) in &files {
        let target = other_bank.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, raw)?;
    }
    fs::write(
        sibling.join("code.py"),
        "VALUE = 42  # staged sibling work\n",
    )?;
    git(&w, &sibling, &["add", "code.py"])?;
    git(
        &w,
        &sibling,
        &["config", "--local", "push.followTags", "true"],
    )?;
    git(
        &w,
        &sibling,
        &[
            "tag",
            "-a",
            "unrelated-local-tag",
            &base,
            "-m",
            "Must not publish an unrelated tag",
        ],
    )?;
    let before = state(&w, &sibling, &other_bank)?;
    let original = state(&w, &repo, &bank)?;
    let request = get(&prepared(&w, &sibling)?, "request").clone();
    let landed = land(&w, &sibling, &request)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    assert_eq!(git(&w, &sibling, &["ls-remote", "--tags", "origin"])?, "");
    assert_eq!(before, state(&w, &sibling, &other_bank)?);
    assert_eq!(original, state(&w, &repo, &bank)?);
    let candidate = text(get(&request, "candidate"));
    assert_eq!(
        git(&w, &sibling, &["show", &format!("{candidate}:code.py")])?,
        "VALUE = 1\n"
    );
    Ok(())
}

// obligation: tests/contracts/test_publication_admission.py::PublicationAdmissionContract.test_admitted_unmerged_discovery_keeps_exact_conditions_evidence_origin_and_history
#[test]
fn admitted_unmerged_discovery_keeps_exact_conditions_evidence_origin_and_history() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    git(&w, &repo, &["checkout", "-b", "unfinished-experiment"])?;
    git(&w, &repo, &["commit", "-m", "Unfinished feature ancestor"])?;
    fs::write(
        repo.join("code.py"),
        "VALUE = 3  # additional staged unfinished code\n",
    )?;
    git(&w, &repo, &["add", "code.py"])?;
    let evidence = b"# Isolated observed experiment\n\nUnder the named component conditions the measured retry cap is three.\n";
    fs::write(repo.join("evidence.md"), evidence)?;
    let scope = good(
        &w,
        "am identity",
        &[os("status"), os("--repo"), repo.as_os_str()],
    )?;
    let project = get(&scope, "project_id").clone();
    let actor = get(&scope, "developer_id").clone();
    let mut candidate = json::decode(
        include_str!("../skills/agent-memory/templates/record-proposal.example.json"),
        DecodePolicy::default(),
    )?;
    set(
        &mut candidate,
        "body",
        s("# Retry policy\n\nUse at most three attempts for this fixture.\n"),
    );
    let metadata = candidate.get_mut("metadata").unwrap();
    set(metadata, "project-id", project.clone());
    set(metadata, "id", s(agent_memory::host::uuid4()?));
    set(metadata, "trust", s("reviewed"));
    set(metadata, "visibility", s("visible"));
    set(
        metadata,
        "admission-reason",
        s("Disposable structural fixture; not real admission evidence"),
    );
    set(metadata, "evidence", Value::Array(vec![s("evidence.md")]));
    set(metadata, "scope", s("repository"));
    set(
        metadata,
        "source",
        obj([
            ("kind", s("verified-repository")),
            ("actor", s("Explicit fixture source; not authenticated")),
        ]),
    );
    set(
        metadata,
        "conditions",
        obj([
            ("component", s("code.py")),
            ("branch", s("unfinished-experiment")),
        ]),
    );
    let record_id = text(get(metadata, "id")).to_owned();
    let origin = metadata.get_mut("origin").unwrap();
    set(origin, "developer-id", actor);
    set(origin, "code-state", s("unmerged"));
    set(origin, "harness", s("custom-fixture"));
    set(origin, "session", s("unfinished-experiment"));
    let request = obj([
        ("project_id", project.clone()),
        ("candidate", candidate.clone()),
        ("source_class", s("verified-repository")),
        (
            "repository_evidence",
            obj([
                ("path", s("evidence.md")),
                (
                    "sha256",
                    s(agent_memory::release_distribution::digest(evidence)),
                ),
            ]),
        ),
        (
            "assessment",
            obj([
                ("category", s("durable-discovery")),
                (
                    "utility",
                    s("Prevent repeated excessive retries under these conditions."),
                ),
                ("durable", Value::Bool(true)),
                ("evidence_confidence", s("confirmed")),
                (
                    "evidence_reason",
                    s("The operator explicitly confirmed the correction."),
                ),
                ("confirmed", Value::Bool(true)),
                ("source_redundant", Value::Bool(false)),
                ("work_state_copy", Value::Bool(false)),
                (
                    "review",
                    s(
                        "Reviewed the actual fixture experiment and its component applicability; feature code remains unmerged.",
                    ),
                ),
            ]),
        ),
    ]);
    let request_path = save(&w, &request, "admission-request.json")?;
    let capture = good(
        &w,
        "am capture",
        &[
            os("--repo"),
            repo.as_os_str(),
            os("--request"),
            request_path.as_os_str(),
        ],
    )?;
    assert_eq!(get(&capture, "decision"), &s("eligible"));
    let comparison = obj([
        ("project_id", project),
        ("candidate", candidate),
        (
            "searches",
            Value::Array(vec![s("retry policy"), s("attempt cap")]),
        ),
        ("read_paths", Value::Array(vec![])),
    ]);
    let compared_path = save(&w, &comparison, "comparison-request.json")?;
    let inspected = good(
        &w,
        "am compare",
        &[
            os("inspect"),
            os("--repo"),
            repo.as_os_str(),
            os("--request"),
            compared_path.as_os_str(),
        ],
    )?;
    let decisions = get(&inspected, "matches")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            obj([
                ("path", get(row, "path").clone()),
                ("classification", s("distinct-knowledge")),
                (
                    "rationale",
                    s("The fixture meaning has been assessed with its evidence and conditions."),
                ),
            ])
        })
        .collect();
    let judgment = obj([
        ("observation", get(&inspected, "observation").clone()),
        ("decisions", Value::Array(decisions)),
        ("pending_decisions", Value::Array(vec![])),
        (
            "rationale",
            s("Read the actual bounded plausible sources before deciding novelty."),
        ),
    ]);
    let judgment_path = save(&w, &judgment, "admission-judgment.json")?;
    let reviewed = good(
        &w,
        "am compare",
        &[
            os("review"),
            os("--repo"),
            repo.as_os_str(),
            os("--request"),
            compared_path.as_os_str(),
            os("--judgment"),
            judgment_path.as_os_str(),
        ],
    )?;
    let capture_path = save(&w, &capture, "admission-capture.json")?;
    let proof_path = save(&w, &reviewed, "admission-proof.json")?;
    let preview = good(
        &w,
        "am admission",
        &[
            os("preview"),
            os("--repo"),
            repo.as_os_str(),
            os("--capture"),
            capture_path.as_os_str(),
            os("--request"),
            compared_path.as_os_str(),
            os("--proof"),
            proof_path.as_os_str(),
        ],
    )?;
    let proposal = save(&w, get(&preview, "proposal"), "admission-proposal.json")?;
    let admitted = good(
        &w,
        "am admission",
        &[os("apply"), os("--proposal"), proposal.as_os_str()],
    )?;
    assert_eq!(get(&admitted, "admission_verified"), &Value::Bool(true));
    let before = state(&w, &repo, &bank)?;
    let feature = git(&w, &repo, &["rev-parse", "HEAD"])?.trim().to_owned();
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let landed = land(&w, &repo, &request)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    assert_eq!(
        get(&landed, "feature_code_freshness_verified"),
        &Value::Bool(false)
    );
    let commit = text(get(&request, "candidate"));
    for (path, content) in &before.0 {
        let actual = git(
            &w,
            &repo,
            &["show", &format!("{commit}:memory-bank/{path}")],
        )?;
        if path == "MEMORY.md" {
            let fields = agent_memory::repository_config::ROUTER_FIELDS;
            let (actual, _) = agent_memory::codec::frontmatter::read(&actual, fields)?;
            let (expected, _) =
                agent_memory::codec::frontmatter::read(std::str::from_utf8(content)?, fields)?;
            assert!(agent_memory::installation::same(
                &Value::Object(actual),
                &Value::Object(expected)
            ));
        } else {
            assert_eq!(actual.as_bytes(), content);
        }
    }
    assert!(fs::read_to_string(bank.join(format!("topics/{record_id}.md")))?.contains("unmerged"));
    assert!(
        !git(&w, &repo, &["rev-list", commit])?
            .lines()
            .any(|line| line == feature)
    );
    assert_eq!(
        git(&w, &repo, &["show", &format!("{commit}:code.py")])?,
        "VALUE = 1\n"
    );
    assert_eq!(before, state(&w, &repo, &bank)?);
    Ok(())
}

// obligation: tests/contracts/test_publication_hold_recovery.py::PublicationHoldRecoveryContract.test_missing_private_hold_state_reports_specific_recovery_and_keeps_pending_work
#[test]
fn missing_private_hold_state_reports_specific_recovery_and_keeps_pending_work() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    let marker = format!("sk-{}", "X".repeat(24));
    let project =
        support::record_fixtures::project_id(&fs::read_to_string(bank.join("MEMORY.md"))?)?;
    let body = support::record_fixtures::record_text(
        &format!("# Isolated evidence\n\n{marker}\n"),
        project.as_str().unwrap(),
    )?;
    fs::create_dir_all(bank.join("topics"))?;
    let held = "topics/isolated-local.md";
    fs::write(bank.join(held), body)?;
    let router = fs::read_to_string(bank.join("MEMORY.md"))?;
    fs::write(bank.join("MEMORY.md"), router.replace("\n## Omitted contexts\n",
        &format!("| [{held}](<{held}>) | Scoped reviewed lesson | repository | active | When conditions match |\n\n## Omitted contexts\n")))?;
    fs::write(
        bank.join("activeContext.md"),
        "# Current focus\n\nIndependent local learning.\n",
    )?;
    fs::write(
        peer.join("memory-bank/projectbrief.md"),
        "# Project\n\nIndependent peer learning.\n",
    )?;
    git(&w, &peer, &["add", "memory-bank"])?;
    git(&w, &peer, &["commit", "-m", "Independent peer learning"])?;
    git(&w, &peer, &["push", "origin", "HEAD:main"])?;
    let first_path = w.inside("first-inspection.json")?;
    good(
        &w,
        "am semantic",
        &[
            os("inspect-v2"),
            os("--repo"),
            repo.as_os_str(),
            os("--output"),
            first_path.as_os_str(),
        ],
    )?;
    let first = decode(&fs::read(&first_path)?)?;
    let rows: BTreeMap<_, _> = get(&first, "sources")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (text(get(row, "key")), row))
        .collect();
    let mut request = get(&first, "request").clone();
    let selections = get(&first, "changed")
        .as_array()
        .unwrap()
        .iter()
        .map(|key| {
            let key = text(key);
            let row = rows[key];
            let subject = get(row, "target_path")
                .as_str()
                .unwrap_or_else(|| text(get(row, "path")));
            (
                key.to_owned(),
                obj([
                    (
                        "searches",
                        Value::Array(vec![s(subject), s(subject.trim_end_matches(".md"))]),
                    ),
                    ("paths", Value::Array(vec![s(subject)])),
                    ("ids", Value::Array(vec![])),
                    ("complete", Value::Bool(true)),
                    ("uncertain", Value::Bool(false)),
                ]),
            )
        })
        .collect();
    set(&mut request, "selections", Value::Object(selections));
    let request_path = save(&w, &request, "versioned-request.json")?;
    let selected_path = w.inside("selected-inspection.json")?;
    let selected = good(
        &w,
        "am semantic",
        &[
            os("inspect-v2"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            request_path.as_os_str(),
            os("--output"),
            selected_path.as_os_str(),
        ],
    )?;
    let view = decode(&fs::read(&selected_path)?)?;
    let mut visible = vec![first.clone(), selected, view.clone()];
    for page in get(&view, "pages").as_array().unwrap() {
        let index = get(page, "index").as_int().unwrap().to_string();
        visible.push(good(
            &w,
            "am semantic",
            &[
                os("page-v2"),
                os("--repo"),
                repo.as_os_str(),
                os("--input"),
                selected_path.as_os_str(),
                os("--page"),
                os(&index),
            ],
        )?);
    }
    let mut review = judgment(&view)?;
    set(
        &mut review,
        "schema",
        s("agent-memory/semantic-sharing-judgment-v2"),
    );
    set(&mut review, "request", request);
    set(
        &mut review,
        "manifest_sha256",
        get(&view, "manifest_sha256").clone(),
    );
    set(
        &mut review,
        "pages",
        Value::Array(
            get(&view, "pages")
                .as_array()
                .unwrap()
                .iter()
                .map(|page| get(page, "sha256").clone())
                .collect(),
        ),
    );
    let input = save(&w, &review, "versioned-judgment.json")?;
    let reviewed_path = w.inside("versioned-reviewed.json")?;
    visible.push(good(
        &w,
        "am semantic",
        &[
            os("review-v2"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
            os("--output"),
            reviewed_path.as_os_str(),
        ],
    )?);
    let reviewed = decode(&fs::read(&reviewed_path)?)?;
    let proof_path = save(&w, get(&reviewed, "proof"), "versioned-proof.json")?;
    let checked = good(
        &w,
        "am semantic",
        &[
            os("check"),
            os("--v2"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            proof_path.as_os_str(),
        ],
    )?;
    assert!(checked.get("refresh_files").is_none() && checked.get("publish_files").is_none());
    visible.push(checked);
    assert!(!encode(&Value::Array(visible))?.contains(&marker));
    assert!(
        !get(&view, "withheld_sources")
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(get(&reviewed, "conflicts_pending"), &Value::Bool(true));
    let prepared = good(
        &w,
        "am publish",
        &[
            os("prepare"),
            os("--v2"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            proof_path.as_os_str(),
        ],
    )?;
    let mut private = BTreeMap::new();
    files_below(&w.paths["state"], &w.paths["state"], &mut private)?;
    let registry = private
        .keys()
        .find(|name| name.contains("semantic-holds/") && name.ends_with("/registry.json"))
        .unwrap();
    let registry = w.paths["state"].join(registry);
    let original = fs::read(&registry)?;
    let journal = registry.with_file_name("journal.json");
    let input = save(&w, get(&prepared, "request"), "held-publication.json")?;
    let request_bytes = fs::read(&input)?;
    let before = state(&w, &repo, &bank)?;
    fs::write(&journal, "pending review evidence")?;
    for code in ["known-holds-interrupted", "known-holds-registry-missing"] {
        let (exit, value) = call(
            &w,
            "am publish",
            &[
                os("land"),
                os("--v2"),
                os("--repo"),
                repo.as_os_str(),
                os("--input"),
                input.as_os_str(),
            ],
        )?;
        assert_eq!(exit, 1, "{value:?}");
        assert_eq!(get(&value, "failure_component"), &s("known-dispute-state"));
        assert_eq!(get(&value, "failure_code"), &s(code));
        assert!(text(get(&value, "repair")).contains("recover-holds"));
        assert_eq!(get(&value, "publication_verified"), &Value::Bool(false));
        assert_eq!(before, state(&w, &repo, &bank)?);
        assert_eq!(fs::read(&input)?, request_bytes);
        if code == "known-holds-interrupted" {
            assert!(journal.exists());
            fs::remove_file(&journal)?;
            fs::remove_file(&registry)?;
        }
    }
    let project = text(
        get(get(get(&reviewed, "proof"), "judgment"), "binding")
            .get("project_id")
            .unwrap(),
    );
    let recovered = good(
        &w,
        "am semantic",
        &[
            os("recover-holds"),
            os("--repo"),
            repo.as_os_str(),
            os("--project-id"),
            os(project),
        ],
    )?;
    assert_eq!(get(&recovered, "verified"), &Value::Bool(true));
    assert_eq!(fs::read(&registry)?, original);
    Ok(())
}

// ---------------------------------------------------------------- layout publication contracts

fn hash_bytes(bytes: &[u8]) -> String {
    agent_memory::developer_identity::digest(Some(bytes)).unwrap()
}
fn hash_value(value: &Value) -> String {
    hash_bytes(&agent_memory::developer_identity::encoded(value).unwrap())
}
fn read_json(path: &Path) -> TestResult<Value> {
    decode(&fs::read(path)?)
}
fn bank_files(bank: &Path) -> TestResult<Value> {
    let mut files = BTreeMap::new();
    files_below(bank, bank, &mut files)?;
    Ok(Value::Object(
        files
            .into_iter()
            .filter(|(p, _)| p.ends_with(".md") && !p.starts_with(".agent-memory"))
            .map(|(p, b)| Ok((p, s(String::from_utf8(b)?))))
            .collect::<TestResult<_>>()?,
    ))
}
fn recovery_call(w: &World, repo: &Path, args: &[&OsStr], expected: i32) -> TestResult<Value> {
    use agent_memory::process::{Input, Spec, Status, Target};
    support::require_world(&w.env, repo)?;
    let argv: Vec<OsString> = [
        w.env[OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone(),
        "--json".into(),
        "bank".into(),
        "recover-layout".into(),
    ]
    .into_iter()
    .chain(args.iter().map(|v| v.to_os_string()))
    .chain(["--repo".into(), repo.as_os_str().to_owned()])
    .collect();
    let env: Vec<_> = w.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    // Baseline World inherits the ordinary creation mask used for these 0644
    // originals. Layout recovery deliberately refuses an unreproducible mode
    // instead of chmod-repairing it (layout_recovery.py:737-740,777-780).
    let output = agent_memory::process::run(&Spec {
        argv: &argv,
        search_path: false,
        env: &env,
        cwd: Some(repo),
        stdin: Input::Null,
        stdout: Target::Capture {
            limit: support::world::CAPTURE_LIMIT,
        },
        stderr: Target::Capture {
            limit: support::world::CAPTURE_LIMIT,
        },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(60))?,
        new_session: false,
        file_size_limit: None,
        umask: Some(0o022),
        pass_fds: &[],
    })?;
    let value = decode(&output.stdout)?;
    assert_eq!(
        output.status,
        Status::Code(expected),
        "{}: {}",
        encode(&value)?,
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(value)
}
fn recovery_output(
    w: &World,
    repo: &Path,
    operation: &str,
    args: &[&OsStr],
    expected: i32,
) -> TestResult<Value> {
    let output = w.root.join(format!(
        "layout-output-{}.json",
        agent_memory::host::uuid4()?
    ));
    let mut argv = args.to_vec();
    argv.extend([
        os("--operation"),
        os(operation),
        os("--output"),
        output.as_os_str(),
    ]);
    let safe = recovery_call(w, repo, &argv, expected)?;
    if expected == 0 {
        read_json(&output)
    } else {
        Ok(safe)
    }
}
fn layout_inspect(w: &World, repo: &Path) -> TestResult<Value> {
    let output = w.root.join("layout-inspected.json");
    recovery_call(
        w,
        repo,
        &[os("inspect"), os("--output"), output.as_os_str()],
        0,
    )?;
    read_json(&output)
}
fn layout_request(w: &World, inspected: &Value, intended: &Value) -> TestResult<Value> {
    let manifest = get(inspected, "manifest");
    let operation = text(get(manifest, "operation_id"));
    let scope = get(get(manifest, "identity"), "scope_context");
    let author = obj([
        ("developer_id", get(scope, "developer_id").clone()),
        ("harness", s("fixture-author")),
        ("session", s(format!("author-{operation}"))),
        (
            "evidence",
            s("Controlled fixture full-source observation, not product acceptance"),
        ),
    ]);
    let mut reviewer = author.clone();
    set(&mut reviewer, "harness", s("fixture-independent-reviewer"));
    set(&mut reviewer, "session", s(format!("reviewer-{operation}")));
    let mut page = 0;
    let mut units = Vec::new();
    for source in get(manifest, "sources").as_array().unwrap() {
        let path = text(get(source, "path"));
        let length = get(source, "byte_count")
            .as_int()
            .unwrap()
            .as_i64()
            .unwrap();
        let pages = (length + 24575) / 24576;
        let pending = intended.get(path).is_none();
        let lengths = if path == "handoff-prompt.md" && length == 28478 {
            vec![3560, 3560, 3560, 3560, 3560, 3560, 3560, 3558]
        } else {
            vec![length]
        };
        let existing = intended
            .get(path)
            .and_then(Value::as_str)
            .and_then(|body| agent_memory::records::parse(body, &[]).ok())
            .filter(|(_, meta, _)| meta.get("id").is_some())
            .map_or(Value::Null, |(_, meta, _)| {
                obj([
                    ("id", get(&meta, "id").clone()),
                    ("version", get(&meta, "version").clone()),
                ])
            });
        let mut offset = 0;
        for size in lengths {
            units.push(obj([
                ("unit_id", s(agent_memory::host::uuid4()?)),
                ("source_ranges", Value::Array(vec![obj([("source_id", get(source, "source_id").clone()), ("start", int(offset)), ("end", int(offset + size))])])),
                ("semantic_kind", s(if pending { "unadmitted-source" } else if path == "MEMORY.md" { "canonical-framing" } else { "canonical-guidance" })),
                ("existing_record", existing.clone()), ("provenance", obj([("origin", s(if pending { "unknown" } else { "existing canonical fixture" })), ("uncertainty", Value::Bool(pending))])),
                ("disposition", s(if pending { "pending-admission" } else { "preserve" })),
                ("targets", Value::Array(if pending { vec![] } else { vec![obj([("path", s(path)), ("sha256", s(hash_bytes(text(get(intended, path)).as_bytes())))])] })),
                ("reason", s("Preserve complete synthetic meaning and conditions; unconfirmed sources need separate admission")),
                ("required_actions", Value::Array(if pending { vec![s("Confirm independent repository evidence before ordinary admission")] } else { vec![] })),
                ("context_pages", Value::Array((page..page + pages.max(1)).map(int).collect())),
            ]));
            offset += size;
        }
        page += pages.max(1);
    }
    let unit_map = obj([
        ("schema", s("agent-memory/layout-unit-map-v1")),
        ("operation_id", s(operation)),
        ("manifest_sha256", s(hash_value(manifest))),
        ("units", Value::Array(units.clone())),
    ]);
    let physical = get(manifest, "physical_entries").as_array().unwrap();
    let metadata = obj([
        (
            "files",
            Value::Object(
                intended
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(path, _)| {
                        let row = physical
                            .iter()
                            .find(|row| text(get(row, "path")) == path)
                            .unwrap();
                        (
                            path.clone(),
                            obj([
                                ("uid", get(row, "uid").clone()),
                                ("gid", get(row, "gid").clone()),
                                (
                                    "mode",
                                    int(get(row, "mode").as_int().unwrap().as_i64().unwrap()
                                        & 0o777),
                                ),
                                ("xattrs", get(row, "xattrs").clone()),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
        ("directories", obj([])),
    ]);
    let review_id = agent_memory::host::uuid4()?;
    let pending = units
        .iter()
        .any(|u| text(get(u, "disposition")) == "pending-admission");
    let attestation = if pending {
        let runtime = agent_memory::artifact::inspect_runtime(
            &w.root.join("dev-source/skills/agent-memory"),
            &w.root.join("dev-source/target/release/am"),
        )?;
        obj([
            ("schema", s("agent-memory/layout-quiescence-attestation-v1")),
            ("attestation_id", s(agent_memory::host::uuid4()?)),
            ("operation_id", s(operation)),
            ("project_id", get(manifest, "project_id").clone()),
            ("repo_root", get(manifest, "repo_root").clone()),
            ("bank_path", get(manifest, "bank_path").clone()),
            ("checkout_key", get(scope, "checkout_key").clone()),
            (
                "state_root",
                s(w.env[OsStr::new("AGENT_MEMORY_STATE_HOME")].to_string_lossy()),
            ),
            ("manifest_sha256", s(hash_value(manifest))),
            (
                "runtime_artifact_digest",
                get(&runtime, "artifact_digest").clone(),
            ),
            ("operator_developer_id", get(scope, "developer_id").clone()),
            ("issued_at", get(manifest, "inspected_at").clone()),
            (
                "declared_principals",
                Value::Array(vec![get(scope, "developer_id").clone()]),
            ),
            (
                "declared_invocation_routes",
                Value::Array(vec![s(w.env
                    [OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")]
                .to_string_lossy())]),
            ),
            ("stopped_sessions", Value::Array(vec![])),
            (
                "statement",
                s(
                    "All authorized legacy invocation routes and in-flight sessions are disabled or quiesced until terminal recovery retirement, or use the independently verified capable runtime and bound private state.",
                ),
            ),
            ("review_id", s(&review_id)),
        ])
    } else {
        Value::Null
    };
    let review = obj([
        ("schema", s("agent-memory/layout-review-v1")),
        ("review_id", s(review_id)),
        ("operation_id", s(operation)),
        ("manifest_sha256", s(hash_value(manifest))),
        ("unit_map_sha256", s(hash_value(&unit_map))),
        ("intended_files_sha256", s(hash_value(intended))),
        ("metadata_plan_sha256", s(hash_value(&metadata))),
        (
            "authority_sha256",
            s(hash_value(get(
                get(manifest, "identity"),
                "project_evidence",
            ))),
        ),
        ("author", author.clone()),
        ("reviewer", reviewer),
        ("reviewed_at", get(manifest, "inspected_at").clone()),
        (
            "unit_decisions",
            Value::Array(
                units
                    .iter()
                    .map(|u| {
                        obj([
                            ("unit_id", get(u, "unit_id").clone()),
                            ("disposition", get(u, "disposition").clone()),
                            ("rationale", get(u, "reason").clone()),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "inherited_frontier_sha256",
            s(hash_value(&Value::Array(vec![]))),
        ),
        ("inherited_unit_decisions", Value::Array(vec![])),
        (
            "quiescence_attestation_sha256",
            if pending {
                s(hash_value(&attestation))
            } else {
                Value::Null
            },
        ),
        ("semantic_preservation", Value::Bool(true)),
        ("security", Value::Bool(true)),
        ("identity", Value::Bool(true)),
        ("decision", s("approve")),
        (
            "evidence",
            s("Controlled full original/context-page and complete native-result review"),
        ),
    ]);
    Ok(obj([
        ("schema", s("agent-memory/layout-prepare-v1")),
        ("operation_id", s(operation)),
        ("author", author),
        ("unit_map", unit_map),
        ("intended_files", intended.clone()),
        ("metadata_plan", metadata),
        ("review", review),
        ("quiescence_attestation", attestation),
        ("retained_source_ids", Value::Array(vec![])),
    ]))
}
fn repair(w: &World, repo: &Path, intended: &Value) -> TestResult<(Value, Value)> {
    let inspected = layout_inspect(w, repo)?;
    let operation = text(get(&inspected, "operation_id"));
    let input = save(
        w,
        &layout_request(w, &inspected, intended)?,
        "layout-prepare.json",
    )?;
    let proposal = recovery_output(
        w,
        repo,
        operation,
        &[os("prepare"), os("--input"), input.as_os_str()],
        0,
    )?;
    let proposal_path = save(w, &proposal, "layout-proposal.json")?;
    let applied = recovery_call(
        w,
        repo,
        &[os("apply"), os("--proposal"), proposal_path.as_os_str()],
        0,
    )?;
    Ok((inspected, applied))
}
struct PublicRepair {
    repo: PathBuf,
    bank: PathBuf,
    remote: PathBuf,
    policy: PathBuf,
    operation: String,
    intended: Value,
    inspected: Value,
    applied: Value,
    originals: BTreeMap<String, Vec<u8>>,
}
fn public_repair(w: &mut World) -> TestResult<PublicRepair> {
    public_repair_with_pending(w, None)
}
fn public_repair_with_pending(w: &mut World, handoff: Option<&str>) -> TestResult<PublicRepair> {
    let (repo, bank, peer, _) = fixture(w)?;
    let remote = PathBuf::from(git(w, &repo, &["remote", "get-url", "origin"])?.trim());
    let policy = github(w, &repo, &bank, Some(&peer), false)?;
    git(
        w,
        &repo,
        &[
            "commit",
            "--only",
            "-m",
            "Fixture existing authority identity",
            "--",
            "memory-bank/MEMORY.md",
        ],
    )?;
    git(
        w,
        &repo,
        &["commit", "-m", "Fixture unfinished feature ancestor"],
    )?;
    git(w, &repo, &["fetch", "origin", "main"])?;
    git(w, &repo, &["merge", "--no-edit", "FETCH_HEAD"])?;
    fs::write(
        repo.join("code.py"),
        "VALUE = 3  # staged feature sentinel\n",
    )?;
    git(w, &repo, &["add", "code.py"])?;
    fs::write(
        repo.join("code.py"),
        "VALUE = 4  # unstaged feature sentinel\n",
    )?;
    let original = fs::read_to_string(bank.join("activeContext.md"))?;
    let sentence = "Under the synthetic condition preserve the original source before acting.\n";
    let mut raw = original.clone() + &sentence.repeat((79054 - original.len()) / sentence.len());
    raw.push_str(&" ".repeat(79054 - raw.len()));
    fs::write(bank.join("activeContext.md"), &raw)?;
    fs::write(peer.join("memory-bank/activeContext.md"), &raw)?;
    git(w, &peer, &["add", "memory-bank/activeContext.md"])?;
    git(
        w,
        &peer,
        &["commit", "-m", "Synthetic invalid authority context"],
    )?;
    git(w, &peer, &["push", "origin", "HEAD:main"])?;
    let mut intended = bank_files(&bank)?;
    set(
        &mut intended,
        "activeContext.md",
        s(original.replacen("# Current focus\n", "# Active context\n", 1) + "\n" + sentence),
    );
    if let Some(handoff) = handoff {
        fs::write(bank.join("handoff-prompt.md"), handoff)?;
    }
    let before = state(w, &repo, &bank)?;
    let (inspected, applied) = repair(w, &repo, &intended)?;
    assert_eq!(
        get(&applied, "semantic_recovery_complete"),
        &Value::Bool(handoff.is_none())
    );
    let after = state(w, &repo, &bank)?;
    assert_eq!(
        (&after.1, &after.2, &after.3, &after.4),
        (&before.1, &before.2, &before.3, &before.4)
    );
    Ok(PublicRepair {
        repo,
        bank,
        remote,
        policy,
        operation: text(get(&inspected, "operation_id")).into(),
        intended,
        inspected,
        applied,
        originals: before.0,
    })
}
fn publication_review(observation: &Value, files: &Value) -> TestResult<Value> {
    let author = obj([
        (
            "developer_id",
            get(
                get(get(observation, "binding"), "scope_context"),
                "developer_id",
            )
            .clone(),
        ),
        ("harness", s("controlled-author")),
        ("session", s(agent_memory::host::uuid4()?)),
        (
            "evidence",
            s("Actual disposable fixture source review; not product acceptance"),
        ),
    ]);
    let mut reviewer = author.clone();
    set(&mut reviewer, "harness", s("controlled-reviewer"));
    set(&mut reviewer, "session", s(agent_memory::host::uuid4()?));
    let mut units = Vec::new();
    for (side, variant) in get(observation, "variants").as_object().unwrap() {
        for source in get(variant, "sources").as_array().unwrap() {
            let path = text(get(source, "path"));
            units.push(obj([("unit_id", s(agent_memory::host::uuid4()?)), ("side", s(side)), ("source_id", get(source,"source_id").clone()), ("start", int(0)), ("end", get(source,"byte_count").clone()), ("disposition", s("preserve")),
                ("targets", Value::Array(vec![obj([("path", s(path)), ("sha256", s(hash_bytes(text(get(files,path)).as_bytes())))])])), ("rationale", s("Complete controlled source meaning and conditions retained in the native result")), ("terminal_evidence", Value::Null)]));
        }
    }
    Ok(obj([
        ("schema", s("agent-memory/layout-publication-review-v1")),
        ("review_id", s(agent_memory::host::uuid4()?)),
        ("observation_sha256", s(hash_value(observation))),
        ("intended_files_sha256", s(hash_value(files))),
        ("author", author),
        ("reviewer", reviewer),
        ("reviewed_at", s(agent_memory::host::utc_now_isoformat()?)),
        ("source_units", Value::Array(units)),
        ("added_paths", Value::Array(vec![])),
        ("record_evidence", Value::Array(vec![])),
        (
            "identity_decisions",
            obj([
                (
                    "base",
                    s("Checked original same-project router and authority"),
                ),
                (
                    "local",
                    s("Checked original same-project router and authority"),
                ),
                (
                    "landed",
                    s("Checked original same-project router and authority"),
                ),
            ]),
        ),
        ("semantic_preservation", Value::Bool(true)),
        ("security", Value::Bool(true)),
        ("decision", s("approve")),
    ]))
}
fn publication_input(observation: &Value, files: &Value) -> TestResult<Value> {
    Ok(obj([
        ("schema", s("agent-memory/layout-publication-prepare-v1")),
        ("observation_id", get(observation, "observation_id").clone()),
        ("intended_files", files.clone()),
        ("review", publication_review(observation, files)?),
    ]))
}
fn layout_publication_observation(w: &World, f: &PublicRepair) -> TestResult<Value> {
    recovery_output(w, &f.repo, &f.operation, &[os("publish"), os("inspect")], 0)
}
fn layout_prepared(w: &World, f: &PublicRepair) -> TestResult<Value> {
    let observed = layout_publication_observation(w, f)?;
    let input = save(
        w,
        &publication_input(get(&observed, "observation"), &f.intended)?,
        "publication-layout-input.json",
    )?;
    let prepared = recovery_output(
        w,
        &f.repo,
        &f.operation,
        &[
            os("publish"),
            os("prepare"),
            os("--input"),
            input.as_os_str(),
        ],
        0,
    )?;
    assert_eq!(text(get(&prepared, "state")), "prepared");
    Ok(get(&prepared, "request").clone())
}
fn layout_land(w: &World, f: &PublicRepair, request: &Value, expected: i32) -> TestResult<Value> {
    let input = save(w, request, "publication-layout-exact.json")?;
    recovery_output(
        w,
        &f.repo,
        &f.operation,
        &[os("publish"), os("land"), os("--input"), input.as_os_str()],
        expected,
    )
}
fn layout_fault_journey(boundary: &str, mode: &str) -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let local = state(&w, &f.repo, &f.bank)?;
    let before = remote_head(&w, &f.repo)?;
    let request = layout_prepared(&w, &f)?;
    let input = save(&w, &request, "fault-exact.json")?;
    let output = w.root.join("fault-interrupted.json");
    let interrupted = w.run_with(
        [
            w.env[OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone(),
            "bank".into(),
            "recover-layout".into(),
            "publish".into(),
            "land".into(),
            "--repo".into(),
            f.repo.as_os_str().to_owned(),
            "--operation".into(),
            f.operation.clone().into(),
            "--input".into(),
            input.into_os_string(),
            "--output".into(),
            output.into_os_string(),
        ],
        RunOptions {
            check: false,
            timeout: Duration::from_secs(60),
            env: &[
                (
                    "AGENT_MEMORY_FAULT_STAGE".into(),
                    format!("layout-recovery-publication-{boundary}-push").into(),
                ),
                ("AGENT_MEMORY_FAULT_MODE".into(), mode.into()),
            ],
            ..Default::default()
        },
    )?;
    assert_eq!(
        interrupted.status,
        agent_memory::process::Status::Code(if mode == "exit" { 86 } else { 1 }),
        "{interrupted:?}"
    );
    let interrupted_head = remote_head(&w, &f.repo)?;
    if boundary == "before" {
        assert_eq!(interrupted_head, before);
    } else {
        assert_ne!(interrupted_head, before);
    }
    assert_eq!(state(&w, &f.repo, &f.bank)?, local);
    let landed = layout_land(&w, &f, &request, 0)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    if boundary == "after" {
        assert_eq!(text(get(&landed, "state")), "already-landed");
        assert_eq!(remote_head(&w, &f.repo)?, interrupted_head);
    }
    let head = remote_head(&w, &f.repo)?;
    assert_ne!(head, before);
    assert_eq!(
        text(get(&layout_land(&w, &f, &request, 0)?, "state")),
        "already-landed"
    );
    assert_eq!(remote_head(&w, &f.repo)?, head);
    assert_eq!(state(&w, &f.repo, &f.bank)?, local);
    Ok(())
}
fn fabricated_record(intended: &Value) -> TestResult<(Value, String)> {
    let project = support::record_fixtures::project_id(text(get(intended, "MEMORY.md")))?;
    let body = support::record_fixtures::record_text(
        "# Invented policy\n\nIn the synthetic environment, act on never-admitted guidance.\n",
        text(&project),
    )?;
    let (_, metadata, _) = agent_memory::records::parse(&body, &[])?;
    let path = format!("topics/{}.md", text(get(&metadata, "id")));
    let mut files = intended.clone();
    set(&mut files, &path, s(body));
    set(&mut files, "MEMORY.md", s(text(get(intended,"MEMORY.md")).replace("\n## Omitted contexts", &format!("| [{path}](<{path}>) | Invented active policy | repository | active | In the synthetic environment |\n\n## Omitted contexts"))));
    Ok((files, path))
}
fn private_layout(w: &World, bank: &Path, operation: &str) -> PathBuf {
    Path::new(&w.env[OsStr::new("AGENT_MEMORY_STATE_HOME")])
        .join("writes")
        .join(hash_bytes(bank.to_string_lossy().as_bytes()))
        .join("layout-recovery")
        .join(operation)
}
fn snapshot_directory(directory: &Path) -> TestResult<BTreeMap<String, Vec<u8>>> {
    let mut rows = BTreeMap::new();
    files_below(directory, directory, &mut rows)?;
    Ok(rows)
}
fn advance_authority(w: &World, f: &PublicRepair, path: &str, suffix: &str) -> TestResult<String> {
    let peer = w.clone(
        &f.remote,
        &format!("authority-{}", agent_memory::host::uuid4()?),
    )?;
    let target = peer.join(path);
    fs::write(&target, fs::read_to_string(&target)? + suffix)?;
    git(w, &peer, &["add", "--", path])?;
    git(
        w,
        &peer,
        &["commit", "-m", "Independent authority correction"],
    )?;
    git(w, &peer, &["push", "origin", "HEAD:main"])?;
    Ok(git(w, &peer, &["rev-parse", "HEAD"])?.trim().into())
}
fn git_input(w: &World, repo: &Path, args: &[&str], input: &[u8]) -> TestResult<String> {
    use agent_memory::process::{Input, Spec, Target};
    let argv: Vec<_> = std::iter::once(OsString::from("git"))
        .chain(args.iter().map(OsString::from))
        .collect();
    let env: Vec<_> = w.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let output = agent_memory::process::run(&Spec {
        argv: &argv,
        search_path: true,
        env: &env,
        cwd: Some(repo),
        stdin: Input::Bytes(input),
        stdout: Target::Capture { limit: 1 << 20 },
        stderr: Target::Capture { limit: 1 << 20 },
        deadline: agent_memory::clock::Deadline::after(Duration::from_secs(60))?,
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    })?;
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(0),
        "{output:?}"
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn pending_handoff(
    w: &World,
    repo: &Path,
    bank: &Path,
    intended: &Value,
) -> TestResult<(Value, Value)> {
    fs::write(
        bank.join("handoff-prompt.md"),
        "# Unconfirmed synthetic handoff\n\nUnknown provenance requires independent review.\n",
    )?;
    let (inspected, applied) = repair(w, repo, intended)?;
    assert_eq!(
        get(&applied, "semantic_recovery_complete"),
        &Value::Bool(false)
    );
    Ok((inspected, applied))
}
fn fresh_recall(
    w: &mut World,
    f: &PublicRepair,
    target: &str,
    expected: &Value,
    record: Option<&Value>,
) -> TestResult {
    let clone = w.clone(&f.remote, "fresh-consumer")?;
    git(
        w,
        &clone,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/fixture/memory.git",
        ],
    )?;
    let original = w.env.clone();
    let profile = w.root.join("empty-consumer-profile");
    fs::create_dir(&profile)?;
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o700))?;
    w.env.insert(
        "AGENT_MEMORY_CONFIG_HOME".into(),
        profile.join("config").into_os_string(),
    );
    w.env.insert(
        "AGENT_MEMORY_STATE_HOME".into(),
        profile.join("state").into_os_string(),
    );
    let result = (|| {
        bind(w, &clone)?;
        good(
            w,
            "am bank",
            &[os("validate"), os("--repo"), clone.as_os_str()],
        )?;
        let project = support::record_fixtures::project_id(text(get(&f.intended, "MEMORY.md")))?;
        let request = obj([
            ("project_id", project),
            ("query", s("synthetic condition")),
            (
                "ids",
                Value::Array(
                    record
                        .map(|r| vec![get(r, "id").clone()])
                        .unwrap_or_default(),
                ),
            ),
            (
                "paths",
                Value::Array(if record.is_some() {
                    vec![]
                } else {
                    vec![s(target)]
                }),
            ),
            ("context", obj([])),
            (
                "kinds",
                Value::Array(
                    record
                        .map(|r| vec![get(r, "kind").clone()])
                        .unwrap_or_default(),
                ),
            ),
            ("minimum_trust", s("reviewed")),
            ("limit", int(8)),
            ("include_context", Value::Bool(record.is_none())),
        ]);
        let input = save(w, &request, "fresh-recall.json")?;
        let recalled = good(
            w,
            "am search",
            &[
                os("--repo"),
                clone.as_os_str(),
                os("--request"),
                input.as_os_str(),
            ],
        )?;
        let results = get(&recalled, "results").as_array().unwrap();
        assert_eq!(results.len(), 1, "{recalled:?}");
        assert_eq!(text(get(&results[0], "path")), target);
        assert_eq!(get(&results[0], "body"), expected);
        if let Some(metadata) = record {
            for field in ["id", "kind", "evidence", "conditions", "origin", "source"] {
                assert_eq!(get(get(&results[0], "record"), field), get(metadata, field));
            }
            assert_eq!(
                get(&recalled, "source_assertions_verified"),
                &Value::Bool(false)
            );
        } else {
            assert_eq!(
                text(get(&results[0], "selection_class")),
                "orientation-only"
            );
            assert_eq!(
                get(get(&results[0], "context_assessment"), "managed_checkpoint"),
                &Value::Bool(false)
            );
        }
        assert_eq!(
            get(&recalled, "code_working_bytes_verified"),
            &Value::Bool(false)
        );
        // Baseline test_layout_recovery_resolved_publication.py:185-186:
        // ordinary recall may keep private writes state, never layout recovery.
        let mut pending = vec![profile.join("state/writes")];
        while let Some(directory) = pending.pop() {
            if !directory.is_dir() {
                continue;
            }
            for entry in fs::read_dir(&directory)? {
                let path = entry?.path();
                assert_ne!(path.file_name(), Some(OsStr::new("layout-recovery")));
                pending.push(path);
            }
        }
        Ok(())
    })();
    w.env = original;
    result
}
const SYNTHETIC_POLICIES: [&str; 8] = [
    "In the synthetic offline-queue environment, attempt delivery at most three times, retain every queued item after failure, and resume delivery only after the operator changes the environment to online.",
    "For synthetic read-only database snapshots, pin the snapshot revision before querying, reject any write request, and discard the query result if the revision changes before readback.",
    "For the synthetic local cache, a missing checksum is a cache miss rather than an empty value; preserve the backing file and rebuild only after reading the authoritative fixture input.",
    "For synthetic batch imports, process records in original sequence, keep the first invalid record pending with its original ordinal, and never report the unread remainder as imported.",
    "For synthetic dry-run deployments, generate the exact target plan without activating it; activation requires a separate operator decision tied to that same plan and target revision.",
    "For synthetic expired credentials, stop the requested network operation without retrying another account; retain the pending request and ask the detecting developer to renew the intended account.",
    "For synthetic concurrent edits, preserve both source versions and their conditions; a later timestamp alone does not choose the correct meaning or authorize overwriting either version.",
    "For synthetic terminal deletion, retain only the content-free stable-identity marker in current memory and reject any later stale snapshot that tries to resurrect the removed body.",
];
fn admission_proposal(
    w: &World,
    repo: &Path,
    bank: &Path,
    body: &str,
    evidence_path: &str,
    inspected: Option<&Value>,
    stdout_order: bool,
) -> TestResult<(Value, String)> {
    // Baseline fixtures save canonical (sorted) JSON; `stdout_order` instead
    // keeps request/capture files in their authored and printed key order.
    let store = |value: &Value, name: &str| -> TestResult<PathBuf> {
        if !stdout_order {
            return save(w, value, name);
        }
        let path = w.root.join(name);
        fs::write(&path, json::encode(value, json::INDENT2_UTF8)?)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(path)
    };
    // Baseline layout_recovery_fixtures.admission_proposal:289-290 takes the
    // project and actor from the inspected manifest while recovery is pending.
    let (project, actor) = match inspected {
        Some(inspected) => {
            let manifest = get(inspected, "manifest");
            (
                get(manifest, "project_id").clone(),
                get(
                    get(get(manifest, "identity"), "scope_context"),
                    "developer_id",
                )
                .clone(),
            )
        }
        None => {
            let scope = good(
                w,
                "am identity",
                &[os("status"), os("--repo"), repo.as_os_str()],
            )?;
            (
                get(&scope, "project_id").clone(),
                get(&scope, "developer_id").clone(),
            )
        }
    };
    let mut candidate = decode(include_bytes!(
        "../skills/agent-memory/templates/record-proposal.example.json"
    ))?;
    set(&mut candidate, "body", s(body));
    let meta = candidate.get_mut("metadata").unwrap();
    set(meta, "project-id", project.clone());
    set(meta, "id", s(agent_memory::host::uuid4()?));
    set(meta, "evidence", Value::Array(vec![s(evidence_path)]));
    set(meta, "scope", s("repository"));
    set(meta, "trust", s("reviewed"));
    set(meta, "visibility", s("visible"));
    set(
        meta,
        "admission-reason",
        s("Controlled structural fixture, not release acceptance evidence"),
    );
    set(meta, "conditions", obj([]));
    set(
        meta,
        "source",
        obj([
            ("kind", s("verified-repository")),
            ("actor", s("Explicit controlled fixture source")),
        ]),
    );
    let origin = meta.get_mut("origin").unwrap();
    set(origin, "developer-id", actor.clone());
    set(origin, "code-state", s("not-code"));
    let request = obj([
        ("project_id", project.clone()),
        ("candidate", candidate),
        ("source_class", s("verified-repository")),
        (
            "repository_evidence",
            obj([
                ("path", s(evidence_path)),
                (
                    "sha256",
                    s(hash_bytes(&fs::read(repo.join(evidence_path))?)),
                ),
            ]),
        ),
        (
            "assessment",
            obj([
                ("category", s("durable-discovery")),
                (
                    "utility",
                    s(
                        "Prevent repeated source and condition mistakes in this synthetic environment",
                    ),
                ),
                ("durable", Value::Bool(true)),
                ("evidence_confidence", s("confirmed")),
                (
                    "evidence_reason",
                    s("Checked independently readable repository evidence"),
                ),
                ("confirmed", Value::Bool(true)),
                ("source_redundant", Value::Bool(false)),
                ("work_state_copy", Value::Bool(false)),
                (
                    "review",
                    s(
                        "Controlled review of actual independent repository evidence and complete source conditions",
                    ),
                ),
            ]),
        ),
    ]);
    let input = store(&request, "admission-capture-input.json")?;
    let captured = good(
        w,
        "am capture",
        &[
            os("--repo"),
            repo.as_os_str(),
            os("--request"),
            input.as_os_str(),
        ],
    )?;
    let capture = captured.get("capture").unwrap_or(&captured);
    assert_eq!(text(get(capture, "decision")), "eligible");
    let candidate = get(get(capture, "request"), "candidate");
    let compared = obj([
        ("project_id", project.clone()),
        ("candidate", candidate.clone()),
        (
            "searches",
            Value::Array(vec![
                s("offline delivery queue"),
                s("retain items after failure"),
            ]),
        ),
        ("read_paths", Value::Array(vec![])),
    ]);
    let compared_path = store(&compared, "admission-comparison-input.json")?;
    let view_path = w.root.join("admission-comparison-private.json");
    good(
        w,
        "am compare",
        &[
            os("inspect"),
            os("--repo"),
            repo.as_os_str(),
            os("--request"),
            compared_path.as_os_str(),
            os("--private-output"),
            view_path.as_os_str(),
        ],
    )?;
    let view = read_json(&view_path)?;
    let operation = agent_memory::host::uuid4()?;
    let stamp = inspected
        .map(|v| text(get(get(v, "manifest"), "inspected_at")).to_owned())
        .unwrap_or(agent_memory::host::utc_now_isoformat()?);
    // Baseline layout_recovery_fixtures.py:307-314 builds the intended map from
    // the canonical capture, the form the private preview artifact stores.
    let canonical = agent_memory::developer_identity::load(
        &agent_memory::developer_identity::encoded(capture)?,
    )?;
    let intended = agent_memory::admission::desired(
        &bank_files(bank)?,
        &obj([("capture", canonical), ("recorded_at", s(&stamp))]),
        &operation,
    )?;
    let target = format!("topics/{}.md", text(get(get(candidate, "metadata"), "id")));
    let pending = get(&view, "pending_matches")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .map(|row| {
            let claim = obj([
                ("unit_id", get(row, "unit_id").clone()),
                ("coverage", s("full")),
                ("covered_source_ranges", get(row, "source_ranges").clone()),
                (
                    "target_record_refs",
                    Value::Array(vec![obj([
                        ("id", get(get(candidate, "metadata"), "id").clone()),
                        ("version", int(1)),
                        ("path", s(&target)),
                        (
                            "resulting_sha256",
                            s(hash_bytes(text(get(&intended, &target)).as_bytes())),
                        ),
                    ])]),
                ),
            ]);
            obj([
                ("unit_id", get(row, "unit_id").clone()),
                ("classification", s("distinct-knowledge")),
                (
                    "rationale",
                    s("Controlled full source meaning and conditions reviewed independently"),
                ),
                ("coverage_claim", claim),
            ])
        })
        .collect::<Vec<_>>();
    let judgment = obj([
        ("observation", get(&view, "observation").clone()),
        (
            "decisions",
            Value::Array(
                get(&view, "matches")
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| {
                        obj([
                            ("path", get(row, "path").clone()),
                            ("classification", s("distinct-knowledge")),
                            (
                                "rationale",
                                s("Different independently inspected prior meaning"),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("pending_decisions", Value::Array(pending.clone())),
        (
            "rationale",
            s("Complete controlled comparison, not product acceptance"),
        ),
    ]);
    let judgment_path = save(w, &judgment, "admission-review-input.json")?;
    let proof_path = w.root.join("admission-proof-private.json");
    let mut args = vec![
        os("review"),
        os("--repo"),
        repo.as_os_str(),
        os("--request"),
        compared_path.as_os_str(),
        os("--judgment"),
        judgment_path.as_os_str(),
        os("--private-output"),
        proof_path.as_os_str(),
    ];
    let coverage_path;
    if let Some(inspected) = inspected {
        let directory = private_layout(w, bank, text(get(inspected, "operation_id")));
        let head = read_json(&directory.join("ledger-head.json"))?;
        let generation = get(&head, "generation").as_int().unwrap().as_i64().unwrap();
        let ledger = read_json(&directory.join(format!("ledger/{generation:08}.json")))?;
        let proof = obj([
            ("schema", s("agent-memory/comparison-proof-v2")),
            ("judgment", judgment.clone()),
            ("disposition", s("create-distinct")),
            ("targets", Value::Array(vec![])),
        ]);
        let review = obj([
            ("schema", s("agent-memory/layout-coverage-review-v1")),
            ("review_id", s(agent_memory::host::uuid4()?)),
            ("operation_id", get(inspected, "operation_id").clone()),
            ("reviewer_developer_id", actor.clone()),
            ("reviewer_harness", s("controlled-coverage-reviewer")),
            ("reviewer_session", s(format!("coverage-{operation}"))),
            ("review_time", s(&stamp)),
            ("manifest_sha256", get(&ledger, "manifest_sha256").clone()),
            ("unit_map_sha256", get(&ledger, "unit_map_sha256").clone()),
            ("ledger_head_sha256", get(&head, "sha256").clone()),
            ("comparison_proof_input_sha256", s(hash_value(&proof))),
            ("intended_output_sha256", s(hash_value(&intended))),
            (
                "claims",
                Value::Array(
                    pending
                        .iter()
                        .map(|row| get(row, "coverage_claim").clone())
                        .collect(),
                ),
            ),
            (
                "rationale",
                s("Controlled distinct-session full original-range and intended-result review"),
            ),
            ("decision", s("approve")),
        ]);
        coverage_path = save(w, &review, "admission-coverage-review.json")?;
        args.extend([os("--coverage-review"), coverage_path.as_os_str()]);
    }
    good(w, "am compare", &args)?;
    let capture_path = store(capture, "admission-capture-private.json")?;
    let preview_path = w.root.join("admission-preview-private.json");
    good(
        w,
        "am admission",
        &[
            os("preview"),
            os("--repo"),
            repo.as_os_str(),
            os("--capture"),
            capture_path.as_os_str(),
            os("--request"),
            compared_path.as_os_str(),
            os("--proof"),
            proof_path.as_os_str(),
            os("--operation-id"),
            os(&operation),
            os("--recorded-at"),
            os(&stamp),
            os("--private-output"),
            preview_path.as_os_str(),
        ],
    )?;
    Ok((get(&read_json(&preview_path)?, "proposal").clone(), target))
}
fn apply_admission(w: &World, proposal: &Value) -> TestResult<Value> {
    let path = save(w, proposal, "real-admission-proposal.json")?;
    let result = good(
        w,
        "am admission",
        &[os("apply"), os("--proposal"), path.as_os_str()],
    )?;
    assert_eq!(get(&result, "admission_verified"), &Value::Bool(true));
    Ok(result)
}
fn publication_with_admission(
    w: &World,
    f: &PublicRepair,
    proposal: &Value,
    receipt: &Value,
) -> TestResult<Value> {
    let observed = layout_publication_observation(w, f)?;
    let files = get(proposal, "files");
    let mut input = publication_input(get(&observed, "observation"), files)?;
    set(
        input.get_mut("review").unwrap(),
        "record_evidence",
        Value::Array(vec![obj([
            ("operation_id", get(proposal, "operation_id").clone()),
            ("receipt_sha256", s(hash_value(receipt))),
            ("proposal", proposal.clone()),
        ])]),
    );
    Ok(input)
}

// Rust-only regression: publication measures the receipt-matched runtime
// binary, not the invoked path. macOS `current_exe` reports the installed
// `~/.local/bin/am` symlink unresolved, which the no-follow measurement
// refused as "Runtime binary must not be a symlink".
#[test]
fn publication_inspect_through_the_symlinked_command_measures_the_installed_binary() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let key = OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND");
    let command = PathBuf::from(&w.env[key]);
    assert!(fs::symlink_metadata(&command)?.file_type().is_symlink());
    let linked = layout_publication_observation(&w, &f)?;
    let digest = text(get(get(&linked, "observation"), "runtime_artifact_digest")).to_owned();
    assert_eq!(digest.len(), 64);
    w.env
        .insert(key.into(), fs::canonicalize(&command)?.into_os_string());
    let direct = layout_publication_observation(&w, &f)?;
    w.env.insert(key.into(), command.into_os_string());
    assert_eq!(
        text(get(get(&direct, "observation"), "runtime_artifact_digest")),
        digest
    );
    Ok(())
}

// obligation: tests/contracts/test_layout_publication_faults.py::LayoutPublicationFaults.test_after_push_error
#[test]
fn layout_publication_after_push_error() -> TestResult {
    layout_fault_journey("after", "error")
}
// obligation: tests/contracts/test_layout_publication_faults.py::LayoutPublicationFaults.test_after_push_exit
#[test]
fn layout_publication_after_push_exit() -> TestResult {
    layout_fault_journey("after", "exit")
}
// obligation: tests/contracts/test_layout_publication_faults.py::LayoutPublicationFaults.test_before_push_error
#[test]
fn layout_publication_before_push_error() -> TestResult {
    layout_fault_journey("before", "error")
}
// obligation: tests/contracts/test_layout_publication_faults.py::LayoutPublicationFaults.test_before_push_exit
#[test]
fn layout_publication_before_push_exit() -> TestResult {
    layout_fault_journey("before", "exit")
}
// obligation: tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_publication_refuses_unicode_and_case_aliased_authority_sources
#[test]
fn publication_refuses_unicode_and_case_aliased_authority_sources() -> TestResult {
    for names in [["alias.md", "ALIAS.md"], ["caf\u{e9}.md", "cafe\u{301}.md"]] {
        let mut w = World::new()?;
        let f = public_repair(&mut w)?;
        let peer = w.clone(&f.remote, "alias-peer")?;
        let blob = git_input(
            &w,
            &peer,
            &["hash-object", "-w", "--stdin"],
            b"Distinct raw authority source.\n",
        )?;
        let mut bank_entries = git(&w, &peer, &["ls-tree", "-z", "HEAD:memory-bank"])?;
        for name in names {
            bank_entries.push_str(&format!("100644 blob {blob}\t{name}\0"));
        }
        let bank_tree = git_input(&w, &peer, &["mktree", "-z"], bank_entries.as_bytes())?;
        let raw = git(&w, &peer, &["ls-tree", "-z", &bank_tree])?;
        for name in names {
            assert!(raw.contains(&format!("100644 blob {blob}\t{name}\0")));
        }
        let root = git(&w, &peer, &["ls-tree", "-z", "HEAD"])?;
        let mut entries: String = root
            .split_terminator('\0')
            .filter(|entry| entry.split_once('\t').unwrap().1 != "memory-bank")
            .map(|entry| format!("{entry}\0"))
            .collect();
        entries.push_str(&format!("040000 tree {bank_tree}\tmemory-bank\0"));
        let tree = git_input(&w, &peer, &["mktree", "-z"], entries.as_bytes())?;
        let parent = git(&w, &peer, &["rev-parse", "HEAD"])?;
        let commit = git(
            &w,
            &peer,
            &[
                "commit-tree",
                &tree,
                "-p",
                parent.trim(),
                "-m",
                "Aliased authority sources",
            ],
        )?;
        git(
            &w,
            &peer,
            &[
                "push",
                "origin",
                &format!("{}:refs/heads/main", commit.trim()),
            ],
        )?;
        let before = state(&w, &f.repo, &f.bank)?;
        let refused = recovery_output(
            &w,
            &f.repo,
            &f.operation,
            &[os("publish"), os("inspect")],
            1,
        )?;
        assert_eq!(
            get(&refused, "canonical_writes_performed"),
            &Value::Bool(false)
        );
        assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    }
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_complete_candidate_delta_excludes_original_feature_index
#[test]
fn complete_candidate_delta_excludes_original_feature_index() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let before = state(&w, &f.repo, &f.bank)?;
    let request = layout_prepared(&w, &f)?;
    let base = text(get(&request, "base"));
    let candidate = text(get(&request, "candidate"));
    assert_eq!(
        rev_parents(&w, &f.repo, candidate)?.get(1..).unwrap(),
        &[base.to_owned()]
    );
    let changed = git(
        &w,
        &f.repo,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            base,
            candidate,
        ],
    )?;
    assert!(changed.lines().all(|path| path.starts_with("memory-bank/")));
    assert!(changed.contains("memory-bank/activeContext.md"));
    assert_eq!(
        git(&w, &f.repo, &["show", &format!("{candidate}:code.py")])?,
        git(&w, &f.repo, &["show", &format!("{base}:code.py")])?
    );
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    let index = w.root.join("code-control-index");
    let previous = w
        .env
        .insert("GIT_INDEX_FILE".into(), index.into_os_string());
    git(&w, &f.repo, &["read-tree", candidate])?;
    let blob = git_input(
        &w,
        &f.repo,
        &["hash-object", "-w", "--stdin"],
        b"VALUE = 999\n",
    )?;
    git(
        &w,
        &f.repo,
        &[
            "update-index",
            "--cacheinfo",
            &format!("100644,{blob},code.py"),
        ],
    )?;
    let tree = git(&w, &f.repo, &["write-tree"])?;
    if let Some(previous) = previous {
        w.env.insert("GIT_INDEX_FILE".into(), previous);
    } else {
        w.env.remove(OsStr::new("GIT_INDEX_FILE"));
    }
    let bad = git(
        &w,
        &f.repo,
        &[
            "commit-tree",
            tree.trim(),
            "-p",
            candidate,
            "-m",
            "Unreviewed code delta",
        ],
    )?;
    let mut tampered = request.clone();
    set(&mut tampered, "candidate", s(bad.trim()));
    layout_land(&w, &f, &tampered, 1)?;
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_complete_raw_local_side_includes_opaque_nonmarkdown_occupants
#[test]
fn complete_raw_local_side_includes_opaque_nonmarkdown_occupants() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let body = b"Non-guidance opaque fixture bytes: \xff\x00\n";
    fs::write(f.bank.join("opaque-source.bin"), body)?;
    let before = state(&w, &f.repo, &f.bank)?;
    let observed = layout_publication_observation(&w, &f)?;
    let sources = get(
        get(get(get(&observed, "observation"), "variants"), "local"),
        "sources",
    )
    .as_array()
    .unwrap();
    let source = sources
        .iter()
        .find(|row| text(get(row, "path")) == "opaque-source.bin")
        .unwrap();
    let directory = Path::new(text(get(&observed, "observation_path")))
        .parent()
        .unwrap();
    assert_eq!(fs::read(directory.join(text(get(source, "blob"))))?, body);
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_current_committed_admission_addition_and_receipt_source_fence
#[test]
fn current_committed_admission_addition_and_receipt_source_fence() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    fs::write(
        f.repo.join("evidence.md"),
        "# Confirmed synthetic repository result\n\nUse at most three attempts for this fixture.\n",
    )?;
    let (proposal, _) = admission_proposal(
        &w,
        &f.repo,
        &f.bank,
        "# Retry policy\n\nUse at most three attempts for this fixture.\n",
        "evidence.md",
        None,
        false,
    )?;
    for (path, body) in get(&proposal, "files").as_object().unwrap() {
        fs::create_dir_all(f.bank.join(path).parent().unwrap())?;
        fs::write(f.bank.join(path), text(body))?;
    }
    let mut input = publication_with_admission(&w, &f, &proposal, &Value::Null)?;
    let Value::Array(rows) = input
        .get_mut("review")
        .unwrap()
        .get_mut("record_evidence")
        .unwrap()
    else {
        panic!("evidence")
    };
    set(&mut rows[0], "receipt_sha256", s("0".repeat(64)));
    let input = save(&w, &input, "uncommitted-admission-publication.json")?;
    let before = state(&w, &f.repo, &f.bank)?;
    let head = remote_head(&w, &f.repo)?;
    recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[
            os("publish"),
            os("prepare"),
            os("--input"),
            input.as_os_str(),
        ],
        1,
    )?;
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    for (path, _) in get(&proposal, "files").as_object().unwrap() {
        if get(&proposal, "observed_hashes").get(path).is_none() {
            fs::remove_file(f.bank.join(path))?;
        }
    }
    for (path, body) in f.intended.as_object().unwrap() {
        fs::write(f.bank.join(path), text(body))?;
    }
    let admitted = apply_admission(&w, &proposal)?;
    let receipt_path = Path::new(text(get(&admitted, "receipt_path")));
    let receipt_bytes = fs::read(receipt_path)?;
    let receipt = decode(&receipt_bytes)?;
    let input = save(
        &w,
        &publication_with_admission(&w, &f, &proposal, &receipt)?,
        "committed-admission-publication.json",
    )?;
    let prepared = recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[
            os("publish"),
            os("prepare"),
            os("--input"),
            input.as_os_str(),
        ],
        0,
    )?;
    let request = get(&prepared, "request");
    let before = state(&w, &f.repo, &f.bank)?;
    fs::remove_file(receipt_path)?;
    layout_land(&w, &f, request, 1)?;
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    assert_eq!(remote_head(&w, &f.repo)?, head);
    fs::write(receipt_path, receipt_bytes)?;
    fs::set_permissions(receipt_path, fs::Permissions::from_mode(0o600))?;
    assert_eq!(
        get(&layout_land(&w, &f, request, 0)?, "publication_verified"),
        &Value::Bool(true)
    );
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_new_unsupported_authority_source_cannot_be_admitted_by_publication_mapping
#[test]
fn new_unsupported_authority_source_cannot_be_admitted_by_publication_mapping() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let peer = w.clone(&f.remote, "new-source-authority")?;
    fs::write(
        peer.join("memory-bank/new-handoff.md"),
        "# Unconfirmed source\n\nThis new source requires separate detecting-developer admission.\n",
    )?;
    git(&w, &peer, &["add", "memory-bank/new-handoff.md"])?;
    git(
        &w,
        &peer,
        &["commit", "-m", "Controlled newly discovered source"],
    )?;
    git(&w, &peer, &["push", "origin", "HEAD:main"])?;
    let observed = layout_publication_observation(&w, &f)?;
    let observation = get(&observed, "observation");
    let mut filtered = observation.clone();
    let sources = filtered
        .get_mut("variants")
        .unwrap()
        .get_mut("landed")
        .unwrap()
        .get_mut("sources")
        .unwrap();
    let Value::Array(rows) = sources else {
        panic!("sources")
    };
    let position = rows
        .iter()
        .position(|row| text(get(row, "path")) == "new-handoff.md")
        .unwrap();
    let new = rows.remove(position);
    let mut review = publication_review(&filtered, &f.intended)?;
    set(
        &mut review,
        "observation_sha256",
        s(hash_value(observation)),
    );
    let Value::Array(units) = review.get_mut("source_units").unwrap() else {
        panic!("units")
    };
    units.push(obj([
        ("unit_id", s(agent_memory::host::uuid4()?)),
        ("side", s("landed")),
        ("source_id", get(&new, "source_id").clone()),
        ("start", int(0)),
        ("end", get(&new, "byte_count").clone()),
        ("disposition", s("preserve")),
        (
            "targets",
            Value::Array(vec![obj([
                ("path", s("activeContext.md")),
                (
                    "sha256",
                    s(hash_bytes(
                        text(get(&f.intended, "activeContext.md")).as_bytes(),
                    )),
                ),
            ])]),
        ),
        (
            "rationale",
            s("Controlled invalid attempt to relabel an unconfirmed source as admitted guidance"),
        ),
        ("terminal_evidence", Value::Null),
    ]));
    let mut request = publication_input(observation, &f.intended)?;
    set(&mut request, "review", review);
    let input = save(&w, &request, "unsupported-source.json")?;
    let before = state(&w, &f.repo, &f.bank)?;
    let head = remote_head(&w, &f.repo)?;
    recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[
            os("publish"),
            os("prepare"),
            os("--input"),
            input.as_os_str(),
        ],
        1,
    )?;
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    assert_eq!(remote_head(&w, &f.repo)?, head);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_offline_and_changed_sources_preserve_local_result_and_evidence
#[test]
fn offline_and_changed_sources_preserve_local_result_and_evidence() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let before = state(&w, &f.repo, &f.bank)?;
    let away = f.remote.with_file_name("offline.git");
    fs::rename(&f.remote, &away)?;
    recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[os("publish"), os("inspect")],
        1,
    )?;
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    fs::rename(&away, &f.remote)?;
    let observed = layout_publication_observation(&w, &f)?;
    let input = save(
        &w,
        &publication_input(get(&observed, "observation"), &f.intended)?,
        "changed-source.json",
    )?;
    fs::write(
        f.bank.join("projectbrief.md"),
        format!(
            "{}\nA later independent local change.\n",
            text(get(&f.intended, "projectbrief.md"))
        ),
    )?;
    let changed = state(&w, &f.repo, &f.bank)?;
    recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[
            os("publish"),
            os("prepare"),
            os("--input"),
            input.as_os_str(),
        ],
        1,
    )?;
    assert_eq!(state(&w, &f.repo, &f.bank)?, changed);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_public_raw_authority_publication_and_empty_profile_second_clone
#[test]
fn public_raw_authority_publication_and_empty_profile_second_clone() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let before = state(&w, &f.repo, &f.bank)?;
    let observed = layout_publication_observation(&w, &f)?;
    let mut input = publication_input(get(&observed, "observation"), &f.intended)?;
    let Value::Array(units) = input
        .get_mut("review")
        .unwrap()
        .get_mut("source_units")
        .unwrap()
    else {
        panic!("units")
    };
    let omitted = units.pop().unwrap();
    let rejected = save(&w, &input, "omitted-publication-evidence.json")?;
    recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[
            os("publish"),
            os("prepare"),
            os("--input"),
            rejected.as_os_str(),
        ],
        1,
    )?;
    let Value::Array(units) = input
        .get_mut("review")
        .unwrap()
        .get_mut("source_units")
        .unwrap()
    else {
        panic!("units")
    };
    units.push(omitted);
    let input = save(&w, &input, "complete-publication-evidence.json")?;
    let prepared = recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[
            os("publish"),
            os("prepare"),
            os("--input"),
            input.as_os_str(),
        ],
        0,
    )?;
    let request = get(&prepared, "request");
    assert_eq!(
        get(&layout_land(&w, &f, request, 0)?, "publication_verified"),
        &Value::Bool(true)
    );
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    assert_eq!(
        text(get(&layout_land(&w, &f, request, 0)?, "state")),
        "already-landed"
    );
    fresh_recall(
        &mut w,
        &f,
        "activeContext.md",
        get(&f.intended, "activeContext.md"),
        None,
    )
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_publication_refuses_pending_frontier_before_network
#[test]
fn publication_refuses_pending_frontier_before_network() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _, _) = fixture(&mut w)?;
    let intended = bank_files(&bank)?;
    let (inspected, _) = pending_handoff(&w, &repo, &bank, &intended)?;
    let before = state(&w, &repo, &bank)?;
    let refused = recovery_output(
        &w,
        &repo,
        text(get(&inspected, "operation_id")),
        &[os("publish"), os("inspect")],
        1,
    )?;
    assert_eq!(get(&refused, "publication_verified"), &Value::Bool(false));
    assert_eq!(state(&w, &repo, &bank)?, before);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_publication_review_cannot_admit_fabricated_outputs_or_local_sources
#[test]
fn publication_review_cannot_admit_fabricated_outputs_or_local_sources() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let remote = remote_head(&w, &f.repo)?;
    let (files, path) = fabricated_record(&f.intended)?;
    for local_source in [false, true] {
        if local_source {
            fs::create_dir_all(f.bank.join(&path).parent().unwrap())?;
            fs::write(f.bank.join(&path), text(get(&files, &path)))?;
            fs::write(f.bank.join("MEMORY.md"), text(get(&files, "MEMORY.md")))?;
        }
        let before = state(&w, &f.repo, &f.bank)?;
        let observed = layout_publication_observation(&w, &f)?;
        let mut input = publication_input(get(&observed, "observation"), &files)?;
        set(
            input.get_mut("review").unwrap(),
            "added_paths",
            Value::Array(if local_source { vec![] } else { vec![s(&path)] }),
        );
        let input = save(&w, &input, "fabrication.json")?;
        let refused = recovery_output(
            &w,
            &f.repo,
            &f.operation,
            &[
                os("publish"),
                os("prepare"),
                os("--input"),
                input.as_os_str(),
            ],
            1,
        )?;
        assert_eq!(get(&refused, "publication_verified"), &Value::Bool(false));
        assert_eq!(state(&w, &f.repo, &f.bank)?, before);
        assert_eq!(remote_head(&w, &f.repo)?, remote);
    }
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_raw_source_bound_alias_and_committed_hold_refuse
#[test]
fn raw_source_bound_alias_and_committed_hold_refuse() -> TestResult {
    for variant in ["overflow", "alias", "hold", "symlink"] {
        let mut w = World::new()?;
        let (repo, bank, _, _) = fixture(&mut w)?;
        match variant {
            "overflow" => fs::write(bank.join("activeContext.md"), vec![b'x'; 1048577])?,
            "alias" => {
                let blob = git_input(
                    &w,
                    &repo,
                    &["hash-object", "-w", "--stdin"],
                    b"Portable alias must not be hidden.\n",
                )?;
                git(
                    &w,
                    &repo,
                    &[
                        "update-index",
                        "--add",
                        "--cacheinfo",
                        &format!("100644,{blob},memory-bank/ProjectBrief.md"),
                    ],
                )?;
            }
            "hold" => fs::write(bank.join(".agent-memory-transaction.md"), "{}\n")?,
            _ => std::os::unix::fs::symlink("projectbrief.md", bank.join("redirect.md"))?,
        }
        if variant != "alias" {
            git(&w, &repo, &["add", "-f", "memory-bank"])?;
        }
        git(&w, &repo, &["commit", "-m", "Unsafe raw source control"])?;
        let before = state(&w, &repo, &bank)?;
        let outcome = observe_tree_case(&w, &repo)?;
        assert!(get(&outcome, "error").as_str().is_some(), "{outcome:?}");
        assert_eq!(state(&w, &repo, &bank)?, before);
    }
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication.py::LayoutPublicationContract.test_reported_size_sources_are_raw_not_a_normal_snapshot
#[test]
fn reported_size_sources_are_raw_not_a_normal_snapshot() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    let mut raw = "# Qualified context\n\n".to_owned()
        + &"Context condition and provenance remain review inputs.\n".repeat(2000);
    raw.truncate(79054);
    raw.push_str(&" ".repeat(79054 - raw.len()));
    let handoff =
        "# Pending handoff\n\n".to_owned() + &"x".repeat(28478 - "# Pending handoff\n\n".len());
    fs::write(peer.join("memory-bank/activeContext.md"), &raw)?;
    fs::write(peer.join("memory-bank/handoff-prompt.md"), &handoff)?;
    git(&w, &peer, &["add", "memory-bank"])?;
    git(
        &w,
        &peer,
        &["commit", "-m", "Synthetic invalid authority layout"],
    )?;
    let revision = git(&w, &peer, &["rev-parse", "HEAD"])?;
    git(
        &w,
        &repo,
        &["fetch", peer.to_str().unwrap(), revision.trim()],
    )?;
    let before = state(&w, &repo, &bank)?;
    let outcome = run_case(
        &w,
        &repo,
        &[("tree.json", &obj([("revision", s(revision.trim()))]))],
    )?;
    let manifest = get(&outcome, "manifest");
    assert_eq!(get(manifest, "observation_complete"), &Value::Bool(true));
    for (path, body) in [("activeContext.md", raw), ("handoff-prompt.md", handoff)] {
        let sources = get(manifest, "sources").as_array().unwrap();
        let position = sources
            .iter()
            .position(|row| text(get(row, "path")) == path)
            .unwrap();
        assert_eq!(
            get(&sources[position], "byte_count"),
            &int(body.len() as i64)
        );
        assert_eq!(
            text(&get(&outcome, "retained").as_array().unwrap()[position]),
            body
        );
    }
    assert_eq!(get(&outcome, "normal_refused"), &Value::Bool(true));
    assert_eq!(state(&w, &repo, &bank)?, before);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication_closure.py::RecoveryPublicationClosure.test_ordinary_prepared_land_refuses_pending_recovery_frontier
#[test]
fn ordinary_prepared_land_refuses_pending_recovery_frontier() -> TestResult {
    for pending in [false, true] {
        let mut w = World::new()?;
        let (repo, bank, peer, _) = fixture(&mut w)?;
        github(&mut w, &repo, &bank, Some(&peer), false)?;
        let body = fs::read_to_string(bank.join("activeContext.md"))?.replacen(
            "# Current focus\n",
            "# Active context\n",
            1,
        ) + "\nUnder the controlled fixture condition, preserve the source before acting.\n";
        fs::write(bank.join("activeContext.md"), body)?;
        let prepared = prepared(&w, &repo)?;
        let request = get(&prepared, "request");
        if !pending {
            assert_eq!(
                get(&land(&w, &repo, request)?, "publication_verified"),
                &Value::Bool(true)
            );
            assert!(remote_head(&w, &repo)?.contains(text(get(request, "candidate"))));
            continue;
        }
        let intended = bank_files(&bank)?;
        let (inspected, applied) = pending_handoff(&w, &repo, &bank, &intended)?;
        assert!(
            get(&applied, "pending_units")
                .as_int()
                .unwrap()
                .as_i64()
                .unwrap()
                > 0
        );
        assert_eq!(bank_files(&bank)?, intended);
        let before = state(&w, &repo, &bank)?;
        let directory = private_layout(&w, &bank, text(get(&inspected, "operation_id")));
        let retained = snapshot_directory(&directory)?;
        let candidate = git(
            &w,
            &repo,
            &["cat-file", "commit", text(get(request, "candidate"))],
        )?;
        let refs = git(&w, &repo, &["ls-remote", "origin"])?;
        let (code, refused) = publish(&w, &repo, "land", request)?;
        assert_ne!(code, 0);
        assert_eq!(get(&refused, "publication_verified"), &Value::Bool(false));
        assert_eq!(text(get(&refused, "sharing_state")), "pending-inspection");
        assert_eq!(
            text(get(&refused, "failure_code")),
            "publication-unverified"
        );
        assert_eq!(get(&refused, "git_metadata_writes_performed"), &Value::Null);
        assert_eq!(git(&w, &repo, &["ls-remote", "origin"])?, refs);
        assert_eq!(state(&w, &repo, &bank)?, before);
        assert_eq!(snapshot_directory(&directory)?, retained);
        assert_eq!(
            git(
                &w,
                &repo,
                &["cat-file", "commit", text(get(request, "candidate"))]
            )?,
            candidate
        );
    }
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication_closure.py::RecoveryPublicationClosure.test_ordinary_publish_refuses_a_previously_reviewed_proof_after_pending_handoff
#[test]
fn ordinary_publish_refuses_a_previously_reviewed_proof_after_pending_handoff() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    let body = fs::read_to_string(bank.join("activeContext.md"))?.replacen(
        "# Current focus\n",
        "# Active context\n",
        1,
    ) + "\nUnder the controlled fixture condition, preserve the source before acting.\n";
    fs::write(bank.join("activeContext.md"), body)?;
    let proof = proof(&w, &repo)?;
    assert_eq!(publish(&w, &repo, "prepare", &proof)?.0, 0);
    let intended = bank_files(&bank)?;
    let (inspected, _) = pending_handoff(&w, &repo, &bank, &intended)?;
    let directory = private_layout(&w, &bank, text(get(&inspected, "operation_id")));
    let retained = snapshot_directory(&directory)?;
    let before = state(&w, &repo, &bank)?;
    let head = remote_head(&w, &repo)?;
    let (code, refused) = publish(&w, &repo, "prepare", &proof)?;
    assert_ne!(code, 0);
    assert!(!encode(&refused)?.contains("Unknown provenance requires independent review."));
    assert_eq!(state(&w, &repo, &bank)?, before);
    assert_eq!(snapshot_directory(&directory)?, retained);
    assert_eq!(remote_head(&w, &repo)?, head);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication_closure.py::RecoveryPublicationClosure.test_policy_and_required_review_leave_candidate_and_private_evidence_intact
#[test]
fn policy_and_required_review_leave_candidate_and_private_evidence_intact() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let request = layout_prepared(&w, &f)?;
    let before = state(&w, &f.repo, &f.bank)?;
    let directory = private_layout(&w, &f.bank, &f.operation);
    let retained = snapshot_directory(&directory)?;
    let candidate = git(
        &w,
        &f.repo,
        &["cat-file", "commit", text(get(&request, "candidate"))],
    )?;
    let head = remote_head(&w, &f.repo)?;
    let baseline = read_policy(&f.policy)?;
    for review in [false, true] {
        fs::write(&f.policy, encode(&baseline)?)?;
        if review {
            edit_policy(
                &f.policy,
                &[
                    ("rules", Value::Array(vec![])),
                    ("protected", Value::Bool(true)),
                    ("review", s("REVIEW_REQUIRED")),
                ],
            )?;
        } else {
            edit_policy(
                &f.policy,
                &[(
                    "rules",
                    Value::Array(vec![obj([("type", s("required_deployments"))])]),
                )],
            )?;
        }
        let refused = layout_land(&w, &f, &request, 1)?;
        assert_eq!(text(get(&refused, "state")), "pending");
        assert_eq!(get(&refused, "verified"), &Value::Bool(false));
        for field in [
            "publication_verified",
            "canonical_writes_performed",
            "canonical_mutation_may_have_occurred",
            "original_index_writes_performed",
            "head_writes_performed",
            "feature_writes_performed",
            "history_writes_performed",
        ] {
            assert_eq!(get(&refused, field), &Value::Bool(false), "{field}");
        }
        assert_eq!(remote_head(&w, &f.repo)?, head);
        assert_eq!(state(&w, &f.repo, &f.bank)?, before);
        assert_eq!(snapshot_directory(&directory)?, retained);
        assert_eq!(
            git(
                &w,
                &f.repo,
                &["cat-file", "commit", text(get(&request, "candidate"))]
            )?,
            candidate
        );
    }
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication_closure.py::RecoveryPublicationClosure.test_policy_at_observation_refuses_prepare_or_required_review_land
#[test]
fn policy_at_observation_refuses_prepare_or_required_review_land() -> TestResult {
    for review in [false, true] {
        let mut w = World::new()?;
        let f = public_repair(&mut w)?;
        if review {
            edit_policy(
                &f.policy,
                &[
                    ("rules", Value::Array(vec![])),
                    ("protected", Value::Bool(true)),
                    ("review", s("REVIEW_REQUIRED")),
                ],
            )?;
        } else {
            edit_policy(
                &f.policy,
                &[(
                    "rules",
                    Value::Array(vec![obj([("type", s("required_deployments"))])]),
                )],
            )?;
        }
        let observed = layout_publication_observation(&w, &f)?;
        let input = save(
            &w,
            &publication_input(get(&observed, "observation"), &f.intended)?,
            "policy-at-observation.json",
        )?;
        let input_bytes = fs::read(&input)?;
        let before = state(&w, &f.repo, &f.bank)?;
        let directory = private_layout(&w, &f.bank, &f.operation);
        let mut retained = snapshot_directory(&directory)?;
        let head = remote_head(&w, &f.repo)?;
        let prepared = recovery_output(
            &w,
            &f.repo,
            &f.operation,
            &[
                os("publish"),
                os("prepare"),
                os("--input"),
                input.as_os_str(),
            ],
            if review { 0 } else { 1 },
        )?;
        let pending = if review {
            let request = get(&prepared, "request");
            retained = snapshot_directory(&directory)?;
            let candidate = git(
                &w,
                &f.repo,
                &["cat-file", "commit", text(get(request, "candidate"))],
            )?;
            edit_policy(
                &f.policy,
                &[(
                    "rules",
                    Value::Array(vec![obj([("type", s("required_deployments"))])]),
                )],
            )?;
            assert_eq!(
                get(&layout_land(&w, &f, request, 1)?, "publication_verified"),
                &Value::Bool(false)
            );
            assert_eq!(snapshot_directory(&directory)?, retained);
            edit_policy(&f.policy, &[("rules", Value::Array(vec![]))])?;
            let pending = layout_land(&w, &f, request, 1)?;
            assert_eq!(
                git(
                    &w,
                    &f.repo,
                    &["cat-file", "commit", text(get(request, "candidate"))]
                )?,
                candidate
            );
            pending
        } else {
            prepared
        };
        assert_eq!(
            text(get(&pending, "state")),
            if review {
                "pending-repository-rules"
            } else {
                "pending-policy"
            }
        );
        assert_eq!(get(&pending, "publication_verified"), &Value::Bool(false));
        assert_eq!(state(&w, &f.repo, &f.bank)?, before);
        assert_eq!(snapshot_directory(&directory)?, retained);
        assert_eq!(remote_head(&w, &f.repo)?, head);
        assert_eq!(fs::read(&input)?, input_bytes);
    }
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication_closure.py::RecoveryPublicationClosure.test_recovery_historical_retry_preserves_later_authority_correction
#[test]
fn recovery_historical_retry_preserves_later_authority_correction() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let request = layout_prepared(&w, &f)?;
    let first = layout_land(&w, &f, &request, 0)?;
    assert_eq!(text(get(&first, "state")), "landed");
    assert_eq!(get(&first, "witnessed_commit"), get(&request, "candidate"));
    let correction = "\nA later independently made authority correction takes precedence.\n";
    let advanced = advance_authority(&w, &f, "memory-bank/activeContext.md", correction)?;
    assert_ne!(advanced, text(get(&first, "witnessed_commit")));
    let before = state(&w, &f.repo, &f.bank)?;
    let directory = private_layout(&w, &f.bank, &f.operation);
    let retained = snapshot_directory(&directory)?;
    let refs = git(&w, &f.repo, &["ls-remote", "origin"])?;
    let retry = layout_land(&w, &f, &request, 0)?;
    assert_eq!(text(get(&retry, "state")), "already-landed");
    for field in [
        "publication_verified",
        "receipt_is_historical",
        "already_applied",
    ] {
        assert_eq!(get(&retry, field), &Value::Bool(true));
    }
    assert_eq!(
        get(&retry, "witnessed_commit"),
        get(&first, "witnessed_commit")
    );
    assert_eq!(get(&retry, "receipt"), get(&first, "receipt"));
    assert_eq!(get(&retry, "intended_delta_current"), &Value::Bool(false));
    assert_eq!(
        git(
            &w,
            &f.repo,
            &["show", &format!("{advanced}:memory-bank/activeContext.md")]
        )?,
        text(get(&f.intended, "activeContext.md")).to_owned() + correction
    );
    assert_eq!(git(&w, &f.repo, &["ls-remote", "origin"])?, refs);
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    assert_eq!(snapshot_directory(&directory)?, retained);
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_publication_closure.py::RecoveryPublicationClosure.test_recovery_land_refuses_authority_advance_after_prepare
#[test]
fn recovery_land_refuses_authority_advance_after_prepare() -> TestResult {
    let mut w = World::new()?;
    let f = public_repair(&mut w)?;
    let request = layout_prepared(&w, &f)?;
    let candidate = git(
        &w,
        &f.repo,
        &["cat-file", "commit", text(get(&request, "candidate"))],
    )?;
    let advanced = advance_authority(&w, &f, "code.py", "\n# Independent authority advance.\n")?;
    assert_ne!(advanced, text(get(&request, "base")));
    let before = state(&w, &f.repo, &f.bank)?;
    let directory = private_layout(&w, &f.bank, &f.operation);
    let retained = snapshot_directory(&directory)?;
    let refs = git(&w, &f.repo, &["ls-remote", "origin"])?;
    let pending = layout_land(&w, &f, &request, 1)?;
    assert_eq!(text(get(&pending, "state")), "pending");
    for field in [
        "publication_verified",
        "canonical_writes_performed",
        "canonical_mutation_may_have_occurred",
        "original_index_writes_performed",
        "head_writes_performed",
        "feature_writes_performed",
        "history_writes_performed",
    ] {
        assert_eq!(get(&pending, field), &Value::Bool(false), "{field}");
    }
    assert_eq!(git(&w, &f.repo, &["ls-remote", "origin"])?, refs);
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    assert_eq!(snapshot_directory(&directory)?, retained);
    assert_eq!(
        git(
            &w,
            &f.repo,
            &["cat-file", "commit", text(get(&request, "candidate"))]
        )?,
        candidate
    );
    Ok(())
}

// Regression for a baseline defect (A1): with pending recovery the preview must
// use --private-output, whose artifact is stored canonical (sorted), while the
// capture arrived in authored/printed key order. Apply re-derives the record
// from the stored evidence; preview must have derived it from the same form.
#[test]
fn stdout_ordered_capture_with_pending_recovery_previews_and_applies_from_the_private_artifact()
-> TestResult {
    let mut w = World::new()?;
    let handoff = SYNTHETIC_POLICIES[0].to_owned() + "\n";
    let f = public_repair_with_pending(&mut w, Some(&handoff))?;
    assert!(
        !get(&f.applied, "pending_unit_ids")
            .as_array()
            .unwrap()
            .is_empty()
    );
    fs::write(
        f.repo.join("synthetic-policy.md"),
        "# Independent synthetic repository source\n\n".to_owned() + SYNTHETIC_POLICIES[0] + "\n",
    )?;
    let body = "# Synthetic policy\n\n".to_owned() + SYNTHETIC_POLICIES[0] + "\n";
    let (proposal, target) = admission_proposal(
        &w,
        &f.repo,
        &f.bank,
        &body,
        "synthetic-policy.md",
        Some(&f.inspected),
        true,
    )?;
    let raw = fs::read_to_string(w.root.join("admission-capture-private.json"))?;
    assert!(raw.find("\"schema\"").unwrap() < raw.find("\"admission-reason\"").unwrap());
    let record = text(get(get(&proposal, "files"), &target)).to_owned();
    assert!(record.starts_with("---\nadmission-reason: "));
    // Apply exactly the stored private artifact, as an operator would.
    let artifact = w.root.join("admission-preview-private.json");
    let admitted = good(
        &w,
        "am admission",
        &[os("apply"), os("--proposal"), artifact.as_os_str()],
    )?;
    assert_eq!(get(&admitted, "admission_verified"), &Value::Bool(true));
    assert_eq!(fs::read_to_string(f.bank.join(&target))?, record);
    let status = recovery_call(
        &w,
        &f.repo,
        &[os("status"), os("--operation"), os(&f.operation)],
        0,
    )?;
    assert_eq!(get(&status, "pending_unit_ids"), &Value::Array(vec![]));
    Ok(())
}
// obligation: tests/contracts/test_layout_recovery_resolved_publication.py::ResolvedPublicationJourneyContract.test_eight_resolved_units_land_and_recall_in_an_empty_profile
#[test]
fn eight_resolved_units_land_and_recall_in_an_empty_profile() -> TestResult {
    let mut w = World::new()?;
    let mut handoff = String::new();
    for policy in SYNTHETIC_POLICIES {
        let mut unit = policy.to_owned() + "\n";
        unit.push_str(&" ".repeat(3560 - unit.len()));
        handoff.push_str(&unit);
    }
    handoff.truncate(28478);
    assert_eq!(handoff.len(), 28478);
    let f = public_repair_with_pending(&mut w, Some(&handoff))?;
    fs::write(
        f.repo.join("synthetic-policy.md"),
        "# Independent synthetic repository source\n\n".to_owned()
            + &SYNTHETIC_POLICIES.join("\n\n")
            + "\n",
    )?;
    let originals = &f.originals;
    assert_eq!(originals["activeContext.md"].len(), 79054);
    assert_eq!(originals["handoff-prompt.md"].len(), 28478);
    let feature = state(&w, &f.repo, &f.bank)?;
    let inspected = &f.inspected;
    let applied = &f.applied;
    assert_eq!(
        get(&applied, "semantic_recovery_complete"),
        &Value::Bool(false)
    );
    let pending = get(&applied, "pending_unit_ids").as_array().unwrap();
    assert_eq!(pending.len(), 8);
    let directory = private_layout(&w, &f.bank, &f.operation);
    let initial = read_json(&directory.join("ledger/00000000.json"))?;
    let initial_units = get(&initial, "units").as_array().unwrap();
    assert_eq!(initial_units.len(), 8);
    let before = state(&w, &f.repo, &f.bank)?;
    let remote = remote_head(&w, &f.repo)?;
    recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[os("publish"), os("inspect")],
        1,
    )?;
    assert_eq!(remote_head(&w, &f.repo)?, remote);
    assert_eq!(state(&w, &f.repo, &f.bank)?, before);
    let source = get(get(&inspected, "manifest"), "sources")
        .as_array()
        .unwrap()
        .iter()
        .find(|row| text(get(row, "path")) == "handoff-prompt.md")
        .unwrap();
    let mut ranges: Vec<_> = initial_units
        .iter()
        .flat_map(|u| get(u, "source_ranges").as_array().unwrap())
        .filter(|span| get(span, "source_id") == get(source, "source_id"))
        .map(|span| {
            (
                get(span, "start").as_int().unwrap().as_i64().unwrap(),
                get(span, "end").as_int().unwrap().as_i64().unwrap(),
            )
        })
        .collect();
    ranges.sort();
    assert_eq!(
        ranges,
        vec![
            (0, 3560),
            (3560, 7120),
            (7120, 10680),
            (10680, 14240),
            (14240, 17800),
            (17800, 21360),
            (21360, 24920),
            (24920, 28478)
        ]
    );
    let body = "# Synthetic policies\n\n".to_owned() + &SYNTHETIC_POLICIES.join("\n\n") + "\n";
    let (proposal, target) = admission_proposal(
        &w,
        &f.repo,
        &f.bank,
        &body,
        "synthetic-policy.md",
        Some(&inspected),
        false,
    )?;
    let admitted = apply_admission(&w, &proposal)?;
    let status = recovery_call(
        &w,
        &f.repo,
        &[os("status"), os("--operation"), os(&f.operation)],
        0,
    )?;
    assert_eq!(
        get(&status, "semantic_recovery_complete"),
        &Value::Bool(true)
    );
    assert_eq!(get(&status, "pending_unit_ids"), &Value::Array(vec![]));
    let head = read_json(&directory.join("ledger-head.json"))?;
    let generation = get(&head, "generation").as_int().unwrap().as_i64().unwrap();
    let ledger = read_json(&directory.join(format!("ledger/{generation:08}.json")))?;
    let mut resolutions = std::collections::BTreeSet::new();
    for unit in get(&ledger, "units").as_array().unwrap() {
        let original = initial_units
            .iter()
            .find(|old| get(old, "unit_id") == get(unit, "unit_id"))
            .unwrap();
        assert_eq!(text(get(unit, "state")), "admitted");
        for field in ["source_ranges", "provenance"] {
            assert_eq!(get(unit, field), get(original, field));
        }
        let links = get(unit, "terminal_receipts").as_array().unwrap();
        assert_eq!(links.len(), 1);
        let resolution = text(get(&links[0], "resolution_id"));
        resolutions.insert(resolution.to_owned());
        let receipt = read_json(&directory.join(format!("resolutions/{resolution}.json")))?;
        assert_eq!(hash_value(&receipt), text(get(&links[0], "sha256")));
        assert_eq!(
            get(&receipt, "canonical_operation_id"),
            get(&proposal, "operation_id")
        );
        assert_eq!(get(&receipt, "unit_id"), get(unit, "unit_id"));
    }
    assert_eq!(resolutions.len(), 8);
    let receipt = read_json(Path::new(text(get(&admitted, "receipt_path"))))?;
    let input = save(
        &w,
        &publication_with_admission(&w, &f, &proposal, &receipt)?,
        "resolved-publication.json",
    )?;
    let prepared = recovery_output(
        &w,
        &f.repo,
        &f.operation,
        &[
            os("publish"),
            os("prepare"),
            os("--input"),
            input.as_os_str(),
        ],
        0,
    )?;
    let resolved = state(&w, &f.repo, &f.bank)?;
    assert_eq!(
        get(
            &layout_land(&w, &f, get(&prepared, "request"), 0)?,
            "publication_verified"
        ),
        &Value::Bool(true)
    );
    assert_eq!(state(&w, &f.repo, &f.bank)?, resolved);
    assert_eq!(
        (&resolved.1, &resolved.2, &resolved.3, &resolved.4),
        (&feature.1, &feature.2, &feature.3, &feature.4)
    );
    for source in get(get(&inspected, "manifest"), "sources")
        .as_array()
        .unwrap()
    {
        assert_eq!(
            fs::read(directory.join(text(get(source, "blob"))))?,
            originals[text(get(source, "path"))]
        );
    }
    let changed = git(
        &w,
        &f.remote,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "refs/heads/main",
        ],
    )?;
    assert!(changed.lines().all(|p| p.starts_with("memory-bank/")));
    assert!(
        changed
            .lines()
            .any(|p| p == format!("memory-bank/{target}"))
    );
    // Baseline :117-120 reads the admitted record from the bank itself.
    let (_, metadata, recalled_body) =
        agent_memory::records::parse(&fs::read_to_string(f.bank.join(&target))?, &[])?;
    for policy in SYNTHETIC_POLICIES {
        assert!(recalled_body.contains(policy));
    }
    assert!(
        get(&metadata, "evidence")
            .as_array()
            .unwrap()
            .contains(&s("synthetic-policy.md"))
    );
    fresh_recall(&mut w, &f, &target, &s(recalled_body), Some(&metadata))
}

#[test]
// obligation: tests/contracts/test_validation_memo.py::ValidationMemoContract.test_public_land_rechecks_source_authority_index_and_policy_after_first_check
fn fanout_d_public_land_rechecks_source_authority_index_and_policy() -> TestResult {
    for action in ["source", "authority", "index", "policy"] {
        let mut w = World::new()?;
        let (repo, bank, peer, _) = fixture(&mut w)?;
        let policy = github(&mut w, &repo, &bank, Some(&peer), false)?;
        fs::write(
            bank.join("activeContext.md"),
            "# Current focus\n\nRetain the authored local learning.\n",
        )?;
        let request = get(&prepared(&w, &repo)?, "request").clone();
        let before = state(&w, &repo, &bank)?;
        let head = git(&w, &repo, &["ls-remote", "origin", "refs/heads/main"])?;
        let input = save(&w, &request, "fanout-final-review.json")?;
        let pause = w.inside("fanout-final-review")?;
        let stage = "publication-before-final-review";
        let env = [
            ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
            ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
            (
                "AGENT_MEMORY_FAULT_DIR".into(),
                pause.as_os_str().to_owned(),
            ),
        ];
        let child = w.spawn_with(
            [
                w.env[OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone(),
                "--json".into(),
                "publish".into(),
                "land".into(),
                "--repo".into(),
                repo.as_os_str().to_owned(),
                "--input".into(),
                input.into_os_string(),
            ],
            RunOptions {
                check: false,
                env: &env,
                ..RunOptions::default()
            },
        )?;
        let deadline = child.deadline()?;
        while !pause.join(format!("{stage}.ready")).exists() {
            if deadline.expired() {
                let outcome = child.wait();
                let reached = pause.join(format!("{stage}.ready")).exists();
                match outcome {
                    Ok(output) => panic!(
                        "publication did not reach final review within the deadline for {action}; ready after collection={reached}; status={:?}; stdout={}; stderr={}",
                        output.status,
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr),
                    ),
                    Err(error) => panic!(
                        "publication did not reach final review within the deadline for {action}; ready after collection={reached}; child collection failed: {error:?}"
                    ),
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        match action {
            "source" => {
                let target = bank.join("activeContext.md");
                fs::write(
                    &target,
                    fs::read_to_string(&target)? + "\nLater local evidence stays preserved.\n",
                )?;
            }
            "authority" => {
                git(
                    &w,
                    &peer,
                    &[
                        "commit",
                        "--allow-empty",
                        "-m",
                        "Later authority observation",
                    ],
                )?;
                git(&w, &peer, &["push", "origin", "HEAD:main"])?;
            }
            "index" => {
                fs::write(
                    repo.join("independent-feature.txt"),
                    "Later staged feature work.\n",
                )?;
                git(&w, &repo, &["add", "independent-feature.txt"])?;
            }
            "policy" => {
                let mut value = decode(&fs::read(&policy)?)?;
                set(&mut value, "push", Value::Bool(false));
                fs::write(&policy, encode(&value)?)?;
            }
            _ => unreachable!(),
        }
        fs::write(pause.join(format!("{stage}.release")), b"")?;
        let output = child.wait()?;
        assert_eq!(output.status, agent_memory::process::Status::Code(1));
        assert!(
            !get(&decode(&output.stdout)?, "publication_verified")
                .as_bool()
                .unwrap_or(false)
        );
        let after = state(&w, &repo, &bank)?;
        assert_eq!(before.1, after.1);
        assert_eq!(before.3, after.3);
        assert_eq!(before.4, after.4);
        if action == "index" {
            assert_ne!(before.2, after.2);
        } else {
            assert_eq!(before.2, after.2);
        }
        if action == "source" {
            assert!(
                fs::read_to_string(bank.join("activeContext.md"))?.contains("Later local evidence")
            );
        } else {
            assert_eq!(before.0, after.0);
        }
        let actual = git(&w, &repo, &["ls-remote", "origin", "refs/heads/main"])?;
        if action == "authority" {
            assert_ne!(actual, head);
            assert!(actual.starts_with(git(&w, &peer, &["rev-parse", "HEAD"])?.trim()));
        } else {
            assert_eq!(actual, head);
        }
    }
    Ok(())
}

#[test]
// obligation: tests/contracts/test_claude_boundary.py::ClaudeBoundaryContract.test_published_ack_requires_real_authority_readback
fn fanout_d_claude_published_ack_requires_real_authority_readback() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _) = bootstrap(&mut w)?;
    github(&mut w, &repo, &bank, None, false)?;
    let session = agent_memory::host::uuid4()?;
    let prompt = agent_memory::host::uuid4()?;
    let message = agent_memory::host::uuid4()?;
    let transcript = w.inside("transcript-publish.jsonl")?;
    let row = obj([
        ("type", s("user")),
        ("sessionId", s(&session)),
        ("promptId", s(&prompt)),
        ("uuid", s(&message)),
        (
            "message",
            obj([("role", s("user")), ("content", s("Synthetic decision"))]),
        ),
    ]);
    fs::write(&transcript, encode(&row)? + "\n")?;
    let event = obj([
        ("hook_event_name", s("Stop")),
        ("stop_hook_active", Value::Bool(false)),
        ("session_id", s(&session)),
        ("prompt_id", s(&prompt)),
        ("cwd", s(repo.to_str().unwrap())),
        ("transcript_path", s(transcript.to_str().unwrap())),
        ("last_assistant_message", s("Synthetic answer")),
    ]);
    let (code, stdout, stderr) =
        native_dispatch(&w, &["hook".into(), "claude-stop".into()], Some(&event));
    assert_eq!(code, 2, "{stdout}\n{stderr}");
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let sha = agent_memory::developer_identity::digest(Some(b"Synthetic decision")).unwrap();
    let value = obj([
        ("session", s(&session)),
        ("prompt", s(&prompt)),
        ("source_uuid", s(&message)),
        ("source_sha256", s(sha)),
        ("outcome", s("published")),
        (
            "assessment",
            s("The reviewed candidate reflects the source."),
        ),
        ("publication_request", request.clone()),
    ]);
    let input = save(&w, &value, "publish-ack.json")?;
    let args = [
        OsString::from("--json"),
        "claude-boundary".into(),
        "--repo".into(),
        repo.as_os_str().to_owned(),
        "--input".into(),
        input.into_os_string(),
    ];
    assert_ne!(native_dispatch(&w, &args, None).0, 0);
    assert_eq!(
        get(&land(&w, &repo, &request)?, "publication_verified"),
        &Value::Bool(true)
    );
    let (code, stdout, stderr) = native_dispatch(&w, &args, None);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let after = decode(stdout.as_bytes())?;
    assert_eq!(get(&after, "publication_verified"), &Value::Bool(true));
    assert_eq!(
        get(&after, "assessment_is_agent_assertion"),
        &Value::Bool(true)
    );
    let (code, stdout, stderr) = native_dispatch(&w, &args, None);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let retry = decode(stdout.as_bytes())?;
    assert_eq!(get(&retry, "already_completed"), &Value::Bool(true));
    assert_eq!(get(&retry, "publication_verified"), &Value::Bool(true));
    Ok(())
}

#[test]
// obligation: tests/contracts/test_opencode_boundary_ledger.py::PublishedAcknowledgmentContract.test_requires_real_witnessed_publication
fn fanout_d_opencode_ack_requires_real_witnessed_publication() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _) = bootstrap(&mut w)?;
    github(&mut w, &repo, &bank, None, false)?;
    let request = get(&prepared(&w, &repo)?, "request").clone();
    let invoke = |action: &str, repository: &Path| {
        let payload = obj([
            ("root", s(w.root.to_str().unwrap())),
            ("session", s("fixture-session")),
            ("source", s("fixture-source")),
            ("action", s(action)),
            ("repo", s(repository.to_str().unwrap())),
            ("publication_request", request.clone()),
        ]);
        let (code, stdout, stderr) = native_dispatch(
            &w,
            &["hook".into(), "boundary-ledger".into()],
            Some(&payload),
        );
        let value = decode(stdout.as_bytes()).unwrap_or_else(|e| panic!("{e}: {stdout}\n{stderr}"));
        (code, value)
    };
    assert_eq!(get(&invoke("reserve", &repo).1, "status"), &s("reserved"));
    let (code, before) = invoke("published", &repo);
    assert_ne!(code, 0);
    assert_eq!(get(&before, "status"), &s("pending"));
    let landed = land(&w, &repo, &request)?;
    assert_eq!(get(&landed, "publication_verified"), &Value::Bool(true));
    let (code, proof) = invoke("published", &repo);
    assert_eq!(code, 0);
    assert_eq!(get(&proof, "publication_verified"), &Value::Bool(true));
    assert_eq!(
        get(&proof, "source_publication_link_verified"),
        &Value::Bool(false)
    );
    assert_eq!(
        get(&proof, "witnessed_commit"),
        get(&landed, "witnessed_commit")
    );
    assert_eq!(
        get(&invoke("inspect", &repo).1, "status"),
        &s("completed-source")
    );
    let (code, retry) = invoke("published", &repo);
    assert_eq!(code, 0);
    assert_eq!(get(&retry, "already_completed"), &Value::Bool(true));
    assert_eq!(get(&retry, "publication_verified"), &Value::Bool(true));
    let other = w.repository("other-repository", false)?;
    assert_ne!(invoke("published", &other).0, 0);
    Ok(())
}

#[test]
// obligation: tests/contracts/test_concurrent_bank_writers.py::ConcurrentBankWriters.test_publication_refuses_a_live_bank_lock_then_lands_without_runtime_state
fn fanout_d_publication_refuses_live_bank_lock_then_lands_only_canonical_paths() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, _) = bootstrap(&mut w)?;
    github(&mut w, &repo, &bank, None, false)?;
    let initial = get(&prepared(&w, &repo)?, "request").clone();
    let content = w.inside("defensive-race.md")?;
    fs::write(
        &content,
        fs::read_to_string(bank.join("projectbrief.md"))?
            + "\nRetain this actual defensive race result.\n",
    )?;
    let proposal = good(
        &w,
        "am writes",
        &[
            os("preview"),
            os("--repo"),
            repo.as_os_str(),
            os("--path"),
            os("projectbrief.md"),
            os("--content"),
            content.as_os_str(),
        ],
    )?;
    let input = save(&w, &proposal, "defensive-race.json")?;
    let pause = w.inside("defensive-race")?;
    let stage = "locked-before-inventory";
    let env = [
        ("AGENT_MEMORY_FAULT_STAGE".into(), stage.into()),
        ("AGENT_MEMORY_FAULT_MODE".into(), "pause".into()),
        (
            "AGENT_MEMORY_FAULT_DIR".into(),
            pause.as_os_str().to_owned(),
        ),
    ];
    let child = w.spawn_with(
        [
            w.env[OsStr::new("AGENT_MEMORY_TEST_OWNED_COMMAND")].clone(),
            "--json".into(),
            "writes".into(),
            "apply".into(),
            "--proposal".into(),
            input.into_os_string(),
        ],
        RunOptions {
            check: false,
            env: &env,
            ..RunOptions::default()
        },
    )?;
    let deadline = agent_memory::clock::Deadline::after(Duration::from_secs(8))?;
    while !pause.join(format!("{stage}.ready")).exists() {
        assert!(!deadline.expired());
        std::thread::sleep(Duration::from_millis(5));
    }
    let lock = bank.join(".agent-memory.lock");
    let lock_bytes = fs::read(&lock)?;
    let remote = git(&w, &repo, &["ls-remote", "origin", "refs/heads/main"])?;
    assert_ne!(publish(&w, &repo, "land", &initial)?.0, 0);
    assert_eq!(fs::read(&lock)?, lock_bytes);
    assert_eq!(
        git(&w, &repo, &["ls-remote", "origin", "refs/heads/main"])?,
        remote
    );
    fs::write(pause.join(format!("{stage}.release")), b"")?;
    let output = child.wait()?;
    assert_eq!(output.status, agent_memory::process::Status::Code(0));
    assert_eq!(
        get(&decode(&output.stdout)?, "verified"),
        &Value::Bool(true)
    );
    let before = state(&w, &repo, &bank)?;
    let request = get(&prepared(&w, &repo)?, "request").clone();
    assert_eq!(
        get(&land(&w, &repo, &request)?, "publication_verified"),
        &Value::Bool(true)
    );
    assert_eq!(state(&w, &repo, &bank)?, before);
    let tree = git(
        &w,
        &repo,
        &[
            "ls-tree",
            "-r",
            "--name-only",
            text(get(&request, "candidate")),
            "--",
            "memory-bank",
        ],
    )?;
    let mut files = BTreeMap::new();
    files_below(&bank, &bank, &mut files)?;
    let expected: std::collections::BTreeSet<_> = files
        .keys()
        .filter(|p| p.ends_with(".md") && !p.starts_with(".agent-memory"))
        .map(|p| format!("memory-bank/{p}"))
        .collect();
    assert_eq!(
        tree.lines()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    assert!(!tree.contains(".agent-memory"));
    assert_eq!(
        git(
            &w,
            &repo,
            &[
                "show",
                &format!("{}:code.py", text(get(&request, "candidate")))
            ]
        )?,
        "VALUE = 1\n"
    );
    Ok(())
}

fn fanout_d_new_index(
    w: &World,
    bank: &Path,
    name: &str,
    purpose: &str,
    introduction: Option<&str>,
    source: Option<&[u8]>,
    stamp: &str,
) -> TestResult<String> {
    let router = bank.join("MEMORY.md");
    let original = fs::read_to_string(&router)?;
    let project = support::record_fixtures::project_id(&original)?;
    let path = format!("topics/{name}.md");
    fs::create_dir_all(bank.join("topics"))?;
    let raw = match source {
        Some(bytes) => bytes.to_vec(),
        None => support::record_fixtures::record_text(
            &format!("# Archive policy\n\nRetain the independently evidenced {name} policy.\n"),
            project.as_str().unwrap(),
        )?
        .into_bytes(),
    };
    fs::write(bank.join(&path), raw)?;
    let request = obj([
        ("path", s("topics/shared-policy.md")),
        ("project-id", project),
        ("updated", s(stamp)),
        (
            "routes",
            Value::Array(vec![obj([
                ("path", s(&path)),
                ("purpose", s(purpose)),
                ("scope", s("repository")),
                ("lifecycle", s("active")),
                (
                    "read-when",
                    s("When inspecting this archive policy dimension"),
                ),
            ])]),
        ),
    ]);
    let input = save(w, &request, "fanout-index.json")?;
    let rendered = good(
        w,
        "am bank-layout",
        &[os("render-index"), os("--input"), input.as_os_str()],
    )?;
    let content = match introduction {
        Some(prose) => text(get(&rendered, "content"))
            .replace("\n## Routes\n", &format!("\n{prose}\n\n## Routes\n")),
        None => text(get(&rendered, "content")).to_owned(),
    };
    fs::write(bank.join("topics/shared-policy.md"), content)?;
    let row = "| [topics/shared-policy.md](<topics/shared-policy.md>) | Archive policy map | repository | active | When inspecting archive policy |\n";
    // The newly created record is routed only through its shared index.
    fs::write(
        router,
        original.replace(
            "\n## Omitted contexts",
            &(row.to_owned() + "\n## Omitted contexts"),
        ),
    )?;
    Ok(path)
}
fn fanout_d_publish_index(w: &World, peer: &Path) -> TestResult {
    git(w, peer, &["add", "memory-bank"])?;
    git(w, peer, &["commit", "-m", "Independent source-bound index"])?;
    git(w, peer, &["push", "origin", "HEAD:main"])?;
    Ok(())
}

fn fanout_d_review_new_index(w: &World, repo: &Path) -> TestResult<(i32, Value)> {
    let inspection = good(
        w,
        "am semantic",
        &[os("inspect"), os("--repo"), repo.as_os_str()],
    )?;
    let input = save(w, &judgment(&inspection)?, "fanout-index-judgment.json")?;
    call(
        w,
        "am semantic",
        &[
            os("review"),
            os("--repo"),
            repo.as_os_str(),
            os("--input"),
            input.as_os_str(),
        ],
    )
}

#[test]
// obligation: tests/contracts/test_new_topic_merge.py::NewTopicMergeContract.test_new_index_with_identical_introduction_and_distinct_routes_preserves_both_sources
fn fanout_d_new_shared_index_preserves_both_independent_source_routes() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    let first = fanout_d_new_index(
        &w,
        &bank,
        "retention",
        "Archive policy",
        None,
        None,
        "2026-10-23T00:00:00Z",
    )?;
    let second = fanout_d_new_index(
        &w,
        &peer.join("memory-bank"),
        "inspection",
        "Archive policy",
        None,
        None,
        "2026-10-24T00:00:00Z",
    )?;
    fanout_d_publish_index(&w, &peer)?;
    let before = state(&w, &repo, &bank)?;
    let (code, plan) = fanout_d_review_new_index(&w, &repo)?;
    assert_eq!(code, 0, "{plan:?}");
    assert_eq!(get(&plan, "conflicts_pending"), &Value::Bool(false));
    for key in ["refresh_files", "publish_files"] {
        let candidate = get(&plan, key);
        assert_eq!(
            text(get(candidate, &first)),
            fs::read_to_string(bank.join(&first))?
        );
        assert_eq!(
            text(get(candidate, &second)),
            fs::read_to_string(peer.join("memory-bank").join(&second))?
        );
        let index = text(get(candidate, "topics/shared-policy.md"));
        assert!(index.contains(&first));
        assert!(index.contains(&second));
    }
    assert_eq!(state(&w, &repo, &bank)?, before);
    Ok(())
}

#[test]
// obligation: tests/contracts/test_new_topic_merge.py::NewTopicMergeContract.test_absent_prose_baseline_does_not_license_different_new_introductions
fn fanout_d_absent_prose_baseline_does_not_license_conflicting_introductions() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    fanout_d_new_index(
        &w,
        &bank,
        "retention",
        "Archive policy",
        Some("The shared policy requires a yellow finish."),
        None,
        "2026-10-23T00:00:00Z",
    )?;
    fanout_d_new_index(
        &w,
        &peer.join("memory-bank"),
        "inspection",
        "Archive policy",
        Some("The shared policy requires a blue finish."),
        None,
        "2026-10-23T00:00:00Z",
    )?;
    fanout_d_publish_index(&w, &peer)?;
    let before = state(&w, &repo, &bank)?;
    assert_eq!(fanout_d_review_new_index(&w, &repo)?.0, 1);
    assert_eq!(state(&w, &repo, &bank)?, before);
    Ok(())
}

#[test]
// obligation: tests/contracts/test_new_topic_merge.py::NewTopicMergeContract.test_absent_route_baseline_does_not_choose_between_different_captions_for_one_target
fn fanout_d_absent_route_baseline_never_chooses_conflicting_captions() -> TestResult {
    let mut w = World::new()?;
    let (repo, bank, peer, _) = fixture(&mut w)?;
    github(&mut w, &repo, &bank, Some(&peer), false)?;
    let path = fanout_d_new_index(
        &w,
        &bank,
        "shared-source",
        "Retention interpretation",
        None,
        None,
        "2026-10-23T00:00:00Z",
    )?;
    fanout_d_new_index(
        &w,
        &peer.join("memory-bank"),
        "shared-source",
        "Inspection interpretation",
        None,
        Some(&fs::read(bank.join(path))?),
        "2026-10-23T00:00:00Z",
    )?;
    fanout_d_publish_index(&w, &peer)?;
    let before = state(&w, &repo, &bank)?;
    assert_eq!(fanout_d_review_new_index(&w, &repo)?.0, 1);
    assert_eq!(state(&w, &repo, &bank)?, before);
    Ok(())
}

#[path = "support/gaps_multi.rs"]
mod gaps_multi;
