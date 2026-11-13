use crate::error::{ContractError, Result};
use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u64,
    pub size: i64,
    pub mtime_ns: i128,
    pub ctime_ns: i128,
}

impl Identity {
    pub(crate) fn from_stat(value: &libc::stat) -> Self {
        Self {
            dev: value.st_dev as u64, ino: value.st_ino as u64,
            mode: value.st_mode as u32, uid: value.st_uid, gid: value.st_gid,
            nlink: value.st_nlink as u64, size: value.st_size,
            mtime_ns: value.st_mtime as i128 * 1_000_000_000 + value.st_mtime_nsec as i128,
            ctime_ns: value.st_ctime as i128 * 1_000_000_000 + value.st_ctime_nsec as i128,
        }
    }
    pub fn is_directory(self) -> bool { self.mode & libc::S_IFMT as u32 == libc::S_IFDIR as u32 }
    pub fn is_regular(self) -> bool { self.mode & libc::S_IFMT as u32 == libc::S_IFREG as u32 }
    pub fn is_symlink(self) -> bool { self.mode & libc::S_IFMT as u32 == libc::S_IFLNK as u32 }
    pub fn same_inode(self, other: Self) -> bool { self.dev == other.dev && self.ino == other.ino }
    /// Same object with the same access policy. Timestamps, size and link count
    /// change on shared parents such as a sticky temporary directory without
    /// the path being rebound, so trust walks compare this, not full equality.
    pub fn same_binding(self, other: Self) -> bool {
        self.same_inode(other) && self.mode == other.mode && self.uid == other.uid && self.gid == other.gid
    }
}

pub fn signature(info: &Identity) -> Identity { *info }

pub(crate) fn c_path(path: &OsStr) -> io::Result<CString> {
    CString::new(path.as_bytes()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in filesystem path"))
}

pub(crate) fn child_name(name: &OsStr) -> io::Result<CString> {
    if name.is_empty() || name.as_bytes().contains(&b'/') || name == "." || name == ".." {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "One ordinary path component is required"));
    }
    c_path(name)
}

