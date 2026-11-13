use agent_memory::error::Kind;
use agent_memory::fs::trust::{self, InspectOptions};
use agent_memory::fs::{
    self as amfs, Descriptor, DirectoryChain, Fingerprint, FlockGuard, LockError, LockMode,
    PinnedRead, PrivateLock, RenameMode,
};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, DirBuilder, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct PrivateDirectory(PathBuf);
impl PrivateDirectory {
    fn new() -> Self {
        loop {
            let path = Path::new("/tmp").join(format!(
                "am-t0-fs-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Self(path.canonicalize().unwrap()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("private directory: {error}"),
            }
        }
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn join(&self, path: &str) -> PathBuf {
        self.0.join(path)
    }
}
impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn mode(path: &Path, value: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(value)).unwrap();
}
fn file(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
}
fn reason(error: &agent_memory::error::ContractError) -> &str {
    match error.kind() {
        Kind::TrustPath { reason, .. } => reason,
        _ => panic!("not a trust-path refusal: {error}"),
    }
}

#[test]
fn writable_ancestor_refused_before_private_child() {
    let root = PrivateDirectory::new();
    mode(root.path(), 0o755);
    let exposed = root.join("exposed");
    fs::create_dir(&exposed).unwrap();
    mode(&exposed, 0o775);
    let private = exposed.join("private");
    fs::create_dir(&private).unwrap();
    mode(&private, 0o700);
    let error = trust::state(&private).unwrap_err();
    assert_eq!(
        reason(&error),
        "group/other write access permits an untrusted principal to change it"
    );
    assert_eq!(fs::metadata(&exposed).unwrap().mode() & 0o777, 0o775);
    mode(root.path(), 0o700);
}

#[test]
fn private_barrier_protects_descendants_without_chmod() {
    let root = PrivateDirectory::new();
    let shared = root.join("shared");
    fs::create_dir(&shared).unwrap();
    mode(&shared, 0o777);
    let child = shared.join("record");
    file(&child, b"preserve");
    let observed = trust::inspect(&child, InspectOptions::default()).unwrap();
    assert_eq!(observed.path, child);
    assert!(observed.present && observed.unix_path_policy_verified);
    assert!(
        !observed.acl_audited
            && !observed.executing_verifier_authenticated
            && !observed.writes_performed
    );
    assert_eq!(fs::metadata(&shared).unwrap().mode() & 0o777, 0o777);
    let absent = trust::inspect(&shared.join("missing/leaf"), InspectOptions::default()).unwrap();
    assert!(!absent.present);
}

#[test]
fn symlink_components_refused_and_only_explicit_final_link_allowed() {
    let root = PrivateDirectory::new();
    fs::create_dir(root.join("real")).unwrap();
    file(&root.join("real/file"), b"untouched");
    symlink("real", root.join("alias")).unwrap();
    assert_eq!(
        reason(&trust::inspect(&root.join("alias/file"), InspectOptions::default()).unwrap_err()),
        "a trusted ordinary path must not be redirected by a symlink"
    );
    assert!(
        trust::inspect(
            &root.join("alias/file"),
            InspectOptions {
                allow_final_link: true,
                ..InspectOptions::default()
            }
        )
        .is_err()
    );
    assert!(trust::inspect(&root.join("alias"), InspectOptions::default()).is_err());
    assert!(
        trust::inspect(
            &root.join("alias"),
            InspectOptions {
                allow_final_link: true,
                ..InspectOptions::default()
            }
        )
        .unwrap()
        .present
    );
    assert!(trust::state(&root.join("alias")).is_err());
    assert!(trust::record(&root.join("alias")).is_err());
    assert!(Descriptor::open_directory(&root.join("alias")).is_err());
    assert_eq!(fs::read(root.join("real/file")).unwrap(), b"untouched");
    trust::tree(root.path(), false).unwrap();
    symlink("real", root.join(".git")).unwrap();
    assert!(trust::tree(root.path(), false).is_err());
}

#[test]
fn records_refuse_aliases_and_nonregular_nodes() {
    let root = PrivateDirectory::new();
    let path = root.join("record");
    file(&path, b"evidence");
    trust::record(&path).unwrap();
    fs::hard_link(&path, root.join("alias")).unwrap();
    assert_eq!(
        reason(&trust::record(&path).unwrap_err()),
        "lifecycle records must be current-user-owned, regular, single-link and not group/other writable"
    );
    assert_eq!(fs::read(&path).unwrap(), b"evidence");
    let socket = root.join("socket");
    let _socket = UnixListener::bind(&socket).unwrap();
    assert_eq!(
        reason(&trust::record(&socket).unwrap_err()),
        "an ordinary directory or regular file is required"
    );
    assert!(trust::record(root.path()).is_err());
    mode(&path, 0o666);
    assert_eq!(
        reason(&trust::inspect(&path, InspectOptions::default()).unwrap_err()),
        "a writable payload has aliases outside its checked path"
    );
}

#[test]
fn private_state_refuses_nonprivate_modes_without_repair() {
    let root = PrivateDirectory::new();
    let state = root.join("state");
    fs::create_dir(&state).unwrap();
    for permissions in [0o755, 0o770, 0o777] {
        mode(&state, permissions);
        assert_eq!(
            reason(&trust::state(&state).unwrap_err()),
            "installation state must be a private current-user-owned directory (0700)"
        );
        assert_eq!(fs::metadata(&state).unwrap().mode() & 0o777, permissions);
    }
    mode(&state, 0o700);
    assert!(trust::state(&state).unwrap().present);
    assert!(trust::inspect(Path::new("relative"), InspectOptions::default()).is_err());
    assert!(trust::inspect(&state.join("../state"), InspectOptions::default()).is_err());
}

#[test]
fn mkdir_preserves_existing_modes_and_refuses_aliases() {
    let root = PrivateDirectory::new();
    let existing = root.join("existing");
    fs::create_dir(&existing).unwrap();
    mode(&existing, 0o775);
    amfs::private_mkdir(&existing.join("one/two")).unwrap();
    assert_eq!(fs::metadata(&existing).unwrap().mode() & 0o777, 0o775);
    assert_eq!(
        fs::metadata(existing.join("one")).unwrap().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(existing.join("one/two")).unwrap().mode() & 0o777,
        0o700
    );
    symlink("one", existing.join("alias")).unwrap();
    assert!(amfs::private_mkdir(&existing.join("alias/uncreated")).is_err());
    assert!(!existing.join("one/uncreated").exists());
}

#[test]
fn private_mkdir_umask_child() {
    let Some(root) = std::env::var_os("AM_T0_FS_UMASK_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mut file = Descriptor::open_directory(&root)
        .unwrap()
        .create_exclusive(OsStr::new("exact-file"), libc::O_WRONLY, 0o600)
        .unwrap();
    file.write_all(b"exact").unwrap();
    assert_eq!(file.stat().unwrap().mode & 0o777, 0o600);
    let result = amfs::private_mkdir(&root.join("new/nested"));
    if std::env::var_os("AM_T0_FS_EXPECT_REFUSAL").is_some() {
        assert!(result.is_err());
        assert_eq!(
            fs::symlink_metadata(root.join("new")).unwrap().mode() & 0o777,
            0
        );
        assert!(!root.join("new/nested").exists());
    } else {
        result.unwrap();
        assert_eq!(
            fs::metadata(root.join("new")).unwrap().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(root.join("new/nested")).unwrap().mode() & 0o777,
            0o700
        );
    }
}

#[test]
fn private_creation_obeys_exact_modes_without_global_umask_changes() {
    for mask in ["022", "077", "777"] {
        let root = PrivateDirectory::new();
        let mut child = Command::new("/bin/sh");
        child.args([
            "-c",
            "umask \"$1\"; exec \"$2\" --exact private_mkdir_umask_child --nocapture",
            "am-umask",
            mask,
        ]);
        child
            .arg(std::env::current_exe().unwrap())
            .env("AM_T0_FS_UMASK_ROOT", root.path());
        if mask == "777" {
            child.env("AM_T0_FS_EXPECT_REFUSAL", "1");
        }
        let output = child.output().unwrap();
        // Restore only our test-owned evidence so the fixture can remove it.
        if root.join("new").exists() {
            mode(&root.join("new"), 0o700);
        }
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn exclusive_create_refuses_existing_entries_and_writes_all_bytes() {
    let root = PrivateDirectory::new();
    let directory = Descriptor::open_directory(root.path()).unwrap();
    let mut descriptor = directory
        .create_exclusive(OsStr::new("file"), libc::O_WRONLY, 0o600)
        .unwrap();
    let bytes = vec![b'!'; 100_000];
    descriptor.write_all(&bytes).unwrap();
    descriptor.fsync().unwrap();
    directory.fsync_directory().unwrap();
    assert_eq!(fs::read(root.join("file")).unwrap(), bytes);
    assert!(
        directory
            .create_exclusive(OsStr::new("file"), libc::O_WRONLY, 0o600)
            .is_err()
    );
    symlink("file", root.join("link")).unwrap();
    assert!(
        directory
            .create_exclusive(OsStr::new("link"), libc::O_WRONLY, 0o600)
            .is_err()
    );
    assert!(
        directory
            .open_or_create(OsStr::new("link"), libc::O_RDWR | libc::O_NONBLOCK, 0o600)
            .is_err()
    );
    assert!(
        directory
            .open_at(OsStr::new("../outside"), libc::O_RDONLY, 0)
            .is_err()
    );
    assert!(
        directory
            .open_at(OsStr::new("unsafe"), libc::O_CREAT | libc::O_RDWR, 0o600)
            .is_err()
    );
    assert!(!root.join("unsafe").exists());
}

#[test]
fn atomic_exchange_swaps_directories_and_noreplace_preserves_collision() {
    let root = PrivateDirectory::new();
    fs::create_dir(root.join("left")).unwrap();
    fs::create_dir(root.join("right")).unwrap();
    file(&root.join("left/value"), b"left");
    file(&root.join("right/value"), b"right");
    let left = fs::metadata(root.join("left")).unwrap().ino();
    let right = fs::metadata(root.join("right")).unwrap().ino();
    let error = amfs::native_rename(
        &root.join("left"),
        &root.join("right"),
        RenameMode::NoReplace,
    )
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EEXIST));
    assert_eq!(fs::read(root.join("left/value")).unwrap(), b"left");
    amfs::native_rename(
        &root.join("left"),
        &root.join("right"),
        RenameMode::Exchange,
    )
    .unwrap();
    assert_eq!(fs::metadata(root.join("left")).unwrap().ino(), right);
    assert_eq!(fs::metadata(root.join("right")).unwrap().ino(), left);
    assert_eq!(fs::read(root.join("left/value")).unwrap(), b"right");
    assert_eq!(fs::read(root.join("right/value")).unwrap(), b"left");
    amfs::native_rename(
        &root.join("left"),
        &root.join("moved"),
        RenameMode::NoReplace,
    )
    .unwrap();
    assert!(!root.join("left").exists());
    assert_eq!(fs::read(root.join("moved/value")).unwrap(), b"right");
}

#[test]
fn shared_flocks_coexist_and_exclusive_contention_is_distinct() {
    let root = PrivateDirectory::new();
    file(&root.join("lock"), b"");
    let directory = Descriptor::open_directory(root.path()).unwrap();
    let open = || {
        directory
            .open_at(OsStr::new("lock"), libc::O_RDWR | libc::O_NONBLOCK, 0)
            .unwrap()
    };
    let first = FlockGuard::try_acquire(open(), LockMode::Shared).unwrap();
    let second = FlockGuard::try_acquire(open(), LockMode::Shared).unwrap();
    assert!(matches!(
        FlockGuard::try_acquire(open(), LockMode::Exclusive),
        Err(LockError::Busy)
    ));
    drop(first);
    drop(second);
    let exclusive = FlockGuard::try_acquire(open(), LockMode::Exclusive).unwrap();
    assert!(matches!(
        FlockGuard::try_acquire(open(), LockMode::Shared),
        Err(LockError::Busy)
    ));
    drop(exclusive);
    FlockGuard::try_acquire(open(), LockMode::Exclusive).unwrap();
}

#[test]
fn private_lock_rechecks_named_inode_and_parent() {
    let root = PrivateDirectory::new();
    let parent = root.join("parent");
    amfs::private_mkdir(&parent).unwrap();
    let path = parent.join("lock");
    assert!(PrivateLock::acquire(&path, false, true).is_err());
    assert!(!path.exists());
    let held = PrivateLock::acquire(&path, false, false).unwrap();
    held.recheck().unwrap();
    assert!(matches!(
        PrivateLock::acquire(&path, false, true),
        Err(LockError::Busy)
    ));
    fs::rename(&path, parent.join("old")).unwrap();
    file(&path, b"");
    assert!(
        held.recheck()
            .unwrap_err()
            .to_string()
            .contains("lock changed")
    );
    drop(held);
    let held = PrivateLock::acquire(&path, true, true).unwrap();
    fs::rename(&parent, root.join("old-parent")).unwrap();
    fs::create_dir(&parent).unwrap();
    mode(&parent, 0o700);
    fs::hard_link(root.join("old-parent/lock"), &path).unwrap();
    assert!(
        held.recheck()
            .unwrap_err()
            .to_string()
            .contains("parent changed")
    );
}

#[test]
fn unsafe_private_lock_is_preserved() {
    let root = PrivateDirectory::new();
    let path = root.join("lock");
    file(&path, b"evidence");
    mode(&path, 0o644);
    assert!(PrivateLock::acquire(&path, false, true).is_err());
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
    assert_eq!(fs::read(&path).unwrap(), b"evidence");
    mode(&path, 0o600);
    fs::hard_link(&path, root.join("alias")).unwrap();
    assert!(PrivateLock::acquire(&path, false, false).is_err());
}

#[test]
fn empty_created_slot_cleanup_requires_exact_identity() {
    let root = PrivateDirectory::new();
    let directory = Descriptor::open_directory(root.path()).unwrap();
    let mut descriptor = directory
        .create_exclusive(OsStr::new("lock"), libc::O_RDWR, 0o600)
        .unwrap();
    let created = descriptor.stat().unwrap();
    descriptor.write_all(b"published").unwrap();
    assert!(!amfs::discard_created_empty_slot(&root.join("lock"), &descriptor, created).unwrap());
    assert_eq!(fs::read(root.join("lock")).unwrap(), b"published");
    let descriptor = directory
        .create_exclusive(OsStr::new("empty"), libc::O_RDWR, 0o600)
        .unwrap();
    let created = descriptor.stat().unwrap();
    assert!(amfs::discard_created_empty_slot(&root.join("empty"), &descriptor, created).unwrap());
    assert!(!root.join("empty").exists());
    let descriptor = directory
        .create_exclusive(OsStr::new("rebound"), libc::O_RDWR, 0o600)
        .unwrap();
    let created = descriptor.stat().unwrap();
    fs::rename(root.join("rebound"), root.join("old-slot")).unwrap();
    file(&root.join("rebound"), b"");
    assert!(
        !amfs::discard_created_empty_slot(&root.join("rebound"), &descriptor, created).unwrap()
    );
    assert!(root.join("rebound").exists());
}

#[test]
fn pinned_regular_read_enforces_bounds_type_links_and_modes() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("nested")).unwrap();
    file(&root.join("nested/source"), b"exact bytes\n");
    mode(&root.join("nested/source"), 0o640);
    let (bytes, permissions) =
        amfs::read_regular(root.path(), Path::new("nested/source"), 12).unwrap();
    assert_eq!(bytes, b"exact bytes\n");
    assert_eq!(permissions, 0o640);
    assert!(amfs::read_regular(root.path(), Path::new("nested/source"), 11).is_err());
    symlink("source", root.join("nested/link")).unwrap();
    assert!(amfs::read_regular(root.path(), Path::new("nested/link"), 100).is_err());
    fs::hard_link(root.join("nested/source"), root.join("alias")).unwrap();
    assert!(amfs::read_regular(root.path(), Path::new("nested/source"), 100).is_err());
    assert!(amfs::read_regular(root.path(), Path::new("nested"), 100).is_err());
    let _socket = UnixListener::bind(root.join("socket")).unwrap();
    assert!(amfs::read_regular(root.path(), Path::new("socket"), 100).is_err());
    for path in ["../outside", "/absolute", ""] {
        assert!(amfs::read_regular(root.path(), Path::new(path), 100).is_err());
    }
}

