//! `am-dev release build|publish`: package and sign a release-v2.
//!
//! build:   am-dev release build --private-key KEY --binary TRIPLE=PATH (x3) --output DIR [--root PATH]
//! publish: am-dev release publish ... --output DIR --notes-file NOTES
//!          (build, then `gh release create vX.Y.Z` with every asset)
use agent_memory::bail;
use agent_memory::install::release::{self, BinaryIdentity, Manifest};
use agent_memory::json::{self, Value};
use agent_memory::util::{Result, io_at, run, sha256_hex};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    let output = run("git", &args, Some(root), &[], None)?;
    if !output.ok() {
        bail!("git failed: {}", output.error_text());
    }
    Ok(output.stdout)
}

fn sign(key: &Path, path: &Path, namespace: &str) -> Result<()> {
    let output = run(
        "ssh-keygen",
        &[
            OsStr::new("-Y"),
            OsStr::new("sign"),
            OsStr::new("-n"),
            OsStr::new(namespace),
            OsStr::new("-f"),
            key.as_os_str(),
            path.as_os_str(),
        ],
        None,
        &[],
        None,
    )?;
    if !output.ok() {
        bail!("ssh-keygen sign failed: {}", output.error_text());
    }
    let signed = PathBuf::from(format!("{}.sig", path.display()));
    let signature = io_at(std::fs::read(&signed), &signed)?;
    io_at(
        std::fs::rename(&signed, format!("{}.sshsig", path.display())),
        &signed,
    )?;
    let limit = if namespace == release::BINARY_NAMESPACE {
        release::MAX_BINARY
    } else {
        release::MAX_MANIFEST
    };
    let raw = io_at(std::fs::read(path), path)?;
    if raw.len() > limit {
        bail!("{} exceeds its release bound", path.display());
    }
    release::verify_signature(&raw, &signature, namespace, release::ALLOWED_SIGNERS)
}

fn build(
    root: &Path,
    key: &Path,
    binaries: &BTreeMap<String, PathBuf>,
    output: &Path,
) -> Result<Manifest> {
    if !String::from_utf8_lossy(&git(
        root,
        &["status", "--porcelain", "--untracked-files=all"],
    )?)
    .trim()
    .is_empty()
    {
        bail!("Release source must be clean");
    }
    if binaries.keys().map(String::as_str).collect::<Vec<_>>() != {
        let mut t = release::TARGETS.to_vec();
        t.sort_unstable();
        t
    } {
        bail!(
            "Exactly one --binary per target is required: {}",
            release::TARGETS.join(", ")
        );
    }
    let commit = String::from_utf8_lossy(&git(root, &["rev-parse", "HEAD"])?)
        .trim()
        .to_owned();
    // The pack is exactly the tracked files at HEAD.
    let listing = git(root, &["ls-tree", "-rz", &commit, "--", release::PACK])?;
    let mut files = release::Inventory::new();
    for row in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = String::from_utf8_lossy(row);
        let (meta, name) = row.split_once('\t').unwrap_or(("", ""));
        let parts: Vec<&str> = meta.split(' ').collect();
        let [mode, "blob", blob] = parts.as_slice() else {
            bail!("Unsupported tracked release entry: {row}");
        };
        let relative = name
            .strip_prefix(&format!("{}/", release::PACK))
            .unwrap_or(name);
        let mode = if *mode == "100755" { 0o755 } else { 0o644 };
        files.insert(
            relative.to_owned(),
            (git(root, &["cat-file", "blob", blob])?, mode),
        );
    }
    let (version, artifact_digest, inventory) = release::package_identity(&files)?;
    if version != release::VERSION {
        bail!(
            "SKILL.md declares {version} but this am is {}",
            release::VERSION
        );
    }
    io_at(std::fs::create_dir_all(output), output)?;
    let archive = release::encode_archive(&files)?;
    io_at(
        std::fs::write(output.join(release::ARCHIVE), &archive),
        output,
    )?;
    let mut targets = BTreeMap::new();
    for (triple, path) in binaries {
        let raw = io_at(std::fs::read(path), path)?;
        let sha = sha256_hex(&raw);
        targets.insert(
            triple.clone(),
            BinaryIdentity {
                size: raw.len(),
                artifact_digest: release::runtime_digest(&inventory, &sha),
                sha256: sha,
            },
        );
        let asset = output.join(format!("am-{triple}"));
        io_at(std::fs::write(&asset, &raw), &asset)?;
        sign(key, &asset, release::BINARY_NAMESPACE)?;
    }
    let commands = [
        "status",
        "doctor",
        "init",
        "recall",
        "remember",
        "share",
        "refresh",
        "install",
        "update",
        "uninstall",
        "hook",
    ];
    let manifest = Manifest {
        tag: format!("v{version}"),
        version,
        source_commit: commit.clone(),
        archive_size: archive.len(),
        archive_sha256: sha256_hex(&archive),
        artifact_digest,
        targets,
        command_surface: json::obj([("commands", json::strings(commands))]),
    };
    let path = output.join(release::MANIFEST);
    io_at(std::fs::write(&path, manifest.encoded()), &path)?;
    sign(key, &path, release::NAMESPACE)?;
    let bootstrap = git(root, &["show", &format!("{commit}:install.sh")])?;
    io_at(std::fs::write(output.join("install.sh"), bootstrap), output)?;
    // Read everything back as an installer would.
    let signature = io_at(std::fs::read(output.join(release::SIGNATURE)), output)?;
    let verified = release::verify_manifest(
        &manifest.encoded(),
        &signature,
        Some(&manifest.tag),
        release::ALLOWED_SIGNERS,
    )?;
    let scratch = release::Scratch::new("am-release-check-")?;
    release::extract(&archive, &scratch.0, &verified)?;
    Ok(verified)
}

