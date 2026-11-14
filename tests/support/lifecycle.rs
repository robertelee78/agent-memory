use super::{RunOptions, World};
use agent_memory::codec::json::{self, Value};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

pub fn source(world: &World) -> PathBuf {
    let root = world.root.join("development source with spaces");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        &root.join("skills/agent-memory"),
    );
    std::fs::create_dir_all(root.join("target/release")).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_am"), root.join("target/release/am")).unwrap();
    mode(&root.join("target/release/am"), 0o755);
    root
}

pub fn mode(path: &Path, bits: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap();
}

pub fn run(world: &World, args: &[&str], expected: i32) -> Value {
    let output = world
        .am_with(
            std::iter::once("--json").chain(args.iter().copied()),
            RunOptions {
                check: false,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(
        output.status,
        agent_memory::process::Status::Code(expected),
        "{output:?}"
    );
    json::decode_bytes(&output.stdout, Default::default())
        .unwrap_or_else(|error| panic!("{error}: {output:?}"))
}

pub fn install(world: &World, root: &Path, options: &[&str]) -> Value {
    let mut args = vec!["install", "--development-source", root.to_str().unwrap()];
    args.extend_from_slice(options);
    let value = run(world, &args, 0);
    assert_eq!(get(&value, "verified"), &Value::Bool(true), "{value:?}");
    value
}

pub fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

pub fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    get(value, key).as_str().unwrap()
}

pub fn read(path: &Path) -> Value {
    json::decode_bytes(&std::fs::read(path).unwrap(), Default::default()).unwrap()
}

pub fn save(path: &Path, value: &Value) {
    std::fs::write(path, json::encode(value, json::DEFAULT).unwrap()).unwrap();
}

pub fn state(world: &World) -> PathBuf {
    world.paths["state"].join("agent-memory/install")
}

pub fn command(receipt: &Value) -> PathBuf {
    PathBuf::from(text(receipt, "bin_dir")).join("am")
}

pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    fn walk(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
        let metadata = std::fs::symlink_metadata(path).unwrap();
        let body = if metadata.is_symlink() {
            std::fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else if metadata.is_file() {
            std::fs::read(path).unwrap()
        } else {
            Vec::new()
        };
        result.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            (metadata.mode(), body),
        );
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                walk(root, &entry.unwrap().path(), result);
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}

pub fn fault(
    world: &World,
    args: &[&str],
    stage: &str,
    fault_mode: &str,
) -> agent_memory::process::Output {
    let overrides = [
        (
            OsString::from("AGENT_MEMORY_FAULT_STAGE"),
            OsString::from(stage),
        ),
        (
            OsString::from("AGENT_MEMORY_FAULT_MODE"),
            OsString::from(fault_mode),
        ),
    ];
    world
        .am_with(
            std::iter::once("--json").chain(args.iter().copied()),
            RunOptions {
                check: false,
                env: &overrides,
                ..RunOptions::default()
            },
        )
        .unwrap()
}

pub fn bank(world: &World, repo: &Path) -> PathBuf {
    let proposal = world.root.join("bank-input.json");
    std::fs::write(&proposal, br##"{"project-id":"8e0b66aa-2be5-4fa3-b47f-1245474aa7dd","router-updated":"2026-10-22T17:00:00Z","contexts":{"projectbrief.md":{"body":"# Project\n\nPreserve durable shared learning.\n","purpose":"Orient the agent","scope":"repository","lifecycle":"active","read-when":"When resuming work"}},"omitted":{"productContext.md":"No applicable evidence yet","activeContext.md":"No applicable evidence yet","systemPatterns.md":"No applicable evidence yet","techContext.md":"No applicable evidence yet","progress.md":"No applicable evidence yet"}}"##).unwrap();
    let rendered = run(
        world,
        &["bank", "render", "--input", proposal.to_str().unwrap()],
        0,
    );
    let directory = repo.join("memory-bank");
    std::fs::create_dir(&directory).unwrap();
    for (name, body) in get(&rendered, "files").as_object().unwrap() {
        std::fs::write(directory.join(name), body.as_str().unwrap()).unwrap();
    }
    run(
        world,
        &["bank", "validate", "--repo", repo.to_str().unwrap()],
        0,
    );
    directory
}
