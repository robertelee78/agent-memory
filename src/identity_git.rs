//! Local read-only identity plumbing without ambient Git redirection or hooks.
//! All child ownership and pipe supervision belongs to the T0 process layer.
use crate::clock::Deadline;
use crate::error::ContractError;
use crate::process::{self, Input, Output, ProcessError, Refusal, Spec, Status, Target};
use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

mod observation;

pub const _QUERY_TIMEOUT: f64 = 10.0;
pub const _EXIT_POLL_INTERVAL: f64 = 0.05;
pub const _INTERRUPT_GRACE: f64 = 0.25;

#[derive(Debug)]
pub enum Error {
    Contract(ContractError),
    Infrastructure(ProcessError),
    InvalidInput(&'static str),
    Supervision {
        message: &'static str,
        source: Option<ProcessError>,
    },
    Cleanup {
        primary: Box<Error>,
        cleanup: ProcessError,
    },
}
impl Error {
    /// The caller's contract refusal stays primary even if cleanup also failed.
    pub fn contract(&self) -> Option<&ContractError> {
        match self {
            Self::Contract(error) => Some(error),
            Self::Cleanup { primary, .. } => primary.contract(),
            _ => None,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(error) => error.fmt(f),
            Self::Infrastructure(error) => error.fmt(f),
            Self::InvalidInput(message) | Self::Supervision { message, .. } => f.write_str(message),
            Self::Cleanup { primary, .. } => primary.fmt(f),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Contract(error) => Some(error),
            Self::Infrastructure(error) => Some(error),
            Self::InvalidInput(_) => None,
            Self::Supervision { source, .. } => source
                .as_ref()
                .map(|e| e as &(dyn std::error::Error + 'static)),
            Self::Cleanup { primary, .. } => Some(primary.as_ref()),
        }
    }
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone, PartialEq, Eq)]
pub struct Invocation {
    pub argv: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
}
pub fn _invocation(repo: &Path, args: &[OsString], glob_pathspecs: bool) -> Invocation {
    invocation_with_env(repo, args, glob_pathspecs, std::env::vars_os())
}
pub fn invocation_with_env(
    repo: &Path,
    args: &[OsString],
    glob_pathspecs: bool,
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
) -> Invocation {
    let mut seen = std::collections::HashSet::new();
    let mut env: Vec<_> = inherited
        .into_iter()
        .filter(|(key, _)| !key.as_bytes().starts_with(b"GIT_") && seen.insert(key.clone()))
        .collect();
    env.extend(
        [
            ("GIT_NO_LAZY_FETCH", "1"),
            ("GIT_ALLOW_PROTOCOL", ""),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("GIT_TRACE", "0"),
            ("GIT_TRACE2", "0"),
            ("GIT_TRACE2_EVENT", "0"),
            ("GIT_TRACE2_PERF", "0"),
        ]
        .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    );
    let mut argv: Vec<_> = [
        "git",
        "--no-pager",
        "--no-replace-objects",
        if glob_pathspecs {
            "--glob-pathspecs"
        } else {
            "--literal-pathspecs"
        },
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-C",
    ]
    .map(OsString::from)
    .into();
    argv.push(repo.as_os_str().to_owned());
    argv.extend_from_slice(args);
    Invocation { argv, env }
}

struct QueryObservation {
    root: PathBuf,
    values: Vec<(Invocation, Output)>,
    facts: Option<observation::Facts>,
}
thread_local! {
    static QUERIES: RefCell<Option<QueryObservation>> = const { RefCell::new(None) };
}
pub(crate) struct QueryObservationGuard(Option<QueryObservation>);
impl Drop for QueryObservationGuard {
    fn drop(&mut self) {
        QUERIES.with(|queries| *queries.borrow_mut() = self.0.take());
    }
}
pub(crate) fn observe_queries(root: &Path) -> QueryObservationGuard {
    let facts = observation::Facts::capture(root);
    QueryObservationGuard(QUERIES.with(|queries| {
        queries.replace(Some(QueryObservation {
            root: root.to_path_buf(),
            values: Vec::new(),
            facts,
        }))
    }))
}
pub(crate) fn finish_queries() {
    QUERIES.with(|queries| *queries.borrow_mut() = None);
}
fn copy_output(output: &Output) -> Output {
    Output {
        pid: output.pid,
        status: output.status,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
        elapsed_ns: output.elapsed_ns,
        launch_elapsed_ns: output.launch_elapsed_ns,
        poll_wakeups: output.poll_wakeups,
    }
}
fn observed_query(invocation: &Invocation) -> Option<Output> {
    QUERIES.with(|queries| {
        let queries = queries.borrow();
        let observation = queries.as_ref()?;
        observation
            .facts
            .as_ref()
            .and_then(|facts| facts.output(invocation))
            .or_else(|| {
                observation
                    .values
                    .iter()
                    .find(|(key, _)| key == invocation)
                    .map(|(_, output)| copy_output(output))
            })
    })
}
fn batch_value(invocation: &Invocation) -> Option<Option<OsString>> {
    QUERIES.with(|queries| queries.borrow().as_ref()?.facts.as_ref()?.value(invocation))
}
fn accepted_value(value: Option<OsString>, optional: bool) -> Result<Option<OsString>> {
    if value.is_some() || optional {
        Ok(value)
    } else {
        Err(Error::Contract(ContractError::new(
            "Cannot inspect repository evidence with Git; no identity change performed",
        )))
    }
}
fn retain_query(invocation: &Invocation, output: &Output) {
    // Only the local read-only identity commands participate. No mutation,
    // failed supervision or different repository enters the observation.
    let args = &invocation.argv[10..];
    let read_only = match args.first().and_then(|arg| arg.to_str()) {
        Some("rev-parse" | "ls-files" | "ls-tree" | "cat-file") => true,
        Some("config") => args
            .iter()
            .any(|arg| matches!(arg.to_str(), Some("--get-all" | "--get-regexp" | "--get"))),
        Some("remote") => args.len() == 1 || args.get(1).is_some_and(|arg| arg == "get-url"),
        _ => false,
    };
    if !read_only {
        return;
    }
    QUERIES.with(|queries| {
        if let Some(observation) = queries.borrow_mut().as_mut() {
            if invocation.argv.get(9).map(OsString::as_os_str) == Some(observation.root.as_os_str())
                && !observation.values.iter().any(|(key, _)| key == invocation)
            {
                observation
                    .values
                    .push((invocation.clone(), copy_output(output)));
            }
        }
    });
}
/// Re-run every sampled Git query in bounded groups before publishing a result.
/// Filesystem/source validators still run normally; these bytes are never
/// retained beyond the owning repository observation.
pub(crate) fn recheck_queries() -> crate::error::Result<()> {
    QUERIES.with(|queries| {
        let queries = queries.borrow();
        let Some(observation) = queries.as_ref() else {
            return Ok(());
        };
        if let Some(facts) = &observation.facts {
            facts.recheck()?;
        }
        // Finish enumeration children before launching per-object readers.
        // A later scalar read must never overlap the discovery that selected it.
        let enumeration = |invocation: &Invocation| {
            let args = &invocation.argv[10..];
            matches!(
                args.first().and_then(|arg| arg.to_str()),
                Some("ls-files" | "ls-tree" | "remote")
            ) || args.iter().any(|arg| arg == "--get-regexp")
        };
        for phase in [true, false] {
            let selected: Vec<_> = observation
                .values
                .iter()
                .filter(|(invocation, _)| enumeration(invocation) == phase)
                .collect();
            for chunk in selected.chunks(3) {
                let specs: Vec<_> = chunk
                    .iter()
                    .map(|(invocation, _)| spec(invocation, Deadline::from_absolute_ns(u64::MAX)))
                    .collect();
                let mut group = process::Group::launch(&specs, Duration::from_secs(10))
                    .map_err(|error| ContractError::new(error.to_string()))?;
                let checked = (|| {
                    for (index, (_, expected)) in chunk.iter().enumerate() {
                        let current = group
                            .value(index)
                            .map_err(|error| ContractError::new(error.to_string()))?;
                        #[cfg(feature = "dev-tools")]
                        let _ = crate::runtime_faults::git_spawn(current.pid);
                        if current.status != expected.status || current.stdout != expected.stdout {
                            return Err(ContractError::identity_unresolved(
                                "Repository identity evidence changed during observation",
                            ));
                        }
                    }
                    Ok(())
                })();
                let finished = group
                    .finish()
                    .map_err(|error| ContractError::new(error.to_string()));
                checked?;
                finished?;
            }
        }
        Ok(())
    })
}

pub(crate) fn recheck_operation_queries() -> crate::error::Result<()> {
    recheck_queries()
}

pub fn _run(
    repo: &Path,
    args: &[OsString],
    glob_pathspecs: bool,
) -> std::result::Result<Output, ProcessError> {
    let invocation = _invocation(repo, args, glob_pathspecs);
    if let Some(output) = observed_query(&invocation) {
        return Ok(output);
    }
    let result = run_invocation_for(&invocation, Duration::from_secs(10));
    #[cfg(feature = "dev-tools")]
    if args.first().is_some_and(|arg| arg == "remote")
        && args.get(1).is_some_and(|arg| arg == "get-url")
    {
        if let Err(fault) =
            crate::bank_writes::fault("project_identity.remote_get_url.Popen.stdout.close")
        {
            let mut error = match result {
                Err(error) => error,
                Ok(output) => ProcessError {
                    reason: process::Refusal::Infrastructure,
                    evidence: Some(process::Evidence {
                        argv0: invocation.argv.first().cloned().unwrap_or_default(),
                        pid: output.pid,
                        elapsed_ns: output.elapsed_ns,
                        launch_elapsed_ns: output.launch_elapsed_ns,
                        stdout: output.stdout,
                        stderr: output.stderr,
                        killed: false,
                        reaped: true,
                        status: Some(output.status),
                        poll_wakeups: output.poll_wakeups,
                    }),
                    source: None,
                    cleanup_error: None,
                    incomplete: false,
                },
            };
            error.reason = process::Refusal::Infrastructure;
            error.cleanup_error = Some(std::io::Error::other(fault.to_string()));
            return Err(error);
        }
    }
    if let Ok(output) = &result {
        retain_query(&invocation, output);
    }
    result
}
/// Separate deadline/environment injection lets the caller retain its own fixed
/// operation context; production `_run` always inherits the exact ambient non-GIT environment.
pub fn run_invocation(
    invocation: &Invocation,
    deadline: Deadline,
) -> std::result::Result<Output, ProcessError> {
    process::run(&spec(invocation, deadline))
}

pub fn run_invocation_for(
    invocation: &Invocation,
    timeout: Duration,
) -> std::result::Result<Output, ProcessError> {
    let result = process::run_for(
        &spec(invocation, Deadline::from_absolute_ns(u64::MAX)),
        timeout,
    );
    #[cfg(feature = "dev-tools")]
    if let Ok(output) = &result {
        let _ = crate::runtime_faults::git_spawn(output.pid);
    }
    result
}
fn spec(invocation: &Invocation, deadline: Deadline) -> Spec<'_> {
    Spec {
        argv: &invocation.argv,
        env: &invocation.env,
        cwd: None,
        search_path: true,
        stdin: Input::Inherit,
        // The baseline has no stdout/stderr capture bound. Do not invent one.
        stdout: Target::Capture { limit: usize::MAX },
        stderr: Target::Capture { limit: usize::MAX },
        deadline,
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    }
}
fn ordinary_absence(error: &ProcessError) -> bool {
    error.cleanup_error.is_none()
        && (error.reason == Refusal::Deadline
            || (error.reason == Refusal::Spawn
                && error
                    .source
                    .as_ref()
                    .is_some_and(|e| e.raw_os_error() == Some(libc::ENOENT))))
}

/// `os.fsdecode` semantics are represented losslessly by OsString on Unix;
/// only one trailing LF is removed, never whitespace or an additional LF.
pub fn _git_output(
    command: std::result::Result<Output, ProcessError>,
    optional: bool,
) -> Result<Option<OsString>> {
    match command {
        Err(error) if ordinary_absence(&error) => {
            if optional {
                Ok(None)
            } else {
                Err(Error::Contract(ContractError::new(
                    "Cannot inspect local repository identity evidence",
                )))
            }
        }
        Err(error) => Err(Error::Infrastructure(error)),
        Ok(command) if command.status != Status::Code(0) => {
            if optional {
                Ok(None)
            } else {
                Err(Error::Contract(ContractError::new(
                    "Cannot inspect repository evidence with Git; no identity change performed",
                )))
            }
        }
        Ok(mut command) => {
            if command.stdout.last() == Some(&b'\n') {
                command.stdout.pop();
            }
            Ok(Some(OsString::from_vec(command.stdout)))
        }
    }
}
pub fn git(
    repo: &Path,
    args: &[OsString],
    optional: bool,
    glob_pathspecs: bool,
) -> Result<Option<OsString>> {
    if let Some(value) = batch_value(&_invocation(repo, args, glob_pathspecs)) {
        return accepted_value(value, optional);
    }
    _git_output(_run(repo, args, glob_pathspecs), optional)
}

pub fn root_head(repo: &Path) -> Result<(Option<OsString>, Option<String>)> {
    if let Some(value) = QUERIES.with(|queries| {
        queries
            .borrow()
            .as_ref()
            .filter(|observation| observation.root == repo)
            .and_then(|observation| {
                let current = _invocation(
                    repo,
                    &["rev-parse", "--show-toplevel"].map(OsString::from),
                    false,
                );
                observation
                    .facts
                    .as_ref()
                    .filter(|facts| facts.value(&current).is_some())
                    .map(observation::Facts::root_head)
            })
    }) {
        return Ok(value);
    }
    let args = [
        "rev-parse",
        "--show-toplevel",
        "--verify",
        "--quiet",
        "HEAD",
    ]
    .map(OsString::from);
    match _run(repo, &args, false) {
        Err(error)
            if error.cleanup_error.is_none()
                && matches!(error.reason, Refusal::Spawn | Refusal::Deadline) =>
        {
            Ok((None, None))
        }
        Err(error) => Err(Error::Infrastructure(error)),
        Ok(command) => root_head_output(&command),
    }
}
/// Parse a complete observation without treating malformed success as absence.
pub fn root_head_output(command: &Output) -> Result<(Option<OsString>, Option<String>)> {
    let output = &command.stdout;
    if command.status != Status::Code(0) {
        if matches!(command.status, Status::Code(n) if n > 0)
            && output.ends_with(b"\n")
            && output != b"\n"
        {
            return Ok((
                Some(OsStr::from_bytes(&output[..output.len() - 1]).to_owned()),
                None,
            ));
        }
        return Ok((None, None));
    }
    if let Some(bytes) = output.strip_suffix(b"\n") {
        if let Some(separator) = bytes.iter().rposition(|b| *b == b'\n') {
            let (root, head) = (&bytes[..separator], &bytes[separator + 1..]);
            if !root.is_empty()
                && matches!(head.len(), 40 | 64)
                && head
                    .iter()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            {
                return Ok((
                    Some(OsStr::from_bytes(root).to_owned()),
                    Some(String::from_utf8(head.to_vec()).unwrap()),
                ));
            }
        }
    }
    Err(Error::Contract(ContractError::new(
        "Repository root/HEAD observation is incomplete; do not infer absence",
    )))
}

/// A query's owned process never escapes the invoking closure.
enum CachedValue {
    Process(Output),
    Batch(Option<OsString>),
}
pub struct GitGroup<'a> {
    group: Option<process::Group<'a>>,
    cached: Vec<Option<CachedValue>>,
    invocations: &'a [Invocation],
    next: usize,
    len: usize,
    failed: bool,
}
impl GitGroup<'_> {
    pub fn value(&mut self, index: usize, optional: bool) -> Result<Option<OsString>> {
        if index != self.next || index >= self.len {
            return Err(Error::InvalidInput(
                "Git query outputs must be consumed once in original order",
            ));
        }
        if self.failed {
            return Err(Error::Supervision {
                message: "Git query supervision failed; no observation accepted",
                source: None,
            });
        }
        if self.group.is_none() {
            let value = self.cached[index]
                .take()
                .expect("Ordered cached Git output");
            self.next += 1;
            return match value {
                CachedValue::Process(output) => _git_output(Ok(output), optional),
                CachedValue::Batch(value) => accepted_value(value, optional),
            };
        }
        let outcome = match &mut self.group {
            Some(group) => group.value(index),
            None => unreachable!("Cached Git value already consumed"),
        };
        if let Err(error) = &outcome {
            if error.reason == Refusal::Infrastructure {
                self.failed = true;
                return Err(Error::Supervision {
                    message: "Git query supervision failed; no observation accepted",
                    source: outcome.err(),
                });
            }
        }
        self.next += 1;
        if let Ok(output) = &outcome {
            #[cfg(feature = "dev-tools")]
            let _ = crate::runtime_faults::git_spawn(output.pid);
            if let Some(invocation) = self.invocations.get(index) {
                retain_query(invocation, output);
            }
        }
        _git_output(outcome, optional)
    }
}

