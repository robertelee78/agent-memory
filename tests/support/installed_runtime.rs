use super::World;
use agent_memory::codec::json;
use std::{
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// Install the real compiled command and shipped pack with the native installer.
pub fn installed_runtime(world: &World) -> (PathBuf, PathBuf) {
    fn copy_assets(source: &Path, target: &Path) {
        fs::create_dir_all(target).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy_assets(&entry.path(), &target.join(entry.file_name()));
            } else if entry.file_type().unwrap().is_file() {
                fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
            }
        }
    }
    let source = world.inside("development-runtime").unwrap();
    copy_assets(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-memory"),
        &source.join("skills/agent-memory"),
    );
    for relative in ["target/release/am", "bin/am"] {
        let binary = source.join(relative);
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_am"), &binary).unwrap();
        fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = world
        .am(vec![
            OsString::from("--json"),
            "install".into(),
            "--development-source".into(),
            source.into_os_string(),
            "--mode".into(),
            "link".into(),
            "--presets".into(),
            "none".into(),
            "--no-guidance".into(),
            "--bin-dir".into(),
            world.inside("bin").unwrap().into_os_string(),
        ])
        .unwrap();
    assert_eq!(output.status, agent_memory::process::Status::Code(0));
    let receipt = json::decode_bytes(
        &fs::read(
            world
                .inside("state/agent-memory/install/receipt.json")
                .unwrap(),
        )
        .unwrap(),
        Default::default(),
    )
    .unwrap();
    let source = receipt.get("source").unwrap();
    let command = agent_memory::installation::source_binary(source).unwrap();
    let pack = Path::new(source.get("pack").unwrap().as_str().unwrap());
    (
        pack.parent().unwrap().parent().unwrap().to_path_buf(),
        command,
    )
}
