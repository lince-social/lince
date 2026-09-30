use super::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use tokio::{
    io::{AsyncWrite, BufReader},
    time::Instant,
};

#[derive(Clone)]
struct Opened {
    text: Rope,
    identity: u64,
    revision: u64,
    version: i64,
}

struct Pending {
    token: u64,
    path: PathBuf,
    document: Opened,
    position: Option<usize>,
    deadline: Instant,
}

struct Tasks(Vec<tokio::task::JoinHandle<()>>);

impl Drop for Tasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

fn emit(
    events: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
    event: Event,
) -> Result<()> {
    events
        .try_send(event)
        .map_err(|_| "Too many pending language server responses")?;
    wake();
    Ok(())
}

async fn notify(output: &mut (impl AsyncWrite + Unpin), method: &str, params: Value) -> Result<()> {
    protocol::write(
        output,
        json!({"jsonrpc":"2.0", "method":method, "params":params}),
    )
    .await
}

async fn server_request(
    output: &mut (impl AsyncWrite + Unpin),
    message: &Value,
    root: &str,
) -> Result<()> {
    let id = &message["id"];
    let result = match message["method"].as_str().unwrap_or_default() {
        "workspace/configuration" => Value::Array(message["params"]["items"].as_array().ok_or("Invalid configuration request")?.iter().take(64).map(|_| Value::Null).collect()),
        "workspace/workspaceFolders" => json!([{"uri":root,"name":"Project"}]),
        "window/workDoneProgress/create" => Value::Null,
        "workspace/applyEdit" => json!({"applied":false,"failureReason":"Apply edits through an editor action"}),
        _ => return protocol::write(output, json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Client method is not supported"}})).await,
    };
    protocol::write(output, json!({"jsonrpc":"2.0","id":id,"result":result})).await
}

pub(super) async fn run(
    command: Vec<String>,
    root: PathBuf,
    commands: mpsc::Receiver<Command>,
    events: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<()> {
    let mut process = crate::tooling::launch(&command, &root)?;
    let output = process.0.stdin.take().unwrap();
    let input = process.0.stdout.take().unwrap();
    let mut stderr = process.0.stderr.take().unwrap();
    let _tasks = Tasks(vec![tokio::spawn(async move {
        let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
    })]);
    serve(output, input, root, commands, events, wake).await
}

async fn serve(
    mut output: impl AsyncWrite + Unpin,
    input: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    root: PathBuf,
    mut commands: mpsc::Receiver<Command>,
    events: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<()> {
    let mut input = BufReader::new(input);
    let (sender, mut incoming) = mpsc::channel(8);
    let _tasks = Tasks(vec![tokio::spawn(async move {
        loop {
            let message = protocol::read(&mut input).await;
            let failed = message.is_err();
            if sender.send(message).await.is_err() || failed {
                break;
            }
        }
    })]);
    let root_uri = protocol::uri(&root)?;
    protocol::write(&mut output, json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
        "processId":std::process::id(),"clientInfo":{"name":"Lince"},"rootUri":root_uri,
        "workspaceFolders":[{"uri":root_uri,"name":"Project"}],
        "capabilities":{"general":{"positionEncodings":["utf-16"]},"workspace":{"configuration":true,"workspaceFolders":true,"applyEdit":false},"textDocument":{"synchronization":{"didSave":true},"completion":{"completionItem":{"snippetSupport":false}},"publishDiagnostics":{"versionSupport":true},"formatting":{"dynamicRegistration":false}}}
    }})).await?;
    let initialize = async {
        loop {
            let message = incoming
                .recv()
                .await
                .ok_or("Language server stopped during initialization")??;
            if message["method"].is_string() && !message["id"].is_null() {
                server_request(&mut output, &message, &root_uri).await?;
            } else if message["id"] == 0 {
                if !message["error"].is_null() {
                    return Err(format!("Initialization failed: {}", message["error"]));
                }
                break Ok(message["result"]["capabilities"].clone());
            }
        }
    };
    let capabilities: Value = tokio::time::timeout(Duration::from_secs(30), initialize)
        .await
        .map_err(|_| "Language server initialization timed out")??;
    if capabilities["positionEncoding"]
        .as_str()
        .is_some_and(|encoding| encoding != "utf-16")
    {
        return Err("This server does not support UTF-16 positions".into());
    }
    let sync = capabilities["textDocumentSync"]
        .as_u64()
        .or_else(|| capabilities["textDocumentSync"]["change"].as_u64())
        .unwrap_or(1);
    notify(&mut output, "initialized", json!({})).await?;
    emit(
        events,
        wake,
        Event::Ready {
            formatting: !capabilities["documentFormattingProvider"].is_null()
                && capabilities["documentFormattingProvider"] != false,
        },
    )?;
    let mut documents: BTreeMap<PathBuf, Opened> = BTreeMap::new();
    let mut pending: BTreeMap<u64, Pending> = BTreeMap::new();
    let mut next = 1;
    loop {
        let deadline = pending.values().map(|request| request.deadline).min();
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break; };
                match command {
                    Command::Sync { path, language, text, identity, revision } => {
                        if text.len_bytes() > crate::tooling::MAX_TOOL_BYTES { return Err("Language tools are limited to files up to 2 MiB".into()); }
                        let uri = protocol::uri(&path)?;
                        let previous = documents.remove(&path);
                        let mut version = previous.as_ref().map_or(1, |document| document.version + 1);
                        if let Some(document) = previous.as_ref().filter(|document| document.identity != identity) {
                            let _ = document;
                            notify(&mut output, "textDocument/didClose", json!({"textDocument":{"uri":uri}})).await?;
                        }
                        if let Some(document) = previous.filter(|document| document.identity == identity) {
                            let old = document.text.to_string();
                            let current = text.to_string();
                            if let Some(edit) = Edit::between(&old, &current, 0) {
                                let changes = if sync == 2 {
                                    json!([{"range":{"start":protocol::position(&document.text, edit.range.start),"end":protocol::position(&document.text, edit.range.end)},"text":edit.text}])
                                } else if sync == 1 { json!([{"text":current}]) }
                                else { return Err("Language server does not accept document changes".into()); };
                                notify(&mut output, "textDocument/didChange", json!({"textDocument":{"uri":uri,"version":version},"contentChanges":changes})).await?;
                            } else { version = document.version; }
                        } else {
                            notify(&mut output, "textDocument/didOpen", json!({"textDocument":{"uri":uri,"languageId":language,"version":version,"text":text.to_string()}})).await?;
                        }
                        documents.insert(path, Opened { text, identity, revision, version });
                    }
                    Command::Close(path) => {
                        if documents.remove(&path).is_some() { notify(&mut output, "textDocument/didClose", json!({"textDocument":{"uri":protocol::uri(&path)?}})).await?; }
                        pending.retain(|_, request| request.path != path);
                    }
                    Command::Saved(path) => {
                        if let Some(document) = documents.get(&path) {
                            let mut params = json!({"textDocument":{"uri":protocol::uri(&path)?}});
                            if capabilities["textDocumentSync"]["save"]["includeText"] == true { params["text"] = json!(document.text.to_string()); }
                            notify(&mut output, "textDocument/didSave", params).await?;
                        }
                    }
                    command @ (Command::Complete { .. } | Command::Format { .. }) => {
                        let (path, position, token) = match command {
                            Command::Complete { path, position, token } => (path, Some(position), token),
                            Command::Format { path, token } => (path, None, token),
                            _ => unreachable!(),
                        };
                        let Some(document) = documents.get(&path) else { emit(events, wake, Event::RequestError { token, message:"Wait for the file to connect".into() })?; continue; };
                        let provider = if position.is_some() { "completionProvider" } else { "documentFormattingProvider" };
                        if capabilities[provider].is_null() || capabilities[provider] == false || pending.len() >= 8 {
                            emit(events, wake, Event::RequestError { token, message:"Server does not support this action or is busy".into() })?;
                            continue;
                        }
                        let mut params = json!({"textDocument":{"uri":protocol::uri(&path)?}});
                        if let Some(position) = position { params["position"] = protocol::position(&document.text, position); params["context"] = json!({"triggerKind":1}); }
                        else { params["options"] = json!({"tabSize":4,"insertSpaces":true}); }
                        let method = if position.is_some() { "textDocument/completion" } else { "textDocument/formatting" };
                        protocol::write(&mut output, json!({"jsonrpc":"2.0","id":next,"method":method,"params":params})).await?;
                        pending.insert(next, Pending { token, path, document: document.clone(), position, deadline: Instant::now() + Duration::from_secs(10) });
                        next += 1;
                    }
                }
            }
            message = incoming.recv() => {
                let message = message.ok_or("Language server stopped")??;
                if message["method"].is_string() {
                    if !message["id"].is_null() { server_request(&mut output, &message, &root_uri).await?; }
                    else if message["method"] == "textDocument/publishDiagnostics" {
                        diagnostics(message["params"].clone(), &documents, events, wake)?;
                    }
                } else if let Some(request) = message["id"].as_u64().and_then(|id| pending.remove(&id)) {
                    if !message["error"].is_null() {
                        emit(events, wake, Event::RequestError { token: request.token, message: message["error"]["message"].as_str().unwrap_or("Language server request failed").chars().take(2048).collect() })?;
                    } else {
                        response(request, &message["result"], events, wake)?;
                    }
                }
            }
            () = async { if let Some(deadline) = deadline { tokio::time::sleep_until(deadline).await; } else { std::future::pending::<()>().await; } } => {
                let expired: Vec<_> = pending.iter().filter(|(_, request)| request.deadline <= Instant::now()).map(|(id, _)| *id).collect();
                for id in expired {
                    let request = pending.remove(&id).unwrap();
                    notify(&mut output, "$/cancelRequest", json!({"id":id})).await?;
                    emit(events, wake, Event::RequestError { token: request.token, message:"Language server request timed out".into() })?;
                }
            }
        }
    }
    Ok(())
}

