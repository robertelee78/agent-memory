use crate::error::{ContractError, Result};
use std::time::Duration;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("agent-memory requires the Linux or macOS monotonic clock contract");

#[cfg(target_os = "linux")]
pub fn monotonic_ns() -> u64 {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let code = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut value) };
    assert_eq!(code, 0, "CLOCK_MONOTONIC unavailable");
    assert!(
        value.tv_sec >= 0 && (0..1_000_000_000).contains(&value.tv_nsec),
        "Invalid monotonic clock value"
    );
    let ns = value.tv_sec as u128 * 1_000_000_000 + value.tv_nsec as u128;
    u64::try_from(ns).expect("Monotonic nanoseconds exceed u64")
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
pub fn monotonic_ns() -> u64 {
    static TIMEBASE: std::sync::LazyLock<(u32, u32)> = std::sync::LazyLock::new(|| {
        let mut info = libc::mach_timebase_info { numer: 0, denom: 0 };
        let code = unsafe { libc::mach_timebase_info(&mut info) };
        assert_eq!(code, 0, "mach_timebase_info unavailable");
        assert!(info.numer != 0 && info.denom != 0, "Invalid Mach timebase");
        (info.numer, info.denom)
    });
    let &(numer, denom) = &*TIMEBASE;
    // mach_absolute_time, unlike Darwin CLOCK_MONOTONIC, excludes system sleep.
    let ticks = unsafe { libc::mach_absolute_time() };
    let ns = u128::from(ticks) * u128::from(numer) / u128::from(denom);
    u64::try_from(ns).expect("Monotonic nanoseconds exceed u64")
}

pub fn clock_identity() -> &'static str {
    let _ = monotonic_ns();
    #[cfg(target_os = "linux")]
    {
        "linux:clock_gettime(CLOCK_MONOTONIC)"
    }
    #[cfg(target_os = "macos")]
    {
        "macos:mach_absolute_time"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Deadline(u64);

impl Deadline {
    pub const fn from_absolute_ns(ns: u64) -> Self {
        Self(ns)
    }

    pub const fn as_ns(self) -> u64 {
        self.0
    }

    pub fn from_seconds(seconds: f64) -> Result<Self> {
        let duration = Duration::try_from_secs_f64(seconds).map_err(|_| {
            ContractError::new("Deadline seconds are negative, nonfinite or overflowing")
        })?;
        Self::after(duration)
    }

    pub fn after(duration: Duration) -> Result<Self> {
        let ns = u64::try_from(duration.as_nanos())
            .map_err(|_| ContractError::new("Deadline duration overflows monotonic nanoseconds"))?;
        let end = monotonic_ns()
            .checked_add(ns)
            .ok_or_else(|| ContractError::new("Deadline overflows monotonic nanoseconds"))?;
        Ok(Self(end))
    }

    pub fn remaining(self) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(monotonic_ns()))
    }

    pub fn expired(self) -> bool {
        monotonic_ns() >= self.0
    }
}
