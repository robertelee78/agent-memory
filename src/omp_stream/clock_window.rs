//! Purpose-limited callback clock calibration for optional wake guidance.
//! A faulty or absent calibration only revokes guidance; source capture never
//! depends on it.
use super::frame::Responder;
use super::{
    MAC_PROVISIONAL_RHO_NUMERATOR, MARGIN_NS, MAX_FRAMES, RATE_DENOMINATOR, err, exact, string,
    uuid,
};
use crate::codec::json::Value;
use crate::error::Result;
use crate::lifecycle::get;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

const STAMP_LIMIT: u128 = 1_000_000_000_000_000_000_000_000;
const MAX_CALIBRATION_NS: u128 = 950_000_000;

/// The platform whose clock relation a window may rely on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Darwin,
    Linux,
    Unsupported,
}
impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Darwin
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unsupported
        }
    }
}

/// One helper's background parent-image observation; never caller authority.
pub trait NativeImage: Send + Sync {
    fn observed(&self) -> bool;
    fn valid(&self) -> bool;
    fn close(&self);
}

/// Injected local monotonic nanoseconds (production: `clock::monotonic_ns`).
pub type Monotonic = Arc<dyn Fn() -> u64 + Send + Sync>;

/// `(0|[1-9][0-9]{0,23})` decimal JavaScript hrtime stamp.
pub(crate) fn stamp(value: &Value) -> Result<u128> {
    let text = value
        .as_str()
        .filter(|s| {
            *s == "0"
                || (s.len() <= 24
                    && s.bytes().next().is_some_and(|b| (b'1'..=b'9').contains(&b))
                    && s.bytes().all(|b| b.is_ascii_digit()))
        })
        .ok_or_else(|| err("Invalid monotonic clock timestamp"))?;
    text.parse()
        .map_err(|_| err("Invalid monotonic clock timestamp"))
}

struct Pending {
    nonce: String,
    j0: u128,
    deadline: u128,
    p1: u128,
    p2: u128,
    image_observed: bool,
}
struct State {
    pending: Option<Pending>,
    verified: Option<Arc<VerifiedNativeDeliveryWindow>>,
    ended: bool,
    used_nonces: BTreeSet<String>,
}
struct Owner {
    instance: String,
    eligible: Arc<dyn Fn() -> bool + Send + Sync>,
    platform: Platform,
    monotonic: Monotonic,
    native_image: Option<Arc<dyn NativeImage>>,
    state: Mutex<State>,
}
impl Owner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Purpose-limited, internally minted calibration; never JSON authority.
/// No public constructor, Clone or serialization exists.
///
/// ```compile_fail
/// let _ = agent_memory::omp_stream::VerifiedNativeDeliveryWindow { };
/// ```
pub struct VerifiedNativeDeliveryWindow {
    lower: i128,
    j3: u128,
    j_deadline: u128,
    nonce: String,
    instance: String,
    platform: Platform,
    owner: Weak<Owner>,
    cancelled: AtomicBool,
    reserving: AtomicBool,
}
impl VerifiedNativeDeliveryWindow {
    /// `L`: the lower bound of local minus callback clock offset.
    pub fn lower_offset(&self) -> i128 {
        self.lower
    }
    pub fn j3(&self) -> u128 {
        self.j3
    }
    pub fn j_deadline(&self) -> u128 {
        self.j_deadline
    }
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
    pub fn support(&self) -> &'static str {
        if self.platform == Platform::Darwin {
            "provisional-unqualified"
        } else {
            "unqualified"
        }
    }
    /// Spend this window once, only while it is the owner's live calibration
    /// and the local deadline (minus rate allowance and margin) is ahead.
    pub fn reserve(self: &Arc<Self>) -> bool {
        let Some(owner) = self.owner.upgrade() else {
            return false;
        };
        let state = owner.lock();
        let current = state
            .verified
            .as_ref()
            .is_some_and(|verified| Arc::ptr_eq(verified, self));
        if !current
            || owner.instance != self.instance
            || state.ended
            || self.cancelled.load(Ordering::SeqCst)
            || self.reserving.load(Ordering::SeqCst)
            || !(owner.eligible)()
        {
            return false;
        }
        let Some(remaining) = self.j_deadline.checked_sub(self.j3) else {
            return false;
        };
        let drift = match self.platform {
            // Ceiling subtraction keeps integer arithmetic conservative: exact R3 D.
            Platform::Darwin => {
                (MAC_PROVISIONAL_RHO_NUMERATOR * remaining + RATE_DENOMINATOR - 1)
                    / RATE_DENOMINATOR
            }
            // Both qualified clocks use integer CLOCK_MONOTONIC, so E = 0.
            Platform::Linux
                if owner
                    .native_image
                    .as_ref()
                    .is_some_and(|image| image.valid()) =>
            {
                0
            }
            _ => return false,
        };
        let (Ok(j_deadline), Ok(drift)) = (i128::try_from(self.j_deadline), i128::try_from(drift))
        else {
            return false;
        };
        let deadline = j_deadline + self.lower - drift;
        let now = u128::from((owner.monotonic)());
        if now >= STAMP_LIMIT {
            return false;
        }
        let Ok(now) = i128::try_from(now) else {
            return false;
        };
        if now + MARGIN_NS as i128 >= deadline {
            return false;
        }
        self.reserving.store(true, Ordering::SeqCst);
        true
    }
}

