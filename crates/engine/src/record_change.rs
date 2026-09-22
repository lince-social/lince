use chrono::Utc;
use nucleus::DecimalValue;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use store::sqlx::{Row, Sqlite, Transaction};

use crate::{Engine, EngineError, actions::ActionOutcome};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub record_uid: String,
    pub mutation: Mutation,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mutation {
    Text {
        update_base64: String,
    },
    Quantity {
        value: String,
    },
    Slug {
        value: Option<String>,
    },
    Work {
        field: WorkField,
        value: Value,
    },
    Timer {
        running: bool,
    },
    WorkLog {
        log_id: String,
        value: Option<Value>,
    },
    WorkMetadata {
        value: Value,
    },
    Assertion {
        predicate: String,
        object: Option<String>,
        quantity: Option<String>,
        unit: Option<String>,
    },
    NumberAssertion {
        predicate: String,
        position: u32,
    },
    RetractAssertion {
        assertion: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkField {
    Start,
    Due,
    Estimate,
}

impl WorkField {
    pub fn key(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Due => "due",
            Self::Estimate => "estimate_min",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub clock: i64,
    pub peer: String,
    pub change_uid: String,
    pub value: Value,
}

impl Register {
    fn rank(&self) -> (i64, &str, &str) {
        (self.clock, &self.peer, &self.change_uid)
    }
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

fn decimal(value: &str) -> Result<DecimalValue, EngineError> {
    DecimalValue::parse_inferred(value).map_err(|_| invalid("Enter an exact decimal quantity"))
}

async fn current_register(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    property: &str,
) -> Result<Option<Register>, EngineError> {
    let row = store::sqlx::query("SELECT clock, peer, change_uid, value FROM record_property WHERE record_uid = ? AND property = ?")
        .bind(uid).bind(property).fetch_optional(&mut **tx).await?;
    row.map(|row| {
        Ok(Register {
            clock: row.get("clock"),
            peer: row.get("peer"),
            change_uid: row.get("change_uid"),
            value: serde_json::from_str(row.get::<&str, _>("value")).map_err(EngineError::Json)?,
        })
    })
    .transpose()
}

async fn work_on(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<Value, EngineError> {
    let raw: Option<String> = store::sqlx::query_scalar(
        "SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = 'work'",
    )
    .bind(uid)
    .fetch_optional(&mut **tx)
    .await?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(EngineError::Json))
        .unwrap_or_else(|| Ok(json!({})))
}

async fn save_work(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    work: &Value,
) -> Result<(), EngineError> {
    crate::private_work::WorkMetadata::parse(work).map_err(invalid_display)?;
    store::sqlx::query("INSERT INTO record_extension (record_uid, namespace, version, fds) VALUES (?, 'work', 1, ?) ON CONFLICT(record_uid, namespace) DO UPDATE SET fds = excluded.fds")
        .bind(uid).bind(work.to_string()).execute(&mut **tx).await?;
    Ok(())
}

fn invalid_display(error: impl std::fmt::Display) -> EngineError {
    invalid(error.to_string())
}

async fn put_register(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    property: &str,
    register: &Register,
) -> Result<(), EngineError> {
    store::sqlx::query("INSERT INTO record_property (record_uid, property, clock, peer, change_uid, value) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(record_uid, property) DO UPDATE SET clock = excluded.clock, peer = excluded.peer, change_uid = excluded.change_uid, value = excluded.value")
        .bind(uid).bind(property).bind(register.clock).bind(&register.peer).bind(&register.change_uid).bind(register.value.to_string())
        .execute(&mut **tx).await?;
    Ok(())
}

async fn seed_logs(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<(), EngineError> {
    let count: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM record_property WHERE record_uid = ? AND property LIKE 'work.log:%'",
    )
    .bind(uid)
    .fetch_one(&mut **tx)
    .await?;
    if count != 0 {
        return Ok(());
    }
    let work = work_on(tx, uid).await?;
    for (index, value) in work["logs"].as_array().into_iter().flatten().enumerate() {
        let key = format!("work.log:seed-{index}");
        put_register(
            tx,
            uid,
            &key,
            &Register {
                clock: 0,
                peer: String::new(),
                change_uid: key.clone(),
                value: value.clone(),
            },
        )
        .await?;
    }
    Ok(())
}

async fn project_logs(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<(), EngineError> {
    let rows: Vec<String> = store::sqlx::query_scalar("SELECT value FROM record_property WHERE record_uid = ? AND property LIKE 'work.log:%' AND json_type(value) = 'object' ORDER BY json_extract(value, '$.start'), property")
        .bind(uid).fetch_all(&mut **tx).await?;
    let mut work = work_on(tx, uid).await?;
    work["logs"] = Value::Array(
        rows.into_iter()
            .map(|raw| serde_json::from_str(&raw))
            .collect::<Result<Vec<Value>, _>>()
            .map_err(EngineError::Json)?,
    );
    save_work(tx, uid, &work).await
}

async fn apply_register(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    property: &str,
    register: &Register,
) -> Result<bool, EngineError> {
    if register.peer.len() > 128
        || register.change_uid.len() > 128
        || register.value.to_string().len() > crate::private_work::MAX_WORK_BYTES
    {
        return Err(invalid("Property change exceeds its limits"));
    }
    let previous = current_register(tx, uid, property).await?;
    if previous.as_ref().is_some_and(|previous| {
        previous.rank() == register.rank() && previous.value != register.value
    }) {
        return Err(invalid(
            "Property operation identity has conflicting content",
        ));
    }
    if previous
        .as_ref()
        .is_some_and(|previous| previous.rank() >= register.rank())
    {
        return Ok(false);
    }
    match property {
        "quantity" => {
            let offset = decimal(
                register.value["offset"]
                    .as_str()
                    .ok_or_else(|| invalid("Quantity change has no offset"))?,
            )?;
            let old = previous
                .as_ref()
                .and_then(|value| value.value["offset"].as_str())
                .map(decimal)
                .transpose()?
                .unwrap_or_else(store::exact::zero);
            store::records::bump_quantity(
                tx,
                uid,
                store::exact::difference(offset, old)?,
                &Utc::now().to_rfc3339(),
            )
            .await?;
        }
        "slug" => {
            let slug = match &register.value {
                Value::Null => None,
                Value::String(value) if nucleus::valid_slug(value) => Some(value.as_str()),
                _ => return Err(invalid("Invalid slug")),
            };
            let old_slug: Option<String> =
                store::sqlx::query_scalar("SELECT slug FROM record WHERE uid = ?")
                    .bind(uid)
                    .fetch_one(&mut **tx)
                    .await?;
            put_register(tx, uid, property, register).await?;
            store::sqlx::query("UPDATE record SET slug = NULL WHERE uid = ?")
                .bind(uid)
                .execute(&mut **tx)
                .await?;
            let mut claims: Vec<String> = [
                old_slug.as_deref(),
                previous
                    .as_ref()
                    .and_then(|previous| previous.value.as_str()),
                slug,
            ]
            .into_iter()
            .flatten()
            .map(str::to_owned)
            .collect();
            claims.sort();
            claims.dedup();
            for claim in claims {
                let winner: Option<String> = store::sqlx::query_scalar("SELECT p.record_uid FROM record_property p JOIN record r ON r.uid = p.record_uid WHERE p.property = 'slug' AND json_extract(p.value, '$') = ? AND r.deleted_at IS NULL ORDER BY p.clock DESC, p.peer DESC, p.change_uid DESC, p.record_uid DESC LIMIT 1")
                    .bind(&claim).fetch_optional(&mut **tx).await?;
                if let Some(winner) = winner {
                    store::sqlx::query("UPDATE record SET slug = NULL WHERE slug = ?")
                        .bind(&claim)
                        .execute(&mut **tx)
                        .await?;
                    store::sqlx::query("UPDATE record SET slug = ?, updated_at = ? WHERE uid = ?")
                        .bind(&claim)
                        .bind(Utc::now().to_rfc3339())
                        .bind(winner)
                        .execute(&mut **tx)
                        .await?;
                }
            }
        }
        "work.start" | "work.due" | "work.estimate_min" => {
            let mut work = work_on(tx, uid).await?;
            let field = property.strip_prefix("work.").expect("work field");
            if register.value.is_null() {
                if let Some(work) = work.as_object_mut() {
                    work.remove(field);
                }
            } else {
                work[field] = register.value.clone();
            }
            save_work(tx, uid, &work).await?;
        }
        property if property.starts_with("work.log:") && property.len() <= 160 => {
            if !register.value.is_null() {
                crate::private_work::WorkMetadata::parse(&json!({"logs": [register.value]}))
                    .map_err(invalid_display)?;
            }
            seed_logs(tx, uid).await?;
        }
        _ => return Err(invalid("Unsupported collaborative property")),
    }
    put_register(tx, uid, property, register).await?;
    if property.starts_with("work.log:") {
        project_logs(tx, uid).await?;
    }
    Ok(true)
}

impl Engine {
    pub(crate) async fn project_work_registers(&self, uid: &str) -> Result<(), EngineError> {
        let mut tx = store::write_tx(&self.store.pool).await?;
        let rows: Vec<(String, String)> = store::sqlx::query_as("SELECT property, value FROM record_property WHERE record_uid = ? AND property IN ('work.start', 'work.due', 'work.estimate_min')").bind(uid).fetch_all(&mut *tx).await?;
        let mut work = work_on(&mut tx, uid).await?;
        for (property, raw) in &rows {
            let field = property.strip_prefix("work.").expect("work field");
            let value: Value = serde_json::from_str(raw).map_err(EngineError::Json)?;
            if value.is_null() {
                if let Some(work) = work.as_object_mut() {
                    work.remove(field);
                }
            } else {
                work[field] = value;
            }
        }
        if !rows.is_empty() {
            save_work(&mut tx, uid, &work).await?;
        }
        let logs: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_property WHERE record_uid = ? AND property LIKE 'work.log:%')").bind(uid).fetch_one(&mut *tx).await?;
        if logs {
            project_logs(&mut tx, uid).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn prepare_property_rebuild(&self) -> Result<(), EngineError> {
        let mut tx = store::write_tx(&self.store.pool).await?;
        let quantities: Vec<(String, String)> = store::sqlx::query_as(
            "SELECT record_uid, value FROM record_property WHERE property = 'quantity'",
        )
        .fetch_all(&mut *tx)
        .await?;
        for (uid, raw) in quantities {
            let value: Value = serde_json::from_str(&raw).map_err(EngineError::Json)?;
            let offset = decimal(
                value["offset"]
                    .as_str()
                    .ok_or_else(|| invalid("Missing quantity offset"))?,
            )?;
            store::records::bump_quantity(
                &mut tx,
                &uid,
                store::exact::difference(store::exact::zero(), offset)?,
                &Utc::now().to_rfc3339(),
            )
            .await?;
        }
        store::sqlx::query("UPDATE record SET slug = NULL WHERE uid IN (SELECT record_uid FROM record_property WHERE property = 'slug')").execute(&mut *tx).await?;
        store::sqlx::query("UPDATE record_extension SET fds = json_set(fds, '$.logs', json('[]')) WHERE namespace = 'work' AND record_uid IN (SELECT record_uid FROM record_property WHERE property LIKE 'work.log:%')").execute(&mut *tx).await?;
        store::sqlx::query("DELETE FROM record_property")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn materialise_property_op(
        &self,
        op: &crate::sync::WireOp,
    ) -> Result<bool, EngineError> {
        let register: Register = serde_json::from_str(
            op.value
                .as_deref()
                .ok_or_else(|| invalid("Missing property register"))?,
        )
        .map_err(EngineError::Json)?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let changed = apply_register(
            &mut tx,
            &op.uid,
            op.field
                .strip_prefix("property:")
                .ok_or_else(|| invalid("Missing property name"))?,
            &register,
        )
        .await?;
        tx.commit().await?;
        Ok(changed)
    }

    pub async fn load_record_edit_draft(
        &self,
        source: &str,
        uid: &str,
    ) -> Result<Option<String>, EngineError> {
        Ok(store::sqlx::query_scalar(
            "SELECT state FROM record_edit_draft WHERE source = ? AND record_uid = ?",
        )
        .bind(source)
        .bind(uid)
        .fetch_optional(&self.store.pool)
        .await?)
    }

    pub async fn save_record_edit_draft(
        &self,
        source: &str,
        uid: &str,
        state: &str,
    ) -> Result<(), EngineError> {
        if state.len() > crate::collab::limits().snapshot_bytes * 4 {
            return Err(invalid("Pending edits exceed their storage limit"));
        }
        store::sqlx::query("INSERT INTO record_edit_draft (source, record_uid, state, updated_at) VALUES (?, ?, ?, ?) ON CONFLICT(source, record_uid) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at")
            .bind(source).bind(uid).bind(state).bind(Utc::now().to_rfc3339()).execute(&self.store.pool).await?;
        Ok(())
    }

    pub async fn clear_record_edit_draft(
        &self,
        source: &str,
        uid: &str,
    ) -> Result<(), EngineError> {
        store::sqlx::query("DELETE FROM record_edit_draft WHERE source = ? AND record_uid = ?")
            .bind(source)
            .bind(uid)
            .execute(&self.store.pool)
            .await?;
        Ok(())
    }

    pub async fn change_record(
        &self,
        request: Request,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        self.access_scope(true, self.change_record_inner(request, actor))
            .await
    }

    async fn change_record_inner(
        &self,
        request: Request,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        if !nucleus::valid_uid(&request.id, "op") || !nucleus::valid_uid(&request.record_uid, "r") {
            return Err(invalid(
                "A property change needs durable Record and change identities",
            ));
        }
        let uid = &request.record_uid;
        self.authorize_action(
            &crate::actions::Action::EditRecordText {
                target: uid.clone(),
                head: None,
                body: None,
            },
            actor,
        )
        .await?;
        self.reject_direct_transfer_record_mutation(uid).await?;
        use protein::authority::{ExtensionProperty, Property};
        let required = match &request.mutation {
            Mutation::Text { .. }
            | Mutation::WorkMetadata { .. }
            | Mutation::Assertion { .. }
            | Mutation::NumberAssertion { .. }
            | Mutation::RetractAssertion { .. } => None,
            Mutation::Quantity { .. } => Some(Property::Quantity),
            Mutation::Slug { .. } => Some(Property::Slug),
            Mutation::Work { field, .. } => Some(Property::Extension(ExtensionProperty {
                namespace: "work".into(),
                field: field.key().into(),
            })),
            Mutation::Timer { .. } | Mutation::WorkLog { .. } => {
                Some(Property::Extension(ExtensionProperty {
                    namespace: "work".into(),
                    field: "logs".into(),
                }))
            }
        };
        if let Some(property) = required {
            if !self
                .record_property_permissions(
                    actor,
                    uid,
                    std::collections::BTreeSet::from([property.clone()]),
                )
                .await?
                .contains(&property)
            {
                return Err(EngineError::Forbidden(
                    "This Record property is not writable".into(),
                ));
            }
        }
        if matches!(request.mutation, Mutation::WorkMetadata { .. }) {
            let required: std::collections::BTreeSet<_> = ["start", "due", "estimate_min", "logs"]
                .into_iter()
                .map(|field| {
                    Property::Extension(ExtensionProperty {
                        namespace: "work".into(),
                        field: field.into(),
                    })
                })
                .collect();
            if self
                .record_property_permissions(actor, uid, required.clone())
                .await?
                != required
            {
                return Err(EngineError::Forbidden(
                    "Work metadata is not writable".into(),
                ));
            }
        }
        let payload = serde_json::to_string(&request).map_err(EngineError::Json)?;
        if payload.len() > crate::collab::limits().delta_bytes * 2 {
            return Err(invalid("Property change is too large"));
        }
        if let Mutation::Text { update_base64 } = &request.mutation {
            let facts = self
                .apply_text_change_as(uid, update_base64, actor, Some((&request.id, &payload)))
                .await?;
            return Ok(ActionOutcome {
                facts,
                data: Some(json!({"change_id": request.id, "state": "saved"})),
                ..Default::default()
            });
        }
        if matches!(
            request.mutation,
            Mutation::Assertion { .. }
                | Mutation::NumberAssertion { .. }
                | Mutation::RetractAssertion { .. }
        ) {
            return self.change_assertion(&request, actor, &payload).await;
        }
        let identity = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("Local Cell is unavailable"))?;
        let signer = self.signer.lock().await.clone();
        let _serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let prior: Option<(String, String)> = store::sqlx::query_as(
            "SELECT payload, result FROM record_change_receipt WHERE actor = ? AND change_uid = ?",
        )
        .bind(actor.unwrap_or(""))
        .bind(&request.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((original, result)) = prior {
            if original != payload {
                return Err(invalid("Change identity was already used for another edit"));
            }
            return Ok(ActionOutcome {
                data: Some(serde_json::from_str(&result).map_err(EngineError::Json)?),
                ..Default::default()
            });
        }
        let row = store::sqlx::query("SELECT quantity_mantissa, quantity_scale FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid).fetch_optional(&mut *tx).await?.ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
        let mut related = Vec::new();
        let (property, value) = match &request.mutation {
            Mutation::WorkMetadata { value } => {
                crate::private_work::WorkMetadata::parse(value).map_err(invalid_display)?;
                seed_logs(&mut tx, uid).await?;
                for field in ["start", "due", "estimate_min"] {
                    related.push((format!("work.{field}"), value[field].clone()));
                }
                let mut existing: Vec<(String, String)> = store::sqlx::query_as("SELECT property, value FROM record_property WHERE record_uid = ? AND property LIKE 'work.log:%' AND json_type(value) = 'object' ORDER BY property").bind(uid).fetch_all(&mut *tx).await?;
                for (index, log) in value["logs"].as_array().into_iter().flatten().enumerate() {
                    let found = existing.iter().position(|(_, raw)| {
                        serde_json::from_str::<Value>(raw)
                            .is_ok_and(|old| old["start"] == log["start"])
                    });
                    let key = found
                        .map(|index| existing.remove(index).0)
                        .unwrap_or_else(|| format!("work.log:{}-{index}", request.id));
                    related.push((key, log.clone()));
                }
                for (key, _) in existing {
                    related.push((key, Value::Null));
                }
                related.pop().expect("work fields")
            }
            Mutation::WorkLog { log_id, value } => {
                if !log_id.starts_with("work.log:") || log_id.len() > 160 {
                    return Err(invalid("Invalid work log identity"));
                }
                (log_id.clone(), value.clone().unwrap_or(Value::Null))
            }
            Mutation::Quantity { value } => {
                let target = decimal(value.trim())?;
                let current = store::exact::read_decimal(&row, "quantity")?;
                let previous = current_register(&mut tx, uid, "quantity").await?;
                let offset = previous
                    .as_ref()
                    .and_then(|value| value.value["offset"].as_str())
                    .map(decimal)
                    .transpose()?
                    .unwrap_or_else(store::exact::zero);
                let total = store::exact::difference(current, offset)?;
                (
                    "quantity".into(),
                    json!({"offset": store::exact::difference(target, total)?.to_string(), "assigned": target.to_string()}),
                )
            }
            Mutation::Slug { value } => {
                if let Some(slug) = value {
                    if !nucleus::valid_slug(slug) {
                        return Err(invalid("Invalid slug"));
                    }
                    let taken: bool = store::sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM record WHERE slug = ? AND uid != ?)",
                    )
                    .bind(slug)
                    .bind(uid)
                    .fetch_one(&mut *tx)
                    .await?;
                    if taken {
                        return Err(invalid("That slug belongs to another Record"));
                    }
                }
                ("slug".into(), json!(value))
            }
            Mutation::Work { field, value } => (format!("work.{}", field.key()), value.clone()),
            Mutation::Timer { running } => {
                seed_logs(&mut tx, uid).await?;
                let mut active: Vec<(String, String)> = store::sqlx::query_as("SELECT property, value FROM record_property WHERE record_uid = ? AND property LIKE 'work.log:%' AND json_type(value) = 'object' AND json_extract(value, '$.end') IS NULL ORDER BY clock DESC")
                    .bind(uid).fetch_all(&mut *tx).await?;
                match (*running, active.pop()) {
                    (true, None) => (
                        format!("work.log:{}", request.id),
                        json!({"start": Utc::now().to_rfc3339(), "end": null}),
                    ),
                    (false, Some((key, raw))) => {
                        for (key, raw) in active {
                            let mut log: Value =
                                serde_json::from_str(&raw).map_err(EngineError::Json)?;
                            log["end"] = json!(Utc::now().to_rfc3339());
                            related.push((key, log));
                        }
                        let mut log: Value =
                            serde_json::from_str(&raw).map_err(EngineError::Json)?;
                        log["end"] = json!(Utc::now().to_rfc3339());
                        (key, log)
                    }
                    _ => {
                        let data =
                            json!({"change_id": request.id, "state": "saved", "changed": false});
                        store::sqlx::query("INSERT INTO record_change_receipt (actor, change_uid, record_uid, payload, result) VALUES (?, ?, ?, ?, ?)")
                            .bind(actor.unwrap_or("")).bind(&request.id).bind(uid).bind(&payload).bind(data.to_string()).execute(&mut *tx).await?;
                        tx.commit().await?;
                        return Ok(ActionOutcome {
                            data: Some(data),
                            ..Default::default()
                        });
                    }
                }
            }
            Mutation::Text { .. }
            | Mutation::Assertion { .. }
            | Mutation::NumberAssertion { .. }
            | Mutation::RetractAssertion { .. } => unreachable!(),
        };
        related.push((property, value));
        let mut changed = false;
        let mut changes = Vec::new();
        for (property, value) in related {
            let register = Register {
                clock: nucleus::hlc::next(),
                peer: identity.uid.clone(),
                change_uid: request.id.clone(),
                value,
            };
            changed |= apply_register(&mut tx, uid, &property, &register).await?;
            store::sync_ops::log_local_tx(
                &mut tx,
                "record",
                uid,
                &format!("property:{property}"),
                store::sync_ops::OpKind::Set,
                Some(serde_json::to_string(&register).map_err(EngineError::Json)?),
            )
            .await?;
            changes.push(json!({"property":property,"value":register.value}));
        }
        let now = Utc::now();
        let fact = crate::append::append_one_in_transaction(
            &mut tx,
            nucleus::NewFact {
                uid: None,
                record_uid: uid.clone(),
                delta: store::exact::zero(),
                at: None,
                actor_uid: actor.map(str::to_owned),
                cause: nucleus::Cause::user_edit(),
                payload: Some(json!({"change_id":request.id,"changes":changes}).to_string()),
            },
            now,
            signer.as_ref(),
        )
        .await?;
        let data = json!({"change_id":request.id,"state":"saved","changed":changed});
        store::sqlx::query("INSERT INTO record_change_receipt (actor, change_uid, record_uid, payload, result) VALUES (?, ?, ?, ?, ?)")
            .bind(actor.unwrap_or("")).bind(&request.id).bind(uid).bind(payload).bind(data.to_string()).execute(&mut *tx).await?;
        tx.commit().await?;
        drop(_serial);
        let mut outcome = ActionOutcome {
            data: Some(data),
            ..Default::default()
        };
        if let Some(fact) = fact {
            outcome.facts = self.observe_committed_fact(fact, now).await?;
        }
        Ok(outcome)
    }

    async fn change_assertion(
        &self,
        request: &Request,
        actor: Option<&str>,
        payload: &str,
    ) -> Result<ActionOutcome, EngineError> {
        let uid = &request.record_uid;
        let numbering = matches!(request.mutation, Mutation::NumberAssertion { .. });
        let (predicate, object, quantity, unit, retract) = match &request.mutation {
            Mutation::NumberAssertion {
                predicate,
                position,
            } => {
                if *position == 0 {
                    return Err(invalid("Assertion numbering starts at 1"));
                }
                self.authorize_action(
                    &crate::actions::Action::AssertRecord {
                        subject: uid.clone(),
                        predicate: predicate.clone(),
                        object: None,
                        quantity: Some(position.to_string()),
                        unit: None,
                    },
                    actor,
                )
                .await?;
                let predicate = store::concepts::resolve(&self.store.pool, predicate)
                    .await?
                    .ok_or_else(|| invalid("Unknown assertion"))?;
                (
                    predicate,
                    None,
                    Some(decimal(&position.to_string())?),
                    None,
                    None,
                )
            }
            Mutation::Assertion {
                predicate,
                object,
                quantity,
                unit,
            } => {
                self.authorize_action(
                    &crate::actions::Action::AssertRecord {
                        subject: uid.clone(),
                        predicate: predicate.clone(),
                        object: object.clone(),
                        quantity: quantity.clone(),
                        unit: unit.clone(),
                    },
                    actor,
                )
                .await?;
                let predicate = store::concepts::resolve(&self.store.pool, predicate)
                    .await?
                    .ok_or_else(|| invalid("Unknown assertion"))?;
                let object = match object {
                    Some(value) => Some(self.resolve(value).await?),
                    None => None,
                };
                let unit = match unit {
                    Some(value) => Some(
                        store::concepts::resolve(&self.store.pool, value)
                            .await?
                            .ok_or_else(|| invalid("Unknown assertion unit"))?,
                    ),
                    None => None,
                };
                (
                    predicate,
                    object,
                    quantity.as_deref().map(decimal).transpose()?,
                    unit,
                    None,
                )
            }
            Mutation::RetractAssertion { assertion } => {
                self.authorize_action(
                    &crate::actions::Action::RetractAssertion {
                        assertion: assertion.clone(),
                    },
                    actor,
                )
                .await?;
                let row = store::assertions::get(&self.store.pool, assertion)
                    .await?
                    .ok_or_else(|| invalid("Unknown assertion"))?;
                if row.subject_uid != *uid {
                    return Err(invalid("Choose an assertion attached to this Record"));
                }
                (
                    row.predicate_uid,
                    row.object_uid,
                    row.quantity,
                    row.unit_uid,
                    Some(assertion.clone()),
                )
            }
            _ => unreachable!(),
        };
        if retract.is_none()
            && store::concepts::resolve(&self.store.pool, "descendant-of").await?.as_deref() == Some(&predicate)
            && store::records::get_extension(&self.store.pool, uid, "lince.fiote").await?.is_some()
        {
            let parent = object.as_deref().ok_or_else(|| invalid("Choose a Fiote parent."))?;
            if self.fiote_parent(uid).await?.is_some_and(|existing| existing != parent) {
                return Err(invalid("Remove the current prompt parent before choosing another."));
            }
            self.validate_fiote_parent(uid, Some(parent), actor).await?;
        }
        if let Some(actor) = actor {
            if let Some(role) = store::auth::person_access(&self.store.pool, actor)
                .await?
                .and_then(|person| person.role_id)
            {
                if let Some(policy) = store::role_policies::get(&self.store.pool, role)
                    .await?
                    .and_then(|row| row.policy)
                {
                    use protein::authority::{
                        AssertionProperty, AssertionRole, AssertionTarget, Operation, RolePolicy,
                    };
                    let policy: RolePolicy =
                        serde_json::from_value(policy).map_err(EngineError::Json)?;
                    let role = if let Some(assertion) = &retract {
                        if store::assertions::get(&self.store.pool, assertion)
                            .await?
                            .is_some_and(|row| row.role == "identity")
                        {
                            AssertionRole::Identity
                        } else {
                            AssertionRole::Ordinary
                        }
                    } else {
                        AssertionRole::Ordinary
                    };
                    let mut allowed = false;
                    for grant in policy.grants {
                        if grant.operation != Operation::Update {
                            continue;
                        }
                        if numbering
                            && !grant.assertions_remove.iter().any(|grant| {
                                grant.predicate_uid == predicate
                                    && grant.role == AssertionRole::Ordinary
                                    && matches!(grant.target, AssertionTarget::Unary)
                            })
                        {
                            continue;
                        }
                        let assertions = if retract.is_some() {
                            grant.assertions_remove
                        } else {
                            grant.assertions_add
                        };
                        if !assertions.iter().any(|grant| {
                            grant.predicate_uid == predicate
                                && grant.role == role
                                && match (&grant.target, &object) {
                                    (AssertionTarget::Unary, None)
                                    | (AssertionTarget::AnyReadableRecord, Some(_)) => true,
                                    (AssertionTarget::Record(target), Some(object)) => {
                                        target == object
                                    }
                                    _ => false,
                                }
                                && (retract.is_some()
                                    || quantity.is_none()
                                    || grant.properties.contains(&AssertionProperty::Quantity))
                                && (retract.is_some()
                                    || unit.is_none()
                                    || grant.properties.contains(&AssertionProperty::Unit))
                        }) {
                            continue;
                        }
                        let query = protein::Protein {
                            source: protein::Source::Record,
                            filter: vec![protein::Predicate::UidEq(uid.clone()), grant.selector],
                            fields: Some(vec!["uid".into()]),
                            include: Default::default(),
                            aggregate: None,
                            order: vec![],
                            limit: Some(1),
                        };
                        if !protein::execute_for(&self.store, &query, Some(actor))
                            .await?
                            .is_empty()
                        {
                            allowed = true;
                            break;
                        }
                    }
                    if !allowed {
                        return Err(EngineError::Forbidden(
                            "This assertion is not writable".into(),
                        ));
                    }
                }
            }
        }
        let signer = self.signer.lock().await.clone();
        let serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let prior: Option<(String, String)> = store::sqlx::query_as(
            "SELECT payload, result FROM record_change_receipt WHERE actor = ? AND change_uid = ?",
        )
        .bind(actor.unwrap_or(""))
        .bind(&request.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((original, result)) = prior {
            if original != payload {
                return Err(invalid("Change identity was already used for another edit"));
            }
            let data: Value = serde_json::from_str(&result).map_err(EngineError::Json)?;
            return Ok(ActionOutcome {
                created: data["assertion"]
                    .as_str()
                    .filter(|_| retract.is_none())
                    .map(str::to_owned),
                data: Some(data),
                ..Default::default()
            });
        }
        let (assertion, changed) = if let Some(assertion) = &retract {
            (
                assertion.clone(),
                store::assertions::retract_tx(&mut tx, assertion, actor).await?,
            )
        } else {
            let existing: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ? AND retracted_at IS NULL ORDER BY uid").bind(uid).bind(&predicate).bind(&object).fetch_all(&mut *tx).await?;
            if numbering && existing.len() > 1 {
                return Err(invalid("Resolve duplicate assertions before numbering"));
            }
            let existing = existing.into_iter().next();
            if let Some(assertion) = existing {
                if numbering {
                    let previous = store::sqlx::query("SELECT role, unit_uid, quantity_mantissa, quantity_scale FROM record_assertion WHERE uid = ?").bind(&assertion).fetch_one(&mut *tx).await?;
                    if previous.get::<String, _>("role") == "identity" {
                        return Err(invalid("Identity assertions cannot be numbered"));
                    }
                    if previous.get::<Option<String>, _>("unit_uid").is_some() {
                        return Err(invalid("Choose an assertion without units for numbering"));
                    }
                    let old_quantity = previous
                        .get::<Option<String>, _>("quantity_mantissa")
                        .map(|_| store::exact::read_decimal(&previous, "quantity"))
                        .transpose()?;
                    let changed = old_quantity != quantity;
                    let row = store::assertions::set_quantity_tx(
                        &mut tx,
                        &assertion,
                        store::assertions::AssertionQuantity {
                            quantity,
                            unit_uid: None,
                        },
                    )
                    .await?;
                    (row.uid, changed)
                } else {
                    (assertion, false)
                }
            } else {
                let assertion = nucleus::new_uid("a");
                store::assertions::insert_tx(
                    &mut tx,
                    &assertion,
                    store::assertions::NewAssertion {
                        subject_uid: uid,
                        predicate_uid: &predicate,
                        object_uid: object.as_deref(),
                        role: store::assertions::AssertionRole::Ordinary,
                        quantity,
                        unit_uid: unit.as_deref(),
                        asserted_by: actor,
                    },
                )
                .await?;
                (assertion, true)
            }
        };
        let data = json!({"change_id": request.id, "state": "saved", "changed": changed, "assertion": assertion, "operation": if retract.is_some() { "remove" } else if numbering { "quantity" } else { "add" }});
        store::sqlx::query("INSERT INTO record_change_receipt (actor, change_uid, record_uid, payload, result) VALUES (?, ?, ?, ?, ?)").bind(actor.unwrap_or("")).bind(&request.id).bind(uid).bind(payload).bind(data.to_string()).execute(&mut *tx).await?;
        let now = Utc::now();
        let mut facts = Vec::new();
        if changed {
            let targets: std::collections::BTreeSet<_> =
                std::iter::once(uid.clone()).chain(object).collect();
            for target in targets {
                if let Some(fact) = crate::append::append_one_in_transaction(
                    &mut tx,
                    nucleus::NewFact {
                        uid: None,
                        record_uid: target,
                        delta: store::exact::zero(),
                        at: None,
                        actor_uid: actor.map(str::to_owned),
                        cause: nucleus::Cause::user_edit(),
                        payload: Some(data.to_string()),
                    },
                    now,
                    signer.as_ref(),
                )
                .await?
                {
                    facts.push(fact);
                }
            }
        }
        tx.commit().await?;
        drop(serial);
        let mut outcome = ActionOutcome {
            created: retract.is_none().then_some(assertion),
            data: Some(data),
            ..Default::default()
        };
        for fact in facts {
            outcome
                .facts
                .extend(self.observe_committed_fact(fact, now).await?);
        }
        Ok(outcome)
    }

    pub(crate) async fn import_property_op(
        &self,
        op: &crate::sync::WireOp,
        root: Option<&str>,
    ) -> Result<bool, EngineError> {
        let key = op
            .field
            .strip_prefix("property:")
            .ok_or_else(|| invalid("Missing property name"))?;
        let raw = op
            .value
            .as_deref()
            .ok_or_else(|| invalid("Missing property value"))?;
        if raw.len() > crate::private_work::MAX_WORK_BYTES {
            return Err(invalid("Property value is too large"));
        }
        let register: Register = serde_json::from_str(raw).map_err(EngineError::Json)?;
        if register.clock <= 0 || !nucleus::hlc::within_drift(register.clock) {
            return Err(invalid("Invalid property clock"));
        }
        let owner = store::records::get(&self.store.pool, &op.uid)
            .await?
            .and_then(|row| row.organ_uid);
        let local = store::cells::local(&self.store.pool).await?;
        let relay = local.is_some_and(|local| {
            local.organ_uid != op.organ_uid && owner.as_deref() == Some(local.organ_uid.as_str())
        });
        if register.peer != op.actor_cell && owner.as_deref() != Some(op.organ_uid.as_str()) {
            return Err(invalid("Property writer does not match the sending Cell"));
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        let changed = apply_register(&mut tx, &op.uid, key, &register).await?;
        if changed && relay {
            store::sync_ops::log_local_tx(
                &mut tx,
                "record",
                &op.uid,
                &op.field,
                store::sync_ops::OpKind::Set,
                op.value.clone(),
            )
            .await?;
        }
        store::sqlx::query("INSERT OR IGNORE INTO sync_op (tbl, uid, field, kind, value, hlc, actor_cell, organ_uid, replica_root) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&op.tbl).bind(&op.uid).bind(&op.field).bind(&op.kind).bind(&op.value).bind(op.hlc)
            .bind(&op.actor_cell).bind(&op.organ_uid).bind(root).execute(&mut *tx).await?;
        tx.commit().await?;
        nucleus::hlc::observe(register.clock);
        Ok(changed)
    }
}
