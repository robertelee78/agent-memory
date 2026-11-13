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
            dev: value.st_dev as u64,
            ino: value.st_ino as u64,
            mode: value.st_mode as u32,
            uid: value.st_uid,
            gid: value.st_gid,
            nlink: value.st_nlink as u64,
            size: value.st_size,
            mtime_ns: value.st_mtime as i128 * 1_000_000_000 + value.st_mtime_nsec as i128,
            ctime_ns: value.st_ctime as i128 * 1_000_000_000 + value.st_ctime_nsec as i128,
        }
    }
    pub fn is_directory(self) -> bool {
        self.mode & libc::S_IFMT as u32 == libc::S_IFDIR as u32
    }
    pub fn is_regular(self) -> bool {
        self.mode & libc::S_IFMT as u32 == libc::S_IFREG as u32
    }
    pub fn is_symlink(self) -> bool {
        self.mode & libc::S_IFMT as u32 == libc::S_IFLNK as u32
    }
    pub fn same_inode(self, other: Self) -> bool {
        self.dev == other.dev && self.ino == other.ino
    }
    /// Same object with the same access policy. Timestamps, size and link count
    /// change on shared parents such as a sticky temporary directory without
    /// the path being rebound, so trust walks compare this, not full equality.
    pub fn same_binding(self, other: Self) -> bool {
        self.same_inode(other)
            && self.mode == other.mode
            && self.uid == other.uid
            && self.gid == other.gid
    }
}

pub fn signature(info: &Identity) -> Identity {
    *info
}

pub(crate) fn c_path(path: &OsStr) -> io::Result<CString> {
    CString::new(path.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in filesystem path"))
}

pub(crate) fn child_name(name: &OsStr) -> io::Result<CString> {
    if name.is_empty() || name.as_bytes().contains(&b'/') || name == "." || name == ".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "One ordinary path component is required",
        ));
    }
    c_path(name)
}

pub(crate) fn status(result: libc::c_int) -> io::Result<()> {
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn contract_io(error: io::Error) -> ContractError {
    ContractError::new(error.to_string())
}

#[derive(Debug)]
pub enum ReadError {
    Io(io::Error),
    InvalidPath,
    NotRegular,
    MultipleLinks,
    InvalidSize,
    LimitOverflow,
    SourceChanged,
    ParentChanged,
    SizeLimit {
        limit: usize,
        observed: u64,
        exact: bool,
    },
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => std::fmt::Display::fmt(error, f),
            Self::InvalidPath => f.write_str("Read paths must remain relative to their exact root"),
            Self::NotRegular => f.write_str("Source is no longer an ordinary file"),
            Self::MultipleLinks => f.write_str("Source must be single-link"),
            Self::InvalidSize => f.write_str("Source has an invalid size"),
            Self::LimitOverflow => f.write_str("Read limit overflow"),
            Self::SourceChanged => {
                f.write_str("Canonical source changed during bounded observation")
            }
            Self::ParentChanged => {
                f.write_str("Canonical source parent changed during bounded observation")
            }
            Self::SizeLimit { .. } => f.write_str("Ordinary file exceeds its read bound"),
        }
    }
}
impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReadOptions {
    pub require_single_link: bool,
    pub full_parent_signature: bool,
    pub growth_is_capacity: bool,
    pub capacity_before_binding: bool,
}

impl ReadOptions {
    pub const LAYOUT_RECOVERY: Self = Self {
        require_single_link: true,
        full_parent_signature: true,
        growth_is_capacity: true,
        capacity_before_binding: true,
    };
}

#[cfg(target_os = "linux")]
const SEARCH_ACCESS: i32 = libc::O_PATH;
#[cfg(target_os = "macos")]
const SEARCH_ACCESS: i32 = libc::O_SEARCH;
const SEARCH_DIRECTORY: i32 = SEARCH_ACCESS | libc::O_DIRECTORY;

#[derive(Debug)]
pub struct Descriptor(OwnedFd, bool);

