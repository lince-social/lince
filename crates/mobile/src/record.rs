use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    actions::Action,
    record_change::{Mutation, Request, WorkField},
};
use loro::LoroDoc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize)]
pub struct Prepared {
    pub action: Action,
    pub snapshot: Option<String>,
    pub submitted: String,
}

pub const SORTS: [(&str, &str); 4] = [
    ("Title", "head"),
    ("Due date", "due_date"),
    ("Quantity", "quantity"),
    ("Created", "created_at"),
];

pub fn new_draft(task: Option<String>) -> engine::record_creation::Draft {
    engine::record_creation::Draft {
        head: if task.is_some() {
            "New task"
        } else {
            "New Record"
        }
        .into(),
        assertions: task
            .into_iter()
            .map(|predicate| engine::record_creation::Assertion {
                predicate,
                object: None,
                quantity: None,
                unit: None,
            })
            .collect(),
        ..Default::default()
    }
}

pub fn change(
    uid: &str,
    property: &str,
    value: &str,
    snapshot: Option<&str>,
) -> Result<Action, String> {
    prepare_change(uid, property, value, snapshot).map(|(action, _)| action)
}

pub fn prepare_change(
    uid: &str,
    property: &str,
    value: &str,
    snapshot: Option<&str>,
) -> Result<(Action, Option<String>), String> {
    let mut updated_snapshot = None;
    let mutation = match property {
        "head" | "body" => {
            let snapshot = snapshot.ok_or("Wait for the Record editor to connect")?;
            let document = LoroDoc::new();
            document
                .import(&B64.decode(snapshot).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let before = document.oplog_vv();
            document
                .get_text(property)
                .update(value, Default::default())
                .map_err(|e| e.to_string())?;
            document.commit();
            updated_snapshot = Some(
                B64.encode(
                    document
                        .export(loro::ExportMode::Snapshot)
                        .map_err(|error| error.to_string())?,
                ),
            );
            Mutation::Text {
                update_base64: engine::collab::encode_update(&document, &before)?,
            }
        }
        "quantity" => {
            nucleus::DecimalValue::parse_inferred(value)
                .map_err(|_| "Enter an exact decimal quantity")?;
            Mutation::Quantity {
                value: value.into(),
            }
        }
        "slug" => Mutation::Slug {
            value: (!value.trim().is_empty()).then(|| value.trim().into()),
        },
        "start_date" | "due_date" => Mutation::Work {
            field: if property == "start_date" {
                WorkField::Start
            } else {
                WorkField::Due
            },
            value: if value.trim().is_empty() {
                Value::Null
            } else {
                Value::String(value.trim().into())
            },
        },
        "estimate_min" => Mutation::Work {
            field: WorkField::Estimate,
            value: if value.trim().is_empty() {
                Value::Null
            } else {
                serde_json::json!(value.parse::<u64>().map_err(|_| "Enter whole minutes")?)
            },
        },
        _ => return Err("This field has its own controls".into()),
    };
    Ok((
        Action::ChangeRecord {
            request: Request {
                id: nucleus::new_uid("op"),
                record_uid: uid.into(),
                mutation,
            },
        },
        updated_snapshot,
    ))
}

pub fn query(uid: Option<&str>, limit: usize, search: &str) -> protein::Protein {
    let filter = match uid {
        Some(uid) => vec![protein::Predicate::UidEq(uid.into())],
        None if search.trim().is_empty() => Vec::new(),
        None => vec![protein::Predicate::TextContains(search.trim().into())],
    };
    protein::Protein {
        source: protein::Source::Record,
        filter,
        fields: uid.is_none().then(|| {
            ["uid", "head", "quantity", "due_date", "created_at"]
                .map(String::from)
                .to_vec()
        }),
        include: protein::Include {
            threads: uid.map(|_| Default::default()),
            extension: uid.map(|_| protein::ExtensionInclude {
                namespace: "work".into(),
            }),
            ..Default::default()
        },
        aggregate: None,
        order: vec![protein::Order::Asc("head".into())],
        limit: Some(limit),
    }
}