/// Per-instance callback clock calibration state.
#[derive(Clone)]
pub struct ClockWindows {
    owner: Arc<Owner>,
}
impl ClockWindows {
    pub fn new(
        instance: &str,
        eligible: Arc<dyn Fn() -> bool + Send + Sync>,
        platform: Platform,
        monotonic: Monotonic,
        native_image: Option<Arc<dyn NativeImage>>,
    ) -> Self {
        Self {
            owner: Arc::new(Owner {
                instance: instance.to_owned(),
                eligible,
                platform,
                monotonic,
                native_image,
                state: Mutex::new(State {
                    pending: None,
                    verified: None,
                    ended: false,
                    used_nonces: BTreeSet::new(),
                }),
            }),
        }
    }
    /// The running process's platform clock, with the Linux parent-image
    /// observation started in the background on Linux.
    pub fn system(instance: &str, eligible: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        let platform = Platform::current();
        let image = super::linux_image::system_image(platform);
        Self::new(
            instance,
            eligible,
            platform,
            Arc::new(crate::clock::monotonic_ns),
            image,
        )
    }
    pub fn platform(&self) -> Platform {
        self.owner.platform
    }
    pub fn instance(&self) -> &str {
        &self.owner.instance
    }
    pub fn control(&self, kind: &str, value: &Value, writer: &dyn Responder) -> Result<()> {
        let owner = &self.owner;
        let mut state = owner.lock();
        match kind {
            "CLOCK" => {
                let image_observed = owner
                    .native_image
                    .as_ref()
                    .is_some_and(|image| image.observed());
                let p1 = u128::from((owner.monotonic)());
                exact(value, &["nonce", "j0", "j_deadline"])?;
                let nonce = uuid(get(value, "nonce"))?.to_owned();
                let j0 = stamp(get(value, "j0"))?;
                let deadline = stamp(get(value, "j_deadline"))?;
                if deadline < j0 || deadline - j0 > MAX_CALIBRATION_NS || state.ended {
                    return Err(err("Invalid callback clock interval"));
                }
                if state.used_nonces.contains(&nonce) || state.used_nonces.len() >= MAX_FRAMES {
                    return Err(err(
                        "Callback clock nonce was already used or capacity reached",
                    ));
                }
                state.used_nonces.insert(nonce.clone());
                if let Some(verified) = state.verified.take() {
                    verified.cancelled.store(true, Ordering::SeqCst);
                }
                let p2 = u128::from((owner.monotonic)());
                if !(p1 <= p2 && p2 < STAMP_LIMIT) {
                    return Err(err("Local callback clock regressed"));
                }
                state.pending = Some(Pending {
                    nonce: nonce.clone(),
                    j0,
                    deadline,
                    p1,
                    p2,
                    image_observed,
                });
                writer.send(
                    "CLOCK-REPLY",
                    0,
                    vec![
                        ("nonce".into(), string(nonce)),
                        ("p1".into(), string(p1.to_string())),
                        ("p2".into(), string(p2.to_string())),
                    ],
                );
            }
            "CLOCK-ACK" => {
                exact(value, &["nonce", "j0", "j3"])?;
                let nonce = uuid(get(value, "nonce"))?.to_owned();
                let j0 = stamp(get(value, "j0"))?;
                let j3 = stamp(get(value, "j3"))?;
                let pending = state.pending.take();
                let Some(pending) =
                    pending.filter(|p| p.nonce == nonce && p.j0 == j0 && j3 >= j0 && !state.ended)
                else {
                    return Err(err("Unmatched callback clock acknowledgement"));
                };
                let (Ok(p1), Ok(p2), Ok(j0), Ok(j3_signed)) = (
                    i128::try_from(pending.p1),
                    i128::try_from(pending.p2),
                    i128::try_from(j0),
                    i128::try_from(j3),
                ) else {
                    return Err(err("Contradictory callback clock calibration"));
                };
                let lower = p2 - j3_signed;
                let upper = p1 - j0;
                if lower > upper {
                    return Err(err("Contradictory callback clock calibration"));
                }
                if j3 <= pending.deadline
                    && (owner.platform != Platform::Linux || pending.image_observed)
                {
                    state.verified = Some(Arc::new(VerifiedNativeDeliveryWindow {
                        lower,
                        j3,
                        j_deadline: pending.deadline,
                        nonce,
                        instance: owner.instance.clone(),
                        platform: owner.platform,
                        owner: Arc::downgrade(owner),
                        cancelled: AtomicBool::new(false),
                        reserving: AtomicBool::new(false),
                    }));
                }
            }
            _ => {
                exact(value, &["nonce"])?;
                let nonce = uuid(get(value, "nonce"))?.to_owned();
                if !state.used_nonces.contains(&nonce) {
                    if state.used_nonces.len() >= MAX_FRAMES {
                        return Err(err("Callback clock nonce capacity reached"));
                    }
                    state.used_nonces.insert(nonce.clone());
                }
                if state.pending.as_ref().is_some_and(|p| p.nonce == nonce) {
                    state.pending = None;
                }
                if let Some(verified) = state.verified.as_ref().filter(|v| v.nonce == nonce) {
                    verified.cancelled.store(true, Ordering::SeqCst);
                }
            }
        }
        Ok(())
    }
    pub fn window(&self, nonce: &str) -> Option<Arc<VerifiedNativeDeliveryWindow>> {
        let state = self.owner.lock();
        state
            .verified
            .as_ref()
            .filter(|v| v.nonce == nonce && !state.ended)
            .cloned()
    }
    pub fn revoke(&self) {
        let mut state = self.owner.lock();
        state.pending = None;
        if let Some(verified) = state.verified.as_ref() {
            verified.cancelled.store(true, Ordering::SeqCst);
        }
    }
    pub fn end(&self) {
        {
            let mut state = self.owner.lock();
            state.ended = true;
            state.pending = None;
            if let Some(verified) = state.verified.as_ref() {
                verified.cancelled.store(true, Ordering::SeqCst);
            }
        }
        if let Some(image) = self.owner.native_image.as_ref() {
            image.close();
        }
    }
}