/// Typed equivalent of the baseline `_git_group` context manager. All one to
/// three children launch before the parser runs. Output acceptance is ordered;
/// parsing time is charged to still-pending children by the T0 owner.
pub fn _git_group<T>(
    repo: &Path,
    specs: &[(Vec<OsString>, bool)],
    body: impl FnOnce(&mut GitGroup<'_>) -> Result<T>,
) -> Result<T> {
    if !(1..=3).contains(&specs.len()) {
        return Err(Error::InvalidInput(
            "A Git query group requires one to three fixed commands",
        ));
    }
    if QUERIES.with(|queries| queries.borrow().is_some()) {
        let cached: Option<Vec<_>> = specs
            .iter()
            .map(|(args, glob)| {
                let invocation = _invocation(repo, args, *glob);
                batch_value(&invocation)
                    .map(CachedValue::Batch)
                    .or_else(|| observed_query(&invocation).map(CachedValue::Process))
            })
            .collect();
        if let Some(cached) = cached {
            let mut group = GitGroup {
                group: None,
                cached: cached.into_iter().map(Some).collect(),
                invocations: &[],
                next: 0,
                len: specs.len(),
                failed: false,
            };
            let result = body(&mut group)?;
            if group.next != group.len {
                return Err(Error::InvalidInput(
                    "Incomplete ordered Git query observation",
                ));
            }
            return Ok(result);
        }
    }
    git_group_with(
        repo,
        specs,
        &std::env::vars_os().collect::<Vec<_>>(),
        Duration::from_secs(10),
        body,
    )
}

/// Explicit environment/interval injection; no global environment mutation and
/// no worker thread are needed to exercise real Git supervision.
pub fn git_group_with<T>(
    repo: &Path,
    specs: &[(Vec<OsString>, bool)],
    inherited: &[(OsString, OsString)],
    timeout: Duration,
    body: impl FnOnce(&mut GitGroup<'_>) -> Result<T>,
) -> Result<T> {
    if !(1..=3).contains(&specs.len()) {
        return Err(Error::InvalidInput(
            "A Git query group requires one to three fixed commands",
        ));
    }
    let invocations: Vec<_> = specs
        .iter()
        .map(|(args, glob)| invocation_with_env(repo, args, *glob, inherited.iter().cloned()))
        .collect();
    let process_specs: Vec<_> = invocations
        .iter()
        .map(|invocation| spec(invocation, Deadline::from_absolute_ns(u64::MAX)))
        .collect();
    let mut group =
        process::Group::launch(&process_specs, timeout).map_err(|source| Error::Supervision {
            message: "Git query supervision failed; no observation accepted",
            source: Some(source),
        })?;
    group
        .set_exit_poll_interval(Duration::from_millis(50))
        .map_err(|source| Error::Supervision {
            message: "Git query supervision failed; no observation accepted",
            source: Some(source),
        })?;
    let mut owned = GitGroup {
        group: Some(group),
        cached: Vec::new(),
        invocations: &invocations,
        next: 0,
        len: specs.len(),
        failed: false,
    };
    let primary = body(&mut owned);
    let incomplete = owned.next != owned.len;
    let group = owned.group.take().expect("Owned Git query group");
    let failed = owned.failed || group.failed();
    // Explicit finish is mandatory: Drop is solely the panic/early-exit safety net.
    let cleanup = group.finish();
    match primary {
        Err(primary) => match cleanup {
            Ok(()) => Err(primary),
            Err(cleanup) => Err(Error::Cleanup {
                primary: Box::new(primary),
                cleanup,
            }),
        },
        Ok(_) if failed => Err(Error::Supervision {
            message: "Git query supervision failed; no observation accepted",
            source: cleanup.err(),
        }),
        Ok(_) if incomplete => Err(Error::Supervision {
            message: "Incomplete ordered Git query observation",
            source: cleanup.err(),
        }),
        Ok(value) => match cleanup {
            Ok(()) => Ok(value),
            Err(source) => Err(Error::Supervision {
                message: "Unsafe Git query cleanup; no observation accepted",
                source: Some(source),
            }),
        },
    }
}