impl AsRawFd for Descriptor {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}
impl AsFd for Descriptor {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Descriptor {
    pub(crate) fn open_raw_at(
        parent: RawFd,
        path: &CStr,
        flags: i32,
        mode: u32,
    ) -> io::Result<Self> {
        if flags & libc::O_CREAT != 0 && flags & libc::O_EXCL == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Creation requires O_EXCL",
            ));
        }
        let fd = unsafe {
            libc::openat(
                parent,
                path.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                mode as libc::c_uint,
            )
        };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(
            unsafe { OwnedFd::from_raw_fd(fd) },
            flags & SEARCH_ACCESS == SEARCH_ACCESS,
        ))
    }

    pub fn open_directory(path: &Path) -> io::Result<Self> {
        let chain = DirectoryChain::open(path)?;
        let directory = chain.directory().reopen_directory()?;
        chain.recheck()?;
        Ok(directory)
    }

    pub fn pin_directory(path: &Path) -> io::Result<Self> {
        Ok(DirectoryChain::open(path)?.into_directory())
    }

    pub fn pin_directory_at(&self, name: &OsStr) -> io::Result<Self> {
        self.open_at(name, SEARCH_DIRECTORY, 0)
    }

    pub fn reopen_directory(&self) -> io::Result<Self> {
        let readable = Self::open_raw_at(
            self.as_raw_fd(),
            c".",
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        if !readable.stat()?.same_binding(self.stat()?) {
            return Err(io::Error::other(
                "Directory binding changed while reopening",
            ));
        }
        Ok(readable)
    }

    pub fn is_search_only(&self) -> bool {
        self.1
    }

    pub(crate) fn require_io(&self) -> io::Result<()> {
        if self.is_search_only() {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Search-only directory descriptor cannot perform data I/O",
            ))
        } else {
            Ok(())
        }
    }

    pub fn open_at(&self, name: &OsStr, flags: i32, mode: u32) -> io::Result<Self> {
        Self::open_raw_at(self.as_raw_fd(), &child_name(name)?, flags, mode)
    }

    pub fn open_directory_at(&self, name: &OsStr) -> io::Result<Self> {
        self.open_at(name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
    }

    pub fn create_exclusive(&self, name: &OsStr, flags: i32, mode: u32) -> io::Result<Self> {
        if flags & SEARCH_ACCESS == SEARCH_ACCESS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Exclusive creation requires a data descriptor",
            ));
        }
        let opened = self.open_at(name, flags | libc::O_CREAT | libc::O_EXCL, mode)?;
        status(unsafe { libc::fchmod(opened.as_raw_fd(), mode as libc::mode_t) })?;
        Ok(opened)
    }

    /// The existing inode is never chmodded. ENOENT from the second open is not retried.
    pub fn open_or_create(&self, name: &OsStr, flags: i32, mode: u32) -> io::Result<(Self, bool)> {
        if flags & (libc::O_CREAT | libc::O_EXCL | libc::O_TRUNC) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Explicit non-creating flags are required",
            ));
        }
        match self.create_exclusive(name, flags, mode) {
            Ok(fd) => Ok((fd, true)),
            Err(error) if error.raw_os_error() == Some(libc::EEXIST) => {
                Ok((self.open_at(name, flags, 0)?, false))
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn native_stat(&self) -> io::Result<libc::stat> {
        let mut value = MaybeUninit::uninit();
        status(unsafe { libc::fstat(self.as_raw_fd(), value.as_mut_ptr()) })?;
        Ok(unsafe { value.assume_init() })
    }

    pub fn stat(&self) -> io::Result<Identity> {
        Ok(Identity::from_stat(&self.native_stat()?))
    }

    pub fn stat_at(&self, name: &OsStr) -> io::Result<Identity> {
        let name = child_name(name)?;
        let mut value = MaybeUninit::uninit();
        status(unsafe {
            libc::fstatat(
                self.as_raw_fd(),
                name.as_ptr(),
                value.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;
        Ok(Identity::from_stat(&unsafe { value.assume_init() }))
    }

    pub fn fsync(&self) -> io::Result<()> {
        self.require_io()?;
        loop {
            match status(unsafe { libc::fsync(self.as_raw_fd()) }) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }

    pub fn fsync_directory(&self) -> io::Result<()> {
        if !self.stat()?.is_directory() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Directory fsync requires a directory",
            ));
        }
        if self.is_search_only() {
            self.reopen_directory()?.fsync()
        } else {
            self.fsync()
        }
    }

    pub fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        self.require_io()?;
        while !bytes.is_empty() {
            let written = unsafe {
                libc::write(
                    self.as_raw_fd(),
                    bytes.as_ptr().cast(),
                    bytes.len().min(isize::MAX as usize),
                )
            };
            if written == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if written == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Incomplete descriptor write",
                ));
            }
            bytes = &bytes[written as usize..];
        }
        Ok(())
    }

    /// Exactly one successful write; only EINTR is retried.
    pub fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.require_io()?;
        loop {
            let written = unsafe {
                libc::write(
                    self.as_raw_fd(),
                    bytes.as_ptr().cast(),
                    bytes.len().min(isize::MAX as usize),
                )
            };
            if written >= 0 {
                return Ok(written as usize);
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    /// Read at most maximum bytes from offset zero, without requiring EOF.
    pub fn read_prefix(&mut self, maximum: usize) -> std::result::Result<Vec<u8>, ReadError> {
        self.require_io()?;
        let before = self.stat()?;
        if !before.is_regular() {
            return Err(ReadError::NotRegular);
        }
        let mut bytes = Vec::new();
        let mut block = [0u8; 8192];
        while bytes.len() < maximum {
            let count = block.len().min(maximum - bytes.len());
            let offset = i64::try_from(bytes.len()).map_err(|_| ReadError::LimitOverflow)?;
            let amount = unsafe {
                libc::pread(
                    self.as_raw_fd(),
                    block.as_mut_ptr().cast(),
                    count,
                    offset as libc::off_t,
                )
            };
            if amount == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(ReadError::Io(error));
            }
            if amount == 0 {
                break;
            }
            bytes.extend_from_slice(&block[..amount as usize]);
        }
        if before != self.stat()? {
            return Err(ReadError::SourceChanged);
        }
        Ok(bytes)
    }

    fn read_up_to(&mut self, limit: usize) -> std::result::Result<Vec<u8>, ReadError> {
        self.require_io()?;
        let maximum = limit.checked_add(1).ok_or(ReadError::LimitOverflow)?;
        let mut bytes = Vec::new();
        let mut block = [0u8; 8192];
        while bytes.len() < maximum {
            let count = block.len().min(maximum - bytes.len());
            let read = unsafe { libc::read(self.as_raw_fd(), block.as_mut_ptr().cast(), count) };
            if read == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(ReadError::Io(error));
            }
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&block[..read as usize]);
        }
        Ok(bytes)
    }

    /// Reads from the current offset; a size error measures the consumed range, not the whole file.
    pub fn read_bounded(&mut self, limit: usize) -> std::result::Result<Vec<u8>, ReadError> {
        let before = self.stat()?;
        let bytes = self.read_up_to(limit)?;
        let after = self.stat()?;
        let over_limit = bytes.len() > limit;
        if !read_identity_matches(before, after, over_limit.then_some(bytes.len())) {
            return Err(ReadError::SourceChanged);
        }
        if over_limit {
            return Err(ReadError::SizeLimit {
                limit,
                observed: bytes.len() as u64,
                exact: false,
            });
        }
        Ok(bytes)
    }

    pub fn unlink_at(&self, name: &OsStr) -> io::Result<()> {
        status(unsafe { libc::unlinkat(self.as_raw_fd(), child_name(name)?.as_ptr(), 0) })
    }

    pub fn read_link_at(&self, name: &OsStr) -> io::Result<PathBuf> {
        let name = child_name(name)?;
        let mut buffer = vec![0u8; 4097];
        let count = unsafe {
            libc::readlinkat(
                self.as_raw_fd(),
                name.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if count == -1 {
            return Err(io::Error::last_os_error());
        }
        if count as usize == buffer.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Symlink target exceeds its bound",
            ));
        }
        buffer.truncate(count as usize);
        Ok(PathBuf::from(OsString::from_vec(buffer)))
    }

    pub fn entries(&self) -> io::Result<Vec<OsString>> {
        match self.entries_up_to(None)? {
            Some(names) => Ok(names),
            None => Err(io::Error::other(
                "Unbounded directory listing was truncated",
            )),
        }
    }

    /// Sorted names, or `None` once the directory holds more than `limit` entries.
    pub fn entries_bounded(&self, limit: usize) -> io::Result<Option<Vec<OsString>>> {
        self.entries_up_to(Some(limit))
    }

    fn entries_up_to(&self, limit: Option<usize>) -> io::Result<Option<Vec<OsString>>> {
        let fresh = Self::open_raw_at(
            self.as_raw_fd(),
            c".",
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        let raw = fresh.0.into_raw_fd();
        let directory = unsafe { libc::fdopendir(raw) };
        if directory.is_null() {
            let error = io::Error::last_os_error();
            drop(unsafe { OwnedFd::from_raw_fd(raw) });
            return Err(error);
        }
        struct Stream(*mut libc::DIR);
        impl Drop for Stream {
            fn drop(&mut self) {
                unsafe {
                    libc::closedir(self.0);
                }
            }
        }
        let stream = Stream(directory);
        let mut names = Vec::new();
        loop {
            #[cfg(target_os = "macos")]
            let errno = unsafe { libc::__error() };
            #[cfg(target_os = "linux")]
            let errno = unsafe { libc::__errno_location() };
            unsafe {
                *errno = 0;
            }
            let entry = unsafe { libc::readdir(stream.0) };
            if entry.is_null() {
                if unsafe { *errno } != 0 {
                    return Err(io::Error::last_os_error());
                }
                break;
            }
            let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if bytes != b"." && bytes != b".." {
                names.push(OsString::from_vec(bytes.to_vec()));
                if limit.is_some_and(|limit| names.len() > limit) {
                    return Ok(None);
                }
            }
        }
        names.sort();
        Ok(Some(names))
    }
}