pub(crate) fn status(result: libc::c_int) -> io::Result<()> {
    if result == -1 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

pub(crate) fn contract_io(error: io::Error) -> ContractError { ContractError::new(error.to_string()) }

#[derive(Debug)]
pub struct Descriptor(OwnedFd);

impl AsRawFd for Descriptor { fn as_raw_fd(&self) -> RawFd { self.0.as_raw_fd() } }
impl AsFd for Descriptor { fn as_fd(&self) -> BorrowedFd<'_> { self.0.as_fd() } }

impl Descriptor {
    pub(crate) fn open_raw_at(parent: RawFd, path: &CStr, flags: i32, mode: u32) -> io::Result<Self> {
        if flags & libc::O_CREAT != 0 && flags & libc::O_EXCL == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Creation requires O_EXCL"));
        }
        let fd = unsafe { libc::openat(parent, path.as_ptr(), flags | libc::O_NOFOLLOW | libc::O_CLOEXEC, mode as libc::c_uint) };
        if fd == -1 { return Err(io::Error::last_os_error()); }
        Ok(Self(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    pub fn open_directory(path: &Path) -> io::Result<Self> {
        Ok(DirectoryChain::open(path)?.into_directory())
    }

    pub fn open_at(&self, name: &OsStr, flags: i32, mode: u32) -> io::Result<Self> {
        Self::open_raw_at(self.as_raw_fd(), &child_name(name)?, flags, mode)
    }

    pub fn open_directory_at(&self, name: &OsStr) -> io::Result<Self> {
        self.open_at(name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
    }

    pub fn create_exclusive(&self, name: &OsStr, flags: i32, mode: u32) -> io::Result<Self> {
        let opened = self.open_at(name, flags | libc::O_CREAT | libc::O_EXCL, mode)?;
        status(unsafe { libc::fchmod(opened.as_raw_fd(), mode as libc::mode_t) })?;
        Ok(opened)
    }

    /// The existing inode is never chmodded. ENOENT from the second open is not retried.
    pub fn open_or_create(&self, name: &OsStr, flags: i32, mode: u32) -> io::Result<(Self, bool)> {
        if flags & (libc::O_CREAT | libc::O_EXCL | libc::O_TRUNC) != 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Explicit non-creating flags are required"));
        }
        match self.create_exclusive(name, flags, mode) {
            Ok(fd) => Ok((fd, true)),
            Err(error) if error.raw_os_error() == Some(libc::EEXIST) => Ok((self.open_at(name, flags, 0)?, false)),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn native_stat(&self) -> io::Result<libc::stat> {
        let mut value = MaybeUninit::uninit();
        status(unsafe { libc::fstat(self.as_raw_fd(), value.as_mut_ptr()) })?;
        Ok(unsafe { value.assume_init() })
    }

    pub fn stat(&self) -> io::Result<Identity> { Ok(Identity::from_stat(&self.native_stat()?)) }

    pub fn stat_at(&self, name: &OsStr) -> io::Result<Identity> {
        let name = child_name(name)?;
        let mut value = MaybeUninit::uninit();
        status(unsafe { libc::fstatat(self.as_raw_fd(), name.as_ptr(), value.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW) })?;
        Ok(Identity::from_stat(&unsafe { value.assume_init() }))
    }

    pub fn fsync(&self) -> io::Result<()> {
        loop {
            match status(unsafe { libc::fsync(self.as_raw_fd()) }) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }

    pub fn fsync_directory(&self) -> io::Result<()> {
        if !self.stat()?.is_directory() { return Err(io::Error::new(io::ErrorKind::InvalidInput, "Directory fsync requires a directory")); }
        self.fsync()
    }

    pub fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let written = unsafe { libc::write(self.as_raw_fd(), bytes.as_ptr().cast(), bytes.len().min(isize::MAX as usize)) };
            if written == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted { continue; }
                return Err(error);
            }
            if written == 0 { return Err(io::Error::new(io::ErrorKind::WriteZero, "Incomplete descriptor write")); }
            bytes = &bytes[written as usize..];
        }
        Ok(())
    }

    /// Reads from the current descriptor offset, consuming at most limit + 1 bytes.
    pub fn read_bounded(&mut self, limit: usize) -> io::Result<Vec<u8>> {
        let maximum = limit.checked_add(1).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Read limit overflow"))?;
        let mut bytes = Vec::new();
        let mut block = [0u8; 8192];
        while bytes.len() < maximum {
            let count = block.len().min(maximum - bytes.len());
            let read = unsafe { libc::read(self.as_raw_fd(), block.as_mut_ptr().cast(), count) };
            if read == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted { continue; }
                return Err(error);
            }
            if read == 0 { break; }
            bytes.extend_from_slice(&block[..read as usize]);
        }
        if bytes.len() > limit { return Err(io::Error::new(io::ErrorKind::InvalidData, "Ordinary file exceeds its read bound")); }
        Ok(bytes)
    }

    pub fn unlink_at(&self, name: &OsStr) -> io::Result<()> {
        status(unsafe { libc::unlinkat(self.as_raw_fd(), child_name(name)?.as_ptr(), 0) })
    }

    pub fn read_link_at(&self, name: &OsStr) -> io::Result<PathBuf> {
        let name = child_name(name)?;
        let mut buffer = vec![0u8; 4097];
        let count = unsafe { libc::readlinkat(self.as_raw_fd(), name.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if count == -1 { return Err(io::Error::last_os_error()); }
        if count as usize == buffer.len() { return Err(io::Error::new(io::ErrorKind::InvalidData, "Symlink target exceeds its bound")); }
        buffer.truncate(count as usize);
        Ok(PathBuf::from(OsString::from_vec(buffer)))
    }

    pub fn entries(&self) -> io::Result<Vec<OsString>> {
        let fresh = Self::open_raw_at(self.as_raw_fd(), c".", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        let raw = fresh.0.into_raw_fd();
        let directory = unsafe { libc::fdopendir(raw) };
        if directory.is_null() {
            let error = io::Error::last_os_error();
            drop(unsafe { OwnedFd::from_raw_fd(raw) });
            return Err(error);
        }
        struct Stream(*mut libc::DIR);
        impl Drop for Stream { fn drop(&mut self) { unsafe { libc::closedir(self.0); } } }
        let stream = Stream(directory);
        let mut names = Vec::new();
        loop {
            #[cfg(target_os = "macos")]
            let errno = unsafe { libc::__error() };
            #[cfg(target_os = "linux")]
            let errno = unsafe { libc::__errno_location() };
            unsafe { *errno = 0; }
            let entry = unsafe { libc::readdir(stream.0) };
            if entry.is_null() {
                if unsafe { *errno } != 0 { return Err(io::Error::last_os_error()); }
                break;
            }
            let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if bytes != b"." && bytes != b".." { names.push(OsString::from_vec(bytes.to_vec())); }
        }
        names.sort();
        Ok(names)
    }
}

#[derive(Debug)]
pub struct DirectoryChain {
    names: Vec<OsString>,
    descriptors: Vec<Descriptor>,
}

impl DirectoryChain {
    pub fn open(path: &Path) -> io::Result<Self> { Self::walk(path, false) }

    fn walk(path: &Path, create: bool) -> io::Result<Self> {
        let absolute = path.is_absolute();
        if path.components().any(|part| matches!(part, Component::ParentDir | Component::Prefix(_))) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Parent traversal is not permitted"));
        }
        let anchor = if absolute { c"/" } else { c"." };
        let root = Descriptor::open_raw_at(libc::AT_FDCWD, anchor, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        let mut chain = Self { names: Vec::new(), descriptors: vec![root] };
        for part in path.components() {
            let Component::Normal(name) = part else { continue; };
            let parent = chain.directory();
            let child = match parent.open_directory_at(name) {
                Ok(child) => child,
                Err(error) if create && error.raw_os_error() == Some(libc::ENOENT) => {
                    let created = match status(unsafe { libc::mkdirat(parent.as_raw_fd(), child_name(name)?.as_ptr(), 0o700) }) {
                        Ok(()) => true,
                        Err(error) if error.raw_os_error() == Some(libc::EEXIST) => false,
                        Err(error) => return Err(error),
                    };
                    let child = parent.open_directory_at(name)?;
                    if created {
                        let before = child.stat()?;
                        let named = parent.stat_at(name)?;
                        if !before.same_inode(named) || !before.is_directory() {
                            return Err(io::Error::other("Private state path changed while creating directories"));
                        }
                        if before.mode & 0o7777 != 0o700 {
                            return Err(io::Error::other("Cannot safely create private state directories; preserve existing paths"));
                        }
                    }
                    child
                }
                Err(error) => return Err(error),
            };
            chain.names.push(name.to_os_string());
            chain.descriptors.push(child);
        }
        chain.recheck()?;
        Ok(chain)
    }

    pub fn directory(&self) -> &Descriptor { self.descriptors.last().expect("directory chain always includes its anchor") }
    pub fn descriptors(&self) -> &[Descriptor] { &self.descriptors }
    pub fn recheck(&self) -> io::Result<()> {
        for (index, name) in self.names.iter().enumerate() {
            let named = self.descriptors[index].stat_at(name)?;
            let opened = self.descriptors[index + 1].stat()?;
            if !named.is_directory() || !opened.is_directory() || !named.same_inode(opened) {
                return Err(io::Error::other("Private state path changed while creating directories"));
            }
        }
        Ok(())
    }
    fn into_directory(mut self) -> Descriptor { self.descriptors.pop().expect("directory chain always includes its anchor") }
}

pub fn private_mkdir(path: &Path) -> Result<()> {
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err(ContractError::new("Private state directory must be an absolute canonical path"));
    }
    DirectoryChain::walk(path, true).map(|_| ()).map_err(|error| {
        if error.to_string() == "Private state path changed while creating directories" { contract_io(error) }
        else { ContractError::new("Cannot safely create private state directories; preserve existing paths") }
    })
}

#[derive(Debug)]
pub struct ReadObservation { pub bytes: Vec<u8>, pub identity: Identity }

/// Keeps every ancestor and the named file pinned until its completed read is rechecked.
#[derive(Debug)]
pub struct PinnedRead {
    parents: DirectoryChain,
    file: Descriptor,
    name: OsString,
    before: Identity,
    limit: usize,
}

impl PinnedRead {
    pub fn open(directory: &Path, relative: &Path, limit: usize) -> Result<Self> {
        if relative.is_absolute() || relative.as_os_str().is_empty() || relative.components().any(|part| !matches!(part, Component::Normal(_))) {
            return Err(ContractError::new("Migration paths must remain relative to their exact root"));
        }
        let name = relative.file_name().ok_or_else(|| ContractError::new("Migration paths must remain relative to their exact root"))?.to_os_string();
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let parents = DirectoryChain::open(&directory.join(parent)).map_err(contract_io)?;
        let file = parents.directory().open_at(&name, libc::O_RDONLY | libc::O_NONBLOCK, 0).map_err(contract_io)?;
        let before = file.stat().map_err(contract_io)?;
        if !before.is_regular() || before.nlink != 1 || before.size < 0 || before.size as u128 > limit as u128 {
            return Err(ContractError::new("Migration reads bounded ordinary single-link files only"));
        }
        Ok(Self { parents, file, name, before, limit })
    }

    pub fn identity(&self) -> Identity { self.before }
    pub fn recheck(&self) -> Result<()> {
        let after = self.file.stat().map_err(contract_io)?;
        let named = self.parents.directory().stat_at(&self.name).map_err(contract_io)?;
        if self.before != after || after != named {
            return Err(ContractError::new("Canonical source changed during bounded observation"));
        }
        self.parents.recheck().map_err(|_| ContractError::new("Canonical source parent changed during bounded observation"))
    }

    pub fn read(mut self) -> Result<ReadObservation> {
        let bytes = self.file.read_bounded(self.limit).map_err(contract_io)?;
        if bytes.len() as u128 != self.before.size as u128 {
            return Err(ContractError::new("Migration source changed during its bounded read"));
        }
        self.recheck()?;
        Ok(ReadObservation { bytes, identity: self.before })
    }
}

pub fn read_source(directory: &Path, relative: &Path, limit: usize) -> Result<ReadObservation> {
    PinnedRead::open(directory, relative, limit)?.read()
}

pub fn read_regular(directory: &Path, relative: &Path, limit: usize) -> Result<(Vec<u8>, u32)> {
    let observation = read_source(directory, relative, limit)?;
    Ok((observation.bytes, observation.identity.mode & 0o777))
}
