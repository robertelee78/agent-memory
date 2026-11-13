#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("filesystem contracts require macOS or Linux");

pub mod descriptor;
pub mod lock;
pub mod metadata;
pub mod rename;
pub mod trust;
pub mod inventory;
pub use descriptor::{Descriptor, DirectoryChain, Identity, PinnedRead, ReadObservation, private_mkdir, read_regular, read_source, signature};
pub use lock::{FlockGuard, LockError, LockMode, PrivateLock, discard_created_empty_slot};
pub use metadata::{inode_flags, list_xattrs, metadata_supported, read_xattr};
pub use rename::{RenameMode, native_rename, rename_at};
pub use inventory::{Fingerprint, fingerprint};
