use nucleus::plugins::{
    Compatibility, Contribution, Field, FieldType, ImportedRecord, Manifest, PreparedPublication,
    PublishRecord,
};
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
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
        format: nucleus::plugins::FORMAT.into(),
        id: "lince.activitypub".into(),
        name: "ActivityPub via Mastodon".into(),
        version: 1,
        compatibility: Compatibility::native(),
        executable,
        arguments: vec![],
        contributions: vec![
            Contribution::Sync {
                id: "notes".into(), name: "Mastodon public notes".into(), incoming: true, outgoing: true,
                replay_safe: false, schema: "note".into(), document: None,
                settings: vec![
                    field("instance", "Mastodon instance URL (HTTPS)", FieldType::Text, json!("https://mastodon.social")),
                    field("token_file", "Absolute path to access token file (read:statuses, write:statuses)", FieldType::Text, json!("")),
                    field("limit", "Notes per refresh (1–40)", FieldType::Integer, json!(20)),
                ],
            },
            Contribution::Schema {
                id: "note".into(), name: "Mastodon public note".into(), namespace: "plugin.lince.activitypub.note".into(),
                fields: vec![
                    field("author", "Author", FieldType::Text, Value::Null),
                    field("url", "Original URL", FieldType::Text, Value::Null),
                    field("published_at", "Published at", FieldType::Text, Value::Null),
                    field("reply_to", "Reply to status", FieldType::Json, Value::Null),
                ],
            },
        ],
        licenses: vec![nucleus::sand_package::License { name: "Lince — MIT".into(), text: include_str!("../../../LICENSE").into() }, nucleus::sand_package::License { name: "reqwest — MIT".into(), text: include_str!("../licenses/reqwest-MIT.txt").into() }, nucleus::sand_package::License { name: "html-escape — MIT".into(), text: include_str!("../licenses/html-escape-MIT.txt").into() }],
        credits: vec!["Mastodon client API: https://docs.joinmastodon.org/methods/ . The configured instance performs ActivityPub federation.".into()],
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    instance: String,
    token_file: String,
    limit: usize,
}

impl Settings {
    fn parse(value: Value) -> Result<Self, String> {
        let settings: Self =
            serde_json::from_value(value).map_err(|_| "Invalid Mastodon settings")?;
        let url = Url::parse(&settings.instance).map_err(|_| "Invalid instance URL")?;
        let local = url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if !(url.scheme() == "https" || (url.scheme() == "http" && local))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || url.host_str().is_none()
            || !(1..=40).contains(&settings.limit)
            || !Path::new(&settings.token_file).is_absolute()
        {
            return Err(
                "Choose an HTTPS instance, an absolute token-file path and a limit of 1–40".into(),
            );
        }
        Ok(settings)
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.instance.trim_end_matches('/'), path)
    }

    async fn token(&self) -> Result<Zeroizing<String>, String> {
        let metadata = tokio::fs::metadata(&self.token_file)
            .await
            .map_err(|_| "Cannot read access token file")?;
        if !metadata.is_file() || metadata.len() > 4096 {
            return Err("Token file must be a regular file of at most 4096 bytes".into());
        }
        let token = Zeroizing::new(
            tokio::fs::read_to_string(&self.token_file)
                .await
                .map_err(|_| "Cannot read access token file")?,
        );
        if token.trim().is_empty() || token.trim().chars().any(char::is_whitespace) {
            return Err("Token file must contain one access token".into());
        }
        Ok(token)
    }
}

fn client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent("Lince/0.8")
        .build()
        .map_err(|_| "Cannot create HTTP client".into())
}

