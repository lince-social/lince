use super::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::Read,
    net::{TcpListener, TcpStream},
    os::unix::{fs::DirBuilderExt, process::CommandExt},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
type Environment = BTreeMap<OsString, OsString>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn ensure(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn write(path: &Path, value: &Value) -> Result<()> {
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(value).map_err(err)?),
    )
    .map_err(err)
}
fn read(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)
}
fn pause(seconds: f64) -> Result<()> {
    let end = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < end {
        ensure(
            !INTERRUPTED.load(Ordering::Relaxed),
            "Browser validation interrupted",
        )?;
        thread::sleep(Duration::from_millis(50).min(end.saturating_duration_since(Instant::now())));
    }
    Ok(())
}
fn wait<T>(seconds: u64, mut check: impl FnMut() -> Result<T>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(seconds);
    loop {
        match check() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if Instant::now() >= end {
                    return Err(format!("Timed out waiting: {error}"));
                }
            }
        }
        pause(0.1)?;
    }
}
fn host_environment() -> Environment {
    restore_host_environment(env::vars_os().collect())
}
fn restore_host_environment(mut environment: Environment) -> Environment {
    for name in [
        "LD_LIBRARY_PATH",
        "GBM_BACKENDS_PATH",
        "__EGL_VENDOR_LIBRARY_DIRS",
    ] {
        if let Some(saved) = environment.remove(OsStr::new(&format!("LINCE_HOST_{name}"))) {
            let value = saved.to_string_lossy();
            if value == "0" {
                environment.remove(OsStr::new(name));
            } else if let Some(value) = value.strip_prefix('1') {
                environment.insert(name.into(), value.into());
            }
        }
    }
    environment
}
fn configured(program: impl AsRef<OsStr>, environment: &Environment, directory: &Path) -> Command {
    let mut command = Command::new(program);
    command.env_clear().envs(environment).current_dir(directory);
    command
}
struct Group(Child);
impl Drop for Group {
    fn drop(&mut self) {
        if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::TERM);
        }
        let end = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait().ok().flatten().is_none() && Instant::now() < end {
            thread::sleep(Duration::from_millis(50));
        }
        if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = self.0.wait();
    }
}
fn output(command: &mut Command, seconds: u64) -> Result<Output> {
    let mut child = Group(
        command
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(err)?,
    );
    let mut stdout = child.0.stdout.take().ok_or("Missing child stdout")?;
    let mut stderr = child.0.stderr.take().ok_or("Missing child stderr")?;
    let out = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let errors = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let end = Instant::now() + Duration::from_secs(seconds);
    let status = loop {
        if let Some(status) = child.0.try_wait().map_err(err)? {
            break Ok(status);
        }
        if Instant::now() >= end {
            break Err("External program timed out".to_string());
        }
        thread::sleep(Duration::from_millis(20));
    };
    drop(child);
    let stdout = out
        .join()
        .map_err(|_| "Child stdout reader failed")?
        .map_err(err)?;
    let stderr = errors
        .join()
        .map_err(|_| "Child stderr reader failed")?
        .map_err(err)?;
    Ok(Output {
        status: status?,
        stdout,
        stderr,
    })
}
fn checked(command: &mut Command, seconds: u64) -> Result<Vec<u8>> {
    let result = output(command, seconds)?;
    ensure(
        result.status.success(),
        format!(
            "Program failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(result.stdout)
}
fn launch(command: &mut Command, log: &Path) -> Result<Group> {
    let file = File::create(log).map_err(err)?;
    Ok(Group(
        command
            .process_group(0)
            .stdout(file.try_clone().map_err(err)?)
            .stderr(file)
            .spawn()
            .map_err(err)?,
    ))
}
fn alive(child: &mut Group) -> Result<()> {
    ensure(
        child.0.try_wait().map_err(err)?.is_none(),
        "Application exited during validation",
    )
}
fn exited(child: &mut Group, seconds: u64) -> Result<i32> {
    let end = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(status) = child.0.try_wait().map_err(err)? {
            ensure(
                status.success(),
                format!("Application exited with {status}"),
            )?;
            return Ok(status.code().unwrap_or(0));
        }
        ensure(
            Instant::now() < end,
            "Timed out waiting for orderly application exit",
        )?;
        thread::sleep(Duration::from_millis(50));
    }
}
fn private_dir(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(err)
}
fn environment_json(names: &[&str]) -> Value {
    names
        .iter()
        .map(|name| {
            (
                (*name).to_string(),
                env::var(name)
                    .ok()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            )
        })
        .collect()
}
fn program(value: &Value) -> Result<Command> {
    let executable = value["program"]
        .as_str()
        .ok_or("Provider program must be a string")?;
    ensure(!executable.is_empty(), "Empty provider program")?;
    let mut command = Command::new(executable);
    if let Some(arguments) = value.get("arguments") {
        for arg in arguments
            .as_array()
            .ok_or("Provider arguments must be an array")?
        {
            command.arg(arg.as_str().ok_or("Provider argument must be a string")?);
        }
    }
    Ok(command)
}
fn settings_environment(command: &mut Command, run: &Path) {
    for (key, dir) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        command.env(key, run.join(dir));
    }
}