#[derive(Debug)]
pub struct DirectoryChain {
    names: Vec<OsString>,
    descriptors: Vec<Descriptor>,
    observed: Vec<Identity>,
    absolute: bool,
}

impl DirectoryChain {
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::walk(path, false, false)
    }

    pub fn create(path: &Path, sync_created: bool) -> io::Result<Self> {
        Self::walk(path, true, sync_created)
    }

    fn walk(path: &Path, create: bool, sync_created: bool) -> io::Result<Self> {
        let absolute = path.is_absolute();
        if path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Parent traversal is not permitted",
            ));
        }
        let anchor = if absolute { c"/" } else { c"." };
        let root = Descriptor::open_raw_at(libc::AT_FDCWD, anchor, SEARCH_DIRECTORY, 0)?;
        let observed = vec![root.stat()?];
        let mut chain = Self {
            names: Vec::new(),
            descriptors: vec![root],
            observed,
            absolute,
        };
        for part in path.components() {
            let Component::Normal(name) = part else {
                continue;
            };
            let parent = chain.directory();
            let child = match parent.pin_directory_at(name) {
                Ok(child) => child,
                Err(error) if create && error.raw_os_error() == Some(libc::ENOENT) => {
                    let created = match status(unsafe {
                        libc::mkdirat(parent.as_raw_fd(), child_name(name)?.as_ptr(), 0o700)
                    }) {
                        Ok(()) => true,
                        Err(error) if error.raw_os_error() == Some(libc::EEXIST) => false,
                        Err(error) => return Err(error),
                    };
                    let child = parent.pin_directory_at(name)?;
                    if created {
                        let before = child.stat()?;
                        let named = parent.stat_at(name)?;
                        if !before.same_inode(named) || !before.is_directory() {
                            return Err(io::Error::other(
                                "Private state path changed while creating directories",
                            ));
                        }
                        if before.mode & 0o777 != 0o700 {
                            return Err(io::Error::other(
                                "Cannot safely create private state directories; preserve existing paths",
                            ));
                        }
                        if sync_created {
                            parent.fsync_directory()?;
                            child.fsync_directory()?;
                        }
                    }
                    child
                }
                Err(error) => return Err(error),
            };
            chain.observed.push(child.stat()?);
            chain.names.push(name.to_os_string());
            chain.descriptors.push(child);
        }
        chain.recheck()?;
        Ok(chain)
    }

    pub fn directory(&self) -> &Descriptor {
        self.descriptors
            .last()
            .expect("directory chain always includes its anchor")
    }
    pub fn descriptors(&self) -> &[Descriptor] {
        &self.descriptors
    }
    pub fn recheck(&self) -> io::Result<()> {
        for (index, name) in self.names.iter().enumerate() {
            let named = self.descriptors[index].stat_at(name)?;
            let opened = self.descriptors[index + 1].stat()?;
            if !named.is_directory() || !opened.is_directory() || !named.same_inode(opened) {
                return Err(io::Error::other(
                    "Private state path changed while creating directories",
                ));
            }
        }
        Ok(())
    }

    /// Full signatures are compared only at root and below, never on shared ancestors above it.
    pub fn recheck_full_from(&self, root: &Path) -> io::Result<()> {
        if root.is_absolute() != self.absolute
            || root
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Full-signature root is outside the pinned chain",
            ));
        }
        let parts: Vec<_> = root
            .components()
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect();
        if parts.len() > self.names.len()
            || parts
                .iter()
                .zip(&self.names)
                .any(|(left, right)| *left != right.as_os_str())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Full-signature root is outside the pinned chain",
            ));
        }
        self.recheck()?;
        for index in parts.len()..self.descriptors.len() {
            let named = if index == 0 {
                Descriptor::open_raw_at(
                    libc::AT_FDCWD,
                    if self.absolute { c"/" } else { c"." },
                    SEARCH_DIRECTORY,
                    0,
                )?
                .stat()?
            } else {
                self.descriptors[index - 1].stat_at(&self.names[index - 1])?
            };
            if named != self.observed[index]
                || self.descriptors[index].stat()? != self.observed[index]
            {
                return Err(io::Error::other(
                    "Raw source parent changed during observation",
                ));
            }
        }
        Ok(())
    }

    fn into_directory(mut self) -> Descriptor {
        self.descriptors
            .pop()
            .expect("directory chain always includes its anchor")
    }
}

