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
// Bound the projected response, not unused release notes or comparison patches.
const MAX_GH_RESPONSE: usize = release::MAX_METADATA;
const RELEASE_FIELDS: &str =
    "{tag_name,draft,prerelease,target_commitish,html_url,assets:(.assets | map({name,size}))}";
const BRANCH_FIELDS: &str = "{protected,commit:{sha:.commit.sha}}";
const TREE_FIELDS: &str = "{truncated,tree:(.tree | map({path,type,mode,sha}))}";
const TAG_FIELDS: &str = "{object:{type:.object.type,sha:.object.sha}}";
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
            release::canonical(&providers.command_surface)?.len() <= release::MAX_MANIFEST,
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
        "Exactly three prebuilt target binaries are required",
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
        || matches!(argv, ["gh","api",path,..] if path.contains("/git/ref/tags/"));
    let timeout = Duration::from_secs(if short_lookup { 60 } else { 180 });
    let maximum = if matches!(argv, ["git", "cat-file", "blob", _]) {
        release::MAX_FILE
    } else if argv.first() == Some(&"gh") {
        MAX_GH_RESPONSE
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
fn api(root: &Path, path: &str, fields: &str) -> Result<Value> {
    release::parse(&run(root, &["gh", "api", path, "--jq", fields])?)
}
fn path_text(path: &Path) -> Result<&str> {
    Ok(crate::host::text(path.as_os_str(), "release path")?)
}
fn signing_key(inputs: &BuildInputs, output: &Path, signers: &str) -> Result<PathBuf> {
    let key = crate::host::expanduser(&inputs.key)?;
    // Anchor lexically before trust inspection; resolving here would hide a final symlink.
    let key = if key.is_absolute() {
        key
    } else {
        std::env::current_dir()?.join(key)
    };
    let key = key.components().fold(PathBuf::new(), |mut path, part| {
        if part == std::path::Component::ParentDir {
            path.pop();
        } else {
            path.push(part.as_os_str());
        }
        path
    });
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
    let pinned = signers
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
fn sign(
    root: &Path,
    key: &Path,
    output: &Path,
    name: &str,
    namespace: &str,
    signers: &str,
) -> Result<()> {
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
        release::MAX_MANIFEST
    };
    release::verify_signature_with(
        &release::regular(&path, maximum)?,
        &signature,
        namespace,
        signers,
    )
}
pub fn build(
    inputs: &BuildInputs,
    output: &Path,
    surface: &dyn crate::pack_validation::CommandSurface,
    migration: &dyn crate::artifact::MigrationContract,
    command_surface: &Value,
) -> Result<ReleaseManifest> {
    build_trusted(
        inputs,
        output,
        surface,
        migration,
        command_surface,
        release::ALLOWED_SIGNERS,
    )
}
// Explicit authority stays private: production entrypoints always use the compiled pin.
fn build_trusted(
    inputs: &BuildInputs,
    output: &Path,
    surface: &dyn crate::pack_validation::CommandSurface,
    migration: &dyn crate::artifact::MigrationContract,
    command_surface: &Value,
    signers: &str,
) -> Result<ReleaseManifest> {
    let root = crate::fs::canonical_path(&inputs.root, true)?;
    let output = crate::fs::canonical_path(output, false)?;
    let output = output.as_path();
    let key = signing_key(inputs, output, signers)?;
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
        "Exactly three prebuilt target binaries are required",
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
        sign(
            &root,
            &key,
            output,
            &name,
            release::BINARY_NAMESPACE,
            signers,
        )?;
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
    sign(
        &root,
        &key,
        output,
        release::MANIFEST,
        release::NAMESPACE,
        signers,
    )?;
    let verified = release::verify_manifest_with(
        &manifest_raw,
        &release::regular(&output.join(release::SIGNATURE), 4096)?,
        Some(&manifest.tag),
        signers,
    )?;
    release::extract(&archive, &scratch.path.join("verified"), &verified)?;
    let bootstrap = run(&root, &["git", "show", &format!("{source}:install.sh")])?;
    release::write_new(&output.join("install.sh"), &bootstrap, 0o755)?;
    require(
        git(&root, &["rev-parse", "HEAD"])? == source
            && git(&root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty(),
        "Source changed during release packaging",
    )?;
    Ok(verified)
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
    Ok(Some(api(root, url, RELEASE_FIELDS)?))
}
fn tag_commit(root: &Path, tag: &str, required: bool) -> Result<Option<String>> {
    let result = command(
        root,
        &[
            "gh",
            "api",
            &format!("repos/{}/git/ref/tags/{tag}", release::REPOSITORY),
            "--jq",
            TAG_FIELDS,
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
                TAG_FIELDS,
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
    Ok(utf8(run(
        root,
        &[
            "gh",
            "api",
            &format!("repos/{}/compare/main...{source}", release::REPOSITORY),
            "--jq",
            ".status",
        ],
    )?)?
    .trim()
    .into())
}
fn verify_default(root: &Path, source: &str) -> Result<String> {
    let prefix = format!("repos/{}", release::REPOSITORY);
    let main = text(
        get(
            &api(root, &format!("{prefix}/branches/main"), BRANCH_FIELDS)?,
            "commit",
        ),
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
    let tree = api(root, &format!("{prefix}/git/trees/{main}"), TREE_FIELDS)?;
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
        TREE_FIELDS,
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
            get(
                &api(root, &format!("{prefix}/branches/main"), BRANCH_FIELDS)?,
                "commit",
            ),
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
    publish_trusted(
        inputs,
        notes,
        integration_branch,
        surface,
        migration,
        command_surface,
        release::ALLOWED_SIGNERS,
    )
}
fn publish_trusted(
    inputs: &BuildInputs,
    notes: &Path,
    integration_branch: &str,
    surface: &dyn crate::pack_validation::CommandSurface,
    migration: &dyn crate::artifact::MigrationContract,
    command_surface: &Value,
    signers: &str,
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
        BRANCH_FIELDS,
    )?;
    let rules = api(
        root,
        &format!("repos/{}/rules/branches/main", release::REPOSITORY),
        "if type == \"array\" then length == 0 else false end",
    )?;
    require(
        get(&branch, "protected") == &Value::Bool(false) && rules == Value::Bool(true),
        "Main promotion policy changed; review it before publishing this release",
    )?;
    release::regular(notes, release::MAX_FILE)?;
    let scratch = release::Scratch::new("am-release-")?;
    let output = scratch.path.join("assets");
    let manifest = build_trusted(
        inputs,
        &output,
        surface,
        migration,
        command_surface,
        signers,
    )?;
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
        RELEASE_FIELDS,
    )?;
    require(
        text(&actual, "tag_name") == Some(tag)
            && get(&actual, "draft") == &Value::Bool(false)
            && get(&actual, "prerelease") == &Value::Bool(false),
        "Published stable release readback differs",
    )?;
    let public = release::Scratch::new("am-public-readback-")?;
    let observed = release::probe_with(
        &format!(
            "https://github.com/{}/releases/latest/download/{}",
            release::REPOSITORY,
            release::MANIFEST
        ),
        release::ASSETS,
        release::DownloadOptions::default(),
        signers,
    )?;
    require(
        release::acquire_with(
            &public.path.join("source"),
            &observed,
            release::DownloadOptions::default(),
        )? == manifest,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{Input, Spec, Target};
    use std::os::unix::fs::{PermissionsExt, symlink};

    // Real Git object reads and real, namespaced SSH signatures. Only remote
    // Git, gh and curl are simulated; fixture authority never impersonates the pin.
    const GH: &[u8] = br#"#!/bin/sh
set -eu
s=$DELIVERY_ROOT/service
. "$s/identities"
release_json() {
  printf '{\n"body":"'; cat "$s/padding"; printf '",\n'
  printf '"tag_name":"v0.0.15","draft":'
  if [ -f "$s/published" ]; then printf false; else printf true; fi
  printf ',"prerelease":false,"target_commitish":"%s","html_url":"' "$DELIVERY_SOURCE"
  if [ "$DELIVERY_MODE" = release-bound ]; then cat "$s/padding"; else printf 'https://github.com/robertelee78/agent-memory/releases/tag/v0.0.15'; fi
  printf '","assets":['
  separator=
  for asset in "$s/assets/"*; do
    [ -f "$asset" ] || continue
    size=$(wc -c < "$asset" | tr -d ' ')
    printf '%s{"name":"%s","size":%s}' "$separator" "${asset##*/}" "$size"
    separator=,
  done
  printf ']}\n'
}
case "$1 $2" in
  'release view')
    if [ ! -f "$s/created" ]; then echo 'release not found' >&2; exit 1; fi
    printf '{"apiUrl":"https://api.github.com/repos/robertelee78/agent-memory/releases/1"}\n' ;;
  'release create')
    [ ! -f "$s/created" ]; : > "$s/created"; echo create >> "$s/events" ;;
  'release upload')
    if [ "$DELIVERY_MODE" = partial ] && [ -f "$s/uploaded" ] && [ ! -f "$s/interrupted" ]; then
      : > "$s/interrupted"; exit 1
    fi
    cp "$4" "$s/assets/${4##*/}"
    : > "$s/uploaded"; echo upload >> "$s/events" ;;
  'release download')
    shift 3
    pattern=
    destination=
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --repo) shift 2 ;;
        --dir) destination=$2; shift 2 ;;
        --pattern) pattern=$2; shift 2 ;;
        *) exit 90 ;;
      esac
    done
    [ -n "$destination" ]
    if [ -n "$pattern" ]; then cp "$s/assets/$pattern" "$destination/$pattern"
    else for asset in "$s/assets/"*; do [ -f "$asset" ] || continue; cp "$asset" "$destination/${asset##*/}"; done
    fi ;;
  'release edit')
    [ -f "$s/created" ]; [ ! -f "$s/published" ]
    : > "$s/published"; echo publish >> "$s/events" ;;
  api*)
    endpoint=$2
    selector=
    if [ "$#" -eq 4 ] && [ "$3" = --jq ]; then selector=$4; elif [ "$#" -ne 2 ]; then exit 90; fi
    case "$endpoint" in
      */compare/main...*)
        if [ "$DELIVERY_MODE" = compare-bound ]; then record=$s/compare-excessive
        elif [ -f "$s/promoted" ]; then record=$s/compare-identical
        else record=$s/compare-ahead; fi
        if [ "$selector" = .status ]; then cut -d '"' -f 4 "$record"
        elif [ -z "$selector" ]; then cat "$record"; else exit 90; fi ;;
      */releases/1|*/releases/latest)
        case "$selector" in
          '{tag_name,draft,prerelease,target_commitish,html_url,assets:(.assets | map({name,size}))}')
            release_json | awk '!/^"body":/' ;;
          '') release_json ;;
          *) exit 90 ;;
        esac ;;
      repos/robertelee78/agent-memory/branches/main) printf '{"protected":false,"commit":{"sha":"%s"}}\n' "$DELIVERY_SOURCE" ;;
      */rules/branches/main)
        [ "$selector" = 'if type == "array" then length == 0 else false end' ]
        printf 'true\n' ;;
      */commits/*) [ "$selector" = .sha ]; printf '%s\n' "$DELIVERY_SOURCE" ;;
      */git/ref/tags/v0.0.15)
        if [ ! -f "$s/published" ]; then echo 404 >&2; exit 1; fi
        printf '{"object":{"type":"commit","sha":"%s"}}\n' "$DELIVERY_SOURCE" ;;
      */git/trees/*)
        case "$endpoint" in
          */"$DELIVERY_SOURCE") cat "$s/root-tree" ;;
          */"$DELIVERY_SKILLS") cat "$s/skills-tree" ;;
          *) exit 90 ;;
        esac ;;
      *) echo "Unrecorded API: $endpoint" >&2; exit 90 ;;
    esac ;;
  *) echo 'Unrecorded gh operation' >&2; exit 90 ;;
