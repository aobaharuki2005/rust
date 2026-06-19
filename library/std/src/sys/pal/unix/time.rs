use core::num::niche_types::Nanoseconds;

use crate::time::Duration;
use crate::{fmt, io};

const NSEC_PER_SEC: u64 = 1_000_000_000;
pub const UNIX_EPOCH: SystemTime = SystemTime { t: Timespec::zero() };
#[allow(dead_code)] // Used for pthread condvar timeouts
pub const TIMESPEC_MAX: libc::timespec =
    libc::timespec { tv_sec: <libc::time_t>::MAX, tv_nsec: 1_000_000_000 - 1 };

// This additional constant is only used when calling
// `libc::pthread_cond_timedwait`.
#[cfg(target_os = "nto")]
pub(in crate::sys) const TIMESPEC_MAX_CAPPED: libc::timespec = libc::timespec {
    tv_sec: (u64::MAX / NSEC_PER_SEC) as i64,
    tv_nsec: (u64::MAX % NSEC_PER_SEC) as i64,
};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemTime {
    pub(crate) t: Timespec,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Timespec {
    tv_sec: i64,
    tv_nsec: Nanoseconds,
}

impl SystemTime {
    #[cfg_attr(any(target_os = "horizon", target_os = "hurd"), allow(unused))]
    pub fn new(tv_sec: i64, tv_nsec: i64) -> Result<SystemTime, io::Error> {
        Ok(SystemTime { t: Timespec::new(tv_sec, tv_nsec)? })
    }

    pub fn now() -> SystemTime {
        // On macOS, clock_gettime was not available before 10.12 Sierra.
        // Use gettimeofday for compatibility with macOS 10.7–10.11.
        #[cfg(target_os = "macos")]
        {
            use crate::ptr;
            let mut s = libc::timeval { tv_sec: 0, tv_usec: 0 };
            crate::sys::cvt(unsafe { libc::gettimeofday(&mut s, ptr::null_mut()) }).unwrap();
            SystemTime { t: Timespec::from_timeval(s) }
        }
        #[cfg(not(target_os = "macos"))]
        {
            SystemTime { t: Timespec::now(libc::CLOCK_REALTIME) }
        }
    }

    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        self.t.sub_timespec(&other.t)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime { t: self.t.checked_add_duration(other)? })
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime { t: self.t.checked_sub_duration(other)? })
    }
}

impl fmt::Debug for SystemTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemTime")
            .field("tv_sec", &self.t.tv_sec)
            .field("tv_nsec", &self.t.tv_nsec)
            .finish()
    }
}

impl Timespec {
    const unsafe fn new_unchecked(tv_sec: i64, tv_nsec: i64) -> Timespec {
        Timespec { tv_sec, tv_nsec: unsafe { Nanoseconds::new_unchecked(tv_nsec as u32) } }
    }

    pub const fn zero() -> Timespec {
        unsafe { Self::new_unchecked(0, 0) }
    }

    // Construct a Timespec from a POSIX timeval (microsecond resolution).
    // Used by the macOS SystemTime::now() path which calls gettimeofday.
    #[cfg(target_os = "macos")]
    fn from_timeval(t: libc::timeval) -> Timespec {
        // tv_usec is microseconds; multiply by 1000 to get nanoseconds.
        // The Apple-epoch-representation fixup in Timespec::new() applies here
        // too, so route through the checked constructor.
        Timespec::new(t.tv_sec as i64, 1_000 * t.tv_usec as i64)
            .expect("gettimeofday returned invalid timeval")
    }

