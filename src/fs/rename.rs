use super::descriptor::{Descriptor, DirectoryChain, child_name, status};
use std::ffi::OsStr;
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameMode { NoReplace, Exchange }

/// One native atomic namespace operation; no emulation on unsupported filesystems.
pub fn rename_at(source_directory: &Descriptor, source: &OsStr, target_directory: &Descriptor, target: &OsStr, mode: RenameMode) -> io::Result<()> {
    let source = child_name(source)?;
    let target = child_name(target)?;
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(source_directory.as_raw_fd(), source.as_ptr(), target_directory.as_raw_fd(), target.as_ptr(),
            match mode { RenameMode::NoReplace => libc::RENAME_EXCL, RenameMode::Exchange => libc::RENAME_SWAP })
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        // Invoke the kernel directly so old glibc and musl do not need a renameat2 wrapper.
        libc::syscall(libc::SYS_renameat2, source_directory.as_raw_fd(), source.as_ptr(), target_directory.as_raw_fd(), target.as_ptr(),
            match mode { RenameMode::NoReplace => 1u32, RenameMode::Exchange => 2u32 }) as libc::c_int
    };
    status(result)
}

/// Pins both parents, performs the atomic switch, then syncs both containing directories.
pub fn native_rename(source: &Path, target: &Path, mode: RenameMode) -> io::Result<()> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "Rename requires two named filesystem entries");
    let left = DirectoryChain::open(source.parent().ok_or_else(invalid)?)?;
    let right = DirectoryChain::open(target.parent().ok_or_else(invalid)?)?;
    left.recheck()?;
    right.recheck()?;
    rename_at(left.directory(), source.file_name().ok_or_else(invalid)?, right.directory(), target.file_name().ok_or_else(invalid)?, mode)?;
    right.directory().fsync_directory()?;
    if !left.directory().stat()?.same_inode(right.directory().stat()?) { left.directory().fsync_directory()?; }
    left.recheck()?;
    right.recheck()
}
