use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{Result, checked};

const PACKAGE: &str = "social.lince.mobile.debug";
const COMPONENT: &str = "social.lince.mobile.debug/social.lince.mobile.MainActivity";

#[derive(Debug)]
struct Options {
    action: String,
    sdk: Option<PathBuf>,
    avd: String,
    apk: Option<PathBuf>,
    device: Option<String>,
    headless: bool,
}

impl Options {
    fn parse(args: &[OsString]) -> Result<Self> {
        let mut options = Self {
            action: "run".into(),
            sdk: None,
            avd: "lince-api35".into(),
            apk: None,
            device: None,
            headless: false,
        };
        let mut args = args.iter().peekable();
        if args
            .peek()
            .is_some_and(|arg| !arg.to_string_lossy().starts_with('-'))
        {
            options.action = args.next().unwrap().to_string_lossy().into_owned();
        }
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h") => options.action = "help".into(),
                Some("--headless") => options.headless = true,
                Some("--sdk") => options.sdk = Some(PathBuf::from(value(&mut args, "--sdk")?)),
                Some("--apk") => options.apk = Some(PathBuf::from(value(&mut args, "--apk")?)),
                Some("--avd") => options.avd = identifier(value(&mut args, "--avd")?, "AVD name")?,
                Some("--device") => {
                    options.device =
                        Some(identifier(value(&mut args, "--device")?, "device serial")?)
                }
                _ => return Err(format!("unknown Android option: {}", arg.to_string_lossy())),
            }
        }
        if !matches!(options.action.as_str(), "setup" | "run" | "logs" | "help") {
            return Err("usage: cargo xtask android [setup|run|logs|help] [options]".into());
        }
        if options.action != "run" && (options.apk.is_some() || options.headless) {
            return Err("--apk and --headless are only available when running Android".into());
        }
        if options.action == "setup" && options.device.is_some() {
            return Err("setup creates an emulator, so it does not accept --device".into());
        }
        Ok(options)
    }
}

fn value<'a>(args: &mut impl Iterator<Item = &'a OsString>, flag: &str) -> Result<&'a OsString> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn identifier(value: &OsString, kind: &str) -> Result<String> {
    let value = value.to_str().ok_or_else(|| format!("invalid {kind}"))?;
    if value.is_empty()
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
    {
        return Err(format!("invalid {kind}: {value}"));
    }
    Ok(value.into())
}