fn diagnostics(
    params: Value,
    documents: &BTreeMap<PathBuf, Opened>,
    events: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<()> {
    let Some(path) = params["uri"]
        .as_str()
        .and_then(|uri| url::Url::parse(uri).ok())
        .and_then(|uri| uri.to_file_path().ok())
    else {
        return Ok(());
    };
    let Some(document) = documents.get(&path) else {
        return Ok(());
    };
    if params["version"]
        .as_i64()
        .is_some_and(|version| version != document.version)
    {
        return Ok(());
    }
    let items = params["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .take(100)
        .filter_map(|item| {
            let position = protocol::offset(&document.text, &item["range"]["start"]).ok()?;
            Some(Diagnostic {
                position,
                line: document.text.char_to_line(position),
                severity: item["severity"].as_u64().unwrap_or(1),
                message: item["message"].as_str()?.chars().take(2048).collect(),
            })
        })
        .collect();
    emit(
        events,
        wake,
        Event::Diagnostics {
            path,
            identity: document.identity,
            revision: document.revision,
            items,
        },
    )
}

fn response(
    request: Pending,
    result: &Value,
    events: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<()> {
    let Pending {
        token,
        path,
        document,
        position,
        ..
    } = request;
    if let Some(position) = position {
        let values = result.as_array().or_else(|| result["items"].as_array());
        let items = values
            .into_iter()
            .flatten()
            .take(100)
            .filter_map(|item| completion(&document.text, position, item).ok())
            .take(24)
            .collect();
        emit(
            events,
            wake,
            Event::Completions {
                token,
                path,
                identity: document.identity,
                revision: document.revision,
                items,
            },
        )
    } else {
        match if result.is_null() {
            Ok(Vec::new())
        } else {
            protocol::edits(&document.text, result)
        } {
            Ok(edits) => emit(
                events,
                wake,
                Event::Formatted {
                    token,
                    path,
                    identity: document.identity,
                    revision: document.revision,
                    edits,
                },
            ),
            Err(message) => emit(events, wake, Event::RequestError { token, message }),
        }
    }
}

fn completion(text: &Rope, position: usize, item: &Value) -> Result<Completion> {
    if item["insertTextFormat"] == 2 {
        return Err("Snippet completion is not enabled".into());
    }
    let label: String = item["label"]
        .as_str()
        .ok_or("Missing completion label")?
        .chars()
        .take(160)
        .collect();
    let primary = if item["textEdit"].is_object() {
        item["textEdit"].clone()
    } else {
        let position = position.min(text.len_chars());
        let mut start = position;
        while start > 0 && (text.char(start - 1).is_alphanumeric() || text.char(start - 1) == '_') {
            start -= 1;
        }
        json!({"range":{"start":protocol::position(text, start),"end":protocol::position(text, position)},"newText":item["insertText"].as_str().unwrap_or(&label)})
    };
    let mut values = vec![primary];
    if let Some(additional) = item["additionalTextEdits"].as_array() {
        if additional.len() > 64 {
            return Err("Too many completion edits".into());
        }
        values.extend_from_slice(additional);
    }
    let edits = protocol::edits(text, &Value::Array(values))?;
    Ok(Completion { label, edits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn server_session_initializes_syncs_utf16_and_returns_versioned_edits() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let (read, write) = tokio::io::split(client);
            let (server_read, mut server_write) = tokio::io::split(server);
            let mut server_read = BufReader::new(server_read);
            let (commands, receiver) = mpsc::channel(16);
            let (events, mut replies) = mpsc::channel(32);
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
            let task = tokio::spawn(async move { serve(write, read, PathBuf::from("/project"), receiver, &events, &wake).await });
            let initialize = protocol::read(&mut server_read).await.unwrap();
            assert_eq!(initialize["method"], "initialize");
            assert_eq!(initialize["params"]["capabilities"]["general"]["positionEncodings"], json!(["utf-16"]));
            protocol::write(&mut server_write, json!({"jsonrpc":"2.0","id":0,"result":{"capabilities":{"textDocumentSync":2,"completionProvider":{},"documentFormattingProvider":true}}})).await.unwrap();
            assert_eq!(protocol::read(&mut server_read).await.unwrap()["method"], "initialized");
            assert!(matches!(replies.recv().await, Some(Event::Ready { formatting: true })));
            let path = PathBuf::from("/project/file.rs");
            commands.send(Command::Sync { path: path.clone(), language:"rust".into(), text:Rope::from_str("🐈 pri"), identity:7, revision:1 }).await.unwrap();
            assert_eq!(protocol::read(&mut server_read).await.unwrap()["method"], "textDocument/didOpen");
            commands.send(Command::Sync { path: path.clone(), language:"rust".into(), text:Rope::from_str("🐈 prin"), identity:7, revision:2 }).await.unwrap();
            let change = protocol::read(&mut server_read).await.unwrap();
            assert_eq!(change["params"]["contentChanges"][0]["range"]["start"]["character"], 6);
            commands.send(Command::Complete { path: path.clone(), position:6, token:10 }).await.unwrap();
            let request = protocol::read(&mut server_read).await.unwrap();
            assert_eq!(request["params"]["position"]["character"], 7);
            protocol::write(&mut server_write, json!({"jsonrpc":"2.0","id":request["id"],"result":[{"label":"print","insertText":"print"}]})).await.unwrap();
            let Some(Event::Completions { token, identity, revision, items, .. }) = replies.recv().await else { panic!("expected completion"); };
            assert_eq!((token, identity, revision), (10, 7, 2));
            assert_eq!(items[0].edits[0].range, 2..6);
            assert_eq!(items[0].edits[0].text, "print");
            commands.send(Command::Format { path: path.clone(), token:11 }).await.unwrap();
            let request = protocol::read(&mut server_read).await.unwrap();
            protocol::write(&mut server_write, json!({"jsonrpc":"2.0","id":request["id"],"result":[{"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":7}},"newText":"print()"}]})).await.unwrap();
            let Some(Event::Formatted { token, revision, edits, .. }) = replies.recv().await else { panic!("expected formatting"); };
            assert_eq!((token, revision), (11, 2));
            assert_eq!(edits[0].range, 2..6);
            protocol::write(&mut server_write, json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///project/file.rs","version":2,"diagnostics":[{"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":7}},"severity":2,"message":"sample"}]}})).await.unwrap();
            let Some(Event::Diagnostics { revision, items, .. }) = replies.recv().await else { panic!("expected diagnostics"); };
            assert_eq!(revision, 2);
            assert_eq!(items[0].position, 2);
            commands.send(Command::Close(path)).await.unwrap();
            assert_eq!(protocol::read(&mut server_read).await.unwrap()["method"], "textDocument/didClose");
            drop(commands);
            task.await.unwrap().unwrap();
        }).await.unwrap();
    }

    #[test]
    fn completion_refuses_snippets_and_overlapping_additional_edits() {
        let text = Rope::from_str("value");
        assert!(
            completion(
                &text,
                5,
                &json!({"label":"snippet", "insertTextFormat":2, "insertText":"${1:value}"})
            )
            .is_err()
        );
        let range = json!({"start":{"line":0,"character":0},"end":{"line":0,"character":5}});
        assert!(completion(&text, 5, &json!({"label":"edit","textEdit":{"range":range,"newText":"one"},"additionalTextEdits":[{"range":range,"newText":"two"}]})).is_err());
    }
}