/// One nonce-bound lookup, never a calibration or caller-supplied deadline.
pub struct LateNativeDeliveryWindow {
    owner: ClockWindows,
    nonce: String,
    state: Mutex<LateState>,
}
struct LateState {
    live: bool,
    resolved: bool,
    reservation_started: bool,
}
impl LateNativeDeliveryWindow {
    pub(crate) fn new(owner: ClockWindows, nonce: &str) -> Result<Self> {
        super::uuid_text(nonce)?;
        Ok(Self {
            owner,
            nonce: nonce.to_owned(),
            state: Mutex::new(LateState {
                live: true,
                resolved: false,
                reservation_started: false,
            }),
        })
    }
    pub fn reserve(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.live || state.resolved {
            return false;
        }
        state.resolved = true;
        state.reservation_started = self
            .owner
            .window(&self.nonce)
            .is_some_and(|window| window.reserve());
        state.reservation_started
    }
    pub fn reservation_started(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reservation_started
    }
    pub fn close(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).live = false;
    }
}

/// The window handed to an observer: a late nonce lookup for an armed live
/// finite seal, or the currently verified calibration for other nonce frames.
pub enum DeliveryWindow<'a> {
    Late(&'a LateNativeDeliveryWindow),
    Verified(Arc<VerifiedNativeDeliveryWindow>),
}
impl DeliveryWindow<'_> {
    pub fn reserve(&self) -> bool {
        match self {
            Self::Late(window) => window.reserve(),
            Self::Verified(window) => window.reserve(),
        }
    }
}
