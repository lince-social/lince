use crate::{Include, Order, Predicate, Protein};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use store::{Store, records::RecordRow};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub key: &'static str,
    pub title: &'static str,
    pub kind: &'static str,
    pub editable: bool,
}

pub const FIELDS: &[Field] = &[
    Field {
        key: "head",
        title: "Title",
        kind: "text",
        editable: true,
    },
    Field {
        key: "body",
        title: "Description",
        kind: "text",
        editable: true,
    },
    Field {
        key: "slug",
        title: "Slug",
        kind: "text",
        editable: true,
    },
    Field {
        key: "quantity",
        title: "Quantity",
        kind: "decimal",
        editable: true,
    },
    Field {
        key: "assertions",
        title: "Assertions",
        kind: "assertions",
        editable: true,
    },
    Field {
        key: "kind",
        title: "Kind",
        kind: "text",
        editable: false,
    },
    Field {
        key: "assignees",
        title: "Assignees",
        kind: "records",
        editable: true,
    },
    Field {
        key: "start_date",
        title: "Start",
        kind: "date-time",
        editable: true,
    },
    Field {
        key: "due_date",
        title: "Due",
        kind: "date-time",
        editable: true,
    },
    Field {
        key: "estimate_min",
        title: "Estimate (minutes)",
        kind: "number",
        editable: true,
    },
    Field {
        key: "spent_seconds",
        title: "Time spent (seconds)",
        kind: "number",
        editable: false,
    },
    Field {
        key: "running_since",
        title: "Running since",
        kind: "timestamp",
        editable: false,
    },
    Field {
        key: "work_logs",
        title: "Work logs",
        kind: "logs",
        editable: true,
    },
    Field {
        key: "work_timer",
        title: "Work timer",
        kind: "timer",
        editable: true,
    },
    Field {
        key: "threads",
        title: "Threads and messages",
        kind: "threads",
        editable: true,
    },
    Field {
        key: "created_at",
        title: "Created",
        kind: "timestamp",
        editable: false,
    },
    Field {
        key: "updated_at",
        title: "Updated",
        kind: "timestamp",
        editable: false,
    },
];

pub fn fields() -> Vec<Field> {
    FIELDS.to_vec()
}

pub fn selected(protein: &Protein) -> Vec<Field> {
    FIELDS
        .iter()
        .filter(|field| {
            protein.fields.as_ref().is_none_or(|fields| {
                fields.iter().any(|key| key == field.key) || field.key == "kind"
            })
        })
        .cloned()
        .collect()
}

pub(crate) fn requests(query: &Protein, key: &str) -> bool {
    query
        .fields
        .as_ref()
        .is_none_or(|fields| fields.iter().any(|field| field == key))
}

pub(crate) fn selected_include(query: &Protein) -> std::borrow::Cow<'_, Include> {
    if query.fields.is_none() {
        return std::borrow::Cow::Borrowed(&query.include);
    }
    let include = &query.include;
    let wants = |key| requests(query, key);
    std::borrow::Cow::Owned(Include {
        relation_context: include
            .relation_context
            .as_ref()
            .filter(|_| wants("relation_context"))
            .cloned(),
        record_after: include.record_after.clone(),
        facts: include.facts.as_ref().filter(|_| wants("facts")).cloned(),
        promises: include
            .promises
            .as_ref()
            .filter(|_| wants("promises"))
            .cloned(),
        links: include.links.as_ref().filter(|_| wants("links")).cloned(),
        threads: include
            .threads
            .as_ref()
            .filter(|_| wants("threads"))
            .cloned(),
        availability: include.availability
            && [
                "actual",
                "reserved",
                "available",
                "surplus",
                "can_offer",
                "actual_exact",
                "reserved_exact",
                "surplus_exact",
                "can_offer_exact",
                "availability_complete",
                "availability_incomplete_count",
                "scenario_required",
            ]
            .into_iter()
            .any(wants),
        extension: include
            .extension
            .as_ref()
            .filter(|_| wants("extension"))
            .cloned(),
        shader_effect: include.shader_effect && wants("shader_effect"),
        numeric_fields: if wants("numeric_values") {
            include.numeric_fields.clone()
        } else {
            Vec::new()
        },
        numeric_extensions: include.numeric_extensions && wants("numeric_extensions"),
        projection: include
            .projection
            .as_ref()
            .filter(|_| wants("projected"))
            .cloned(),
        contact: include.contact && wants("contact"),
        conversations: include.conversations && wants("conversations"),
        reference_reads: include.reference_reads && wants("reference_reads"),
    })
}

