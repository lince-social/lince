pub fn size(bytes: u64) -> String {
    for (unit, divisor) in [
        ("TiB", 1u64 << 40),
        ("GiB", 1 << 30),
        ("MiB", 1 << 20),
        ("KiB", 1 << 10),
    ] {
        if bytes >= divisor {
            return format!("{:.1} {unit}", bytes as f64 / divisor as f64);
        }
    }
    format!("{bytes} B")
}

pub fn state(state: &str, incoming: bool) -> &'static str {
    match (state, incoming) {
        ("offered", true) => "Waiting for your acceptance",
        ("offered", false) => "Waiting for acceptance",
        ("accepted", true) => "Receiving",
        ("accepted", false) => "Sending",
        ("completed", _) => "Completed",
        ("declined", _) => "Declined",
        ("cancelled", _) => "Cancelled",
        _ => "Unavailable",
    }
}
