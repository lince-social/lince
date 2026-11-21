#[cfg(unix)]
pub fn process_cpu_us() -> Option<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a writable timespec kept alive for the duration of the call.
    if unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) } != 0 {
        return None;
    }
    u64::try_from(time.tv_sec)
        .ok()?
        .checked_mul(1_000_000)?
        .checked_add(u64::try_from(time.tv_nsec).ok()? / 1000)
}

#[cfg(not(unix))]
pub fn process_cpu_us() -> Option<u64> {
    None
}