#[test]
fn pinned_read_detects_file_replacement_and_parent_redirection() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("nested")).unwrap();
    file(&root.join("nested/file"), b"old");
    let pinned = PinnedRead::open(root.path(), Path::new("nested/file"), 100).unwrap();
    fs::rename(root.join("nested/file"), root.join("nested/old")).unwrap();
    file(&root.join("nested/file"), b"new");
    assert!(pinned.read().is_err());
    let pinned = PinnedRead::open(root.path(), Path::new("nested/file"), 100).unwrap();
    fs::rename(root.join("nested"), root.join("retained")).unwrap();
    symlink("retained", root.join("nested")).unwrap();
    assert!(pinned.read().is_err());
    assert_eq!(fs::read(root.join("retained/file")).unwrap(), b"new");
    assert!(PinnedRead::open(root.path(), Path::new("nested/file"), 100).is_err());
}

#[test]
fn directory_pins_detect_ancestor_rebinding() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("parent/child")).unwrap();
    let chain = DirectoryChain::open(&root.join("parent/child")).unwrap();
    fs::rename(root.join("parent"), root.join("old-parent")).unwrap();
    amfs::private_mkdir(&root.join("parent/child")).unwrap();
    assert!(chain.recheck().is_err());
}

#[test]
fn git_locators_check_lexical_path_before_following_metadata() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("source")).unwrap();
    amfs::private_mkdir(&root.join("metadata")).unwrap();
    file(&root.join("source/.git"), b"gitdir: ../metadata\r\n");
    trust::source(&root.join("source")).unwrap();
    assert_eq!(
        trust::git_pointer(&root.join("source"), "../metadata").unwrap(),
        root.join("metadata")
    );
    symlink("metadata", root.join("alias")).unwrap();
    assert!(trust::git_pointer(root.path(), "alias").is_err());
    assert!(trust::git_pointer(root.path(), "alias/../metadata").is_err());
    fs::write(root.join("metadata/commondir"), "../alias\n").unwrap();
    assert!(trust::source(&root.join("source")).is_err());
    fs::remove_file(root.join("metadata/commondir")).unwrap();
    for invalid in ["", "a\nb", "a\u{7f}b"] {
        assert!(trust::git_pointer(root.path(), invalid).is_err());
    }
    fs::write(root.join("source/.git"), vec![b'x'; 4097]).unwrap();
    assert_eq!(
        reason(&trust::source(&root.join("source")).unwrap_err()),
        "Git directory pointer exceeds its bound"
    );
}

