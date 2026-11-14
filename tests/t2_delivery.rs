use agent_memory::cache_recovery as cache;
use agent_memory::codec::json::Value;
use agent_memory::release_distribution::{self as release, BinaryIdentity, ReleaseManifest};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn object(pairs: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn string(s: impl Into<String>) -> Value {
    Value::String(s.into())
}
fn command(args: &[OsString], input: &[u8]) -> Vec<u8> {
    let out =
        release::run_command(args, input, release::MAX_TAR, None, Duration::from_secs(20)).unwrap();
    assert_eq!(
        out.status,
        agent_memory::process::Status::Code(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
struct Publisher {
    scratch: release::Scratch,
    key: PathBuf,
    signers: String,
}
impl Publisher {
    fn new() -> Self {
        let scratch = release::Scratch::new("am-delivery-test-").unwrap();
        let key = scratch.path.join("key");
        command(
            &[
                "ssh-keygen".into(),
                "-q".into(),
                "-t".into(),
                "ed25519".into(),
                "-N".into(),
                "".into(),
                "-f".into(),
                key.as_os_str().to_owned(),
            ],
            b"",
        );
        let public =
            String::from_utf8(release::regular(&key.with_extension("pub"), 4096).unwrap()).unwrap();
        let key_fields = public
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        let signers = format!(
            "{} namespaces=\"{},{}\" {}\n",
            release::PRINCIPAL,
            release::NAMESPACE,
            release::BINARY_NAMESPACE,
            key_fields
        );
        Self {
            scratch,
            key,
            signers,
        }
    }
    fn sign(&self, raw: &[u8], namespace: &str) -> Vec<u8> {
        command(
            &[
                "ssh-keygen".into(),
                "-Y".into(),
                "sign".into(),
                "-n".into(),
                namespace.into(),
                "-f".into(),
                self.key.as_os_str().to_owned(),
            ],
            raw,
        )
    }
}
fn pack_files() -> release::Inventory {
    BTreeMap::from([
        ("SKILL.md".into(),(b"---\nname: agent-memory\nmetadata: {\"version\": \"0.0.15\"}\n---\nPortable fixture.\n".to_vec(),0o644)),
        ("keys/allowed-signers".into(),(release::ALLOWED_SIGNERS.as_bytes().to_vec(),0o644)),
        ("scripts/agent_memory/example.py".into(),(b"Opaque legacy source bytes, never executed.\n".to_vec(),0o644)),
        ("schemas/runtime-compatibility.json".into(),(b"{\"schema\":\"fixture/compatibility-v1\"}\n".to_vec(),0o644)),
    ])
}
fn write_pack(root: &Path, files: &release::Inventory) {
    agent_memory::fs::private_mkdir(root).unwrap();
    for (name, (raw, mode)) in files {
        let path = root.join(name);
        release::portable_directory(root, Path::new(name).parent().unwrap()).unwrap();
        release::write_new(&path, raw, *mode).unwrap();
    }
}
fn manifest(files: &release::Inventory, archive: &[u8], binary: &[u8]) -> ReleaseManifest {
    let inventory = release::inventory_value(files);
    let sha256 = release::digest(binary);
    let target = BinaryIdentity {
        size: binary.len(),
        artifact_digest: agent_memory::artifact::runtime_digest(&inventory, &sha256, 0o755)
            .unwrap()
            .1,
        sha256,
    };
    ReleaseManifest {
        version: "0.0.15".into(),
        tag: "v0.0.15".into(),
        source_commit: "a".repeat(40),
        archive_size: archive.len(),
        archive_sha256: release::digest(archive),
        artifact_digest: release::inventory_digest(&inventory).unwrap(),
        targets: release::TARGETS
            .into_iter()
            .map(|t| (t.into(), target.clone()))
            .collect(),
        command_surface: object([(
            "commands",
            Value::Array(vec![string("install"), string("update")]),
        )]),
    }
}
fn archive(files: &release::Inventory) -> Vec<u8> {
    release::encode_archive(
        &files
            .iter()
            .map(|(k, v)| (format!("{}{k}", release::PREFIX), v.clone()))
            .collect(),
    )
    .unwrap()
}
fn gzip(raw: &[u8]) -> Vec<u8> {
    let mut writer = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::default());
    writer.write_all(raw).unwrap();
    writer.finish().unwrap()
}
fn resign_archive(manifest: &ReleaseManifest, raw: &[u8]) -> ReleaseManifest {
    let mut value = manifest.clone();
    value.archive_size = raw.len();
    value.archive_sha256 = release::digest(raw);
    value
}
fn tar_header(name: &str, size: usize, mode: u32, kind: u8) -> Vec<u8> {
    let mut raw = vec![0u8; 512];
    raw[..name.len()].copy_from_slice(name.as_bytes());
    for (start, width, value) in [
        (100, 8, mode as usize),
        (108, 8, 0),
        (116, 8, 0),
        (124, 12, size),
        (136, 12, 0),
    ] {
        raw[start..start + width - 1]
            .copy_from_slice(format!("{value:0width$o}", width = width - 1).as_bytes());
    }
    raw[148..156].fill(b' ');
    raw[156] = kind;
    raw[257..263].copy_from_slice(b"ustar\0");
    raw[263..265].copy_from_slice(b"00");
    let sum: usize = raw.iter().map(|b| *b as usize).sum();
    raw[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    raw
}

#[test]
fn signed_manifest_has_one_strict_v2_definition_and_canonical_encoding() {
    let p = Publisher::new();
    let files = pack_files();
    let m = manifest(&files, &archive(&files), b"native runtime bytes");
    let raw = m.encoded().unwrap();
    assert_eq!(raw.last(), Some(&b'\n'));
    assert!(!raw.ends_with(b"\n\n"));
    assert!(raw.starts_with(b"{\"archive\":\"agent-memory.tar.gz\",\"archive_sha256\":"));
    let signature = p.sign(&raw, release::NAMESPACE);
    assert_eq!(
        release::verify_manifest_with(&raw, &signature, Some("v0.0.15"), &p.signers).unwrap(),
        m
    );
    assert!(release::verify_manifest(&raw, &signature, None).is_err());
    assert!(release::verify_manifest_with(&raw, &signature, Some("v0.0.16"), &p.signers).is_err());
    let schema = release::parse(include_bytes!(
        "../skills/agent-memory/schemas/release-v2.json"
    ))
    .unwrap();
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 11);
    assert_eq!(
        release::canonical(&object([("é", string("☃")), ("z", Value::Int(3.into()))])).unwrap(),
        b"{\"z\":3,\"\\u00e9\":\"\\u2603\"}"
    );
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadBootstrap.test_invalid_signature_is_not_retried
#[test]
fn tampering_signature_or_payload_refuses() {
    let p = Publisher::new();
    let files = pack_files();
    let m = manifest(&files, &archive(&files), b"runtime");
    let mut raw = m.encoded().unwrap();
    let signature = p.sign(&raw, release::NAMESPACE);
    raw[20] ^= 1;
    assert!(release::verify_manifest_with(&raw, &signature, None, &p.signers).is_err());
    assert!(
        release::verify_manifest_with(&m.encoded().unwrap(), b"invalid", None, &p.signers).is_err()
    );
}

#[test]
fn manifest_signature_is_not_a_binary_signature() {
    let p = Publisher::new();
    let raw = b"identical bytes across namespaces";
    let signature = p.sign(raw, release::NAMESPACE);
    assert!(
        release::verify_signature_with(raw, &signature, release::NAMESPACE, &p.signers).is_ok()
    );
    assert!(
        release::verify_signature_with(raw, &signature, release::BINARY_NAMESPACE, &p.signers)
            .is_err()
    );
}

#[test]
fn binary_signature_is_not_a_manifest_signature() {
    let p = Publisher::new();
    let raw = b"identical bytes across namespaces";
    let signature = p.sign(raw, release::BINARY_NAMESPACE);
    assert!(
        release::verify_signature_with(raw, &signature, release::BINARY_NAMESPACE, &p.signers)
            .is_ok()
    );
    assert!(
        release::verify_signature_with(raw, &signature, release::NAMESPACE, &p.signers).is_err()
    );
}

#[test]
fn manifest_schema_rejects_legacy_unknown_fields_invalid_targets_and_numbers() {
    let files = pack_files();
    let m = manifest(&files, &archive(&files), b"runtime");
    let mut value = m.value();
    *value.get_mut("schema").unwrap() = string("agent-memory/release-v1");
    assert!(
        ReleaseManifest::from_value(&value, None)
            .unwrap_err()
            .to_string()
            .contains("re-run the trusted Rust bootstrap")
    );
    for field in [
        "archive_size",
        "targets",
        "version",
        "repository",
        "command_surface",
        "source_commit",
        "artifact_digest",
    ] {
        let mut value = m.value();
        *value.get_mut(field).unwrap() = Value::Bool(false);
        assert!(
            ReleaseManifest::from_value(&value, None).is_err(),
            "{field}"
        );
    }
    let mut value = m.value();
    *value.get_mut("archive_size").unwrap() = Value::Float(1.0);
    assert!(ReleaseManifest::from_value(&value, None).is_err());
    let mut value = m.value();
    *value.get_mut("targets").unwrap() = object([("unsupported", Value::Null)]);
    assert!(ReleaseManifest::from_value(&value, None).is_err());
    let mut value = m.value();
    if let Value::Object(pairs) = &mut value {
        pairs.push(("extra".into(), Value::Null));
    }
    assert!(ReleaseManifest::from_value(&value, None).is_err());
    for raw in [
        b"{\"x\":1,\"x\":2}".as_slice(),
        b"{\"x\":NaN}",
        b"\xef\xbb\xbf{}",
        b"\xff",
    ] {
        assert!(release::parse(raw).is_err());
    }
    assert!(release::parse(&vec![b' '; release::MAX_METADATA + 1]).is_err());
}

#[test]
fn command_surface_comparison_is_structural_canonical_and_candidate_free() {
    let a = object([
        ("a", Value::Int(1.into())),
        ("b", object([("x", string("native"))])),
    ]);
    let b = object([
        ("b", object([("x", string("native"))])),
        ("a", Value::Int(1.into())),
    ]);
    release::compare_command_surfaces(&a, &b).unwrap();
    assert!(release::compare_command_surfaces(&a, &object([("a", Value::Int(2.into()))])).is_err());
    assert!(release::compare_command_surfaces(&Value::Null, &Value::Null).is_err());
}

#[test]
fn bootstrap_binds_exact_bytes_target_and_compiled_version() {
    let p = Publisher::new();
    let binary = p.scratch.path.join("am");
    release::write_new(&binary, b"runtime", 0o755).unwrap();
    let files = pack_files();
    let m = manifest(&files, &archive(&files), b"runtime");
    assert_eq!(
        release::bind_bootstrap_binary(&m, &binary, release::TARGETS[0], "0.0.15").unwrap(),
        b"runtime"
    );
    assert!(release::bind_bootstrap_binary(&m, &binary, release::TARGETS[0], "0.0.14").is_err());
    assert!(release::bind_bootstrap_binary(&m, &binary, "unknown", "0.0.15").is_err());
    std::fs::write(&binary, b"swapped").unwrap();
    assert!(release::bind_bootstrap_binary(&m, &binary, release::TARGETS[0], "0.0.15").is_err());
    let a = agent_memory::artifact::runtime_digest(
        &release::inventory_value(&files),
        &release::digest(b"runtime"),
        0o755,
    )
    .unwrap()
    .1;
    let b = agent_memory::artifact::runtime_digest(
        &release::inventory_value(&files),
        &release::digest(b"swapped"),
        0o755,
    )
    .unwrap()
    .1;
    assert_ne!(a, b);
}

#[test]
fn reproducible_archive_roundtrip_preserves_bytes_and_modes() {
    let p = Publisher::new();
    let files = pack_files();
    let raw = archive(&files);
    assert_eq!(raw, archive(&files));
    assert_eq!(&raw[..8], b"\x1f\x8b\x08\0\0\0\0\0");
    let m = manifest(&files, &raw, b"runtime");
    let destination = p.scratch.path.join("unpacked");
    release::extract(&raw, &destination, &m).unwrap();
    let (version, identity, observed) =
        release::package_identity(&destination.join("skills/agent-memory")).unwrap();
    assert_eq!(version, "0.0.15");
    assert_eq!(identity, m.artifact_digest);
    assert_eq!(observed, release::inventory_value(&files));
    assert_eq!(
        std::fs::metadata(destination.join("skills"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert!(release::extract(&raw, &destination, &m).is_err());
}

#[test]
fn malformed_archives_never_escape_destination() {
    let p = Publisher::new();
    let files = pack_files();
    let m = manifest(&files, &archive(&files), b"runtime");
    for (index, (name, kind, mode, size)) in [
        ("../escape", b'0', 0o644, 0),
        ("/tmp/escape", b'0', 0o644, 0),
        ("skills/agent-memory/../escape", b'0', 0o644, 0),
        ("skills//agent-memory/x", b'0', 0o644, 0),
        ("skills/agent-memory/x", b'2', 0o644, 0),
        ("skills/agent-memory/x", b'1', 0o644, 0),
        ("skills/agent-memory/x", b'x', 0o644, 0),
        ("skills/agent-memory/x", b'0', 0o777, 0),
        ("skills/agent-memory/x", b'0', 0o644, release::MAX_FILE + 1),
        ("skills/agent-memory/dir", b'5', 0o644, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut tar = tar_header(name, size, mode, kind);
        tar.resize(2048, 0);
        let raw = gzip(&tar);
        assert!(
            release::extract(
                &raw,
                &p.scratch.path.join(format!("bad-{index}")),
                &resign_archive(&m, &raw)
            )
            .is_err(),
            "{name}"
        );
    }
    assert!(!p.scratch.path.join("escape").exists());
    assert!(release::tar_entries(&[0; 512]).is_err());
    let mut duplicate = tar_header("skills/agent-memory/x", 0, 0o644, b'0');
    duplicate.extend_from_slice(&duplicate.clone());
    duplicate.resize(2048, 0);
    assert!(release::tar_entries(&duplicate).is_err());
}

#[test]
fn gzip_trailer_trailing_data_multimember_and_expansion_bound_refuse() {
    let p = Publisher::new();
    let files = pack_files();
    let raw = archive(&files);
    let m = manifest(&files, &raw, b"runtime");
    let mut crc = raw.clone();
    let last = crc.len() - 8;
    crc[last] ^= 1;
    let mut trailing = raw.clone();
    trailing.extend_from_slice(b"trailing");
    let mut multi = raw.clone();
    multi.extend_from_slice(&raw);
    let oversized = gzip(&vec![0; release::MAX_TAR + 1]);
    for (i, bytes) in [crc, trailing, multi, oversized].into_iter().enumerate() {
        assert!(
            release::extract(
                &bytes,
                &p.scratch.path.join(format!("bad-gz-{i}")),
                &resign_archive(&m, &bytes)
            )
            .is_err()
        );
    }
}

#[test]
fn portable_inputs_refuse_symlink_hardlink_modes_and_asset_bounds() {
    let p = Publisher::new();
    let root = p.scratch.path.join("pack");
    write_pack(&root, &pack_files());
    release::package_identity(&root).unwrap();
    let link = root.join("linked");
    symlink("SKILL.md", &link).unwrap();
    assert!(release::package_identity(&root).is_err());
    std::fs::remove_file(&link).unwrap();
    std::fs::hard_link(root.join("SKILL.md"), &link).unwrap();
    assert!(release::package_identity(&root).is_err());
    std::fs::remove_file(&link).unwrap();
    std::fs::set_permissions(
        root.join("SKILL.md"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(release::package_identity(&root).is_err());
    assert!(release::regular(&root.join("SKILL.md"), 1).is_err());
}

#[test]
fn file_url_acquisition_uses_selected_tag_after_first_fetch() {
    let p = Publisher::new();
    let files = pack_files();
    let raw = archive(&files);
    let m = manifest(&files, &raw, b"runtime");
    let assets = p.scratch.path.join("assets");
    let version = assets.join("v0.0.15");
    agent_memory::fs::private_mkdir(&version).unwrap();
    let manifest_raw = m.encoded().unwrap();
    let signature = p.sign(&manifest_raw, release::NAMESPACE);
    release::write_new(&version.join(release::ARCHIVE), &raw, 0o644).unwrap();
    release::write_new(&version.join(release::SIGNATURE), &signature, 0o644).unwrap();
    let first = p.scratch.path.join("first-manifest");
    release::write_new(&first, &manifest_raw, 0o644).unwrap();
    let options = release::DownloadOptions {
        allow_file: true,
        json_response: false,
    };
    let observed = release::probe_with(
        &format!("file://{}", first.display()),
        &format!("file://{}", assets.display()),
        options,
        &p.signers,
    )
    .unwrap();
    assert!(observed.base.ends_with("/v0.0.15/"));
    assert_eq!(
        release::acquire_with(&p.scratch.path.join("acquired"), &observed, options).unwrap(),
        m
    );
    assert!(
        release::download(
            &format!("file://{}", first.display()),
            release::MAX_METADATA
        )
        .is_err()
    );
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_no_retry_for_permanent_errors
#[test]
fn file_download_size_limit_discards_partial_bytes_and_retains_private_evidence() {
    let p = Publisher::new();
    let file = p.scratch.path.join("asset");
    release::write_new(&file, &[b'x'; 100], 0o644).unwrap();
    let error = release::download_with(
        &format!("file://{}", file.display()),
        16,
        release::DownloadOptions {
            allow_file: true,
            json_response: false,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.details.get("attempt_count"),
        Some(&Value::Int(1.into()))
    );
    assert_eq!(error.details.get("reason"), Some(&string("size-limit")));
    assert_eq!(
        error.details.get("activation_accepted"),
        Some(&Value::Bool(false))
    );
    let report = PathBuf::from(
        error
            .details
            .get("diagnostic_path")
            .unwrap()
            .as_str()
            .unwrap(),
    );
    assert_eq!(
        std::fs::metadata(&report).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(report.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(release::regular(&report, 8192).is_ok());
    std::fs::remove_dir_all(report.parent().unwrap()).unwrap();
}

// obligation: tests/contracts/test_download_diagnostics.py::ReviewRegressions.test_diagnostic_redaction_normalizes_before_matching
#[test]
fn diagnostic_detail_never_retains_authentication_environment_urls_or_paths() {
    let raw = b"curl: Authorization: Bearer AUTH_SECRET\ntoken=\"SAFE_PREFIX SECRET_TAIL\"\nCURL_CA_BUNDLE=/private/ACCOUNT_SECRET.pem\nhttps:\x00//host/path?sig=URL_SECRET\nauth_token=AUTH_TOKEN_SECRET client_secret=\"CLIENT SECRET_TAIL\"\ncurl: (77) error setting certificate file: /private/BARE_ACCOUNT_SECRET.pem";
    let detail = release::safe_download_detail(raw);
    for secret in [
        "AUTH_SECRET",
        "SECRET_TAIL",
        "ACCOUNT_SECRET",
        "URL_SECRET",
        "AUTH_TOKEN_SECRET",
        "BARE_ACCOUNT_SECRET",
        "https://",
    ] {
        assert!(!detail.contains(secret), "{detail}");
    }
    assert!(detail.len() <= 512);
}

struct FixtureInstallation;
impl cache::Installation for FixtureInstallation {
    fn fault(&self, _point: &str) -> release::Result<()> {
        Ok(())
    }
    fn inspect_package(&self, pack: &Path) -> release::Result<Value> {
        let (version, identity, _) = release::package_identity(pack)?;
        let compatibility = release::parse(&release::regular(
            &pack.join("schemas/runtime-compatibility.json"),
            release::MAX_METADATA,
        )?)?;
        Ok(object([
            ("version", string(version)),
            ("artifact_digest", string(identity)),
            ("compatibility", compatibility),
        ]))
    }
}
fn recovery_fixture(p: &Publisher) -> (PathBuf, Value) {
    let root = p.scratch.path.join("source");
    let files = pack_files();
    let m = manifest(&files, &archive(&files), b"runtime");
    let raw = m.encoded().unwrap();
    let signature = p.sign(&raw, release::NAMESPACE);
    write_pack(&root.join("skills/agent-memory"), &files);
    release::portable_directory(&root, Path::new("bin")).unwrap();
    release::write_new(&root.join("bin/am"), b"runtime", 0o755).unwrap();
    release::write_new(&root.join(release::MANIFEST), &raw, 0o644).unwrap();
    release::write_new(&root.join(release::SIGNATURE), &signature, 0o644).unwrap();
    let target = release::TARGETS[0];
    let source = object([
        ("kind", string("release")),
        ("path", string(root.to_string_lossy())),
        (
            "pack",
            string(root.join("skills/agent-memory").to_string_lossy()),
        ),
        (
            "remote",
            string("https://github.com/robertelee78/agent-memory.git"),
        ),
        ("ref", string("releases/latest")),
        ("revision", string(&m.source_commit)),
        ("dirty", Value::Bool(false)),
        ("version", string(&m.version)),
        (
            "artifact_digest",
            string(&m.targets[target].artifact_digest),
        ),
        (
            "compatibility",
            object([("schema", string("fixture/compatibility-v1"))]),
        ),
        ("release_manifest_sha256", string(release::digest(&raw))),
        ("signature_namespace", string(release::NAMESPACE)),
        ("signature_principal", string(release::PRINCIPAL)),
        ("target", string(target)),
    ]);
    let identity = cache::observe_with(&root, &FixtureInstallation)
        .unwrap()
        .identity(&BTreeSet::new())
        .unwrap();
    let receipt = object([
        ("source", source),
        ("artifact_files", release::inventory_value(&files)),
        (
            "targets",
            Value::Array(vec![object([
                ("kind", string("source")),
                ("path", string(root.to_string_lossy())),
                ("identity", identity),
            ])]),
        ),
    ]);
    (root, receipt)
}
fn add_cache(root: &Path, stem: &str, raw: &[u8]) -> PathBuf {
    let relative = Path::new("skills/agent-memory/scripts/agent_memory/__pycache__");
    release::portable_directory(root, relative).unwrap();
    let path = root.join(relative).join(format!("{stem}.pyc"));
    release::write_new(&path, raw, 0o600).unwrap();
    path
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_same_version_preview_fresh_acquisition_exact_backup_and_clean_noop
#[test]
fn cache_qualification_authenticates_all_bytes_without_executing_cache() {
    let p = Publisher::new();
    let (root, receipt) = recovery_fixture(&p);
    assert!(cache::qualify_with(&root, &receipt, &FixtureInstallation, &p.signers).is_err());
    let path = add_cache(
        &root,
        "example.cpython-311",
        b"opaque bytes not a bytecode file",
    );
    let before = release::regular(&path, 1024).unwrap();
    let qualified = cache::qualify_with(&root, &receipt, &FixtureInstallation, &p.signers).unwrap();
    assert_eq!(
        qualified.summary.get("cache_count"),
        Some(&Value::Int(1.into()))
    );
    assert_eq!(
        qualified.summary.get("cache_code_executed"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        qualified.summary.get("cache_validity_verified"),
        Some(&Value::Bool(false))
    );
    assert_eq!(release::regular(&path, 1024).unwrap(), before);
    cache::recheck_with(&qualified, &FixtureInstallation).unwrap();
    std::fs::write(&path, b"concurrently changed opaque bytes").unwrap();
    assert!(cache::recheck_with(&qualified, &FixtureInstallation).is_err());
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_unqualified_payload_cache_types_and_bounds_preserve_every_target
#[test]
fn cache_qualification_refuses_unrelated_unsigned_wrong_mode_and_signed_drift() {
    for variant in [
        "module", "tag", "mode", "symlink", "payload", "binary", "receipt",
    ] {
        let p = Publisher::new();
        let (root, mut receipt) = recovery_fixture(&p);
        let stem = match variant {
            "module" => "foreign.cpython-311",
            "tag" => "example.cpython-31",
            _ => "example.cpython-311.opt-1",
        };
        let path = add_cache(&root, stem, b"opaque cache");
        match variant {
            "mode" => {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap()
            }
            "symlink" => {
                std::fs::remove_file(&path).unwrap();
                symlink("../../example.py", &path).unwrap();
            }
            "payload" => std::fs::write(
                root.join("skills/agent-memory/scripts/agent_memory/example.py"),
                b"changed",
            )
            .unwrap(),
            "binary" => std::fs::write(root.join("bin/am"), b"changed").unwrap(),
            "receipt" => {
                *receipt.get_mut("artifact_files").unwrap() = object([("foreign", Value::Null)]);
            }
            _ => {}
        }
        assert!(
            cache::qualify_with(&root, &receipt, &FixtureInstallation, &p.signers).is_err(),
            "{variant}"
        );
        assert!(path.symlink_metadata().is_ok());
    }
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_python_created_group_writable_cache_directories_below_private_source_recover
#[test]
fn private_barrier_allows_legacy_writable_cache_directory() {
    let p = Publisher::new();
    let (root, receipt) = recovery_fixture(&p);
    let path = add_cache(&root, "example.cpython-311", b"opaque");
    std::fs::set_permissions(
        path.parent().unwrap(),
        std::fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    cache::qualify_with(&root, &receipt, &FixtureInstallation, &p.signers).unwrap();
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_late_signed_payload_or_other_target_change_never_receives_race_qualification
#[test]
fn final_observation_recheck_refuses_concurrent_writer() {
    struct Writer {
        path: PathBuf,
    }
    impl cache::Installation for Writer {
        fn fault(&self, _point: &str) -> release::Result<()> {
            std::fs::write(&self.path, b"changed by concurrent writer")?;
            Ok(())
        }
        fn inspect_package(&self, pack: &Path) -> release::Result<Value> {
            FixtureInstallation.inspect_package(pack)
        }
    }
    let p = Publisher::new();
    let (root, _) = recovery_fixture(&p);
    let path = add_cache(&root, "example.cpython-311", b"original opaque");
    assert!(cache::observe_with(&root, &Writer { path: path.clone() }).is_err());
    assert_eq!(
        release::regular(&path, 1024).unwrap(),
        b"changed by concurrent writer"
    );
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_exact_count_and_byte_limits_allow_recovery_but_next_cache_refuses
#[test]
fn cache_exact_count_limit_accepts_256_and_refuses_257() {
    let p = Publisher::new();
    let (root, receipt) = recovery_fixture(&p);
    // All cache spellings refer to the same authenticated module; distinct CPython tags are opaque.
    for index in 0..256 {
        let major = 10 + index / 10;
        let optimization = index % 10;
        add_cache(
            &root,
            &format!("example.cpython-3{major:02}.opt-{optimization}"),
            b"x",
        );
    }
    let qualified = cache::qualify_with(&root, &receipt, &FixtureInstallation, &p.signers).unwrap();
    assert_eq!(
        qualified.summary.get("cache_count"),
        Some(&Value::Int(256.into()))
    );
    add_cache(&root, "example.cpython-399.opt-9", b"x");
    assert!(cache::qualify_with(&root, &receipt, &FixtureInstallation, &p.signers).is_err());
}

#[test]
fn missing_installation_registration_is_an_explicit_refusal() {
    cache::register_installation(None).unwrap();
    let p = Publisher::new();
    assert!(
        cache::observe(&p.scratch.path)
            .unwrap_err()
            .to_string()
            .contains("not registered")
    );
}

fn transport_environment(p: &Publisher, mode: &str) -> Vec<(OsString, OsString)> {
    let script = br#"#!/bin/sh
n=0
if [ -f "$COUNTER" ]; then read -r n < "$COUNTER"; fi
n=$((n+1))
printf '%s\n' "$n" > "$COUNTER"
output=
while [ "$#" -gt 0 ]; do
  if [ "$1" = --output ]; then shift; output=$1; break; fi
  shift
done
case "$FAULT" in
  slow) exec /bin/sleep 10 ;;
  signal) kill -XFSZ $$ ;;
esac
body=complete
code=0
status=200
case "$FAULT" in
  flaky) if [ "$n" = 1 ]; then body=partial; code=56; status=000; fi ;;
  http) if [ "$n" = 1 ]; then code=22; status=503; fi ;;
  receive|redact) body=partial; code=56; status=000 ;;
  tls) code=60; status=503 ;;
  missing) code=22; status=404 ;;
  rate) code=22; status=429 ;;
  permanent-status) code=56; status=403 ;;
  malformed) code=22; status=503bad ;;
esac
printf '%s' "$body" > "$output"
printf '%s' "$status"
printf '%s\n' 'receive error' >&2
if [ "$FAULT" = redact ]; then
  printf '%s\n' 'Authorization: Bearer AUTH_SECRET' 'https://a:PASS_SECRET@host/?token=URL_SECRET' 'token=TOKEN_SECRET' >&2
fi
exit "$code"
"#;
    let curl = p.scratch.path.join("curl");
    if !curl.exists() {
        release::write_new(&curl, script, 0o755).unwrap();
    }
    let path = format!(
        "{}:{}",
        p.scratch.path.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    std::env::vars_os()
        .filter(|(k, _)| !["PATH", "COUNTER", "FAULT"].iter().any(|v| k == v))
        .chain([
            (OsString::from("PATH"), OsString::from(path)),
            (
                OsString::from("COUNTER"),
                p.scratch.path.join("count").into_os_string(),
            ),
            (OsString::from("FAULT"), OsString::from(mode)),
        ])
        .collect()
}
fn remove_report(error: &release::DownloadError) {
    if let Some(path) = error.details.get("diagnostic_path").and_then(Value::as_str) {
        std::fs::remove_dir_all(Path::new(path).parent().unwrap()).unwrap();
    }
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_recovery_discards_partial_bytes
#[test]
fn transient_receive_failure_discards_partial_bytes_before_retry() {
    let p = Publisher::new();
    let env = transport_environment(&p, "flaky");
    let raw = release::download_with_environment(
        "https://example.com/asset",
        1024,
        Default::default(),
        &env,
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(raw, b"complete");
    assert_eq!(
        release::regular(&p.scratch.path.join("count"), 16).unwrap(),
        b"2\n"
    );
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_private_bounded_redacted_evidence
#[test]
fn three_failed_attempts_retain_only_private_redacted_transport_evidence() {
    let p = Publisher::new();
    let env = transport_environment(&p, "redact");
    let error = release::download_with_environment(
        "https://example.com/release-manifest.json?token=URL_SECRET",
        1024,
        Default::default(),
        &env,
        Duration::from_secs(30),
    )
    .unwrap_err();
    assert_eq!(
        error.details.get("attempt_count"),
        Some(&Value::Int(3.into()))
    );
    assert_eq!(
        error.details.get("reason"),
        Some(&string("receive failure"))
    );
    assert_eq!(
        error.details.get("stage"),
        Some(&string("release manifest"))
    );
    let diagnostic = error
        .details
        .get("diagnostic_path")
        .unwrap()
        .as_str()
        .unwrap();
    let raw = release::regular(Path::new(diagnostic), 8192).unwrap();
    let details = String::from_utf8(release::canonical(&error.details).unwrap()).unwrap();
    let combined = format!(
        "{} {} {} {}",
        String::from_utf8_lossy(&raw),
        error,
        error.repair,
        details
    );
    for secret in [
        "AUTH_SECRET",
        "PASS_SECRET",
        "URL_SECRET",
        "TOKEN_SECRET",
        "\x1b",
    ] {
        assert!(!combined.contains(secret));
    }
    remove_report(&error);
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_http_retry_and_deadline
#[test]
fn retryable_http_rejection_and_absolute_deadline_preserve_budget() {
    let p = Publisher::new();
    let env = transport_environment(&p, "http");
    assert_eq!(
        release::download_with_environment(
            "https://example.com/asset",
            1024,
            Default::default(),
            &env,
            Duration::from_secs(30)
        )
        .unwrap(),
        b"complete"
    );
    let env = transport_environment(&p, "slow");
    let start = agent_memory::clock::monotonic_ns();
    let error = release::download_with_environment(
        "https://example.com/asset",
        1024,
        Default::default(),
        &env,
        Duration::from_millis(200),
    )
    .unwrap_err();
    assert!(agent_memory::clock::monotonic_ns() - start < 2_000_000_000);
    assert_eq!(
        error.details.get("attempt_count"),
        Some(&Value::Int(1.into()))
    );
    assert_eq!(error.details.get("reason"), Some(&string("deadline")));
    remove_report(&error);
}

// obligation: tests/contracts/test_download_diagnostics.py::ReviewRegressions.test_http_permanent_status_wins_over_transient_curl_exit
// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_no_retry_for_permanent_errors
#[test]
fn permanent_failures_are_not_retried_even_with_transient_curl_exit() {
    for (mode, reason) in [
        ("tls", "certificate validation failure"),
        ("missing", "HTTP rejection"),
        ("rate", "HTTP rejection"),
        ("malformed", "HTTP rejection"),
        ("permanent-status", "receive failure"),
    ] {
        let p = Publisher::new();
        let env = transport_environment(&p, mode);
        let error = release::download_with_environment(
            "https://example.com/asset",
            1024,
            Default::default(),
            &env,
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(
            error.details.get("attempt_count"),
            Some(&Value::Int(1.into())),
            "{mode}"
        );
        assert_eq!(error.details.get("reason"), Some(&string(reason)), "{mode}");
        remove_report(&error);
    }
}

// obligation: tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_resource_signal_not_retried
#[test]
fn file_size_signal_is_a_nonretryable_size_limit_refusal() {
    let p = Publisher::new();
    let env = transport_environment(&p, "signal");
    let error = release::download_with_environment(
        "https://example.com/asset",
        1024,
        Default::default(),
        &env,
        Duration::from_secs(30),
    )
    .unwrap_err();
    assert_eq!(
        error.details.get("attempt_count"),
        Some(&Value::Int(1.into()))
    );
    assert_eq!(error.details.get("reason"), Some(&string("size-limit")));
    remove_report(&error);
}

// obligation: tests/contracts/test_cache_update_recovery.py::CacheUpdateRecoveryContract.test_exact_count_and_byte_limits_allow_recovery_but_next_cache_refuses
#[test]
fn cache_exact_byte_bound_accepts_16_mib_and_refuses_one_more_byte() {
    let p = Publisher::new();
    let (root, receipt) = recovery_fixture(&p);
    let raw = vec![b'x'; cache::MAX_CACHE_FILE];
    for tag in 10..18 {
        add_cache(&root, &format!("example.cpython-3{tag}"), &raw);
    }
    let receipt = release::parse(&cache::encoded(&receipt).unwrap()).unwrap();
    let qualified = cache::qualify_with(&root, &receipt, &FixtureInstallation, &p.signers).unwrap();
    assert_eq!(
        qualified.summary.get("cache_bytes"),
        Some(&Value::Int(cache::MAX_CACHE_BYTES.into()))
    );
    add_cache(&root, "example.cpython-318", b"x");
    assert!(cache::qualify_with(&root, &receipt, &FixtureInstallation, &p.signers).is_err());
}

#[test]
fn descriptor_directory_creation_is_exclusive_and_listing_is_bounded() {
    let scratch = release::Scratch::new("am-delivery-fs-test-").unwrap();
    let dir = agent_memory::fs::Descriptor::open_directory(&scratch.path).unwrap();
    dir.create_directory(std::ffi::OsStr::new("a"), 0o755)
        .unwrap();
    assert_eq!(dir.entries_bounded(0).unwrap(), None);
    assert_eq!(
        dir.entries_bounded(1).unwrap(),
        Some(vec![OsString::from("a")])
    );
    assert!(
        dir.create_directory(std::ffi::OsStr::new("a"), 0o700)
            .is_err()
    );
    assert_eq!(
        dir.stat_at(std::ffi::OsStr::new("a")).unwrap().mode & 0o777,
        0o755
    );
}

#[test]
fn installed_runtime_checks_binary_mode_and_exact_owned_layout() {
    let p = Publisher::new();
    let (root, _) = recovery_fixture(&p);
    release::installed_with(&root, release::TARGETS[0], &p.signers).unwrap();
    std::fs::set_permissions(root.join("bin/am"), std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(release::installed_with(&root, release::TARGETS[0], &p.signers).is_err());
    std::fs::set_permissions(root.join("bin/am"), std::fs::Permissions::from_mode(0o755)).unwrap();
    release::write_new(&root.join("foreign"), b"preserved", 0o600).unwrap();
    assert!(release::installed_with(&root, release::TARGETS[0], &p.signers).is_err());
    assert_eq!(
        release::regular(&root.join("foreign"), 1024).unwrap(),
        b"preserved"
    );
}

// obligation: tests/contracts/test_signed_release_publisher.py::SignedPublisherContract.test_unsafe_signing_key_paths_refuse_before_output_or_key_use
#[cfg(feature = "dev-tools")]
#[test]
fn developer_builder_refuses_unsafe_and_unpinned_signing_keys_before_output() {
    let p = Publisher::new();
    let checkout = p.scratch.path.join("checkout");
    agent_memory::fs::private_mkdir(&checkout).unwrap();
    let inside = checkout.join("key");
    release::write_new(&inside, &release::regular(&p.key, 4096).unwrap(), 0o600).unwrap();
    let linked = p.scratch.path.join("linked-key");
    std::fs::hard_link(&p.key, &linked).unwrap();
    let redirected = p.scratch.path.join("redirected-key");
    symlink(&p.key, &redirected).unwrap();
    let unsafe_parent = p.scratch.path.join("unsafe-key-parent");
    agent_memory::fs::private_mkdir(&unsafe_parent).unwrap();
    let unsafe_key = unsafe_parent.join("key");
    release::write_new(
        &unsafe_key,
        &release::regular(&inside, 4096).unwrap(),
        0o600,
    )
    .unwrap();
    std::fs::set_permissions(&unsafe_parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (index, key) in [inside, linked, redirected, unsafe_key]
        .into_iter()
        .enumerate()
    {
        let output = p.scratch.path.join(format!("refused-output-{index}"));
        let inputs = agent_memory::release_builder::BuildInputs {
            root: checkout.clone(),
            key,
            binaries: BTreeMap::new(),
        };
        let error = agent_memory::release_builder::build(
            &inputs,
            &output,
            &NoExecutableSurface,
            &agent_memory::migration_contract::Bundled,
            &object([] as [(&str, Value); 0]),
        )
        .unwrap_err();
        assert!(!error.to_string().contains("Release source must be clean"));
        assert!(!output.exists());
    }
}

#[cfg(feature = "dev-tools")]
struct NoExecutableSurface;
#[cfg(feature = "dev-tools")]
impl agent_memory::pack_validation::CommandSurface for NoExecutableSurface {
    fn dispatch(
        &self,
        script: &str,
        _files: &agent_memory::pack_validation::Inventory,
    ) -> agent_memory::pack_validation::Result<agent_memory::pack_validation::Dispatch> {
        Err(agent_memory::pack_validation::Invalid::new(
            "dispatch",
            script,
            "Fixture declares no executable command routes",
        ))
    }
}

#[cfg(feature = "dev-tools")]
#[test]
fn builder_preflight_uses_runtime_inspection_for_all_four_prebuilt_targets() {
    let p = Publisher::new();
    let root = p.scratch.path.join("portable");
    let mut files = release::Inventory::new();
    files.insert("SKILL.md".into(),(
        b"---\nname: agent-memory\ndescription: Bounded portable release fixture\nmetadata: {\"version\":\"0.0.15\"}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n".to_vec(),0o644,
    ));
    files.insert(
        "bank/GUIDE.md".into(),
        (
            b"---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n".to_vec(),
            0o644,
        ),
    );
    let mut operation =
        String::from("---\nkind: operation\ndomain: bank\nfile-operations: [\"inspect\"]\n---\n\n");
    for section in agent_memory::pack_validation::SECTIONS {
        operation.push_str(&format!("## {section}\n\n"));
        if section == "Required context" {
            operation.push_str(
                "A session MUST first load and follow the [bank domain](../bank/GUIDE.md).\n\n",
            );
        } else {
            operation.push_str("Explicit operation context.\n\n");
        }
    }
    files.insert("inspect/GUIDE.md".into(), (operation.into_bytes(), 0o644));
    files.insert("schemas/runtime-compatibility.json".into(),(br#"{"schema":"agent-memory/runtime-compatibility-v1","python_minimum":"3.11","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":[],"git_required_for":["sharing"],"optional_adapters":[],"required_python_packages":[],"required_agent_clients":[]}"#.to_vec(),0o644));
    files.insert(
        "schemas/bank-router.json".into(),
        (
            br#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#.to_vec(),
            0o644,
        ),
    );
    files.insert(
        "keys/allowed-signers".into(),
        (release::ALLOWED_SIGNERS.as_bytes().to_vec(), 0o644),
    );
    write_pack(&root, &files);
    let mut binaries = BTreeMap::new();
    for triple in release::TARGETS {
        let binary = p.scratch.path.join(format!("am-{triple}"));
        release::write_new(&binary, triple.as_bytes(), 0o755).unwrap();
        binaries.insert(triple.to_owned(), binary);
    }
    let inspections = agent_memory::release_builder::preflight(
        &root,
        &binaries,
        &NoExecutableSurface,
        &agent_memory::migration_contract::Bundled,
    )
    .unwrap();
    assert_eq!(inspections.len(), 4);
    let digests: BTreeSet<_> = inspections
        .values()
        .map(|value| value.get("artifact_digest").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(digests.len(), 4);
    let binary = binaries.values().next().unwrap();
    std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        agent_memory::release_builder::preflight(
            &root,
            &binaries,
            &NoExecutableSurface,
            &agent_memory::migration_contract::Bundled
        )
        .is_err()
    );
}

#[cfg(feature = "dev-tools")]
#[test]
fn developer_cli_has_its_providers_and_refuses_a_build_without_binaries() {
    let p = Publisher::new();
    let argv = [
        OsString::from(env!("CARGO_BIN_EXE_am-dev")),
        "release".into(),
        "build".into(),
        "--private-key".into(),
        p.key.as_os_str().to_owned(),
        "--output".into(),
        p.scratch.path.join("output").into_os_string(),
    ];
    let out = release::run_command(&argv, b"", 8192, None, Duration::from_secs(20)).unwrap();
    assert_eq!(out.status, agent_memory::process::Status::Code(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The binary registers the real command surface and migration contract,
    // so the refusal is about this request, not about a missing provider.
    assert!(stderr.starts_with("am release: "), "{stderr}");
    assert!(!stderr.contains("CommandSurface"), "{stderr}");
    assert!(!p.scratch.path.join("output").exists());
}

#[test]
fn portable_metadata_matches_literal_nonempty_lf_declarations_without_normalizing_cr() {
    let p = Publisher::new();
    for (index, (metadata, refusal)) in [
        (
            "metadata: {\"version\":\"0.0.15\"}\nmetadata: fields are described below\n",
            None,
        ),
        ("metadata: {\"version\":\"0.0.15\"}\nmetadata: {}\n", None),
        ("metadata: {\"version\":\"0.0.15\"}", None),
        (
            "metadata: {\"version\":\"0.0.15\"}\r\n",
            Some("Release needs one version declaration"),
        ),
        (
            "metadata: {}\n",
            Some("Release needs one version declaration"),
        ),
        ("metadata: { }\n", Some("Invalid portable release version")),
        (
            "metadata: {\"version\":\"0.0.15\"} \n",
            Some("Release needs one version declaration"),
        ),
        (
            "metadata: {\"version\":\"0.0.15\"}\nmetadata: {broken}\n",
            Some("Release needs one version declaration"),
        ),
        (
            "metadata: {\"version\":\"0.0.15\"}\nmetadata: {\"version\":\"0.0.16\"}\n",
            Some("Release needs one version declaration"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut files = pack_files();
        files.insert("SKILL.md".into(), (metadata.as_bytes().to_vec(), 0o644));
        let root = p.scratch.path.join(format!("metadata-{index}"));
        write_pack(&root, &files);
        let result = release::package_identity(&root);
        match refusal {
            Some(message) => assert_eq!(result.unwrap_err().to_string(), message, "{metadata:?}"),
            None => assert_eq!(result.unwrap().0, "0.0.15", "{metadata:?}"),
        }
    }
}

#[test]
fn terminal_download_failure_preserves_baseline_cli_details_insertion_order() {
    let p = Publisher::new();
    let error = release::download_with_environment(
        "https://example.com/release-manifest.json",
        1024,
        Default::default(),
        &transport_environment(&p, "missing"),
        Duration::from_secs(30),
    )
    .unwrap_err();
    // Public CLI receipts use this insertion-order, indent-2 encoder, not the
    // sorted private diagnostic codec. Timing/path values are intentionally not golden.
    let raw =
        agent_memory::codec::json::encode(&error.details, agent_memory::codec::json::INDENT2_ASCII)
            .unwrap();
    assert!(
        raw.starts_with(concat!(
            "{\n",
            "  \"schema\": \"agent-memory/download-failure-v1\",\n",
            "  \"stage\": \"release manifest\",\n",
            "  \"host\": \"example.com\",\n",
            "  \"attempts\": [\n",
            "    {\n",
            "      \"attempt\": 1,\n",
            "      \"curl_exit\": 22,\n",
            "      \"http_status\": 404,\n",
            "      \"reason\": \"transport\",\n",
            "      \"detail\": \"receive error\",\n",
            "      \"received_bytes\": 8,\n",
            "      \"seconds\": "
        )),
        "{raw}"
    );
    assert!(
        raw.contains("    }\n  ],\n  \"attempt_count\": 1,\n  \"elapsed_seconds\": "),
        "{raw}"
    );
    assert!(raw.contains("\"reason\": \"HTTP rejection\""), "{raw}");
    assert_eq!(
        release::regular(&p.scratch.path.join("count"), 16).unwrap(),
        b"1\n"
    );
    remove_report(&error);
}
