//! Baseline `locked`/`read_owner`: the `flock(2)` lifecycle lock at
//! `state/lock` and the separately published `owner.json` PID metadata. Both
//! runtimes use the same paths, lock class and record format.
use super::filesystem::{
    cleanup_file, lexists, make_state_directory, named, native_rename, replace_record, unique_name,
    write_file,
};
use super::plan::{preflight_path, unchanged_parent};
use super::{
    Environment, Error, LOCK_SCHEMA, MAX_LOCK, OWNER_SCHEMA, Result, encoded, load, path_value,
    require,
};
use crate::codec::json::{self, DecodePolicy, Value};
use crate::error::ContractError;
use crate::fs::{self, DirectoryChain, FlockGuard, LockError, LockMode};
use crate::lifecycle::{object, string, text};
use std::ffi::OsStr;
use std::path::Path;

/// Baseline `read_owner`: read existing lock metadata without creating,
/// replacing or stealing it.
pub fn read_owner(state: &Path) -> Result<Option<Value>> {
    let path = state.join("owner.json");
    if !lexists(&path)? {
        return Ok(None);
    }
    let owner = load(&path)?;
    require(
        owner.as_object().is_some() && text(&owner, "schema") == Some(OWNER_SCHEMA),
        "Unrelated lifecycle owner record; preserve it for reconciliation",
    )?;
    Ok(Some(owner))
}

/// The held lifecycle lock. Dropping it releases the `flock`.
#[derive(Debug)]
pub struct Locked {
    _guard: FlockGuard,
}

fn seed_lock(state: &Path, path: &Path) -> Result<()> {
    let seed = state.join(unique_name(".lock-seed-")?);
    let seeded = (|| -> Result<()> {
        write_file(
            &seed,
            &encoded(&object([("schema", string(LOCK_SCHEMA))]))?,
            0o600,
        )?;
        match native_rename(&seed, path, false) {
            Ok(()) => Ok(()),
            // A concurrent seeder won; its lock file is the one to hold.
            Err(Error::Contract(error)) if lexists(path)? && error.message().contains("exists") => {
                Ok(())
            }
            Err(error) => Err(error),
        }
    })();
    let cleaned = cleanup_file(&seed);
    seeded?;
    cleaned
}

fn acquire(state: &Path, path: &Path) -> Result<FlockGuard> {
    fs::trust::record(path)?;
    let chain = DirectoryChain::open(state)?;
    let file = chain
        .directory()
        .open_at(OsStr::new("lock"), libc::O_RDWR | libc::O_NONBLOCK, 0)?;
    let mut guard =
        FlockGuard::try_acquire(file, LockMode::Exclusive).map_err(|error| match error {
            LockError::Busy => Error::Contract(ContractError::new(
                "Another lifecycle process holds the lock; retry after it finishes",
            )),
            other => other.into(),
        })?;
    let info = guard.descriptor().stat()?;
    require(
        info.is_regular() && info.nlink == 1,
        "Lifecycle lock is not a private regular file",
    )?;
    let unrelated = || {
        Error::Contract(ContractError::new(
            "Unrelated lifecycle lock file; preserve it for reconciliation",
        ))
    };
    let prior = guard.read_bounded(MAX_LOCK).map_err(|_| unrelated())?;
    let value = json::decode_bytes(&prior, DecodePolicy::default()).map_err(|_| unrelated())?;
    if value.as_object().is_none() || text(&value, "schema") != Some(LOCK_SCHEMA) {
        return Err(unrelated());
    }
    chain.recheck()?;
    Ok(guard)
}

/// Baseline `locked`: trusted private state, seeded lock inode, non-blocking
/// exclusive `flock`, then atomically published owner metadata.
pub fn locked<T>(env: &Environment, state: &Path, body: impl FnOnce() -> Result<T>) -> Result<T> {
    fs::trust::state(state)?;
    preflight_path(env, state)?;
    unchanged_parent(state)?;
    require(
        !named(state)?.is_some_and(|info| info.is_symlink()),
        "Installation state directory is a symlink",
    )?;
    make_state_directory(state)?;
    let path = state.join("lock");
    require(
        !named(&path)?.is_some_and(|info| info.is_symlink()),
        format!("Lifecycle lock is a symlink: {}", path.display()),
    )?;
    if !lexists(&path)? {
        seed_lock(state, &path)?;
    }
    let guard = acquire(state, &path)?;
    read_owner(state)?;
    // Keep the flock inode stable. Diagnostic PID metadata is published
    // separately and atomically, so a killed writer cannot tear the lock.
    replace_record(
        &state.join("owner.json"),
        &object([
            ("schema", string(OWNER_SCHEMA)),
            ("lock_path", path_value(&path)?),
            ("pid", Value::Int(crate::host::pid().into())),
            ("started_at", string(crate::host::utc_now_isoformat()?)),
        ]),
    )?;
    fs::trust::state(state)?;
    fs::trust::record(&path)?;
    let held = Locked { _guard: guard };
    let result = body();
    drop(held);
    result
}