#[test]
fn metadata_observation_is_descriptor_bound_and_refuses_links() {
    let root = PrivateDirectory::new();
    file(&root.join("file"), b"bytes");
    let directory = Descriptor::open_directory(root.path()).unwrap();
    let descriptor = directory
        .open_at(OsStr::new("file"), libc::O_RDONLY, 0)
        .unwrap();
    let names = amfs::list_xattrs(&descriptor).unwrap();
    let info = descriptor.stat().unwrap();
    let ordinary = fs::metadata(root.join("file")).unwrap();
    assert_eq!(info.dev, ordinary.dev());
    assert_eq!(info.ino, ordinary.ino());
    assert_eq!(info.size, 5);
    assert_eq!(info.uid, ordinary.uid());
    assert_eq!(info.gid, ordinary.gid());
    assert_eq!(
        info.mtime_ns,
        ordinary.mtime() as i128 * 1_000_000_000 + ordinary.mtime_nsec() as i128
    );
    assert_eq!(
        info.ctime_ns,
        ordinary.ctime() as i128 * 1_000_000_000 + ordinary.ctime_nsec() as i128
    );
    if names.is_empty() && amfs::inode_flags(&descriptor).is_ok() {
        amfs::metadata_supported(&root.join("file")).unwrap();
    }
    symlink("file", root.join("link")).unwrap();
    assert_eq!(
        amfs::metadata_supported(&root.join("link"))
            .unwrap_err()
            .to_string(),
        "pending-metadata-preservation"
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
#[test]
fn managed_fingerprints_match_canonical_inventory_bytes() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("managed")).unwrap();
    assert_eq!(amfs::fingerprint(&root.join("absent")).unwrap(), None);
    let empty = amfs::fingerprint(&root.join("managed")).unwrap().unwrap();
    assert_eq!(
        empty,
        Fingerprint::Directory {
            hash: hex(&Sha256::digest(b"{}\n")),
            mode: 0o700
        }
    );
    file(&root.join("managed/f"), b"abc");
    mode(&root.join("managed/f"), 0o640);
    amfs::private_mkdir(&root.join("managed/nested")).unwrap();
    symlink("f", root.join("managed/soft")).unwrap();
    assert_eq!(
        amfs::fingerprint(&root.join("managed/f")).unwrap().unwrap(),
        Fingerprint::File {
            hash: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
            mode: 0o640
        }
    );
    let expected = b"{\n  \"f\": {\n    \"hash\": \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\",\n    \"kind\": \"file\",\n    \"mode\": 416\n  },\n  \"nested\": {\n    \"kind\": \"directory\",\n    \"mode\": 448\n  },\n  \"soft\": {\n    \"kind\": \"link\",\n    \"target\": \"f\"\n  }\n}\n";
    assert_eq!(
        amfs::fingerprint(&root.join("managed")).unwrap().unwrap(),
        Fingerprint::Directory {
            hash: hex(&Sha256::digest(expected)),
            mode: 0o700
        }
    );
    assert_eq!(
        amfs::fingerprint(&root.join("managed/soft"))
            .unwrap()
            .unwrap(),
        Fingerprint::Link { target: "f".into() }
    );
}