pub fn private_mkdir(path: &Path) -> Result<()> {
    private_mkdir_with_sync(path, false)
}

pub fn private_mkdir_with_sync(path: &Path, sync_created: bool) -> Result<()> {
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err(ContractError::new(
            "Private state directory must be an absolute canonical path",
        ));
    }
    DirectoryChain::create(path, sync_created)
        .map(|_| ())
        .map_err(|error| {
            if error.to_string() == "Private state path changed while creating directories" {
                contract_io(error)
            } else {
                ContractError::new(
                    "Cannot safely create private state directories; preserve existing paths",
                )
            }
        })
}

fn read_identity_matches(
    before: Identity,
    after: Identity,
    growing_measurement: Option<usize>,
) -> bool {
    before == after
        || growing_measurement.is_some_and(|measured| {
            before.same_binding(after)
                && before.nlink == after.nlink
                && after.size > before.size
                && after.size >= 0
                && after.size as u128 >= measured as u128
        })
}

#[derive(Debug)]
pub struct ReadObservation {
    pub bytes: Vec<u8>,
    pub identity: Identity,
}

/// Keeps every ancestor and the named file pinned until its completed read is rechecked.
#[derive(Debug)]
pub struct PinnedRead {
    parents: DirectoryChain,
    root: PathBuf,
    file: Descriptor,
    name: OsString,
    before: Identity,
    limit: usize,
    options: ReadOptions,
}