pub(crate) fn dispatch(root: &Path, args: &[OsString]) -> Result<()> {
    let options = Options::parse(args)?;
    if options.action == "help" {
        println!("cargo xtask android setup [--sdk PATH] [--avd NAME]");
        println!("cargo xtask android [run] [--sdk PATH] [--avd NAME] [--apk PATH] [--headless]");
        println!("cargo xtask android run --device SERIAL [--apk PATH] [--sdk PATH]");
        println!("cargo xtask android logs [--device SERIAL] [--avd NAME] [--sdk PATH]");
        println!(
            "Requires Android Command-line Tools, a JDK for setup, and hardware virtualization."
        );
        println!(
            "SDK discovery: --sdk, ANDROID_HOME, ANDROID_SDK_ROOT, or the Android Studio default."
        );
        println!(
            "setup downloads the emulator and Android 35 Google APIs image for this computer."
        );
        println!(
            "run builds nothing. It opens the emulator and optionally installs an existing debug APK."
        );
        println!(
            "If Lince is installed, run restarts it and saves ten seconds of startup logs under target/android."
        );
        println!(
            "logs streams logs from the chosen emulator or explicitly selected phone. Ctrl+C stops logging."
        );
        println!(
            "On Linux enable KVM access; on NixOS enter nix develop .#android for the JDK and emulator libraries."
        );
        return Ok(());
    }
    let sdk = sdk_path(&options)?;
    if options.action == "setup" {
        return setup(&sdk, &options.avd);
    }
    let apk = options
        .apk
        .as_ref()
        .map(|path| {
            path.canonicalize()
                .map_err(|e| format!("cannot open APK {}: {e}", path.display()))
        })
        .transpose()?;
    let adb = tool(&sdk, "platform-tools/adb")?;
    checked(Command::new(&adb).arg("start-server"))?;
    let target = root
        .join(env::var_os("CARGO_TARGET_DIR").unwrap_or_else(|| "target".into()))
        .join("android");
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let serial = if let Some(serial) = &options.device {
        let state = output(Command::new(&adb).args(["-s", serial, "get-state"]))?;
        if state.trim() != "device" {
            return Err(format!(
                "device {serial} is not ready; enable USB debugging and authorize this computer"
            ));
        }
        serial.clone()
    } else if let Some(serial) = running_avd(&adb, &options.avd)? {
        serial
    } else if options.action == "logs" {
        return Err("the selected emulator is not running; run cargo xtask android first".into());
    } else {
        start(&sdk, &adb, &options, &target)?
    };
    println!("Android device: {serial}");
    if options.action == "logs" {
        let uid =
            app_uid(&adb, &serial)?.ok_or("Lince debug APK is not installed on this device")?;
        return checked(&mut logcat(&adb, &serial, uid, "1"));
    }
    if let Some(apk) = apk {
        println!(
            "Installing {} (existing app data is preserved)",
            apk.display()
        );
        let result = checked(
            Command::new(&adb)
                .args(["-s", &serial, "install", "-r"])
                .arg(apk),
        );
        if let Err(error) = result {
            return Err(format!(
                "{error}\nIf the APK has no matching ABI, use an APK built for this emulator. A signing-key mismatch requires the original signing key; this command never uninstalls your data."
            ));
        }
    }
    let Some(uid) = app_uid(&adb, &serial)? else {
        println!(
            "Emulator ready. Install Lince with cargo xtask android --apk /path/to/app-debug.apk"
        );
        return Ok(());
    };
    checked(Command::new(&adb).args(["-s", &serial, "shell", "am", "force-stop", PACKAGE]))?;
    let since = output(Command::new(&adb).args(["-s", &serial, "shell", "date", "+%s.%N"]))?;
    let log = target.join(format!("{}.log", serial.replace(':', "_")));
    let file = File::create(&log).map_err(|e| e.to_string())?;
    let mut capture = logcat(&adb, &serial, uid, since.trim())
        .stdout(file)
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())?;
    println!("Starting Lince; startup logs: {}", log.display());
    let launched = output(
        Command::new(&adb).args(["-s", &serial, "shell", "am", "start", "-W", "-n", COMPONENT]),
    );
    let launched = launched.map(|report| {
        print!("{report}");
        report
    });
    if launched.is_ok() {
        thread::sleep(Duration::from_secs(10));
    }
    let capture_exit = capture.try_wait().map_err(|e| e.to_string());
    stop(&mut capture);
    if let Some(status) = capture_exit? {
        return Err(format!(
            "startup log capture ended early with {status}; see {}",
            log.display()
        ));
    }
    let report = launched.map_err(|e| format!("{e}; see {}", log.display()))?;
    let captured = fs::read_to_string(&log).map_err(|e| e.to_string())?;
    startup_result(&report, &captured).map_err(|e| format!("{e}; see {}", log.display()))?;
    let alive = Command::new(&adb)
        .args(["-s", &serial, "shell", "pidof", PACKAGE])
        .output()
        .map_err(|e| e.to_string())?;
    if !alive.status.success() || alive.stdout.is_empty() {
        return Err(format!(
            "Lince exited during startup. Crash logs: {}",
            log.display()
        ));
    }
    println!(
        "Lince stayed running for ten seconds. Startup logs: {}",
        log.display()
    );
    println!("For live logs: cargo xtask android logs --device {serial}");
    Ok(())
}

fn app_uid(adb: &Path, serial: &str) -> Result<Option<u32>> {
    let packages = output(Command::new(adb).args([
        "-s", serial, "shell", "pm", "list", "packages", "-U", PACKAGE,
    ]))?;
    for line in packages.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() == Some(&format!("package:{PACKAGE}")) {
            return fields
                .find_map(|field| field.strip_prefix("uid:"))
                .ok_or("Android did not report the app UID")?
                .parse()
                .map(Some)
                .map_err(|_| "invalid app UID".into());
        }
    }
    Ok(None)
}

fn logcat(adb: &Path, serial: &str, uid: u32, since: &str) -> Command {
    let mut command = Command::new(adb);
    command
        .args([
            "-s",
            serial,
            "logcat",
            "-b",
            "main",
            "-b",
            "system",
            "-b",
            "crash",
            "-v",
            "threadtime",
            "-T",
            since,
            "--uid",
        ])
        .arg(uid.to_string());
    command
}

fn startup_result(report: &str, log: &str) -> Result<()> {
    if log.lines().any(|line| {
        ["panicked at", "FATAL EXCEPTION", "Fatal signal"]
            .iter()
            .any(|message| line.contains(message))
    }) {
        return Err("Lince reported a startup crash".into());
    }
    if !report.lines().any(|line| line.trim() == "Status: ok") {
        return Err("Android did not confirm startup (it may have timed out)".into());
    }
    Ok(())
}

