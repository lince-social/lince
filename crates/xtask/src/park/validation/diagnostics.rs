use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};

fn read(path: impl AsRef<Path>) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut value = String::new();
    file.take(65537).read_to_string(&mut value).ok()?;
    (value.len() <= 65536).then(|| value.trim().to_owned())
}

pub(super) fn hardware() -> Value {
    let mut cards = Vec::new();
    let mut connectors = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.strip_prefix("card").is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())) {
                let driver = fs::canonicalize(path.join("device/driver")).ok().and_then(|driver| driver.file_name().map(|name| name.to_string_lossy().into_owned()));
                let version = driver.as_ref().and_then(|driver| read(Path::new("/sys/module").join(driver).join("version")));
                cards.push(json!({"name": name, "driver": driver, "driver_version": version,
                    "vendor": read(path.join("device/vendor")), "device": read(path.join("device/device")), "revision": read(path.join("device/revision"))}));
            } else if name.starts_with("card") && path.join("status").is_file() {
                connectors.push(json!({"name": name, "status": read(path.join("status")), "enabled": read(path.join("enabled")), "modes": read(path.join("modes"))}));
            }
        }
    }
    cards.sort_by_key(|card| card["name"].as_str().unwrap_or_default().to_owned());
    connectors.sort_by_key(|connector| connector["name"].as_str().unwrap_or_default().to_owned());
    json!({"kernel": read("/proc/sys/kernel/osrelease"), "cards": cards, "connectors": connectors,
        "scope": "Kernel/sysfs inventory; backend-selected device and applied modes are recorded in session.log. Missing values are unavailable, not a passed check."})
}
