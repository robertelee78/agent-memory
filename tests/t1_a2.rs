// Tier 1 A2 baseline contract ports. Integration verification belongs to Main.

// Lane rule "no panic on input": the parsers below receive repository, pack
// and proposal text. Deterministic generated inputs mix every delimiter the
// parsers slice on with multibyte scalars and Python line separators; any
// panic fails the test, refusals are expected.
mod malformed_input_contract {
    use agent_memory::codec::json::{self, DecodePolicy, Value};
    use agent_memory::{handoff, pack_validation as pack, records, repository_config, scopes};

    const ALPHABET: &[&str] = &[
        "[",
        "]",
        "(",
        ")",
        "<",
        ">",
        "`",
        "``",
        "```",
        "~~~",
        "#",
        "## ",
        "!",
        "\n",
        "\r",
        "\r\n",
        " ",
        "    ",
        "\t",
        "a",
        "é",
        "𝄞",
        "\u{2028}",
        "\u{85}",
        "\u{1c}",
        "\"",
        "'",
        ":",
        "/",
        "../",
        ".",
        "*",
        "MUST",
        " read ",
        "&",
        ";",
        "&#",
        "x",
        "\\",
        "|",
        "-",
        "=",
        "$",
        "<a href=\"",
        "</a>",
        "http://",
        "[x]:",
        "GUIDE.md",
        "%",
        "?",
        "@",
        "v1.",
        "{",
        "}",
        ",",
        "0",
        "9",
    ];

    struct Generator(u64);
    impl Generator {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn text(&mut self) -> String {
            let length = (self.next() % 40) as usize;
            (0..length)
                .map(|_| ALPHABET[(self.next() as usize) % ALPHABET.len()])
                .collect()
        }
    }

    fn files() -> pack::Inventory {
        ["SKILL.md", "bank/GUIDE.md", "x/GUIDE.md"]
            .into_iter()
            .map(|s| (s.to_string(), (Vec::new(), 0o600)))
            .collect()
    }

    #[test]
    fn generated_markdown_and_reference_text_never_panics() {
        let files = files();
        let mut generator = Generator(0x5eed_a2f1);
        for _ in 0..100_000 {
            let text = generator.text();
            let _ = pack::markdown(&text);
            let _ = pack::anchors(&text);
            let references = pack::reference_definitions(&text).ok();
            let _ = pack::link_occurrences(&text, references.as_ref());
            let _ = pack::links(&text, None);
            let _ = pack::mandatory_targets(&text, "bank/GUIDE.md", &files, references.as_ref());
            let _ = pack::destination("bank/GUIDE.md", &text, &files);
            let _ = pack::command_argv(&text, "SKILL.md");
            let _ = pack::semantic_version(&Value::String(text.clone()));
            let _ = handoff::reference_path(&text);
            let _ = handoff::managed_cues(&text, &["Checkpoint evidence".to_string()]);
            let _ = handoff::json_section(&text);
            let _ = repository_config::router_kind(text.as_bytes());
            let _ = repository_config::validate_branch(&text);
            let _ = records::parse(&text, &[]);
            let _ = scopes::parse_scope(&Value::String(text));
        }
    }

    #[test]
    fn malformed_json_documents_refuse_without_panicking() {
        let mut generator = Generator(0x00a2_f1c5);
        let decode = |text: &str| json::decode(text, DecodePolicy::default());
        let shapes = [
            r#"{"bank_project_id":"123e4567-e89b-42d3-a456-426614174000","project_id":"123e4567-e89b-42d3-a456-426614174000","scope":S,"conditions":C,"context":C}"#,
            r#"{"fact":S,"fact_key":S,"project_id":S,"memory":C,"sources":[C],"detecting_developer_id":S}"#,
            r#"{"schema":"agent-memory/checkpoint-evidence-v1","references":[C],"observed_checkout":C,"publication_verified":false,"identity_authenticated":false}"#,
        ];
        let values = [
            "null",
            "1",
            "-0",
            "1e999",
            "\"\"",
            "\"x\"",
            "[]",
            "{}",
            "[[[]]]",
            r#"{"component":"a/b"}"#,
            r#"{"branch":null}"#,
            "true",
        ];
        for _ in 0..3_000 {
            for shape in shapes {
                let pick = |g: &mut Generator| values[(g.next() as usize) % values.len()];
                let text = shape
                    .replacen('S', pick(&mut generator), 1)
                    .replacen('C', pick(&mut generator), 1)
                    .replacen('C', pick(&mut generator), 1)
                    .replace('S', "\"s\"")
                    .replace('C', "{}");
                let Ok(value) = decode(&text) else {
                    continue;
                };
                let _ = scopes::decide(&value);
                let _ = agent_memory::authority::assess(&value);
                let _ = handoff::validate_observation(&value);
                let _ = scopes::command_decide(Ok(value.clone()));
                let _ = agent_memory::authority::command_assess(Ok(value));
            }
        }
    }
}

