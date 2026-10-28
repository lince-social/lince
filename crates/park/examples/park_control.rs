#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use lince_interface::host::control::{Arguments, Entry, Error, Invocation, Response};
    use lince_park::control_client::ControlClient;
    use std::time::{Duration, Instant};
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|argument| argument == "--help") {
        println!("park_control snapshot | watch | invoke COMMAND [WORKSPACE_ID]\nUse an authorized native Wayland helper connection. Commands are sent once; failures are never retried automatically.");
        return Ok(());
    }
    let mode = arguments.first().map(String::as_str).ok_or("Choose snapshot, watch or invoke")?;
    if !matches!(mode, "snapshot" | "watch" | "invoke") || mode == "invoke" && !(2..=3).contains(&arguments.len()) || mode != "invoke" && arguments.len() != 1 {
        return Err("Use snapshot, watch or invoke COMMAND [WORKSPACE_ID]".into());
    }
    let mut client = ControlClient::connect()?;
    if mode == "invoke" {
        let deadline = Instant::now() + Duration::from_secs(5);
        let context = loop {
            match client.snapshot(0, 0)? {
                Response::Snapshot { entries, .. } => {
                    if entries.iter().any(|entry| matches!(entry, Entry::Session { locked: true, .. })) { return Err("Desktop commands are paused while Park is locked".into()); }
                    if entries.iter().any(|entry| matches!(entry, Entry::Session { active: false, .. })) { return Err("The Park seat or nested view is inactive".into()); }
                    if let Some(context) = entries.into_iter().find_map(|entry| match entry { Entry::Context { value } => value, _ => None }) { break context; }
                }
                response => return Err(format!("Could not read the current desktop context: {response:?}").into()),
            }
            if Instant::now() >= deadline { return Err("The desktop context is not ready".into()); }
            std::thread::sleep(Duration::from_millis(25));
        };
        let invocation = Invocation { command: arguments[1].clone(), context,
            arguments: arguments.get(2).map(|id| id.parse().map(|id| Arguments::Workspace { id })).transpose()?.unwrap_or_default() };
        let response = client.invoke(&invocation)?;
        println!("{}", serde_json::to_string(&response)?);
        if let Response::Failed { failure } = response { return Err(format!("{:?}: {}", failure.code, failure.message).into()); }
        return Ok(());
    }
    if mode == "watch" { client.subscribe(true)?; }
    loop {
        let mut revision = 0;
        let mut cursor = 0;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let response = client.snapshot(revision, cursor)?;
            println!("{}", serde_json::to_string(&response)?);
            match response {
                Response::Snapshot { revision: observed, next: Some(next), .. } => { revision = observed; cursor = next; }
                Response::Snapshot { next: None, .. } => break,
                Response::Failed { failure } if failure.code == Error::StaleTarget && Instant::now() < deadline => {
                    revision = 0;
                    cursor = 0;
                    std::thread::sleep(Duration::from_millis(16));
                }
                Response::Failed { failure } if failure.code == Error::RateLimited && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(16));
                }
                Response::Failed { failure } => return Err(format!("{:?}: {}", failure.code, failure.message).into()),
                _ => return Err("An incompatible snapshot response was returned".into()),
            }
        }
        if mode != "watch" { return Ok(()); }
        loop {
            match client.next_change(Duration::from_secs(60)) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}