#[test]
fn bounded_descriptor_read_and_fifo_refusal_are_nonblocking() {
    use std::os::unix::ffi::OsStrExt;
    let root = PrivateDirectory::new();
    file(&root.join("file"), b"abcdef");
    let directory = Descriptor::open_directory(root.path()).unwrap();
    let mut descriptor = directory
        .open_at(OsStr::new("file"), libc::O_RDONLY, 0)
        .unwrap();
    assert!(matches!(
        descriptor.read_bounded(5),
        Err(amfs::ReadError::SizeLimit {
            limit: 5,
            observed: 6,
            exact: false
        })
    ));
    let mut descriptor = directory
        .open_at(OsStr::new("file"), libc::O_RDONLY, 0)
        .unwrap();
    assert_eq!(descriptor.read_bounded(6).unwrap(), b"abcdef");
    let fifo = std::ffi::CString::new(root.join("fifo").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert!(amfs::read_regular(root.path(), Path::new("fifo"), 100).is_err());
    assert!(trust::record(&root.join("fifo")).is_err());
}

#[test]
fn descriptor_xattrs_are_exact_and_unsupported_attributes_refused() {
    use std::os::fd::AsRawFd;
    let root = PrivateDirectory::new();
    file(&root.join("file"), b"preserve");
    let directory = Descriptor::open_directory(root.path()).unwrap();
    let descriptor = directory
        .open_at(OsStr::new("file"), libc::O_RDONLY, 0)
        .unwrap();
    let name = c"user.am_t0_fs";
    let bytes = b"\0exact\xffattribute";
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::fsetxattr(
            descriptor.as_raw_fd(),
            name.as_ptr(),
            bytes.as_ptr().cast(),
            bytes.len(),
            0,
            0,
        )
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::fsetxattr(
            descriptor.as_raw_fd(),
            name.as_ptr(),
            bytes.as_ptr().cast(),
            bytes.len(),
            0,
        )
    };
    assert_eq!(result, 0, "{}", std::io::Error::last_os_error());
    assert!(
        amfs::list_xattrs(&descriptor)
            .unwrap()
            .iter()
            .any(|item| item == name.to_bytes())
    );
    assert_eq!(
        amfs::read_xattr(&descriptor, name.to_bytes()).unwrap(),
        bytes
    );
    assert_eq!(
        amfs::metadata_supported(&root.join("file"))
            .unwrap_err()
            .to_string(),
        "pending-metadata-preservation"
    );
    assert_eq!(fs::read(root.join("file")).unwrap(), b"preserve");
}

