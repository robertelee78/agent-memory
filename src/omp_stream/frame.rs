//! Byte-exact `AM249` framing, the bounded retained-input budget and the
//! independent bounded response writer.
use super::{
    CONTROL_TYPES, MAX_FRAME, MAX_FRAMES, MAX_HEADER, MAX_RESPONSE, MAX_RESPONSES, MAX_RETAINED,
    READ_CHUNK, SOURCE_TYPES, check_counter, err, io, string, uuid_text,
};
use crate::codec::json::Value;
use crate::error::Result;
use crate::turn_sources;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// `AM249 <kind> <sequence> <length> <nonce|->\n` followed by sorted compact JSON.
pub fn encode_frame(
    kind: &str,
    sequence: u64,
    value: &Value,
    nonce: Option<&str>,
) -> Result<Vec<u8>> {
    let raw = turn_sources::encoded(value)?;
    if raw.len() > MAX_FRAME {
        return Err(err("Finite stream frame exceeds bound"));
    }
    check_counter(sequence)?;
    if let Some(nonce) = nonce {
        uuid_text(nonce)?;
    }
    let header = format!(
        "AM249 {kind} {sequence} {} {}\n",
        raw.len(),
        nonce.unwrap_or("-")
    );
    if !header.is_ascii() || header.len() > MAX_HEADER {
        return Err(err("Finite stream header exceeds bound"));
    }
    let mut frame = header.into_bytes();
    frame.extend_from_slice(&raw);
    Ok(frame)
}

/// Retained source bytes and frames: queued, executing and partial bodies.
pub struct Budget {
    state: Mutex<(usize, usize)>,
    max_frames: usize,
    max_bytes: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}
impl Budget {
    pub fn new() -> Self {
        Self::with_limits(MAX_FRAMES, MAX_RETAINED)
    }
    pub fn with_limits(max_frames: usize, max_bytes: usize) -> Self {
        Self {
            state: Mutex::new((0, 0)),
            max_frames,
            max_bytes,
        }
    }
    pub fn reserve(&self, count: usize) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let (frames, bytes) = *state;
        if frames >= self.max_frames || bytes.saturating_add(count) > self.max_bytes {
            return Err(err("Finite stream accepted queue exceeds bound"));
        }
        *state = (frames + 1, bytes + count);
        Ok(())
    }
    pub fn release(&self, count: usize) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *state = (state.0.saturating_sub(1), state.1.saturating_sub(count));
    }
    /// `(frames, bytes)` currently reserved.
    pub fn usage(&self) -> (usize, usize) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// One complete frame. Source bodies remain reserved in the budget until the
/// executor (or the reader's refusal path) releases them.
#[derive(Debug)]
pub struct Frame {
    pub kind: String,
    pub sequence: u64,
    pub nonce: Option<String>,
    pub raw: Vec<u8>,
}

fn decimal(text: &str, max_digits: usize) -> Option<u64> {
    let canonical = text == "0"
        || (!text.is_empty()
            && text.len() <= max_digits
            && text
                .bytes()
                .next()
                .is_some_and(|b| (b'1'..=b'9').contains(&b))
            && text.bytes().all(|b| b.is_ascii_digit()));
    if !canonical {
        return None;
    }
    text.parse().ok()
}
/// Full match of the header grammar; `None` for any other byte string.
fn parse_header(header: &[u8]) -> Option<(&str, u64, usize, Option<&str>)> {
    let text = std::str::from_utf8(header).ok()?.strip_suffix('\n')?;
    let rest = text.strip_prefix("AM249 ")?;
    let mut parts = rest.split(' ');
    let kind = parts.next()?;
    let sequence = parts.next()?;
    let size = parts.next()?;
    let nonce = parts.next()?;
    if parts.next().is_some()
        || kind.is_empty()
        || !kind.bytes().all(|b| b.is_ascii_uppercase() || b == b'-')
    {
        return None;
    }
    let sequence = decimal(sequence, 16)?;
    let size = usize::try_from(decimal(size, 9)?).ok()?;
    let nonce = if nonce == "-" {
        None
    } else if nonce.len() == 36
        && nonce
            .bytes()
            .all(|b| b == b'-' || b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Some(nonce)
    } else {
        return None;
    };
    Some((kind, sequence, size, nonce))
}