fn sdk_path(options: &Options) -> Result<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let default = if cfg!(target_os = "windows") {
        env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Android/Sdk"))
    } else if cfg!(target_os = "macos") {
        home.map(|p| p.join("Library/Android/sdk"))
    } else {
        home.map(|p| p.join("Android/Sdk"))
    };
    options.sdk.clone().or_else(|| env::var_os("ANDROID_HOME").filter(|v| !v.is_empty()).map(PathBuf::from))
        .or_else(|| env::var_os("ANDROID_SDK_ROOT").filter(|v| !v.is_empty()).map(PathBuf::from))
        .or(default).filter(|p| p.is_dir())
        .ok_or_else(|| "Android SDK not found. Install Android Command-line Tools and set ANDROID_HOME, or pass --sdk PATH.".into())
}

fn tool(sdk: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = sdk.join(relative);
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{} not found; run cargo xtask android setup --sdk {}",
            path.display(),
            sdk.display()
        ))
    }
}

fn manager(sdk: &Path, name: &str) -> Result<PathBuf> {
    let filename = if cfg!(target_os = "windows") {
        format!("{name}.bat")
    } else {
        name.into()
    };
    let root = sdk.join("cmdline-tools");
    let latest = root.join("latest/bin").join(&filename);
    if latest.is_file() {
        return Ok(latest);
    }
    let mut entries: Vec<_> = fs::read_dir(&root)
        .map_err(|e| {
            format!(
                "install Android Command-line Tools in {}: {e}",
                root.display()
            )
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("bin").join(&filename))
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    entries
        .pop()
        .ok_or_else(|| format!("{name} not found in {}", root.display()))
}

fn image(arch: &str) -> Result<String> {
    let abi = match arch {
        "x86_64" => "x86_64",
        "aarch64" => "arm64-v8a",
        _ => return Err(format!("Android emulator setup is unsupported on {arch}")),
    };
    Ok(format!("system-images;android-35;google_apis;{abi}"))
}

fn setup(sdk: &Path, avd: &str) -> Result<()> {
    let image = image(env::consts::ARCH)?;
    println!("Installing Android emulator and {image}. The download can take several GB.");
    checked(
        Command::new(manager(sdk, "sdkmanager")?)
            .arg(format!("--sdk_root={}", sdk.display()))
            .args(["platform-tools", "emulator", &image]),
    )?;
    let emulator = tool(sdk, "emulator/emulator")?;
    if output(Command::new(&emulator).arg("-list-avds"))?
        .lines()
        .any(|name| name.trim() == avd)
    {
        println!("Keeping existing virtual device {avd} and its data.");
        return Ok(());
    }
    let mut create = Command::new(manager(sdk, "avdmanager")?)
        .env("ANDROID_HOME", sdk)
        .args([
            "create",
            "avd",
            "--name",
            avd,
            "--package",
            &image,
            "--device",
            "pixel_7",
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    create
        .stdin
        .take()
        .ok_or("avdmanager stdin unavailable")?
        .write_all(b"no\n")
        .map_err(|e| e.to_string())?;
    let status = create.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("avdmanager exited with {status}"));
    }
    println!(
        "Virtual device {avd} is ready. Run cargo xtask android --sdk {}",
        sdk.display()
    );
    Ok(())
}

fn emulators(devices: &str) -> Vec<&str> {
    devices
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            (serial.starts_with("emulator-") && parts.next() == Some("device")).then_some(serial)
        })
        .collect()
}

fn running_avd(adb: &Path, avd: &str) -> Result<Option<String>> {
    let devices = output(Command::new(adb).arg("devices"))?;
    for serial in emulators(&devices) {
        if let Ok(name) = output(Command::new(adb).args(["-s", serial, "emu", "avd", "name"])) {
            if name.lines().next().map(str::trim) == Some(avd) {
                return Ok(Some(serial.into()));
            }
        }
    }
    Ok(None)
}