#[cfg(target_os = "macos")]
#[test]
fn nonempty_darwin_acl_is_refused_without_repair() {
    let root = PrivateDirectory::new();
    file(&root.join("file"), b"preserve");
    let output = Command::new("/bin/chmod")
        .args(["+a", "everyone allow read"])
        .arg(root.join("file"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        amfs::metadata_supported(&root.join("file"))
            .unwrap_err()
            .to_string(),
        "pending-metadata-preservation"
    );
    assert_eq!(fs::read(root.join("file")).unwrap(), b"preserve");
}

#[test]
fn size_refusals_are_typed_and_follow_binding_checks() {
    use amfs::ReadError;
    let root = PrivateDirectory::new();
    file(&root.join("source"), b"oversize");
    assert!(matches!(
        amfs::read_source(root.path(), Path::new("source"), 4),
        Err(ReadError::SizeLimit {
            limit: 4,
            observed: 8,
            exact: true
        })
    ));
    let pinned = PinnedRead::open(root.path(), Path::new("source"), 4).unwrap();
    fs::rename(root.join("source"), root.join("old-source")).unwrap();
    file(&root.join("source"), b"replaced");
    assert!(matches!(pinned.read(), Err(ReadError::SourceChanged)));
    fs::write(root.join("source"), b"abc").unwrap();
    let pinned = PinnedRead::open(root.path(), Path::new("source"), 4).unwrap();
    fs::write(root.join("source"), b"abcdefghi").unwrap();
    assert!(matches!(pinned.read(), Err(ReadError::SourceChanged)));
    amfs::private_mkdir(&root.join("parent")).unwrap();
    file(&root.join("parent/file"), b"oversize");
    let pinned = PinnedRead::open(root.path(), Path::new("parent/file"), 4).unwrap();
    fs::rename(root.join("parent"), root.join("old-parent")).unwrap();
    amfs::private_mkdir(&root.join("parent")).unwrap();
    assert!(matches!(pinned.read(), Err(ReadError::ParentChanged)));
}

#[test]
fn single_link_policy_is_selected_by_the_consumer() {
    use amfs::{ReadError, ReadOptions};
    let root = PrivateDirectory::new();
    file(&root.join("source"), b"shared");
    fs::hard_link(root.join("source"), root.join("alias")).unwrap();
    assert_eq!(
        amfs::read_source(root.path(), Path::new("source"), 100)
            .unwrap()
            .bytes,
        b"shared"
    );
    assert!(matches!(
        amfs::read_regular(root.path(), Path::new("source"), 100),
        Err(ReadError::MultipleLinks)
    ));
    assert!(matches!(
        amfs::read_source_with_options(
            root.path(),
            Path::new("source"),
            100,
            ReadOptions::LAYOUT_RECOVERY
        ),
        Err(ReadError::MultipleLinks)
    ));
}

#[test]
fn full_parent_signatures_stop_at_the_requested_root() {
    use amfs::{ReadError, ReadOptions};
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("ancestor/bank/nested")).unwrap();
    file(&root.join("ancestor/bank/nested/file"), b"exact");
    let bank = root.join("ancestor/bank");
    let options = ReadOptions::LAYOUT_RECOVERY;
    let pinned =
        PinnedRead::open_with_options(&bank, Path::new("nested/file"), 100, options).unwrap();
    fs::File::open(root.join("ancestor"))
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(2))
        .unwrap();
    assert_eq!(pinned.read().unwrap().bytes, b"exact");
    let pinned =
        PinnedRead::open_with_options(&bank, Path::new("nested/file"), 100, options).unwrap();
    fs::File::open(&bank)
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(3))
        .unwrap();
    assert!(matches!(pinned.read(), Err(ReadError::ParentChanged)));
    let pinned =
        PinnedRead::open_with_options(&bank, Path::new("nested/file"), 100, options).unwrap();
    fs::File::open(bank.join("nested"))
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(4))
        .unwrap();
    assert!(matches!(pinned.read(), Err(ReadError::ParentChanged)));
    let pinned = PinnedRead::open(&bank, Path::new("nested/file"), 100).unwrap();
    fs::File::open(bank.join("nested"))
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(pinned.read().unwrap().bytes, b"exact");
}

