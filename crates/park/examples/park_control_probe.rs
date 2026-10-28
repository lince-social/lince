#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|argument| argument == "--help") {
        println!("park_control_probe REPORT SESSION_PROBE: finite SDK acceptance through an owner-authorized Wayland helper connection, using private native windows");
        return Ok(());
    }
    let [report, native] = arguments.as_slice() else { return Err("Use park_control_probe REPORT SESSION_PROBE".into()); };
    probe::run(std::path::Path::new(report), std::path::Path::new(native))?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod probe {
    use lince_interface::host::control::{Arguments, Context, Disposition, Entry, Error, Invocation, Response};
    use lince_park::control_client::ControlClient;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{fs::{File, OpenOptions}, io::{self, Read, Write}, os::unix::fs::{DirBuilderExt, OpenOptionsExt}, path::{Path, PathBuf}, process::{Child, Command, Stdio}, time::{Duration, Instant}};

    struct Native(Child);

    impl Drop for Native {
        fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
    }

    fn fail(message: impl Into<String>) -> io::Error { io::Error::other(message.into()) }

    fn digest(path: &Path) -> io::Result<String> {
        let mut file = File::open(path)?;
        let mut hash = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let length = file.read(&mut bytes)?;
            if length == 0 { return Ok(hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect()); }
            hash.update(&bytes[..length]);
        }
    }

    fn identity(pid: u32) -> io::Result<Value> {
        let base = PathBuf::from(format!("/proc/{pid}"));
        let stat = std::fs::read_to_string(base.join("stat"))?;
        let start = stat.rsplit_once(") ").and_then(|(_, fields)| fields.split_whitespace().nth(19)).and_then(|value| value.parse::<u64>().ok()).ok_or_else(|| fail("The process start time is unavailable"))?;
        let arguments = std::fs::read(base.join("cmdline"))?;
        let arguments = arguments.split(|byte| *byte == 0).collect::<Vec<_>>();
        let directory = arguments.windows(2).find(|pair| pair[0] == b"--directory").map(|pair| std::str::from_utf8(pair[1]).map(str::to_owned)).transpose().map_err(io::Error::other)?;
        Ok(json!({"pid":pid,"start":start,"payload":std::fs::read_link(base.join("exe"))?,"sha256":digest(&base.join("exe"))?,"data_directory":directory}))
    }

    fn snapshot(client: &mut ControlClient) -> io::Result<(u64, Vec<Entry>)> {
        let mut revision = 0;
        let mut cursor = 0;
        let mut entries = Vec::new();
        let mut length = 0;
        for _ in 0..64 {
            let response = client.snapshot(revision, cursor)?;
            length += serde_json::to_vec(&response).map_err(io::Error::other)?.len();
            if length > 768 * 1024 { return Err(fail("The bounded probe snapshot exceeded its report budget")); }
            let Response::Snapshot { revision: next_revision, next, entries: page } = response else { return Err(fail(format!("The snapshot refused its captured revision: {response:?}"))); };
            if next_revision == 0 || revision != 0 && revision != next_revision { return Err(fail("The snapshot spliced different revisions")); }
            revision = next_revision;
            entries.extend(page);
            if entries.len() > 8192 { return Err(fail("The probe snapshot exceeded its entry budget")); }
            let Some(next) = next else { return Ok((revision, entries)); };
            if next <= cursor { return Err(fail("The snapshot cursor did not advance")); }
            cursor = next;
        }
        Err(fail("The snapshot did not finish within 64 pages"))
    }

    fn context(entries: &[Entry]) -> io::Result<Context> {
        if !entries.iter().any(|entry| matches!(entry, Entry::Session { locked: false, active: true })) { return Err(fail("The Park seat is not active and unlocked")); }
        entries.iter().find_map(|entry| match entry { Entry::Context { value: Some(context) } => Some(context.clone()), _ => None }).ok_or_else(|| fail("Park has not published a current command context"))
    }

    fn invoke(client: &mut ControlClient, report: &mut Value, tag: &str, command: &str, context: Context, expected: Option<Error>) -> io::Result<()> {
        let invocation = Invocation { command: command.into(), context, arguments: Arguments::None };
        let started = Instant::now();
        let result = client.invoke(&invocation);
        match result {
            Ok(response) => {
                let accepted = match (&response, expected) {
                    (Response::Complete { outcome: Disposition::Applied }, None) => true,
                    (Response::Failed { failure }, Some(expected)) => failure.code == expected,
                    _ => false,
                };
                report["calls"].as_array_mut().unwrap().push(json!({"tag":tag,"invocation":invocation,"response":response,"elapsed_ms":started.elapsed().as_millis()}));
                if accepted { Ok(()) } else { Err(fail(format!("The {tag} invocation did not return its required actual outcome"))) }
            }
            Err(error) => {
                report["calls"].as_array_mut().unwrap().push(json!({"tag":tag,"invocation":invocation,"transport_error":error.to_string(),"outcome_unknown":true,"elapsed_ms":started.elapsed().as_millis()}));
                Err(error)
            }
        }
    }

    fn exercise(report: &mut Value, directory: &Path, native: &Path) -> io::Result<()> {
        let connection = wayland_client::Connection::connect_to_env().map_err(io::Error::other)?;
        let peer = rustix::net::sockopt::socket_peercred(&connection)?;
        if peer.uid != rustix::process::geteuid() { return Err(fail("The private compositor connection belongs to another user")); }
        report["compositor"] = identity(peer.pid.as_raw_nonzero().get() as u32)?;
        report["probe"] = identity(std::process::id())?;
        report["protocol_sha256"] = json!(Sha256::digest(include_bytes!("../protocols/lince-park-control-v1.xml")).iter().map(|byte| format!("{byte:02x}")).collect::<String>());
        let socket = std::env::var_os("WAYLAND_DISPLAY").map(PathBuf::from).ok_or_else(|| fail("The helper has no Park display"))?;
        let socket = if socket.is_absolute() { socket } else { PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| fail("The display runtime directory is unavailable"))?).join(socket) };
        report["socket"] = json!(socket);
        let mut client = ControlClient::from_connection(connection)?;
        client.subscribe(true)?;
        let (revision, scope) = client.next_change(Duration::from_secs(5))?;
        if scope & lince_interface::host::control::CHANGE_RESYNC == 0 { return Err(fail("The initial subscription did not require a snapshot")); }
        report["subscription"] = json!({"revision":revision,"scope":scope});
        let release = directory.join("automation-native.release");
        let log = OpenOptions::new().create_new(true).write(true).mode(0o600).open(directory.join("automation-native.log"))?;
        let mut child = Native(Command::new(native).args([std::ffi::OsStr::new("--hold-until"), release.as_os_str()]).env_remove("WAYLAND_SOCKET").stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log).spawn()?);
        let deadline = Instant::now() + Duration::from_secs(20);
        while !release.with_extension("ready").is_file() {
            if let Some(status) = child.0.try_wait()? { return Err(fail(format!("The native fixture exited before presenting its windows: {status}"))); }
            if Instant::now() >= deadline { return Err(fail("The native fixture did not present within its deadline")); }
            std::thread::sleep(Duration::from_millis(10));
        }
        let native_pid = std::fs::read_to_string(release.with_extension("ready"))?.parse::<u32>().map_err(io::Error::other)?;
        if native_pid != child.0.id() { return Err(fail("The native presentation witness belongs to another child")); }
        report["native"] = identity(native_pid)?;
        let (revision, before) = snapshot(&mut client)?;
        let original = context(&before)?;
        if before.iter().filter(|entry| matches!(entry, Entry::Window { app_id, .. } if app_id == "social.lince.park.validation")).count() != 2 { return Err(fail("The observer did not see both actual native fixture windows")); }
        if !before.iter().any(|entry| matches!(entry, Entry::Workspace { id, .. } if *id == original.workspace))
            || !before.iter().any(|entry| matches!(entry, Entry::Output { id, enabled: true, refresh_mhz: Some(refresh), modes, .. } if Some(*id) == original.output && *refresh > 0 && !modes.is_empty()))
        { return Err(fail("The original workspace and enabled output/refresh observation are incomplete")); }
        report["before"] = json!({"revision":revision,"entries":before});
        invoke(&mut client, report, "toggle", "layout.toggle-tiling", original.clone(), None)?;
        let (revision, toggled) = snapshot(&mut client)?;
        let restore = context(&toggled)?;
        if (restore.owner, &restore.canvas, restore.workspace, restore.output) != (original.owner, &original.canvas, original.workspace, original.output) { return Err(fail("The private original view changed; the probe will not toggle an unrelated workspace to restore it")); }
        report["after_toggle"] = json!({"revision":revision,"entries":toggled});
        invoke(&mut client, report, "restore", "layout.toggle-tiling", restore, None)?;
        let (revision, restored) = snapshot(&mut client)?;
        let current = context(&restored)?;
        report["after_restore"] = json!({"revision":revision,"entries":restored});
        invoke(&mut client, report, "deny-launch", "launch.launcher", current.clone(), Some(Error::Denied))?;
        invoke(&mut client, report, "deny-output", "focus.monitor-left", current, Some(Error::Denied))?;
        OpenOptions::new().create_new(true).write(true).mode(0o600).open(&release)?.sync_all()?;
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            if let Some(status) = child.0.try_wait()? {
                report["native_exit"] = json!(status.code());
                if !status.success() { return Err(fail(format!("The native lifecycle did not finish normally: {status}"))); }
                break;
            }
            if Instant::now() >= deadline { return Err(fail("The native lifecycle did not finish after release")); }
            std::thread::sleep(Duration::from_millis(10));
        }
        let (revision, closed) = snapshot(&mut client)?;
        if closed.iter().any(|entry| matches!(entry, Entry::Window { app_id, .. } if app_id == "social.lince.park.validation")) { return Err(fail("The native fixture windows survived normal child exit")); }
        report["after_close"] = json!({"revision":revision,"entries":closed});
        client.close()?;
        report["checks"] = json!(["authenticated-peer","bounded-snapshot","native-windows","workspace-output-refresh","subscription-resync","actual-command","explicit-restoration","denied-launch","denied-output","native-lifecycle","connection-closed"]);
        Ok(())
    }

    pub(super) fn run(path: &Path, native: &Path) -> io::Result<()> {
        if !path.is_absolute() || !native.is_absolute() || path.exists() { return Err(fail("Use new absolute private report and native-probe paths")); }
        let directory = path.with_extension("artifacts");
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let mut report = json!({"status":"failed","calls":[],"checks":[],"limits":"Actual SDK/worker/native lifecycle only; no pixel, frame-pacing, physical scanout, portal or provider restart acceptance."});
        let result = exercise(&mut report, &directory, native);
        match &result { Ok(()) => report["status"] = json!("passed"), Err(error) => report["error"] = json!(error.to_string()) }
        let temporary = path.with_extension("pending");
        let mut file = OpenOptions::new().create_new(true).write(true).mode(0o600).open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(&report).map_err(io::Error::other)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        std::fs::rename(temporary, path)?;
        result
    }
}