pub(super) fn helper(args: &[OsString]) -> Result<()> {
    let mode = args
        .first()
        .and_then(|s| s.to_str())
        .ok_or("Missing browser helper mode")?;
    let run = Path::new(args.get(1).ok_or("Missing browser helper directory")?);
    let executable = args.get(2).ok_or("Missing browser helper executable")?;
    let mut command = match mode {
        "ready" => {
            checked(
                Command::new("dbus-update-activation-environment").args([
                    "WAYLAND_DISPLAY",
                    "XDG_CURRENT_DESKTOP",
                    "XDG_SESSION_TYPE",
                    "XDG_SESSION_DESKTOP",
                ]),
                10,
            )?;
            write(
                &run.join("ready.json"),
                &json!({"pid":rustix::process::getppid().map(|p|p.as_raw_nonzero().get()),"helper_pid":std::process::id(),"environment":environment_json(&["WAYLAND_DISPLAY","XDG_RUNTIME_DIR","DBUS_SESSION_BUS_ADDRESS","GDK_BACKEND","QT_QPA_PLATFORM","XDG_CURRENT_DESKTOP","XDG_SESSION_DESKTOP","XDG_SESSION_TYPE"])}),
            )?;
            if run.join("provider.json").exists() {
                let mut command = program(&read(&run.join("provider.json"))?["target"])?;
                settings_environment(&mut command, run);
                command
            } else {
                wait(300, || {
                    ensure(
                        run.join("capture.request").exists(),
                        "Waiting for capture request",
                    )
                })?;
                let mut command = Command::new(executable);
                command
                    .arg(run.join("capture.json"))
                    .arg("--image")
                    .arg(run.join("scene.ppm"))
                    .args(["--application", "firefox"]);
                command
            }
        }
        "client" => {
            write(
                &run.join("client-environment.json"),
                &environment_json(&[
                    "WAYLAND_DISPLAY",
                    "DISPLAY",
                    "XDG_RUNTIME_DIR",
                    "GDK_BACKEND",
                    "LD_LIBRARY_PATH",
                    "GBM_BACKENDS_PATH",
                    "__EGL_VENDOR_LIBRARY_DIRS",
                    "NIRI_SOCKET",
                    "SWAYSOCK",
                    "HYPRLAND_INSTANCE_SIGNATURE",
                ]),
            )?;
            wait(60, || {
                ensure(
                    run.join("ready.json").exists(),
                    "Waiting for session readiness",
                )
            })?;
            let mut command = Command::new(executable);
            command.args(&args[3..]);
            command
        }
        "sandbox" => {
            write(
                &run.join("sandbox.json"),
                &json!({"pid":std::process::id()}),
            )?;
            let mut command = Command::new(executable);
            command.args(&args[3..]);
            command
        }
        _ => return Err(format!("Unknown helper mode {mode}")),
    };
    command.current_dir(run);
    Err(command.exec().to_string())
}