async fn response(request: reqwest::RequestBuilder) -> Result<Value, String> {
    let mut response = request
        .send()
        .await
        .map_err(|_| "Mastodon request failed; remote outcome may be unknown")?;
    if !response.status().is_success() {
        return Err(format!(
            "Mastodon returned HTTP {}; inspect access and server limits",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Mastodon response interrupted")?
    {
        if bytes.len() + chunk.len() > 512 * 1024 {
            return Err("Mastodon response exceeds its size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid Mastodon response".into())
}

fn numeric_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_digit())
}
fn compare_id(left: &str, right: &str) -> std::cmp::Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

pub fn imported(status: &Value) -> Result<ImportedRecord, String> {
    let id = status["id"]
        .as_str()
        .filter(|id| numeric_id(id))
        .ok_or("Invalid status ID")?;
    if status["visibility"] != "public" || !status["reblog"].is_null() {
        return Err("Only original public text statuses are supported".into());
    }
    let html = status["content"]
        .as_str()
        .filter(|html| html.len() <= 32768)
        .ok_or("Missing or oversized status content")?;
    let lines = regex::Regex::new("(?i)</p\\s*>|<br\\s*/?>")
        .map_err(|_| "Invalid HTML pattern")?
        .replace_all(html, "\n");
    let plain = regex::Regex::new("<[^>]*>")
        .map_err(|_| "Invalid HTML pattern")?
        .replace_all(&lines, "");
    let body = html_escape::decode_html_entities(&plain).trim().to_owned();
    if body.len() > 16384 {
        return Err("Status exceeds the Record content limit".into());
    }
    let author = status["account"]["acct"]
        .as_str()
        .ok_or("Status author is missing")?;
    let head: String = body
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(120)
        .collect();
    Ok(ImportedRecord {
        external_id: id.into(),
        head: if head.trim().is_empty() {
            format!("Note by {author}")
        } else {
            head
        },
        body,
        metadata: json!({"author":author,"url":status["url"].as_str().unwrap_or(""),"published_at":status["created_at"].as_str().unwrap_or(""),"reply_to":status["in_reply_to_id"]}),
    })
}

fn scoped_import(settings: &Settings, status: &Value) -> Result<ImportedRecord, String> {
    let mut record = imported(status)?;
    let scope: String = Sha256::digest(settings.instance.trim_end_matches('/').as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    record.external_id = format!("{scope}:{}", record.external_id);
    Ok(record)
}

pub async fn handle(contribution: &str, operation: &str, input: Value) -> Result<Value, String> {
    if operation == "initialize" {
        return Ok(json!({"protocol":nucleus::plugins::PROTOCOL}));
    }
    if contribution != "notes" {
        return Err("Unknown contribution".into());
    }
    let settings = Settings::parse(input["settings"].clone())?;
    match operation {
        "validate" => {
            settings.token().await?;
            Ok(json!({"valid":true}))
        }
        "pull" => {
            let token = settings.token().await?;
            let mut query = vec![("limit", settings.limit.to_string())];
            let mut cursor = input["cursor"].as_str().unwrap_or("").to_owned();
            if !cursor.is_empty() {
                if !numeric_id(&cursor) {
                    return Err("Invalid saved timeline cursor".into());
                }
                query.push(("min_id", cursor.clone()));
            }
            let mut url = Url::parse(&settings.endpoint("/api/v1/timelines/public"))
                .map_err(|_| "Invalid timeline endpoint")?;
            url.query_pairs_mut().extend_pairs(&query);
            let batch = response(client()?.get(url).bearer_auth(token.trim())).await?;
            let statuses = batch
                .as_array()
                .filter(|items| items.len() <= settings.limit)
                .ok_or("Timeline exceeded its item limit")?;
            let mut records = Vec::new();
            for status in statuses {
                let id = status["id"]
                    .as_str()
                    .filter(|id| numeric_id(id))
                    .ok_or("Invalid status ID")?;
                if !cursor.is_empty()
                    && compare_id(id, input["cursor"].as_str().unwrap_or(""))
                        != std::cmp::Ordering::Greater
                {
                    continue;
                }
                if cursor.is_empty() || compare_id(id, &cursor).is_gt() {
                    cursor = id.into();
                }
                if status["visibility"] == "public" && status["reblog"].is_null() {
                    records.push(scoped_import(&settings, status)?);
                }
            }
            Ok(
                json!({"records":records,"cursor":if cursor.is_empty() { Value::Null } else { json!(cursor) }}),
            )
        }
        "prepare" => {
            let record: PublishRecord = serde_json::from_value(input["record"].clone())
                .map_err(|_| "Invalid publication Record")?;
            let content = if record.body.is_empty() {
                record.head.clone()
            } else {
                format!("{}\n\n{}", record.head, record.body)
            };
            if content.trim().is_empty() || content.chars().count() > 500 {
                return Err("Mastodon publication must contain 1–500 characters".into());
            }
            let hash = Sha256::digest(
                serde_json::to_vec(&json!([settings.instance, record.uid, content]))
                    .map_err(|error| error.to_string())?,
            );
            let id: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
            Ok(
                json!({"record":{"external_id":id,"head":record.head,"body":record.body,"metadata":{"author":"","url":"","published_at":"","reply_to":null}},"payload":{"status":content,"visibility":"public","idempotency_key":id}}),
            )
        }
        "push" => {
            let prepared: PreparedPublication = serde_json::from_value(input["prepared"].clone())
                .map_err(|_| "Invalid prepared publication")?;
            let key = prepared.payload["idempotency_key"]
                .as_str()
                .filter(|key| key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .ok_or("Invalid publication key")?;
            let content = prepared.payload["status"]
                .as_str()
                .filter(|text| !text.trim().is_empty() && text.chars().count() <= 500)
                .ok_or("Invalid publication content")?;
            let token = settings.token().await?;
            let result = response(
                client()?
                    .post(settings.endpoint("/api/v1/statuses"))
                    .bearer_auth(token.trim())
                    .header("Idempotency-Key", key)
                    .json(&json!({"status":content,"visibility":"public"})),
            )
            .await?;
            let published = scoped_import(&settings, &result)?;
            Ok(
                json!({"accepted":true,"record":published,"status_id":result["id"],"url":result["url"]}),
            )
        }
        _ => Err("Unknown operation".into()),
    }
}
