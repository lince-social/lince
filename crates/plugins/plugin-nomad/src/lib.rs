// non-lean-yet
// Scope: mod client, mod export, mod graph, mod schema.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 70, 157, 164.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 98.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites fetch: String
//   (69), handle: String (138), pull: String (90), sync: String (74).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (56, 119, 134).

pub mod client;
pub mod export;
pub mod graph;
pub mod schema;
pub mod settings;

use graph::Graph;
use nucleus::plugins::{Compatibility, Contribution, Field, FieldType, Manifest, PullResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use settings::Settings;
use std::path::Path;

fn field(id: &str, name: &str, kind: FieldType, default: Value, required: bool) -> Field {
    Field {
        id: id.into(),
        name: name.into(),
        kind,
        required,
        default,
    }
}

pub fn manifest(executable: String) -> Manifest {
    Manifest {
        format: nucleus::plugins::FORMAT.into(), id: "lince.nomad".into(), name: "Local NOMAD sources".into(),
        version: 1, compatibility: Compatibility::native(), executable, arguments: vec![],
        contributions: vec![
            Contribution::Sync {
                id: "sources".into(), name: "NOMAD source discovery".into(), incoming: true, outgoing: false,
                replay_safe: true, schema: "source".into(), document: None,
                settings: vec![
                    field("base_url", "Local NOMAD URL", FieldType::Text, json!("http://127.0.0.1:8080"), true),
                    field("instance_id", "Durable name for this NOMAD (optional)", FieldType::Text, json!(""), false),
                    field("kiwix_url", "Local Kiwix URL (optional)", FieldType::Text, json!(""), false),
                    field("include_maps", "Include map catalog and installed maps", FieldType::Boolean, json!(false), false),
                    field("include_documents", "Include AI knowledge-base files", FieldType::Boolean, json!(false), false),
                    field("include_wikipedia", "Include Wikipedia editions", FieldType::Boolean, json!(false), false),
                    field("batch_size", "Records per refresh (1–100)", FieldType::Integer, json!(50), false),
                ],
            },
            Contribution::Schema {
                id: "source".into(), name: "NOMAD source snapshot".into(), namespace: graph::NAMESPACE.into(),
                fields: vec![
                    field("identity", "Stable source identity", FieldType::Text, Value::Null, true),
                    field("kind", "Source kind", FieldType::Text, Value::Null, true),
                    field("original_title", "Original source title", FieldType::Text, Value::Null, true),
                    field("source_url", "Source URL", FieldType::Text, Value::Null, true),
                    field("data", "NOMAD metadata", FieldType::Json, Value::Null, true),
                    field("relations", "Relations by stable source identity", FieldType::Json, Value::Null, true),
                ],
            },
            Contribution::Action {
                id: "export".into(), name: "Refresh connected NOMAD Records".into(),
                arguments: vec![
                    field("settings", "NOMAD settings", FieldType::Json, serde_json::to_value(Settings::default()).unwrap(), true),
                    field("directory", "Absolute Lingua export directory", FieldType::Text, json!(""), true),
                ],
            },
        ],
        licenses: vec![
            nucleus::sand_package::License { name: "Lince — MIT".into(), text: include_str!("../../../../LICENSE").into() },
            nucleus::sand_package::License { name: "reqwest — MIT".into(), text: include_str!("../../plugin-activitypub/licenses/reqwest-MIT.txt").into() },
        ],
        credits: vec!["Project NOMAD: https://github.com/Crosstalk-Solutions/project-nomad . Source content retains its own attribution and reuse terms.".into()],
    }
}

pub async fn fetch(settings: Settings) -> Result<Graph, String> {
    let snapshot = client::Nomad::new(settings.clone())?.snapshot().await?;
    Graph::build(&settings, snapshot)
}

pub async fn sync(settings: Settings, directory: &Path) -> Result<export::ExportReport, String> {
    let graph = fetch(settings).await?;
    let directory = directory.to_path_buf();
    tokio::task::spawn_blocking(move || export::write_graph(&graph, &directory))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u32,
    snapshot: String,
    offset: usize,
}

pub fn pull(graph: &Graph, batch_size: usize, cursor: Value) -> Result<PullResult, String> {
    if !(1..=100).contains(&batch_size) {
        return Err("Batch size must be 1–100".into());
    }
    let fingerprint = graph.fingerprint();
    let offset = if cursor.is_null() {
        0
    } else {
        let cursor: Cursor = serde_json::from_value(cursor).map_err(|_| "Invalid NOMAD cursor")?;
        if cursor.version != 1
            || cursor.offset > graph::MAX_NODES
            || cursor.snapshot.len() != 64
            || !cursor.snapshot.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("Invalid NOMAD cursor".into());
        }
        if cursor.snapshot == fingerprint {
            if cursor.offset > graph.nodes.len() {
                return Err("NOMAD cursor exceeds its snapshot".into());
            }
            cursor.offset
        } else {
            0
        }
    };
    let mut records = Vec::new();
    let mut bytes = 512;
    for node in graph.nodes.values().skip(offset).take(batch_size) {
        let record = graph.imported(node)?;
        let size = serde_json::to_vec(&record).unwrap().len() + 1;
        if bytes + size >= nucleus::plugins::MAX_BYTES {
            if records.is_empty() {
                return Err("NOMAD Record exceeds the plugin frame budget".into());
            }
            break;
        }
        bytes += size;
        records.push(record);
    }
    let cursor = serde_json::to_value(Cursor {
        version: 1,
        snapshot: fingerprint,
        offset: offset + records.len(),
    })
    .unwrap();
    Ok(PullResult { records, cursor })
}

pub async fn handle(contribution: &str, operation: &str, input: Value) -> Result<Value, String> {
    if operation == "initialize" {
        return Ok(json!({"protocol":nucleus::plugins::PROTOCOL}));
    }
    if contribution == "export" && operation == "invoke" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Arguments {
            settings: Settings,
            directory: String,
        }
        let args: Arguments =
            serde_json::from_value(input).map_err(|e| format!("Invalid export arguments: {e}"))?;
        return serde_json::to_value(sync(args.settings, Path::new(&args.directory)).await?)
            .map_err(|e| e.to_string());
    }
    if contribution != "sources" || !matches!(operation, "pull" | "validate") {
        return Err("Unknown NOMAD contribution or operation".into());
    }
    let settings: Settings = serde_json::from_value(input["settings"].clone())
        .map_err(|e| format!("Invalid NOMAD settings: {e}"))?;
    let batch_size = settings.batch_size;
    let graph = fetch(settings).await?;
    if operation == "validate" {
        return Ok(json!({"valid":true,"records":graph.nodes.len()}));
    }
    serde_json::to_value(pull(&graph, batch_size, input["cursor"].clone())?)
        .map_err(|e| e.to_string())
}
