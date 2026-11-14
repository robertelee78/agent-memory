//! Interpreter-free bootstrap (`install.sh`): the real script driven by
//! `/bin/sh`, `dash` and `bash` in private directories, with a throwaway
//! signing key, a shell script standing in for the signed `am` binary and
//! assets served through `file://` URLs by substituting the script's release
//! constants in a private copy.

use std::fs::{self, DirBuilder};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SCRIPT: &str = include_str!("../install.sh");
const PINNED_KEY_FILE: &str = include_str!("../skills/agent-memory/keys/allowed-signers");
const PRODUCTION_BASE: &str =
    "https://github.com/robertelee78/agent-memory/releases/latest/download/";
const BINARY_NAMESPACE: &str = "agent-memory.release-binary";
const MANIFEST_NAMESPACE: &str = "agent-memory.release";
const SUPPORTED_REPAIR: &str = "No signed am binary exists for this host. Supported: aarch64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl.";
const SIGNATURE_REPAIR: &str = "The downloaded am binary is not signed by the pinned release key. Nothing was run; retry, and report a persistent failure.";
const DOWNLOAD_PREFIX: &str = "Failed downloaded bytes were not accepted for activation. ";
const NETWORK_GUIDANCE: &str = "Retry the same command after checking network, DNS, VPN/proxy access and connection speed to the reported host.";
const SIZE_GUIDANCE: &str = "Check expected release size and local temporary-storage/file-size limits; report a persistent mismatch.";
const MISSING_GUIDANCE: &str = "The release endpoint or asset is unavailable. Check the selected release or report the missing asset.";
const RATE_GUIDANCE: &str = "The server rate-limited this request. Wait for its limit to reset, then retry the same command.";
const TLS_GUIDANCE: &str = "Check the system clock, trusted certificates and any TLS-inspecting proxy configuration, then retry.";
/// Default fake host: Linux x86_64. The stand-in binary is a shell script, so
/// any supported triple runs on the test machine.
const TRIPLE: &str = "x86_64-unknown-linux-musl";
const TOOLS: &[&str] = &[
    "uname",
    "mktemp",
    "rm",
    "chmod",
    "wc",
    "sleep",
    "curl",
    "ssh-keygen",
    "cmp",
    "cat",
    "ls",
    "grep",
];