#[derive(Debug, Default)]
struct Options {
    values: BTreeMap<String, String>,
    flags: Vec<String>,
}
impl Options {
    fn parse(args: &[OsString]) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.iter();
        let executable = args
            .next()
            .ok_or("Missing installed executable")?
            .to_str()
            .ok_or("Arguments must be UTF-8")?;
        ensure(
            Path::new(executable).is_absolute(),
            "Use an absolute installed executable path",
        )?;
        options
            .values
            .insert("executable".into(), executable.into());
        for (key, value) in [
            ("--seconds", "75"),
            ("--outputs", "1"),
            ("--lifecycle-rounds", "0"),
            ("--firefox", "firefox"),
            ("--terminal", "xdg-terminal-exec"),
        ] {
            options.values.insert(key.into(), value.into());
        }
        let mut seen = std::collections::BTreeSet::new();
        while let Some(arg) = args.next() {
            let arg = arg.to_str().ok_or("Arguments must be UTF-8")?;
            ensure(
                seen.insert(arg.to_string()),
                format!("Repeated argument {arg}"),
            )?;
            if [
                "--plain",
                "--keep-size",
                "--fullscreen",
                "--ownership",
                "--save-failure",
                "--reopen-sandbox",
            ]
            .contains(&arg)
            {
                options.flags.push(arg.into());
            } else {
                ensure(
                    [
                        "--geckodriver",
                        "--capture-probe",
                        "--provider",
                        "--host-close",
                        "--host-prepare",
                        "--fixtures",
                        "--seconds",
                        "--outputs",
                        "--lifecycle-rounds",
                        "--firefox",
                        "--terminal",
                    ]
                    .contains(&arg),
                    format!("Unknown argument {arg}"),
                )?;
                let value = args
                    .next()
                    .and_then(|v| v.to_str())
                    .ok_or_else(|| format!("Missing value for {arg}"))?;
                options.values.insert(arg.into(), value.into());
            }
        }
        for name in ["--geckodriver", "--capture-probe", "--fixtures"] {
            ensure(options.values.contains_key(name), format!("Missing {name}"))?;
        }
        ensure(
            (15..=3600).contains(&options.number("--seconds")?),
            "--seconds must be between 15 and 3600",
        )?;
        ensure(
            (1..=8).contains(&options.number("--outputs")?),
            "--outputs must be between 1 and 8",
        )?;
        ensure(
            options.number("--lifecycle-rounds")? <= 20,
            "--lifecycle-rounds must be between 0 and 20",
        )?;
        ensure(
            !(options.flag("--save-failure") || options.flag("--reopen-sandbox"))
                || options.values.contains_key("--host-close"),
            "Save/reopen checks require --host-close",
        )?;
        for key in [
            "--geckodriver",
            "--capture-probe",
            "--fixtures",
            "--provider",
            "--host-close",
            "--host-prepare",
        ] {
            if let Some(value) = options.values.get_mut(key) {
                *value = fs::canonicalize(&*value)
                    .map_err(err)?
                    .to_str()
                    .ok_or("Paths must be UTF-8")?
                    .to_string();
            }
        }
        Ok(options)
    }
    fn get(&self, key: &str) -> &str {
        &self.values[key]
    }
    fn flag(&self, key: &str) -> bool {
        self.flags.iter().any(|s| s == key)
    }
    fn number(&self, key: &str) -> Result<u64> {
        self.get(key).parse().map_err(err)
    }
}
struct Driver {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
    base: String,
    session: String,
}
impl Driver {
    fn new(port: u16) -> Result<Self> {
        Ok(Self {
            runtime: tokio::runtime::Runtime::new().map_err(err)?,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(err)?,
            base: format!("http://127.0.0.1:{port}"),
            session: String::new(),
        })
    }
    fn request(&self, method: &str, path: &str, payload: Option<Value>) -> Result<Value> {
        self.runtime.block_on(async {
            let mut request = self
                .client
                .request(method.parse().map_err(err)?, format!("{}{path}", self.base));
            if let Some(payload) = payload {
                request = request.json(&payload);
            }
            let response = request.send().await.map_err(err)?;
            let status = response.status();
            let text = response.text().await.map_err(err)?;
            let mut value: Value = serde_json::from_str(&text)
                .map_err(|e| format!("WebDriver response {status}: {e}: {text}"))?;
            if let Some(inner) = value.get("value") {
                value = inner.clone();
            }
            ensure(
                status.is_success() && value.get("error").is_none(),
                format!("WebDriver {status}: {value}"),
            )?;
            Ok(value)
        })
    }
    fn command(&self, method: &str, path: &str, payload: Option<Value>) -> Result<Value> {
        self.request(method, &format!("/session/{}{path}", self.session), payload)
    }
    fn js(&self, script: &str) -> Result<Value> {
        self.command(
            "POST",
            "/execute/sync",
            Some(json!({"script":script,"args":[]})),
        )
    }
    fn post(&self, path: &str, payload: Value) -> Result<Value> {
        self.command("POST", path, Some(payload))
    }
    fn get(&self, path: &str) -> Result<Value> {
        self.command("GET", path, None)
    }
    fn video_ready(&self) -> Result<()> {
        wait(40, || {
            ensure(self.js("return typeof window.parkMetrics === 'function' && window.parkMetrics().videoWidth > 0")?==true,"Waiting for decoded video")
        })
    }
}
fn port() -> Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))
        .map_err(err)?
        .local_addr()
        .map_err(err)?
        .port())
}
fn uri(path: &Path) -> String {
    format!(
        "file://{}",
        path.to_string_lossy()
            .bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            })
            .collect::<String>()
    )
}
fn fields(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            line.split_once(':')
                .map(|(k, v)| (k.to_string(), v.trim().to_string()))
        })
        .collect()
}
fn resources(pid: u32, elapsed: f64) -> Result<Value> {
    let process = PathBuf::from(format!("/proc/{pid}"));
    let status = fields(&fs::read_to_string(process.join("status")).map_err(err)?);
    let stat = fs::read_to_string(process.join("stat")).map_err(err)?;
    let stat: Vec<_> = stat
        .rsplit_once(')')
        .ok_or("Invalid proc stat")?
        .1
        .split_whitespace()
        .collect();
    let mut drm = BTreeMap::new();
    for entry in fs::read_dir(process.join("fdinfo")).map_err(err)?.flatten() {
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let data = fields(&text);
        let Some(id) = data.get("drm-client-id") else {
            continue;
        };
        let key = format!(
            "{}:{id}",
            data.get("drm-pdev")
                .map(String::as_str)
                .unwrap_or("unknown")
        );
        drm.insert(
            key,
            data.into_iter()
                .filter(|(k, _)| {
                    k.starts_with("drm-total-")
                        || k.starts_with("drm-resident-")
                        || k.starts_with("drm-memory-")
                        || k == "drm-driver"
                })
                .collect::<BTreeMap<_, _>>(),
        );
    }
    let cpu = |i: usize| -> Result<u64> {
        stat.get(i)
            .ok_or("Missing CPU counter")?
            .parse()
            .map_err(err)
    };
    Ok(
        json!({"elapsed":elapsed,"rss":status.get("VmRSS"),"swap":status.get("VmSwap"),"virtual":status.get("VmSize"),"threads":status.get("Threads"),"descriptors":fs::read_dir(process.join("fd")).map_err(err)?.count(),"cpu_ticks":cpu(11)?+cpu(12)?,"drm_clients":if drm.is_empty(){Value::Null}else{json!(drm)}}),
    )
}
fn manager_environment() -> Option<Value> {
    let result = output(
        Command::new("systemctl").args(["--user", "show-environment"]),
        5,
    )
    .ok()?;
    if !result.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&result.stdout)
            .lines()
            .filter_map(|line| line.split_once('='))
            .filter(|(key, _)| {
                [
                    "WAYLAND_DISPLAY",
                    "XDG_CURRENT_DESKTOP",
                    "XDG_SESSION_TYPE",
                    "NIRI_SOCKET",
                ]
                .contains(key)
            })
            .map(|(key, value)| (key.to_string(), json!(value)))
            .collect(),
    )
}
fn host_program(options: &Options, key: &str, pid: u32, run: &Path) -> Result<()> {
    if let Some(program) = options.values.get(key) {
        checked(
            configured(program, &host_environment(), run).arg(pid.to_string()),
            10,
        )?;
    }
    Ok(())
}
fn live_work(
    options: &Options,
    run: &Path,
    environment: &Environment,
    children: &mut Vec<Group>,
    driver: &mut Driver,
    result: &mut Value,
) -> Result<(Value, Environment)> {
    let executable = options.get("executable");
    let info: Value = serde_json::from_slice(&checked(
        configured(executable, environment, run).arg("--build-info"),
        10,
    )?)
    .map_err(err)?;
    ensure(
        info["optimized"] == true
            && info["features"]["park"] == true
            && info["features"]["ui"] == true,
        "Installed browser validation requires an optimized Park GUI executable",
    )?;
    result["build_info"] = info;
    let payload = Path::new(executable).with_file_name(".lince-wrapped");
    let payload = if payload.exists() {
        payload
    } else {
        PathBuf::from(executable)
    };
    let mut file = File::open(&payload).map_err(err)?;
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let n = file.read(&mut bytes).map_err(err)?;
        if n == 0 {
            break;
        }
        digest.update(&bytes[..n]);
    }
    result["executable_sha256"] = json!(
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    result["executable"] = json!(executable);
    result["payload"] = json!(payload);
    let own = env::current_exe().map_err(err)?;
    let mut command = configured("dbus-run-session", environment, run);
    command
        .arg("--")
        .arg(executable)
        .arg("--park-nested")
        .arg("--directory")
        .arg(run.join("cell"))
        .args([
            "--listen-addr",
            "127.0.0.1:0",
            "--peer-port",
            "0",
            "--park-outputs",
            options.get("--outputs"),
        ]);
    if !options.flag("--plain") {
        command.arg("--showoff");
    }
    command
        .arg("--")
        .arg(&own)
        .args(["maintenance", "browser-helper", "client"])
        .arg(run)
        .arg(options.get("--firefox"))
        .args(["--no-remote", "--marionette", "--profile"])
        .arg(run.join("profile"))
        .arg(uri(&run.join("page.html")));
    children.push(launch(&mut command, &run.join("session.log"))?);
    let ready = wait(40, || read(&run.join("ready.json")))?;
    let pid = u32::try_from(ready["pid"].as_u64().ok_or("Missing compositor PID")?).map_err(err)?;
    result["ready"] = ready.clone();
    host_program(options, "--host-prepare", pid, run)?;
    let mut inner = environment.clone();
    for (key, value) in ready["environment"]
        .as_object()
        .ok_or("Missing session environment")?
    {
        if let Some(value) = value.as_str() {
            inner.insert(key.into(), value.into());
        }
    }
    for key in [
        "WAYLAND_SOCKET",
        "NIRI_SOCKET",
        "SWAYSOCK",
        "I3SOCK",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "WAYFIRE_SOCKET",
    ] {
        inner.remove(OsStr::new(key));
    }
    let marionette = options.get("--marionette-port");
    let mut gecko = configured(options.get("--geckodriver"), environment, run);
    gecko.args([
        "--connect-existing",
        "--marionette-port",
        marionette,
        "--port",
        options.get("--webdriver-port"),
    ]);
    children.push(launch(&mut gecko, &run.join("webdriver.log"))?);
    wait(40, || driver.request("GET", "/status", None))?;
    wait(40, || {
        TcpStream::connect_timeout(
            &format!("127.0.0.1:{marionette}").parse().map_err(err)?,
            Duration::from_millis(200),
        )
        .map_err(err)
    })?;
    let response = wait(40, || {
        driver.request(
            "POST",
            "/session",
            Some(json!({"capabilities":{"alwaysMatch":{"browserName":"firefox"}}})),
        )
    })?;
    driver.session = response["sessionId"]
        .as_str()
        .ok_or("Missing WebDriver session ID")?
        .into();
    driver.video_ready()?;
    result["browser"] = response["capabilities"].clone();
    let element = driver.post("/element", json!({"using":"css selector","value":"input"}))?;
    let id = element["element-6066-11e4-a52e-4f735466cecf"]
        .as_str()
        .ok_or("Missing input element")?;
    driver.post(&format!("/element/{id}/click"), json!({}))?;
    driver.post(
        &format!("/element/{id}/value"),
        json!({"text":"Park typed 67"}),
    )?;
    ensure(
        driver.js("return document.querySelector('input').value")? == "Park typed 67",
        "Browser did not receive typed input",
    )?;
    driver.post("/actions",json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","duration":0,"origin":element,"x":0,"y":0},{"type":"pointerDown","button":2},{"type":"pointerUp","button":2}]}]}))?;
    pause(1.0)?;
    if !run.join("provider.json").exists() {
        fs::write(run.join("capture.request"), []).map_err(err)?;
        let capture = wait(40, || read(&run.join("capture.json")))?;
        ensure(
            capture["status"] == "passed",
            format!("Capture probe failed: {capture}"),
        )?;
        result["capture"] = capture;
    }
    driver.post(&format!("/element/{id}/value"), json!({"text":"\u{e00c}"}))?;
    lifecycle(options, run, driver, &mut children[0], pid, result)?;
    if run.join("provider.json").exists() {
        provider(run, driver, &mut inner, result)?;
    }
    if !options.flag("--keep-size") {
        driver.post("/window/maximize", json!({}))?;
        pause(2.0)?;
    }
    if options.flag("--fullscreen") {
        driver.post("/window/fullscreen", json!({}))?;
        pause(2.0)?;
    }
    driver.js("document.querySelector('video').scrollIntoView({block:'center'});return true")?;
    result["video_visibility"]=driver.js("const r=document.querySelector('video').getBoundingClientRect();return {top:r.top,bottom:r.bottom,height:r.height,viewport:innerHeight,visible:r.top>=0&&r.bottom<=innerHeight}")?;
    ensure(
        result["video_visibility"]["visible"] == true,
        "Video is outside the viewport",
    )?;
    let offset = fs::metadata(run.join("session.log")).map_err(err)?.len() as usize;
    result["video_start"] = driver.js("return window.parkMetrics()")?;
    result["resources"] = json!([]);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(options.number("--seconds")?) {
        alive(&mut children[0])?;
        result["resources"]
            .as_array_mut()
            .ok_or("Invalid resource report")?
            .push(resources(pid, started.elapsed().as_secs_f64())?);
        pause(1.0)?;
    }
    result["video_end"] = driver.js("return window.parkMetrics()")?;
    let trace = fs::read(run.join("session.log")).map_err(err)?;
    fs::write(
        run.join("measured.log"),
        trace.get(offset..).ok_or("Session log was truncated")?,
    )
    .map_err(err)?;
    if options.flag("--ownership") {
        for mode in ["--park-nested", "--park"] {
            let contender = output(
                configured(executable, environment, run)
                    .args([mode, "--directory"])
                    .arg(run.join("cell"))
                    .args(["--listen-addr", "127.0.0.1:0", "--peer-port", "0"]),
                10,
            )?;
            ensure(
                !contender.status.success()
                    && String::from_utf8_lossy(&contender.stderr)
                        .contains("Another Lince instance owns"),
                "Second session owner was not rejected",
            )?;
        }
        checked(
            configured(executable, environment, run)
                .arg("--directory")
                .arg(run.join("cell")),
            10,
        )?;
        alive(&mut children[0])?;
        result["ownership"] =
            json!("session contenders rejected; ordinary invocation activated the same owner");
    }
    if options.flag("--save-failure") {
        failed_save(options, run, driver, &mut children[0], pid, result)?;
    }
    Ok((ready, inner))
}
fn lifecycle(
    options: &Options,
    run: &Path,
    driver: &Driver,
    compositor: &mut Group,
    pid: u32,
    result: &mut Value,
) -> Result<()> {
    let rounds = options.number("--lifecycle-rounds")?;
    if rounds == 0 {
        return Ok(());
    }
    let original = driver.get("/window")?;
    let baseline = driver.get("/window/handles")?;
    result["client_lifecycle"] = json!({"baseline":resources(pid,0.0)?,"rounds":[],"scope":"Real application window creation, geometry transitions and closure; WebDriver does not establish physical keyboard focus or scanout."});
    let started = Instant::now();
    for index in 0..rounds {
        let round = Instant::now();
        let created = driver.post("/window/new", json!({"type":"window"}))?;
        driver.post("/window", json!({"handle":created["handle"]}))?;
        driver.post("/url", json!({"url":uri(&run.join("page.html"))}))?;
        driver.video_ready()?;
        let maximized = driver.post("/window/maximize", json!({}))?;
        let fullscreen = driver.post("/window/fullscreen", json!({}))?;
        let requested = json!({"width":720+index%2*80,"height":480+index%2*80});
        let restored = driver.post("/window/rect", requested.clone())?;
        for rectangle in [&maximized, &fullscreen, &restored] {
            ensure(
                rectangle["width"].as_u64().is_some_and(|v| v > 0)
                    && rectangle["height"].as_u64().is_some_and(|v| v > 0),
                "Invalid window dimensions",
            )?;
        }
        for axis in ["width", "height"] {
            ensure(
                fullscreen[axis].as_u64() >= maximized[axis].as_u64(),
                "Fullscreen shrank the window",
            )?;
            ensure(
                restored[axis] == requested[axis],
                "Window size was not restored",
            )?;
        }
        ensure(
            driver.js("return !window.fullScreen")? == true,
            "Browser did not leave fullscreen",
        )?;
        driver.command("DELETE", "/window", None)?;
        driver.post("/window", json!({"handle":original}))?;
        let handles = driver.get("/window/handles")?;
        ensure(
            same_handles(&handles, &baseline),
            "A closed application window was retained",
        )?;
        ensure(
            driver.js("return document.querySelector('input').value")? == "Park typed 67",
            "Original window input was lost",
        )?;
        alive(compositor)?;
        pause(1.0)?;
        result["client_lifecycle"]["rounds"].as_array_mut().ok_or("Invalid lifecycle report")?.push(json!({"round":index+1,"seconds":round.elapsed().as_secs_f64(),"maximized":maximized,"fullscreen":fullscreen,"requested_restore":requested,"actual_restore":restored,"resources":resources(pid,started.elapsed().as_secs_f64())?}));
    }
    result["client_lifecycle"]["status"] = json!("passed");
    Ok(())
}
fn same_handles(first: &Value, last: &Value) -> bool {
    let sorted = |value: &Value| -> Option<Vec<String>> {
        let mut values = value
            .as_array()?
            .iter()
            .map(|s| s.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()?;
        values.sort();
        Some(values)
    };
    matches!((sorted(first),sorted(last)),(Some(a),Some(b)) if a==b)
}
fn provider(
    run: &Path,
    driver: &Driver,
    inner: &mut Environment,
    result: &mut Value,
) -> Result<()> {
    let provider = read(&run.join("provider.json"))?;
    for (key, name) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        inner.insert(key.into(), run.join(name).into_os_string());
    }
    driver.post("/window/maximize", json!({}))?;
    let control = |name: &str| -> Result<()> {
        let mut command = program(&provider[name])?;
        command.env_clear().envs(&*inner).current_dir(run);
        checked(&mut command, 10)?;
        Ok(())
    };
    wait(40, || control("show"))?;
    pause(2.0)?;
    let shown = driver.get("/window/rect")?;
    control("hide")?;
    pause(2.0)?;
    let hidden = driver.get("/window/rect")?;
    control("show")?;
    pause(2.0)?;
    let restored = driver.get("/window/rect")?;
    ensure(
        hidden["height"].as_u64() > shown["height"].as_u64()
            && restored["height"] == shown["height"],
        "Provider did not reserve/release its bar area",
    )?;
    result["bar_rectangles"] = json!({"shown":shown,"hidden":hidden,"restored":restored});
    for name in ["open", "close"] {
        if provider.get(name).is_some() {
            control(name)?;
            pause(1.0)?;
        }
    }
    result["bar_placement"] = json!("passed");
    result["provider_permissions"] = provider["capabilities"].clone();
    Ok(())
}
fn failed_save(
    options: &Options,
    run: &Path,
    driver: &Driver,
    compositor: &mut Group,
    pid: u32,
    result: &mut Value,
) -> Result<()> {
    let blocked = run.join("cell/park-windows.json");
    let backup = blocked.with_extension("before-failure");
    let exists = blocked.exists();
    if exists {
        fs::rename(&blocked, &backup).map_err(err)?;
    }
    let action = (|| {
        fs::create_dir(&blocked).map_err(err)?;
        driver.post("/window/new", json!({"type":"window"}))?;
        pause(3.0)?;
        host_program(options, "--host-close", pid, run)?;
        pause(7.0)?;
        alive(compositor)?;
        ensure(
            fs::read_to_string(run.join("session.log"))
                .map_err(err)?
                .contains("window customizations could not be saved"),
            "Missing failed-save diagnostic",
        )?;
        result["failed_save_keeps_desktop"] =
            json!("passed; retry after restoring storage follows during cleanup");
        Ok(())
    })();
    if blocked.is_dir() {
        fs::remove_dir(&blocked).map_err(err)?;
    }
    if exists {
        fs::rename(backup, blocked).map_err(err)?;
    }
    action
}
fn orderly_cleanup(
    options: &Options,
    run: &Path,
    children: &mut Vec<Group>,
    result: &mut Value,
) -> Result<()> {
    if children.is_empty() || children[0].0.try_wait().map_err(err)?.is_some() {
        return Ok(());
    }
    ensure(
        options.values.contains_key("--host-close"),
        "Save-aware logout unrun: supply --host-close accepting the compositor PID",
    )?;
    let pid = result["ready"]["pid"]
        .as_u64()
        .ok_or("Missing ready PID for orderly cleanup")? as u32;
    host_program(options, "--host-close", pid, run)?;
    result["compositor_exit"] = json!(exited(&mut children[0], 20)?);
    result["save_aware_logout"] = json!("passed");
    if options.flag("--reopen-sandbox") {
        let own = env::current_exe().map_err(err)?;
        let mut command = configured("dbus-run-session", &host_environment(), run);
        command
            .arg("--")
            .arg(own)
            .args(["maintenance", "browser-helper", "sandbox"])
            .arg(run)
            .arg(options.get("executable"))
            .arg("--directory")
            .arg(run.join("cell"))
            .args(["--listen-addr", "127.0.0.1:0", "--peer-port", "0"]);
        children.push(launch(&mut command, &run.join("sandbox.log"))?);
        let pid = wait(40, || read(&run.join("sandbox.json")))?["pid"]
            .as_u64()
            .ok_or("Missing sandbox PID")? as u32;
        pause(5.0)?;
        let sandbox = children.last_mut().ok_or("Missing sandbox process")?;
        alive(sandbox)?;
        host_program(options, "--host-prepare", pid, run)?;
        pause(5.0)?;
        host_program(options, "--host-close", pid, run)?;
        exited(sandbox, 20)?;
        let mut snapshots = fs::read_dir(run.join("cell/interface.snapshots"))
            .map_err(err)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension() == Some(OsStr::new("json")))
            .collect::<Vec<_>>();
        snapshots.sort();
        let saved = read(snapshots.last().ok_or("No saved workspace snapshot")?)?;
        if !options.flag("--plain") {
            ensure(
                saved["workspaces"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|v| v["name"] == "showoff")),
                "Showoff was not preserved after Sandbox reopened",
            )?;
        }
        result["sandbox_reopen"] =
            json!("passed; same Cell, no --showoff argument, orderly save and close");
    }
    Ok(())
}