pub(crate) fn predicates_require_body(predicates: &[Predicate]) -> bool {
    predicates.iter().any(|predicate| match predicate {
        Predicate::All(children) | Predicate::Any(children) => predicates_require_body(children),
        Predicate::Not(child) => predicates_require_body(std::slice::from_ref(child.as_ref())),
        Predicate::QuantityLt(_)
        | Predicate::QuantityLte(_)
        | Predicate::QuantityGt(_)
        | Predicate::QuantityGte(_)
        | Predicate::QuantityEq(_)
        | Predicate::UidEq(_)
        | Predicate::KindEq(_)
        | Predicate::SlugEq(_)
        | Predicate::ConceptIn(_)
        | Predicate::Relation { .. }
        | Predicate::Under { .. }
        | Predicate::WorkDate { .. }
        | Predicate::Near { .. }
        | Predicate::OrganEq(_)
        | Predicate::OrganIn(_) => false,
        _ => true,
    })
}

fn field_requires_body(field: &str) -> bool {
    !matches!(
        field,
        "uid"
            | "kind"
            | "slug"
            | "head"
            | "title"
            | "quantity"
            | "concept"
            | "concept_name"
            | "unit"
            | "place"
            | "organ"
            | "created_at"
            | "updated_at"
            | "created_hlc"
            | "start_date"
            | "due_date"
            | "estimate_min"
            | "spent_seconds"
            | "running_since"
            | "work_logs"
            | "work_timer"
            | "assertions"
            | "assignees"
            | "assignee"
            | "assignee_name"
            | "facts"
            | "promises"
            | "links"
            | "threads"
            | "extension"
            | "numeric_extensions"
            | "numeric_values"
            | "shader_effect"
            | "relation_context"
            | "reference_reads"
            | "contact"
            | "conversations"
            | "projected"
            | "actual"
            | "reserved"
            | "available"
            | "surplus"
            | "can_offer"
            | "actual_exact"
            | "reserved_exact"
            | "surplus_exact"
            | "can_offer_exact"
            | "availability_complete"
            | "availability_incomplete_count"
            | "scenario_required"
    )
}

fn includes_require_body(include: &Include) -> bool {
    match include {
        Include {
            relation_context: _,
            record_after: _,
            facts: _,
            promises: _,
            links: _,
            threads: _,
            availability: _,
            extension: _,
            shader_effect: _,
            numeric_fields: _,
            numeric_extensions: _,
            projection: _,
            contact: _,
            conversations: _,
            reference_reads: _,
        } => false,
    }
}

pub(crate) fn requires_body(query: &Protein) -> bool {
    predicates_require_body(&query.filter)
        || query.aggregate.is_none()
            && (query
                .fields
                .as_ref()
                .is_none_or(|fields| fields.iter().any(|field| field_requires_body(field)))
                || query.order.iter().any(|order| match order {
                    Order::Link(_) => false,
                    Order::Asc(field) | Order::Desc(field) => {
                        !field.starts_with('#') && field_requires_body(field)
                    }
                })
                || includes_require_body(&query.include))
}