#[test]
fn tree_walk_tolerates_shared_parent_mtime_changes_and_vanished_entries() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("shared/bank/vanished-directory")).unwrap();
    mode(&root.join("shared"), 0o755);
    let bank = root.join("shared/bank");
    file(&bank.join("vanished-file"), b"old");
    file(&bank.join("index"), b"old");
    let observed = trust::TreeInspection::open(&bank, true).unwrap();
    fs::remove_file(bank.join("vanished-file")).unwrap();
    fs::remove_dir(bank.join("vanished-directory")).unwrap();
    file(&bank.join("new-index"), b"new");
    fs::rename(bank.join("new-index"), bank.join("index")).unwrap();
    fs::File::open(root.join("shared"))
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(2))
        .unwrap();
    fs::File::open(&bank)
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(3))
        .unwrap();
    observed.finish().unwrap();
    assert_eq!(fs::read(bank.join("index")).unwrap(), b"new");
}

#[test]
fn tree_walk_refuses_an_ancestor_rebound_between_observation_phases() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("parent/bank")).unwrap();
    file(&root.join("parent/bank/file"), b"old");
    let observed = trust::TreeInspection::open(&root.join("parent/bank"), false).unwrap();
    fs::rename(root.join("parent"), root.join("retained-parent")).unwrap();
    amfs::private_mkdir(&root.join("parent/bank")).unwrap();
    file(&root.join("parent/bank/file"), b"new");
    assert!(observed.finish().is_err());
    assert_eq!(
        fs::read(root.join("retained-parent/bank/file")).unwrap(),
        b"old"
    );
    assert_eq!(fs::read(root.join("parent/bank/file")).unwrap(), b"new");
}

#[test]
fn bare_repository_symlink_probes_trigger_strict_metadata_walk() {
    for redirected in ["HEAD", "objects"] {
        let root = PrivateDirectory::new();
        amfs::private_mkdir(&root.join("repository")).unwrap();
        amfs::private_mkdir(&root.join("objects")).unwrap();
        file(&root.join("HEAD"), b"ref: refs/heads/main\n");
        let repo = root.join("repository");
        if redirected == "HEAD" {
            symlink("../HEAD", repo.join("HEAD")).unwrap();
            amfs::private_mkdir(&repo.join("objects")).unwrap();
        } else {
            file(&repo.join("HEAD"), b"ref: refs/heads/main\n");
            symlink("../objects", repo.join("objects")).unwrap();
        }
        trust::tree(&repo, false).unwrap();
        assert_eq!(
            reason(&trust::source(&repo).unwrap_err()),
            "a trusted ordinary path must not be redirected by a symlink"
        );
    }
}

