//! Descriptor and byte reuse inside one source observation, never between frames.
use super::*;
use std::path::Component;

const MAX_DIRECTORIES: usize = 64;
const MAX_BYTES: usize = 8 * 1024 * 1024;

struct Directory {
    descriptor: Descriptor,
    identity: Identity,
    protected: bool,
}
#[derive(Default)]
struct Observation {
    directories: BTreeMap<PathBuf, Directory>,
    records: BTreeMap<PathBuf, (Identity, Vec<u8>)>,
    bytes: usize,
}
thread_local! {
    static CURRENT: RefCell<Option<Observation>> = const { RefCell::new(None) };
}
pub(crate) struct Guard(Option<Observation>);
impl Drop for Guard {
    fn drop(&mut self) {
        CURRENT.with(|current| *current.borrow_mut() = self.0.take());
    }
}
pub(crate) fn observe() -> Guard {
    Guard(CURRENT.with(|current| current.replace(Some(Observation::default()))))
}
pub(crate) fn finish() {
    CURRENT.with(|current| *current.borrow_mut() = None);
}
impl Observation {
    fn recheck(&self) -> Result<()> {
        for (path, directory) in &self.directories {
            let opened = directory.descriptor.stat().map_err(io)?;
            let named = if path == Path::new("/") {
                opened
            } else {
                self.directories
                    .get(parent(path)?)
                    .ok_or_else(|| err("Observed source parent is unavailable"))?
                    .descriptor
                    .stat_at(leaf(path)?)
                    .map_err(io)?
            };
            if !directory.identity.same_binding(opened) || !directory.identity.same_binding(named) {
                return Err(err(
                    "Source state or its parent changed; retain pending evidence",
                ));
            }
        }
        Ok(())
    }
    fn directory(&mut self, path: &Path) -> Result<()> {
        if self.directories.contains_key(path) {
            return Ok(());
        }
        let (descriptor, identity, protected) = if path == Path::new("/") {
            let descriptor = Descriptor::pin_directory(path).map_err(io)?;
            let identity = descriptor.stat().map_err(io)?;
            let protected = fs::trust::inspect_entry(path, &identity, false, false, false)?.0;
            (descriptor, identity, protected)
        } else {
            let parent = parent(path)?;
            self.directory(parent)?;
            let directory = &self.directories[parent];
            let identity = directory.descriptor.stat_at(leaf(path)?).map_err(io)?;
            let protected =
                fs::trust::inspect_entry(path, &identity, directory.protected, false, false)?.0;
            let descriptor = directory
                .descriptor
                .pin_directory_at(leaf(path)?)
                .map_err(io)?;
            if !descriptor.stat().map_err(io)?.same_binding(identity) {
                return Err(err(
                    "Source state or its parent changed; retain pending evidence",
                ));
            }
            (descriptor, identity, protected)
        };
        self.directories.insert(
            path.to_path_buf(),
            Directory {
                descriptor,
                identity,
                protected,
            },
        );
        Ok(())
    }
    fn read(&mut self, path: &Path, maximum: usize) -> Result<Option<Vec<u8>>> {
        if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
            return fs::trust::refuse(path, "an absolute canonical path is required");
        }
        if self.directories.len() >= MAX_DIRECTORIES {
            self.recheck()?;
            self.directories.clear();
        }
        let parent = parent(path)?;
        if let Err(error) = self.directory(parent) {
            if matches!(
                error.kind(),
                Kind::Io {
                    kind: std::io::ErrorKind::NotFound,
                    ..
                }
            ) {
                return Ok(None);
            }
            return Err(error);
        }
        let directory = &self.directories[parent];
        let before = match directory.descriptor.stat_at(leaf(path)?) {
            Ok(identity) => identity,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io(error)),
        };
        fs::trust::inspect_entry(path, &before, directory.protected, true, false)?;
        if !before.is_regular()
            || before.uid != crate::host::effective_uid()
            || before.nlink != 1
            || before.mode & 0o077 != 0
            || before.size < 0
            || before.size as u128 > maximum as u128
        {
            return Err(err("Unsafe source state; preserve it for inspection"));
        }
        if let Some((identity, raw)) = self.records.get(path) {
            if *identity == before {
                if directory.descriptor.stat_at(leaf(path)?).map_err(io)? != before {
                    return Err(err(
                        "Source state or its parent changed; retain pending evidence",
                    ));
                }
                return Ok(Some(raw.clone()));
            }
        }
        let mut file =
            match directory
                .descriptor
                .open_at(leaf(path)?, libc::O_RDONLY | libc::O_NONBLOCK, 0)
            {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(io(error)),
            };
        if file.stat().map_err(io)? != before {
            return Err(err(
                "Source state or its parent changed; retain pending evidence",
            ));
        }
        let raw = file
            .read_prefix(
                maximum
                    .checked_add(1)
                    .ok_or_else(|| err("Source read bound overflow"))?,
            )
            .map_err(io)?;
        let after = file.stat().map_err(io)?;
        let current = match directory.descriptor.stat_at(leaf(path)?) {
            Ok(current) => current,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io(error)),
        };
        if raw.len() as u128 != before.size as u128 || before != after || before != current {
            return Err(err(
                "Source state or its parent changed; retain pending evidence",
            ));
        }
        if let Some((_, old)) = self.records.remove(path) {
            self.bytes -= old.len();
        }
        if raw.len() <= MAX_BYTES {
            if self.bytes + raw.len() > MAX_BYTES {
                self.records.clear();
                self.bytes = 0;
            }
            self.bytes += raw.len();
            self.records
                .insert(path.to_path_buf(), (before, raw.clone()));
        }
        Ok(Some(raw))
    }
}
pub(crate) fn recheck() -> Result<()> {
    CURRENT.with(|current| match current.borrow().as_ref() {
        Some(observation) => observation.recheck(),
        None => Ok(()),
    })
}
pub(super) fn read(path: &Path, maximum: usize) -> Option<Result<Option<Vec<u8>>>> {
    CURRENT.with(|current| {
        current
            .borrow_mut()
            .as_mut()
            .map(|observation| observation.read(path, maximum))
    })
}