fn start(sdk: &Path, adb: &Path, options: &Options, logs: &Path) -> Result<String> {
    let emulator = tool(sdk, "emulator/emulator")?;
    let names = output(Command::new(&emulator).arg("-list-avds"))?;
    if !names.lines().any(|name| name.trim() == options.avd) {
        return Err(format!(
            "virtual device {} not found; run cargo xtask android setup --sdk {} --avd {}",
            options.avd,
            sdk.display(),
            options.avd
        ));
    }
    checked(Command::new(&emulator).arg("-accel-check"))?;
    let port = (5554..=5582)
        .step_by(2)
        .find(|&port| {
            let console = TcpListener::bind(("127.0.0.1", port));
            let adb = TcpListener::bind(("127.0.0.1", port + 1));
            console.is_ok() && adb.is_ok()
        })
        .ok_or("no free Android emulator port")?;
    let serial = format!("emulator-{port}");
    let log = logs.join(format!("{serial}-emulator.log"));
    let file = File::create(&log).map_err(|e| e.to_string())?;
    let mut command = Command::new(&emulator);
    command
        .env("ANDROID_HOME", sdk)
        .args([
            "-avd",
            &options.avd,
            "-port",
            &port.to_string(),
            "-memory",
            "2048",
            "-gpu",
            "auto",
            "-no-snapshot",
            "-no-boot-anim",
            "-camera-front",
            "none",
            "-camera-back",
            "none",
        ])
        .stdin(Stdio::null())
        .stderr(file.try_clone().map_err(|e| e.to_string())?)
        .stdout(file);
    if options.headless {
        command.arg("-no-window");
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start emulator: {e}"))?;
    println!("Booting {}. Emulator log: {}", options.avd, log.display());
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "emulator exited with {status}; see {}",
                log.display()
            ));
        }
        let boot = Command::new(adb)
            .args(["-s", &serial, "shell", "getprop", "sys.boot_completed"])
            .output()
            .map_err(|e| e.to_string())?;
        if boot.status.success() && String::from_utf8_lossy(&boot.stdout).trim() == "1" {
            let name = output(Command::new(adb).args(["-s", &serial, "emu", "avd", "name"]))?;
            if name.lines().next().map(str::trim) != Some(&options.avd) {
                stop(&mut child);
                return Err("emulator port was taken by another virtual device; retry".into());
            }
            return Ok(serial);
        }
        thread::sleep(Duration::from_secs(2));
    }
    stop(&mut child);
    Err(format!(
        "emulator did not boot within 180 seconds; see {}",
        log.display()
    ))
}

fn output(command: &mut Command) -> Result<String> {
    let output = command
        .output()
        .map_err(|e| format!("cannot start {:?}: {e}", command.get_program()))?;
    if !output.status.success() {
        return Err(format!(
            "{:?} failed: {}{}",
            command.get_program(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_physical_devices_out_of_automatic_selection() {
        assert_eq!(
            emulators(
                "List of devices attached\nphone123\tdevice\nemulator-5554\toffline\nemulator-5556\tdevice\n"
            ),
            vec!["emulator-5556"]
        );
    }

    #[test]
    fn accepts_explicit_phone_and_apk_with_spaces() {
        let args = [
            "run",
            "--device",
            "192.168.1.2:5555",
            "--apk",
            "/tmp/my app.apk",
        ]
        .map(OsString::from);
        let options = Options::parse(&args).unwrap();
        assert_eq!(options.device.as_deref(), Some("192.168.1.2:5555"));
        assert_eq!(options.apk.unwrap(), PathBuf::from("/tmp/my app.apk"));
    }

    #[test]
    fn rejects_ambiguous_or_invalid_requests() {
        for args in [
            vec!["--apk"],
            vec!["--device", "--all"],
            vec!["--avd", "bad;name"],
            vec!["setup", "--device", "phone"],
            vec!["logs", "--apk", "file.apk"],
            vec!["delete"],
        ] {
            assert!(
                Options::parse(&args.into_iter().map(OsString::from).collect::<Vec<_>>()).is_err()
            );
        }
    }

    #[test]
    fn selects_images_for_the_computer_cpu() {
        assert!(image("x86_64").unwrap().ends_with(";x86_64"));
        assert!(image("aarch64").unwrap().ends_with(";arm64-v8a"));
        assert!(image("riscv64").is_err());
    }

    #[test]
    fn rejects_startup_panics_and_timeouts_even_when_process_is_alive() {
        assert!(startup_result("Status: timeout\nComplete", "").is_err());
        assert!(
            startup_result(
                "Status: ok",
                "RustStdoutStderr: thread 'main' panicked at schedule.rs"
            )
            .is_err()
        );
        assert!(startup_result("Status: ok", "libc: Fatal signal 6").is_err());
        assert!(startup_result("Error: Activity class does not exist", "").is_err());
        assert!(startup_result("Status: ok\nComplete", "INFO Started").is_ok());
    }
}