esac
"#;
    const GIT: &[u8] = br#"#!/bin/sh
set -eu
[ ! -f "$DELIVERY_ROOT/service/identities" ] || . "$DELIVERY_ROOT/service/identities"
case "$1" in
  ls-remote) exit 0 ;;
  -c)
    [ "$1" = -c ] && [ "$2" = credential.helper= ] &&
    [ "$3" = -c ] && [ "$4" = 'credential.helper=!gh auth git-credential' ] &&
    [ "$5" = push ] && [ "$6" = origin ] &&
    [ "$7" = "$DELIVERY_SOURCE:refs/heads/main" ] || exit 90
    : > "$DELIVERY_ROOT/service/promoted"
    echo push >> "$DELIVERY_ROOT/service/events" ;;
  *) exec "$REAL_GIT" "$@" ;;
esac
"#;
    const CURL: &[u8] = br#"#!/bin/sh
set -eu
output=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output) output=$2; shift 2 ;;
    --) shift; url=$1; shift ;;
    *) shift ;;
  esac
done
case "$url" in
  https://github.com/robertelee78/agent-memory/releases/latest/download/release-manifest.json)
    source=$DELIVERY_ROOT/service/assets/release-manifest.json ;;
  https://github.com/robertelee78/agent-memory/releases/download/v0.0.15/*)
    source=$DELIVERY_ROOT/service/assets/${url##*/} ;;
  https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh)
    source=$DELIVERY_ROOT/checkout/install.sh ;;
  *) echo 'Unrecorded download' >&2; exit 90 ;;