    const fn new(tv_sec: i64, tv_nsec: i64) -> Result<Timespec, io::Error> {
        // On Apple OS, dates before epoch are represented differently than on other
        // Unix platforms: e.g. 1/10th of a second before epoch is represented as `seconds=-1`
        // and `nanoseconds=100_000_000` on other platforms, but is `seconds=0` and
        // `nanoseconds=-900_000_000` on Apple OS.
        //
        // To compensate, we first detect this special case by checking if both
        // seconds and nanoseconds are in range, and then correct the value for seconds
        // and nanoseconds to match the common unix representation.
        //
        // Please note that Apple OS nonetheless accepts the standard unix format when
        // setting file times, which makes this compensation round-trippable and generally
        // transparent.
        #[cfg(target_vendor = "apple")]
        let (tv_sec, tv_nsec) =
            if (tv_sec <= 0 && tv_sec > i64::MIN) && (tv_nsec < 0 && tv_nsec > -1_000_000_000) {
                (tv_sec - 1, tv_nsec + 1_000_000_000)
            } else {
                (tv_sec, tv_nsec)
            };
        if tv_nsec >= 0 && tv_nsec < NSEC_PER_SEC as i64 {
            Ok(unsafe { Self::new_unchecked(tv_sec, tv_nsec) })
        } else {
            Err(io::const_error!(io::ErrorKind::InvalidData, "invalid timestamp"))
        }
    }

    #[allow(dead_code)]
    pub fn now(clock: libc::clockid_t) -> Timespec {
        use crate::mem::MaybeUninit;
        use crate::sys::cvt;

        // Try to use 64-bit time in preparation for Y2038.
        #[cfg(all(
            target_os = "linux",
            target_env = "gnu",
            target_pointer_width = "32",
            not(target_arch = "riscv32")
        ))]
        {
            use crate::sys::weak::weak;

            // __clock_gettime64 was added to 32-bit arches in glibc 2.34,
            // and it handles both vDSO calls and ENOSYS fallbacks itself.
            weak!(
                fn __clock_gettime64(
                    clockid: libc::clockid_t,
                    tp: *mut __timespec64,
                ) -> libc::c_int;
            );

            if let Some(clock_gettime64) = __clock_gettime64.get() {
                let mut t = MaybeUninit::uninit();
                cvt(unsafe { clock_gettime64(clock, t.as_mut_ptr()) }).unwrap();
                let t = unsafe { t.assume_init() };
                return Timespec::new(t.tv_sec as i64, t.tv_nsec as i64).unwrap();
            }
        }

        let mut t = MaybeUninit::uninit();
        cvt(unsafe { libc::clock_gettime(clock, t.as_mut_ptr()) }).unwrap();
        let t = unsafe { t.assume_init() };
        Timespec::new(t.tv_sec as i64, t.tv_nsec as i64).unwrap()
    }

    pub fn sub_timespec(&self, other: &Timespec) -> Result<Duration, Duration> {
        if self >= other {
            // NOTE(eddyb) two aspects of this `if`-`else` are required for LLVM
            // to optimize it into a branchless form (see also #75545):
            //
            // 1. `self.tv_sec - other.tv_sec` shows up as a common expression
            //    in both branches, i.e. the `else` must have its `- 1`
            //    subtraction after the common one, not interleaved with it
            //    (it used to be `self.tv_sec - 1 - other.tv_sec`)
            //
            // 2. the `Duration::new` call (or any other additional complexity)
            //    is outside of the `if`-`else`, not duplicated in both branches
            //
            // Ideally this code could be rearranged such that it more
            // directly expresses the lower-cost behavior we want from it.
            let (secs, nsec) = if self.tv_nsec.as_inner() >= other.tv_nsec.as_inner() {
                (
                    (self.tv_sec - other.tv_sec) as u64,
                    self.tv_nsec.as_inner() - other.tv_nsec.as_inner(),
                )
            } else {
                (
                    (self.tv_sec - other.tv_sec - 1) as u64,
                    self.tv_nsec.as_inner() + (NSEC_PER_SEC as u32) - other.tv_nsec.as_inner(),
                )
            };

            Ok(Duration::new(secs, nsec))
        } else {
            match other.sub_timespec(self) {
                Ok(d) => Err(d),
                Err(d) => Ok(d),
            }
        }
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Timespec> {
        let mut secs = self.tv_sec.checked_add_unsigned(other.as_secs())?;

        // Nano calculations can't overflow because nanos are <1B which fit
        // in a u32.
        let mut nsec = other.subsec_nanos() + self.tv_nsec.as_inner();
        if nsec >= NSEC_PER_SEC as u32 {
            nsec -= NSEC_PER_SEC as u32;
            secs = secs.checked_add(1)?;
        }
        Some(unsafe { Timespec::new_unchecked(secs, nsec.into()) })
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Timespec> {
        let mut secs = self.tv_sec.checked_sub_unsigned(other.as_secs())?;

        // Similar to above, nanos can't overflow.
        let mut nsec = self.tv_nsec.as_inner() as i32 - other.subsec_nanos() as i32;
        if nsec < 0 {
            nsec += NSEC_PER_SEC as i32;
            secs = secs.checked_sub(1)?;
        }
        Some(unsafe { Timespec::new_unchecked(secs, nsec.into()) })
    }

    #[allow(dead_code)]
    pub fn to_timespec(&self) -> Option<libc::timespec> {
        Some(libc::timespec {
            tv_sec: self.tv_sec.try_into().ok()?,
            tv_nsec: self.tv_nsec.as_inner().try_into().ok()?,
        })
    }

    // On QNX Neutrino, the maximum timespec for e.g. pthread_cond_timedwait
    // is 2^64 nanoseconds
    #[cfg(target_os = "nto")]
    pub(in crate::sys) fn to_timespec_capped(&self) -> Option<libc::timespec> {
        // Check if timeout in nanoseconds would fit into an u64
        if (self.tv_nsec.as_inner() as u64)
            .checked_add((self.tv_sec as u64).checked_mul(NSEC_PER_SEC)?)
            .is_none()
        {
            return None;
        }
        self.to_timespec()
    }

    #[cfg(all(
        target_os = "linux",
        target_env = "gnu",
        target_pointer_width = "32",
        not(target_arch = "riscv32")
    ))]
    pub fn to_timespec64(&self) -> __timespec64 {
        __timespec64::new(self.tv_sec, self.tv_nsec.as_inner() as _)
    }
}

