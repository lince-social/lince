use crate::{Engine, EngineError, actions::ActionOutcome, instinct::BundledRecord};
use nucleus::{Cause, DecimalValue, NewFact, RecordKind};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use store::sqlx::{Row, Sqlite, Transaction};

#[derive(Clone, Debug, Serialize)]
pub struct Preview {
    pub fingerprint: String,
    pub records: Vec<BundledRecord>,
    pub created: usize,
    pub reused: usize,
    pub conflicts: Vec<String>,
    pub vocabulary: String,
    pub concepts: BTreeMap<String, String>,
    pub states: BTreeMap<String, String>,
}

fn canonical_uid(prefix: &str, name: &str) -> String {
    let hash = Sha256::digest(format!("Lince Instinct/{prefix}/{name}"));
    let entropy = u128::from_be_bytes(hash[..16].try_into().unwrap());
    format!("{prefix}_{}", nucleus::ulid_from(0, entropy))
}

fn failure(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

fn decimal(source: &str) -> Result<DecimalValue, EngineError> {
    DecimalValue::parse_inferred(source).map_err(|error| failure(error.to_string()))
}

fn expected_assertions(
    record: &BundledRecord,
    concepts: &BTreeMap<String, String>,
) -> Result<Vec<Value>, EngineError> {
    let mut values = Vec::new();
    for assertion in &record.projection.assertions {
        let amount = assertion
            .quantity
            .as_deref()
            .map(decimal)
            .transpose()?
            .map(store::exact::decimal_columns);
        values.push(json!({
            "predicate": concepts[&assertion.predicate],
            "object": assertion.object.as_ref().map(|object| &object.uid),
            "role": if assertion.identity { "identity" } else { "ordinary" },
            "quantity": amount,
            "unit": assertion.unit.as_ref().map(|name| &concepts[name]),
        }));
    }
    values.sort_by_key(Value::to_string);
    Ok(values)
}

impl Engine {
    async fn require_instinct_owner(&self, actor: Option<&str>) -> Result<(), EngineError> {
        self.require_permission(actor, "record:create").await?;
        if actor.is_some() {
            return Err(EngineError::Forbidden(
                "Handbook import requires the local owner session.".into(),
            ));
        }
        Ok(())
    }

    async fn instinct_preview_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<Preview, EngineError> {
        let records = crate::instinct::records().map_err(|error| failure(error.to_string()))?;
        if records.is_empty() {
            return Err(failure("Instinct is not enabled in this build."));
        }
        let vocabulary = canonical_uid("g", "vocabulary");
        let mut names = BTreeSet::new();
        for record in &records {
            for assertion in &record.projection.assertions {
                names.insert(assertion.predicate.clone());
                names.extend(assertion.unit.iter().cloned());
            }
            names.extend(
                record
                    .projection
                    .quantity
                    .as_ref()
                    .and_then(|(_, unit)| unit.clone()),
            );
        }
        let concepts: BTreeMap<_, _> = names
            .into_iter()
            .map(|name| {
                let uid = canonical_uid("c", &name);
                (name, uid)
            })
            .collect();
        let mut conflicts = Vec::new();
        let mut snapshot = Vec::new();
        let local: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug = ? AND kind = 'organ' AND deleted_at IS NULL",
        )
        .bind(store::organs::LOCAL_ORGAN_SLUG)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| failure("The local Organ is unavailable."))?;
        let lingua =
            store::sqlx::query("SELECT name, owner_organ, visibility FROM lingua WHERE uid = ?")
                .bind(&vocabulary)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(row) = lingua {
            let name: String = row.try_get("name")?;
            let owner: Option<String> = row.try_get("owner_organ")?;
            let visibility: String = row.try_get("visibility")?;
            snapshot.push(json!([vocabulary, name, owner, visibility]));
            if name != "Lince Instinct"
                || owner.as_deref() != Some(&local)
                || visibility != "private"
            {
                conflicts
                    .push("The handbook Vocabulary has different ownership or meaning.".into());
            }
        }
        for (name, uid) in &concepts {
            let row =
                store::sqlx::query("SELECT canonical_name, instinct FROM concept WHERE uid = ?")
                    .bind(uid)
                    .fetch_optional(&mut **tx)
                    .await?;
            let members: Vec<String> = store::sqlx::query_scalar(
                "SELECT lingua_uid FROM lingua_concept WHERE concept_uid = ? ORDER BY lingua_uid",
            )
            .bind(uid)
            .fetch_all(&mut **tx)
            .await?;
            let parents: Vec<String> = store::sqlx::query_scalar(
                "SELECT parent_uid FROM concept_parent WHERE concept_uid = ? ORDER BY parent_uid",
            )
            .bind(uid)
            .fetch_all(&mut **tx)
            .await?;
            let aliases: Vec<(String, String)> = store::sqlx::query_as(
                "SELECT lang, name FROM concept_name WHERE concept_uid = ? ORDER BY lang, name",
            )
            .bind(uid)
            .fetch_all(&mut **tx)
            .await?;
            let definition = row
                .as_ref()
                .map(|row| row.try_get::<Option<String>, _>("instinct"))
                .transpose()?
                .flatten();
            let canonical = row
                .map(|row| row.try_get::<String, _>("canonical_name"))
                .transpose()?;
            snapshot.push(json!([
                uid, canonical, definition, members, parents, aliases
            ]));
            if canonical.as_deref().is_some_and(|value| value != name)
                || canonical.is_some()
                    && (members != [vocabulary.clone()]
                        || !parents.is_empty()
                        || !aliases.is_empty()
                        || definition.is_some())
            {
                conflicts.push(format!(
                    "Handbook concept #{name} has a different name, Vocabulary or definition."
                ));
            }
        }
        let mut created = 0;
        let mut reused = 0;
        let mut states = BTreeMap::new();
        for record in &records {
            let uid = &record.projection.uid;
            let label = record.slug.as_ref().map_or_else(
                || record.head.clone(),
                |slug| format!("{} (@{slug})", record.head),
            );
            let owners: Vec<String> = if let Some(slug) = &record.slug {
                store::sqlx::query_scalar("SELECT uid FROM record WHERE slug = ? ORDER BY uid")
                    .bind(slug)
                    .fetch_all(&mut **tx)
                    .await?
            } else {
                Vec::new()
            };
            let row = store::sqlx::query("SELECT kind, head, body, slug, quantity_mantissa, quantity_scale, unit_uid, deleted_at, organ_uid FROM record WHERE uid = ?").bind(uid).fetch_optional(&mut **tx).await?;
            let assertions = store::sqlx::query("SELECT predicate_uid, object_uid, role, quantity_mantissa, quantity_scale, unit_uid FROM record_assertion WHERE subject_uid = ? AND retracted_at IS NULL").bind(uid).fetch_all(&mut **tx).await?;
            let mut active = Vec::new();
            for row in assertions {
                let amount = row
                    .try_get::<Option<String>, _>("quantity_mantissa")?
                    .map(|mantissa| {
                        Ok::<_, store::sqlx::Error>((
                            mantissa,
                            row.try_get::<i64, _>("quantity_scale")?,
                        ))
                    })
                    .transpose()?;
                active.push(json!({"predicate": row.try_get::<String, _>("predicate_uid")?, "object": row.try_get::<Option<String>, _>("object_uid")?, "role": row.try_get::<String, _>("role")?, "quantity": amount, "unit": row.try_get::<Option<String>, _>("unit_uid")?}));
            }
            active.sort_by_key(Value::to_string);
            let row_value = if let Some(row) = &row {
                let quantity = store::exact::read_decimal(row, "quantity")?;
                Some(
                    json!({"kind": row.try_get::<String, _>("kind")?, "head": row.try_get::<String, _>("head")?, "body": row.try_get::<String, _>("body")?, "slug": row.try_get::<Option<String>, _>("slug")?, "quantity": quantity.to_string(), "unit": row.try_get::<Option<String>, _>("unit_uid")?, "deleted": row.try_get::<Option<String>, _>("deleted_at")?, "organ": row.try_get::<Option<String>, _>("organ_uid")?}),
                )
            } else {
                None
            };
            snapshot.push(json!([uid, owners, row_value, active]));
            let before = conflicts.len();
            if owners.iter().any(|owner| owner != uid) {
                conflicts.push(format!("{label}: the slug belongs to another Record."));
            }
            if let Some(row) = row_value {
                let amount = record.quantity().unwrap_or_else(|| "0".into());
                let unit = record
                    .projection
                    .quantity
                    .as_ref()
                    .and_then(|(_, unit)| unit.as_ref())
                    .map(|name| &concepts[name]);
                if row["head"] != record.head
                    || row["body"] != record.body
                    || row["slug"] != json!(record.slug)
                    || row["kind"] != "plain"
                    || !row["deleted"].is_null()
                    || row["organ"] != local
                    || !decimal(row["quantity"].as_str().unwrap_or(""))?
                        .exact_numeric_cmp(decimal(&amount)?)
                        .is_eq()
                    || row["unit"] != json!(unit)
                    || active != expected_assertions(record, &concepts)?
                {
                    conflicts.push(format!("{label}: this UID already has different content, quantity, identity or Assertions."));
                } else {
                    reused += 1;
                }
            } else {
                created += 1;
            }
            states.insert(
                uid.clone(),
                if conflicts.len() > before {
                    "conflict"
                } else if row.is_some() {
                    "reuse"
                } else {
                    "create"
                }
                .into(),
            );
        }
        let fingerprint = Sha256::digest(serde_json::to_vec(&json!([records, snapshot]))?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Preview {
            fingerprint,
            records,
            created,
            reused,
            conflicts,
            vocabulary,
            concepts,
            states,
        })
    }

    pub async fn preview_instinct(&self, actor: Option<&str>) -> Result<Preview, EngineError> {
        self.require_instinct_owner(actor).await?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let preview = self.instinct_preview_on(&mut tx).await?;
        tx.rollback().await?;
        Ok(preview)
    }

    pub async fn import_instinct(
        &self,
        fingerprint: &str,
        actor: Option<&str>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_instinct_owner(actor).await?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let preview = self.instinct_preview_on(&mut tx).await?;
        if preview.fingerprint != fingerprint {
            return Err(failure(
                "The handbook preview changed. Preview again before importing.",
            ));
        }
        if !preview.conflicts.is_empty() {
            return Err(failure(preview.conflicts.join("\n")));
        }
        let local: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug = ? AND kind = 'organ' AND deleted_at IS NULL",
        )
        .bind(store::organs::LOCAL_ORGAN_SLUG)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| failure("The local Organ is unavailable."))?;
        store::sqlx::query("INSERT INTO lingua (uid, name, owner_organ, visibility, created_at) VALUES (?, 'Lince Instinct', ?, 'private', ?) ON CONFLICT(uid) DO NOTHING")
            .bind(&preview.vocabulary).bind(&local).bind(now.to_rfc3339()).execute(&mut *tx).await?;
        for (name, uid) in &preview.concepts {
            let created = store::sqlx::query("INSERT INTO concept (uid, canonical_name, created_at) VALUES (?, ?, ?) ON CONFLICT(uid) DO NOTHING").bind(uid).bind(name).bind(now.to_rfc3339()).execute(&mut *tx).await?.rows_affected() > 0;
            store::sqlx::query("INSERT INTO lingua_concept (lingua_uid, concept_uid, adopted_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING").bind(&preview.vocabulary).bind(uid).bind(now.to_rfc3339()).execute(&mut *tx).await?;
            if created {
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "concept",
                    uid,
                    "canonical_name",
                    store::sync_ops::OpKind::Set,
                    Some(json!(name).to_string()),
                )
                .await?;
            }
        }
        for record in &preview.records {
            if preview.states[&record.projection.uid] != "create" {
                continue;
            }
            store::records::create_with_uid_on(
                &mut tx,
                &record.projection.uid,
                store::records::NewRecord {
                    slug: record.slug.as_deref(),
                    kind: RecordKind::Plain,
                    head: &record.head,
                    body: &record.body,
                    quantity: store::exact::zero(),
                },
                &local,
                None,
            )
            .await?;
        }
        let signer = self.signer.lock().await.clone();
        let mut facts = Vec::new();
        for record in &preview.records {
            let uid = &record.projection.uid;
            if preview.states[uid] != "create" {
                continue;
            }
            for assertion in &record.projection.assertions {
                store::assertions::insert_tx(
                    &mut tx,
                    &nucleus::new_uid("a"),
                    store::assertions::NewAssertion {
                        subject_uid: uid,
                        predicate_uid: &preview.concepts[&assertion.predicate],
                        object_uid: assertion.object.as_ref().map(|object| object.uid.as_str()),
                        role: if assertion.identity {
                            store::assertions::AssertionRole::Identity
                        } else {
                            store::assertions::AssertionRole::Ordinary
                        },
                        quantity: assertion.quantity.as_deref().map(decimal).transpose()?,
                        unit_uid: assertion
                            .unit
                            .as_ref()
                            .map(|name| preview.concepts[name].as_str()),
                        asserted_by: actor,
                    },
                )
                .await?;
            }
            let unit = record
                .projection
                .quantity
                .as_ref()
                .and_then(|(_, unit)| unit.as_ref())
                .map(|name| preview.concepts[name].as_str());
            if unit.is_some() {
                store::records::set_unit_on(&mut tx, uid, unit).await?;
            }
            let amount = decimal(&record.quantity().unwrap_or_else(|| "0".into()))?;
            if let Some(fact) = crate::append::append_one_in_transaction(
                &mut tx,
                NewFact {
                    actor_uid: actor.map(str::to_owned),
                    ..NewFact::quantity(uid.clone(), amount, Cause::user_edit())
                },
                now,
                signer.as_ref(),
            )
            .await?
            {
                facts.push(fact);
            }
        }
        tx.commit().await?;
        let mut observed = Vec::new();
        for fact in facts {
            observed.extend(self.publish_committed_fact(fact));
        }
        self.query_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(ActionOutcome {
            facts: observed,
            data: Some(
                json!({"created": preview.created, "reused": preview.reused, "conflicting": 0, "records": preview.records.iter().map(|record| &record.projection.uid).collect::<Vec<_>>()}),
            ),
            ..Default::default()
        })
    }
}