impl PinnedRead {
    pub fn open(
        directory: &Path,
        relative: &Path,
        limit: usize,
    ) -> std::result::Result<Self, ReadError> {
        Self::open_with_options(directory, relative, limit, ReadOptions::default())
    }

    pub fn open_with_options(
        directory: &Path,
        relative: &Path,
        limit: usize,
        options: ReadOptions,
    ) -> std::result::Result<Self, ReadError> {
        if relative.is_absolute()
            || relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(ReadError::InvalidPath);
        }
        limit.checked_add(1).ok_or(ReadError::LimitOverflow)?;
        let name = relative
            .file_name()
            .ok_or(ReadError::InvalidPath)?
            .to_os_string();
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let parents = DirectoryChain::open(&directory.join(parent))?;
        let file = parents
            .directory()
            .open_at(&name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
        let before = file.stat()?;
        if !before.is_regular() {
            return Err(ReadError::NotRegular);
        }
        if options.require_single_link && before.nlink != 1 {
            return Err(ReadError::MultipleLinks);
        }
        if before.size < 0 {
            return Err(ReadError::InvalidSize);
        }
        Ok(Self {
            parents,
            root: directory.to_path_buf(),
            file,
            name,
            before,
            limit,
            options,
        })
    }

    pub fn identity(&self) -> Identity {
        self.before
    }
    fn verify(&self, growing_measurement: Option<usize>) -> std::result::Result<(), ReadError> {
        let after = self.file.stat()?;
        let named = self
            .parents
            .directory()
            .stat_at(&self.name)
            .map_err(|error| {
                if error.raw_os_error() == Some(libc::ENOENT) {
                    ReadError::SourceChanged
                } else {
                    ReadError::Io(error)
                }
            })?;
        if !read_identity_matches(self.before, after, growing_measurement) || after != named {
            return Err(ReadError::SourceChanged);
        }
        if self.options.full_parent_signature {
            self.parents
                .recheck_full_from(&self.root)
                .map_err(|_| ReadError::ParentChanged)
        } else {
            self.parents.recheck().map_err(|_| ReadError::ParentChanged)
        }
    }