fn main() {
    let result = (|| -> Result<Value> {
        let mut args = std::env::args().skip(1);
        if args.next().as_deref() != Some("release") {
            bail!(
                "Usage: am-dev release build|publish --private-key KEY --binary TRIPLE=PATH (x3) --output DIR [--root PATH] [--notes-file NOTES]"
            );
        }
        let operation = args.next().unwrap_or_default();
        let mut options = BTreeMap::new();
        let mut binaries = BTreeMap::new();
        while let Some(key) = args.next() {
            let Some(value) = args.next() else {
                bail!("{key} needs a value")
            };
            if key == "--binary" {
                let Some((triple, path)) = value.split_once('=') else {
                    bail!("--binary needs TRIPLE=PATH")
                };
                binaries.insert(triple.to_owned(), PathBuf::from(path));
            } else if ["--root", "--private-key", "--output", "--notes-file"]
                .contains(&key.as_str())
            {
                options.insert(key, value);
            } else {
                bail!("Unknown option {key}");
            }
        }
        let get = |key: &str| options.get(key).map(PathBuf::from);
        let root = get("--root").unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let (Some(key), Some(output)) = (get("--private-key"), get("--output")) else {
            bail!("--private-key and --output are required");
        };
        let manifest = build(&root, &key, &binaries, &output)?;
        if operation == "publish" {
            let Some(notes) = get("--notes-file") else {
                bail!("publish needs --notes-file")
            };
            let mut argv: Vec<PathBuf> = [
                "release",
                "create",
                &manifest.tag,
                "--target",
                &manifest.source_commit,
                "--title",
                &manifest.tag,
                "--notes-file",
            ]
            .iter()
            .map(PathBuf::from)
            .collect();
            argv.push(notes);
            for entry in io_at(std::fs::read_dir(&output), &output)? {
                argv.push(entry?.path());
            }
            let argv: Vec<&OsStr> = argv.iter().map(|p| p.as_os_str()).collect();
            let published = run("gh", &argv, Some(&root), &[], None)?;
            if !published.ok() {
                bail!("gh release create failed: {}", published.error_text());
            }
        } else if operation != "build" {
            bail!("Unknown release operation {operation}");
        }
        Ok(manifest.value())
    })();
    match result {
        Ok(value) => println!("{}", json::encode(&value, json::PRETTY)),
        Err(error) => {
            eprintln!("am-dev: {error}");
            std::process::exit(1);
        }
    }
}
