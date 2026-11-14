//! Linux parent-image observation: the exact supported relation between this
//! helper's `CLOCK_MONOTONIC` and the parent's JavaScript hrtime.
//!
//! The parent executable must hash to one of the qualified upstream images,
//! both processes must share a time namespace (or both lack one), and the
//! parent must stay the same live process with the same unchanged image.
//! The baseline CPython/version gate is replaced by a self-check of the Rust
//! clock identity. Everything else, including the allowlist, is unchanged.
use super::clock_window::{NativeImage, Platform};
use super::{LINUX_MONOTONIC_IMAGES, MAX_NATIVE_IMAGE_BYTES};
use sha2::{Digest, Sha256};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

const MAX_PROCESS_STAT: usize = 8192;
const IMAGE_BLOCK: u64 = 1024 * 1024;
/// The only local clock whose relation to the qualified images is known.
pub const QUALIFIED_CLOCK: &str = "linux:clock_gettime(CLOCK_MONOTONIC)";
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;

/// `(st_mode, st_size, st_dev, st_ino, st_mtime_ns, st_ctime_ns)` of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageStat {
    pub mode: u32,
    pub size: u64,
    pub dev: u64,
    pub ino: u64,
    pub mtime_ns: i128,
    pub ctime_ns: i128,
}
impl ImageStat {
    fn regular(&self) -> bool {
        self.mode & S_IFMT == S_IFREG
    }
    fn identity(&self) -> (u64, u64, u64, i128, i128) {
        (self.dev, self.ino, self.size, self.mtime_ns, self.ctime_ns)
    }
}

/// A process's time namespace: its `ns/time` inode, or none at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeNamespace {
    Time(u64, u64),
    Absent,
}

/// Classify from `stat` of `ns/time`, `ns/time_for_children`, `ns/mnt`.
/// A kernel with children-only time namespaces but no `ns/time` is ambiguous.
pub fn time_namespace_with(
    stat: impl Fn(&str) -> io::Result<(u64, u64)>,
) -> io::Result<TimeNamespace> {
    match stat("time") {
        Ok((dev, ino)) => Ok(TimeNamespace::Time(dev, ino)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => match stat("time_for_children") {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                stat("mnt")?;
                Ok(TimeNamespace::Absent)
            }
            Err(error) => Err(error),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Ambiguous time namespace",
            )),
        },
        Err(error) => Err(error),
    }
}

/// Field 22 (`starttime`) of `/proc/<pid>/stat`, after the last `)`.
pub fn process_start(raw: &[u8]) -> io::Result<u64> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid process identity");
    if raw.len() > MAX_PROCESS_STAT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unbounded process identity",
        ));
    }
    let end = raw.iter().rposition(|b| *b == b')').ok_or_else(invalid)?;
    let rest = raw.get(end + 2..).ok_or_else(invalid)?;
    let text = std::str::from_utf8(rest).map_err(|_| invalid())?;
    text.split_ascii_whitespace()
        .nth(19)
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())
}

/// Operating-system access of the observation. The production probe is the
/// Linux `/proc` + pidfd implementation; contracts drive the same logic with
/// controlled values, which qualify arithmetic only, never provenance.
pub trait ImageProbe: Send + Sync + 'static {
    type Handle: Send;
    fn clock_identity(&self) -> &str;
    fn allowed(&self, digest: &str) -> bool;
    fn parent_pid(&self) -> i32;
    fn own_pid(&self) -> i32;
    fn pidfd_open(&self, pid: i32) -> io::Result<Self::Handle>;
    fn pidfd_exited(&self, pidfd: &Self::Handle) -> io::Result<bool>;
    fn process_start(&self, pid: i32) -> io::Result<u64>;
    fn time_namespace(&self, pid: i32) -> io::Result<TimeNamespace>;
    fn open_image(&self, pid: i32) -> io::Result<Self::Handle>;
    fn fstat(&self, handle: &Self::Handle) -> io::Result<ImageStat>;
    fn stat_image(&self, pid: i32) -> io::Result<ImageStat>;
    fn read(&self, handle: &mut Self::Handle, count: usize) -> io::Result<Vec<u8>>;
    fn close(&self, handle: Self::Handle);
}

struct Ready<H> {
    image: H,
    pidfd: H,
    parent: i32,
    start: u64,
    namespaces: (TimeNamespace, TimeNamespace),
    identity: (u64, u64, u64, i128, i128),
}
struct ImageState<H> {
    ready: Option<Ready<H>>,
    closed: bool,
}