/// One bounded partial body; its reservation includes queued and executing bytes.
pub struct FrameReader<R: Read> {
    stream: R,
    budget: Arc<Budget>,
}
impl<R: Read> FrameReader<R> {
    pub fn new(stream: R, budget: Arc<Budget>) -> Self {
        Self { stream, budget }
    }
    fn read_some(&mut self, buffer: &mut [u8]) -> Result<usize> {
        loop {
            match self.stream.read(buffer) {
                Ok(count) => return Ok(count),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(io(error)),
            }
        }
    }
    pub fn read(&mut self) -> Result<Option<Frame>> {
        let mut header = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            if self.read_some(&mut byte)? == 0 {
                if !header.is_empty() {
                    return Err(err("Partial finite stream header"));
                }
                return Ok(None);
            }
            let [value] = byte;
            header.push(value);
            if header.len() > MAX_HEADER {
                return Err(err("Finite stream header exceeds bound"));
            }
            if value == b'\n' {
                break;
            }
        }
        let (kind, sequence, size, nonce) =
            parse_header(&header).ok_or_else(|| err("Invalid finite stream header"))?;
        let nonce = nonce.map(uuid_text).transpose()?.map(str::to_owned);
        let control = CONTROL_TYPES.contains(&kind);
        if !(control || SOURCE_TYPES.contains(&kind)) || size > MAX_FRAME {
            return Err(err("Unknown or oversized finite stream frame"));
        }
        if control && (sequence != 0 || nonce.is_some() || size > MAX_RESPONSE) {
            return Err(err("Invalid finite stream control frame"));
        }
        let kind = kind.to_owned();
        // Controls have their own one-frame 64KiB bound; a full source FIFO
        // must not prevent CANCEL from revoking a not-yet-started reservation.
        let reserved = !control;
        if reserved {
            self.budget.reserve(size)?;
        }
        match self.body(size) {
            Ok(raw) => Ok(Some(Frame {
                kind,
                sequence,
                nonce,
                raw,
            })),
            Err(error) => {
                if reserved {
                    self.budget.release(size);
                }
                Err(error)
            }
        }
    }
    fn body(&mut self, size: usize) -> Result<Vec<u8>> {
        let mut raw = Vec::with_capacity(size.min(READ_CHUNK));
        let mut chunk = vec![0u8; READ_CHUNK.min(size.max(1))];
        while raw.len() < size {
            let want = READ_CHUNK.min(size - raw.len());
            let Some(buffer) = chunk.get_mut(..want) else {
                return Err(err("Partial finite stream body"));
            };
            let count = self.read_some(buffer)?;
            if count == 0 {
                return Err(err("Partial finite stream body"));
            }
            raw.extend(buffer.iter().take(count));
        }
        Ok(raw)
    }
}

/// Result of offering one response frame to the bounded writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// The original frame was queued.
    Queued,
    /// A fixed notice replaced an unencodable or oversized frame.
    Substituted,
    /// Nothing was queued (dead or finishing recipient, or pressure).
    Dropped,
}

/// Response sink of the finite executor and clock control.
pub trait Responder: Send + Sync {
    fn send(&self, kind: &str, sequence: u64, value: Vec<(String, Value)>) -> Sent;
    fn live(&self) -> bool;
    /// A finite output grace; never a source execution timeout or helper kill.
    fn finish(&self);
}

/// Where response frames go: a descriptor written nonblocking, or an
/// in-process stream written as is (the baseline `fileno()`-less branch).
pub enum Output {
    Descriptor(OwnedFd),
    Stream(Box<dyn Write + Send>),
}

struct WriterState {
    queue: VecDeque<(String, Vec<u8>)>,
    live: bool,
    finishing: bool,
    inflight: bool,
    loss_pending: bool,
    exited: bool,
}
struct Shared {
    instance_id: String,
    state: Mutex<WriterState>,
    condition: Condvar,
}
impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, WriterState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn frame(&self, kind: &str, sequence: u64, value: Vec<(String, Value)>) -> Result<Vec<u8>> {
        let mut pairs = vec![("instance_id".to_owned(), string(&self.instance_id))];
        pairs.extend(value);
        encode_frame(kind, sequence, &Value::Object(pairs), None)
    }
    fn notice(&self, kind: &str, sequence: u64, code: &str) -> Result<Vec<u8>> {
        self.frame(
            kind,
            sequence,
            vec![
                ("code".into(), string(code)),
                ("outcome".into(), string("unknown")),
                ("status".into(), string("pending")),
            ],
        )
    }
    fn loss_notice(&self, state: &mut WriterState) {
        if state.loss_pending && state.queue.len() + usize::from(state.inflight) < MAX_RESPONSES {
            if let Ok(raw) = self.notice("REFUSED", 0, "response-pressure") {
                state.queue.push_back(("REFUSED".into(), raw));
            }
            state.loss_pending = false;
        }
    }
}

