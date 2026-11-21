#[cfg(target_os = "linux")]
pub(super) fn process_reads() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/self/io").ok()?;
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name == key).then_some(value)?.trim().parse().ok()
        })
    };
    Some((value("rchar")?, value("syscr")?))
}

#[cfg(not(target_os = "linux"))]
pub(super) fn process_reads() -> Option<(u64, u64)> {
    None
}
