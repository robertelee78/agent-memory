//! Developer-only publisher. Every remote operation is an explicit supervised gh invocation.
use crate::codec::json::Value;
use crate::error::ContractError;
use crate::fs::{Descriptor, DirectoryChain};
use crate::process::Status;
use crate::release_distribution::Result;
use crate::release_distribution::{self as release, BinaryIdentity, ReleaseManifest, require};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn object(pairs: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}
const RAW_DEFAULT: &str =
    "https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh";
pub struct BuildInputs {
    pub root: PathBuf,
    pub key: PathBuf,
    pub binaries: BTreeMap<String, PathBuf>,
}

#[derive(Clone)]
pub struct Providers {
    pub surface: std::sync::Arc<dyn crate::pack_validation::CommandSurface>,
    pub migration: std::sync::Arc<dyn crate::artifact::MigrationContract>,
    pub command_surface: Value,
}
static PROVIDERS: std::sync::RwLock<Option<Providers>> = std::sync::RwLock::new(None);
pub fn register_providers(providers: Option<Providers>) -> Result<()> {
    if let Some(providers) = &providers {
        require(
            providers.command_surface.as_object().is_some(),
            "Release command surface must be an object",
        )?;
        require(
            release::canonical(&providers.command_surface)?.len() <= release::MAX_METADATA,
            "Release metadata exceeds its bound",
        )?;
    }
    *PROVIDERS
        .write()
        .map_err(|_| ContractError::new("Release provider registry is poisoned"))? = providers;
    Ok(())
}
pub fn registered_providers() -> Result<Providers> {
    PROVIDERS
        .read()
        .map_err(|_| ContractError::new("Release provider registry is poisoned"))?
        .clone()
        .ok_or(release::Error::MissingProvider(
            "CLI CommandSurface/declaration and migration_contract::Bundled",
        ))
}
pub fn preflight(
    pack: &Path,
    binaries: &BTreeMap<String, PathBuf>,
    surface: &dyn crate::pack_validation::CommandSurface,
    migration: &dyn crate::artifact::MigrationContract,
) -> Result<BTreeMap<String, Value>> {
    require(
        binaries.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == release::TARGETS.into_iter().collect(),
        "Exactly four prebuilt target binaries are required",
    )?;
    let mut inspections = BTreeMap::new();
    for triple in release::TARGETS {
        let binary = binaries
            .get(triple)
            .ok_or_else(|| ContractError::new("Missing prebuilt release binary"))?;
        let inspection =
            crate::artifact::inspect_runtime_with(pack, binary, Some(surface), Some(migration))?;
        inspections.insert(triple.into(), inspection);
    }
    Ok(inspections)
}
fn command(root: &Path, argv: &[&str]) -> Result<crate::process::Output> {
    let short_lookup = matches!(argv, ["gh", "release", "view", ..])
        || matches!(argv, ["gh","api",path] if path.contains("/git/ref/tags/"));
    let timeout = Duration::from_secs(if short_lookup { 60 } else { 180 });
    let maximum = if matches!(argv, ["git", "cat-file", "blob", _]) {
        release::MAX_FILE
    } else if argv.first() == Some(&"gh") {
        release::MAX_METADATA
    } else {
        release::MAX_TAR
    };
    release::run_command(
        &argv.iter().map(OsString::from).collect::<Vec<_>>(),
        b"",
        maximum,
        Some(root),
        timeout,
    )
}
fn run(root: &Path, argv: &[&str]) -> Result<Vec<u8>> {
    let result = command(root, argv)?;
    require(
        result.status == Status::Code(0),
        &format!(
            "Release command failed: {}",
            argv.first().copied().unwrap_or("command")
        ),
    )?;
    Ok(result.stdout)
}
fn utf8(raw: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(raw)
        .map_err(|_| ContractError::new("Release command returned non-UTF-8 text"))?)
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut argv = vec!["git"];
    argv.extend_from_slice(args);
    Ok(utf8(run(root, &argv)?)?.trim().into())
}
fn api(root: &Path, path: &str) -> Result<Value> {
    release::parse(&run(root, &["gh", "api", path])?)
}
fn path_text(path: &Path) -> Result<&str> {
    Ok(crate::host::text(path.as_os_str(), "release path")?)
}
fn signing_key(inputs: &BuildInputs, output: &Path) -> Result<PathBuf> {
    let key = crate::host::expanduser(&inputs.key)?;
    crate::fs::trust::record(&key)?;
    crate::fs::trust::state(
        key.parent()
            .ok_or_else(|| ContractError::new("Signing key needs a parent"))?,
    )?;
    let key = crate::fs::canonical_path(&key, true)?;
    let parent = DirectoryChain::open(
        key.parent()
            .ok_or_else(|| ContractError::new("Signing key needs a parent"))?,
    )?;
    let info = parent.directory().stat_at(
        key.file_name()
            .ok_or_else(|| ContractError::new("Signing key needs a name"))?,
    )?;
    require(
        info.is_regular() && info.nlink == 1 && info.mode & 0o077 == 0 && info.mode & 0o400 != 0,
        "Signing key must be owner-readable and owner-only",
    )?;
    let root = crate::fs::canonical_path(&inputs.root, true)?;
    require(
        !key.starts_with(&root),
        "Signing key must be outside the publishing checkout and portable artifact",
    )?;
    require(
        !key.starts_with(output),
        "Signing key must be outside release output",
    )?;
    let public = utf8(run(&root, &["ssh-keygen", "-y", "-f", path_text(&key)?])?)?;
    let public = public
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let pinned = release::ALLOWED_SIGNERS
        .split_whitespace()
        .skip(2)
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    require(
        public == pinned,
        "Private key does not match the pinned AM key",
    )?;
    Ok(key)
}
fn sign(root: &Path, key: &Path, output: &Path, name: &str, namespace: &str) -> Result<()> {
    let path = output.join(name);
    run(
        root,
        &[
            "ssh-keygen",
            "-Y",
            "sign",
            "-n",
            namespace,
            "-f",
            path_text(key)?,
            path_text(&path)?,
        ],
    )?;
    let signature = release::regular(&output.join(format!("{name}.sig")), 4096)?;
    release::write_new(&output.join(format!("{name}.sshsig")), &signature, 0o644)?;
    Descriptor::open_directory(output)?.unlink_at(std::ffi::OsStr::new(&format!("{name}.sig")))?;
    let maximum = if namespace == release::BINARY_NAMESPACE {
        release::MAX_BINARY
    } else {
        release::MAX_METADATA
    };
    release::verify_signature_with(
        &release::regular(&path, maximum)?,
        &signature,
        namespace,
        release::ALLOWED_SIGNERS,
    )
}
pub fn build(
    inputs: &BuildInputs,
    output: &Path,
    surface: &dyn crate::pack_validation::CommandSurface,
    migration: &dyn crate::artifact::MigrationContract,
    command_surface: &Value,
) -> Result<ReleaseManifest> {
    let root = crate::fs::canonical_path(&inputs.root, true)?;
    let output = crate::fs::canonical_path(output, false)?;
    let output = output.as_path();
    let key = signing_key(inputs, output)?;
    require(
        git(&root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty(),
        "Release source must be clean",
    )?;
    let source = git(&root, &["rev-parse", "HEAD"])?;
    require(
        command_surface.as_object().is_some(),
        "Release command surface must be an object",
    )?;
    require(
        inputs
            .binaries
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == release::TARGETS.into_iter().collect(),
        "Exactly four prebuilt target binaries are required",
    )?;
    let listing = run(
        &root,
        &[
            "git",
            "ls-tree",
            "-rz",
            &source,
            "--",
            "skills/agent-memory",
        ],
    )?;
    let mut files = release::Inventory::new();
    let mut total = 0;
    for row in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = std::str::from_utf8(row)
            .map_err(|_| ContractError::new("Tracked asset path is not UTF-8"))?;
        let (metadata, name) = row
            .split_once('\t')
            .ok_or_else(|| ContractError::new("Invalid tracked release entry"))?;
        let parts: Vec<_> = metadata.split(' ').collect();
        let [mode, kind, blob] = parts.as_slice() else {
            return Err(ContractError::new("Unsafe tracked release entry").into());
        };
        require(
            *kind == "blob"
                && ["100644", "100755"].contains(mode)
                && name.starts_with(release::PREFIX),
            "Unsafe tracked release entry",
        )?;
        require(
            !name.ends_with(".py")
                && !name.ends_with(".pyc")
                && !name.ends_with(".pyo")
                && !name.contains("/__pycache__/"),
            "Rust release must not contain Python source or bytecode",
        )?;
        let raw = run(&root, &["git", "cat-file", "blob", blob])?;
        total += raw.len();
        require(
            raw.len() <= release::MAX_FILE
                && total <= release::MAX_BYTES
                && files.len() < release::MAX_ENTRIES,
            "Tracked asset exceeds portable bound",
        )?;
        files.insert(
            name.to_owned(),
            (raw, if *mode == "100755" { 0o755 } else { 0o644 }),
        );
    }
    let scratch = release::Scratch::new("am-release-pack-")?;
    for (name, (raw, mode)) in &files {
        let path = scratch.path.join(name);
        DirectoryChain::create(
            path.parent()
                .ok_or_else(|| ContractError::new("Invalid tracked release entry"))?,
            false,
        )?;
        release::write_new(&path, raw, *mode)?;
    }
    let (version, identity, _) =
        release::package_identity(&scratch.path.join("skills/agent-memory"))?;
    let inspections = preflight(
        &scratch.path.join("skills/agent-memory"),
        &inputs.binaries,
        surface,
        migration,
    )?;
    crate::fs::trust::state(output)?;
    crate::fs::private_mkdir(output)?;
    let archive = release::encode_archive(&files)?;
    release::write_new(&output.join(release::ARCHIVE), &archive, 0o644)?;
    let mut targets = BTreeMap::new();
    for triple in release::TARGETS {
        let path = inputs
            .binaries
            .get(triple)
            .ok_or_else(|| ContractError::new("Missing prebuilt release binary"))?;
        let raw = release::regular(path, release::MAX_BINARY)?;
        require(!raw.is_empty(), "Release binary is empty")?;
        let sha256 = release::digest(&raw);
        let inspection = inspections
            .get(triple)
            .ok_or_else(|| ContractError::new("Missing release runtime inspection"))?;
        require(
            text(get(get(inspection, "files"), "bin/am"), "sha256") == Some(sha256.as_str()),
            "Release binary changed after runtime preflight",
        )?;
        let artifact_digest = text(inspection, "artifact_digest")
            .ok_or_else(|| ContractError::new("Missing runtime artifact digest"))?
            .to_owned();
        targets.insert(
            triple.into(),
            BinaryIdentity {
                size: raw.len(),
                sha256,
                artifact_digest,
            },
        );
        let name = format!("am-{triple}");
        release::write_new(&output.join(&name), &raw, 0o755)?;
        sign(&root, &key, output, &name, release::BINARY_NAMESPACE)?;
    }
    let manifest = ReleaseManifest {
        tag: format!("v{version}"),
        version,
        source_commit: source.clone(),
        archive_size: archive.len(),
        archive_sha256: release::digest(&archive),
        artifact_digest: identity,
        targets,
        command_surface: command_surface.clone(),
    };
    let manifest_raw = manifest.encoded()?;
    ReleaseManifest::from_value(&manifest.value(), None)?;
    release::write_new(&output.join(release::MANIFEST), &manifest_raw, 0o644)?;
    sign(&root, &key, output, release::MANIFEST, release::NAMESPACE)?;
    let verified = release::verify_manifest(
        &manifest_raw,
        &release::regular(&output.join(release::SIGNATURE), 4096)?,
        Some(&manifest.tag),
    )?;
    release::extract(&archive, &scratch.path.join("verified"), &verified)?;
    let bootstrap = run(&root, &["git", "show", &format!("{source}:install.sh")])?;
    release::write_new(&output.join("install.sh"), &bootstrap, 0o755)?;
    require(
        git(&root, &["rev-parse", "HEAD"])? == source
            && git(&root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty(),
        "Source changed during release packaging",
    )?;
    Ok(manifest)
}
fn release_record(root: &Path, tag: &str) -> Result<Option<Value>> {
    let result = command(
        root,
        &[
            "gh",
            "release",
            "view",
            tag,
            "--repo",
            release::REPOSITORY,
            "--json",
            "apiUrl",
        ],
    )?;
    if result.status != Status::Code(0) {
        require(
            result.status == Status::Code(1)
                && String::from_utf8_lossy(&result.stderr)
                    .to_ascii_lowercase()
                    .contains("release not found"),
            "Release lookup failed; absence cannot be established",
        )?;
        return Ok(None);
    }
    let locator = release::parse(&result.stdout)?;
    let url = text(&locator, "apiUrl").unwrap_or("");
    let prefix = format!(
        "https://api.github.com/repos/{}/releases/",
        release::REPOSITORY
    );
    require(
        url.strip_prefix(&prefix)
            .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())),
        "Wrong release authority",
    )?;
    Ok(Some(api(root, url)?))
}
fn tag_commit(root: &Path, tag: &str, required: bool) -> Result<Option<String>> {
    let result = command(
        root,
        &[
            "gh",
            "api",
            &format!("repos/{}/git/ref/tags/{tag}", release::REPOSITORY),
        ],
    )?;
    if result.status != Status::Code(0) {
        require(
            !required && String::from_utf8_lossy(&result.stderr).contains("404"),
            "Actual release tag cannot be verified",
        )?;
        return Ok(None);
    }
    let mut value = get(&release::parse(&result.stdout)?, "object").clone();
    for _ in 0..4 {
        let sha = text(&value, "sha").unwrap_or("");
        require(
            sha.len() == 40
                && sha
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Invalid release tag object",
        )?;
        if text(&value, "type") == Some("commit") {
            return Ok(Some(sha.into()));
        }
        require(
            text(&value, "type") == Some("tag"),
            "Release tag does not name a commit",
        )?;
        value = get(
            &api(
                root,
                &format!("repos/{}/git/tags/{sha}", release::REPOSITORY),
            )?,
            "object",
        )
        .clone();
    }
    Err(ContractError::new("Release tag nesting exceeds its bound").into())
}
fn output_files(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let dir = Descriptor::open_directory(path)?;
    let names = dir
        .entries_bounded(32)?
        .ok_or_else(|| ContractError::new("Excessive release assets"))?;
    names
        .into_iter()
        .map(|name| {
            Ok((
                crate::host::text(&name, "release asset")?.into(),
                release::regular(&path.join(name), release::MAX_BINARY)?,
            ))
        })
        .collect()
}
fn compare_status(root: &Path, source: &str) -> Result<String> {
    Ok(text(
        &api(
            root,
            &format!("repos/{}/compare/main...{source}", release::REPOSITORY),
        )?,
        "status",
    )
    .unwrap_or("")
    .into())
}
fn verify_default(root: &Path, source: &str) -> Result<String> {
    let prefix = format!("repos/{}", release::REPOSITORY);
    let main = text(
        get(&api(root, &format!("{prefix}/branches/main"))?, "commit"),
        "sha",
    )
    .unwrap_or("")
    .to_owned();
    require(
        main.len() == 40
            && main
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid main identity",
    )?;
    let tree = api(root, &format!("{prefix}/git/trees/{main}"))?;
    require(
        get(&tree, "truncated") == &Value::Bool(false),
        "Default source tree is incomplete",
    )?;
    let rows = get(&tree, "tree")
        .as_array()
        .ok_or_else(|| ContractError::new("Default source layout differs"))?;
    let unique: BTreeSet<_> = rows.iter().filter_map(|v| text(v, "path")).collect();
    require(unique.len() == rows.len(), "Default source layout differs")?;
    let bootstrap = rows
        .iter()
        .find(|v| text(v, "path") == Some("install.sh"))
        .ok_or_else(|| ContractError::new("Default source layout differs"))?;
    let skills = rows
        .iter()
        .find(|v| text(v, "path") == Some("skills"))
        .ok_or_else(|| ContractError::new("Default source layout differs"))?;
    let mode = git(root, &["ls-tree", source, "--", "install.sh"])?;
    require(
        text(bootstrap, "type") == Some("blob")
            && text(bootstrap, "mode") == mode.split_whitespace().next()
            && text(bootstrap, "sha")
                == Some(git(root, &["rev-parse", &format!("{source}:install.sh")])?.as_str())
            && text(skills, "type") == Some("tree"),
        "Published release is verified, but current default bootstrap differs; activation remains pending",
    )?;
    let skills_sha =
        text(skills, "sha").ok_or_else(|| ContractError::new("Default source layout differs"))?;
    require(
        skills_sha.len() == 40
            && skills_sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Default source layout differs",
    )?;
    let packages = api(
        root,
        &format!("{prefix}/git/trees/{}", text(skills, "sha").unwrap_or("")),
    )?;
    let portable: Vec<_> = get(&packages, "tree")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter(|v| text(v, "path") == Some("agent-memory"))
        .collect();
    // The length check below short-circuits before examining the sole matching entry.
    require(
        get(&packages, "truncated") == &Value::Bool(false)
            && portable.len() == 1
            && text(portable[0], "type") == Some("tree")
            && text(portable[0], "sha")
                == Some(
                    git(
                        root,
                        &["rev-parse", &format!("{source}:skills/agent-memory")],
                    )?
                    .as_str(),
                ),
        "Published release is verified, but current default runtime differs; activation remains pending",
    )?;
    let expected = run(root, &["git", "show", &format!("{source}:install.sh")])?;
    require(
        release::download(RAW_DEFAULT, release::MAX_FILE)
            .map_err(|e| ContractError::new(e.to_string()))?
            == expected,
        "Published release is verified, but the public default bootstrap has not converged",
    )?;
    require(
        text(
            get(&api(root, &format!("{prefix}/branches/main"))?, "commit"),
            "sha",
        ) == Some(main.as_str()),
        "Main changed during default readback; retry without forcing any ref",
    )?;
    Ok(main)
}
pub fn publish(
    inputs: &BuildInputs,
    notes: &Path,
    integration_branch: &str,
    surface: &dyn crate::pack_validation::CommandSurface,
    migration: &dyn crate::artifact::MigrationContract,
    command_surface: &Value,
) -> Result<Value> {
    let root = &inputs.root;
    require(
        integration_branch.starts_with("integrate/")
            && git(root, &["symbolic-ref", "--short", "HEAD"])? == integration_branch,
        "Publish the reviewed release integration branch, not active main",
    )?;
    require(
        git(root, &["remote", "get-url", "origin"])?
            == format!("https://github.com/{}.git", release::REPOSITORY),
        "Publisher origin differs from the product repository",
    )?;
    let branch = api(
        root,
        &format!("repos/{}/branches/main", release::REPOSITORY),
    )?;
    let rules = api(
        root,
        &format!("repos/{}/rules/branches/main", release::REPOSITORY),
    )?;
    require(
        get(&branch, "protected") == &Value::Bool(false)
            && rules.as_array().is_some_and(|v| v.is_empty()),
        "Main promotion policy changed; review it before publishing this release",
    )?;
    release::regular(notes, release::MAX_FILE)?;
    let scratch = release::Scratch::new("am-release-")?;
    let output = scratch.path.join("assets");
    let manifest = build(inputs, &output, surface, migration, command_surface)?;
    let tag = &manifest.tag;
    let source = &manifest.source_commit;
    let mut existing = release_record(root, tag)?;
    require(
        utf8(run(
            root,
            &[
                "gh",
                "api",
                &format!("repos/{}/commits/{source}", release::REPOSITORY),
                "--jq",
                ".sha",
            ],
        )?)?
        .trim()
            == source,
        "Exact source is not published",
    )?;
    if existing.is_none() {
        require(
            run(
                root,
                &[
                    "git",
                    "ls-remote",
                    "--tags",
                    "origin",
                    &format!("refs/tags/{tag}"),
                ],
            )?
            .iter()
            .all(u8::is_ascii_whitespace),
            "Existing tag has no matching release; refusing version reuse",
        )?;
        require(
            compare_status(root, source)? == "ahead",
            "Release source must include current main before publication",
        )?;
        run(
            root,
            &[
                "gh",
                "release",
                "create",
                tag,
                "--repo",
                release::REPOSITORY,
                "--target",
                source,
                "--draft",
                "--title",
                tag,
                "--notes-file",
                path_text(notes)?,
            ],
        )?;
        existing = release_record(root, tag)?;
    }
    let existing = existing.ok_or_else(|| {
        ContractError::new("Created draft could not be resolved by its release identity")
    })?;
    require(
        text(&existing, "tag_name") == Some(tag)
            && get(&existing, "prerelease") == &Value::Bool(false),
        "Existing release selection differs",
    )?;
    let draft = get(&existing, "draft") == &Value::Bool(true);
    if draft {
        require(
            text(&existing, "target_commitish") == Some(source),
            "Draft source differs; refusing replacement",
        )?;
    } else {
        require(
            tag_commit(root, tag, true)?.as_deref() == Some(source),
            "Published source differs; refusing version reuse",
        )?;
    }
    let expected = output_files(&output)?;
    let assets = get(&existing, "assets")
        .as_array()
        .ok_or_else(|| ContractError::new("Invalid release assets"))?;
    let mut names = BTreeSet::new();
    for asset in assets {
        let name =
            text(asset, "name").ok_or_else(|| ContractError::new("Invalid release asset name"))?;
        require(
            names.insert(name.to_owned()) && expected.contains_key(name),
            "Unexpected existing release assets",
        )?;
        require(
            get(asset, "size")
                .as_int()
                .and_then(|n| n.to_string().parse::<usize>().ok())
                == expected.get(name).map(Vec::len),
            "Existing asset size differs",
        )?;
    }
    let readback = release::Scratch::new("am-release-readback-")?;
    for name in &names {
        run(
            root,
            &[
                "gh",
                "release",
                "download",
                tag,
                "--repo",
                release::REPOSITORY,
                "--pattern",
                name,
                "--dir",
                path_text(&readback.path)?,
            ],
        )?;
    }
    let actual = output_files(&readback.path)?;
    require(
        actual.len() == names.len() && names.iter().all(|n| actual.get(n) == expected.get(n)),
        "Existing release bytes differ; refusing replacement",
    )?;
    let missing: Vec<_> = expected
        .keys()
        .filter(|name| !names.contains(*name))
        .collect();
    require(
        missing.is_empty() || draft,
        "Published release is incomplete; refusing alteration",
    )?;
    for name in missing {
        run(
            root,
            &[
                "gh",
                "release",
                "upload",
                tag,
                path_text(&output.join(name))?,
                "--repo",
                release::REPOSITORY,
            ],
        )?;
    }
    let complete = release::Scratch::new("am-complete-readback-")?;
    run(
        root,
        &[
            "gh",
            "release",
            "download",
            tag,
            "--repo",
            release::REPOSITORY,
            "--dir",
            path_text(&complete.path)?,
        ],
    )?;
    require(
        output_files(&complete.path)? == expected,
        "Complete release assets differ after upload",
    )?;
    if draft {
        require(
            tag_commit(root, tag, false)?.is_none_or(|v| &v == source),
            "Existing tag differs; preserving draft and tag",
        )?;
        run(
            root,
            &[
                "gh",
                "release",
                "edit",
                tag,
                "--repo",
                release::REPOSITORY,
                "--draft=false",
                "--latest",
            ],
        )?;
    }
    require(
        tag_commit(root, tag, true)?.as_deref() == Some(source),
        "Published tag differs from signed source; no default activation",
    )?;
    let actual = api(
        root,
        &format!("repos/{}/releases/latest", release::REPOSITORY),
    )?;
    require(
        text(&actual, "tag_name") == Some(tag)
            && get(&actual, "draft") == &Value::Bool(false)
            && get(&actual, "prerelease") == &Value::Bool(false),
        "Published stable release readback differs",
    )?;
    let public = release::Scratch::new("am-public-readback-")?;
    require(
        release::acquire(&public.path.join("source"), None)? == manifest,
        "Public release differs from signed publication",
    )?;
    require(
        release::download(
            &format!("{}/{tag}/install.sh", release::ASSETS),
            release::MAX_FILE,
        )? == *expected
            .get("install.sh")
            .ok_or_else(|| ContractError::new("Release bootstrap is missing"))?,
        "Public release bootstrap differs from the reviewed source",
    )?;
    require(
        tag_commit(root, tag, true)?.as_deref() == Some(source),
        "Release tag changed during public readback",
    )?;
    if !["identical", "behind"].contains(&compare_status(root, source)?.as_str()) {
        let result = command(
            root,
            &[
                "git",
                "-c",
                "credential.helper=",
                "-c",
                "credential.helper=!gh auth git-credential",
                "push",
                "origin",
                &format!("{source}:refs/heads/main"),
            ],
        )?;
        require(
            result.status == Status::Code(0),
            &format!(
                "Signed {tag} is published and verified, but main advanced; no ref was forced. Review integration of {source} before claiming default activation"
            ),
        )?;
    }
    require(
        ["identical", "behind"].contains(&compare_status(root, source)?.as_str()),
        "Main activation could not be verified",
    )?;
    let main = verify_default(root, source)?;
    require(
        tag_commit(root, tag, true)?.as_deref() == Some(source),
        "Release tag changed during default activation",
    )?;
    Ok(object([
        ("published", Value::Bool(true)),
        ("tag", string(tag)),
        ("source_commit", string(source)),
        ("artifact_digest", string(&manifest.artifact_digest)),
        ("url", get(&actual, "html_url").clone()),
        ("default_activation_performed", Value::Bool(true)),
        ("verified_main_commit", string(main)),
    ]))
}
