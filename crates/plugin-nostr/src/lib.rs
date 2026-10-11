use futures::{SinkExt, StreamExt};
use nostr::prelude::{Event, EventBuilder, FinalizeEvent, Keys, Kind, PublicKey, Timestamp};
use nucleus::plugins::{Contribution, Field, FieldType, ImportedRecord, Manifest, PublishRecord};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_tungstenite::tungstenite::{Message, protocol::WebSocketConfig};
use zeroize::Zeroizing;

pub fn manifest(executable: String) -> Manifest {
    fn field(id: &str, name: &str, kind: FieldType, default: Value) -> Field {
        Field {
            id: id.into(),
            name: name.into(),
            kind,
            required: true,
            default,
        }
    }
    Manifest {
        format: nucleus::plugins::FORMAT.into(),
        id: "example.nostr".into(),
        name: "Nostr public notes".into(),
        version: 1,
        executable,
        arguments: Vec::new(),
        contributions: vec![
            Contribution::Sync {
                id: "notes".into(),
                name: "Nostr notes".into(),
                incoming: true,
                outgoing: true,
                replay_safe: true,
                schema: "note".into(),
                document: None,
                settings: vec![
                    field(
                        "relays",
                        "Relay URLs, separated by commas",
                        FieldType::Text,
                        json!("wss://relay.damus.io"),
                    ),
                    field(
                        "authors",
                        "Author public keys in hex, separated by commas (empty means all)",
                        FieldType::Text,
                        json!(""),
                    ),
                    field(
                        "limit",
                        "Recent notes per relay (1–100)",
                        FieldType::Integer,
                        json!(20),
                    ),
                ],
            },
            Contribution::Action {
                id: "identity".into(),
                name: "Create or show publication identity".into(),
                arguments: Vec::new(),
            },
            Contribution::Schema {
                id: "note".into(),
                name: "Signed Nostr note".into(),
                namespace: "plugin.example.nostr.note".into(),
                fields: vec![
                    field("event_id", "Event ID", FieldType::Text, Value::Null),
                    field("author", "Author public key", FieldType::Text, Value::Null),
                    field(
                        "published_at",
                        "Published timestamp",
                        FieldType::Integer,
                        Value::Null,
                    ),
                    field(
                        "signed_event",
                        "Original signed event",
                        FieldType::Json,
                        Value::Null,
                    ),
                ],
            },
        ],
        licenses: vec![
            nucleus::sand_package::License {
                name: "Lince — AGPL-3.0-only".into(),
                text: include_str!("../../../LICENSE").into(),
            },
            nucleus::sand_package::License {
                name: "nostr — MIT".into(),
                text: include_str!("../licenses/nostr-MIT.txt").into(),
            },
            nucleus::sand_package::License {
                name: "tokio-tungstenite — MIT".into(),
                text: include_str!("../licenses/tokio-tungstenite-MIT.txt").into(),
            },
            nucleus::sand_package::License {
                name: "tungstenite — MIT".into(),
                text: include_str!("../licenses/tungstenite-MIT.txt").into(),
            },
        ],
        credits: vec![
            "rust-nostr/nostr (MIT), tokio-tungstenite (MIT), rustls (Apache-2.0 / ISC / MIT)"
                .into(),
        ],
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    relays: String,
    authors: String,
    limit: usize,
}

impl Settings {
    fn parse(value: Value) -> Result<Self, String> {
        let settings: Self = serde_json::from_value(value).map_err(|error| error.to_string())?;
        let relays = settings.relays();
        if relays.is_empty() || relays.len() > 3 || !(1..=100).contains(&settings.limit) {
            return Err("Choose 1–3 relays and a note limit between 1 and 100".into());
        }
        for relay in relays {
            let uri: tokio_tungstenite::tungstenite::http::Uri =
                relay.parse().map_err(|_| "Invalid relay URL")?;
            let local = matches!(uri.host(), Some("localhost" | "127.0.0.1" | "[::1]"));
            if uri
                .authority()
                .is_none_or(|authority| authority.as_str().contains('@'))
                || !(uri.scheme_str() == Some("wss") || (uri.scheme_str() == Some("ws") && local))
            {
                return Err("Use wss:// relays, or ws:// for a local test relay".into());
            }
        }
        let authors = settings.authors();
        if authors.len() > 32
            || authors
                .iter()
                .any(|author| author.len() != 64 || PublicKey::parse(author).is_err())
        {
            return Err("Choose at most 32 hex public keys".into());
        }
        Ok(settings)
    }
    fn relays(&self) -> Vec<&str> {
        self.relays
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect()
    }
    fn authors(&self) -> Vec<String> {
        self.authors
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()
    }
}

fn imported(event: &Event) -> Result<ImportedRecord, String> {
    event
        .verify()
        .map_err(|_| "Invalid event signature or ID")?;
    if event.kind != Kind::TextNote
        || event.content.len() > 16384
        || event.created_at.as_secs() > Timestamp::now().as_secs() + 300
    {
        return Err("Unsupported or oversized Nostr note".into());
    }
    let head: String = event
        .content
        .lines()
        .next()
        .unwrap_or("Nostr note")
        .chars()
        .take(120)
        .collect();
    Ok(ImportedRecord {
        external_id: event.id.to_hex(),
        head: if head.trim().is_empty() {
            "Nostr note".into()
        } else {
            head
        },
        body: event.content.clone(),
        metadata: json!({"event_id":event.id.to_hex(),"author":event.pubkey.to_hex(),"published_at":event.created_at.as_secs(),"signed_event":event}),
    })
}

async fn relay_pull(
    relay: &str,
    settings: &Settings,
    cursor: &Value,
) -> Result<Vec<ImportedRecord>, String> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(262144))
        .max_frame_size(Some(262144));
    let (mut socket, _) = tokio_tungstenite::connect_async_with_config(relay, Some(config), false)
        .await
        .map_err(|error| error.to_string())?;
    let mut filter = json!({"kinds":[1],"limit":settings.limit});
    if !settings.authors().is_empty() {
        filter["authors"] = json!(settings.authors());
    }
    if let Some(since) = cursor.as_u64() {
        filter["since"] = json!(since.saturating_sub(1));
    }
    socket
        .send(Message::Text(
            json!(["REQ", "lince-notes", filter]).to_string().into(),
        ))
        .await
        .map_err(|error| error.to_string())?;
    let mut records = BTreeMap::new();
    for _ in 0..512 {
        let message = socket
            .next()
            .await
            .ok_or("Relay closed before EOSE")?
            .map_err(|error| error.to_string())?;
        let Message::Text(message) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&message).map_err(|error| error.to_string())?;
        match value[0].as_str() {
            Some("EVENT") if value[1] == "lince-notes" => {
                let event: Event =
                    serde_json::from_value(value[2].clone()).map_err(|error| error.to_string())?;
                let record = imported(&event)?;
                if !settings.authors().is_empty()
                    && !settings.authors().contains(&event.pubkey.to_hex())
                {
                    return Err("Relay returned an author outside the subscription".into());
                }
                if records.len() < settings.limit {
                    records.insert(record.external_id.clone(), record);
                }
            }
            Some("EOSE") if value[1] == "lince-notes" => {
                let _ = socket
                    .send(Message::Text(
                        json!(["CLOSE", "lince-notes"]).to_string().into(),
                    ))
                    .await;
                return Ok(records.into_values().collect());
            }
            Some("CLOSED") if value[1] == "lince-notes" => {
                return Err(format!("Relay rejected subscription: {}", value[2]));
            }
            _ => {}
        }
    }
    Err("Relay exceeded the subscription message budget".into())
}

