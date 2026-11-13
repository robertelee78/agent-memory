use super::descriptor::{
    Descriptor, DirectoryChain, Identity, contract_io, private_mkdir_with_sync,
};
use super::trust;
use crate::error::{ContractError, Result};
use std::fmt;
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Debug)]
pub enum LockError {
    Busy,
    Io(io::Error),
    Unsafe(ContractError),
}
impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("Source transport busy; preserve pending work and retry"),
            Self::Io(error) => fmt::Display::fmt(error, f),
            Self::Unsafe(error) => fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for LockError {}
impl From<io::Error> for LockError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<ContractError> for LockError {
    fn from(error: ContractError) -> Self {
        Self::Unsafe(error)
    }
}

#[derive(Debug)]
pub struct FlockGuard {
    descriptor: Descriptor,
}
impl FlockGuard {
    pub fn try_acquire(
        descriptor: Descriptor,
        mode: LockMode,
    ) -> std::result::Result<Self, LockError> {
        Self::try_acquire_owned(descriptor, mode).map_err(|(_, error)| error)
    }
    /// Failed acquisition preserves the live exclusive-creation descriptor.
    pub fn try_acquire_owned(
        descriptor: Descriptor,
        mode: LockMode,
    ) -> std::result::Result<Self, (Descriptor, LockError)> {
        if let Err(error) = descriptor.require_io() {
            return Err((descriptor, LockError::Io(error)));
        }
        let operation = match mode {
            LockMode::Shared => libc::LOCK_SH,
            LockMode::Exclusive => libc::LOCK_EX,
        } | libc::LOCK_NB;
        loop {
            if unsafe { libc::flock(descriptor.as_raw_fd(), operation) } == 0 {
                return Ok(Self { descriptor });
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if error.raw_os_error() == Some(libc::EWOULDBLOCK)
                || error.raw_os_error() == Some(libc::EAGAIN)
            {
                return Err((descriptor, LockError::Busy));
            }
            return Err((descriptor, LockError::Io(error)));
        }
    }
    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub fn descriptor_mut(&mut self) -> &mut Descriptor {
        &mut self.descriptor
    }
}
impl Drop for FlockGuard {
    fn drop(&mut self) {
        loop {
            if unsafe { libc::flock(self.descriptor.as_raw_fd(), libc::LOCK_UN) } == 0 {
                break;
            }
            if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                break;
            }
        }
    }
}

#[derive(Debug)]
pub struct PrivateLock {
    guard: FlockGuard,
    parents: DirectoryChain,
    path: PathBuf,
    identity: Identity,
    parent_identity: Identity,
}
impl PrivateLock {
    pub fn acquire(
        path: &Path,
        shared: bool,
        existing_only: bool,
    ) -> std::result::Result<Self, LockError> {
        let parent = path
            .parent()
            .ok_or_else(|| ContractError::new("Private source lock parent changed"))?;
        let name = path
            .file_name()
            .ok_or_else(|| ContractError::new("Unsafe private source lock"))?;
        if !existing_only {
            trust::inspect(parent, trust::InspectOptions::default())?;
            private_mkdir_with_sync(parent, true)?;
        }
        trust::state(parent)?;
        let parents = DirectoryChain::open(parent)?;
        let parent_identity = parents.directory().stat()?;
        let flags = libc::O_RDWR | libc::O_NONBLOCK;
        let file = if existing_only {
            parents
                .directory()
                .open_at(name, flags, 0)
                .map_err(|error| {
                    if error.raw_os_error() == Some(libc::ENOENT) {
                        LockError::Unsafe(ContractError::new(
                            "Private source lock is unavailable; retain pending evidence",
                        ))
                    } else {
                        LockError::Io(error)
                    }
                })?
        } else {
            parents.directory().open_or_create(name, flags, 0o600)?.0
        };
        let identity = file.stat()?;
        if !identity.is_regular()
            || identity.uid != unsafe { libc::geteuid() }
            || identity.nlink != 1
            || identity.mode & 0o077 != 0
        {
            return Err(ContractError::new("Unsafe private source lock").into());
        }
        let guard = FlockGuard::try_acquire(
            file,
            if shared {
                LockMode::Shared
            } else {
                LockMode::Exclusive
            },
        )?;
        let held = Self {
            guard,
            parents,
            path: path.to_path_buf(),
            identity,
            parent_identity,
        };
        held.recheck()?;
        Ok(held)
    }

    pub fn descriptor(&self) -> &Descriptor {
        self.guard.descriptor()
    }
    pub fn recheck(&self) -> Result<()> {
        self.parents.recheck().map_err(|_| {
            ContractError::new("Private source lock parent changed; retain pending evidence")
        })?;
        if !self
            .parents
            .directory()
            .stat()
            .map_err(contract_io)?
            .same_inode(self.parent_identity)
        {
            return Err(ContractError::new(
                "Private source lock parent changed; retain pending evidence",
            ));
        }
        let named = self
            .parents
            .directory()
            .stat_at(self.path.file_name().expect("lock has a filename"))
            .map_err(contract_io)?;
        let opened = self.descriptor().stat().map_err(contract_io)?;
        if !named.same_inode(self.identity)
            || !opened.same_inode(self.identity)
            || !named.is_regular()
        {
            return Err(ContractError::new(
                "Private source lock changed; retain pending evidence",
            ));
        }
        Ok(())
    }

    /// Unlike Drop, explicit completion can report a failed exit recheck.
    pub fn finish(self) -> Result<()> {
        self.recheck()
    }
}

pub fn discard_created_empty_slot(
    path: &Path,
    file: &Descriptor,
    created: Identity,
) -> Result<bool> {
    let parent = path.parent().ok_or_else(|| {
        ContractError::new("Created metadata slot parent changed; preserve the marker")
    })?;
    let name = path.file_name().ok_or_else(|| {
        ContractError::new("Created metadata slot parent changed; preserve the marker")
    })?;
    let parents = DirectoryChain::open(parent).map_err(contract_io)?;
    parents.recheck().map_err(|_| {
        ContractError::new("Created metadata slot parent changed; preserve the marker")
    })?;
    let opened = file.stat().map_err(contract_io)?;
    let named = match parents.directory().stat_at(name) {
        Ok(info) => info,
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => return Ok(false),
        Err(error) => return Err(contract_io(error)),
    };
    if created.size != 0
        || created.nlink != 1
        || created.uid != unsafe { libc::geteuid() }
        || !created.is_regular()
        || created.mode & 0o077 != 0
        || created != opened
        || created != named
    {
        return Ok(false);
    }
    parents.directory().unlink_at(name).map_err(contract_io)?;
    Ok(true)
}
