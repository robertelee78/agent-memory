//! The finite worker: bounded startup, READY, one framed reader thread and
//! exactly one serial source executor, then FINAL accounting.
use super::clock_window::ClockWindows;
use super::coverage::{Coverage, Installed, StreamAuthority, binding};
use super::executor::{FiniteExecutor, metadata, parse_utf8};
use super::frame::{Budget, Frame, FrameReader, Output, Responder, ResponseWriter};
use super::{
    BUSY_BACKOFF, BUSY_SECONDS, CONTROL_TYPES, MAX_FRAME, MAX_FRAMES, MAX_RETAINED, busy, int,
    preload, string, uuid_text,
};
use crate::cli::parser::{self, Arg, Opt, Parser};
use crate::codec::json::Value;
use crate::error::Result;
use crate::lifecycle::{CommandResult, get};
use crate::turn_sources;
use std::collections::VecDeque;
use std::io::Read;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Injected monotonic time and sleep for bounded busy retries.
#[derive(Clone)]
pub struct Timing {
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
    sleep: Arc<dyn Fn(Duration) + Send + Sync>,
}
impl Timing {
    pub fn system() -> Self {
        Self::new(
            Arc::new(crate::clock::monotonic_ns),
            Arc::new(std::thread::sleep),
        )
    }
    pub fn new(
        now: Arc<dyn Fn() -> u64 + Send + Sync>,
        sleep: Arc<dyn Fn(Duration) + Send + Sync>,
    ) -> Self {
        Self { now, sleep }
    }
    pub fn now_ns(&self) -> u64 {
        (self.now)()
    }
    pub fn sleep(&self, duration: Duration) {
        (self.sleep)(duration)
    }
}

/// Seam to `omp_boundary::stream_startup` (fresh enrollment is a lifecycle
/// transaction, never an observer write). `None` means no bank: INACTIVE.
pub trait StreamStartup: Send + Sync {
    fn stream_startup(
        &self,
        repo: &Path,
        session: &str,
        instance_id: &str,
        lifecycle: &str,
        receipt: &Value,
        runtime_root: &Path,
    ) -> Result<Option<Value>>;
}

/// Startup callback of `run` (the baseline `startup=` argument).
pub type Startup<'a> = dyn FnMut(&Path, &str, &str, &str) -> Result<Option<Value>> + 'a;

/// Everything `run` needs besides its identities.
pub struct RunOptions {
    pub authority: Arc<dyn StreamAuthority>,
    pub receipt: Option<Value>,
    pub input: Box<dyn Read + Send>,
    pub writer: Arc<dyn Responder>,
    /// Clock windows for this instance; `ClockWindows::system` in production.
    pub clocks: fn(&str, Arc<dyn Fn() -> bool + Send + Sync>) -> ClockWindows,
    pub timing: Timing,
}

#[derive(Default)]
struct QueueState {
    frames: VecDeque<Frame>,
    eof: bool,
    error: bool,
    last: Option<Value>,
    next: u64,
}
struct Queue {
    state: Mutex<QueueState>,
    condition: Condvar,
}
impl Queue {
    fn lock(&self) -> std::sync::MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn read_frames(
    mut reader: FrameReader<Box<dyn Read + Send>>,
    budget: &Budget,
    clocks: &ClockWindows,
    writer: &dyn Responder,
    queue: &Queue,
) {
    loop {
        let frame = match reader.read() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(_) => {
                queue.lock().error = true;
                break;
            }
        };
        if CONTROL_TYPES.contains(&frame.kind.as_str()) {
            let applied = parse_utf8(&frame.raw)
                .and_then(|value| clocks.control(&frame.kind, &value, writer));
            if applied.is_err() {
                // A faulty calibration revokes guidance, never source capture.
                clocks.revoke();
            }
            continue;
        }
        let mut state = queue.lock();
        if state.last.is_some() || frame.sequence != state.next {
            budget.release(frame.raw.len());
            state.error = true;
            break;
        }
        if frame.kind == "FINAL" {
            budget.release(frame.raw.len());
            if frame.nonce.is_some() {
                state.error = true;
                break;
            }
            match parse_utf8(&frame.raw) {
                Ok(value) => state.last = Some(value),
                Err(_) => {
                    state.error = true;
                    break;
                }
            }
        } else {
            state.next += 1;
            state.frames.push_back(frame);
        }
        queue.condition.notify_all();
    }
    clocks.end();
    let mut state = queue.lock();
    state.eof = true;
    queue.condition.notify_all();
}

fn refused(writer: &dyn Responder, code: &str, pending: bool) {
    let mut value = vec![("code".to_owned(), string(code))];
    if pending {
        value.push(("outcome".into(), string("unknown")));
        value.push(("status".into(), string("pending")));
    }
    writer.send("REFUSED", 0, value);
}

