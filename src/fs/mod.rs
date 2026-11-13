#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("filesystem contracts require macOS or Linux");

pub mod descriptor;
pub mod inventory;
pub mod lock;
pub mod metadata;
pub mod rename;
pub mod resolve;
pub mod trust;
pub use descriptor::{
    Descriptor, DirectoryChain, Identity, PinnedRead, ReadError, ReadObservation, ReadOptions,
    private_mkdir, private_mkdir_with_sync, read_regular, read_source, read_source_with_options,
    signature,
};
pub use inventory::{Fingerprint, fingerprint};
pub use lock::{FlockGuard, LockError, LockMode, PrivateLock, discard_created_empty_slot};
pub use metadata::{inode_flags, list_xattrs, metadata_supported, read_xattr};
pub use rename::{RenameMode, native_rename, rename_at};
pub use resolve::canonical_path;