#[cfg(all(
    target_os = "linux",
    target_env = "gnu",
    target_pointer_width = "32",
    not(target_arch = "riscv32")
))]
#[repr(C)]
pub(crate) struct __timespec64 {
    pub(crate) tv_sec: i64,
    #[cfg(target_endian = "big")]
    _padding: i32,
    pub(crate) tv_nsec: i32,
    #[cfg(target_endian = "little")]
    _padding: i32,
}

#[cfg(all(
    target_os = "linux",
    target_env = "gnu",
    target_pointer_width = "32",
    not(target_arch = "riscv32")
))]
impl __timespec64 {
    pub(crate) fn new(tv_sec: i64, tv_nsec: i32) -> Self {
        Self { tv_sec, tv_nsec, _padding: 0 }
    }
}

// On macOS, clock_gettime was not available before 10.12 Sierra, so Instant
// is implemented using mach_absolute_time() — which has been available since
// 10.0 and gives monotonic, high-resolution tick counts that do not advance
// while the system is asleep (equivalent to CLOCK_UPTIME_RAW semantics).
//
// On all other platforms (including other Apple OS families: iOS, watchOS,
// tvOS, visionOS) the standard Timespec-based path is used, since those
// platforms either always had clock_gettime or do not need 10.7–10.11 compat.
#[cfg(target_os = "macos")]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    /// Raw mach timebase ticks from mach_absolute_time().
    t: u64,
}

#[cfg(not(target_os = "macos"))]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    t: Timespec,
}

