use std::{
    collections::BTreeSet,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

use serde_json::{Value, from_slice, json, to_vec};

pub(super) struct Client {
    child: Child,
    input: ChildStdin,
    responses: Receiver<io::Result<Value>>,
    sequence: u64,
}

impl Client {
    pub(super) fn start(
        root: &Path,
        manifest: &Path,
        projects: BTreeSet<PathBuf>,
    ) -> io::Result<Self> {
        let mut child = Command::new("rust-analyzer")
            .current_dir(manifest.parent().unwrap_or(root))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let input = child.stdin.take().expect("analyzer stdin is piped");
        let mut output = BufReader::new(child.stdout.take().expect("analyzer stdout is piped"));
        let (sender, responses) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                let frame = read_frame(&mut output);
                let failed = frame.is_err();
                if sender.send(frame).is_err() || failed {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            responses,
            sequence: 0,
        };
        let response = client.call("initialize", json!({
            "rootUri": file_uri(manifest.parent().unwrap_or(root))?,
            "capabilities": {
                "general": { "positionEncodings": ["utf-8"] },
                "experimental": { "serverStatusNotification": true },
                "textDocument": { "codeAction": { "codeActionLiteralSupport": {
                    "codeActionKind": { "valueSet": ["refactor", "refactor.rewrite"] }
                }}}
            },
            "initializationOptions": {
                "linkedProjects": projects,
                "cachePriming": { "enable": false },
                "checkOnSave": false,
                "procMacro": { "enable": false },
                "cargo": {"buildScripts": {"enable": false}, "extraArgs": ["--locked", "--offline"],
                    "targetDir": root.join("target/sensei-analyzer")}
            }
        }))?;
        if response["capabilities"]["positionEncoding"] != "utf-8" {
            return Err(io::Error::other(
                "rust-analyzer did not negotiate UTF-8 positions",
            ));
        }
        client.notify("initialized", json!({}))?;
        loop {
            let message = client.receive()?;
            if message["method"] == "experimental/serverStatus"
                && message["params"]["quiescent"] == true
            {
                if message["params"]["health"] == "ok" {
                    return Ok(client);
                }
                return Err(io::Error::other(format!(
                    "rust-analyzer workspace status {}",
                    message["params"]
                )));
            }
        }
    }

    pub(super) fn call(&mut self, method: &str, params: Value) -> io::Result<Value> {
        let mut retries = 0;
        loop {
            self.sequence += 1;
            let id = self.sequence;
            self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":&params}))?;
            let response = loop {
                let response = self.receive()?;
                if response["method"].is_null() && response["id"] == id {
                    break response;
                }
            };
            let error = &response["error"];
            if error.is_null() {
                return Ok(response["result"].clone());
            }
            if retries == 4 || !matches!(error["code"].as_i64(), Some(-32802..=-32800)) {
                return Err(io::Error::other(format!("{method}: {error}")));
            }
            retries += 1;
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    pub(super) fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }

    fn send(&mut self, value: Value) -> io::Result<()> {
        let payload = to_vec(&value).map_err(io::Error::other)?;
        write!(self.input, "Content-Length: {}\r\n\r\n", payload.len())?;
        self.input.write_all(&payload)?;
        self.input.flush()
    }

    fn receive(&mut self) -> io::Result<Value> {
        let message = self
            .responses
            .recv_timeout(Duration::from_secs(90))
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => io::Error::new(io::ErrorKind::TimedOut, error),
                mpsc::RecvTimeoutError::Disconnected => {
                    io::Error::new(io::ErrorKind::BrokenPipe, error)
                }
            })??;
        if !message["method"].is_null() && !message["id"].is_null() {
            self.send(json!({"jsonrpc":"2.0","id":message["id"],"error":{
                "code":-32601,"message":"Sensei handles document refactor requests only"
            }}))?;
        }
        Ok(message)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_frame(reader: &mut impl BufRead) -> io::Result<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::other("rust-analyzer stopped"));
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some((header, value)) = line.split_once(':')
            && header.eq_ignore_ascii_case("Content-Length")
        {
            length = value.trim().parse().ok();
        }
    }
    let size = length.ok_or_else(|| io::Error::other("missing analyzer message length"))?;
    let mut payload = vec![0; size];
    reader.read_exact(&mut payload)?;
    from_slice(&payload).map_err(io::Error::other)
}

pub(super) fn file_uri(path: &Path) -> io::Result<String> {
    let text = path
        .to_str()
        .ok_or_else(|| io::Error::other("non-UTF-8 analyzer path"))?;
    let text = if cfg!(windows) {
        text.trim_start_matches(r"\\?\").replace('\\', "/")
    } else {
        text.to_owned()
    };
    let mut uri = String::from(if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'.' | b'_' | b'~') {
            uri.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(uri, "%{byte:02X}").expect("writing to String is infallible");
        }
    }
    Ok(uri)
}