async fn keys(state: &Path, create: bool) -> Result<Keys, String> {
    let path = state.join("identity.key");
    if let Ok(value) = tokio::fs::read_to_string(&path).await {
        return Keys::parse(Zeroizing::new(value).as_str())
            .map_err(|_| "Invalid stored publication identity".into());
    }
    if !create {
        return Err("Run Create or show publication identity before publishing".into());
    }
    tokio::fs::create_dir_all(state)
        .await
        .map_err(|error| error.to_string())?;
    let keys = Keys::generate();
    let value = Zeroizing::new(keys.secret_key().to_secret_hex());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(value.as_bytes())
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Box::pin(keys_read(state)).await;
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(keys)
}

async fn keys_read(state: &Path) -> Result<Keys, String> {
    let value = Zeroizing::new(
        tokio::fs::read_to_string(state.join("identity.key"))
            .await
            .map_err(|error| error.to_string())?,
    );
    Keys::parse(value.as_str()).map_err(|_| "Invalid stored publication identity".into())
}

async fn relay_push(relay: &str, event: &Event) -> Result<(), String> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(262144))
        .max_frame_size(Some(262144));
    let (mut socket, _) = tokio_tungstenite::connect_async_with_config(relay, Some(config), false)
        .await
        .map_err(|error| error.to_string())?;
    socket
        .send(Message::Text(json!(["EVENT", event]).to_string().into()))
        .await
        .map_err(|error| error.to_string())?;
    for _ in 0..64 {
        let message = socket
            .next()
            .await
            .ok_or("Relay closed without acknowledgement")?
            .map_err(|error| error.to_string())?;
        let Message::Text(message) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&message).map_err(|error| error.to_string())?;
        if value[0] == "OK" && value[1] == event.id.to_hex() {
            return if value[2] == true {
                Ok(())
            } else {
                Err(format!("Relay rejected event: {}", value[3]))
            };
        }
    }
    Err("Relay exceeded the acknowledgement message budget".into())
}