    pub fn recheck(&self) -> std::result::Result<(), ReadError> {
        self.verify(None)
    }

    pub fn read(mut self) -> std::result::Result<ReadObservation, ReadError> {
        if self.before.size as u128 > self.limit as u128 {
            if !self.options.capacity_before_binding {
                self.recheck()?;
            }
            return Err(ReadError::SizeLimit {
                limit: self.limit,
                observed: self.before.size as u64,
                exact: true,
            });
        }
        let bytes = self.file.read_up_to(self.limit)?;
        let over_limit = bytes.len() > self.limit;
        if over_limit {
            if !self.options.growth_is_capacity {
                return Err(ReadError::SourceChanged);
            }
            if !self.options.capacity_before_binding {
                self.verify(Some(bytes.len()))?;
            }
            return Err(ReadError::SizeLimit {
                limit: self.limit,
                observed: bytes.len() as u64,
                exact: false,
            });
        }
        self.verify(None)?;
        if bytes.len() as u128 != self.before.size as u128 {
            return Err(ReadError::SourceChanged);
        }
        Ok(ReadObservation {
            bytes,
            identity: self.before,
        })
    }
}

pub fn read_source(
    directory: &Path,
    relative: &Path,
    limit: usize,
) -> std::result::Result<ReadObservation, ReadError> {
    PinnedRead::open(directory, relative, limit)?.read()
}

pub fn read_source_with_options(
    directory: &Path,
    relative: &Path,
    limit: usize,
    options: ReadOptions,
) -> std::result::Result<ReadObservation, ReadError> {
    PinnedRead::open_with_options(directory, relative, limit, options)?.read()
}

pub fn read_regular(
    directory: &Path,
    relative: &Path,
    limit: usize,
) -> std::result::Result<(Vec<u8>, u32), ReadError> {
    let observation = read_source_with_options(
        directory,
        relative,
        limit,
        ReadOptions {
            require_single_link: true,
            ..ReadOptions::default()
        },
    )?;
    Ok((observation.bytes, observation.identity.mode & 0o777))
}