pub(crate) async fn attach<Body>(
    store: &Store,
    query: &Protein,
    records: &[RecordRow<Body>],
    visible: Option<&HashSet<String>>,
    work: &mut HashMap<String, Value>,
    concept_names: &HashMap<String, String>,
) -> Result<HashMap<String, Value>, crate::ProteinError> {
    let _schema = nucleus::performance::Span::new("protein-record-schema-await");
    let wants = |key| requests(query, key);
    let wants_logs = wants("work_logs");
    let wants_spent = wants("spent_seconds");
    let wants_running = wants("running_since");
    if ![
        "estimate_min",
        "work_logs",
        "spent_seconds",
        "running_since",
        "assertions",
        "assignees",
    ]
    .iter()
    .any(|key| wants(key))
    {
        return Ok(HashMap::new());
    }
    let mut log_ids = HashMap::<String, Vec<String>>::new();
    if wants_logs {
        let owners: Vec<_> = records
            .iter()
            .filter(|record| {
                work.get(&record.uid)
                    .and_then(|metadata| metadata.get("logs"))
                    .and_then(Value::as_array)
                    .is_some_and(|logs| !logs.is_empty())
            })
            .map(|record| record.uid.as_str())
            .collect();
        for owners in owners.chunks(512) {
            let _ids = nucleus::performance::Span::new("protein-work-log-ids-await");
            let mut query = store::sqlx::QueryBuilder::new(
                "SELECT record_uid, property FROM record_property WHERE record_uid IN (",
            );
            let mut arguments = query.separated(",");
            for uid in owners {
                arguments.push_bind(*uid);
            }
            arguments.push_unseparated(")");
            query.push(" AND property LIKE 'work.log:%' AND json_type(value) = 'object' ORDER BY record_uid, json_extract(value, '$.start'), property");
            for (uid, property) in query
                .build_query_as::<(String, String)>()
                .fetch_all(&store.pool)
                .await?
            {
                log_ids.entry(uid).or_default().push(property);
            }
        }
    }
    let mut out = HashMap::new();
    for record in records {
        let _publish = nucleus::performance::Span::new("protein-record-work-publish");
        let mut value = json!({});
        let mut metadata = work.get_mut(&record.uid).and_then(Value::as_object_mut);
        if wants("estimate_min") {
            value["estimate_min"] = metadata
                .as_mut()
                .and_then(|work| work.remove("estimate_min"))
                .unwrap_or(Value::Null);
        }
        let mut logs = if wants_logs || wants_spent || wants_running {
            match metadata.and_then(|work| work.remove("logs")) {
                Some(Value::Array(logs)) => logs,
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        if wants_logs {
            let mut entries = log_ids.remove(&record.uid).unwrap_or_default().into_iter();
            for (index, log) in logs.iter_mut().enumerate() {
                log["id"] = Value::String(
                    entries
                        .next()
                        .unwrap_or_else(|| format!("work.log:seed-{index}")),
                );
            }
        }
        if wants_spent || wants_running {
            let mut spent = 0i64;
            let mut running = None;
            for (index, log) in logs.iter().enumerate() {
                let Some(start) = log["start"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                else {
                    continue;
                };
                let end = log["end"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
                if wants_running && end.is_none() {
                    running = Some(index);
                }
                if wants_spent {
                    spent = spent.saturating_add(
                        (end.map(|v| v.with_timezone(&chrono::Utc))
                            .unwrap_or_else(chrono::Utc::now)
                            - start.with_timezone(&chrono::Utc))
                        .num_seconds()
                        .max(0),
                    );
                }
            }
            let running = running.map(|index| {
                if wants_logs {
                    logs[index]["start"].clone()
                } else {
                    logs[index]["start"].take()
                }
            });
            if wants_logs {
                value["work_logs"] = Value::Array(logs);
            }
            if wants_spent {
                value["spent_seconds"] = spent.into();
            }
            if wants_running {
                value["running_since"] = running.unwrap_or(Value::Null);
            }
        } else if wants_logs {
            value["work_logs"] = Value::Array(logs);
        }
        if wants("assertions") {
            value["assertions"] = json!([]);
        }
        if wants("assignees") {
            value["assignees"] = json!([]);
        }
        out.insert(record.uid.clone(), value);
    }
    if wants("assertions") || wants("assignees") {
        let assignment: HashSet<_> = if wants("assignees") {
            store::vocabulary::feature_binding(&store.pool, "assignment")
                .await?
                .concept_uids
                .into_iter()
                .collect()
        } else {
            HashSet::new()
        };
        let mut assignees = HashMap::<String, String>::new();
        let ids: Vec<_> = records.iter().map(|r| r.uid.clone()).collect();
        let mut assertions = Vec::new();
        for chunk in ids.chunks(400) {
            assertions.extend(
                store::assertions::for_subjects(&store.pool, chunk)
                    .await?
                    .into_iter()
                    .filter(|assertion| {
                        !visible.is_some_and(|visible| {
                            assertion
                                .object_uid
                                .as_ref()
                                .is_some_and(|uid| !visible.contains(uid))
                        })
                    }),
            );
        }
        if wants("assignees") {
            let people: HashSet<_> = assertions
                .iter()
                .filter(|assertion| assignment.contains(&assertion.predicate_uid))
                .filter_map(|assertion| assertion.object_uid.as_deref())
                .collect();
            let people: Vec<_> = people.into_iter().collect();
            assignees = store::records::live_heads_for(&store.pool, &people).await?;
        }
        for assertion in assertions {
            let Some(row) = out.get_mut(&assertion.subject_uid) else {
                continue;
            };
            if wants("assignees")
                && assignment.contains(&assertion.predicate_uid)
                && let Some(uid) = assertion.object_uid.as_deref()
                && let Some(head) = assignees.get(uid)
            {
                row["assignees"].as_array_mut().unwrap().push(json!({
                    "uid": uid,
                    "head": head,
                    "assertion": assertion.uid,
                    "predicate_uid": assertion.predicate_uid,
                }));
            }
            if wants("assertions") {
                let predicate = concept_names
                    .get(&assertion.predicate_uid)
                    .cloned()
                    .unwrap_or(assertion.predicate);
                row["assertions"]
                    .as_array_mut()
                    .unwrap()
                    .push(Value::from_iter([
                        ("uid", Value::from(assertion.uid)),
                        ("role", assertion.role.into()),
                        ("predicate", predicate.into()),
                        ("predicate_uid", assertion.predicate_uid.into()),
                        ("object", assertion.object_uid.into()),
                        ("uri", assertion.object_uri.into()),
                        ("resolved_object", assertion.resolved_object_uid.into()),
                        ("quantity", assertion.quantity.map(|q| q.to_string()).into()),
                        ("unit", assertion.unit_uid.into()),
                    ]));
            }
        }
    }
    Ok(out)
}