#[test]
fn search_only_parent_pins_do_not_require_directory_read_permission() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("execute/nested")).unwrap();
    file(&root.join("execute/nested/file"), b"exact");
    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            mode(&self.0, 0o700);
        }
    }
    let _restore = Restore(root.join("execute"));
    mode(&root.join("execute"), 0o111);
    let mut pinned = Descriptor::pin_directory(&root.join("execute")).unwrap();
    assert!(pinned.is_search_only());
    assert!(pinned.stat().unwrap().is_directory());
    assert!(pinned.stat_at(OsStr::new("nested")).unwrap().is_directory());
    assert_eq!(
        amfs::read_regular(root.path(), Path::new("execute/nested/file"), 100)
            .unwrap()
            .0,
        b"exact"
    );
    trust::inspect(&root.join("execute/nested/file"), InspectOptions::default()).unwrap();
    assert!(
        !Descriptor::open_directory(&root.join("execute/nested"))
            .unwrap()
            .is_search_only()
    );
    assert_eq!(
        pinned.fsync().unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert!(
        matches!(pinned.read_bounded(10), Err(amfs::ReadError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(
        pinned.write_all(b"x").unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert!(matches!(FlockGuard::try_acquire(pinned, LockMode::Shared),
        Err(LockError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput));
}

#[cfg(target_os = "linux")]
#[test]
fn private_mkdir_accepts_inherited_setgid_without_chmod() {
    let root = PrivateDirectory::new();
    let parent = root.join("setgid");
    fs::create_dir(&parent).unwrap();
    mode(&parent, 0o2700);
    amfs::private_mkdir(&parent.join("new/nested")).unwrap();
    assert_eq!(fs::metadata(&parent).unwrap().mode() & 0o7777, 0o2700);
    assert_eq!(
        fs::metadata(parent.join("new")).unwrap().mode() & 0o7777,
        0o2700
    );
    assert_eq!(
        fs::metadata(parent.join("new/nested")).unwrap().mode() & 0o7777,
        0o2700
    );
}

#[test]
fn sync_created_walk_produces_usable_private_lock_directories() {
    let root = PrivateDirectory::new();
    let chain = DirectoryChain::create(&root.join("parent/child"), true).unwrap();
    chain.recheck().unwrap();
    assert!(chain.descriptors().iter().all(Descriptor::is_search_only));
    chain.directory().fsync_directory().unwrap();
    let held = PrivateLock::acquire(&root.join("other/nested/lock"), false, false).unwrap();
    held.finish().unwrap();
    assert_eq!(
        fs::metadata(root.join("other/nested")).unwrap().mode() & 0o777,
        0o700
    );
}

#[test]
fn managed_inventory_uses_ascii_pretty_json_and_file_bound_precedence() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("managed")).unwrap();
    symlink("é/😀", root.join("managed/café")).unwrap();
    let expected = b"{\n  \"caf\\u00e9\": {\n    \"kind\": \"link\",\n    \"target\": \"\\u00e9/\\ud83d\\ude00\"\n  }\n}\n";
    assert_eq!(
        amfs::fingerprint(&root.join("managed")).unwrap().unwrap(),
        Fingerprint::Directory {
            hash: hex(&Sha256::digest(expected)),
            mode: 0o700
        }
    );
    fs::File::create(root.join("managed/oversized"))
        .unwrap()
        .set_len(513 * 1024 * 1024)
        .unwrap();
    assert!(
        amfs::fingerprint(&root.join("managed"))
            .unwrap_err()
            .to_string()
            .starts_with("Oversized managed file: ")
    );
}

#[test]
fn read_failure_kinds_do_not_require_message_parsing() {
    use amfs::ReadError;
    let root = PrivateDirectory::new();
    file(&root.join("source"), b"x");
    assert!(matches!(
        amfs::read_source(root.path(), Path::new("../source"), 10),
        Err(ReadError::InvalidPath)
    ));
    assert!(matches!(
        amfs::read_source(root.path(), Path::new("source"), usize::MAX),
        Err(ReadError::LimitOverflow)
    ));
    assert!(matches!(
        amfs::read_source(root.path(), Path::new("missing"), 10),
        Err(ReadError::Io(_))
    ));
    amfs::private_mkdir(&root.join("directory")).unwrap();
    assert!(matches!(
        amfs::read_source(root.path(), Path::new("directory"), 10),
        Err(ReadError::NotRegular)
    ));
}

#[test]
fn tree_finish_inspects_entries_added_after_open() {
    let root = PrivateDirectory::new();
    amfs::private_mkdir(&root.join("repository")).unwrap();
    let repo = root.join("repository");
    let observed = trust::TreeInspection::open(&repo, false).unwrap();
    symlink("../outside-metadata", repo.join(".git")).unwrap();
    assert_eq!(
        reason(&observed.finish().unwrap_err()),
        "a trusted ordinary path must not be redirected by a symlink"
    );
    assert!(
        fs::symlink_metadata(repo.join(".git"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn growth_capacity_is_selected_per_consumer() {
    use amfs::{ReadError, ReadOptions};
    for growth_is_capacity in [false, true] {
        let root = PrivateDirectory::new();
        file(&root.join("source"), b"abc");
        let options = ReadOptions {
            growth_is_capacity,
            ..ReadOptions::default()
        };
        let pinned =
            PinnedRead::open_with_options(root.path(), Path::new("source"), 4, options).unwrap();
        fs::write(root.join("source"), b"abcdefghi").unwrap();
        let result = pinned.read();
        if growth_is_capacity {
            assert!(matches!(
                result,
                Err(ReadError::SizeLimit {
                    limit: 4,
                    observed: 5,
                    exact: false
                })
            ));
        } else {
            assert!(matches!(result, Err(ReadError::SourceChanged)));
        }
    }
}

#[test]
fn layout_capacity_precedes_binding_checks_but_bank_capacity_does_not() {
    use amfs::{ReadError, ReadOptions};
    for capacity_before_binding in [false, true] {
        let root = PrivateDirectory::new();
        file(&root.join("source"), b"oversize");
        let options = ReadOptions {
            capacity_before_binding,
            ..ReadOptions::LAYOUT_RECOVERY
        };
        let pinned =
            PinnedRead::open_with_options(root.path(), Path::new("source"), 4, options).unwrap();
        fs::rename(root.join("source"), root.join("retained")).unwrap();
        file(&root.join("source"), b"replacement");
        let result = pinned.read();
        if capacity_before_binding {
            assert!(matches!(
                result,
                Err(ReadError::SizeLimit {
                    limit: 4,
                    observed: 8,
                    exact: true
                })
            ));
        } else {
            assert!(matches!(result, Err(ReadError::SourceChanged)));
        }
    }
    let root = PrivateDirectory::new();
    file(&root.join("source"), b"abc");
    let pinned = PinnedRead::open_with_options(
        root.path(),
        Path::new("source"),
        4,
        ReadOptions::LAYOUT_RECOVERY,
    )
    .unwrap();
    fs::write(root.join("source"), b"abcdefghi").unwrap();
    fs::rename(root.join("source"), root.join("retained")).unwrap();
    file(&root.join("source"), b"replacement");
    assert!(matches!(
        pinned.read(),
        Err(ReadError::SizeLimit {
            limit: 4,
            observed: 5,
            exact: false
        })
    ));
}

#[test]
fn canonical_names_resolve_links_before_parent_components() {
    let root = PrivateDirectory::new();
    fs::create_dir_all(root.join("actual/nested")).unwrap();
    symlink(root.join("actual/nested"), root.join("link")).unwrap();
    assert_eq!(
        amfs::canonical_path(&root.join("link"), true).unwrap(),
        root.join("actual/nested")
    );
    assert_eq!(
        amfs::canonical_path(&root.join("link/.."), true).unwrap(),
        root.join("actual")
    );
    assert!(amfs::canonical_path(&root.join("missing/tail"), true).is_err());
    assert_eq!(
        amfs::canonical_path(&root.join("missing/tail"), false).unwrap(),
        root.join("missing/tail")
    );
    symlink("loop", root.join("loop")).unwrap();
    assert!(amfs::canonical_path(&root.join("loop"), true).is_err());
    assert!(amfs::canonical_path(&root.join("loop"), false).is_err());
    // A self-amplifying link re-queues its whole target on every expansion;
    // the symlink and pending-component bounds refuse it quickly.
    let amplifying = vec!["amp"; 250].join("/");
    symlink(&amplifying, root.join("amp")).unwrap();
    let started = std::time::Instant::now();
    assert!(amfs::canonical_path(&root.join("amp"), true).is_err());
    assert!(amfs::canonical_path(&root.join("amp"), false).is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    // Exactly MAX_SYMLINKS links resolve; one more is refused (declared delta).
    fs::create_dir(root.join("end")).unwrap();
    symlink(root.join("end"), root.join("chain-0")).unwrap();
    for index in 1..=amfs::resolve::MAX_SYMLINKS {
        symlink(
            root.join(&format!("chain-{}", index - 1)),
            root.join(&format!("chain-{index}")),
        )
        .unwrap();
    }
    let last = amfs::resolve::MAX_SYMLINKS - 1;
    assert_eq!(
        amfs::canonical_path(&root.join(&format!("chain-{last}")), true).unwrap(),
        root.join("end")
    );
    let beyond = amfs::resolve::MAX_SYMLINKS;
    assert!(amfs::canonical_path(&root.join(&format!("chain-{beyond}")), true).is_err());
}

#[test]
fn replace_rename_never_exposes_missing_target_to_a_reader() {
    let root = PrivateDirectory::new();
    file(&root.join("target"), b"old");
    let target = root.join("target");
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let flag = running.clone();
    let ready = barrier.clone();
    let reader = std::thread::spawn(move || {
        let mut reads = 0u64;
        ready.wait();
        while flag.load(Ordering::Acquire) || reads == 0 {
            let bytes = fs::read(&target).expect("target is never missing during replace");
            assert!(bytes == b"old" || bytes == b"new");
            reads += 1;
        }
        reads
    });
    let directory = Descriptor::open_directory(root.path()).unwrap();
    barrier.wait();
    for round in 0..2000 {
        let body: &[u8] = if round % 2 == 0 { b"new" } else { b"old" };
        file(&root.join("source"), body);
        amfs::rename_at(
            &directory,
            OsStr::new("source"),
            &directory,
            OsStr::new("target"),
            RenameMode::Replace,
        )
        .unwrap();
    }
    running.store(false, Ordering::Release);
    assert!(reader.join().unwrap() > 0);
    assert!(!root.join("source").exists());
    assert_eq!(fs::read(root.join("target")).unwrap(), b"old");
}

#[test]
fn flock_failure_returns_the_live_descriptor_and_mutable_io_is_locked() {
    let root = PrivateDirectory::new();
    let directory = Descriptor::open_directory(root.path()).unwrap();
    let file = directory
        .create_exclusive(OsStr::new("lock"), libc::O_RDWR, 0o600)
        .unwrap();
    let identity = file.stat().unwrap();
    let mut owner = FlockGuard::try_acquire(file, LockMode::Exclusive).unwrap();
    assert_eq!(owner.write_once(b"payload").unwrap(), 7);
    let other = directory
        .open_at(OsStr::new("lock"), libc::O_RDONLY, 0)
        .unwrap();
    let (mut other, error) = FlockGuard::try_acquire_owned(other, LockMode::Shared).unwrap_err();
    assert!(matches!(error, LockError::Busy));
    assert!(other.stat().unwrap().same_inode(identity));
    assert_eq!(other.read_prefix(7).unwrap(), b"payload");
    assert_eq!(owner.write_once(b"").unwrap(), 0);
    drop(owner);
    let _owner = FlockGuard::try_acquire(other, LockMode::Exclusive).unwrap();
}

#[test]
fn prefix_reads_bound_bytes_not_source_size_and_refuse_nonregular_files() {
    let root = PrivateDirectory::new();
    file(&root.join("large"), &vec![b'x'; 1024 * 1024]);
    file(&root.join("short"), b"short");
    let directory = Descriptor::open_directory(root.path()).unwrap();
    let mut large = directory
        .open_at(OsStr::new("large"), libc::O_RDONLY, 0)
        .unwrap();
    assert_eq!(large.read_prefix(16385).unwrap(), vec![b'x'; 16385]);
    let mut short = directory
        .open_at(OsStr::new("short"), libc::O_RDONLY, 0)
        .unwrap();
    assert_eq!(short.read_prefix(16385).unwrap(), b"short");
    let mut dir = Descriptor::open_directory(root.path()).unwrap();
    assert!(matches!(
        dir.read_prefix(1),
        Err(amfs::ReadError::NotRegular)
    ));
    assert!(
        Command::new("mkfifo")
            .arg(root.join("fifo"))
            .status()
            .unwrap()
            .success()
    );
    let mut fifo = directory
        .open_at(OsStr::new("fifo"), libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .unwrap();
    assert!(matches!(
        fifo.read_prefix(1),
        Err(amfs::ReadError::NotRegular)
    ));
}
