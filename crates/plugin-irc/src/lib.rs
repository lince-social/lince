use nucleus::plugins::{
    Compatibility, Contribution, Field, FieldType, ImportedRecord, Manifest, PreparedPublication,
    PublishRecord,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::{Mutex, mpsc, oneshot},
};
use tokio_rustls::{TlsConnector, rustls};
use zeroize::Zeroizing;

fn field(id: &str, name: &str, kind: FieldType, default: Value) -> Field {
    Field {
        id: id.into(),
        name: name.into(),
        kind,
        required: true,
        default,
    }
}

pub fn manifest(executable: String) -> Manifest {
    Manifest {
        format: nucleus::plugins::FORMAT.into(), id: "lince.irc".into(), name: "IRC channel messages".into(),
        version: 1, compatibility: Compatibility::native(), executable, arguments: vec![],
        contributions: vec![
            Contribution::Sync {
                id: "messages".into(), name: "IRC channel messages".into(), incoming: true, outgoing: true,
                replay_safe: false, schema: "message".into(), document: None,
                settings: vec![
                    field("server", "IRC server URL (ircs://host:6697)", FieldType::Text, json!("ircs://irc.libera.chat:6697")),
                    field("nickname", "Nickname dedicated to Lince", FieldType::Text, json!("lince-cell")),
                    field("channel", "Channel", FieldType::Text, json!("#lince")),
                    field("password_file", "Server password file (absolute path, or empty)", FieldType::Text, json!("")),
                ],
            },
            Contribution::Schema {
                id: "message".into(), name: "IRC channel message".into(), namespace: "plugin.lince.irc.message".into(),
                fields: vec![
                    field("author", "Nickname", FieldType::Text, Value::Null),
                    field("channel", "Channel", FieldType::Text, Value::Null),
                    field("server", "Server", FieldType::Text, Value::Null),
                    field("published_at", "Server timestamp", FieldType::Text, Value::Null),
                ],
            },
        ],
        licenses: vec![nucleus::sand_package::License { name: "Lince — MIT".into(), text: include_str!("../../../LICENSE").into() }, nucleus::sand_package::License { name: "reqwest — MIT".into(), text: include_str!("../licenses/reqwest-MIT.txt").into() }, nucleus::sand_package::License { name: "tokio-rustls — MIT".into(), text: include_str!("../licenses/tokio-rustls-MIT.txt").into() }, nucleus::sand_package::License { name: "rustls — MIT".into(), text: include_str!("../licenses/rustls-MIT.txt").into() }, nucleus::sand_package::License { name: "webpki-roots — CDLA-Permissive-2.0".into(), text: include_str!("../licenses/webpki-roots-CDLA.txt").into() }],
        credits: vec!["IRC client protocol: IETF RFC 2812. Optional server timestamps: IRCv3 server-time. Incoming messages are received only while connected.".into()],
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    server: String,
    nickname: String,
    channel: String,
    password_file: String,
}

impl Settings {
    fn parse(value: Value) -> Result<Self, String> {
        let settings: Self = serde_json::from_value(value).map_err(|_| "Invalid IRC settings")?;
        let url = Url::parse(&settings.server).map_err(|_| "Invalid IRC server URL")?;
        let local = url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if !(url.scheme() == "ircs" || (url.scheme() == "irc" && local))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
            || settings.nickname.is_empty()
            || settings.nickname.len() > 30
            || settings
                .nickname
                .bytes()
                .any(|byte| !byte.is_ascii_alphanumeric() && !b"_-[]\\`^{}|".contains(&byte))
            || settings.nickname.as_bytes()[0].is_ascii_digit()
            || !settings.channel.starts_with('#')
            || settings.channel.len() < 2
            || settings.channel.len() > 50
            || settings.channel[1..]
                .bytes()
                .any(|byte| !byte.is_ascii_alphanumeric() && !b"_-".contains(&byte))
            || (!settings.password_file.is_empty()
                && !Path::new(&settings.password_file).is_absolute())
        {
            return Err("Choose a TLS IRC server, a valid nickname, a #channel and an absolute password-file path".into());
        }
        Ok(settings)
    }
    fn key(&self) -> String {
        hash(&json!([self.server, self.nickname, self.password_file]))
    }
    fn scope(&self) -> String {
        format!("{}/{}", self.key(), self.channel.to_ascii_lowercase())
    }
    async fn password(&self) -> Result<Zeroizing<String>, String> {
        if self.password_file.is_empty() {
            return Ok(Zeroizing::new(String::new()));
        }
        let metadata = tokio::fs::metadata(&self.password_file)
            .await
            .map_err(|_| "Cannot read IRC password file")?;
        if !metadata.is_file() || metadata.len() > 512 {
            return Err("IRC password file must be at most 512 bytes".into());
        }
        let password = Zeroizing::new(
            tokio::fs::read_to_string(&self.password_file)
                .await
                .map_err(|_| "Cannot read IRC password file")?,
        );
        if password.trim().is_empty() || password.trim().chars().any(char::is_whitespace) {
            return Err("Password file must contain one IRC server password".into());
        }
        Ok(password)
    }
}

fn hash(value: &Value) -> String {
    Sha256::digest(value.to_string().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Serialize, Deserialize)]
struct Message {
    sequence: u64,
    scope: String,
    record: ImportedRecord,
}

#[derive(Default, Serialize, Deserialize)]
struct Inbox {
    sequence: u64,
    records: VecDeque<Message>,
    dropped: BTreeMap<String, u64>,
}

struct Shared {
    path: PathBuf,
    inbox: Mutex<Inbox>,
    errors: Mutex<BTreeMap<String, String>>,
}

enum Operation {
    Join(String, oneshot::Sender<Result<(), String>>),
    Send(String, String, oneshot::Sender<Result<(), String>>),
}

struct Connection {
    sender: mpsc::Sender<Operation>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub struct Runtime {
    shared: Arc<Shared>,
    connections: Mutex<BTreeMap<String, Connection>>,
}

impl Runtime {
    pub async fn open(state: PathBuf) -> Result<Self, String> {
        tokio::fs::create_dir_all(&state)
            .await
            .map_err(|error| error.to_string())?;
        let path = state.join("inbox.json");
        if tokio::fs::metadata(&path)
            .await
            .is_ok_and(|metadata| metadata.len() > 4 * 1024 * 1024)
        {
            return Err("Retained IRC inbox exceeds its size limit".into());
        }
        let inbox = match tokio::fs::read(&path).await {
            Ok(bytes) if bytes.len() <= 4 * 1024 * 1024 => {
                serde_json::from_slice::<Inbox>(&bytes).map_err(|_| "Invalid retained IRC inbox")?
            }
            Ok(_) => return Err("Retained IRC inbox exceeds its size limit".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Inbox::default(),
            Err(_) => return Err("Cannot read retained IRC inbox".into()),
        };
        if inbox.records.len() > 1024 || inbox.dropped.len() > 256 {
            return Err("Retained IRC inbox exceeds its item limit".into());
        }
        Ok(Self {
            shared: Arc::new(Shared {
                path,
                inbox: Mutex::new(inbox),
                errors: Mutex::new(BTreeMap::new()),
            }),
            connections: Mutex::new(BTreeMap::new()),
        })
    }

    async fn join(&self, settings: &Settings) -> Result<mpsc::Sender<Operation>, String> {
        let mut connections = self.connections.lock().await;
        let key = settings.key();
        if connections
            .get(&key)
            .is_some_and(|connection| connection.task.is_finished())
        {
            connections.remove(&key);
        }
        if !connections.contains_key(&key) {
            if connections.len() >= 16 {
                return Err("At most 16 IRC connections may be active".into());
            }
            let socket = tokio::time::timeout(Duration::from_secs(15), connect(settings))
                .await
                .map_err(|_| "IRC connection timed out")??;
            let (sender, receiver) = mpsc::channel(16);
            let settings = settings.clone();
            let shared = self.shared.clone();
            let error_key = key.clone();
            let task = tokio::spawn(async move {
                if let Err(error) = session(socket, settings, shared.clone(), receiver).await {
                    shared.errors.lock().await.insert(error_key, error);
                }
            });
            connections.insert(key.clone(), Connection { sender, task });
        }
        let sender = connections.get(&key).unwrap().sender.clone();
        drop(connections);
        let (reply, result) = oneshot::channel();
        sender
            .send(Operation::Join(settings.channel.clone(), reply))
            .await
            .map_err(|_| "IRC connection closed")?;
        tokio::time::timeout(Duration::from_secs(15), result)
            .await
            .map_err(|_| "IRC channel join timed out")?
            .map_err(|_| "IRC connection closed during join")??;
        Ok(sender)
    }

    pub async fn handle(
        &self,
        contribution: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, String> {
        if contribution != "messages" {
            return Err("Unknown contribution".into());
        }
        let settings = Settings::parse(input["settings"].clone())?;
        match operation {
            "validate" => {
                settings.password().await?;
                Ok(json!({"valid":true}))
            }
            "pull" => {
                self.join(&settings).await?;
                if let Some(error) = self.shared.errors.lock().await.remove(&settings.key()) {
                    return Err(error);
                }
                let cursor = input["cursor"].as_u64().unwrap_or(0);
                let scope = settings.scope();
                let inbox = self.shared.inbox.lock().await;
                if !input["cursor"].is_null()
                    && inbox
                        .dropped
                        .get(&scope)
                        .is_some_and(|floor| cursor < *floor)
                {
                    return Err("IRC receive buffer filled before this binding refreshed; messages were lost".into());
                }
                let records: Vec<_> = inbox
                    .records
                    .iter()
                    .filter(|message| message.scope == scope && message.sequence > cursor)
                    .take(nucleus::plugins::MAX_ITEMS)
                    .collect();
                let cursor = records
                    .last()
                    .map(|message| message.sequence)
                    .unwrap_or(cursor);
                Ok(
                    json!({"records":records.into_iter().map(|message| &message.record).collect::<Vec<_>>(),"cursor":cursor}),
                )
            }
            "prepare" => {
                let record: PublishRecord = serde_json::from_value(input["record"].clone())
                    .map_err(|_| "Invalid publication Record")?;
                let content = if record.body.is_empty() {
                    record.head.clone()
                } else {
                    format!("{} — {}", record.head, record.body)
                };
                if content.trim().is_empty()
                    || content.chars().any(char::is_control)
                    || format!("PRIVMSG {} :{}\r\n", settings.channel, content).len() > 512
                {
                    return Err("IRC publication must fit one line of at most 512 bytes including its command".into());
                }
                let id = hash(&json!([record.uid, settings.scope(), content]));
                Ok(
                    json!({"record":{"external_id":id,"head":record.head,"body":record.body,"metadata":{"author":settings.nickname,"channel":settings.channel,"server":settings.server,"published_at":""}},"payload":{"channel":settings.channel,"content":content}}),
                )
            }
            "push" => {
                let prepared: PreparedPublication =
                    serde_json::from_value(input["prepared"].clone())
                        .map_err(|_| "Invalid prepared IRC publication")?;
                let content = prepared.payload["content"]
                    .as_str()
                    .ok_or("Missing IRC publication content")?;
                if prepared.payload["channel"] != settings.channel
                    || content.trim().is_empty()
                    || content.chars().any(char::is_control)
                    || format!("PRIVMSG {} :{}\r\n", settings.channel, content).len() > 512
                {
                    return Err("Invalid IRC publication line or channel".into());
                }
                let sender = self.join(&settings).await?;
                let (reply, result) = oneshot::channel();
                sender
                    .send(Operation::Send(
                        settings.channel.clone(),
                        content.into(),
                        reply,
                    ))
                    .await
                    .map_err(|_| "IRC connection closed before sending")?;
                result.await.map_err(
                    |_| "IRC connection closed while sending; do not replay automatically",
                )??;
                Ok(
                    json!({"accepted":true,"confirmed":false,"detail":"Written to the IRC connection; recipient delivery is not confirmed. Uncertain messages cannot be safely replayed."}),
                )
            }
            _ => Err("Unknown operation".into()),
        }
    }
}

trait Socket: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Socket for T {}
type Stream = Box<dyn Socket>;

async fn connect(settings: &Settings) -> Result<Stream, String> {
    let url = Url::parse(&settings.server).map_err(|_| "Invalid IRC URL")?;
    let host = url
        .host_str()
        .ok_or("Missing IRC server")?
        .trim_matches(['[', ']']);
    let socket = TcpStream::connect((
        host,
        url.port()
            .unwrap_or(if url.scheme() == "ircs" { 6697 } else { 6667 }),
    ))
    .await
    .map_err(|_| "Cannot connect to IRC server")?;
    if url.scheme() == "irc" {
        return Ok(Box::new(socket));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| "Cannot configure IRC TLS")?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|_| "Invalid IRC TLS hostname")?;
    Ok(Box::new(
        TlsConnector::from(Arc::new(config))
            .connect(name, socket)
            .await
            .map_err(|_| "IRC TLS certificate or handshake failed")?,
    ))
}

struct Line {
    tags: BTreeMap<String, String>,
    prefix: String,
    command: String,
    parameters: Vec<String>,
}

fn parse(line: &str) -> Result<Line, String> {
    if line.len() > 8192 || !line.ends_with('\n') {
        return Err("IRC line exceeds its limit or is incomplete".into());
    }
    let mut text = line.trim_end_matches(['\r', '\n']);
    let mut tags = BTreeMap::new();
    if text.starts_with('@') {
        let (raw, rest) = text.split_once(' ').ok_or("Invalid IRC tags")?;
        for item in raw[1..].split(';').take(64) {
            let (key, value) = item.split_once('=').unwrap_or((item, ""));
            tags.insert(key.into(), value.into());
        }
        text = rest.trim_start();
    }
    let mut prefix = String::new();
    if text.starts_with(':') {
        let (raw, rest) = text.split_once(' ').ok_or("Invalid IRC prefix")?;
        prefix = raw[1..].into();
        text = rest.trim_start();
    }
    let (command, mut rest) = text.split_once(' ').unwrap_or((text, ""));
    let mut parameters = Vec::new();
    while !rest.is_empty() && parameters.len() < 15 {
        rest = rest.trim_start();
        if let Some(trailing) = rest.strip_prefix(':') {
            parameters.push(trailing.into());
            break;
        }
        let (parameter, tail) = rest.split_once(' ').unwrap_or((rest, ""));
        parameters.push(parameter.into());
        rest = tail;
    }
    Ok(Line {
        tags,
        prefix,
        command: command.into(),
        parameters,
    })
}

async fn session(
    socket: Stream,
    settings: Settings,
    shared: Arc<Shared>,
    mut operations: mpsc::Receiver<Operation>,
) -> Result<(), String> {
    let (read, mut write) = tokio::io::split(socket);
    let mut reader = BufReader::new(read);
    let password = settings.password().await?;
    if !password.is_empty() {
        write
            .write_all(format!("PASS {}\r\n", password.trim()).as_bytes())
            .await
            .map_err(|_| "IRC authentication connection closed")?;
    }
    write
        .write_all(
            format!(
                "CAP LS 302\r\nNICK {}\r\nUSER {} 0 * :Lince\r\n",
                settings.nickname, settings.nickname
            )
            .as_bytes(),
        )
        .await
        .map_err(|_| "IRC registration connection closed")?;
    let mut registered = false;
    let mut pending: BTreeMap<String, Vec<oneshot::Sender<Result<(), String>>>> = BTreeMap::new();
    let mut joined = BTreeSet::new();
    let mut line = Vec::new();
    let mut budget = tokio::time::Instant::now();
    let mut count = 0;
    loop {
        tokio::select! {
            operation = operations.recv(), if registered => {
                match operation {
                    Some(Operation::Join(channel, reply)) => {
                        let key = channel.to_ascii_lowercase();
                        if joined.contains(&key) { let _ = reply.send(Ok(())); continue; }
                        if joined.len() + pending.len() >= 32 { let _ = reply.send(Err("At most 32 channels may share an IRC connection".into())); continue; }
                        if !pending.contains_key(&key) { write.write_all(format!("JOIN {channel}\r\n").as_bytes()).await.map_err(|_| "IRC channel connection closed")?; }
                        pending.entry(key).or_default().push(reply);
                    }
                    Some(Operation::Send(channel, content, reply)) => {
                        let result = write.write_all(format!("PRIVMSG {channel} :{content}\r\n").as_bytes()).await.map_err(|_| "IRC send interrupted; delivery is unknown".to_owned());
                        let failed = result.is_err(); let _ = reply.send(result);
                        if failed { return Err("IRC send interrupted".into()); }
                    }
                    None => { let _ = write.write_all(b"QUIT :Lince connection stopped\r\n").await; return Ok(()); }
                }
            }
            result = async { let remaining = 8193usize.saturating_sub(line.len()); (&mut reader).take(remaining as u64).read_until(b'\n', &mut line).await } => {
                let size = result.map_err(|_| "IRC connection interrupted")?;
                if size == 0 { return Err("IRC connection closed; reconnect on the next refresh".into()); }
                if budget.elapsed() >= Duration::from_secs(1) { budget = tokio::time::Instant::now(); count = 0; }
                count += 1;
                if count > 512 || line.len() > 8192 { return Err("IRC server exceeded its message budget".into()); }
                let message = parse(std::str::from_utf8(&line).map_err(|_| "IRC message is not UTF-8")?)?;
                line.clear();
                match message.command.as_str() {
                    "PING" => { let ping = message.parameters.last().ok_or("Invalid IRC ping")?; write.write_all(format!("PONG :{ping}\r\n").as_bytes()).await.map_err(|_| "IRC ping connection closed")?; }
                    "CAP" => {
                        if message.parameters.get(1).is_some_and(|parameter| parameter == "LS") && !message.parameters.get(2).is_some_and(|parameter| parameter == "*") {
                            let available = message.parameters.last().map(String::as_str).unwrap_or("");
                            if available.split_whitespace().any(|capability| capability == "server-time") { write.write_all(b"CAP REQ :server-time\r\n").await.map_err(|_| "IRC capability connection closed")?; }
                            else { write.write_all(b"CAP END\r\n").await.map_err(|_| "IRC capability connection closed")?; }
                        } else if message.parameters.get(1).is_some_and(|parameter| parameter == "ACK" || parameter == "NAK") { write.write_all(b"CAP END\r\n").await.map_err(|_| "IRC capability connection closed")?; }
                    }
                    "001" => { registered = true; }
                    "JOIN" if message.prefix.split('!').next().is_some_and(|nick| nick.eq_ignore_ascii_case(&settings.nickname)) => {
                        let channel = message.parameters.first().ok_or("Invalid IRC join")?.to_ascii_lowercase(); joined.insert(channel.clone());
                        for reply in pending.remove(&channel).unwrap_or_default() { let _ = reply.send(Ok(())); }
                    }
                    "PRIVMSG" if message.parameters.len() == 2 => {
                        let channel = message.parameters[0].to_ascii_lowercase();
                        if !joined.contains(&channel) { continue; }
                        let author = message.prefix.split('!').next().unwrap_or("");
                        if author.eq_ignore_ascii_case(&settings.nickname) { continue; }
                        let content = &message.parameters[1];
                        if content.len() > 16384 { return Err("IRC message content is too large".into()); }
                        let mut inbox = shared.inbox.lock().await;
                        inbox.sequence = inbox.sequence.checked_add(1).ok_or("IRC receive sequence exhausted")?;
                        let sequence = inbox.sequence;
                        let external_id = message.tags.get("msgid").cloned().unwrap_or_else(|| nucleus::new_uid("r"));
                        let record = ImportedRecord { external_id: format!("{}:{external_id}", settings.key()), head: format!("{author} in {channel}"), body: content.clone(), metadata: json!({"author":author,"channel":channel,"server":settings.server,"published_at":message.tags.get("time").cloned().unwrap_or_default()}) };
                        inbox.records.push_back(Message { sequence, scope: format!("{}/{channel}", settings.key()), record });
                        while inbox.records.len() > 1024 { let dropped = inbox.records.pop_front().unwrap(); inbox.dropped.insert(dropped.scope, dropped.sequence); }
                        if inbox.dropped.len() > 256 { return Err("IRC checkpoint scope limit reached".into()); }
                        let bytes = serde_json::to_vec(&*inbox).map_err(|_| "Cannot encode IRC inbox")?;
                        let staging = shared.path.with_extension("new");
                        tokio::fs::write(&staging, bytes).await.map_err(|_| "Cannot retain IRC inbox")?;
                        tokio::fs::rename(staging, &shared.path).await.map_err(|_| "Cannot commit IRC inbox")?;
                    }
                    "KICK" if message.parameters.get(1).is_some_and(|nick| nick.eq_ignore_ascii_case(&settings.nickname)) => { return Err("Lince was removed from the IRC channel".into()); }
                    "ERROR" => return Err("IRC server closed the connection".into()),
                    code if code.len() == 3 && code.starts_with('4') => {
                        for replies in std::mem::take(&mut pending).into_values() { for reply in replies { let _ = reply.send(Err(format!("IRC server refused the request ({code})"))); } }
                        if !registered { return Err(format!("IRC registration refused ({code}); review nickname and server access")); }
                    }
                    _ => {}
                }
            }
        }
    }
}