/// Bounded startup retry; `Ok(None)` is a finished worker with its exit code.
fn start(
    repo: &Path,
    session: &str,
    instance_id: &str,
    lifecycle: &str,
    startup: &mut Startup<'_>,
    options: &RunOptions,
) -> std::result::Result<Coverage, i32> {
    let writer = &*options.writer;
    if preload(options.receipt.as_ref()).is_err() {
        refused(writer, "binding-refused", false);
        writer.finish();
        return Err(1);
    }
    let deadline = options
        .timing
        .now_ns()
        .saturating_add(BUSY_SECONDS * 1_000_000_000);
    loop {
        let attempt = (|| -> Result<Option<Coverage>> {
            let Some(value) = startup(repo, session, instance_id, lifecycle)? else {
                return Ok(None);
            };
            binding(&value, repo, session, instance_id, lifecycle)?;
            let normalized = PathBuf::from(get(&value, "repo_root").as_str().unwrap_or_default());
            Coverage::open(
                Arc::clone(&options.authority),
                &normalized,
                session,
                instance_id,
                lifecycle,
                &value,
                options.receipt.as_ref(),
            )
            .map(Some)
        })();
        match attempt {
            Ok(Some(coverage)) => return Ok(coverage),
            Ok(None) => {
                writer.send(
                    "INACTIVE",
                    0,
                    vec![("code".into(), string("inactive-no-bank"))],
                );
                writer.finish();
                return Err(0);
            }
            Err(error) if busy(&error) => {
                let now = options.timing.now_ns();
                if now >= deadline {
                    refused(writer, "busy-exhausted", true);
                    writer.finish();
                    return Err(1);
                }
                options
                    .timing
                    .sleep(BUSY_BACKOFF.min(Duration::from_nanos(deadline - now)));
            }
            Err(_) => {
                refused(writer, "binding-refused", false);
                writer.finish();
                return Err(1);
            }
        }
    }
}

/// Drain a detached finite stdin, with exactly one serial source executor.
pub fn run(
    repo: &Path,
    session: &str,
    instance_id: &str,
    lifecycle: &str,
    startup: &mut Startup<'_>,
    options: RunOptions,
) -> Result<i32> {
    turn_sources::bounded(&string(session), 512)?;
    uuid_text(instance_id)?;
    uuid_text(lifecycle)?;
    let mut coverage = match start(repo, session, instance_id, lifecycle, startup, &options) {
        Ok(coverage) => coverage,
        Err(code) => return Ok(code),
    };
    let RunOptions {
        receipt,
        input,
        writer,
        clocks,
        timing,
        ..
    } = options;
    let eligible_writer = Arc::clone(&writer);
    let clocks = clocks(instance_id, Arc::new(move || eligible_writer.live()));
    let budget = Arc::new(Budget::new());
    let queue = Arc::new(Queue {
        state: Mutex::new(QueueState {
            next: 1,
            ..QueueState::default()
        }),
        condition: Condvar::new(),
    });
    let reader = FrameReader::new(input, Arc::clone(&budget));
    let thread = {
        let (budget, clocks, writer, queue) = (
            Arc::clone(&budget),
            clocks.clone(),
            Arc::clone(&writer),
            Arc::clone(&queue),
        );
        std::thread::Builder::new()
            .name("omp-stream-reader".into())
            .spawn(move || read_frames(reader, &budget, &clocks, &*writer, &queue))
    };
    let code = match thread {
        Ok(thread) => {
            let executor = FiniteExecutor::new(
                Arc::clone(&writer),
                clocks.clone(),
                receipt,
                None,
                timing.clone(),
            );
            drain(
                &mut coverage,
                &executor,
                &writer,
                &budget,
                &queue,
                thread,
                &timing,
            )
        }
        Err(_) => 1,
    };
    clocks.end();
    writer.finish();
    coverage.close_live();
    Ok(code)
}

