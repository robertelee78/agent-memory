use agent_memory::codec::json::{self, Value};
use agent_memory::omp_installation::{self as omp, Configuration};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

struct World(PathBuf);
impl World {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "am-omp-install-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(agent_memory::fs::canonical_path(&path, true).unwrap())
    }
    fn root(&self) -> String {
        self.0.join("native root").to_str().unwrap().into()
    }
}
impl Drop for World {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn obj(values: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(values.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn set(value: &mut Value, key: &str, next: Value) {
    *value.get_mut(key).unwrap() = next;
}
fn receipt(root: &str, no_guidance: bool) -> Value {
    let selected = omp::binding(root, no_guidance, omp::READER_CAPABILITY).unwrap();
    let digest = s("a".repeat(64));
    let mut targets = vec![
        obj([
            ("path", field(&selected, "skill_target").clone()),
            ("kind", s("copy")),
            ("artifact_digest", digest.clone()),
        ]),
        obj([
            ("path", field(&selected, "extension_target").clone()),
            ("kind", s("wrapper")),
            ("artifact_digest", digest.clone()),
        ]),
    ];
    if !no_guidance {
        targets.push(obj([
            ("path", field(&selected, "activation_target").clone()),
            ("kind", s("activation")),
            ("block_hash", s("b".repeat(64))),
        ]));
    }
    obj([
        ("schema", s("agent-memory/install-receipt-v2")),
        ("presets", s("omp")),
        ("enabled_adapters", Value::Array(vec![s("omp")])),
        ("omp_binding", selected),
        ("no_guidance", Value::Bool(no_guidance)),
        ("installation_id", s("0123456789abcdef0123456789abcdef")),
        (
            "source",
            obj([
                ("artifact_digest", digest),
                ("pack", s("/owned/runtime/pack")),
            ]),
        ),
        ("targets", Value::Array(targets)),
    ])
}
fn error(result: omp::Result<impl Sized>) -> String {
    match result {
        Ok(_) => panic!("operation unexpectedly succeeded"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn canonical_binding_has_exact_saved_targets_and_field_order() {
    let world = World::new();
    let root = world.root();
    let value = omp::binding(&root, false, omp::READER_CAPABILITY).unwrap();
    assert_eq!(
        json::encode(&value, json::COMPACT_ASCII).unwrap(),
        format!(
            "{{\"agent_dir\":\"{root}\",\"skill_target\":\"{root}/skills/agent-memory\",\"extension_target\":\"{root}/extensions/agent-memory.ts\",\"activation_target\":\"{root}/AGENTS.md\",\"peer_reader_capability\":\"omp-stored-source-v2\",\"rollout_attested\":true}}"
        )
    );
    assert!(
        !Path::new(&root).exists(),
        "binding must not create installation paths"
    );
}

#[test]
fn binding_refuses_relative_parent_traversal_and_unknown_capability() {
    let world = World::new();
    for root in ["relative", "/root/../root"] {
        assert_eq!(
            error(omp::binding(root, false, omp::READER_CAPABILITY)),
            "Explicit --omp-agent-dir must be an absolute canonical root"
        );
    }
    assert_eq!(
        error(omp::binding(&world.root(), false, "future-reader")),
        "Unknown native reader capability"
    );
    let value = omp::binding(&world.root(), true, omp::READER_CAPABILITY).unwrap();
    assert_eq!(field(&value, "activation_target"), &Value::Null);
}

#[test]
fn binding_refuses_live_dangling_and_ancestor_links_without_writes() {
    let world = World::new();
    let destination = world.0.join("destination");
    std::fs::create_dir(&destination).unwrap();
    let live = world.0.join("live");
    let dangling = world.0.join("dangling");
    symlink(&destination, &live).unwrap();
    symlink(world.0.join("missing"), &dangling).unwrap();
    for path in [&live, &dangling, &live.join("child")] {
        assert_eq!(
            error(omp::binding(
                path.to_str().unwrap(),
                false,
                omp::READER_CAPABILITY
            )),
            "omp agent root must not contain symlinks"
        );
    }
    assert!(live.is_symlink());
    assert!(dangling.is_symlink());
}

// obligation: tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_legacy_rollout_boolean_cannot_authorize_new_capability
#[test]
fn legacy_rollout_boolean_cannot_authorize_new_capability() {
    let world = World::new();
    let saved = omp::binding(&world.root(), false, omp::HISTORICAL_READER_CAPABILITY).unwrap();
    let old = obj([("omp_binding", saved.clone())]);
    assert_eq!(
        error(omp::configuration(
            &Configuration::default(),
            Some(&old),
            "omp",
            false
        )),
        "Stored-source authority upgrade requires renewed --omp-peers-upgraded for omp-stored-source-v2"
    );
    let args = Configuration {
        omp_peers_upgraded: true,
        ..Configuration::default()
    };
    let renewed = omp::configuration(&args, Some(&old), "omp", false)
        .unwrap()
        .unwrap();
    assert_eq!(
        field(&renewed, "peer_reader_capability"),
        &s("omp-stored-source-v2")
    );
    assert_eq!(field(&renewed, "agent_dir"), field(&saved, "agent_dir"));
    assert_eq!(field(&old, "omp_binding"), &saved);
}

#[test]
fn rust_receipt_reuses_current_binding_without_renewed_attestation() {
    let world = World::new();
    let mut old = receipt(&world.root(), false);
    set(&mut old, "schema", s("agent-memory/install-receipt-v3"));
    let reused = omp::configuration(&Configuration::default(), Some(&old), "omp", false)
        .unwrap()
        .unwrap();
    assert_eq!(&reused, field(&old, "omp_binding"));
    assert_eq!(field(&reused, "rollout_attested"), &Value::Bool(true));
}

#[test]
fn python_receipt_adoption_requires_renewed_attestation() {
    let world = World::new();
    for schema in [
        "agent-memory/install-receipt-v1",
        "agent-memory/install-receipt-v2",
    ] {
        let mut old = receipt(&world.root(), false);
        set(&mut old, "schema", s(schema));
        assert_eq!(
            error(omp::configuration(
                &Configuration::default(),
                Some(&old),
                "omp",
                false
            )),
            "Native omp requires --presets omp, --omp-agent-dir and --omp-peers-upgraded after quiescing/upgrading bank peers"
        );
        let args = Configuration {
            omp_peers_upgraded: true,
            ..Configuration::default()
        };
        let renewed = omp::configuration(&args, Some(&old), "omp", false)
            .unwrap()
            .unwrap();
        assert_eq!(&renewed, field(&old, "omp_binding"));
        assert_eq!(field(&renewed, "rollout_attested"), &Value::Bool(true));
    }
}

#[test]
fn rust_receipt_with_stale_capability_requires_renewed_attestation() {
    let world = World::new();
    let mut old = receipt(&world.root(), false);
    set(&mut old, "schema", s("agent-memory/install-receipt-v3"));
    set(
        &mut old,
        "omp_binding",
        omp::binding(&world.root(), false, omp::HISTORICAL_READER_CAPABILITY).unwrap(),
    );
    assert_eq!(
        error(omp::configuration(
            &Configuration::default(),
            Some(&old),
            "omp",
            false
        )),
        "Stored-source authority upgrade requires renewed --omp-peers-upgraded for omp-stored-source-v2"
    );
    let args = Configuration {
        omp_peers_upgraded: true,
        ..Configuration::default()
    };
    let renewed = omp::configuration(&args, Some(&old), "omp", false)
        .unwrap()
        .unwrap();
    assert_eq!(
        field(&renewed, "peer_reader_capability"),
        &s(omp::READER_CAPABILITY)
    );
    assert_eq!(field(&renewed, "rollout_attested"), &Value::Bool(true));
}

#[test]
fn selection_and_explicit_root_do_not_adopt_or_remove_saved_ownership() {
    let world = World::new();
    let root = world.root();
    let old = receipt(&root, false);
    assert_eq!(
        error(omp::configuration(
            &Configuration::default(),
            Some(&old),
            "all",
            false
        )),
        "Removing saved omp selection requires explicit owned uninstall/reinstall; all excludes omp"
    );
    let args = Configuration {
        omp_agent_dir: Some("/different/root"),
        omp_peers_upgraded: true,
        omp_layout_quiescence: &[],
    };
    assert_eq!(
        error(omp::configuration(&args, Some(&old), "omp", false)),
        "omp root differs from saved ownership; uninstall/reinstall explicitly"
    );
    let mut changed = old.clone();
    set(
        changed.get_mut("omp_binding").unwrap(),
        "extension_target",
        s("/foreign/extension.ts"),
    );
    let args = Configuration {
        omp_peers_upgraded: true,
        ..Configuration::default()
    };
    assert_eq!(
        error(omp::configuration(&args, Some(&changed), "omp", false)),
        "omp saved binding changed; preserve owned targets"
    );
    assert_eq!(
        error(omp::configuration(&args, Some(&old), "omp", true)),
        "omp saved binding changed; preserve owned targets"
    );
}

#[test]
fn nonnative_selection_refuses_each_native_only_argument() {
    for args in [
        Configuration {
            omp_agent_dir: Some("/explicit"),
            ..Configuration::default()
        },
        Configuration {
            omp_peers_upgraded: true,
            ..Configuration::default()
        },
        Configuration {
            omp_layout_quiescence: &[PathBuf::from("/attestation.json")],
            ..Configuration::default()
        },
    ] {
        assert_eq!(
            error(omp::configuration(&args, None, "all", false)),
            "omp root/rollout attestation requires an explicit omp preset"
        );
    }
    assert_eq!(
        omp::configuration(&Configuration::default(), None, "codex,claude", false).unwrap(),
        None
    );
}

#[test]
fn initial_native_configuration_requires_root_and_explicit_renewal() {
    let world = World::new();
    let root = world.root();
    for args in [
        Configuration::default(),
        Configuration {
            omp_agent_dir: Some(&root),
            ..Configuration::default()
        },
        Configuration {
            omp_peers_upgraded: true,
            ..Configuration::default()
        },
    ] {
        assert_eq!(
            error(omp::configuration(&args, None, "omp", false)),
            "Native omp requires --presets omp, --omp-agent-dir and --omp-peers-upgraded after quiescing/upgrading bank peers"
        );
    }
    let args = Configuration {
        omp_agent_dir: Some(&root),
        omp_peers_upgraded: true,
        omp_layout_quiescence: &[
            PathBuf::from("/proof.json"),
            PathBuf::from("/second-proof.json"),
        ],
    };
    let value = omp::configuration(&args, None, "codex,omp", true)
        .unwrap()
        .unwrap();
    assert_eq!(field(&value, "activation_target"), &Value::Null);
}

#[test]
fn valid_receipts_keep_native_and_nonnative_schema_distinction() {
    let world = World::new();
    for no_guidance in [false, true] {
        let mut value = receipt(&world.root(), no_guidance);
        omp::validate_receipt(&value).unwrap();
        let mut targets = field(&value, "targets").as_array().unwrap().to_vec();
        set(&mut targets[0], "kind", s("link"));
        set(&mut value, "targets", Value::Array(targets));
        omp::validate_receipt(&value).unwrap();
    }
    omp::validate_receipt(&obj([("schema", s("agent-memory/install-receipt-v1"))])).unwrap();
    assert_eq!(
        error(omp::validate_receipt(&obj([(
            "schema",
            s("agent-memory/install-receipt-v2")
        )]))),
        "Non-omp installs retain receipt-v1"
    );
    assert_eq!(
        error(omp::validate_receipt(&obj([(
            "schema",
            s("future-receipt")
        )]))),
        "Unknown installation receipt schema"
    );
}

#[test]
fn receipt_selection_adapter_and_binding_shape_refuse_independently() {
    let world = World::new();
    let original = receipt(&world.root(), false);
    for (field, changed, expected) in [
        ("presets", s("all"), "omp preset and receipt binding differ"),
        (
            "enabled_adapters",
            Value::Array(vec![]),
            "Native enabled adapter differs from ownership binding",
        ),
        (
            "schema",
            s("agent-memory/install-receipt-v1"),
            "Malformed native ownership receipt",
        ),
        (
            "omp_binding",
            Value::Bool(true),
            "Malformed native ownership receipt",
        ),
    ] {
        let mut value = original.clone();
        set(&mut value, field, changed);
        assert_eq!(error(omp::validate_receipt(&value)), expected);
    }
    let mut value = original.clone();
    let mut pairs = field(&value, "omp_binding").as_object().unwrap().to_vec();
    pairs.push(("unowned_extra".into(), Value::Bool(true)));
    set(&mut value, "omp_binding", Value::Object(pairs));
    assert_eq!(
        error(omp::validate_receipt(&value)),
        "Malformed native ownership receipt"
    );
}

#[test]
fn receipt_rollout_identity_and_canonical_targets_are_checked() {
    let world = World::new();
    let original = receipt(&world.root(), false);
    for bad in [Value::Bool(false), Value::Int(1.into()), s("true")] {
        let mut value = original.clone();
        set(
            value.get_mut("omp_binding").unwrap(),
            "rollout_attested",
            bad,
        );
        assert_eq!(
            error(omp::validate_receipt(&value)),
            "Invalid native rollout/installation identity"
        );
    }
    for bad in [
        "A123456789abcdef0123456789abcdef",
        "0123456789abcdef0123456789abcde",
        "01234567-89ab-cdef-0123-456789abcdef",
    ] {
        let mut value = original.clone();
        set(&mut value, "installation_id", s(bad));
        assert_eq!(
            error(omp::validate_receipt(&value)),
            "Invalid native rollout/installation identity"
        );
    }
    let mut value = original;
    set(
        value.get_mut("omp_binding").unwrap(),
        "skill_target",
        s("/foreign/skill"),
    );
    assert_eq!(
        error(omp::validate_receipt(&value)),
        "Native ownership targets differ from canonical saved binding"
    );
}

#[test]
fn every_required_owned_target_is_present_with_exact_kind_and_artifact() {
    let world = World::new();
    let original = receipt(&world.root(), false);
    for index in 0..3 {
        let mut value = original.clone();
        let mut owned = field(&value, "targets").as_array().unwrap().to_vec();
        owned.remove(index);
        set(&mut value, "targets", Value::Array(owned));
        assert_eq!(
            error(omp::validate_receipt(&value)),
            "Native target is absent from ownership receipt"
        );
    }
    for (index, key, changed, expected) in [
        (
            0,
            "kind",
            s("wrapper"),
            "Native skill target differs from installed artifact",
        ),
        (
            0,
            "artifact_digest",
            s("wrong"),
            "Native skill target differs from installed artifact",
        ),
        (
            1,
            "kind",
            s("copy"),
            "Native extension artifact identity differs from owned runtime",
        ),
        (
            1,
            "artifact_digest",
            s("wrong"),
            "Native extension artifact identity differs from owned runtime",
        ),
        (
            2,
            "kind",
            s("wrapper"),
            "Native activation target differs from owned Markdown block",
        ),
    ] {
        let mut value = original.clone();
        let mut owned = field(&value, "targets").as_array().unwrap().to_vec();
        set(&mut owned[index], key, changed);
        set(&mut value, "targets", Value::Array(owned));
        assert_eq!(error(omp::validate_receipt(&value)), expected);
    }
}

#[test]
fn wrapper_bytes_bind_artifact_root_and_current_reader_without_python() {
    let value = obj([
        ("agent_dir", s("/native \"root\"")),
        ("peer_reader_capability", s(omp::READER_CAPABILITY)),
    ]);
    let actual = omp::extension_wrapper(
        Path::new("/owned pack"),
        Path::new("/owned runtime/bin/am"),
        &"a".repeat(64),
        &value,
    )
    .unwrap();
    let expected = concat!(
        "// Receipt-owned native executable. Never edit provider or approval settings.\n",
        "import nativeMemory from \"/owned pack/hooks/omp.ts\";\n",
        "export default pi => nativeMemory(pi, {\"artifact\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"executable\":\"/owned runtime/bin/am\",\"agentDir\":\"/native \\\"root\\\"\",\"readerCapability\":\"omp-stored-source-v2\",\"rolloutAttested\":true});\n"
    );
    assert_eq!(actual, expected.as_bytes());
    assert!(!actual.windows(6).any(|b| b == b"python"));
    assert_eq!(
        error(omp::extension_wrapper(
            Path::new("/pack"),
            Path::new("relative/am"),
            &"a".repeat(64),
            &value
        )),
        "Native extension executable must be an absolute canonical path"
    );
}

#[test]
fn preview_never_claims_native_loading_assessment_or_publication() {
    let value = omp::preview(&Value::Null);
    assert_eq!(field(&value, "binding"), &Value::Null);
    assert_eq!(
        field(&value, "qualification"),
        &s(
            "disk ownership is not native loading, source completion or platform/provider acceptance"
        )
    );
    assert_eq!(
        field(&value, "executable_authority"),
        &s("stored native source observation and bounded advisory wakes only")
    );
    assert_eq!(value.as_object().unwrap().len(), 6);
}

#[test]
fn absent_registration_is_typed_and_never_fabricates_ownership() {
    assert!(matches!(
        omp::observer_binding(&"a".repeat(64), "/native"),
        Err(omp::Error::MissingActivation)
    ));
    assert!(matches!(
        omp::verified_observer_binding(&"a".repeat(64), "/native", None),
        Err(omp::Error::MissingActivation)
    ));
    assert!(matches!(
        omp::owned_manual(&Value::Null, "SKILL.md"),
        Err(omp::Error::MissingActivation)
    ));
    let world = World::new();
    assert!(matches!(
        omp::diagnostics(&world.0, Some(&receipt(&world.root(), false))),
        Err(omp::Error::MissingActivation)
    ));
    assert!(!world.0.join("receipt.json").exists());
}

#[test]
fn not_installed_diagnostics_remain_read_only_without_runtime_registration() {
    let world = World::new();
    let before = std::fs::read_dir(&world.0).unwrap().count();
    let expected = "{\"binding\":\"not-installed\",\"native_loading\":\"not-observed\",\"source_assessment\":\"not-inferred\",\"publication\":\"not-inferred\"}";
    for value in [
        None,
        Some(&Value::Null),
        Some(&obj([("schema", s("agent-memory/install-receipt-v1"))])),
    ] {
        assert_eq!(
            json::encode(
                &omp::diagnostics(&world.0, value).unwrap(),
                json::COMPACT_ASCII
            )
            .unwrap(),
            expected
        );
    }
    assert_eq!(std::fs::read_dir(&world.0).unwrap().count(), before);
}

#[test]
fn artifact_digest_includes_binary_bytes_and_required_mode() {
    let files = obj([(
        "SKILL.md",
        obj([("sha256", s("0".repeat(64))), ("mode", s("0o644"))]),
    )]);
    let (inventory, first) =
        agent_memory::artifact::runtime_digest(&files, &"a".repeat(64), 0o755).unwrap();
    assert_eq!(field(field(&inventory, "bin/am"), "mode"), &s("0o755"));
    // Independent golden: SHA256 of the baseline sorted compact ASCII inventory.
    assert_eq!(
        first,
        "f737278517e7584b393ac341546bb8519363d958b7afe5f55b298b99b9812c6e"
    );
    let (_, swapped) =
        agent_memory::artifact::runtime_digest(&files, &"b".repeat(64), 0o755).unwrap();
    assert_ne!(first, swapped);
    assert!(agent_memory::artifact::runtime_digest(&files, &"a".repeat(64), 0o644).is_err());
    assert!(agent_memory::artifact::runtime_digest(&inventory, &"a".repeat(64), 0o755).is_err());
}

#[test]
fn private_test_roots_are_canonical_and_owner_only() {
    let world = World::new();
    assert_eq!(
        agent_memory::fs::canonical_path(&world.0, true).unwrap(),
        world.0
    );
    assert_eq!(
        std::fs::metadata(&world.0).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

/// The receipt-owned adapter's runtime gate admits exactly the Rust pin, and
/// its stock 18.20.1 `session_clear` handler captures the stored reset boundary
/// before dropping the own-guidance request that `/clear` discarded.
#[test]
fn adapter_gates_on_the_single_supported_version_and_handles_session_clear() {
    let adapter = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory/hooks/omp.ts"),
    )
    .unwrap();
    assert_eq!(omp::SUPPORTED_VERSION, "18.20.1");
    let gate = format!(
        "const supported = () => pi.pi.VERSION === '{}' &&",
        omp::SUPPORTED_VERSION
    );
    assert!(adapter.contains(&gate));
    assert_eq!(adapter.matches("pi.pi.VERSION").count(), 1);
    let clear = adapter
        .split("pi.on('session_clear', (_event, ctx) => {\n")
        .nth(1)
        .and_then(|rest| rest.split("\n  });\n").next())
        .unwrap();
    assert_eq!(
        clear,
        "    const current = capture(ctx, 'ingest'); if (!current) return;\n    cancel(current.state); current.state.request = undefined;"
    );
    assert_eq!(adapter.matches("pi.on('session_clear'").count(), 1);
}