fn locate(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for directory in ["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
        let candidate = Path::new(directory).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn required(name: &str) -> PathBuf {
    locate(name).unwrap_or_else(|| panic!("APPARATUS: {name} is required by the bootstrap tests"))
}

/// `/bin/sh` always; `dash` and `bash` when present.
fn shells() -> Vec<PathBuf> {
    let mut result = vec![PathBuf::from("/bin/sh")];
    for name in ["dash", "bash"] {
        if let Some(path) = locate(name) {
            result.push(path);
        }
    }
    result
}

fn failure(
    stage: &str,
    reason: &str,
    os: &str,
    arch: &str,
    curl: &str,
    http: &str,
    repair: &str,
) -> String {
    format!(
        "{{\"schema\":\"agent-memory/bootstrap-failure-v1\",\"stage\":\"{stage}\",\"reason\":\"{reason}\",\"os\":\"{os}\",\"arch\":\"{arch}\",\"curl_exit\":{curl},\"http_status\":{http},\"activation_accepted\":false,\"repair\":\"{repair}\"}}\n"
    )
}

fn download_failure(stage: &str, reason: &str, curl: &str, http: &str, guidance: &str) -> String {
    let repair = format!("{DOWNLOAD_PREFIX}{guidance}");
    failure(stage, reason, "linux", "x86_64", curl, http, &repair)
}

fn write_executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn text(raw: &[u8]) -> String {
    String::from_utf8(raw.to_vec()).unwrap()
}

/// Exactly one line `NAME='value'` among the release constants.
fn constant(script: &str, name: &str) -> String {
    let prefix = format!("{name}='");
    let lines: Vec<&str> = script
        .lines()
        .filter(|line| line.starts_with(&prefix))
        .collect();
    assert_eq!(lines.len(), 1, "{name} must be assigned exactly once");
    let value = lines[0].strip_prefix(&prefix).unwrap();
    let value = value
        .strip_suffix('\'')
        .expect("constant ends with a quote");
    assert!(!value.contains('\''));
    value.to_owned()
}

fn substitute(script: &str, name: &str, value: &str) -> String {
    let prefix = format!("{name}='");
    constant(script, name);
    let mut result = String::new();
    for line in script.split_inclusive('\n') {
        if line.starts_with(&prefix) {
            result.push_str(&format!("{name}='{value}'\n"));
        } else {
            result.push_str(line);
        }
    }
    result
}

struct World {
    root: PathBuf,
    tmp: PathBuf,
    home: PathBuf,
    assets: PathBuf,
    tools: PathBuf,
    record: PathBuf,
    key: PathBuf,
    script: PathBuf,
}

impl World {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = fs::canonicalize("/tmp").unwrap();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = loop {
            let candidate = base.join(format!(
                "am-t2-bootstrap-{}-{}-{}",
                std::process::id(),
                nanos,
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match DirBuilder::new().mode(0o700).create(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("Private test directory: {error}"),
            }
        };
        let world = Self {
            tmp: root.join("tmp with spaces"),
            home: root.join("home"),
            assets: root.join("assets"),
            tools: root.join("tools"),
            record: root.join("record"),
            key: root.join("keys/release"),
            script: root.join("bootstrap copy with spaces.sh"),
            root,
        };
        for directory in [
            &world.tmp,
            &world.home,
            &world.assets,
            &world.tools,
            &world.record,
            &world.root.join("keys"),
            &world.root.join("sign"),
        ] {
            DirBuilder::new().mode(0o700).create(directory).unwrap();
        }
        for name in TOOLS {
            symlink(required(name), world.tools.join(name)).unwrap();
        }
        world.fake_uname("Linux", "x86_64");
        world.keygen(&world.key);
        let script = world.script_text(&world.signers_line(&world.key), "=file");
        fs::write(&world.script, script).unwrap();
        world
    }

    fn keygen(&self, path: &Path) {
        let status = Command::new(required("ssh-keygen"))
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "t2-bootstrap", "-f"])
            .arg(path)
            .env("HOME", &self.home)
            .stdin(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn signers_line(&self, key: &Path) -> String {
        let public = fs::read_to_string(key.with_extension("pub")).unwrap();
        let fields: Vec<&str> = public.split_whitespace().take(2).collect();
        format!(
            "release@agent-memory namespaces=\"agent-memory.release,agent-memory.release-binary\" {}",
            fields.join(" ")
        )
    }

    fn base_url(&self) -> String {
        format!("file://{}/", self.assets.display())
    }

    fn script_text(&self, signers: &str, protocols: &str) -> String {
        let script = substitute(SCRIPT, "AM_RELEASE_BASE", &self.base_url());
        let script = substitute(&script, "AM_PROTOCOLS", protocols);
        substitute(&script, "AM_ALLOWED_SIGNERS", signers)
    }

    fn fake_uname(&self, kernel: &str, machine: &str) {
        let path = self.tools.join("uname");
        let _ = fs::remove_file(&path);
        write_executable(
            &path,
            &format!(
                "#!/bin/sh\ncase $1 in\n  -s) printf '%s\\n' '{kernel}' ;;\n  -m) printf '%s\\n' '{machine}' ;;\n  *) exit 1 ;;\nesac\n"
            ),
        );
    }

    fn real_uname(&self) {
        let path = self.tools.join("uname");
        fs::remove_file(&path).unwrap();
        symlink(required("uname"), path).unwrap();
    }

    fn remove_tool(&self, name: &str) {
        fs::remove_file(self.tools.join(name)).unwrap();
    }

    /// Stand-in for the signed binary: records what it was given and exits 7.
    fn binary_text(&self, extra: &str) -> String {
        format!(
            "#!/bin/sh\nrecord='{record}'\nprintf '%s\\n' \"$0\" > \"$record/argv0\"\n: > \"$record/args\"\nfor a in \"$@\"; do printf '[%s]\\n' \"$a\" >> \"$record/args\"; done\nls -ld \"${{0%/*}}\" > \"$record/directory\"\nls -l \"$0\" > \"$record/binary\"\nif cmp -s \"$0\" \"$3\"; then echo same > \"$record/same\"; fi\necho 'fake am ran'\n{extra}\nexit 7\n",
            record = self.record.display()
        )
    }

    fn publish(&self, triple: &str, binary: &str, namespace: &str, key: &Path) {
        let asset = self.assets.join(format!("am-{triple}"));
        fs::write(&asset, binary).unwrap();
        let signed = self.root.join("sign").join(format!("am-{triple}"));
        let _ = fs::remove_file(&signed);
        let _ = fs::remove_file(signed.with_extension("sig"));
        fs::write(&signed, binary).unwrap();
        let status = Command::new(required("ssh-keygen"))
            .args(["-Y", "sign", "-n", namespace, "-f"])
            .arg(key)
            .arg(&signed)
            .env("HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let signature = fs::read(PathBuf::from(format!("{}.sig", signed.display()))).unwrap();
        fs::write(self.assets.join(format!("am-{triple}.sshsig")), signature).unwrap();
    }

    fn publish_default(&self) {
        self.publish(
            TRIPLE,
            &self.binary_text(""),
            BINARY_NAMESPACE,
            &self.key.clone(),
        );
    }

    fn command(&self, shell: &Path) -> Command {
        let mut command = Command::new(shell);
        command
            .env_clear()
            .env("PATH", &self.tools)
            .env("HOME", &self.home)
            .env("TMPDIR", &self.tmp)
            .env("LC_ALL", "C")
            .stdin(Stdio::null());
        command
    }

    fn run(&self, shell: &Path, args: &[&str]) -> Output {
        self.command(shell)
            .arg(&self.script)
            .args(args)
            .output()
            .unwrap()
    }

    /// `curl … | sh -s -- ARGS`: the script arrives on standard input.
    fn pipe(&self, shell: &Path, script: &[u8], args: &[&str]) -> Output {
        let copy = self.root.join("piped-script");
        fs::write(&copy, script).unwrap();
        self.command(shell)
            .args(["-s", "--"])
            .args(args)
            .stdin(fs::File::open(&copy).unwrap())
            .output()
            .unwrap()
    }

    fn record(&self, name: &str) -> Option<String> {
        fs::read_to_string(self.record.join(name)).ok()
    }

    fn reset_record(&self) {
        fs::remove_dir_all(&self.record).unwrap();
        DirBuilder::new().mode(0o700).create(&self.record).unwrap();
    }

    fn assert_clean(&self) {
        let left: Vec<_> = fs::read_dir(&self.tmp).unwrap().collect();
        assert!(left.is_empty(), "temporary directory left behind: {left:?}");
    }

    fn assert_refused(&self, output: &Output, code: i32, expected: &str) {
        assert_eq!(text(&output.stdout), expected, "{}", text(&output.stderr));
        assert_eq!(output.status.code(), Some(code));
        assert!(self.record("argv0").is_none(), "binary must not run");
        self.assert_clean();
    }

    /// Replace curl with a logging wrapper. URLs ending in `suffix` run
    /// `fault` (shell text with `$n` = attempt number, `$out` = output path);
    /// every other URL is forwarded to the real curl.
    fn fake_curl(&self, suffix: &str, fault: &str) -> PathBuf {
        let log = self.root.join("curl.log");
        let path = self.tools.join("curl");
        fs::remove_file(&path).unwrap();
        write_executable(
            &path,
            &format!(
                "#!/bin/sh\nout=\nprev=\nurl=\nfor a in \"$@\"; do\n  if [ \"$prev\" = --output ]; then out=$a; fi\n  prev=$a\n  url=$a\ndone\nprintf '%s\\n' \"$url\" >> '{log}'\ncase $url in\n  *'{suffix}') ;;\n  *) exec '{real}' \"$@\" ;;\nesac\nn=$(grep -c -x -F -e \"$url\" '{log}')\n{fault}\nexec '{real}' \"$@\"\n",
                log = log.display(),
                real = required("curl").display(),
            ),
        );
        log
    }

    fn requests(&self, log: &Path, suffix: &str) -> usize {
        let raw = fs::read_to_string(log).unwrap_or_default();
        raw.lines().filter(|line| line.ends_with(suffix)).count()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn binary_url_suffix() -> String {
    format!("am-{TRIPLE}")
}

fn signature_url_suffix() -> String {
    format!("am-{TRIPLE}.sshsig")
}

#[test]
fn pinned_allowed_signers_equal_the_committed_key_file() {
    let pinned = constant(SCRIPT, "AM_ALLOWED_SIGNERS");
    assert_eq!(format!("{pinned}\n"), PINNED_KEY_FILE);
    assert_eq!(constant(SCRIPT, "AM_RELEASE_BASE"), PRODUCTION_BASE);
    assert_eq!(constant(SCRIPT, "AM_PROTOCOLS"), "=https");
    assert_eq!(constant(SCRIPT, "AM_PRINCIPAL"), "release@agent-memory");
    assert_eq!(constant(SCRIPT, "AM_NAMESPACE"), BINARY_NAMESPACE);
    assert!(pinned.contains("namespaces=\"agent-memory.release,agent-memory.release-binary\""));
}

#[test]
fn script_is_posix_sh_without_interpreters_hashing_or_eval() {
    assert!(SCRIPT.starts_with("#!/bin/sh\n"));
    let code: Vec<&str> = SCRIPT
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect();
    assert_eq!(code.first(), Some(&"if :; then"));
    assert_eq!(code.last(), Some(&"fi"));
    assert!(code.contains(&"set -eu"));
    let lowered = SCRIPT.to_lowercase();
    for forbidden in [
        "python",
        "perl",
        "eval",
        "awk",
        "sed -",
        "sha256sum",
        "shasum",
        "openssl",
        "jq",
        "bash",
        "pipefail",
        "\n  local ",
        "\nlocal ",
        "[[",
    ] {
        assert!(!lowered.contains(forbidden), "{forbidden}");
    }
    for shell in shells() {
        let output = Command::new(&shell)
            .arg("-n")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text(&output.stderr));
    }
}

// obligation: tests/contracts/test_installer.py::OneLineBootstrapContract.test_default_one_line_install_ignores_caller_checkout_and_keeps_runtime_after_cleanup
#[test]
fn verified_binary_runs_with_its_own_path_and_user_arguments_then_scratch_is_removed() {
    for shell in shells() {
        let world = World::new();
        world.publish_default();
        let args = [
            "--json",
            "--target-dir",
            "custom skills with spaces/$HOME;`x`",
            "--mode=copy",
            "--presets",
            "none",
        ];
        let output = world.run(&shell, &args);
        assert_eq!(output.status.code(), Some(7), "{}", text(&output.stderr));
        assert_eq!(text(&output.stdout), "fake am ran\n");
        assert_eq!(text(&output.stderr), "");
        let argv0 = world.record("argv0").unwrap();
        let argv0 = argv0.trim_end_matches('\n');
        let scratch = format!("{}/am-bootstrap.", world.tmp.display());
        assert!(argv0.starts_with(&scratch), "{argv0}");
        assert!(argv0.ends_with(&format!("/am-{TRIPLE}")), "{argv0}");
        let expected = format!(
            "[install]\n[--bootstrap-binary]\n[{argv0}]\n[--json]\n[--target-dir]\n[custom skills with spaces/$HOME;`x`]\n[--mode=copy]\n[--presets]\n[none]\n"
        );
        assert_eq!(world.record("args").unwrap(), expected);
        assert_eq!(world.record("same").unwrap(), "same\n");
        assert!(world.record("directory").unwrap().starts_with("drwx------"));
        assert!(world.record("binary").unwrap().starts_with("-rwx------"));
        world.assert_clean();
    }
}

// obligation: tests/contracts/test_installer.py::MacOSCommandContract.test_aliased_tmpdir_pipe_preview_install_update_and_both_commands
#[test]
fn piped_script_runs_the_verified_binary_and_cleans_scratch() {
    for shell in shells() {
        let world = World::new();
        world.publish_default();
        let script = fs::read(&world.script).unwrap();
        let output = world.pipe(&shell, &script, &["--json", "--check"]);
        assert_eq!(output.status.code(), Some(7), "{}", text(&output.stderr));
        assert_eq!(text(&output.stdout), "fake am ran\n");
        let argv0 = world.record("argv0").unwrap();
        let expected = format!(
            "[install]\n[--bootstrap-binary]\n[{}]\n[--json]\n[--check]\n",
            argv0.trim_end_matches('\n')
        );
        assert_eq!(world.record("args").unwrap(), expected);
        world.assert_clean();
    }
}

#[test]
fn truncated_script_executes_no_prefix() {
    let world = World::new();
    world.publish_default();
    let script = fs::read(&world.script).unwrap();
    let mut cuts: Vec<usize> = script
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'\n')
        .map(|(index, _)| index + 1)
        .step_by(5)
        .collect();
    cuts.extend([script.len() / 3, script.len() / 2, script.len() - 2]);
    for shell in shells() {
        for cut in &cuts {
            if *cut >= script.len() - 1 {
                continue;
            }
            let output = world.pipe(&shell, &script[..*cut], &["--json"]);
            // A prefix holding only comments runs nothing and exits 0; any
            // prefix reaching the program is an unterminated `if`.
            assert_ne!(output.status.code(), Some(7), "cut {cut}");
            assert_eq!(text(&output.stdout), "", "cut {cut}");
            if text(&script[..*cut]).contains("\nif :; then\n") {
                assert_ne!(output.status.code(), Some(0), "cut {cut}");
            }
            assert!(world.record("argv0").is_none());
            world.assert_clean();
        }
    }
}

#[test]
fn every_supported_host_selects_its_own_signed_binary() {
    let hosts = [
        ("Darwin", "arm64", "aarch64-apple-darwin"),
        ("Linux", "x86_64", "x86_64-unknown-linux-musl"),
        ("Linux", "aarch64", "aarch64-unknown-linux-musl"),
        ("Linux", "arm64", "aarch64-unknown-linux-musl"),
    ];
    let world = World::new();
    for (_, _, triple) in hosts {
        world.publish(triple, &world.binary_text(""), BINARY_NAMESPACE, &world.key);
    }
    for shell in shells() {
        for (kernel, machine, triple) in hosts {
            world.reset_record();
            world.fake_uname(kernel, machine);
            let output = world.run(&shell, &[]);
            assert_eq!(output.status.code(), Some(7), "{kernel} {machine}");
            let argv0 = world.record("argv0").unwrap();
            assert!(argv0.ends_with(&format!("/am-{triple}\n")), "{argv0}");
            world.assert_clean();
        }
    }
}

#[test]
fn real_uname_selects_the_host_triple() {
    let triple = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        _ => return,
    };
    let world = World::new();
    world.real_uname();
    world.publish(triple, &world.binary_text(""), BINARY_NAMESPACE, &world.key);
    let output = world.run(Path::new("/bin/sh"), &[]);
    assert_eq!(output.status.code(), Some(7), "{}", text(&output.stdout));
    let argv0 = world.record("argv0").unwrap();
    assert!(argv0.ends_with(&format!("/am-{triple}\n")), "{argv0}");
}

#[test]
fn unsupported_target_refuses_with_exact_constant_json() {
    let cases = [
        ("Darwin", "x86_64", "darwin", "x86_64"),
        ("Linux", "riscv64", "linux", "riscv64"),
        ("FreeBSD", "amd64", "freebsd", "x86_64"),
        ("Linux", "i686", "linux", "x86"),
        ("Darwin", "ppc", "darwin", "other"),
        ("MINGW64_NT-10.0", "x86_64", "windows", "x86_64"),
        ("SunOS", "sun4v", "sunos", "other"),
        ("Linux\"; echo injected", "x86_64\\u0000", "other", "other"),
    ];
    let world = World::new();
    world.publish_default();
    for shell in shells() {
        for (kernel, machine, os, arch) in cases {
            world.fake_uname(kernel, machine);
            let output = world.run(&shell, &["--json"]);
            let expected = failure(
                "target",
                "unsupported-target",
                os,
                arch,
                "null",
                "null",
                SUPPORTED_REPAIR,
            );
            world.assert_refused(&output, 1, &expected);
        }
    }
    world.fake_uname("Linux", "riscv64");
    let output = world.run(Path::new("/bin/sh"), &[]);
    assert_eq!(
        output.stdout,
        b"{\"schema\":\"agent-memory/bootstrap-failure-v1\",\"stage\":\"target\",\"reason\":\"unsupported-target\",\"os\":\"linux\",\"arch\":\"riscv64\",\"curl_exit\":null,\"http_status\":null,\"activation_accepted\":false,\"repair\":\"No signed am binary exists for this host. Supported: aarch64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl.\"}\n"
    );
    assert_eq!(output.stderr, b"");
}

#[test]
fn missing_curl_refuses_before_any_scratch() {
    let world = World::new();
    world.publish_default();
    world.remove_tool("curl");
    for shell in shells() {
        let output = world.run(&shell, &[]);
        let expected = "{\"schema\":\"agent-memory/bootstrap-failure-v1\",\"stage\":\"dependency\",\"reason\":\"missing-curl\",\"os\":\"linux\",\"arch\":\"x86_64\",\"curl_exit\":null,\"http_status\":null,\"activation_accepted\":false,\"repair\":\"curl is required to download the signed am binary. Install curl, then retry.\"}\n";
        world.assert_refused(&output, 1, expected);
    }
}

#[test]
fn missing_ssh_keygen_refuses_before_any_download() {
    let world = World::new();
    world.publish_default();
    world.remove_tool("ssh-keygen");
    let log = world.fake_curl("", "");
    for shell in shells() {
        let output = world.run(&shell, &[]);
        let expected = "{\"schema\":\"agent-memory/bootstrap-failure-v1\",\"stage\":\"dependency\",\"reason\":\"missing-ssh-keygen\",\"os\":\"linux\",\"arch\":\"x86_64\",\"curl_exit\":null,\"http_status\":null,\"activation_accepted\":false,\"repair\":\"OpenSSH ssh-keygen with SSH signature support is required. Install OpenSSH, then retry.\"}\n";
        world.assert_refused(&output, 1, expected);
    }
    assert_eq!(world.requests(&log, ""), 0);
}

#[test]
fn unusable_temporary_directory_refuses() {
    let world = World::new();
    world.publish_default();
    for shell in shells() {
        let output = world
            .command(&shell)
            .env("TMPDIR", world.root.join("missing parent"))
            .arg(&world.script)
            .output()
            .unwrap();
        let expected = failure(
            "workspace",
            "temporary-directory",
            "linux",
            "x86_64",
            "null",
            "null",
            "Could not create a private temporary directory. Check TMPDIR space and permissions.",
        );
        world.assert_refused(&output, 1, &expected);
    }
}

// obligation: tests/contracts/test_installer.py::OneLineBootstrapContract.test_help_and_invalid_options_need_no_download_and_old_python_is_rejected
#[test]
fn help_and_invalid_options_need_no_download_or_dependencies() {
    let world = World::new();
    world.publish_default();
    let log = world.fake_curl("", "");
    for shell in shells() {
        for help in ["--help", "-h"] {
            let output = world.run(&shell, &["--json", help, "--unknown"]);
            assert_eq!(output.status.code(), Some(0));
            let stdout = text(&output.stdout);
            assert!(stdout.contains("am update"), "{stdout}");
            assert!(stdout.contains("--development-source PATH"), "{stdout}");
            assert_eq!(text(&output.stderr), "");
            world.assert_clean();
        }
        let usage = |reason: &str, repair: &str| {
            failure("usage", reason, "linux", "x86_64", "null", "null", repair)
        };
        let unknown = usage(
            "unknown-option",
            "Unknown install option. Run install.sh --help.",
        );
        let missing = usage(
            "missing-value",
            "An option needs a value. Run install.sh --help.",
        );
        let conflict = usage(
            "conflicting-sources",
            "Choose development source or managed remote/ref, not both.",
        );
        let cases: [(&[&str], &String); 9] = [
            (&["--unknown"], &unknown),
            (&["--check=yes"], &unknown),
            (&["--bootstrap-binary", "/tmp/x"], &unknown),
            (&["--remote="], &missing),
            (&["--ref"], &missing),
            (&["--mode", "--json"], &missing),
            (&["--presets="], &missing),
            (&["--development-source", "a", "--remote", "b"], &conflict),
            (&["--ref=main", "--development-source=a"], &conflict),
        ];
        for (args, expected) in cases {
            let output = world.run(&shell, args);
            world.assert_refused(&output, 2, expected);
        }
        world.fake_uname("Linux", "riscv64");
        let output = world.run(&shell, &["--help"]);
        assert_eq!(output.status.code(), Some(0));
        world.fake_uname("Linux", "x86_64");
    }
    world.remove_tool("curl");
    world.remove_tool("ssh-keygen");
    let output = world.run(Path::new("/bin/sh"), &["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(world.requests(&log, ""), 0);
}

// obligation: tests/contracts/test_installer.py::OneLineBootstrapContract.test_standalone_copy_forwards_equals_options_and_checkout_entry_still_works
#[test]
fn every_baseline_option_is_forwarded_verbatim() {
    let world = World::new();
    world.publish_default();
    let args = [
        "--json",
        "--check",
        "--dry-run",
        "--no-guidance",
        "--verbose",
        "-v",
        "--omp-peers-upgraded",
        "--remote=file:///x y",
        "--ref",
        "main",
        "--mode",
        "link",
        "--presets=codex,claude",
        "--target-dir",
        "-dash-leading value",
        "--bin-dir=/b",
        "--omp-agent-dir",
        "/o",
        "--omp-layout-quiescence=q",
        "--layout-quiescence",
        "r",
    ];
    for shell in shells() {
        world.reset_record();
        let output = world.run(&shell, &args);
        assert_eq!(output.status.code(), Some(7), "{}", text(&output.stdout));
        let recorded = world.record("args").unwrap();
        let mut expected = String::new();
        for arg in args {
            expected.push_str(&format!("[{arg}]\n"));
        }
        assert!(recorded.ends_with(&expected), "{recorded}");
        world.reset_record();
        let output = world.run(&shell, &["--development-source", "/source", "--json"]);
        assert_eq!(output.status.code(), Some(7));
        assert!(
            world
                .record("args")
                .unwrap()
                .ends_with("[--development-source]\n[/source]\n[--json]\n")
        );
    }
}

// obligation: tests/contracts/test_installer.py::OneLineBootstrapContract.test_clone_and_install_failures_propagate_and_clean_scratch
#[test]
fn binary_exit_status_propagates_and_scratch_is_removed() {
    for shell in shells() {
        let world = World::new();
        world.publish(
            TRIPLE,
            &world.binary_text("exit 0"),
            BINARY_NAMESPACE,
            &world.key,
        );
        assert_eq!(world.run(&shell, &[]).status.code(), Some(0));
        world.assert_clean();
        world.publish(
            TRIPLE,
            &world.binary_text("exit 128"),
            BINARY_NAMESPACE,
            &world.key,
        );
        assert_eq!(world.run(&shell, &[]).status.code(), Some(128));
        world.assert_clean();
    }
}

#[test]
fn termination_while_the_binary_runs_still_removes_scratch() {
    for shell in shells() {
        let world = World::new();
        world.publish(
            TRIPLE,
            &world.binary_text("kill -TERM \"$PPID\"\nexit 0"),
            BINARY_NAMESPACE,
            &world.key,
        );
        let output = world.run(&shell, &[]);
        assert_eq!(output.status.code(), Some(143), "{}", text(&output.stderr));
        assert!(world.record("argv0").is_some());
        world.assert_clean();
    }
}

#[test]
fn hangup_while_the_binary_runs_still_removes_scratch() {
    for shell in shells() {
        let world = World::new();
        world.publish(
            TRIPLE,
            &world.binary_text("kill -HUP \"$PPID\"\nexit 0"),
            BINARY_NAMESPACE,
            &world.key,
        );
        let output = world.run(&shell, &[]);
        assert_eq!(output.status.code(), Some(129), "{}", text(&output.stderr));
        assert!(world.record("argv0").is_some());
        world.assert_clean();
    }
}

#[test]
fn interrupt_while_the_binary_runs_still_removes_scratch() {
    for shell in shells() {
        let world = World::new();
        world.publish(
            TRIPLE,
            &world.binary_text("kill -INT \"$PPID\"\nexit 0"),
            BINARY_NAMESPACE,
            &world.key,
        );
        let output = world.run(&shell, &[]);
        assert_eq!(output.status.code(), Some(130), "{}", text(&output.stderr));
        assert!(world.record("argv0").is_some());
        world.assert_clean();
    }
}

#[test]
fn signals_during_mktemp_cannot_leak_an_unassigned_scratch_directory() {
    for shell in shells() {
        for signal in ["HUP", "INT", "TERM"] {
            let world = World::new();
            world.publish_default();
            world.remove_tool("mktemp");
            write_executable(
                &world.tools.join("mktemp"),
                &format!(
                    "#!/bin/sh\n'{mktemp}' \"$@\"\nkill -{signal} \"$PPID\"\n",
                    mktemp = required("mktemp").display()
                ),
            );
            let output = world.run(&shell, &[]);
            assert_eq!(
                output.status.code(),
                Some(7),
                "{}: {signal}: {}",
                shell.display(),
                text(&output.stderr)
            );
            assert!(world.record("argv0").is_some());
            world.assert_clean();
        }
    }
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadBootstrap.test_failed_download_never_activates_and_has_guidance
#[test]
fn missing_assets_fail_as_downloads_without_running_anything() {
    for shell in shells() {
        let world = World::new();
        let output = world.run(&shell, &[]);
        let expected = download_failure(
            "binary-download",
            "transport failure",
            "37",
            "null",
            NETWORK_GUIDANCE,
        );
        world.assert_refused(&output, 1, &expected);
        world.publish_default();
        fs::remove_file(world.assets.join(signature_url_suffix())).unwrap();
        let output = world.run(&shell, &[]);
        let expected = download_failure(
            "signature-download",
            "transport failure",
            "37",
            "null",
            NETWORK_GUIDANCE,
        );
        world.assert_refused(&output, 1, &expected);
    }
}

#[test]
fn shipped_protocol_restriction_refuses_non_https_and_environment_cannot_redirect() {
    let world = World::new();
    world.publish_default();
    let https_only = world.script_text(&world.signers_line(&world.key), "=https");
    fs::write(&world.script, https_only).unwrap();
    for shell in shells() {
        let output = world.run(&shell, &[]);
        let expected = download_failure(
            "binary-download",
            "transport failure",
            "1",
            "null",
            NETWORK_GUIDANCE,
        );
        world.assert_refused(&output, 1, &expected);
    }
    fs::write(&world.script, SCRIPT).unwrap();
    let log = world.fake_curl("", "printf 404\nexit 22");
    for shell in shells() {
        let output = world
            .command(&shell)
            .env("AM_RELEASE_BASE", world.base_url())
            .env("AM_PROTOCOLS", "=file")
            .env("AM_ALLOWED_SIGNERS", world.signers_line(&world.key))
            .arg(&world.script)
            .output()
            .unwrap();
        let expected = download_failure(
            "binary-download",
            "HTTP rejection",
            "22",
            "404",
            MISSING_GUIDANCE,
        );
        world.assert_refused(&output, 1, &expected);
    }
    let requested = fs::read_to_string(&log).unwrap();
    for line in requested.lines() {
        assert_eq!(line, format!("{PRODUCTION_BASE}am-{TRIPLE}"));
    }
    assert_eq!(requested.lines().count(), shells().len());
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadBootstrap.test_invalid_signature_is_not_retried
#[test]
fn signature_from_another_key_is_refused_once_without_running() {
    for shell in shells() {
        let world = World::new();
        let other = world.root.join("keys/other");
        world.keygen(&other);
        world.publish(TRIPLE, &world.binary_text(""), BINARY_NAMESPACE, &other);
        let log = world.fake_curl("never-faulted", "");
        let output = world.run(&shell, &[]);
        let expected = failure(
            "signature-verification",
            "invalid-signature",
            "linux",
            "x86_64",
            "null",
            "null",
            SIGNATURE_REPAIR,
        );
        world.assert_refused(&output, 1, &expected);
        assert_eq!(world.requests(&log, &signature_url_suffix()), 1);
        assert_eq!(world.requests(&log, &binary_url_suffix()), 1);
    }
}

#[test]
fn manifest_namespace_signature_is_refused_for_the_binary() {
    for shell in shells() {
        let world = World::new();
        world.publish(
            TRIPLE,
            &world.binary_text(""),
            MANIFEST_NAMESPACE,
            &world.key,
        );
        let output = world.run(&shell, &[]);
        let expected = failure(
            "signature-verification",
            "invalid-signature",
            "linux",
            "x86_64",
            "null",
            "null",
            SIGNATURE_REPAIR,
        );
        world.assert_refused(&output, 1, &expected);
    }
}

#[test]
fn truncated_or_altered_binary_is_refused() {
    for shell in shells() {
        let world = World::new();
        world.publish_default();
        let asset = world.assets.join(binary_url_suffix());
        let complete = fs::read(&asset).unwrap();
        let expected = failure(
            "signature-verification",
            "invalid-signature",
            "linux",
            "x86_64",
            "null",
            "null",
            SIGNATURE_REPAIR,
        );
        fs::write(&asset, &complete[..complete.len() / 2]).unwrap();
        world.assert_refused(&world.run(&shell, &[]), 1, &expected);
        fs::write(&asset, []).unwrap();
        world.assert_refused(&world.run(&shell, &[]), 1, &expected);
        let mut altered = complete.clone();
        altered.extend_from_slice(b"echo appended\n");
        fs::write(&asset, &altered).unwrap();
        world.assert_refused(&world.run(&shell, &[]), 1, &expected);
        let signature = world.assets.join(signature_url_suffix());
        fs::write(&asset, &complete).unwrap();
        let raw = fs::read(&signature).unwrap();
        fs::write(&signature, &raw[..raw.len() / 2]).unwrap();
        world.assert_refused(&world.run(&shell, &[]), 1, &expected);
    }
}

#[test]
fn oversized_signature_is_bounded() {
    for shell in shells() {
        let world = World::new();
        world.publish_default();
        fs::write(world.assets.join(signature_url_suffix()), vec![b'x'; 5000]).unwrap();
        let output = world.run(&shell, &[]);
        let expected = download_failure(
            "signature-download",
            "size-limit",
            "63",
            "null",
            SIZE_GUIDANCE,
        );
        world.assert_refused(&output, 1, &expected);
    }
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_no_retry_for_permanent_errors
#[test]
fn permanent_download_failures_are_not_retried() {
    let cases: [(&str, &str, &str, &str, &str, &str); 5] = [
        (
            "printf 503\nexit 60",
            "binary",
            "certificate validation failure",
            "60",
            "503",
            TLS_GUIDANCE,
        ),
        (
            "printf 404\nexit 22",
            "binary",
            "HTTP rejection",
            "22",
            "404",
            MISSING_GUIDANCE,
        ),
        (
            "printf 429\nexit 22",
            "binary",
            "HTTP rejection",
            "22",
            "429",
            RATE_GUIDANCE,
        ),
        (
            "printf 503bad\nexit 22",
            "binary",
            "HTTP rejection",
            "22",
            "null",
            NETWORK_GUIDANCE,
        ),
        (
            "head=$(printf '%5000s' x)\nprintf '%s' \"$head\" > \"$out\"\nprintf 200\nexit 0",
            "signature",
            "size-limit",
            "0",
            "200",
            SIZE_GUIDANCE,
        ),
    ];
    for shell in shells() {
        for (fault, asset, reason, curl, http, guidance) in cases {
            let world = World::new();
            world.publish_default();
            let (suffix, stage) = if asset == "binary" {
                (binary_url_suffix(), "binary-download")
            } else {
                (signature_url_suffix(), "signature-download")
            };
            let log = world.fake_curl(&suffix, fault);
            let output = world.run(&shell, &[]);
            let expected = download_failure(stage, reason, curl, http, guidance);
            world.assert_refused(&output, 1, &expected);
            assert_eq!(world.requests(&log, &suffix), 1, "{fault}");
        }
    }
}

// obligation: tests/contracts/test_download_diagnostics.py::ReviewRegressions.test_http_permanent_status_wins_over_transient_curl_exit
#[test]
fn permanent_http_status_wins_over_transient_curl_exit() {
    for (status, guidance) in [
        ("429", RATE_GUIDANCE),
        (
            "403",
            "Check server access restrictions, proxy policy and GitHub API rate limits, then retry.",
        ),
        ("404", MISSING_GUIDANCE),
    ] {
        let world = World::new();
        world.publish_default();
        let log = world.fake_curl(&binary_url_suffix(), &format!("printf {status}\nexit 56"));
        let output = world.run(Path::new("/bin/sh"), &[]);
        let expected =
            download_failure("binary-download", "receive failure", "56", status, guidance);
        world.assert_refused(&output, 1, &expected);
        assert_eq!(world.requests(&log, &binary_url_suffix()), 1);
    }
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_resource_signal_not_retried
#[test]
fn file_size_signal_is_a_size_limit_and_not_retried() {
    for shell in shells() {
        let world = World::new();
        world.publish_default();
        let log = world.fake_curl(&binary_url_suffix(), "kill -XFSZ $$");
        let output = world.run(&shell, &[]);
        let expected = download_failure(
            "binary-download",
            "size-limit",
            "153",
            "null",
            SIZE_GUIDANCE,
        );
        world.assert_refused(&output, 1, &expected);
        assert_eq!(world.requests(&log, &binary_url_suffix()), 1);
    }
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_recovery_discards_partial_bytes
// obligation: tests/contracts/test_download_diagnostics.py::DownloadBootstrap.test_transient_manifest_failure_recovers_to_authenticated_install
#[test]
fn transient_failure_discards_partial_bytes_and_recovers() {
    let world = World::new();
    world.publish_default();
    let log = world.fake_curl(
        &binary_url_suffix(),
        "if [ \"$n\" -eq 1 ]; then\n  printf partial > \"$out\"\n  printf 000\n  exit 56\nfi",
    );
    let output = world.run(Path::new("/bin/sh"), &[]);
    assert_eq!(output.status.code(), Some(7), "{}", text(&output.stdout));
    assert_eq!(world.record("same").unwrap(), "same\n");
    assert_eq!(world.requests(&log, &binary_url_suffix()), 2);
    world.assert_clean();
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_http_retry_and_deadline
#[test]
fn retryable_http_status_is_retried_and_attempts_are_bounded() {
    let world = World::new();
    world.publish_default();
    let log = world.fake_curl(
        &binary_url_suffix(),
        "if [ \"$n\" -eq 1 ]; then\n  printf 503\n  exit 22\nfi",
    );
    let output = world.run(Path::new("/bin/sh"), &[]);
    assert_eq!(output.status.code(), Some(7), "{}", text(&output.stdout));
    assert_eq!(world.requests(&log, &binary_url_suffix()), 2);
    world.assert_clean();

    let world = World::new();
    world.publish_default();
    let log = world.fake_curl(
        &binary_url_suffix(),
        "printf partial > \"$out\"\nprintf 000\nexit 56",
    );
    let output = world.run(Path::new("/bin/sh"), &[]);
    let expected = download_failure(
        "binary-download",
        "receive failure",
        "56",
        "null",
        NETWORK_GUIDANCE,
    );
    world.assert_refused(&output, 1, &expected);
    assert_eq!(world.requests(&log, &binary_url_suffix()), 3);
}