/// One helper's background parent-image observation; never caller authority.
pub struct LinuxNativeImage<P: ImageProbe> {
    probe: Arc<P>,
    state: Arc<Mutex<ImageState<P::Handle>>>,
}
impl<P: ImageProbe> LinuxNativeImage<P> {
    /// Start the observation in its own thread.
    pub fn start(probe: Arc<P>) -> Arc<Self> {
        let image = Arc::new(Self::unobserved(probe));
        let worker = Arc::clone(&image);
        let spawned = std::thread::Builder::new()
            .name("omp-stream-image".into())
            .spawn(move || worker.observe());
        if spawned.is_err() {
            image.close();
        }
        image
    }
    /// An observation that has not run yet (`observe` runs it inline).
    pub fn unobserved(probe: Arc<P>) -> Self {
        Self {
            probe,
            state: Arc::new(Mutex::new(ImageState {
                ready: None,
                closed: false,
            })),
        }
    }
    fn lock(&self) -> MutexGuard<'_, ImageState<P::Handle>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn closed(&self) -> bool {
        self.lock().closed
    }
    /// Observe the parent image once; any failure leaves it unobserved.
    pub fn observe(&self) {
        let probe = &*self.probe;
        if probe.clock_identity() != QUALIFIED_CLOCK {
            return;
        }
        let parent = probe.parent_pid();
        let Ok(pidfd) = probe.pidfd_open(parent) else {
            return;
        };
        let mut handles = Handles {
            probe,
            pidfd: Some(pidfd),
            image: None,
        };
        let Some(ready) = self.measure(&mut handles, parent) else {
            return;
        };
        let mut state = self.lock();
        if !state.closed {
            state.ready = Some(ready);
        } else {
            drop(state);
            probe.close(ready.image);
            probe.close(ready.pidfd);
        }
    }
    fn measure(&self, handles: &mut Handles<'_, P>, parent: i32) -> Option<Ready<P::Handle>> {
        let probe = handles.probe;
        let start = probe.process_start(parent).ok()?;
        let namespaces = (
            probe.time_namespace(probe.own_pid()).ok()?,
            probe.time_namespace(parent).ok()?,
        );
        if (namespaces.0 == TimeNamespace::Absent) != (namespaces.1 == TimeNamespace::Absent) {
            return None;
        }
        handles.image = Some(probe.open_image(parent).ok()?);
        let image = probe.fstat(handles.image.as_ref()?).ok()?;
        if !image.regular() || image.size == 0 || image.size > MAX_NATIVE_IMAGE_BYTES {
            return None;
        }
        let mut digest = Sha256::new();
        let mut total = 0u64;
        while total < image.size {
            if self.lock().closed {
                return None;
            }
            let count = usize::try_from(IMAGE_BLOCK.min(image.size - total)).ok()?;
            let block = probe.read(handles.image.as_mut()?, count).ok()?;
            if block.is_empty() {
                return None;
            }
            total += block.len() as u64;
            digest.update(&block);
        }
        let hex = crate::validation_memo::hex(&digest.finalize());
        let extra = probe.read(handles.image.as_mut()?, 1).ok()?;
        if !extra.is_empty() || !probe.allowed(&hex) {
            return None;
        }
        let identity = image.identity();
        let current = probe.fstat(handles.image.as_ref()?).ok()?;
        if identity != current.identity() {
            return None;
        }
        Some(Ready {
            image: handles.image.take()?,
            pidfd: handles.pidfd.take()?,
            parent,
            start,
            namespaces,
            identity,
        })
    }
}
/// Descriptors not yet published are closed on every exit.
struct Handles<'a, P: ImageProbe> {
    probe: &'a P,
    pidfd: Option<P::Handle>,
    image: Option<P::Handle>,
}
impl<P: ImageProbe> Drop for Handles<'_, P> {
    fn drop(&mut self) {
        if let Some(image) = self.image.take() {
            self.probe.close(image);
        }
        if let Some(pidfd) = self.pidfd.take() {
            self.probe.close(pidfd);
        }
    }
}
impl<P: ImageProbe> NativeImage for LinuxNativeImage<P> {
    fn observed(&self) -> bool {
        let state = self.lock();
        !state.closed && state.ready.is_some()
    }
    fn valid(&self) -> bool {
        let state = self.lock();
        let Some(ready) = state.ready.as_ref().filter(|_| !state.closed) else {
            return false;
        };
        let probe = &*self.probe;
        let check = || -> io::Result<bool> {
            if probe.pidfd_exited(&ready.pidfd)? {
                return Ok(false);
            }
            if probe.parent_pid() != ready.parent
                || probe.process_start(ready.parent)? != ready.start
            {
                return Ok(false);
            }
            let namespaces = (
                probe.time_namespace(probe.own_pid())?,
                probe.time_namespace(ready.parent)?,
            );
            if namespaces != ready.namespaces {
                return Ok(false);
            }
            let held = probe.fstat(&ready.image)?;
            let fresh = probe.stat_image(ready.parent)?;
            Ok(fresh.regular()
                && ready.identity == held.identity()
                && ready.identity == fresh.identity())
        };
        check().unwrap_or(false)
    }
    fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        if let Some(ready) = state.ready.take() {
            drop(state);
            self.probe.close(ready.image);
            self.probe.close(ready.pidfd);
        }
    }
}