pub async fn handle(
    contribution: &str,
    operation: &str,
    input: Value,
    state: &Path,
) -> Result<Value, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    if operation == "initialize" {
        let _ = rustls::ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();
        return Ok(json!({"protocol":nucleus::plugins::PROTOCOL}));
    }
    if contribution == "identity" && operation == "invoke" {
        return Ok(json!({"public_key":keys(state, true).await?.public_key().to_hex()}));
    }
    if contribution != "notes" {
        return Err("Unknown contribution".into());
    }
    let settings = Settings::parse(input["settings"].clone())?;
    match operation {
        "validate" => Ok(json!({"valid":true})),
        "pull" => {
            let mut records = BTreeMap::new();
            let mut cursor = input["cursor"].as_u64().unwrap_or(0);
            for relay in settings.relays() {
                let batch = tokio::time::timeout(
                    Duration::from_secs(12),
                    relay_pull(relay, &settings, &input["cursor"]),
                )
                .await
                .map_err(|_| "Relay subscription timed out")??;
                for record in batch {
                    cursor = cursor.max(record.metadata["published_at"].as_u64().unwrap_or(0));
                    records.insert(record.external_id.clone(), record);
                }
            }
            if records.len() > nucleus::plugins::MAX_ITEMS {
                return Err("Combined relay result exceeds 100 notes; narrow authors or reduce the per-relay limit".into());
            }
            Ok(json!({"records":records.into_values().collect::<Vec<_>>(),"cursor":cursor}))
        }
        "prepare" => {
            let record: PublishRecord = serde_json::from_value(input["record"].clone())
                .map_err(|error| error.to_string())?;
            let content = if record.body.is_empty() {
                record.head
            } else {
                format!("{}\n\n{}", record.head, record.body)
            };
            if content.len() > 16384 {
                return Err("Publication exceeds the note budget".into());
            }
            let event = EventBuilder::new(Kind::TextNote, content)
                .custom_created_at(Timestamp::from(record.created_at))
                .finalize(&keys(state, false).await?)
                .map_err(|error| error.to_string())?;
            Ok(json!({"record":imported(&event)?,"payload":event}))
        }
        "push" => {
            let event: Event = serde_json::from_value(input["prepared"]["payload"].clone())
                .map_err(|error| error.to_string())?;
            imported(&event)?;
            if event.pubkey != keys(state, false).await?.public_key() {
                return Err("Publication identity does not match the prepared event".into());
            }
            let mut acknowledgements = Vec::new();
            for relay in settings.relays() {
                let result =
                    tokio::time::timeout(Duration::from_secs(12), relay_push(relay, &event))
                        .await
                        .map_err(|_| "Relay acknowledgement timed out".to_string())
                        .and_then(|result| result);
                acknowledgements
                    .push(json!({"relay":relay,"accepted":result.is_ok(),"detail":result.err()}));
            }
            Ok(
                json!({"accepted":acknowledgements.iter().all(|item| item["accepted"] == true),"event_id":event.id.to_hex(),"relays":acknowledgements}),
            )
        }
        _ => Err("Unknown operation".into()),
    }
}

pub fn install(directory: &Path) -> Result<PathBuf, String> {
    let package = directory.join("plugins/example.nostr");
    std::fs::create_dir_all(&package).map_err(|error| error.to_string())?;
    let executable = format!("lince-plugin-nostr{}", std::env::consts::EXE_SUFFIX);
    let staging = package.join(".executable.new");
    std::fs::copy(
        std::env::current_exe().map_err(|error| error.to_string())?,
        &staging,
    )
    .map_err(|error| error.to_string())?;
    std::fs::rename(staging, package.join(&executable)).map_err(|error| error.to_string())?;
    let manifest = manifest(executable);
    manifest.validate()?;
    std::fs::write(
        package.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(package)
}