// ---------------------------------------------------------------------------
// macOS Instant — mach_absolute_time() based
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos_instant {
    use crate::sync::atomic::{AtomicU64, Ordering};
    use crate::sys_common::mul_div_u64;
    use crate::time::Duration;

    use super::{Instant, NSEC_PER_SEC};

    /// Mach timebase: the kernel-provided rational to convert ticks → nanoseconds.
    /// Packed into a single u64 as `(denom << 32) | numer`; a zero value means
    /// "not yet initialised" (safe because denom == 0 is never a valid timebase).
    static INFO_BITS: AtomicU64 = AtomicU64::new(0);

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct MachTimebaseInfo {
        numer: u32,
        denom: u32,
    }

    extern "C" {
        fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut MachTimebaseInfo) -> libc::c_int;
    }

    #[inline]
    fn pack(info: MachTimebaseInfo) -> u64 {
        ((info.denom as u64) << 32) | (info.numer as u64)
    }

    #[inline]
    fn unpack(bits: u64) -> MachTimebaseInfo {
        MachTimebaseInfo { numer: bits as u32, denom: (bits >> 32) as u32 }
    }

    fn timebase() -> MachTimebaseInfo {
        let bits = INFO_BITS.load(Ordering::Relaxed);
        if bits != 0 {
            return unpack(bits);
        }
        let mut info = unpack(0);
        unsafe { mach_timebase_info(&mut info) };
        INFO_BITS.store(pack(info), Ordering::Relaxed);
        info
    }

    impl Instant {
        pub fn now() -> Instant {
            Instant { t: unsafe { mach_absolute_time() } }
        }

        pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
            let diff = self.t.checked_sub(other.t)?;
            let info = timebase();
            // Convert mach ticks to nanoseconds: ticks * numer / denom
            let nanos = mul_div_u64(diff, info.numer as u64, info.denom as u64);
            Some(Duration::new(nanos / NSEC_PER_SEC, (nanos % NSEC_PER_SEC) as u32))
        }

        pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
            Some(Instant { t: self.t.checked_add(dur_to_ticks(other)?)? })
        }

        pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
            Some(Instant { t: self.t.checked_sub(dur_to_ticks(other)?)? })
        }
    }

    impl core::fmt::Debug for Instant {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            // Surface the tick value directly; the public std::time::Instant
            // Debug impl wraps this with a since-epoch nanosecond conversion.
            f.debug_struct("Instant").field("t", &self.t).finish()
        }
    }

    /// Convert a Duration to mach ticks (inverse of the numer/denom conversion).
    fn dur_to_ticks(dur: &Duration) -> Option<u64> {
        let nanos = dur
            .as_secs()
            .checked_mul(NSEC_PER_SEC)?
            .checked_add(dur.subsec_nanos() as u64)?;
        let info = timebase();
        // ticks = nanos * denom / numer
        Some(mul_div_u64(nanos, info.denom as u64, info.numer as u64))
    }
}

// ---------------------------------------------------------------------------
// Non-macOS Instant — clock_gettime() based
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "macos"))]
impl Instant {
    #[cfg(target_vendor = "apple")]
    pub(crate) const CLOCK_ID: libc::clockid_t = libc::CLOCK_UPTIME_RAW;
    #[cfg(not(target_vendor = "apple"))]
    pub(crate) const CLOCK_ID: libc::clockid_t = libc::CLOCK_MONOTONIC;
    pub fn now() -> Instant {
        // https://www.manpagez.com/man/3/clock_gettime/
        //
        // CLOCK_UPTIME_RAW   clock that increments monotonically, in the same man-
        //                    ner as CLOCK_MONOTONIC_RAW, but that does not incre-
        //                    ment while the system is asleep.  The returned value
        //                    is identical to the result of mach_absolute_time()
        //                    after the appropriate mach_timebase conversion is
        //                    applied.
        //
        // Instant on macos was historically implemented using mach_absolute_time;
        // we preserve this value domain out of an abundance of caution.
        #[cfg(target_vendor = "apple")]
        const CLOCK_ID: libc::clockid_t = libc::CLOCK_UPTIME_RAW;
        #[cfg(not(target_vendor = "apple"))]
        const CLOCK_ID: libc::clockid_t = libc::CLOCK_MONOTONIC;
        Instant { t: Timespec::now(CLOCK_ID) }
    }

    pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
        self.t.sub_timespec(&other.t).ok()
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant { t: self.t.checked_add_duration(other)? })
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant { t: self.t.checked_sub_duration(other)? })
    }

    #[cfg_attr(
        not(target_os = "linux"),
        allow(unused, reason = "needed by the `sleep_until` on some unix platforms")
    )]
    pub(crate) fn into_timespec(self) -> Timespec {
        self.t
    }
}

#[cfg(not(target_os = "macos"))]
impl fmt::Debug for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instant")
            .field("tv_sec", &self.t.tv_sec)
            .field("tv_nsec", &self.t.tv_nsec)
            .finish()
    }
}