fn drain(
    coverage: &mut Coverage,
    executor: &FiniteExecutor,
    writer: &Arc<dyn Responder>,
    budget: &Budget,
    queue: &Queue,
    thread: std::thread::JoinHandle<()>,
    timing: &super::run::Timing,
) -> i32 {
    writer.send(
        "READY",
        0,
        vec![
            ("wake_support".into(), string("unqualified")),
            ("max_frame".into(), int(MAX_FRAME as u64)),
            ("max_retained".into(), int(MAX_RETAINED as u64)),
            ("max_frames".into(), int(MAX_FRAMES as u64)),
        ],
    );
    loop {
        let frame = {
            let mut state = queue.lock();
            while state.frames.is_empty() && !state.eof {
                state = queue
                    .condition
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
            }
            match state.frames.pop_front() {
                Some(frame) => frame,
                None => break,
            }
        };
        let result = executor.execute(
            coverage,
            &frame.kind,
            frame.sequence,
            frame.nonce.as_deref(),
            &frame.raw,
        );
        budget.release(frame.raw.len());
        match result {
            Ok(()) => {}
            Err(error) if busy(&error) => {
                // The current frame may already have durable metadata or a
                // started observer. Preserve that uncertainty and stop before
                // executing another frame with a potentially missing sequence.
                writer.send(
                    "REFUSED",
                    frame.sequence,
                    vec![
                        ("code".into(), string("busy-exhausted")),
                        ("outcome".into(), string("unknown")),
                        ("status".into(), string("pending")),
                    ],
                );
                return 1;
            }
            // Malformed metadata cannot become an arbitrary verified receipt.
            Err(_) => queue.lock().error = true,
        }
    }
    if thread.join().is_err() {
        return 1;
    }
    let (error, last) = {
        let state = queue.lock();
        (state.error, state.last.clone())
    };
    let Some(last) = last.filter(|_| !error) else {
        return 1;
    };
    if coverage.attest_frame().is_err() {
        return 1;
    }
    match metadata(timing, || coverage.close(&last)) {
        Ok(true) => 0,
        _ => 1,
    }
}

const STREAM_ARGS: [Arg; 6] = [
    Arg::Opt(Opt::store(&["--repo"], "repo").required()),
    Arg::Opt(Opt::store(&["--session"], "session").required()),
    Arg::Opt(Opt::store(&["--instance"], "instance").required()),
    Arg::Opt(Opt::store(&["--lifecycle"], "lifecycle").required()),
    Arg::Opt(Opt::store(&["--artifact"], "artifact").required()),
    Arg::Opt(Opt::store(&["--omp-agent-dir", "--agent-dir"], "omp_agent_dir").required()),
];
/// `am omp-boundary --stream` arguments (`--stream` already removed).
pub const STREAM_PARSER: Parser<'static> = Parser {
    description: Some(
        "Finite private omp FIFO. Coverage metadata is never source completion authority.",
    ),
    args: &STREAM_ARGS,
    exclusive: &[],
    subcommands: None,
};

fn failure(prog: &str, message: &str) -> CommandResult {
    CommandResult {
        value: None,
        exit_code: 1,
        stdout: String::new(),
        stderr: format!("{prog}: {message}\n"),
    }
}

/// The framed worker entry: frames go straight to this process's stdout.
pub fn main(argv: &[String], prog: &str, startup: &dyn StreamStartup) -> CommandResult {
    let matches = match parser::parse(&STREAM_PARSER, prog, argv) {
        Ok(matches) => matches,
        Err(exit) => {
            return CommandResult {
                value: None,
                exit_code: exit.code,
                stdout: exit.stdout,
                stderr: exit.stderr,
            };
        }
    };
    let text = |dest| matches.text(dest).unwrap_or_default().to_owned();
    let (repo, session, instance, lifecycle) = (
        text("repo"),
        text("session"),
        text("instance"),
        text("lifecycle"),
    );
    let receipt = match crate::omp_installation::observer_binding(
        &text("artifact"),
        &text("omp_agent_dir"),
    ) {
        Ok(receipt) => receipt,
        Err(error) => return failure(prog, &error.to_string()),
    };
    let runtime_root = match crate::cli::pack_root() {
        Ok(root) => root,
        Err(error) => return failure(prog, error.message()),
    };
    let stdout = match std::io::stdout().as_fd().try_clone_to_owned() {
        Ok(fd) => fd,
        Err(error) => return failure(prog, &error.to_string()),
    };
    let writer: Arc<dyn Responder> =
        Arc::new(ResponseWriter::start(Output::Descriptor(stdout), &instance));
    let startup_receipt = receipt.clone();
    let mut start = |repo: &Path, session: &str, instance: &str, lifecycle: &str| {
        startup.stream_startup(
            repo,
            session,
            instance,
            lifecycle,
            &startup_receipt,
            &runtime_root,
        )
    };
    let options = RunOptions {
        authority: Arc::new(Installed::new(&runtime_root)),
        receipt: Some(receipt),
        input: Box::new(std::io::stdin()),
        writer,
        clocks: ClockWindows::system,
        timing: Timing::system(),
    };
    match run(
        Path::new(&repo),
        &session,
        &instance,
        &lifecycle,
        &mut start,
        options,
    ) {
        Ok(code) => CommandResult {
            value: None,
            exit_code: code,
            stdout: String::new(),
            stderr: String::new(),
        },
        Err(error) => failure(prog, error.message()),
    }
}