/// Independent bounded writer: a blocked recipient cannot hold source locks.
pub struct ResponseWriter {
    shared: Arc<Shared>,
}
impl ResponseWriter {
    /// Start the writer thread on `output`.
    pub fn start(output: Output, instance_id: &str) -> Self {
        let writer = Self::unstarted(instance_id, Vec::new());
        let shared = Arc::clone(&writer.shared);
        let spawned = std::thread::Builder::new()
            .name("omp-stream-response".into())
            .spawn(move || write_loop(&shared, output));
        if spawned.is_err() {
            let mut state = writer.shared.lock();
            state.live = false;
            state.exited = true;
        }
        writer
    }
    /// A writer without a delivery thread, holding `queue`; its frames are
    /// observable through `queued` (the baseline contracts' in-process writer).
    pub fn unstarted(instance_id: &str, queue: Vec<(String, Vec<u8>)>) -> Self {
        Self {
            shared: Arc::new(Shared {
                instance_id: instance_id.to_owned(),
                state: Mutex::new(WriterState {
                    queue: queue.into(),
                    live: true,
                    finishing: false,
                    inflight: false,
                    loss_pending: false,
                    exited: false,
                }),
                condition: Condvar::new(),
            }),
        }
    }
    /// Snapshot of not-yet-written frames.
    pub fn queued(&self) -> Vec<(String, Vec<u8>)> {
        self.shared.lock().queue.iter().cloned().collect()
    }
    /// Remove the oldest queued frame (an unstarted writer's delivery).
    pub fn pop_front(&self) -> Option<(String, Vec<u8>)> {
        let mut state = self.shared.lock();
        let value = state.queue.pop_front();
        self.shared.loss_notice(&mut state);
        self.shared.condition.notify_all();
        value
    }
    /// Remove every queued frame without a pressure notice.
    pub fn clear(&self) {
        self.shared.lock().queue.clear();
    }
    pub fn loss_pending(&self) -> bool {
        self.shared.lock().loss_pending
    }
}
impl Responder for ResponseWriter {
    /// Queue the original frame, a fixed notice, or nothing.
    fn send(&self, kind: &str, sequence: u64, value: Vec<(String, Value)>) -> Sent {
        let mut state = self.shared.lock();
        if !state.live || state.finishing {
            return Sent::Dropped;
        }
        if state.queue.len() + usize::from(state.inflight) >= MAX_RESPONSES {
            if let Some(index) = state.queue.iter().position(|(k, _)| k == "RECEIVED") {
                state.queue.remove(index);
            } else if kind == "RECEIVED" {
                return Sent::Dropped;
            } else {
                state.loss_pending = true;
                self.shared.condition.notify_all();
                return Sent::Dropped;
            }
        }
        let raw = self
            .shared
            .frame(kind, sequence, value)
            .ok()
            .filter(|raw| raw.len() <= MAX_RESPONSE);
        let (kind, raw, sent) = match raw {
            Some(raw) => (kind.to_owned(), raw, Sent::Queued),
            None => {
                if kind == "RECEIVED" {
                    return Sent::Dropped;
                }
                let (kind, code) = if kind == "OBSERVED" {
                    ("OBSERVED", "result-oversized")
                } else {
                    ("REFUSED", "response-unavailable")
                };
                let Ok(raw) = self.shared.notice(kind, sequence, code) else {
                    return Sent::Dropped;
                };
                (kind.to_owned(), raw, Sent::Substituted)
            }
        };
        state.queue.push_back((kind, raw));
        self.shared.condition.notify_all();
        sent
    }
    fn live(&self) -> bool {
        self.shared.lock().live
    }
    fn finish(&self) {
        {
            let mut state = self.shared.lock();
            state.finishing = true;
            self.shared.condition.notify_all();
        }
        self.wait_exit(Duration::from_millis(100));
        {
            let mut state = self.shared.lock();
            state.live = false;
            state.queue.clear();
            self.shared.condition.notify_all();
        }
        self.wait_exit(Duration::from_millis(100));
    }
}
impl ResponseWriter {
    fn wait_exit(&self, grace: Duration) {
        let state = self.shared.lock();
        let _ = self
            .shared
            .condition
            .wait_timeout_while(state, grace, |s| !s.exited);
    }
}

fn write_loop(shared: &Shared, mut output: Output) {
    let failed = match &output {
        Output::Descriptor(fd) => crate::process::set_nonblocking(fd.as_fd()).is_err(),
        Output::Stream(_) => false,
    };
    if failed || deliver(shared, &mut output).is_err() {
        let mut state = shared.lock();
        state.live = false;
        state.queue.clear();
    }
    let mut state = shared.lock();
    state.exited = true;
    shared.condition.notify_all();
}

fn deliver(shared: &Shared, output: &mut Output) -> std::io::Result<()> {
    loop {
        let raw = {
            let mut state = shared.lock();
            while state.live && state.queue.is_empty() && !state.loss_pending && !state.finishing {
                state = shared
                    .condition
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
            }
            shared.loss_notice(&mut state);
            if !state.live || (state.queue.is_empty() && state.finishing) {
                return Ok(());
            }
            let Some((_, raw)) = state.queue.pop_front() else {
                continue;
            };
            state.inflight = true;
            raw
        };
        let mut offset = 0;
        while offset < raw.len() {
            if !shared.lock().live {
                return Ok(());
            }
            let rest = raw.get(offset..).unwrap_or_default();
            match output {
                Output::Stream(stream) => {
                    stream.write_all(rest)?;
                    offset = raw.len();
                }
                Output::Descriptor(fd) => match crate::process::write_some(fd.as_fd(), rest) {
                    Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
                    Ok(count) => offset += count,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        crate::process::wait_writable(fd.as_fd(), super::BUSY_BACKOFF)?;
                    }
                    Err(error) => return Err(error),
                },
            }
        }
        if let Output::Stream(stream) = output {
            stream.flush()?;
        }
        let mut state = shared.lock();
        state.inflight = false;
        shared.loss_notice(&mut state);
        shared.condition.notify_all();
    }
}
