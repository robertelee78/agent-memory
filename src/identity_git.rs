//! Local read-only identity plumbing without ambient Git redirection or hooks.
//! All child ownership and pipe supervision belongs to the T0 process layer.
use crate::clock::Deadline;
use crate::error::ContractError;
use crate::process::{self, Input, Output, ProcessError, Refusal, Spec, Status, Target};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::time::Duration;

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

pub fn _run(
    repo: &Path,
    args: &[OsString],
    glob_pathspecs: bool,
) -> std::result::Result<Output, ProcessError> {
    let invocation = _invocation(repo, args, glob_pathspecs);
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
    process::run_for(
        &spec(invocation, Deadline::from_absolute_ns(u64::MAX)),
        timeout,
    )
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
    _git_output(_run(repo, args, glob_pathspecs), optional)
}

pub fn root_head(repo: &Path) -> Result<(Option<OsString>, Option<String>)> {
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
pub struct GitGroup<'a> {
    group: process::Group<'a>,
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
        let outcome = self.group.value(index);
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
        group,
        next: 0,
        len: specs.len(),
        failed: false,
    };
    let primary = body(&mut owned);
    let incomplete = owned.next != owned.len;
    let failed = owned.failed || owned.group.failed();
    // Explicit finish is mandatory: Drop is solely the panic/early-exit safety net.
    let cleanup = owned.group.finish();
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
