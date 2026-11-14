//! Real native release assets and real local bare Git authority. Only gh's
//! external service is controlled. HTTPS uses real curl and an ephemeral TLS
//! server; the curl launcher changes routing, never response bytes or status.
use super::*;
use crate::process::{self, Input, Spec, Target};
use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::{PermissionsExt, symlink};

fn save(path: &Path, raw: impl AsRef<[u8]>, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn read(path: &Path) -> Value {
    release::parse(&release::regular(path, release::MAX_MANIFEST).unwrap()).unwrap()
}
fn response(value: &Value) -> Result<String> {
    utf8(release::canonical(value)?)
}
fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    text(v, key).unwrap()
}
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), target).unwrap();
        } else {
            panic!("portable fixture contains a link")
        }
    }
}
fn native_binary() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let profile = exe.parent().unwrap().parent().unwrap();
    let binary = profile.join("am");
    assert!(
        binary.is_file(),
        "build the real am binary before publisher contracts: {}",
        binary.display()
    );
    binary
}
fn executable(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("publisher contract requires {name}"))
}
fn execute(argv: &[OsString], cwd: &Path) -> process::Output {
    let env = std::env::vars_os().collect::<Vec<_>>();
    process::run(&Spec {
        argv,
        env: &env,
        cwd: Some(cwd),
        search_path: true,
        stdin: Input::Null,
        stdout: Target::Capture {
            limit: 4 * 1024 * 1024,
        },
        stderr: Target::Capture { limit: 1024 * 1024 },
        deadline: crate::clock::Deadline::after(Duration::from_secs(60)).unwrap(),
        new_session: false,
        file_size_limit: None,
        umask: Some(0o077),
        pass_fds: &[],
    })
    .unwrap()
}
fn git_at(root: &Path, args: &[&str]) -> String {
    let argv = std::iter::once(OsString::from("git"))
        .chain(args.iter().map(OsString::from))
        .collect::<Vec<_>>();
    let out = execute(&argv, root);
    assert_eq!(out.status, Status::Code(0), "{out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn isolated(name: &str, modes: &[&str]) -> bool {
    if std::env::var_os("GAPS_PUBLISH_ROOT").is_some() {
        return true;
    }
    for mode in modes {
        let scratch = release::Scratch::new("am-gaps-publisher-").unwrap();
        for directory in [
            "home",
            "config",
            "state",
            "tmp",
            "tools",
            "service/assets",
            "service/calls",
            "www",
        ] {
            crate::fs::private_mkdir(&scratch.path.join(directory)).unwrap();
        }
        let exe = std::env::current_exe().unwrap();
        let namespace = module_path!().split_once("::").unwrap().1;
        let test_name = format!("{namespace}::{name}");
        let gh_name = format!("{namespace}::gh_worker");
        let env = vec![
            (
                "PATH".into(),
                format!(
                    "{}:/usr/bin:/bin:/usr/local/bin",
                    scratch.path.join("tools").display()
                )
                .into(),
            ),
            ("HOME".into(), scratch.path.join("home").into_os_string()),
            (
                "XDG_CONFIG_HOME".into(),
                scratch.path.join("config").into_os_string(),
            ),
            (
                "XDG_STATE_HOME".into(),
                scratch.path.join("state").into_os_string(),
            ),
            ("TMPDIR".into(), scratch.path.join("tmp").into_os_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("GIT_CONFIG_SYSTEM".into(), "/dev/null".into()),
            ("GIT_ALLOW_PROTOCOL".into(), "file".into()),
            ("GIT_AUTHOR_NAME".into(), "Publisher fixture".into()),
            (
                "GIT_AUTHOR_EMAIL".into(),
                "publisher@example.invalid".into(),
            ),
            ("GIT_COMMITTER_NAME".into(), "Publisher fixture".into()),
            (
                "GIT_COMMITTER_EMAIL".into(),
                "publisher@example.invalid".into(),
            ),
            ("LC_ALL".into(), "C".into()),
            ("NO_PROXY".into(), "*".into()),
            (
                "GAPS_PUBLISH_ROOT".into(),
                scratch.path.clone().into_os_string(),
            ),
            ("GAPS_PUBLISH_MODE".into(), (*mode).into()),
            ("GAPS_PUBLISH_EXE".into(), exe.clone().into_os_string()),
            ("GAPS_PUBLISH_GH_TEST".into(), gh_name.into()),
            ("GAPS_REAL_CURL".into(), executable("curl").into_os_string()),
        ];
        let argv = [
            exe.into_os_string(),
            "--exact".into(),
            test_name.into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ];
        let output = process::run(&Spec {
            argv: &argv,
            env: &env,
            cwd: Some(&scratch.path),
            search_path: false,
            stdin: Input::Null,
            stdout: Target::Capture { limit: 1024 * 1024 },
            stderr: Target::Capture { limit: 1024 * 1024 },
            deadline: crate::clock::Deadline::after(Duration::from_secs(180)).unwrap(),
            new_session: false,
            file_size_limit: None,
            umask: Some(0o077),
            pass_fds: &[],
        })
        .unwrap();
        assert_eq!(
            output.status,
            Status::Code(0),
            "{mode}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
    false
}
struct Fixture {
    root: PathBuf,
    inputs: BuildInputs,
    signers: String,
    server: process::OwnedProcess,
}
impl Fixture {
    fn new() -> Self {
        crate::cli::register_providers().unwrap();
        let root = PathBuf::from(std::env::var_os("GAPS_PUBLISH_ROOT").unwrap());
        let checkout = root.join("checkout");
        fs::create_dir(&checkout).unwrap();
        git_at(&checkout, &["init", "-b", "main"]);
        save(
            &checkout.join("README.txt"),
            b"Initial public source.\n",
            0o644,
        );
        git_at(&checkout, &["add", "."]);
        git_at(&checkout, &["commit", "-m", "Public base"]);
        let base = git_at(&checkout, &["rev-parse", "HEAD"]);
        let bare = root.join("authority.git");
        git_at(
            &root,
            &[
                "init",
                "--bare",
                "--initial-branch=main",
                bare.to_str().unwrap(),
            ],
        );
        git_at(
            &checkout,
            &[
                "remote",
                "add",
                "origin",
                &format!("https://github.com/{}.git", release::REPOSITORY),
            ],
        );
        git_at(
            &checkout,
            &["config", "remote.origin.pushurl", bare.to_str().unwrap()],
        );
        git_at(&checkout, &["push", "origin", "HEAD:main"]);
        git_at(&checkout, &["checkout", "-b", "integrate/gaps"]);
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
            &checkout.join("skills/agent-memory"),
        );
        save(
            &checkout.join("install.sh"),
            include_bytes!("../../../install.sh"),
            0o755,
        );
        git_at(&checkout, &["add", "."]);
        git_at(&checkout, &["commit", "-m", "Reviewed native release"]);
        let source = git_at(&checkout, &["rev-parse", "HEAD"]);
        git_at(
            &checkout,
            &["push", "origin", "HEAD:refs/heads/integrate/gaps"],
        );
        let key = root.join("key");
        let out = execute(
            &[
                "ssh-keygen".into(),
                "-q".into(),
                "-t".into(),
                "ed25519".into(),
                "-N".into(),
                "".into(),
                "-f".into(),
                key.clone().into_os_string(),
            ],
            &root,
        );
        assert_eq!(out.status, Status::Code(0));
        let public = fs::read_to_string(key.with_extension("pub")).unwrap();
        let fields = public
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        let signers = format!(
            "{} namespaces=\"{},{}\" {fields}\n",
            release::PRINCIPAL,
            release::NAMESPACE,
            release::BINARY_NAMESPACE
        );
        let mode = std::env::var("GAPS_PUBLISH_MODE").unwrap();
        let foreign_repo = root.join("foreign");
        git_at(
            &root,
            &[
                "clone",
                "--",
                bare.to_str().unwrap(),
                foreign_repo.to_str().unwrap(),
            ],
        );
        if mode == "push-race" {
            git_at(&foreign_repo, &["checkout", "-b", "foreign", &base]);
        } else {
            git_at(&foreign_repo, &["checkout", "-b", "foreign", &source]);
        }
        match mode.as_str() {
            "bootstrap-revert" => save(
                &foreign_repo.join("install.sh"),
                b"#!/bin/sh\nexit 77\n",
                0o755,
            ),
            "runtime-revert" => save(
                &foreign_repo.join("skills/agent-memory/templates/gaps.txt"),
                b"Changed runtime outside signed release.\n",
                0o644,
            ),
            _ => save(
                &foreign_repo.join("harmless.txt"),
                b"A harmless documentation descendant.\n",
                0o644,
            ),
        }
        git_at(&foreign_repo, &["add", "."]);
        git_at(
            &foreign_repo,
            &["commit", "-m", "Actual independent main commit"],
        );
        let foreign = git_at(&foreign_repo, &["rev-parse", "HEAD"]);
        git_at(
            &foreign_repo,
            &["push", "origin", "HEAD:refs/heads/foreign"],
        );
        if mode == "preexisting-tag" {
            git_at(
                &root,
                &[
                    "--git-dir",
                    bare.to_str().unwrap(),
                    "update-ref",
                    "refs/tags/v0.0.15",
                    &base,
                ],
            );
        }
        let config = object([
            ("root", string(root.to_str().unwrap())),
            ("checkout", string(checkout.to_str().unwrap())),
            ("bare", string(bare.to_str().unwrap())),
            ("source", string(&source)),
            ("base", string(base)),
            ("foreign", string(foreign)),
            ("mode", string(mode)),
        ]);
        save(
            &root.join("service/config.json"),
            release::canonical(&config).unwrap(),
            0o600,
        );
        let real_git = executable("git");
        let router = format!(
            "#!/bin/sh\nset -eu\nif [ \"$1\" = ls-remote ]; then\n shift\n [ \"$1\" = --tags ]; shift\n [ \"$1\" = origin ]; shift\n exec '{}' ls-remote --tags \"$GAPS_PUBLISH_ROOT/authority.git\" \"$@\"\nfi\nfor argument do\n if [ \"$argument\" = push ]; then printf x >> \"$GAPS_PUBLISH_ROOT/service/push-attempts\"; fi\ndone\nexec '{}' \"$@\"\n",
            real_git.display(),
            real_git.display()
        );
        save(&root.join("tools/git"), router, 0o755);
        save(&root.join("tools/gh"),br##"#!/bin/sh
set -eu
call=$(mktemp -d "$GAPS_PUBLISH_ROOT/service/calls/call.XXXXXXXX")
printf '%s\n' "$@" > "$call/args"
export GAPS_PUBLISH_CALL="$call"
"$GAPS_PUBLISH_EXE" --exact "$GAPS_PUBLISH_GH_TEST" --quiet --nocapture --test-threads=1 > "$call/worker.stdout" 2> "$call/worker.stderr" || { cat "$call/worker.stderr" >&2; exit 90; }
cat "$call/body"
cat "$call/stderr" >&2
exit "$(cat "$call/code")"
"##,0o755);
        let www = root.join("www");
        let prefix = www.join("robertelee78/agent-memory");
        fs::create_dir_all(prefix.join("releases/latest")).unwrap();
        fs::create_dir_all(prefix.join("releases/download")).unwrap();
        fs::create_dir_all(prefix.join("main")).unwrap();
        symlink(
            root.join("service/assets"),
            prefix.join("releases/latest/download"),
        )
        .unwrap();
        symlink(
            root.join("service/assets"),
            prefix.join("releases/download/v0.0.15"),
        )
        .unwrap();
        save(
            &prefix.join("main/install.sh"),
            fs::read(checkout.join("install.sh")).unwrap(),
            0o644,
        );
        save(&root.join("tls.cnf"),b"[req]\ndistinguished_name=dn\nx509_extensions=ext\nprompt=no\n[dn]\nCN=github.com\n[ext]\nsubjectAltName=DNS:github.com,DNS:raw.githubusercontent.com\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,digitalSignature,keyEncipherment,keyCertSign\n",0o600);
        let openssl = executable("openssl");
        let result = execute(
            &[
                openssl.clone().into_os_string(),
                "req".into(),
                "-x509".into(),
                "-newkey".into(),
                "rsa:2048".into(),
                "-nodes".into(),
                "-days".into(),
                "1".into(),
                "-config".into(),
                root.join("tls.cnf").into_os_string(),
                "-keyout".into(),
                root.join("tls.key").into_os_string(),
                "-out".into(),
                root.join("tls.pem").into_os_string(),
            ],
            &root,
        );
        assert_eq!(result.status, Status::Code(0), "{result:?}");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let argv = [
            openssl.into_os_string(),
            "s_server".into(),
            "-quiet".into(),
            "-WWW".into(),
            "-accept".into(),
            port.to_string().into(),
            "-cert".into(),
            root.join("tls.pem").into_os_string(),
            "-key".into(),
            root.join("tls.key").into_os_string(),
        ];
        let env = std::env::vars_os().collect::<Vec<_>>();
        let server = process::spawn_owned(&Spec {
            argv: &argv,
            env: &env,
            cwd: Some(&www),
            search_path: false,
            stdin: Input::Null,
            stdout: Target::Capture { limit: 65536 },
            stderr: Target::Capture { limit: 65536 },
            deadline: crate::clock::Deadline::after(Duration::from_secs(150)).unwrap(),
            new_session: false,
            file_size_limit: None,
            umask: None,
            pass_fds: &[],
        })
        .unwrap();
        let deadline = crate::clock::Deadline::after(Duration::from_secs(5)).unwrap();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(!deadline.expired());
            std::thread::sleep(Duration::from_millis(5));
        }
        let wrapper = format!(
            "#!/bin/sh\nexec \"$GAPS_REAL_CURL\" --cacert '{}' --connect-to 'github.com:443:127.0.0.1:{port}' --connect-to 'raw.githubusercontent.com:443:127.0.0.1:{port}' \"$@\"\n",
            root.join("tls.pem").display()
        );
        save(&root.join("tools/curl"), wrapper, 0o755);
        save(
            &root.join("notes"),
            b"Real native release fixture, not a public release.\n",
            0o600,
        );
        let binary = native_binary();
        let binaries = release::TARGETS
            .into_iter()
            .map(|target| (target.to_owned(), binary.clone()))
            .collect();
        Self {
            root,
            inputs: BuildInputs {
                root: checkout,
                key,
                binaries,
            },
            signers,
            server,
        }
    }
    fn publish(&self) -> Result<Value> {
        publish_trusted(
            &self.inputs,
            &self.root.join("notes"),
            "integrate/gaps",
            &crate::cli::surface::Declared::current()?,
            &crate::migration_contract::Bundled,
            &crate::cli::surface::declaration(),
            &self.signers,
        )
        .inspect_err(|error| {
            eprintln!("Publisher refusal: {error}");
            for entry in fs::read_dir(self.root.join("service/calls")).unwrap() {
                let path = entry.unwrap().path();
                for name in ["args", "worker.stderr", "stderr"] {
                    if let Ok(raw) = release::regular(&path.join(name), 65536) {
                        if !raw.is_empty() {
                            eprintln!(
                                "{} {name}: {}",
                                path.display(),
                                String::from_utf8_lossy(&raw)
                            );
                        }
                    }
                }
            }
        })
    }
    fn main(&self) -> String {
        git_at(
            &self.root,
            &[
                "--git-dir",
                self.root.join("authority.git").to_str().unwrap(),
                "rev-parse",
                "main",
            ],
        )
    }
    fn events(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("service/events"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.close().unwrap();
    }
}
fn record(root: &Path, event: &str) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("service/events"))
        .unwrap();
    writeln!(file, "{event}").unwrap();
}
fn service_git(config: &Value, args: &[&str]) -> String {
    let root = Path::new(field(config, "root"));
    let mut argv = vec!["--git-dir", field(config, "bare")];
    argv.extend_from_slice(args);
    git_at(root, &argv)
}
fn release_record_value(config: &Value) -> Value {
    let root = Path::new(field(config, "root"));
    let assets = fs::read_dir(root.join("service/assets"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            object([
                ("name", string(entry.file_name().to_str().unwrap())),
                ("size", Value::Int(entry.metadata().unwrap().len().into())),
            ])
        })
        .collect();
    object([
        ("tag_name", string("v0.0.15")),
        (
            "draft",
            Value::Bool(!root.join("service/published").exists()),
        ),
        ("prerelease", Value::Bool(false)),
        ("target_commitish", string(field(config, "source"))),
        (
            "html_url",
            string("https://github.com/robertelee78/agent-memory/releases/tag/v0.0.15"),
        ),
        ("assets", Value::Array(assets)),
    ])
}
fn tree(config: &Value, oid: &str) -> Value {
    let raw = service_git(config, &["ls-tree", oid]);
    let rows = raw
        .lines()
        .map(|line| {
            let (meta, path) = line.split_once('\t').unwrap();
            let parts = meta.split_whitespace().collect::<Vec<_>>();
            object([
                ("path", string(path)),
                ("mode", string(parts[0])),
                ("type", string(parts[1])),
                ("sha", string(parts[2])),
            ])
        })
        .collect();
    object([
        ("truncated", Value::Bool(false)),
        ("tree", Value::Array(rows)),
    ])
}
#[test]
fn gh_worker() {
    let Some(call) = std::env::var_os("GAPS_PUBLISH_CALL") else {
        return;
    };
    let call = PathBuf::from(call);
    let root = PathBuf::from(std::env::var_os("GAPS_PUBLISH_ROOT").unwrap());
    let config = read(&root.join("service/config.json"));
    let mode = field(&config, "mode");
    let args = String::from_utf8(release::regular(&call.join("args"), 65536).unwrap())
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mut code = 0;
    let mut stderr = String::new();
    let mut body = String::new();
    record(&root, &format!("request:{}", args.join("|")));
    match refs.as_slice() {
        [
            "release",
            "view",
            "v0.0.15",
            "--repo",
            repo,
            "--json",
            "apiUrl",
        ] if *repo == release::REPOSITORY => {
            if !root.join("service/created").exists() {
                code = 1;
                stderr = "release not found\n".into();
            } else {
                body = response(&object([(
                    "apiUrl",
                    string("https://api.github.com/repos/robertelee78/agent-memory/releases/1"),
                )]))
                .unwrap();
            }
        }
        ["api", endpoint, "--jq", projection] => {
            if *endpoint == format!("repos/{}/branches/main", release::REPOSITORY) {
                assert_eq!(*projection, BRANCH_FIELDS);
                let main = service_git(&config, &["rev-parse", "main"]);
                body = response(&object([
                    ("protected", Value::Bool(false)),
                    ("commit", object([("sha", string(main))])),
                ]))
                .unwrap();
            } else if endpoint.ends_with("/rules/branches/main") {
                assert_eq!(
                    *projection,
                    "if type == \"array\" then length == 0 else false end"
                );
                body = "true".into();
            } else if endpoint.contains("/compare/main...") {
                assert_eq!(*projection, ".status");
                let before = service_git(&config, &["rev-parse", "main"]);
                let source = field(&config, "source");
                if mode == "push-race"
                    && root.join("service/published").exists()
                    && !root.join("service/raced").exists()
                {
                    service_git(
                        &config,
                        &["update-ref", "refs/heads/main", field(&config, "foreign")],
                    );
                    save(&root.join("service/raced"), b"raced", 0o600);
                }
                body = if before == source {
                    "identical".into()
                } else {
                    let ancestor = execute(
                        &[
                            "git".into(),
                            "--git-dir".into(),
                            field(&config, "bare").into(),
                            "merge-base".into(),
                            "--is-ancestor".into(),
                            source.into(),
                            before.into(),
                        ],
                        &root,
                    );
                    if ancestor.status == Status::Code(0) {
                        "behind".into()
                    } else {
                        "ahead".into()
                    }
                };
            } else if endpoint.contains("/commits/") {
                assert_eq!(*projection, ".sha");
                body = field(&config, "source").into();
                assert!(service_git(&config, &["cat-file", "-t", &body]) == "commit");
            } else if endpoint.contains("/git/ref/tags/") {
                assert_eq!(*projection, TAG_FIELDS);
                if mode == "tag-before" && !root.join("service/published").exists() {
                    service_git(
                        &config,
                        &["update-ref", "refs/tags/v0.0.15", field(&config, "base")],
                    );
                }
                let out = execute(
                    &[
                        "git".into(),
                        "--git-dir".into(),
                        field(&config, "bare").into(),
                        "rev-parse".into(),
                        "--verify".into(),
                        "refs/tags/v0.0.15".into(),
                    ],
                    &root,
                );
                if out.status != Status::Code(0) {
                    code = 1;
                    stderr = "404 Not Found\n".into();
                } else {
                    body = response(&object([(
                        "object",
                        object([
                            ("type", string("commit")),
                            ("sha", string(String::from_utf8(out.stdout).unwrap().trim())),
                        ]),
                    )]))
                    .unwrap();
                }
            } else if endpoint.contains("/git/trees/") {
                assert_eq!(*projection, TREE_FIELDS);
                body = response(&tree(&config, endpoint.rsplit('/').next().unwrap())).unwrap();
            } else if endpoint.ends_with("/releases/latest") || endpoint.ends_with("/releases/1") {
                assert_eq!(*projection, RELEASE_FIELDS);
                body = response(&release_record_value(&config)).unwrap();
            } else {
                panic!("unrecorded gh api {args:?}");
            }
        }
        [
            "release",
            "create",
            "v0.0.15",
            "--repo",
            repo,
            "--target",
            source,
            "--draft",
            "--title",
            "v0.0.15",
            "--notes-file",
            notes,
        ] => {
            assert_eq!(*repo, release::REPOSITORY);
            assert_eq!(*source, field(&config, "source"));
            assert_eq!(Path::new(notes), root.join("notes"));
            assert!(!root.join("service/created").exists());
            save(&root.join("service/created"), b"draft", 0o600);
            record(&root, "create");
        }
        ["release", "upload", "v0.0.15", asset, "--repo", repo] => {
            assert_eq!(*repo, release::REPOSITORY);
            let assets = root.join("service/assets");
            if mode == "partial-upload"
                && fs::read_dir(&assets).unwrap().count() == 1
                && !root.join("service/interrupted").exists()
            {
                save(&root.join("service/interrupted"), b"partial", 0o600);
                code = 1;
                stderr = "controlled partial upload\n".into();
            } else {
                let source = Path::new(asset);
                let destination = assets.join(source.file_name().unwrap());
                assert!(!destination.exists());
                fs::copy(source, destination).unwrap();
                record(&root, "upload");
            }
        }
        ["release", "download", "v0.0.15", "--repo", repo, tail @ ..] => {
            assert_eq!(*repo, release::REPOSITORY);
            let (pattern, directory) = match *tail {
                ["--dir", directory] => (None, directory),
                ["--pattern", pattern, "--dir", directory] => (Some(pattern), directory),
                _ => panic!("unrecorded download {args:?}"),
            };
            for entry in fs::read_dir(root.join("service/assets")).unwrap() {
                let entry = entry.unwrap();
                if pattern.is_none_or(|pattern| entry.file_name() == pattern) {
                    fs::copy(entry.path(), Path::new(directory).join(entry.file_name())).unwrap();
                }
            }
        }
        [
            "release",
            "edit",
            "v0.0.15",
            "--repo",
            repo,
            "--draft=false",
            "--latest",
        ] => {
            assert_eq!(*repo, release::REPOSITORY);
            assert!(!root.join("service/published").exists());
            let tag = if mode == "tag-after" {
                field(&config, "base")
            } else {
                field(&config, "source")
            };
            service_git(&config, &["update-ref", "refs/tags/v0.0.15", tag]);
            save(&root.join("service/published"), b"published", 0o600);
            record(&root, "publish");
            if ["harmless-descendant", "bootstrap-revert", "runtime-revert"].contains(&mode) {
                service_git(
                    &config,
                    &["update-ref", "refs/heads/main", field(&config, "foreign")],
                );
                let bootstrap = service_git(&config, &["show", "main:install.sh"]);
                save(
                    &root.join("www/robertelee78/agent-memory/main/install.sh"),
                    format!("{bootstrap}\n"),
                    0o644,
                );
            }
        }
        _ => panic!("unrecorded gh request {args:?}"),
    }
    save(&call.join("body"), body, 0o600);
    save(&call.join("stderr"), stderr, 0o600);
    save(&call.join("code"), code.to_string(), 0o600);
}
#[test]
// obligation: tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_one_command_first_publish_exact_resume_and_harmless_descendant
fn one_command_first_publish_exact_resume_and_harmless_descendant() {
    if !isolated(
        "one_command_first_publish_exact_resume_and_harmless_descendant",
        &["normal", "partial-upload", "harmless-descendant"],
    ) {
        return;
    }
    let f = Fixture::new();
    let config = read(&f.root.join("service/config.json"));
    if field(&config, "mode") == "partial-upload" {
        assert!(f.publish().is_err());
        assert!(!f.root.join("service/published").exists());
        assert_eq!(
            fs::read_dir(f.root.join("service/assets")).unwrap().count(),
            1
        );
    }
    let result = f.publish().unwrap();
    assert_eq!(get(&result, "published"), &Value::Bool(true));
    assert_eq!(
        get(&result, "default_activation_performed"),
        &Value::Bool(true)
    );
    let expected = if field(&config, "mode") == "harmless-descendant" {
        field(&config, "foreign")
    } else {
        field(&config, "source")
    };
    assert_eq!(field(&result, "verified_main_commit"), expected);
    assert_eq!(f.main(), expected);
    let events = f.events();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "create")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "publish")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "upload")
            .count(),
        4 + 2 * release::TARGETS.len()
    );
    let assets = output_files(&f.root.join("service/assets")).unwrap();
    let repeated = f.publish().unwrap();
    assert_eq!(repeated, result);
    assert_eq!(
        output_files(&f.root.join("service/assets")).unwrap(),
        assets
    );
    let after = f.events();
    assert_eq!(
        after
            .iter()
            .filter(|event| event.as_str() == "create")
            .count(),
        1
    );
    assert_eq!(
        after
            .iter()
            .filter(|event| event.as_str() == "publish")
            .count(),
        1
    );
    assert_eq!(
        after
            .iter()
            .filter(|event| event.as_str() == "upload")
            .count(),
        4 + 2 * release::TARGETS.len()
    );
}
#[test]
// obligation: tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation
fn wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation() {
    if !isolated(
        "wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation",
        &[
            "preexisting-tag",
            "tag-before",
            "tag-after",
            "bootstrap-revert",
            "runtime-revert",
            "push-race",
        ],
    ) {
        return;
    }
    let f = Fixture::new();
    let config = read(&f.root.join("service/config.json"));
    let result = f.publish();
    assert!(result.is_err(), "must not claim activation: {result:?}");
    let mode = field(&config, "mode");
    let expected = if ["bootstrap-revert", "runtime-revert", "push-race"].contains(&mode) {
        field(&config, "foreign")
    } else {
        field(&config, "base")
    };
    assert_eq!(f.main(), expected);
    if mode == "preexisting-tag" {
        assert!(!f.root.join("service/created").exists());
    } else if mode == "tag-before" {
        assert!(f.root.join("service/created").exists());
        assert!(!f.root.join("service/published").exists());
    } else {
        assert!(f.root.join("service/published").exists());
    }
}