mod pack_contract {
    use agent_memory::{artifact, codec::json::Value, pack_validation as pack};
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::path::PathBuf;
    struct PackDir(PathBuf);
    impl PackDir {
        fn new() -> Self {
            let root = PathBuf::from(format!(
                "/tmp/am-t1-a2-pack-{}",
                agent_memory::host::uuid4().unwrap()
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&root)
                .unwrap();
            Self(root.canonicalize().unwrap())
        }
        fn write(&self, path: &str, body: impl AsRef<[u8]>) {
            let path = self.0.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        fn fixture() -> Self {
            let root = Self::new();
            root.write("SKILL.md","---\nname: agent-memory\ndescription: Bounded portable contract fixture\nmetadata: {\"version\":\"0.0.14\"}\n---\n\n## Route\n| Inspect | [bank](bank/GUIDE.md) | [inspect](inspect/GUIDE.md) |\n");
            root.write(
                "bank/GUIDE.md",
                "---\nkind: domain\n---\n\n[Inspect](../inspect/GUIDE.md).\n",
            );
            let mut operation = String::from(
                "---\nkind: operation\ndomain: bank\nfile-operations: [\"inspect\"]\n---\n\n",
            );
            for section in pack::SECTIONS {
                operation.push_str(&format!("## {section}\n\n"));
                if section == "Required context" {
                    operation.push_str("A session MUST first load and follow the [bank domain](../bank/GUIDE.md).\n\n");
                } else {
                    operation.push_str("Explicit operation context.\n\n");
                }
            }
            root.write("inspect/GUIDE.md", operation);
            root
        }
        fn append(&self, path: &str, body: &str) {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(self.0.join(path))
                .unwrap();
            file.write_all(body.as_bytes()).unwrap();
        }
    }
    impl Drop for PackDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn link_files() -> pack::Inventory {
        ["capture/GUIDE.md", "checkpoint/GUIDE.md", "bank/GUIDE.md"]
            .into_iter()
            .map(|s| (s.to_string(), (Vec::new(), 0o600)))
            .collect()
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_empty_literal_argument_is_data_not_a_hidden_shell_operator
    #[test]
    fn empty_shell_literal_is_data() {
        assert_eq!(
            pack::command_argv("git config example.note \"\"", "SKILL.md").unwrap(),
            vec!["git", "config", "example.note", ""]
        );
        let root = PackDir::fixture();
        root.append("SKILL.md", "\n```sh\ngit config example.note \"\"\n```\n");
        assert_eq!(
            pack::validate(&root.0).unwrap().get("valid"),
            Some(&Value::Bool(true))
        );
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_malformed_routes_links_frontmatter_and_leaf_contracts_are_refused
    #[test]
    fn shell_expansions_and_hidden_operations_refuse() {
        for command in [
            "git config x y && git push main",
            "git config x y |& git push main",
            "git config x \"$(git push main)\"",
            "git -C $EXTRA_ARGS config x y",
            "git -C * config x y",
            "git config x y;git push main",
        ] {
            assert_eq!(
                pack::command_argv(command, "SKILL.md").unwrap_err().code,
                "dispatch",
                "{command}"
            );
        }
        for version in ["broken-version", "0.0.1-..dev", "0.0.1-01", "1\u{662}.0.0"] {
            assert!(!pack::semantic_version(&Value::String(version.into())));
        }
        for version in [
            "0.0.1",
            "1.2.3-alpha.1+build.01",
            "100000000000000000000.0.0",
        ] {
            assert!(pack::semantic_version(&Value::String(version.into())));
        }
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_internal_symlink_is_refused_without_reading_its_target
    #[test]
    fn pack_internal_symlink_never_reads_target() {
        let root = PackDir::fixture();
        let outside = PackDir::new();
        outside.write("private", "PRIVATE_CANARY");
        std::os::unix::fs::symlink(outside.0.join("private"), root.0.join("link")).unwrap();
        let error = pack::inventory(&root.0).unwrap_err();
        assert_eq!(error.code, "file-type");
        assert_eq!(
            std::fs::read(outside.0.join("private")).unwrap(),
            b"PRIVATE_CANARY"
        );
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_unreadable_directory_cannot_hide_a_second_registered_skill
    #[test]
    fn unreadable_subtree_cannot_hide_registration() {
        let root = PackDir::fixture();
        root.write("hidden/SKILL.md", "second");
        let hidden = root.0.join("hidden");
        std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0)).unwrap();
        let error = pack::validate(&root.0).unwrap_err();
        assert_eq!(error.code, "asset-mode");
        std::fs::set_permissions(hidden, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(pack::validate(&root.0).unwrap_err().code, "registration");
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_fenced_project_examples_are_data_and_real_reference_and_duplicate_heading_anchors_work
    #[test]
    fn markdown_fences_references_and_heading_anchors() {
        let text = "````markdown\n```\n[project-only](../absent.md)\n````\n\n### Repeated\n\n### Repeated\n\n[second](#repeated-1)\n\n[the recall domain][recall-alias]\n\n[recall-alias]: recall/GUIDE.md\n";
        let (prose, shell) = pack::markdown(text).unwrap();
        assert!(shell.is_empty());
        assert!(!prose.contains("project-only"));
        assert!(pack::anchors(&prose).contains("repeated-1"));
        assert_eq!(
            pack::links(&prose, None).unwrap().last().unwrap().1,
            "recall/GUIDE.md"
        );
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_supported_reference_and_html_links_cannot_hide_a_required_load_cycle
    #[test]
    fn mandatory_reference_forms_share_one_resolver() {
        let files = link_files();
        for directive in [
            "A session MUST read [checkpoint][cp].\n\n[cp]: ../checkpoint/GUIDE.md",
            "A session MUST read [checkpoint][].\n\n[checkpoint]: ../checkpoint/GUIDE.md",
            "A session MUST read [checkpoint].\n\n[checkpoint]: ../checkpoint/GUIDE.md",
            "A session MUST read <a href=\"../checkpoint/GUIDE.md\">checkpoint</a>.",
        ] {
            assert!(
                pack::mandatory_targets(directive, "capture/GUIDE.md", &files, None)
                    .unwrap()
                    .contains("checkpoint/GUIDE.md"),
                "{directive}"
            );
        }
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_coordinated_objects_and_later_conditions_preserve_the_initial_required_load
    #[test]
    fn coordinated_targets_and_late_conditions_preserve_required_edges() {
        let files = link_files();
        for directive in [
            "A session MUST read [bank](../bank/GUIDE.md) and [checkpoint](../checkpoint/GUIDE.md).",
            "A session MUST read [checkpoint](../checkpoint/GUIDE.md) and [bank](../bank/GUIDE.md).",
            "A session MUST read [checkpoint](../checkpoint/GUIDE.md), and if a source is uncertain it MUST preserve evidence.",
        ] {
            assert!(
                pack::mandatory_targets(directive, "capture/GUIDE.md", &files, None)
                    .unwrap()
                    .contains("checkpoint/GUIDE.md"),
                "{directive}"
            );
        }
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_modifiers_and_purpose_words_cannot_hide_required_load_cycles
    #[test]
    fn modifiers_and_purpose_do_not_guard_loading() {
        let files = link_files();
        for directive in [
            "A session MUST always read [checkpoint](../checkpoint/GUIDE.md).",
            "A session MUST preload [checkpoint](../checkpoint/GUIDE.md) for completeness.",
            "A session MUST read [checkpoint](../checkpoint/GUIDE.md) for complete context only.",
            "A session MUST read [checkpoint](../checkpoint/GUIDE.md) at all times.",
        ] {
            assert!(
                pack::mandatory_targets(directive, "capture/GUIDE.md", &files, None)
                    .unwrap()
                    .contains("checkpoint/GUIDE.md"),
                "{directive}"
            );
        }
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_purpose_clauses_cannot_hide_an_unconditional_loading_cycle
    #[test]
    fn purpose_clauses_keep_required_edges() {
        let files = link_files();
        for purpose in [
            " for completeness",
            " for complete context",
            "\nfor completeness",
            " to complete this operation",
        ] {
            let directive =
                format!("A session MUST read [checkpoint](../checkpoint/GUIDE.md){purpose}.");
            assert!(
                pack::mandatory_targets(&directive, "capture/GUIDE.md", &files, None)
                    .unwrap()
                    .contains("checkpoint/GUIDE.md")
            );
        }
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_mandatory_loading_cycles_fail_but_conditional_navigation_remains_valid
    #[test]
    fn unconditional_cycles_refuse_while_guarded_navigation_remains_valid() {
        let root = PackDir::fixture();
        root.append(
            "bank/GUIDE.md",
            "\nA session MUST read [inspect](../inspect/GUIDE.md) when inspecting.\n",
        );
        assert!(pack::validate(&root.0).is_ok());
        root.append(
            "bank/GUIDE.md",
            "\nA session MUST read [inspect](../inspect/GUIDE.md).\n",
        );
        assert_eq!(pack::validate(&root.0).unwrap_err().code, "loading-cycle");
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_literal_marker_words_are_prose_and_cannot_index_internal_link_state
    #[test]
    fn literal_marker_words_never_index_link_state() {
        let files = link_files();
        assert!(
            pack::mandatory_targets(
                "A session MUST read GUIDELINK999.",
                "capture/GUIDE.md",
                &files,
                None
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            pack::mandatory_targets(
                "A session MUST read \0L999\0.",
                "capture/GUIDE.md",
                &files,
                None
            )
            .unwrap_err()
            .code,
            "markdown"
        );
    }

    // obligation: tests/contracts/test_skill_routes.py::SkillRouteContract.test_candidate_code_is_never_imported_or_executed_by_static_validation
    #[test]
    fn candidate_code_is_data_and_missing_signed_surface_refuses() {
        let root = PackDir::fixture();
        root.write("scripts/checkpoint.py",format!("from pathlib import Path\nPath({:?}).write_text('executed')\nraise RuntimeError('candidate code')\n",root.0.join("must-not-exist")));
        assert!(pack::validate(&root.0).is_ok());
        root.append("SKILL.md", "\n```sh\nam checkpoint inspect\n```\n");
        assert_eq!(
            pack::validate_with_surface(&root.0, None).unwrap_err().code,
            "command-surface-unregistered"
        );
        assert!(!root.0.join("must-not-exist").exists());
    }

    // obligation: tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_repeated_inspection_rechecks_changed_source_modes_and_topology
    #[test]
    fn fresh_inventory_precedes_every_static_memo_hit() {
        let root = PackDir::fixture();
        let mut result = pack::validate(&root.0).unwrap();
        *result.get_mut("files").unwrap() = Value::Object(vec![]);
        assert!(
            !pack::validate(&root.0)
                .unwrap()
                .get("files")
                .unwrap()
                .as_object()
                .unwrap()
                .is_empty()
        );
        root.write("unreachable.md", "# Orphan\n");
        assert_eq!(pack::validate(&root.0).unwrap_err().code, "reachability");
    }

    // obligation: tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_change_after_declaration_read_refuses_cold_and_cached_inspections
    #[test]
    fn complete_inspection_is_bracketed_by_fresh_inventory() {
        for _cached in [false, true] {
            let root = PackDir::fixture();
            pack::validate(&root.0).unwrap();
            let error = pack::with_validated(&root.0, |_, _| {
                root.write("arrival.txt", b"new");
                Ok(())
            })
            .unwrap_err();
            assert_eq!(error.code, "changed-pack");
        }
    }

    // obligation: tests/contracts/test_portable_artifact.py::PortableArtifactContract.test_missing_or_false_runtime_declarations_are_refused_without_modifying_the_copy
    #[test]
    fn artifact_declarations_bind_actual_bytes_and_require_migration_registration() {
        let root = PackDir::fixture();
        root.write("asset.json", b"{}");
        let files = Value::Object(vec![(
            "asset.json".into(),
            Value::Object(vec![(
                "sha256".into(),
                Value::String(
                    "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a".into(),
                ),
            )]),
        )]);
        assert_eq!(
            artifact::declaration(&root.0, &Value::String("asset.json".into()), &files).unwrap(),
            Value::Object(vec![])
        );
        root.write("asset.json", b"[]");
        assert_eq!(
            artifact::declaration(&root.0, &Value::String("asset.json".into()), &files)
                .unwrap_err()
                .message(),
            "Declaration is oversized or changed after package validation"
        );
        root.write(
            "schemas/runtime-compatibility.json",
            include_str!("../skills/agent-memory/schemas/runtime-compatibility.json"),
        );
        assert!(
            artifact::inspect_with(&root.0, None, None)
                .unwrap_err()
                .detail
                .contains("Migration contract provider is not registered")
        );
    }

    #[test]
    fn inventory_has_exact_known_digest_and_reader_limits() {
        let root = PackDir::new();
        root.write("abc", b"abc");
        let files = pack::inventory(&root.0).unwrap();
        assert_eq!(files["abc"].1, 0o600);
        root.write("large", vec![b'x'; 2097153]);
        assert_eq!(
            pack::inventory(&root.0).unwrap_err().code,
            "inventory-bound"
        );
    }

    fn manifest_entry(sha256: &str, mode: &str) -> Value {
        Value::Object(vec![
            ("sha256".into(), Value::String(sha256.into())),
            ("mode".into(), Value::String(mode.into())),
        ])
    }

    // Expected digests were computed by hand with `shasum -a 256` over the
    // canonical JSON `json.dumps(files, sort_keys=True, separators=(',', ':'))`:
    // {"SKILL.md":{"mode":"0o600","sha256":"ba78…15ad"},"bin/am":{"mode":"0o755","sha256":"5983…f40d"}}
    #[test]
    fn runtime_digest_binds_binary_entry_with_documented_encoding() {
        let skill = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let binary = "59837d1683c7bf595a40db1c4f0ff0663354b24483d5124f11072fec7fe7f40d";
        let files = Value::Object(vec![("SKILL.md".into(), manifest_entry(skill, "0o600"))]);
        let (combined, digest) = artifact::runtime_digest(&files, binary, 0o755).unwrap();
        assert_eq!(
            digest,
            "022501c3aec807a7dd5a986c39e4527c8e03c34fba090b9dd77e503d555c2bd5"
        );
        assert_eq!(
            combined,
            Value::Object(vec![
                ("SKILL.md".into(), manifest_entry(skill, "0o600")),
                (
                    artifact::BINARY_ENTRY.into(),
                    manifest_entry(binary, "0o755")
                ),
            ])
        );
        let changed = "0".repeat(64);
        assert_ne!(
            artifact::runtime_digest(&files, &changed, 0o755).unwrap().1,
            digest
        );
        for mode in [0o700, 0o775, 0o4755] {
            assert_eq!(
                artifact::runtime_digest(&files, binary, mode)
                    .unwrap_err()
                    .code,
                "runtime-binary"
            );
        }
        for bad in ["", "ABC", &binary.to_uppercase(), &binary[..63]] {
            assert_eq!(
                artifact::runtime_digest(&files, bad, 0o755)
                    .unwrap_err()
                    .code,
                "runtime-binary"
            );
        }
        for path in [artifact::BINARY_ENTRY, "bin/agent-memory"] {
            let mut with_bin = files.clone();
            let Value::Object(entries) = &mut with_bin else {
                panic!("manifest object")
            };
            entries.push((path.into(), manifest_entry(binary, "0o755")));
            assert_eq!(
                artifact::runtime_digest(&with_bin, binary, 0o755)
                    .unwrap_err()
                    .code,
                "runtime-binary"
            );
        }
        assert!(artifact::runtime_digest(&Value::Array(vec![]), binary, 0o755).is_err());
    }

    struct FixtureCatalog;
    impl artifact::MigrationContract for FixtureCatalog {
        fn inspect(
            &self,
            _root: &std::path::Path,
            _compatibility: &Value,
            _files: &Value,
        ) -> agent_memory::error::Result<Value> {
            Ok(Value::Array(vec![]))
        }
    }

    fn artifact_fixture() -> PackDir {
        let root = PackDir::fixture();
        root.write(
            "schemas/runtime-compatibility.json",
            r#"{"schema":"agent-memory/runtime-compatibility-v1","readable_bank_schemas":["agent-memory/v1"],"migrator_source_schemas":[],"python_minimum":"3.11","git_required_for":["git-sharing"],"required_python_packages":[],"required_agent_clients":[],"optional_adapters":[]}"#,
        );
        root.write(
            "schemas/bank-router.json",
            r#"{"properties":{"schema":{"const":"agent-memory/v1"}}}"#,
        );
        root
    }

    fn write_binary(dir: &PackDir, body: &[u8], mode: u32) -> PathBuf {
        let path = dir.0.join("am");
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn inspect_runtime_binds_binary_bytes_and_keeps_pack_digest() {
        let pack = artifact_fixture();
        let outside = PackDir::new();
        let binary = write_binary(&outside, b"runtime binary", 0o755);
        let catalog = FixtureCatalog;
        let pack_only = artifact::inspect_with(&pack.0, None, Some(&catalog)).unwrap();
        let runtime =
            artifact::inspect_runtime_with(&pack.0, &binary, None, Some(&catalog)).unwrap();
        let pack_files = pack_only.get("files").unwrap();
        let (expected_files, expected_digest) = artifact::runtime_digest(
            pack_files,
            "59837d1683c7bf595a40db1c4f0ff0663354b24483d5124f11072fec7fe7f40d",
            0o755,
        )
        .unwrap();
        assert_eq!(runtime.get("files"), Some(&expected_files));
        assert_eq!(
            runtime.get("artifact_digest"),
            Some(&Value::String(expected_digest.clone()))
        );
        assert_ne!(
            runtime.get("artifact_digest"),
            pack_only.get("artifact_digest")
        );
        let pack_count = pack_files.as_object().unwrap().len();
        assert_eq!(
            runtime.get("file_count"),
            Some(&Value::Int((pack_count + 1).into()))
        );
        let again = artifact::inspect_with(&pack.0, None, Some(&catalog)).unwrap();
        assert_eq!(
            again.get("artifact_digest"),
            pack_only.get("artifact_digest")
        );
        let keys = |value: &Value| -> Vec<String> {
            value
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, _)| k.clone())
                .collect()
        };
        assert_eq!(keys(&runtime), keys(&pack_only));

        write_binary(&outside, b"swapped binary", 0o755);
        let swapped =
            artifact::inspect_runtime_with(&pack.0, &binary, None, Some(&catalog)).unwrap();
        assert_ne!(
            swapped.get("artifact_digest"),
            Some(&Value::String(expected_digest))
        );
        write_binary(&outside, b"swapped binary", 0o700);
        assert_eq!(
            artifact::inspect_runtime_with(&pack.0, &binary, None, Some(&catalog))
                .unwrap_err()
                .code,
            "runtime-binary"
        );
    }

    #[test]
    fn inspect_runtime_refuses_packed_bin_oversize_and_symlinked_binary() {
        let catalog = FixtureCatalog;
        let outside = PackDir::new();
        let binary = write_binary(&outside, b"runtime binary", 0o755);

        let packed = artifact_fixture();
        packed.write("bin/am", b"packed");
        std::fs::set_permissions(
            packed.0.join("bin/am"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert_eq!(
            artifact::inspect_runtime_with(&packed.0, &binary, None, Some(&catalog))
                .unwrap_err()
                .code,
            "runtime-binary"
        );

        let pack = artifact_fixture();
        let link = outside.0.join("am-link");
        std::os::unix::fs::symlink(&binary, &link).unwrap();
        assert_eq!(
            artifact::inspect_runtime_with(&pack.0, &link, None, Some(&catalog))
                .unwrap_err()
                .code,
            "file-type"
        );

        let oversize = outside.0.join("oversize");
        let file = std::fs::File::create(&oversize).unwrap();
        file.set_len(artifact::BINARY_MAX_BYTES + 1).unwrap();
        drop(file);
        std::fs::set_permissions(&oversize, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            artifact::inspect_runtime_with(&pack.0, &oversize, None, Some(&catalog))
                .unwrap_err()
                .code,
            "inventory-bound"
        );
        let at_bound = outside.0.join("at-bound");
        let file = std::fs::File::create(&at_bound).unwrap();
        file.set_len(artifact::BINARY_MAX_BYTES).unwrap();
        drop(file);
        std::fs::set_permissions(&at_bound, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(artifact::inspect_runtime_with(&pack.0, &at_bound, None, Some(&catalog)).is_ok());
    }
}

mod developer_contract {
    use agent_memory::codec::json::{self, DecodePolicy, Value};
    use agent_memory::developer_identity as developer;
    use std::path::Path;

    fn developer_json(input: &str) -> Value {
        json::decode(input, DecodePolicy::default()).unwrap()
    }

    fn developer_private_root(label: &str) -> std::path::PathBuf {
        use std::os::unix::fs::DirBuilderExt;
        let root = std::path::Path::new("/tmp")
            .join(format!("{label}{}", agent_memory::host::uuid4().unwrap()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        root.canonicalize().unwrap()
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_registry_and_proposal_refuse_preferences_harness_aliases_and_wrong_roots
    #[test]
    fn developer_registry_proposal_and_alias_refusals() {
        assert!(developer::declared(&["harness=codex".into()]).is_err());
        assert!(!developer::uuid_valid(&Value::String("not-a-uuid".into())));
        let proposal = developer_json(
            r#"{"schema":"agent-memory/developer-proposal-v1","operation_id":"12345678-1234-4123-8123-123456789abc","action":"create","developer_id":"abcdefab-1234-4123-8123-123456789abc","labels":[],"aliases":[],"observed_sha256":null,"registry_path":"/different/developers.json"}"#,
        );
        assert!(
            developer::proposal_value(&proposal, Path::new("/expected/developers.json")).is_err()
        );
        let registry = developer_json(
            r#"{"schema":"agent-memory/developers-v1","selected":"abcdefab-1234-4123-8123-123456789abc","developers":{"abcdefab-1234-4123-8123-123456789abc":{"labels":[],"aliases":[],"preferences":{"color":"yellow"}}},"operations":{}}"#,
        );
        assert!(developer::validate(&registry).is_err());
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_replay_is_idempotent_without_reselecting_an_old_identity
    #[test]
    fn developer_proposal_hash_binds_metadata() {
        let a = developer_json(
            r#"{"schema":"agent-memory/developer-proposal-v1","operation_id":"12345678-1234-4123-8123-123456789abc","action":"create","developer_id":"abcdefab-1234-4123-8123-123456789abc","labels":[],"aliases":[],"observed_sha256":null,"registry_path":"/profile/developers.json"}"#,
        );
        let b = developer_json(
            r#"{"schema":"agent-memory/developer-proposal-v1","operation_id":"12345678-1234-4123-8123-123456789abc","action":"create","developer_id":"abcdefab-1234-4123-8123-123456789abc","labels":["Changed request"],"aliases":[],"observed_sha256":null,"registry_path":"/profile/developers.json"}"#,
        );
        let p = Path::new("/profile/developers.json");
        assert!(developer::proposal_value(&a, p).is_ok());
        assert!(developer::proposal_value(&b, p).is_ok());
        assert_ne!(
            developer::digest(Some(&developer::encoded(&a).unwrap())),
            developer::digest(Some(&developer::encoded(&b).unwrap()))
        );
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_stale_preview_and_external_edits_stop_without_overwrite
    #[test]
    fn developer_stale_preview_and_replay_in_isolated_process() {
        use agent_memory::host::Sources;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        fn next_uuid() -> agent_memory::error::Result<String> {
            Ok(format!(
                "00000000-0000-4000-8000-{:012x}",
                NEXT.fetch_add(1, Ordering::SeqCst)
            ))
        }
        fn now() -> agent_memory::error::Result<String> {
            Ok("2026-10-22T17:00:00+00:00".into())
        }
        if std::env::var_os("DEVELOPER_TEST_CHILD").is_none() {
            let root = developer_private_root("developer-contract-");
            let config = root.join("config with spaces");
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("developer_stale_preview_and_replay_in_isolated_process")
                .env("DEVELOPER_TEST_CHILD", "1")
                .env("AGENT_MEMORY_CONFIG_HOME", &config)
                .status()
                .unwrap();
            std::fs::remove_dir_all(&root).unwrap();
            assert!(status.success());
            return;
        }
        let sources = Sources {
            uuid4: next_uuid,
            utc_now: now,
        };
        let empty = Value::Array(vec![]);
        let first =
            developer::preview_with_sources("create", None, &empty, &empty, sources).unwrap();
        let stale =
            developer::preview_with_sources("create", None, &empty, &empty, sources).unwrap();
        let receipt = developer::apply_with_sources(&first, sources).unwrap();
        assert_eq!(receipt.get("verified"), Some(&Value::Bool(true)));
        let registry =
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                .join("developers.json");
        let before = std::fs::read(&registry).unwrap();
        let error = developer::apply_with_sources(&stale, sources).unwrap_err();
        assert!(error.to_string().contains("changed after preview"));
        assert_eq!(std::fs::read(&registry).unwrap(), before);
        let replay = developer::apply_with_sources(&first, sources).unwrap();
        assert_eq!(replay.get("already_applied"), Some(&Value::Bool(true)));
        assert_eq!(std::fs::read(&registry).unwrap(), before);
        let second =
            developer::preview_with_sources("create", None, &empty, &empty, sources).unwrap();
        let selected = developer::apply_with_sources(&second, sources)
            .unwrap()
            .get("developer_id")
            .unwrap()
            .clone();
        let replay = developer::apply_with_sources(&first, sources).unwrap();
        assert_eq!(replay.get("developer_id"), Some(&selected));
        assert_eq!(
            replay.get("operation_developer_id"),
            first.get("developer_id")
        );
        let mut forged = first.clone();
        let Value::Array(labels) = forged
            .get_mut("proposal")
            .unwrap()
            .get_mut("labels")
            .unwrap()
        else {
            panic!("proposal labels")
        };
        labels.push(Value::String("Changed request".into()));
        assert!(developer::apply_with_sources(&forged, sources).is_err());
        let select =
            developer::preview_with_sources("select", selected.as_str(), &empty, &empty, sources)
                .unwrap();
        let mut data = developer::load(&std::fs::read(&registry).unwrap()).unwrap();
        let Value::Array(labels) = data
            .get_mut("developers")
            .unwrap()
            .get_mut(selected.as_str().unwrap())
            .unwrap()
            .get_mut("labels")
            .unwrap()
        else {
            panic!("developer labels")
        };
        labels.push(Value::String("External metadata edit".into()));
        let external = developer::encoded(&data).unwrap();
        std::fs::write(&registry, &external).unwrap();
        assert!(developer::apply_with_sources(&select, sources).is_err());
        assert_eq!(std::fs::read(&registry).unwrap(), external);
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_live_and_foreign_locks_refuse_and_dead_local_owner_can_recover
    #[test]
    fn developer_live_foreign_and_stale_recovery_in_isolated_process() {
        use agent_memory::host::{self, PidState, Sources};
        fn id() -> agent_memory::error::Result<String> {
            Ok("12345678-1234-4123-8123-123456789abc".into())
        }
        fn now() -> agent_memory::error::Result<String> {
            Ok("2026-10-22T17:00:00+00:00".into())
        }
        if std::env::var_os("DEVELOPER_RECOVER_CHILD").is_none() {
            let root = developer_private_root("developer-recover-");
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("developer_live_foreign_and_stale_recovery_in_isolated_process")
                .env("DEVELOPER_RECOVER_CHILD", "1")
                .env("AGENT_MEMORY_CONFIG_HOME", &root)
                .status()
                .unwrap();
            std::fs::remove_dir_all(&root).unwrap();
            assert!(status.success());
            return;
        }
        let lock = std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
            .join(".developers.lock");
        let host_name = host::hostname().unwrap();
        let lock_bytes = |hostname: &str, pid: u32| {
            let owner = Value::Object(vec![
                ("host".into(), Value::String(hostname.into())),
                ("pid".into(), Value::Int(pid.into())),
                ("operation".into(), Value::String("developer-create".into())),
                (
                    "operation_id".into(),
                    Value::String("12345678-1234-4123-8123-123456789abc".into()),
                ),
                ("start".into(), Value::String("2026-10-22T17:00:00Z".into())),
            ]);
            developer::encoded(&owner).unwrap()
        };
        let sources = Sources {
            uuid4: id,
            utc_now: now,
        };
        std::fs::write(&lock, lock_bytes(&host_name, std::process::id())).unwrap();
        assert!(
            developer::recover_with_sources(None, sources)
                .unwrap_err()
                .to_string()
                .contains("still alive")
        );
        let dead = (1_000_000..2_000_000)
            .find(|&pid| matches!(host::pid_state(pid), Ok(PidState::Gone)))
            .unwrap() as u32;
        std::fs::write(&lock, lock_bytes("different-machine", dead)).unwrap();
        assert!(
            developer::recover_with_sources(None, sources)
                .unwrap_err()
                .to_string()
                .contains("Foreign-host")
        );
        let original = lock_bytes(&host_name, dead);
        std::fs::write(&lock, &original).unwrap();
        let proposal = developer::recover_with_sources(None, sources).unwrap();
        let mut changed = original.clone();
        changed.push(b' ');
        std::fs::write(&lock, &changed).unwrap();
        assert!(developer::recover_with_sources(Some(&proposal), sources).is_err());
        assert_eq!(std::fs::read(&lock).unwrap(), changed);
        std::fs::write(&lock, &original).unwrap();
        let receipt = developer::recover_with_sources(Some(&proposal), sources).unwrap();
        assert_eq!(receipt.get("verified"), Some(&Value::Bool(true)));
        assert!(!lock.exists());
    }

    #[test]
    fn developer_oversize_registry_and_lock_are_preserved() {
        use agent_memory::error::Kind;
        if std::env::var_os("DEVELOPER_BOUND_CHILD").is_none() {
            let root = developer_private_root("developer-bound-");
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("developer_oversize_registry_and_lock_are_preserved")
                .env("DEVELOPER_BOUND_CHILD", "1")
                .env("AGENT_MEMORY_CONFIG_HOME", &root)
                .status()
                .unwrap();
            std::fs::remove_dir_all(&root).unwrap();
            assert!(status.success());
            return;
        }
        let root = std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap());
        let registry = root.join("developers.json");
        let file = std::fs::File::create(&registry).unwrap();
        file.set_len(developer::REGISTRY_MAX_BYTES as u64 + 1)
            .unwrap();
        drop(file);
        let err = developer::observe().unwrap_err();
        assert!(matches!(err.kind(), Kind::LocalRegistryBound { .. }));
        assert_eq!(
            err.to_string(),
            "Local registry exceeds its reader bound; preserve it and inspect manually"
        );
        assert_eq!(
            std::fs::metadata(&registry).unwrap().len(),
            developer::REGISTRY_MAX_BYTES as u64 + 1
        );
        std::fs::remove_file(&registry).unwrap();
        let lock = root.join(".developers.lock");
        let file = std::fs::File::create(&lock).unwrap();
        file.set_len(developer::LOCK_MAX_BYTES as u64 + 1).unwrap();
        drop(file);
        let err = developer::recover(None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Unrecognized developer lock requires operator review; preserve it"
        );
        assert_eq!(
            std::fs::metadata(lock).unwrap().len(),
            developer::LOCK_MAX_BYTES as u64 + 1
        );
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_first_setup_has_no_registry_bank_install_or_credentials_prerequisite
    #[test]
    fn developer_first_setup_in_isolated_process() {
        use std::os::unix::fs::PermissionsExt;
        if std::env::var_os("DEVELOPER_SETUP_CHILD").is_none() {
            let root = developer_private_root("developer-setup-");
            let config = root.join("config");
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("developer_first_setup_in_isolated_process")
                .env("DEVELOPER_SETUP_CHILD", "1")
                .env("AGENT_MEMORY_CONFIG_HOME", &config)
                .env("HOME", &root)
                .env_remove("USER")
                .current_dir(&root)
                .status()
                .unwrap();
            std::fs::remove_dir_all(&root).unwrap();
            assert!(status.success());
            return;
        }
        let config =
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap());
        let status = developer::status(&[], None, None, Path::new(".")).unwrap();
        assert_eq!(status.get("setup_required"), Some(&Value::Bool(true)));
        assert_eq!(
            status.get("identity_authenticated"),
            Some(&Value::Bool(false))
        );
        assert!(
            status
                .get("security_boundary")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("not authentication")
        );
        assert!(!config.exists());
        let labels = Value::Array(vec![Value::String("Developer A".into())]);
        let proposal = developer::preview("create", None, &labels, &Value::Array(vec![])).unwrap();
        assert!(!config.exists());
        assert_eq!(proposal.get("writes_performed"), Some(&Value::Bool(false)));
        let receipt = developer::apply(&proposal).unwrap();
        assert_eq!(receipt.get("verified"), Some(&Value::Bool(true)));
        assert_eq!(
            receipt.get("changed_memory_paths"),
            Some(&Value::Array(vec![]))
        );
        assert_eq!(
            std::fs::metadata(config.join("developers.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(!Path::new("memory-bank").exists());
        assert!(!Path::new(".git").exists());
    }

    fn developer_isolated_test(name: &str, marker: &str) -> bool {
        if std::env::var_os(marker).is_some() {
            return true;
        }
        let root = developer_private_root("developer-isolated-");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg(name)
            .env(marker, "1")
            .env("AGENT_MEMORY_CONFIG_HOME", root.join("config"))
            .env("HOME", &root)
            .env_remove("USER")
            .current_dir(&root)
            .status()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(status.success());
        false
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_collaborator_selection_is_explicit_and_preserves_metadata
    #[test]
    fn developer_collaborator_selection_and_adoption() {
        if !developer_isolated_test(
            "developer_collaborator_selection_and_adoption",
            "DEVELOPER_COLLAB_CHILD",
        ) {
            return;
        }
        let list = |text: &str| Value::Array(vec![Value::String(text.into())]);
        let empty = Value::Array(vec![]);
        let first = developer::preview(
            "create",
            None,
            &list("Developer A"),
            &developer::declared(&["github-login=alice".into()]).unwrap(),
        )
        .unwrap();
        developer::apply(&first).unwrap();
        let original = first.get("developer_id").unwrap().as_str().unwrap();
        let second = developer::preview(
            "create",
            None,
            &list("Developer B"),
            &developer::declared(&["github-login=bob".into()]).unwrap(),
        )
        .unwrap();
        developer::apply(&second).unwrap();
        let other = second.get("developer_id").unwrap().as_str().unwrap();
        assert_ne!(original, other);
        let selection = developer::preview("select", Some(original), &empty, &empty).unwrap();
        assert_eq!(
            developer::apply(&selection).unwrap().get("developer_id"),
            Some(&Value::String(original.into()))
        );
        let adoption = developer::preview(
            "adopt",
            Some(original),
            &list("Same person on this machine"),
            &developer::declared(&["os-user=local-alias".into()]).unwrap(),
        )
        .unwrap();
        developer::apply(&adoption).unwrap();
        let (registry, _) = developer::observe().unwrap();
        let developers = registry.get("developers").unwrap();
        assert_eq!(developers.as_object().unwrap().len(), 2);
        let alice = developers.get(original).unwrap();
        assert_eq!(
            alice.get("labels"),
            Some(&Value::Array(vec![
                Value::String("Developer A".into()),
                Value::String("Same person on this machine".into())
            ]))
        );
        assert!(
            alice.get("aliases").unwrap().as_array().unwrap().contains(
                &developer::declared(&["github-login=alice".into()])
                    .unwrap()
                    .as_array()
                    .unwrap()[0]
            )
        );
        assert_eq!(
            developers.get(other).unwrap().get("labels"),
            Some(&list("Developer B"))
        );
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_unrelated_or_symlinked_registry_is_preserved
    #[test]
    fn developer_preserves_unrelated_and_symlinked_registry() {
        if !developer_isolated_test(
            "developer_preserves_unrelated_and_symlinked_registry",
            "DEVELOPER_UNRELATED_CHILD",
        ) {
            return;
        }
        let empty = Value::Array(vec![]);
        let proposal = developer::preview("create", None, &empty, &empty).unwrap();
        let config =
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap());
        std::fs::create_dir(&config).unwrap();
        let registry = config.join("developers.json");
        let unrelated = b"Unrelated user file; preserve it.\n";
        std::fs::write(&registry, unrelated).unwrap();
        assert!(developer::apply(&proposal).is_err());
        assert_eq!(std::fs::read(&registry).unwrap(), unrelated);
        std::fs::remove_file(&registry).unwrap();
        let external = config.parent().unwrap().join("unrelated.json");
        std::fs::write(&external, b"Unrelated content").unwrap();
        std::os::unix::fs::symlink(&external, &registry).unwrap();
        assert!(developer::apply(&proposal).is_err());
        assert!(
            std::fs::symlink_metadata(&registry)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read(&external).unwrap(), b"Unrelated content");
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_same_id_is_explicitly_adopted_across_five_machines
    #[test]
    fn developer_identity_adopted_across_five_profiles() {
        if std::env::var_os("DEVELOPER_PROFILE_CHILD").is_none() {
            let root = developer_private_root("developer-profiles-");
            let identity = root.join("identity");
            for (i, harness) in ["claude", "codex", "opencode", "custom", "codex"]
                .iter()
                .enumerate()
            {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("developer_identity_adopted_across_five_profiles")
                    .env("DEVELOPER_PROFILE_CHILD", "1")
                    .env("DEVELOPER_PROFILE_INDEX", i.to_string())
                    .env("DEVELOPER_PROFILE_HARNESS", harness)
                    .env("DEVELOPER_PROFILE_ID", &identity)
                    .env(
                        "AGENT_MEMORY_CONFIG_HOME",
                        root.join(format!("profile-{i}")),
                    )
                    .env("HOME", &root)
                    .env_remove("USER")
                    .current_dir(&root)
                    .status()
                    .unwrap();
                assert!(status.success());
            }
            std::fs::remove_dir_all(root).unwrap();
            return;
        }
        let shared = std::path::PathBuf::from(std::env::var_os("DEVELOPER_PROFILE_ID").unwrap());
        let index = std::env::var("DEVELOPER_PROFILE_INDEX").unwrap();
        let labels = Value::Array(vec![Value::String("Developer A".into())]);
        let empty = Value::Array(vec![]);
        let previous = if index == "0" {
            None
        } else {
            Some(std::fs::read_to_string(&shared).unwrap())
        };
        let proposal = developer::preview(
            if previous.is_some() {
                "adopt"
            } else {
                "create"
            },
            previous.as_deref(),
            &labels,
            &empty,
        )
        .unwrap();
        let receipt = developer::apply(&proposal).unwrap();
        let id = receipt.get("developer_id").unwrap().as_str().unwrap();
        if previous.is_none() {
            std::fs::write(&shared, id).unwrap();
        } else {
            assert_eq!(Some(id), previous.as_deref());
        }
        let status = developer::status(
            &[],
            Some(&std::env::var("DEVELOPER_PROFILE_HARNESS").unwrap()),
            Some(&format!("session-{index}")),
            Path::new("."),
        )
        .unwrap();
        assert_eq!(status.get("developer_id").unwrap().as_str(), Some(id));
        let registry = std::fs::read_to_string(
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                .join("developers.json"),
        )
        .unwrap();
        assert!(!registry.contains("session"));
        assert!(!registry.contains("harness"));
        let decoded = developer::load(registry.as_bytes()).unwrap();
        assert_eq!(
            decoded
                .get("developers")
                .unwrap()
                .get(id)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            2
        );
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_recovery_cannot_remove_a_lock_held_by_another_recovery
    #[test]
    fn developer_recovery_respects_held_flock() {
        if !developer_isolated_test(
            "developer_recovery_respects_held_flock",
            "DEVELOPER_FLOCK_CHILD",
        ) {
            return;
        }
        let root = std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap());
        std::fs::create_dir(&root).unwrap();
        let lock = root.join(".developers.lock");
        let bytes = b"{\"host\":\"local\",\"pid\":1,\"operation\":\"developer-create\",\"operation_id\":\"12345678-1234-4123-8123-123456789abc\",\"start\":\"2026-10-22T17:00:00Z\"}";
        std::fs::write(&lock, bytes).unwrap();
        let chain = agent_memory::fs::DirectoryChain::open(&root).unwrap();
        let file = chain
            .directory()
            .open_at(std::ffi::OsStr::new(".developers.lock"), libc::O_RDONLY, 0)
            .unwrap();
        let _guard =
            agent_memory::fs::FlockGuard::try_acquire(file, agent_memory::fs::LockMode::Exclusive)
                .unwrap();
        assert!(
            developer::recover(None)
                .unwrap_err()
                .to_string()
                .contains("held")
        );
        assert_eq!(std::fs::read(lock).unwrap(), bytes);
    }

    fn developer_git(repo: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_no_match_preserves_caller_observations
    #[test]
    fn developer_git_nonmatching_keys_preserve_caller_observations() {
        if !developer_isolated_test(
            "developer_git_nonmatching_keys_preserve_caller_observations",
            "DEVELOPER_GIT_NOMATCH_CHILD",
        ) {
            return;
        }
        let repo = Path::new(".");
        developer_git(repo, &["init", "-q"]);
        developer_git(
            repo,
            &["config", "user.email-extra", "ignored@example.invalid"],
        );
        developer_git(repo, &["config", "user.nameSuffix", "Ignored Developer"]);
        developer_git(
            repo,
            &["config", "user.other.email", "subsection@example.invalid"],
        );
        let empty = Value::Array(vec![]);
        let aliases =
            developer::declared(&["git-author-email=caller@example.invalid".into()]).unwrap();
        let proposal = developer::preview("create", None, &empty, &aliases).unwrap();
        developer::apply(&proposal).unwrap();
        let before = std::fs::read(
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                .join("developers.json"),
        )
        .unwrap();
        let status = developer::status(aliases.as_array().unwrap(), None, None, repo).unwrap();
        let evidence = status.get("observations").unwrap().as_array().unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            evidence[0].get("state").unwrap().as_str(),
            Some("matches-selected")
        );
        assert_eq!(
            status.get("attribution_conflicts"),
            Some(&Value::Array(vec![]))
        );
        assert_eq!(
            status.get("identity_authenticated"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            std::fs::read(
                std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                    .join("developers.json")
            )
            .unwrap(),
            before
        );
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_rejects_genuine_embedded_and_trailing_lf_values
    #[test]
    fn developer_git_newline_values_are_refused() {
        if !developer_isolated_test(
            "developer_git_newline_values_are_refused",
            "DEVELOPER_GIT_NEWLINE_CHILD",
        ) {
            return;
        }
        let repo = Path::new(".");
        developer_git(repo, &["init", "-q"]);
        let empty = Value::Array(vec![]);
        let proposal = developer::preview("create", None, &empty, &empty).unwrap();
        developer::apply(&proposal).unwrap();
        let registry =
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                .join("developers.json");
        let before = std::fs::read(&registry).unwrap();
        for key in ["user.email", "user.name"] {
            for value in ["First\nSecond", "Trailing\n", "Trailing\n\n"] {
                developer_git(
                    repo,
                    &[
                        "config",
                        "--replace-all",
                        "user.email",
                        "configured@example.invalid",
                    ],
                );
                developer_git(
                    repo,
                    &[
                        "config",
                        "--replace-all",
                        "user.name",
                        "Configured Developer",
                    ],
                );
                developer_git(repo, &["config", "--replace-all", key, value]);
                let config = std::fs::read(".git/config").unwrap();
                assert!(developer::status(&[], None, None, repo).is_err());
                assert_eq!(std::fs::read(".git/config").unwrap(), config);
                assert_eq!(std::fs::read(&registry).unwrap(), before);
            }
        }
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_failure_contributes_neither_alias_and_recovers_freshly
    #[test]
    fn developer_git_invalid_config_then_recovery() {
        if !developer_isolated_test(
            "developer_git_invalid_config_then_recovery",
            "DEVELOPER_GIT_INVALID_CHILD",
        ) {
            return;
        }
        let repo = Path::new(".");
        developer_git(repo, &["init", "-q"]);
        developer_git(
            repo,
            &["config", "user.email", "configured@example.invalid"],
        );
        developer_git(repo, &["config", "user.name", "Configured Developer"]);
        let aliases = developer::declared(&[
            "git-author-email=configured@example.invalid".into(),
            "git-author-name=Configured Developer".into(),
            "github-login=caller".into(),
        ])
        .unwrap();
        let proposal = developer::preview("create", None, &Value::Array(vec![]), &aliases).unwrap();
        developer::apply(&proposal).unwrap();
        let caller = developer::declared(&["github-login=caller".into()]).unwrap();
        let config = Path::new(".git/config");
        let valid = std::fs::read(config).unwrap();
        let mut invalid = valid.clone();
        invalid.extend_from_slice(b"[broken\n");
        std::fs::write(config, &invalid).unwrap();
        let failed = developer::status(caller.as_array().unwrap(), None, None, repo).unwrap();
        let observed = failed.get("observations").unwrap().as_array().unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(
            observed[0].get("kind").unwrap().as_str(),
            Some("github-login")
        );
        assert_eq!(std::fs::read(config).unwrap(), invalid);
        std::fs::write(config, &valid).unwrap();
        let recovered = developer::status(caller.as_array().unwrap(), None, None, repo).unwrap();
        let observed = recovered.get("observations").unwrap().as_array().unwrap();
        assert_eq!(
            observed
                .iter()
                .map(|v| v.get("kind").unwrap().as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["github-login", "git-author-email", "git-author-name"]
        );
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_valueless_and_empty_values_override_earlier_aliases
    #[test]
    fn developer_git_empty_and_valueless_override_prior_config() {
        if !developer_isolated_test(
            "developer_git_empty_and_valueless_override_prior_config",
            "DEVELOPER_GIT_EMPTY_CHILD",
        ) {
            return;
        }
        let repo = Path::new(".");
        developer_git(repo, &["init", "-q"]);
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        std::fs::write(
            home.join(".gitconfig"),
            "[user]\n\temail = earlier@example.invalid\n\tname = Earlier Developer\n",
        )
        .unwrap();
        let aliases = developer::declared(&[
            "git-author-email=earlier@example.invalid".into(),
            "git-author-name=Earlier Developer".into(),
        ])
        .unwrap();
        let proposal = developer::preview("create", None, &Value::Array(vec![]), &aliases).unwrap();
        developer::apply(&proposal).unwrap();
        let config = Path::new(".git/config");
        let original = std::fs::read_to_string(config).unwrap();
        for entries in [
            "[UsEr]\n\tEmAiL\n\tNaMe = \"\"\n",
            "[UsEr]\n\tEmAiL = \"\"\n\tNaMe\n",
        ] {
            std::fs::write(config, format!("{original}{entries}")).unwrap();
            let before = std::fs::read(config).unwrap();
            let status = developer::status(&[], None, None, repo).unwrap();
            assert_eq!(status.get("observations"), Some(&Value::Array(vec![])));
            assert_eq!(
                status.get("attribution_conflicts"),
                Some(&Value::Array(vec![]))
            );
            assert_eq!(std::fs::read(config).unwrap(), before);
            developer_git(
                repo,
                &[
                    "config",
                    "--replace-all",
                    "user.email",
                    "earlier@example.invalid",
                ],
            );
            developer_git(
                repo,
                &["config", "--replace-all", "user.name", "Earlier Developer"],
            );
            let restored = developer::status(&[], None, None, repo).unwrap();
            let observations = restored.get("observations").unwrap().as_array().unwrap();
            assert_eq!(observations.len(), 2);
            assert!(
                observations
                    .iter()
                    .all(|v| v.get("state").unwrap().as_str() == Some("matches-selected"))
            );
        }
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_effective_git_config_preserves_duplicates_includes_and_mixed_case_precedence
    #[test]
    fn developer_git_duplicates_includes_and_mixed_case() {
        if !developer_isolated_test(
            "developer_git_duplicates_includes_and_mixed_case",
            "DEVELOPER_GIT_INCLUDE_CHILD",
        ) {
            return;
        }
        let repo = Path::new(".");
        developer_git(repo, &["init", "-q"]);
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        std::fs::write(home.join("attribution.inc"),"[UsEr]\n\tNaMe = Included Earlier\n\tNaMe = \"  Déveloper Effective  \"\n\tEmAiL = included@example.invalid\n").unwrap();
        std::fs::write(home.join(".gitconfig"),"[UsEr]\n\tNaMe = Global Earlier\n\tEmAiL = global@example.invalid\n[include]\n\tpath = attribution.inc\n[USER]\n\tEMAIL = global-last@example.invalid\n").unwrap();
        developer_git(
            repo,
            &["config", "--add", "user.email", "earlier@example.invalid"],
        );
        developer_git(
            repo,
            &["config", "--add", "USER.EMAIL", "effective@example.invalid"],
        );
        let empty = Value::Array(vec![]);
        let other_aliases = developer::declared(&[
            "git-author-email=earlier@example.invalid".into(),
            "git-author-name=Other Developer".into(),
        ])
        .unwrap();
        let other = developer::preview("create", None, &empty, &other_aliases).unwrap();
        developer::apply(&other).unwrap();
        let selected_aliases = developer::declared(&[
            "git-author-email=effective@example.invalid".into(),
            "git-author-name=  Déveloper Effective  ".into(),
            "github-login=effective".into(),
        ])
        .unwrap();
        let selected = developer::preview("create", None, &empty, &selected_aliases).unwrap();
        developer::apply(&selected).unwrap();
        let caller = developer::declared(&["github-login=effective".into()]).unwrap();
        let status = developer::status(caller.as_array().unwrap(), None, None, repo).unwrap();
        let observations = status.get("observations").unwrap().as_array().unwrap();
        assert_eq!(
            observations
                .iter()
                .map(|v| v.get("kind").unwrap().as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["github-login", "git-author-email", "git-author-name"]
        );
        assert!(
            observations
                .iter()
                .all(|v| v.get("state").unwrap().as_str() == Some("matches-selected"))
        );
        developer_git(
            repo,
            &["config", "--add", "user.email", "earlier@example.invalid"],
        );
        developer_git(repo, &["config", "--add", "user.name", "Other Developer"]);
        let changed = developer::status(caller.as_array().unwrap(), None, None, repo).unwrap();
        let conflicts = changed
            .get("attribution_conflicts")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(conflicts.len(), 2);
        assert!(conflicts.iter().all(|v| {
            v.get("matching_developer_ids")
                .unwrap()
                .as_array()
                .unwrap()
                .contains(other.get("developer_id").unwrap())
        }));
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_true_concurrent_initializers_preserve_the_winner
    #[test]
    fn developer_concurrent_initializers_preserve_winner() {
        if !developer_isolated_test(
            "developer_concurrent_initializers_preserve_winner",
            "DEVELOPER_CONCURRENT_CHILD",
        ) {
            return;
        }
        if let Some(index) = std::env::var_os("DEVELOPER_CONCURRENT_WORKER") {
            let index = index.to_string_lossy();
            let proposal =
                developer::load(&std::fs::read(format!("proposal-{index}.json")).unwrap()).unwrap();
            let outcome = developer::apply(&proposal);
            std::fs::write(
                format!("outcome-{index}"),
                if outcome.is_ok() {
                    "success"
                } else {
                    "refused"
                },
            )
            .unwrap();
            return;
        }
        let empty = Value::Array(vec![]);
        for i in 0..2 {
            let proposal = developer::preview("create", None, &empty, &empty).unwrap();
            std::fs::write(
                format!("proposal-{i}.json"),
                developer::encoded(&proposal).unwrap(),
            )
            .unwrap();
        }
        let mut children: Vec<_> = (0..2)
            .map(|i| {
                std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("developer_concurrent_initializers_preserve_winner")
                    .env("DEVELOPER_CONCURRENT_WORKER", i.to_string())
                    .spawn()
                    .unwrap()
            })
            .collect();
        for child in &mut children {
            assert!(child.wait().unwrap().success());
        }
        let winners = (0..2)
            .filter(|i| std::fs::read_to_string(format!("outcome-{i}")).unwrap() == "success")
            .count();
        assert_eq!(winners, 1);
        let (registry, _) = developer::observe().unwrap();
        assert_eq!(
            registry
                .get("developers")
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            registry
                .get("operations")
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            1
        );
        assert!(
            !std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                .join(".developers.lock")
                .exists()
        );
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_changed_git_hosting_client_and_user_aliases_never_change_attribution
    #[test]
    fn developer_git_alias_changes_do_not_reselect() {
        if !developer_isolated_test(
            "developer_git_alias_changes_do_not_reselect",
            "DEVELOPER_GIT_CHANGE_CHILD",
        ) {
            return;
        }
        let repo = Path::new(".");
        developer_git(repo, &["init", "-q"]);
        developer_git(repo, &["config", "user.name", "Developer A"]);
        developer_git(repo, &["config", "user.email", "alice@example.invalid"]);
        let empty = Value::Array(vec![]);
        let original_aliases = developer::declared(&[
            "git-author-email=alice@example.invalid".into(),
            "github-login=alice".into(),
        ])
        .unwrap();
        let first = developer::preview("create", None, &empty, &original_aliases).unwrap();
        developer::apply(&first).unwrap();
        let original = first.get("developer_id").unwrap().as_str().unwrap();
        let registry =
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                .join("developers.json");
        let before = std::fs::read(&registry).unwrap();
        developer_git(repo, &["config", "user.email", "bob@example.invalid"]);
        developer_git(repo, &["config", "user.name", "Developer B"]);
        let observed =
            developer::declared(&["github-login=bob".into(), "os-user=another-user".into()])
                .unwrap();
        let status = developer::status(
            observed.as_array().unwrap(),
            Some("custom"),
            Some("two"),
            repo,
        )
        .unwrap();
        assert_eq!(status.get("developer_id").unwrap().as_str(), Some(original));
        assert!(
            status
                .get("observations")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.get("state").unwrap().as_str() == Some("unknown"))
        );
        assert_eq!(std::fs::read(&registry).unwrap(), before);
        let other_aliases =
            developer::declared(&["git-author-email=bob@example.invalid".into()]).unwrap();
        let other = developer::preview("create", None, &empty, &other_aliases).unwrap();
        developer::apply(&other).unwrap();
        let selection = developer::preview("select", Some(original), &empty, &empty).unwrap();
        developer::apply(&selection).unwrap();
        let before = std::fs::read(&registry).unwrap();
        let status = developer::status(&[], None, None, repo).unwrap();
        let conflicts = status
            .get("attribution_conflicts")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(conflicts.iter().any(|v| {
            v.get("matching_developer_ids")
                .unwrap()
                .as_array()
                .unwrap()
                .contains(other.get("developer_id").unwrap())
        }));
        assert_eq!(
            status.get("identity_authenticated"),
            Some(&Value::Bool(false))
        );
        assert_eq!(std::fs::read(registry).unwrap(), before);
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_registry_and_proposal_refuse_preferences_harness_aliases_and_wrong_roots
    #[test]
    fn developer_bad_registry_and_wrong_root_are_preserved() {
        if !developer_isolated_test(
            "developer_bad_registry_and_wrong_root_are_preserved",
            "DEVELOPER_WRONG_ROOT_CHILD",
        ) {
            return;
        }
        let empty = Value::Array(vec![]);
        assert!(developer::preview("adopt", Some("not-a-uuid"), &empty, &empty).is_err());
        assert!(developer::declared(&["harness=codex".into()]).is_err());
        let mut proposal = developer::preview("create", None, &empty, &empty).unwrap();
        *proposal
            .get_mut("proposal")
            .unwrap()
            .get_mut("registry_path")
            .unwrap() = Value::String("/different-profile/developers.json".into());
        assert!(developer::apply(&proposal).is_err());
        let registry =
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                .join("developers.json");
        assert!(!registry.exists());
        let proposal = developer::preview("create", None, &empty, &empty).unwrap();
        developer::apply(&proposal).unwrap();
        let id = proposal.get("developer_id").unwrap().as_str().unwrap();
        let mut data = developer::load(&std::fs::read(&registry).unwrap()).unwrap();
        let Value::Object(fields) = data.get_mut("developers").unwrap().get_mut(id).unwrap() else {
            panic!("developer")
        };
        fields.push((
            "preferences".into(),
            developer_json(r#"{"color":"yellow"}"#),
        ));
        let bytes = developer::encoded(&data).unwrap();
        std::fs::write(&registry, &bytes).unwrap();
        assert!(developer::status(&[], None, None, Path::new(".")).is_err());
        assert!(developer::preview("create", None, &empty, &empty).is_err());
        assert_eq!(std::fs::read(&registry).unwrap(), bytes);
    }

    // obligation: tests/contracts/test_developer_identity.py::DeveloperIdentityContract.test_copied_pack_and_config_paths_with_spaces_work_without_installation
    #[test]
    fn developer_configuration_path_with_spaces() {
        if std::env::var_os("DEVELOPER_SPACES_CHILD").is_none() {
            let root = developer_private_root("developer portable pack ");
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("developer_configuration_path_with_spaces")
                .env("DEVELOPER_SPACES_CHILD", "1")
                .env("AGENT_MEMORY_CONFIG_HOME", root.join("config with spaces"))
                .env("HOME", &root)
                .env_remove("USER")
                .current_dir(&root)
                .status()
                .unwrap();
            std::fs::remove_dir_all(root).unwrap();
            assert!(status.success());
            return;
        }
        let labels = Value::Array(vec![Value::String("Developer A".into())]);
        let proposal = developer::preview("create", None, &labels, &Value::Array(vec![])).unwrap();
        let receipt = developer::apply(&proposal).unwrap();
        assert_eq!(receipt.get("verified"), Some(&Value::Bool(true)));
        assert!(
            std::path::PathBuf::from(std::env::var_os("AGENT_MEMORY_CONFIG_HOME").unwrap())
                .join("developers.json")
                .exists()
        );
    }
}

mod repository_contract {
    // Repository discovery fragment; integrate inside its own test module.
    use agent_memory::repository_config as repository;
    use std::os::unix::fs::DirBuilderExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_REPO: AtomicUsize = AtomicUsize::new(0);
    struct RepositoryDir(PathBuf);
    impl RepositoryDir {
        fn new() -> Self {
            let path = Path::new("/tmp").join(format!(
                "agent-memory-repository-{}-{}",
                std::process::id(),
                NEXT_REPO.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path.canonicalize().unwrap())
        }
        fn git(&self, args: &[&str]) {
            let status = Command::new("git")
                .arg("-C")
                .arg(&self.0)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        }
    }
    impl Drop for RepositoryDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    // obligation: tests/contracts/test_sharing_boundary.py::SharingBoundaryContract.test_damaged_untracked_context_bank_is_unknown_not_documentation_for_a_replacement
    #[test]
    fn repository_physical_contexts_refuse_replacement() {
        let repo = RepositoryDir::new();
        let bank = repo.0.join("docs/shared memory");
        std::fs::create_dir_all(&bank).unwrap();
        std::fs::write(bank.join("MEMORY.md"), b"").unwrap();
        std::fs::write(bank.join("projectbrief.md"), b"context").unwrap();
        std::fs::write(repo.0.join(".gitignore"), b"docs/\n").unwrap();
        let error = repository::physical_others(&repo.0, &repo.0.join("memory-bank"), None, false)
            .unwrap_err();
        assert_eq!(
            error.message(),
            "Possible existing context bank needs explicit inspection; do not create a replacement"
        );
        assert!(!repo.0.join("memory-bank").exists());
    }

    // obligation: tests/contracts/test_repository_observation_batch.py::RepositoryObservationBatch.test_independent_git_roots_resamples_head_and_validates_its_own_observation
    #[test]
    fn repository_roots_retain_committed_deleted_locator() {
        let repo = RepositoryDir::new();
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.name", "Fixture"]);
        repo.git(&["config", "user.email", "fixture@example.invalid"]);
        std::fs::create_dir(repo.0.join("notes")).unwrap();
        let router = b"---\nschema: agent-memory/v1\n---\n";
        std::fs::write(repo.0.join("notes/MEMORY.md"), router).unwrap();
        repo.git(&["add", "."]);
        repo.git(&["commit", "-qm", "fixture"]);
        repo.git(&["rm", "-q", "notes/MEMORY.md"]);
        let roots = repository::git_roots(&repo.0, None, false).unwrap();
        assert!(roots.contains("notes"));
        assert!(!Path::new(&repo.0.join("notes/MEMORY.md")).exists());
    }

    // obligation: tests/contracts/test_sharing_boundary.py::SharingBoundaryContract.test_ignored_existing_router_and_unknown_schema_cannot_become_source_evidence_for_second_bank
    #[test]
    fn repository_unknown_router_schema_refuses() {
        let raw = b"---\nschema: agent-memory/v999\n---\n";
        assert!(
            repository::router_kind(raw)
                .unwrap_err()
                .message()
                .contains("Possible existing memory needs repair")
        );
    }

    // obligation: tests/contracts/test_sharing_boundary.py::SharingBoundaryContract.test_unsafe_duplicate_or_ambiguous_configuration_preserves_all_paths
    #[test]
    fn repository_duplicate_local_configuration_refuses() {
        let repo = RepositoryDir::new();
        repo.git(&["init", "-q"]);
        repo.git(&[
            "config",
            "--local",
            "--add",
            "agent-memory.bankRoot",
            "memory-bank",
        ]);
        repo.git(&[
            "config",
            "--local",
            "--add",
            "agent-memory.bankRoot",
            "other",
        ]);
        assert_eq!(
            repository::local_settings(&repo.0).unwrap_err().message(),
            "Repository memory configuration needs one explicit value per key"
        );
    }

    // obligation: tests/contracts/test_sharing_boundary.py::SharingBoundaryContract.test_symlink_spelling_alias_and_two_bank_roots_are_refused_without_adoption
    #[test]
    fn repository_physical_walk_prunes_directory_links_and_refuses_file_links() {
        let repo = RepositoryDir::new();
        let target = repo.0.join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("MEMORY.md"), b"not a router").unwrap();
        let link = repo.0.join("MEMORY.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        repository::physical_others(&repo.0, &repo.0.join("memory-bank"), None, false).unwrap();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(target.join("MEMORY.md"), &link).unwrap();
        assert_eq!(
            repository::physical_others(&repo.0, &repo.0.join("memory-bank"), None, false)
                .unwrap_err()
                .message(),
            "A possible existing memory locator is a symlink; inspect it explicitly"
        );
    }
}

mod registry_contract {
    // obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_unheld_local_owner_is_interruption_not_live_pid_authority
    #[test]
    fn canonical_writer_requires_kernel_contention() {
        use agent_memory::{
            host::Sources,
            local_registry::{CanonicalWriterError, check_canonical_writer, exclusive},
        };
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let root = std::path::PathBuf::from(format!(
            "/tmp/agent-memory-registry-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let root = root.canonicalize().unwrap();
        let lock = root.join(".agent-memory.lock");
        let owner = exclusive(
            &lock,
            "00000000-0000-4000-8000-000000000000",
            "bank-capture",
            Sources::default(),
            |bytes| {
                assert_eq!(
                    check_canonical_writer(&lock, &["capture"]).unwrap_err(),
                    CanonicalWriterError::Busy
                );
                Ok(bytes.to_vec())
            },
        )
        .unwrap();
        std::fs::write(&lock, &owner).unwrap();
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(check_canonical_writer(&lock, &["capture"]).is_err());
        assert_eq!(std::fs::read(&lock).unwrap(), owner);
        std::fs::remove_dir_all(root).unwrap();
    }

    // obligation: tests/contracts/test_omp_finite_stream.py::CanonicalWriterContention.test_empty_held_and_unheld_slots_never_mint_scope_or_claim_known_writer
    #[test]
    fn canonical_writer_empty_slot_is_not_a_known_owner() {
        use agent_memory::local_registry::{CanonicalWriterError, check_canonical_writer};
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let root = std::path::PathBuf::from(format!(
            "/tmp/agent-memory-registry-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let root = root.canonicalize().unwrap();
        let lock = root.join(".agent-memory.lock");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&lock)
            .unwrap();
        assert_eq!(
            check_canonical_writer(&lock, &["capture"]).unwrap_err(),
            CanonicalWriterError::Initializing
        );
        drop(file);
        let chain = agent_memory::fs::DirectoryChain::open(&root).unwrap();
        let descriptor = chain
            .directory()
            .open_at(
                std::ffi::OsStr::new(".agent-memory.lock"),
                libc::O_RDONLY,
                0,
            )
            .unwrap();
        let _held = agent_memory::fs::FlockGuard::try_acquire(
            descriptor,
            agent_memory::fs::LockMode::Exclusive,
        )
        .unwrap();
        assert_eq!(
            check_canonical_writer(&lock, &["capture"]).unwrap_err(),
            CanonicalWriterError::Initializing
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_rejects_oversized_lock_without_modification() {
        use agent_memory::{
            host::Sources,
            local_registry::{LOCK_MAX_BYTES, recover},
        };
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let root = std::path::PathBuf::from(format!(
            "/tmp/agent-memory-registry-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let root = root.canonicalize().unwrap();
        let lock = root.join(".projects.lock");
        let bytes = vec![b'x'; LOCK_MAX_BYTES + 1];
        std::fs::write(&lock, &bytes).unwrap();
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            recover(
                &root.join("projects.json"),
                &lock,
                &["project-create"],
                None,
                "agent-memory/project-lock-recovery-v1",
                Sources::default()
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&lock).unwrap(), bytes);
        std::fs::remove_dir_all(root).unwrap();
    }

    // obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_project_registry_stale_lock_recovery_is_bounded_and_read_back
    #[test]
    fn local_lock_recovery_requires_dead_local_owner_and_matching_preview() {
        use agent_memory::{
            codec::json::Value, developer_identity::encoded, host::Sources, local_registry::recover,
        };
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let root = std::path::PathBuf::from(format!(
            "/tmp/agent-memory-registry-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let root = root.canonicalize().unwrap();
        let mut child = std::process::Command::new("/usr/bin/true")
            .current_dir(&root)
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        let owner = Value::Object(vec![
            (
                "operation_id".into(),
                Value::String("00000000-0000-4000-8000-000000000000".into()),
            ),
            ("operation".into(), Value::String("project-create".into())),
            (
                "host".into(),
                Value::String(agent_memory::host::hostname().unwrap()),
            ),
            ("pid".into(), Value::Int(pid.into())),
            ("start".into(), Value::String("2026-10-22T17:00:00Z".into())),
        ]);
        let lock = root.join(".projects.lock");
        let bytes = encoded(&owner).unwrap();
        std::fs::write(&lock, &bytes).unwrap();
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o600)).unwrap();
        let registry = root.join("projects.json");
        std::fs::write(&registry, b"registry preserved").unwrap();
        let mut live = owner.clone();
        *live.get_mut("pid").unwrap() = Value::Int(std::process::id().into());
        std::fs::write(&lock, encoded(&live).unwrap()).unwrap();
        assert!(
            recover(
                &registry,
                &lock,
                &["project-create"],
                None,
                "agent-memory/project-lock-recovery-v1",
                Sources::default()
            )
            .is_err()
        );
        *live.get_mut("host").unwrap() = Value::String("foreign-host".into());
        *live.get_mut("pid").unwrap() = Value::Int(pid.into());
        std::fs::write(&lock, encoded(&live).unwrap()).unwrap();
        assert!(
            recover(
                &registry,
                &lock,
                &["project-create"],
                None,
                "agent-memory/project-lock-recovery-v1",
                Sources::default()
            )
            .is_err()
        );
        std::fs::write(&lock, &bytes).unwrap();
        let preview = recover(
            &registry,
            &lock,
            &["project-create"],
            None,
            "agent-memory/project-lock-recovery-v1",
            Sources::default(),
        )
        .unwrap();
        assert_eq!(std::fs::read(&lock).unwrap(), bytes);
        let done = recover(
            &registry,
            &lock,
            &["project-create"],
            Some(&preview),
            "agent-memory/project-lock-recovery-v1",
            Sources::default(),
        )
        .unwrap();
        assert_eq!(done.get("verified").and_then(Value::as_bool), Some(true));
        assert_eq!(std::fs::read(&registry).unwrap(), b"registry preserved");
        assert!(!lock.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    // obligation: tests/contracts/test_project_identity.py::ProjectIdentityContract.test_changed_registry_bank_remote_or_authority_refuses_stale_proposals
    #[test]
    fn local_registry_replacement_preserves_external_edit() {
        use agent_memory::{developer_identity::digest, local_registry::replace};
        use std::os::unix::fs::DirBuilderExt;
        let root = std::path::PathBuf::from(format!(
            "/tmp/agent-memory-registry-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let root = root.canonicalize().unwrap();
        let registry = root.join("projects.json");
        let old = b"old registry bytes";
        std::fs::write(&registry, old).unwrap();
        let observed = digest(Some(old)).unwrap();
        std::fs::write(&registry, b"external edit").unwrap();
        assert!(replace(&registry, b"proposed bytes", Some(&observed)).is_err());
        assert_eq!(std::fs::read(&registry).unwrap(), b"external edit");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn canonical_writer_follows_intermediate_symlinks_but_not_bank_symlink() {
        use agent_memory::local_registry::{CanonicalWriterError, check_canonical_writer};
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, symlink};
        let root = std::path::PathBuf::from(format!(
            "/tmp/agent-memory-registry-{}",
            agent_memory::host::uuid4().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let root = root.canonicalize().unwrap();
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("real"))
            .unwrap();
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("real/bank"))
            .unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.join("real/bank/.agent-memory.lock"))
            .unwrap();
        symlink(root.join("real"), root.join("intermediate")).unwrap();
        assert_eq!(
            check_canonical_writer(
                &root.join("intermediate/bank/.agent-memory.lock"),
                &["capture"]
            )
            .unwrap_err(),
            CanonicalWriterError::Initializing
        );
        symlink(root.join("real/bank"), root.join("bank-link")).unwrap();
        assert!(matches!(
            check_canonical_writer(&root.join("bank-link/.agent-memory.lock"), &["capture"]),
            Err(CanonicalWriterError::Contract(_))
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}

mod policy_contract {
    // obligation: tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_every_adr_authority_domain_preserves_the_full_matrix_and_no_memory_proof
    #[test]
    fn authority_policy_matrix() {
        use agent_memory::{authority, codec::json::Value};
        let model = &*authority::MODEL;
        let rows = model.get("domains").unwrap().as_array().unwrap();
        let expected = [
            ("implementation", vec!["code", "test"]),
            ("intent", vec!["approved-planning"]),
            ("delivery", vec!["issue", "ci", "integration", "release"]),
            ("coordination", vec!["coordination"]),
            ("requirement", vec!["user-statement", "approved-decision"]),
            ("historical-observation", vec!["memory-event"]),
            ("procedure", vec!["reviewed-procedure"]),
        ];
        for (name, kinds) in expected {
            let row = rows
                .iter()
                .find(|r| r.get("fact").and_then(Value::as_str) == Some(name))
                .unwrap();
            assert_eq!(
                row.get("source-kinds")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>(),
                kinds
            );
        }
        assert_eq!(model.get("memory-is-proof"), Some(&Value::Bool(false)));
        assert_eq!(
            model.get("landing-time-is-authority"),
            Some(&Value::Bool(false))
        );
    }

    // obligation: tests/contracts/test_scope_policy.py::ScopePolicyContract.test_all_conditions_must_match_and_missing_context_never_becomes_universal
    #[test]
    fn scope_component_and_conditions() {
        use agent_memory::{
            codec::json::{self, DecodePolicy, Value},
            scopes,
        };
        let parse = |s| json::decode(s, DecodePolicy::default()).unwrap();
        let req = parse(
            r#"{"bank_project_id":"123e4567-e89b-42d3-a456-426614174000","project_id":"123e4567-e89b-42d3-a456-426614174000","scope":"component:src/api","conditions":{"branch":"main"},"context":{"branch":"main","component":"src/api/retry.py"}}"#,
        );
        assert_eq!(
            scopes::decide(&req).unwrap().get("applicable"),
            Some(&Value::Bool(true))
        );
        let req = parse(
            r#"{"bank_project_id":"123e4567-e89b-42d3-a456-426614174000","project_id":"123e4567-e89b-42d3-a456-426614174000","scope":"component:src/api","conditions":{"branch":"main"},"context":{"component":"src/api2/file"}}"#,
        );
        let decision = scopes::decide(&req).unwrap();
        assert_eq!(decision.get("applicable"), Some(&Value::Bool(false)));
        assert_eq!(
            decision.get("reasons"),
            Some(&parse(
                r#"["different-context:component","missing-context:branch"]"#
            ))
        );
    }

    // obligation: tests/contracts/test_scope_policy.py::ScopePolicyContract.test_unknown_personal_federation_and_noncanonical_conditions_are_invalid
    #[test]
    fn scope_rejects_noncanonical_paths() {
        use agent_memory::{codec::json::Value, scopes};
        for scope in [
            "personal:me",
            "component:/etc",
            "component:../other",
            "component:src/../other",
            "component:src/./api",
            "component:src//api",
            "component:src/api/",
            "component:C:drive",
            "component:.",
            "branch:",
            "branch:x\ncommand",
        ] {
            assert!(
                scopes::parse_scope(&Value::String(scope.into())).is_err(),
                "{scope}"
            );
        }
    }

    // obligation: tests/contracts/test_scope_policy.py::ScopePolicyContract.test_default_is_one_repository_bank_and_developer_does_not_select_a_namespace
    #[test]
    fn scope_default_and_developer() {
        use agent_memory::{
            codec::json::{self, DecodePolicy, Value},
            scopes,
        };
        let parse = |s| json::decode(s, DecodePolicy::default()).unwrap();
        let a = parse(
            r#"{"bank_project_id":"123e4567-e89b-42d3-a456-426614174000","project_id":"123e4567-e89b-42d3-a456-426614174000"}"#,
        );
        let b = parse(
            r#"{"bank_project_id":"123e4567-e89b-42d3-a456-426614174000","project_id":"123e4567-e89b-42d3-a456-426614174000","developer_id":"d138bba0-0f29-4bc4-8434-ea93497323b1"}"#,
        );
        let result = scopes::decide(&a).unwrap();
        assert_eq!(result, scopes::decide(&b).unwrap());
        assert_eq!(
            result.get("scope"),
            Some(&Value::String("repository".into()))
        );
        assert_eq!(result.get("applicable"), Some(&Value::Bool(true)));
        assert_eq!(
            result.get("identity_authenticated"),
            Some(&Value::Bool(false))
        );
        assert_eq!(result.get("writes_performed"), Some(&Value::Bool(false)));
    }

    // obligation: tests/contracts/test_authority_policy.py::AuthorityPolicyContract.test_peer_conflict_routes_to_detecting_developer_and_known_other_author_without_last_landing_wins
    #[test]
    fn authority_typed_conflicts() {
        use agent_memory::{
            authority,
            codec::json::{self, DecodePolicy, Value},
        };
        let request=json::decode(r#"{"fact":"requirement","fact_key":"wall","project_id":"123e4567-e89b-42d3-a456-426614174000","detecting_developer_id":"d138bba0-0f29-4bc4-8434-ea93497323b1","memory":{"id":"9333c715-e204-49c7-84fa-5794ee129f72","version":2,"project_id":"123e4567-e89b-42d3-a456-426614174000","value":1,"scope":"repository","conditions":{},"developer_id":"4c2bdb46-2d79-4d5d-8e37-6c259d82c0f1","evidence":[]},"sources":[{"kind":"approved-decision","ref":"a","project_id":"123e4567-e89b-42d3-a456-426614174000","fact_key":"wall","current":true,"applicable":true,"qualified":true,"value":true},{"kind":"approved-decision","ref":"b","project_id":"123e4567-e89b-42d3-a456-426614174000","fact_key":"wall","current":true,"applicable":true,"qualified":true,"value":1}]}"#,DecodePolicy::default()).unwrap();
        let result = authority::assess(&request).unwrap();
        assert_eq!(
            result.get("action").and_then(Value::as_str),
            Some("clarify")
        );
        assert_eq!(result.get("correction_candidate"), Some(&Value::Null));
        assert_eq!(
            result
                .get("clarification")
                .unwrap()
                .get("detecting_developer_id"),
            request.get("detecting_developer_id")
        );
    }

    // obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_complete_map_count_is_exact_and_partial_inventory_is_at_least
    #[test]
    fn complete_map_capacity_count() {
        use agent_memory::{bank_capacity, codec::json::Value};
        let files = Value::Object(
            (0..65537)
                .map(|i| (format!("topics/{i}.md"), Value::String("x".into())))
                .collect(),
        );
        let error = bank_capacity::check_files(&files).unwrap_err();
        let details = bank_capacity::details(&error);
        assert_eq!(
            details.get("failure_code").and_then(Value::as_str),
            Some("bank-source-count-capacity")
        );
        assert_eq!(
            details.get("capacity_measurement").and_then(Value::as_str),
            Some("exact")
        );
        assert_eq!(
            details.get("capacity_unit").and_then(Value::as_str),
            Some("sources")
        );
        assert_eq!(
            details.get("failure_path").and_then(Value::as_str),
            Some(".")
        );
    }

    // obligation: tests/contracts/test_bounded_bank_observation.py::BoundedBankObservation.test_complete_utf8_source_boundary_and_file_only_validator
    #[test]
    fn capacity_uses_utf8_bytes_and_redacts_contents() {
        use agent_memory::{bank_capacity, codec::json::Value};
        let text = format!("PRIVATE_BODY_CANARY café {}", "x".repeat(16384));
        let len = text.len();
        let files = Value::Object(vec![("projectbrief.md".into(), Value::String(text))]);
        let error = bank_capacity::check_files(&files).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_BODY_CANARY"));
        let details = bank_capacity::details(&error);
        assert_eq!(
            details.get("failure_code").and_then(Value::as_str),
            Some("bank-source-byte-capacity")
        );
        assert_eq!(
            details.get("failure_path").and_then(Value::as_str),
            Some("projectbrief.md")
        );
        assert_eq!(
            details.get("capacity_observed"),
            Some(&Value::Int(len.into()))
        );
        assert_eq!(
            details.get("capacity_measurement").and_then(Value::as_str),
            Some("exact")
        );
    }

    // Baseline scopes.py:54-56: the scope is parsed before conditions are validated.
    #[test]
    fn scope_refusal_precedes_conditions_refusal() {
        use agent_memory::{
            codec::json::{self, DecodePolicy},
            scopes,
        };
        let request = json::decode(
            r#"{"bank_project_id":"123e4567-e89b-42d3-a456-426614174000","project_id":"123e4567-e89b-42d3-a456-426614174000","scope":"personal:me","conditions":[]}"#,
            DecodePolicy::default(),
        )
        .unwrap();
        assert_eq!(
            scopes::decide(&request).unwrap_err().message(),
            "Scope requires a known route kind"
        );
    }
}

mod handoff_contract {
    // Partial: exercises reference classification; installed publication, independent clones,
    // checkpoint preparation and retrieval require the later checkpoint and sharing tiers.
    // obligation: tests/contracts/test_portable_handoff.py::PortableHandoff.test_published_handoff_filters_other_task_and_branch_but_remains_explicitly_inspectable
    #[test]
    fn handoff_reference_classification() {
        use agent_memory::handoff::reference_path;
        assert_eq!(
            reference_path("[source](../code.py)"),
            (Some("../code.py".into()), None)
        );
        assert_eq!(
            reference_path("https://example.invalid/x"),
            (None, Some("external-unverified"))
        );
        assert_eq!(
            reference_path("%2Fprivate%2Fsecret"),
            (None, Some("outside-uninspected"))
        );
        assert_eq!(
            reference_path("/private/secret"),
            (None, Some("outside-uninspected"))
        );
        assert_eq!(
            reference_path("https://[invalid]/x"),
            (None, Some("unavailable"))
        );
        assert_eq!(
            reference_path("https://[::1]/x"),
            (None, Some("external-unverified"))
        );
        assert_eq!(
            reference_path("https://example.com：443/x"),
            (None, Some("unavailable"))
        );
    }

    // Partial: qualification mechanics only; locked checkpoint application requires later tiers.
    // obligation: tests/contracts/test_portable_handoff.py::PortableHandoff.test_source_pins_refuse_preapply_and_locked_changes_and_outside_reference_never_opens
    #[test]
    fn handoff_rejects_invented_uninspected_bytes() {
        use agent_memory::{
            codec::json::{DecodePolicy, decode},
            handoff,
        };
        let input = r#"{"schema":"agent-memory/checkpoint-evidence-v1","references":[{"ref":"/private","kind":"outside-uninspected","sha256":null,"bytes":1,"commit":null,"blob":null}],"observed_checkout":{"head":null,"branch":null},"publication_verified":false,"identity_authenticated":false}"#;
        let value = decode(input, DecodePolicy::default()).unwrap();
        assert_eq!(
            handoff::validate_observation(&value)
                .unwrap_err()
                .to_string(),
            "Uninspected references cannot contain invented byte evidence"
        );
    }

    // Partial: cue detection only; full installed retrieval/assembly requires later tiers.
    // obligation: tests/contracts/test_portable_handoff.py::PortableHandoff.test_wrong_heading_levels_cannot_turn_managed_checkpoint_into_manual_orientation
    #[test]
    fn handoff_managed_cues_exclude_examples_and_casefold() {
        use agent_memory::handoff::managed_cues;
        let names = vec!["Checkpoint conditions".to_owned(), "STRASSE".to_owned()];
        let body =
            "```markdown\n### Checkpoint conditions\n```\n**checkpoint conditions**\nStraße\n";
        let cues = managed_cues(body, &names);
        assert_eq!(cues.len(), 2);
        assert!(cues.contains(&body.find("**checkpoint").unwrap()));
        assert!(cues.contains(&body.find("Straße").unwrap()));
    }

    // Partial: duplicate/malformed cues; full checkpoint parser and installed retrieval require later tiers.
    // obligation: tests/contracts/test_portable_handoff.py::PortableHandoff.test_mixed_valid_and_malformed_reserved_sections_cannot_hide_duplicate_conditions
    #[test]
    fn handoff_detects_mixed_reserved_labels() {
        let body = "## Checkpoint conditions\n\n{}\n\n<h2>Checkpoint conditions</h2>\n";
        let cues = agent_memory::handoff::managed_cues(body, &["Checkpoint conditions".into()]);
        assert_eq!(cues.len(), 2);
    }

    // Baseline handoff.py:225-235: a missing identity key in the current
    // observation (KeyError) means "not reusable", never a null match.
    #[test]
    fn handoff_reusable_does_not_treat_missing_identity_as_null() {
        use agent_memory::{
            codec::json::{DecodePolicy, Value, decode},
            error::Result,
            handoff::{self, Checkpoint},
        };
        struct Parser;
        impl Checkpoint for Parser {
            fn headings(&self, body: &str) -> Result<Vec<(usize, usize, String)>> {
                Ok(vec![(0, body.len(), handoff::SECTION.to_owned())])
            }
            fn fields(&self) -> Vec<String> {
                vec![]
            }
            fn extra(&self) -> Vec<String> {
                vec![]
            }
            fn list_text(&self, _: &[String]) -> Result<String> {
                Ok(String::new())
            }
        }
        let prior = r#"{"schema":"agent-memory/checkpoint-evidence-v1","references":[{"ref":"x","kind":"unavailable","sha256":null,"bytes":null,"commit":null,"blob":null}],"observed_checkout":{"head":null,"branch":null},"publication_verified":false,"identity_authenticated":false}"#;
        let body = format!("## {}\n```json\n{}\n```", handoff::SECTION, prior);
        let current = decode(
            r#"{"references":[{"ref":"x","kind":"unavailable","sha256":null,"bytes":null}]}"#,
            DecodePolicy::default(),
        )
        .unwrap();
        let result: Value = handoff::reusable_with(&body, &current, &Parser);
        assert_eq!(result, current);
    }
}

mod records_slice {
    mod records_contract {
        use agent_memory::{
            codec::json::{self, DecodePolicy, Value},
            records,
        };
        fn proposal() -> Value {
            json::decode(
                include_str!("../skills/agent-memory/templates/record-proposal.example.json"),
                DecodePolicy::default(),
            )
            .unwrap()
        }
        fn meta(v: &mut Value) -> &mut Value {
            v.get_mut("metadata").unwrap()
        }
        fn set(v: &mut Value, key: &str, value: Value) {
            if let Some(old) = v.get_mut(key) {
                *old = value;
            } else if let Value::Object(items) = v {
                items.push((key.into(), value));
            }
        }
        fn s(v: &str) -> Value {
            Value::String(v.into())
        }
        fn render(v: &Value, required: &[&str]) -> agent_memory::lifecycle::CommandResult {
            records::command("render", Ok(Some(v.clone())), Ok(None), required)
        }
        fn roundtrip(v: &Value, required: &[&str]) {
            let r = render(v, required);
            assert_eq!(r.exit_code, 0, "{}", r.stdout);
            let content = r
                .value
                .unwrap()
                .get("content")
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned();
            let (_, m, b) = records::parse(&content, required).unwrap();
            assert_eq!(&m, v.get("metadata").unwrap());
            assert_eq!(b, v.get("body").unwrap().as_str().unwrap());
        }

        // obligation: tests/contracts/test_records.py::RecordContract.test_all_seven_kinds_roundtrip_with_origin_conditions_evidence_and_exact_times
        #[test]
        fn seven_kinds() {
            let def = records::definition();
            let kinds = def
                .get("record")
                .unwrap()
                .get("properties")
                .unwrap()
                .get("kind")
                .unwrap()
                .get("enum")
                .unwrap()
                .as_array()
                .unwrap();
            assert_eq!(kinds.len(), 7);
            for kind in kinds {
                let mut v = proposal();
                let m = meta(&mut v);
                set(m, "kind", kind.clone());
                set(
                    m,
                    "recorded-at",
                    s("2026-10-22T21:00:00.000000000000000001Z"),
                );
                set(
                    m,
                    "updated",
                    s("2026-10-22T14:00:00.000000000000000002-07:00"),
                );
                roundtrip(&v, &[]);
            }
        }
        // obligation: tests/contracts/test_records.py::RecordContract.test_active_policy_and_stale_versions_remain_explicit_without_history_replay
        #[test]
        fn active_policy_and_stale() {
            let mut v = proposal();
            let m = meta(&mut v);
            for (k, x) in [
                ("status", s("active")),
                ("trust", s("reviewed")),
                ("visibility", s("visible")),
                ("admission-reason", s("inspected")),
                ("confidence", Value::Null),
                ("redaction", s("reviewed")),
            ] {
                set(m, k, x);
            }
            roundtrip(&v, &["confidence", "redaction"]);
            for status in ["quarantined", "revoked", "expired"] {
                let mut x = v.clone();
                set(meta(&mut x), "status", s(status));
                assert_eq!(render(&x, &[]).exit_code, 1);
                set(meta(&mut x), "state-reason", s("historical"));
                roundtrip(&x, &[]);
            }
        }
        // obligation: tests/contracts/test_records.py::RecordContract.test_unicode_metadata_separators_roundtrip_without_losing_values_or_body_text
        #[test]
        fn unicode_separators() {
            for sep in ['\u{85}', '\u{2028}', '\u{2029}'] {
                let mut v = proposal();
                set(
                    meta(&mut v).get_mut("source").unwrap(),
                    "actor",
                    s(&format!("Zoë{sep}李")),
                );
                set(&mut v, "body", s(&format!("Body 言葉{sep} preserved")));
                let r = render(&v, &[]);
                assert_eq!(r.exit_code, 0);
                let out = r.value.unwrap();
                let content = out.get("content").unwrap().as_str().unwrap();
                assert!(!content.split("\n---\n").next().unwrap().contains(sep));
                roundtrip(&v, &[]);
            }
        }
        // obligation: tests/contracts/test_records.py::RecordContract.test_arbitrarily_large_integer_confidence_returns_structured_rejection
        #[test]
        fn huge_confidence() {
            for sign in ["", "-"] {
                let mut v = proposal();
                let n = json::Integer::new(&format!("{sign}1{}", "0".repeat(400))).unwrap();
                set(meta(&mut v), "confidence", Value::Int(n));
                let r = render(&v, &[]);
                assert_eq!(r.exit_code, 1);
                assert_eq!(r.value.unwrap().get("valid"), Some(&Value::Bool(false)));
            }
        }
        // obligation: tests/contracts/test_records.py::RecordContract.test_malformed_identity_source_time_links_and_metadata_are_rejected_without_echo
        #[test]
        fn invalid_redaction() {
            for (key, value) in [
                ("id", s("not-a-uuid")),
                ("version", Value::Bool(true)),
                ("scope", s("personal:me")),
                ("evidence", Value::Array(vec![])),
                ("confidence", Value::Bool(true)),
                ("unexpected", s("SECRET_RECORD_CANARY")),
            ] {
                let mut v = proposal();
                set(meta(&mut v), key, value);
                let r = render(&v, &[]);
                assert_eq!(r.exit_code, 1, "{key}");
                assert!(!r.stdout.contains("SECRET_RECORD_CANARY"));
                assert!(r.stderr.is_empty());
            }
        }
        // obligation: tests/contracts/test_records.py::RecordContract.test_memory_landing_does_not_widen_unmerged_or_unknown_conditions
        #[test]
        fn unknown_applicability() {
            let mut v = proposal();
            let m = meta(&mut v);
            set(m, "applicability", s("unknown"));
            let request = Value::Object(vec![
                (
                    "bank_project_id".into(),
                    m.get("project-id").unwrap().clone(),
                ),
                ("context".into(), m.get("conditions").unwrap().clone()),
            ]);
            let result = records::applicability(m, &request).unwrap();
            assert_eq!(result.get("applicable"), Some(&Value::Bool(false)));
            assert!(
                result
                    .get("reasons")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .contains(&s("unknown-applicability"))
            );
        }
        // obligation: tests/contracts/test_records.py::RecordContract.test_optional_event_loss_preserves_readable_current_record_and_stale_state
        #[test]
        fn optional_history_event() {
            let v = proposal();
            let m = v.get("metadata").unwrap();
            let mut event = Value::Object(vec![
                ("schema".into(), s("agent-memory/history-event-v1")),
                ("id".into(), m.get("id").unwrap().clone()),
                ("project-id".into(), m.get("project-id").unwrap().clone()),
                ("kind".into(), s("memory-revoked")),
                (
                    "record".into(),
                    Value::Object(vec![
                        ("id".into(), m.get("id").unwrap().clone()),
                        ("version".into(), Value::Int(1.into())),
                    ]),
                ),
                ("actor".into(), s("detector")),
                ("recorded-at".into(), s("2026-10-22T21:01:00Z")),
                ("evidence".into(), m.get("evidence").unwrap().clone()),
            ]);
            assert_eq!(
                records::validate(&event, "", &[]).unwrap().0,
                "optional-history-event"
            );
            set(&mut event, "kind", s("memory-deleted"));
            assert!(records::validate(&event, "", &[]).is_err());
        }
        // obligation: tests/contracts/test_records.py::RecordContract.test_copied_artifact_and_five_field_tombstone_exception_require_no_harness_or_state
        #[test]
        fn tombstone_requires_wiring() {
            let id = s("a9abac6f-c33e-44c9-b40f-7b2ebafda860");
            let marker = Value::Object(vec![
                ("id".into(), id.clone()),
                ("scope".into(), id.clone()),
                ("deleted-at".into(), s("2026-10-22T21:02:00Z")),
                ("deletion-authority".into(), id),
                ("reason-class".into(), s("sensitive")),
            ]);
            assert_eq!(
                records::validate_with_parser(&marker, "", &[], None)
                    .unwrap_err()
                    .message(),
                "Tombstone parser is not registered"
            );
            assert!(records::validate_with_parser(&marker, "", &["confidence"], None).is_err());
        }

        // Baseline records.py:79-85: every evidence string is validated before duplicates.
        #[test]
        fn evidence_validates_before_checking_duplicates() {
            let values = Value::Array(vec![s("same"), s("same"), Value::Null]);
            assert_eq!(
                records::evidence(&values).unwrap_err().message(),
                "A nonempty explicit metadata string is required"
            );
        }

        // Baseline records.py:52-59: `{'id': ..., 'version': 1} in derived-from`
        // uses Python equality, so `True` and `1.0` match version 1.
        #[test]
        fn source_capture_uses_python_numeric_equality() {
            let capture = "a9abac6f-c33e-44c9-b40f-7b2ebafda860";
            let source = "ce66f980-2ff9-4c89-9c50-0a7a901f76ec";
            let path = format!("history/sources/{capture}/{source}.md");
            for version in [Value::Bool(true), Value::Float(1.0)] {
                let metadata = Value::Object(vec![
                    ("id".into(), s(source)),
                    (
                        "derived-from".into(),
                        Value::Array(vec![Value::Object(vec![
                            ("id".into(), s(capture)),
                            ("version".into(), version),
                        ])]),
                    ),
                ]);
                assert_eq!(
                    records::source_capture_id(&path, &metadata).as_deref(),
                    Some(capture)
                );
            }
        }

        // Baseline records.py:219-235: `render` never reads `--file`.
        #[test]
        fn render_ignores_unused_content_error() {
            let result = records::command(
                "render",
                Ok(Some(proposal())),
                Err(agent_memory::error::ContractError::new("unused")),
                &[],
            );
            assert_eq!(result.exit_code, 0);
        }
    }
}

mod private_output_contract {
    use agent_memory::{codec::json::Value, sharing_limits};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    struct PrivateRoot(PathBuf);
    impl PrivateRoot {
        fn new() -> Self {
            let path = Path::new("/tmp").join(format!(
                "am-t1-a2-private-{}",
                agent_memory::host::uuid4().unwrap()
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path.canonicalize().unwrap())
        }
        fn child(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            path
        }
    }
    impl Drop for PrivateRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn value() -> Value {
        Value::Object(vec![
            ("operation".into(), Value::String("fixture".into())),
            ("value".into(), Value::String("unchanged".into())),
        ])
    }

    // obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_private_output_preflight_preserves_identical_retry
    #[test]
    fn private_output_identical_retry_preserves_exact_bytes_and_mode() {
        let root = PrivateRoot::new();
        let repo = root.child("repository");
        let runtime = root.child("runtime");
        let target = root.0.join("same-output.json");
        let first = sharing_limits::write_private(&target, &value(), &repo, &runtime).unwrap();
        let expected = b"{\n  \"operation\": \"fixture\",\n  \"value\": \"unchanged\"\n}\n";
        assert_eq!(std::fs::read(&target).unwrap(), expected);
        assert_eq!(std::fs::metadata(&target).unwrap().mode() & 0o777, 0o600);
        assert_eq!(
            sharing_limits::preflight_private_output(&target, &repo, &runtime).unwrap(),
            target.canonicalize().unwrap()
        );
        let second = sharing_limits::write_private(&target, &value(), &repo, &runtime).unwrap();
        assert_eq!(first, second);
        assert_eq!(std::fs::read(&target).unwrap(), expected);
    }

    // obligation: tests/contracts/test_memory_refresh.py::MemoryRefreshContract.test_public_sharing_commands_reject_unsafe_output_before_work
    #[test]
    fn unsafe_private_outputs_refuse_without_overwriting_existing_files() {
        let root = PrivateRoot::new();
        let repo = root.child("repository");
        let runtime = root.child("runtime");
        let unsafe_parent = root.child("unsafe");
        std::fs::set_permissions(&unsafe_parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        let occupied = root.0.join("occupied.json");
        std::fs::write(&occupied, b"preserve original").unwrap();
        std::fs::set_permissions(&occupied, std::fs::Permissions::from_mode(0o600)).unwrap();
        let redirected = root.0.join("redirected.json");
        std::os::unix::fs::symlink(&occupied, &redirected).unwrap();
        for target in [unsafe_parent.join("new.json"), redirected] {
            let error =
                sharing_limits::preflight_private_output(&target, &repo, &runtime).unwrap_err();
            assert!(error.message().contains("owner-only"));
            assert!(!error.message().contains(target.to_str().unwrap()));
        }
        assert!(sharing_limits::write_private(&occupied, &value(), &repo, &runtime).is_err());
        assert_eq!(std::fs::read(&occupied).unwrap(), b"preserve original");
        assert!(!unsafe_parent.join("new.json").exists());
    }

    #[test]
    fn private_inputs_follow_named_symlinks_but_never_truncate_the_bound() {
        let root = PrivateRoot::new();
        let source = root.0.join("input.json");
        let link = root.0.join("alias.json");
        std::fs::write(&source, b"{}").unwrap();
        std::os::unix::fs::symlink(&source, &link).unwrap();
        assert_eq!(
            sharing_limits::read_json(&link, 2).unwrap(),
            Value::Object(vec![])
        );
        assert_eq!(
            sharing_limits::read_bytes(&link, 1).unwrap_err().message(),
            "Complete private input exceeds its explicit encoded budget; no truncation"
        );
        assert_eq!(std::fs::read(source).unwrap(), b"{}");
    }

    // Baseline sharing_limits.py:86-87: len() of the JSON values and Python truthiness.
    #[test]
    fn inspection_receipt_uses_json_lengths_and_truthiness() {
        use agent_memory::codec::json::{self, DecodePolicy};
        let root = PrivateRoot::new();
        let repo = root.child("repository");
        let runtime = root.child("runtime");
        let value = json::decode(
            r#"{"schema":"agent-memory/semantic-sharing-inspection-v2","changed":"éx","pages":{"a":1},"pending_selections":[],"known_causes":"pending","withheld_sources":[]}"#,
            DecodePolicy::default(),
        )
        .unwrap();
        let receipt =
            sharing_limits::write_private(&root.0.join("inspection.json"), &value, &repo, &runtime)
                .unwrap();
        assert_eq!(
            receipt.get("changed_components"),
            Some(&Value::Int(2.into()))
        );
        assert_eq!(receipt.get("review_pages"), Some(&Value::Int(1.into())));
        assert_eq!(
            receipt.get("selection_required"),
            Some(&Value::Int(0.into()))
        );
        assert_eq!(receipt.get("conflicts_pending"), Some(&Value::Bool(true)));
    }
}

mod native_guide_structure {
    use agent_memory::codec::json::Value;
    use agent_memory::pack_validation as pack;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    fn acyclic(
        path: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        visiting: &mut BTreeSet<String>,
        complete: &mut BTreeSet<String>,
    ) {
        assert!(
            !visiting.contains(path),
            "Mandatory loading cycle at {path}"
        );
        if complete.contains(path) {
            return;
        }
        visiting.insert(path.to_owned());
        for child in graph.get(path).into_iter().flatten() {
            acyclic(child, graph, visiting, complete);
        }
        visiting.remove(path);
        complete.insert(path.to_owned());
    }

    /// The public validator intentionally preserves its fail-fast refusal order.
    /// Check every real guide's structure independently of unported commands,
    /// without supplying a permissive command surface or rewriting the pack.
    #[test]
    fn committed_native_pack_preserves_sections_links_loading_and_reachability() {
        let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/skills/agent-memory"));
        let files = pack::inventory(root).unwrap();
        let mut documents = BTreeMap::new();
        let mut metadata = BTreeMap::new();
        let mut originals = BTreeMap::new();
        let mut shell_blocks = BTreeMap::new();
        for (path, (raw, _)) in &files {
            if !path.ends_with(".md") {
                continue;
            }
            let document = std::str::from_utf8(raw).unwrap();
            let (meta, body) = if path == "SKILL.md" {
                let (meta, body) = pack::frontmatter(
                    document,
                    &[
                        "name",
                        "description",
                        "metadata",
                        "license",
                        "compatibility",
                    ],
                    path,
                )
                .unwrap();
                assert_eq!(
                    meta.get("name").and_then(Value::as_str),
                    Some("agent-memory")
                );
                assert!(pack::semantic_version(
                    meta.get("metadata").unwrap().get("version").unwrap()
                ));
                (meta, body)
            } else {
                pack::frontmatter(document, &["kind", "domain", "file-operations"], path).unwrap()
            };
            let (prose, shells) = pack::markdown(body).unwrap();
            documents.insert(path.clone(), prose);
            metadata.insert(path.clone(), meta);
            originals.insert(path.clone(), body.to_owned());
            shell_blocks.insert(path.clone(), shells);
        }
        let mut graph: BTreeMap<String, BTreeSet<String>> = documents
            .keys()
            .map(|path| (path.clone(), BTreeSet::new()))
            .collect();
        let mut mandatory = graph.clone();
        let anchors: BTreeMap<_, _> = documents
            .iter()
            .map(|(path, prose)| (path.clone(), pack::anchors(prose)))
            .collect();
        for (path, prose) in &documents {
            for (_, target) in pack::reference_definitions(prose).unwrap() {
                pack::destination(path, &target, &files).unwrap();
            }
            for link in pack::link_occurrences(prose, None).unwrap() {
                let (destination, anchor) = pack::destination(path, &link.target, &files).unwrap();
                if let Some(destination) = destination {
                    assert!(
                        anchor.is_empty()
                            || anchors
                                .get(&destination)
                                .is_some_and(|set| set.contains(&anchor)),
                        "{path}: missing anchor in {destination}: {anchor}"
                    );
                    if !link.image && documents.contains_key(&destination) {
                        graph.get_mut(path).unwrap().insert(destination);
                    }
                }
            }
            for target in pack::mandatory_targets(prose, path, &files, None).unwrap() {
                if documents.contains_key(&target) {
                    mandatory.get_mut(path).unwrap().insert(target);
                }
            }
            // The literal shell scanner runs even when a command is not ported.
            for block in &shell_blocks[path] {
                for line in block.replace("\\\n", " ").lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let argv = pack::command_argv(line, path).unwrap();
                    if argv.first().is_some_and(|program| program == "git") {
                        pack::git_operation(&argv, path).unwrap();
                    } else {
                        assert!(
                            argv.first().is_some_and(|program| {
                                matches!(program.as_str(), "am" | "agent-memory")
                            }),
                            "{path}: {line}"
                        );
                    }
                }
            }
        }
        for (path, meta) in &metadata {
            if meta.get("kind").and_then(Value::as_str) != Some("operation") {
                continue;
            }
            let prose = &documents[path];
            let headings: Vec<_> = prose
                .lines()
                .filter_map(|line| line.strip_prefix("## "))
                .collect();
            assert_eq!(headings, pack::SECTIONS, "{path}");
            for heading in pack::SECTIONS {
                let section = originals[path]
                    .split_once(&format!("## {heading}\n"))
                    .unwrap()
                    .1
                    .split("\n## ")
                    .next()
                    .unwrap();
                assert!(!section.trim().is_empty(), "{path}: empty {heading}");
            }
            let domain = meta.get("domain").and_then(Value::as_str).unwrap();
            let parent = format!("{domain}/GUIDE.md");
            assert_eq!(
                metadata[&parent].get("kind").and_then(Value::as_str),
                Some("domain"),
                "{path}"
            );
            let context = prose
                .split_once("## Required context\n")
                .unwrap()
                .1
                .split("\n## ")
                .next()
                .unwrap();
            assert!(
                pack::mandatory_targets(context, path, &files, None)
                    .unwrap()
                    .contains(&parent),
                "{path}: required owner"
            );
            assert!(graph[&parent].contains(path), "{path}: owner navigation");
            mandatory.get_mut(path).unwrap().insert(parent);
            assert!(
                !shell_blocks[path].is_empty() || meta.get("file-operations").is_some(),
                "{path}: missing command/file-only operation"
            );
        }
        let route = documents["SKILL.md"]
            .split_once("## Route\n")
            .unwrap()
            .1
            .split("\n## ")
            .next()
            .unwrap();
        for row in route.lines().filter(|line| line.starts_with('|')) {
            let targets = pack::links(row, None).unwrap();
            if let [(_, domain), (_, operation)] = targets.as_slice() {
                if metadata
                    .get(operation)
                    .and_then(|meta| meta.get("kind"))
                    .and_then(Value::as_str)
                    == Some("operation")
                {
                    let owner = metadata[operation]
                        .get("domain")
                        .and_then(Value::as_str)
                        .unwrap();
                    assert_eq!(domain, &format!("{owner}/GUIDE.md"), "{row}");
                }
            }
        }
        let mut reached = BTreeSet::new();
        let mut pending = vec!["SKILL.md".to_owned()];
        while let Some(path) = pending.pop() {
            if reached.insert(path.clone()) {
                pending.extend(graph[&path].iter().cloned());
            }
        }
        assert_eq!(reached, documents.keys().cloned().collect());
        let mut visiting = BTreeSet::new();
        let mut complete = BTreeSet::new();
        for path in mandatory.keys() {
            acyclic(path, &mandatory, &mut visiting, &mut complete);
        }
    }
}