pub(super) fn run(args: &[OsString]) -> Result<()> {
    let mut options = Options::parse(args)?;
    INTERRUPTED.store(false, Ordering::Relaxed);
    let signals = tokio::runtime::Runtime::new().map_err(err)?;
    signals.block_on(async {
        let mut interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(err)?;
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(err)?;
        tokio::spawn(async move {
            tokio::select! {_=interrupt.recv()=>{},_=terminate.recv()=>{}}
            INTERRUPTED.store(true, Ordering::Relaxed);
        });
        Ok::<_, String>(())
    })?;
    let fixtures = Path::new(options.get("--fixtures"));
    let unique = format!(
        "installed-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(err)?
            .as_secs(),
        std::process::id()
    );
    let run = fixtures
        .parent()
        .ok_or("Missing browser validation directory")?
        .join("runs")
        .join(unique);
    ensure(
        !run.exists(),
        "Browser validation run directory already exists",
    )?;
    private_dir(&run)?;
    for name in ["cell", "profile", "config", "cache", "state"] {
        private_dir(&run.join(name))?;
    }
    let runtime = env::current_dir()
        .map_err(err)?
        .join("target/pvr")
        .join(std::process::id().to_string());
    ensure(!runtime.exists(), "Runtime directory already exists")?;
    private_dir(&runtime)?;
    let marionette = port()?;
    let mut webdriver = port()?;
    while webdriver == marionette {
        webdriver = port()?;
    }
    let preferences = fs::read_to_string(fixtures.join("user.js")).map_err(err)?;
    fs::write(run.join("profile/user.js"),format!("{preferences}\nuser_pref(\"marionette.port\", {marionette});\nuser_pref(\"media.autoplay.default\", 0);\n")).map_err(err)?;
    fs::copy(fixtures.join("page.html"), run.join("page.html")).map_err(err)?;
    fs::copy(fixtures.join("video.webm"), run.join("video.webm")).map_err(err)?;
    let provider = options
        .values
        .get("--provider")
        .map(|path| read(Path::new(path)))
        .transpose()?;
    if let Some(provider) = &provider {
        for name in ["target", "show", "hide"] {
            program(&provider[name])?;
        }
        write(&run.join("provider.json"), provider)?;
        if let Some(configuration) = provider.get("configuration") {
            for entry in configuration
                .as_array()
                .ok_or("Provider configuration must be an array")?
            {
                let relative = configuration_destination(&entry["destination"])?;
                let destination = run.join("config").join(relative);
                fs::create_dir_all(destination.parent().ok_or("Missing destination parent")?)
                    .map_err(err)?;
                fs::copy(
                    entry["source"]
                        .as_str()
                        .ok_or("Missing provider configuration source")?,
                    destination,
                )
                .map_err(err)?;
            }
        }
    }
    let own = env::current_exe().map_err(err)?;
    write(
        &run.join("cell/park.json"),
        &json!({"terminal":{"program":options.get("--terminal"),"arguments":[]},"ready":{"program":own,"arguments":["maintenance","browser-helper","ready",run,options.get("--capture-probe")],"capabilities":provider.as_ref().map(|p|p["capabilities"].clone()).unwrap_or_else(||json!(["capture","window-list"]))}}),
    )?;
    let mut environment = host_environment();
    let display = environment
        .get(OsStr::new("WAYLAND_DISPLAY"))
        .cloned()
        .unwrap_or_else(|| "wayland-1".into());
    let outer = if Path::new(&display).is_absolute() {
        PathBuf::from(display)
    } else {
        PathBuf::from(
            environment
                .get(OsStr::new("XDG_RUNTIME_DIR"))
                .ok_or("XDG_RUNTIME_DIR is required")?,
        )
        .join(display)
    };
    environment.insert("XDG_RUNTIME_DIR".into(), runtime.clone().into_os_string());
    environment.insert("WAYLAND_DISPLAY".into(), outer.into_os_string());
    environment.insert("LINCE_PARK_TRACE_FRAMES".into(), "1".into());
    for key in ["WAYLAND_SOCKET", "LINCE_PARK_BOOTSTRAP_PID"] {
        environment.remove(OsStr::new(key));
    }
    options
        .values
        .insert("--marionette-port".into(), marionette.to_string());
    options
        .values
        .insert("--webdriver-port".into(), webdriver.to_string());
    let before = manager_environment();
    let mut result = json!({"directory":run,"functional_automation":"running","physical_acceptance":"not run","user_manager_before":before,"cpu_ticks_per_second":String::from_utf8(checked(Command::new("getconf").arg("CLK_TCK"),5)?).map_err(err)?.trim().parse::<u64>().map_err(err)?});
    let mut children = Vec::new();
    let mut driver = Driver::new(webdriver)?;
    match live_work(
        &options,
        &run,
        &environment,
        &mut children,
        &mut driver,
        &mut result,
    ) {
        Ok(_) => result["functional_automation"] = json!("passed"),
        Err(error) => {
            result["functional_automation"] = json!("failed");
            result["error"] = json!(error);
        }
    }
    write(&run.join("results.json"), &result)?;
    if let Err(error) = orderly_cleanup(&options, &run, &mut children, &mut result) {
        result["cleanup_error"] = json!(error);
        result["save_aware_logout"] = json!("unrun");
        if options.values.contains_key("--host-close") {
            result["functional_automation"] = json!("failed");
        }
    }
    result["cleanup_observations"]=json!(children.iter_mut().map(|child|json!({"pid":child.0.id(),"exit_before_cleanup":child.0.try_wait().ok().flatten().and_then(|s|s.code())})).collect::<Vec<_>>());
    while children.pop().is_some() {}
    let after = manager_environment();
    result["user_manager_after"] = json!(after);
    result["user_manager_unchanged"] =
        json!(before.as_ref().zip(after.as_ref()).map(|(a, b)| a == b));
    if result["user_manager_unchanged"] == false {
        result["functional_automation"] = json!("failed");
    }
    let _ = fs::remove_dir(&runtime);
    write(&run.join("results.json"), &result)?;
    let mut summary = result.clone();
    if let Some(map) = summary.as_object_mut() {
        map.remove("resources");
        map.remove("browser");
    }
    println!("{}", serde_json::to_string_pretty(&summary).map_err(err)?);
    ensure(
        result["functional_automation"] == "passed",
        format!(
            "Installed browser validation failed; see {}",
            run.join("results.json").display()
        ),
    )
}