esac
cat "$source" > "$output"
printf 200
"#;

    fn isolated(name: &str, modes: &[&str]) -> bool {
        if std::env::var_os("DELIVERY_ROOT").is_some() {
            return true;
        }
        let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join("git"))
            .find(|p| p.is_file())
            .expect("Git is required by delivery tests");
        for mode in modes {
            let scratch = release::Scratch::new("am-publisher-test-").unwrap();
            for directory in ["tools", "home", "tmp", "cwd", "service/assets"] {
                crate::fs::private_mkdir(&scratch.path.join(directory)).unwrap();
            }
            for (name, script) in [("gh", GH), ("git", GIT), ("curl", CURL)] {
                release::write_new(&scratch.path.join("tools").join(name), script, 0o755).unwrap();
            }
            let env = [
                (
                    "PATH".into(),
                    format!(
                        "{}:/usr/bin:/bin:/usr/local/bin",
                        scratch.path.join("tools").display()
                    )
                    .into(),
                ),
                ("HOME".into(), scratch.path.join("home").into_os_string()),
                ("TMPDIR".into(), scratch.path.join("tmp").into_os_string()),
                ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
                ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
                ("GIT_CONFIG_SYSTEM".into(), "/dev/null".into()),
                ("GIT_AUTHOR_DATE".into(), "2026-11-13T00:00:00+00:00".into()),
                (
                    "GIT_COMMITTER_DATE".into(),
                    "2026-11-13T00:00:00+00:00".into(),
                ),
                ("LC_ALL".into(), "C".into()),
                ("REAL_GIT".into(), real_git.clone().into_os_string()),
                (
                    "DELIVERY_ROOT".into(),
                    scratch.path.clone().into_os_string(),
                ),
                ("DELIVERY_MODE".into(), (*mode).into()),
            ];
            let test_name = format!("{}::{name}", module_path!().split_once("::").unwrap().1);
            let argv = [
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                test_name.into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ];
            let result = crate::process::run(&Spec {
                argv: &argv,
                search_path: false,
                env: &env,
                cwd: Some(&scratch.path.join("cwd")),
                stdin: Input::Null,
                stdout: Target::Capture { limit: 16384 },
                stderr: Target::Capture { limit: 16384 },
                deadline: crate::clock::Deadline::after(Duration::from_secs(120)).unwrap(),
                new_session: false,
                file_size_limit: None,
                umask: Some(0o077),
                pass_fds: &[],
            })
            .unwrap();
            assert_eq!(
                result.status,
                Status::Code(0),
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(
                String::from_utf8_lossy(&result.stdout).contains("1 passed"),
                "worker did not execute"
            );
        }
        false
    }

    struct NoExecutableSurface;
    impl crate::pack_validation::CommandSurface for NoExecutableSurface {
        fn dispatch(
            &self,
            command: &str,
        ) -> crate::pack_validation::Result<crate::pack_validation::Dispatch> {
            Err(crate::pack_validation::Invalid::new(
                "dispatch",
                command,
                "Fixture has no executable routes",
            ))
        }
        /// Intentionally non-executable test surface: no command resolves.
        fn command(
            &self,
            _name: &str,
        ) -> crate::pack_validation::Result<Option<(crate::pack_validation::Dispatch, Vec<String>)>>
        {
            Ok(None)
        }
    }
    struct Fixture {
        root: PathBuf,
        inputs: BuildInputs,
        signers: String,
        surface: Value,
    }
    impl Fixture {
        fn new() -> Self {
            let root = PathBuf::from(std::env::var_os("DELIVERY_ROOT").unwrap());
            let checkout = root.join("checkout");
            crate::fs::private_mkdir(&checkout).unwrap();
            run(
                &root,
                &[
                    "ssh-keygen",
                    "-q",
                    "-t",
                    "ed25519",
                    "-N",
                    "",
                    "-f",
                    path_text(&root.join("key")).unwrap(),
                ],
            )
            .unwrap();
            let public = utf8(release::regular(&root.join("key.pub"), 4096).unwrap()).unwrap();
            let key = public
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");
            let signers = format!(
                "{} namespaces=\"{},{}\" {key}\n",
                release::PRINCIPAL,
                release::NAMESPACE,
                release::BINARY_NAMESPACE
            );
            let pack = checkout.join("skills/agent-memory");
            crate::fs::private_mkdir(&pack).unwrap();
            let mut files = BTreeMap::from([
                ("SKILL.md", "---\nname: agent-memory\ndescription: Portable publication fixture\nmetadata: {\"version\":\"0.0.15\"}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n".to_owned()),
                ("bank/GUIDE.md", "---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n".to_owned()),
                ("schemas/runtime-compatibility.json", r#"{"schema":"agent-memory/runtime-compatibility-v1","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":[],"git_required_for":["sharing"],"optional_adapters":[],"required_agent_clients":[]}"#.to_owned()),
                ("schemas/bank-router.json", r#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#.to_owned()),
                ("keys/allowed-signers", release::ALLOWED_SIGNERS.to_owned()),
            ]);
            let mut operation =
                "---\nkind: operation\ndomain: bank\nfile-operations: [\"inspect\"]\n---\n\n"
                    .to_owned();
            for section in crate::pack_validation::SECTIONS {
                operation.push_str(&format!("## {section}\n\n"));
                operation.push_str(if section == "Required context" {
                    "A session MUST first load and follow the [bank domain](../bank/GUIDE.md).\n\n"
                } else {
                    "Explicit operation context.\n\n"
                });
            }
            files.insert("inspect/GUIDE.md", operation);
            for (name, raw) in files {
                release::portable_directory(&pack, Path::new(name).parent().unwrap()).unwrap();
                release::write_new(&pack.join(name), raw.as_bytes(), 0o644).unwrap();
            }
            release::write_new(&checkout.join("install.sh"), b"#!/bin/sh\nexit 0\n", 0o755)
                .unwrap();
            git(&checkout, &["init", "-b", "integrate/delivery"]).unwrap();
            git(&checkout, &["config", "user.name", "Delivery Fixture"]).unwrap();
            git(
                &checkout,
                &["config", "user.email", "delivery@example.invalid"],
            )
            .unwrap();
            git(&checkout, &["config", "commit.gpgsign", "false"]).unwrap();
            git(&checkout, &["add", "."]).unwrap();
            git(
                &checkout,
                &["commit", "-m", "Recorded portable publication fixture"],
            )
            .unwrap();
            git(
                &checkout,
                &[
                    "remote",
                    "add",
                    "origin",
                    &format!("https://github.com/{}.git", release::REPOSITORY),
                ],
            )
            .unwrap();
            let source = git(&checkout, &["rev-parse", "HEAD"]).unwrap();
            let skills = git(&checkout, &["rev-parse", "HEAD:skills"]).unwrap();
            let bootstrap = git(&checkout, &["rev-parse", "HEAD:install.sh"]).unwrap();
            let portable = git(&checkout, &["rev-parse", "HEAD:skills/agent-memory"]).unwrap();
            // gh's shell fixture loads recorded identities instead of modifying this process's env.
            release::write_new(
                &root.join("service/identities"),
                format!("DELIVERY_SOURCE={source}\nDELIVERY_SKILLS={skills}\n").as_bytes(),
                0o600,
            )
            .unwrap();
            release::write_new(&root.join("service/root-tree"), format!(r#"{{"truncated":false,"tree":[{{"path":"install.sh","type":"blob","mode":"100755","sha":"{bootstrap}"}},{{"path":"skills","type":"tree","sha":"{skills}"}}]}}"#).as_bytes(), 0o600).unwrap();
            release::write_new(&root.join("service/skills-tree"), format!(r#"{{"truncated":false,"tree":[{{"path":"agent-memory","type":"tree","sha":"{portable}"}}]}}"#).as_bytes(), 0o600).unwrap();
            let padding = "x".repeat(release::MAX_METADATA + 1);
            release::write_new(&root.join("service/padding"), padding.as_bytes(), 0o600).unwrap();
            for (name, status, body) in [
                ("compare-ahead", "ahead", padding.as_str()),
                ("compare-identical", "identical", padding.as_str()),
                ("compare-excessive", padding.as_str(), ""),
            ] {
                release::write_new(
                    &root.join("service").join(name),
                    format!(r#"{{"status":"{status}","patch":"{body}"}}"#).as_bytes(),
                    0o600,
                )
                .unwrap();
            }
            let mut binaries = BTreeMap::new();
            for triple in release::TARGETS {
                let path = root.join(format!("am-{triple}"));
                release::write_new(&path, triple.as_bytes(), 0o755).unwrap();
                binaries.insert(triple.to_owned(), path);
            }
            release::write_new(&root.join("notes"), padding.as_bytes(), 0o600).unwrap();
            Self {
                root,
                inputs: BuildInputs {
                    root: checkout,
                    key: "../key".into(),
                    binaries,
                },
                signers,
                surface: object([
                    (
                        "z",
                        object([("z_nested", string("last")), ("a_nested", string("first"))]),
                    ),
                    (
                        "a",
                        Value::Array(vec![object([
                            ("z_entry", string("last")),
                            ("a_entry", string("first")),
                        ])]),
                    ),
                ]),
            }
        }
        fn publish(&self) -> Result<Value> {
            publish_trusted(
                &self.inputs,
                &self.root.join("notes"),
                "integrate/delivery",
                &NoExecutableSurface,
                &crate::migration_contract::Bundled,
                &self.surface,
                &self.signers,
            )
        }
        fn events(&self) -> String {
            utf8(release::regular(&self.root.join("service/events"), 4096).unwrap()).unwrap()
        }
    }

    #[test]
    fn publication_completion_and_resume_canonicalize_nested_surface_and_project_large_responses() {
        if !isolated(
            "publication_completion_and_resume_canonicalize_nested_surface_and_project_large_responses",
            &["normal", "partial"],
        ) {
            return;
        }
        let fixture = Fixture::new();
        let built = build_trusted(
            &fixture.inputs,
            &fixture.root.join("built"),
            &NoExecutableSurface,
            &crate::migration_contract::Bundled,
            &fixture.surface,
            &fixture.signers,
        )
        .unwrap();
        let reparsed = release::verify_manifest_with(
            &release::regular(
                &fixture.root.join("built").join(release::MANIFEST),
                release::MAX_MANIFEST,
            )
            .unwrap(),
            &release::regular(&fixture.root.join("built").join(release::SIGNATURE), 4096).unwrap(),
            Some("v0.0.15"),
            &fixture.signers,
        )
        .unwrap();
        assert_eq!(built, reparsed);
        assert_ne!(built.command_surface, fixture.surface);
        let raw_compare = run(
            &fixture.inputs.root,
            &[
                "gh",
                "api",
                "repos/robertelee78/agent-memory/compare/main...recorded",
            ],
        );
        assert!(
            matches!(&raw_compare, Err(release::Error::Process(e)) if matches!(e.reason, crate::process::Refusal::CaptureLimit { .. }))
        );
        if std::env::var("DELIVERY_MODE").unwrap() == "partial" {
            assert!(fixture.publish().is_err());
            assert_eq!(fixture.events(), "create\nupload\n");
            assert!(!fixture.root.join("service/published").exists());
        }
        let result = fixture.publish().unwrap();
        assert_eq!(get(&result, "published"), &Value::Bool(true));
        assert_eq!(
            get(&result, "default_activation_performed"),
            &Value::Bool(true)
        );
        assert_eq!(
            text(&result, "verified_main_commit"),
            Some(built.source_commit.as_str())
        );
        let events = fixture.events();
        assert_eq!(events.lines().filter(|v| *v == "create").count(), 1);
        assert_eq!(events.lines().filter(|v| *v == "publish").count(), 1);
        assert_eq!(events.lines().filter(|v| *v == "push").count(), 1);
        assert_eq!(
            events.lines().filter(|v| *v == "upload").count(),
            4 + 2 * release::TARGETS.len()
        );
        let raw_release = run(
            &fixture.inputs.root,
            &[
                "gh",
                "api",
                "repos/robertelee78/agent-memory/releases/latest",
            ],
        );
        assert!(
            matches!(&raw_release, Err(release::Error::Process(e)) if matches!(e.reason, crate::process::Refusal::CaptureLimit { .. }))
        );
        assert_eq!(fixture.publish().unwrap(), result);
        assert_eq!(
            fixture.events(),
            events,
            "resume must not mutate published assets or main"
        );
    }

    #[test]
    fn publication_refuses_excessive_projected_compare_and_release_before_activation() {
        if !isolated(
            "publication_refuses_excessive_projected_compare_and_release_before_activation",
            &["compare-bound", "release-bound"],
        ) {
            return;
        }
        let fixture = Fixture::new();
        let error = fixture.publish().unwrap_err();
        assert!(
            matches!(&error, release::Error::Process(e) if matches!(e.reason, crate::process::Refusal::CaptureLimit { .. })),
            "{error}"
        );
        assert!(!fixture.root.join("service/published").exists());
        assert!(!fixture.root.join("service/promoted").exists());
        if std::env::var("DELIVERY_MODE").unwrap() == "release-bound" {
            assert_eq!(fixture.events(), "create\n");
        } else {
            assert!(!fixture.root.join("service/created").exists());
        }
    }

    #[test]
    fn relative_signing_key_is_anchored_lexically_and_retains_custody_refusals() {
        if !isolated(
            "relative_signing_key_is_anchored_lexically_and_retains_custody_refusals",
            &["key"],
        ) {
            return;
        }
        let mut fixture = Fixture::new();
        let output = fixture.root.join("output");
        assert_eq!(
            signing_key(&fixture.inputs, &output, &fixture.signers).unwrap(),
            fixture.root.join("key")
        );
        symlink("key", fixture.root.join("linked-key")).unwrap();
        fixture.inputs.key = "../linked-key".into();
        let error = build_trusted(
            &fixture.inputs,
            &output,
            &NoExecutableSurface,
            &crate::migration_contract::Bundled,
            &fixture.surface,
            &fixture.signers,
        )
        .unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error}");
        assert!(!output.exists());
        fixture.inputs.key = "../key".into();
        // The exact same valid relative fixture key is not the production key.
        assert_eq!(
            signing_key(&fixture.inputs, &output, release::ALLOWED_SIGNERS)
                .unwrap_err()
                .to_string(),
            "Private key does not match the pinned AM key"
        );
        std::fs::set_permissions(
            fixture.root.join("key"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert_eq!(
            signing_key(&fixture.inputs, &output, &fixture.signers)
                .unwrap_err()
                .to_string(),
            "Signing key must be owner-readable and owner-only"
        );
        assert!(!output.exists());
    }
}

#[cfg(test)]
#[path = "gaps_publisher.rs"]
mod gaps_tests;