#[cfg(target_os = "linux")]
mod system {
    use super::{ImageProbe, ImageStat, TimeNamespace};
    use crate::process::parent;
    use std::io;
    use std::os::fd::{AsFd, OwnedFd};
    use std::path::Path;

    pub enum Handle {
        Pidfd(OwnedFd),
        Image(std::fs::File),
    }
    fn image_stat(value: parent::Stat) -> ImageStat {
        ImageStat {
            mode: value.mode,
            size: value.size,
            dev: value.dev,
            ino: value.ino,
            mtime_ns: value.mtime_ns,
            ctime_ns: value.ctime_ns,
        }
    }
    fn wrong() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, "Unexpected observation handle")
    }
    pub struct Proc;
    impl ImageProbe for Proc {
        type Handle = Handle;
        fn clock_identity(&self) -> &str {
            crate::clock::clock_identity()
        }
        fn allowed(&self, digest: &str) -> bool {
            super::LINUX_MONOTONIC_IMAGES.contains(&digest)
        }
        fn parent_pid(&self) -> i32 {
            parent::parent_pid()
        }
        fn own_pid(&self) -> i32 {
            parent::own_pid()
        }
        fn pidfd_open(&self, pid: i32) -> io::Result<Handle> {
            parent::pidfd_open(pid).map(Handle::Pidfd)
        }
        fn pidfd_exited(&self, pidfd: &Handle) -> io::Result<bool> {
            match pidfd {
                Handle::Pidfd(fd) => parent::pidfd_exited(fd.as_fd()),
                Handle::Image(_) => Err(wrong()),
            }
        }
        fn process_start(&self, pid: i32) -> io::Result<u64> {
            let raw = parent::read_proc(
                Path::new(&format!("/proc/{pid}/stat")),
                super::MAX_PROCESS_STAT + 1,
            )?;
            super::process_start(&raw)
        }
        fn time_namespace(&self, pid: i32) -> io::Result<TimeNamespace> {
            super::time_namespace_with(|name| {
                parent::stat(Path::new(&format!("/proc/{pid}/ns/{name}"))).map(|s| (s.dev, s.ino))
            })
        }
        fn open_image(&self, pid: i32) -> io::Result<Handle> {
            parent::open_image(pid).map(Handle::Image)
        }
        fn fstat(&self, handle: &Handle) -> io::Result<ImageStat> {
            match handle {
                Handle::Image(file) => parent::fstat(file).map(image_stat),
                Handle::Pidfd(_) => Err(wrong()),
            }
        }
        fn stat_image(&self, pid: i32) -> io::Result<ImageStat> {
            parent::stat(Path::new(&format!("/proc/{pid}/exe"))).map(image_stat)
        }
        fn read(&self, handle: &mut Handle, count: usize) -> io::Result<Vec<u8>> {
            match handle {
                Handle::Image(file) => parent::read_block(file, count),
                Handle::Pidfd(_) => Err(wrong()),
            }
        }
        fn close(&self, handle: Handle) {
            drop(handle);
        }
    }
}

/// The background observation for this platform, if it has one.
pub(crate) fn system_image(platform: Platform) -> Option<Arc<dyn NativeImage>> {
    #[cfg(target_os = "linux")]
    if platform == Platform::Linux {
        let image: Arc<dyn NativeImage> = LinuxNativeImage::start(Arc::new(system::Proc));
        return Some(image);
    }
    let _ = platform;
    None
}

/// The allowlisted image digests, exposed for diagnostics and contracts.
pub fn qualified_images() -> &'static [&'static str] {
    &LINUX_MONOTONIC_IMAGES
}