fn configuration_destination(value: &Value) -> Result<&Path> {
    let relative = Path::new(value.as_str().ok_or("Missing configuration destination")?);
    ensure(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "Provider configuration destination must stay within config",
    )?;
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_uris_escape_webdriver_paths() {
        assert_eq!(
            uri(Path::new("/tmp/a #b.html")),
            "file:///tmp/a%20%23b.html"
        );
    }
    #[test]
    fn lifecycle_handles_compare_as_sets() {
        assert!(same_handles(&json!(["b", "a"]), &json!(["a", "b"])));
        assert!(!same_handles(&json!(["b", "a"]), &json!(["a"])));
        assert!(!same_handles(&Value::Null, &Value::Null));
    }
    #[test]
    fn option_parser_rejects_unsupported_and_out_of_range_requests_before_launch() {
        let args = |tail: &[&str]| {
            std::iter::once(OsString::from("/bin/true"))
                .chain(tail.iter().map(OsString::from))
                .collect::<Vec<_>>()
        };
        assert!(Options::parse(&args(&["--unknown"])).is_err());
        assert!(
            Options::parse(&args(&[
                "--seconds",
                "0",
                "--geckodriver",
                "x",
                "--capture-probe",
                "x",
                "--fixtures",
                "x"
            ]))
            .unwrap_err()
            .contains("--seconds")
        );
    }
    #[test]
    fn provider_configuration_cannot_escape_its_private_directory() {
        for path in [
            "",
            "../secret",
            "/tmp/secret",
            "nested/../../secret",
            "./config",
        ] {
            assert!(configuration_destination(&json!(path)).is_err(), "{path}");
        }
        assert_eq!(
            configuration_destination(&json!("bar/config.json")).unwrap(),
            Path::new("bar/config.json")
        );
    }
    #[test]
    fn graphics_environment_restores_the_host_markers() {
        let environment = restore_host_environment(
            [
                ("LD_LIBRARY_PATH".into(), "wrapped".into()),
                ("LINCE_HOST_LD_LIBRARY_PATH".into(), "1original".into()),
                ("GBM_BACKENDS_PATH".into(), "wrapped".into()),
                ("LINCE_HOST_GBM_BACKENDS_PATH".into(), "0".into()),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            environment.get(OsStr::new("LD_LIBRARY_PATH")),
            Some(&OsString::from("original"))
        );
        assert!(!environment.contains_key(OsStr::new("GBM_BACKENDS_PATH")));
        assert!(!environment.contains_key(OsStr::new("LINCE_HOST_LD_LIBRARY_PATH")));
    }
    #[test]
    fn webdriver_decodes_success_and_rejects_protocol_errors() {
        use std::io::Write;
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            for (status, body) in [
                ("200 OK", r#"{"value":{"ready":true}}"#),
                (
                    "500 Internal Server Error",
                    r#"{"value":{"error":"unknown error","message":"fixture"}}"#,
                ),
            ] {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                write!(socket,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        let driver = Driver::new(port).unwrap();
        assert_eq!(
            driver.request("GET", "/status", None).unwrap(),
            json!({"ready":true})
        );
        assert!(
            driver
                .request("GET", "/status", None)
                .unwrap_err()
                .contains("fixture")
        );
        server.join().unwrap();
    }
    #[test]
    fn external_program_timeout_terminates_its_process_group() {
        let started = Instant::now();
        assert!(
            output(Command::new("sh").args(["-c", "sleep 30 & wait"]), 1)
                .unwrap_err()
                .contains("timed out")
        );
        assert!(started.elapsed() < Duration::from_secs(8));
    }
}
