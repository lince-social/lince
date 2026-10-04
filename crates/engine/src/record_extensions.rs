use crate::{
    Engine, EngineError,
    actions::{Action, ActionOutcome},
};
use chrono::{DateTime, Utc};
use nucleus::record_extension::{
    Definition, Inspection, Preset, Request, SCHEMA_NAMESPACE, Schema, VALUES_NAMESPACE, Values,
};
use protein::authority::{
    AssertionProperty, AssertionRole, AssertionTarget, ExtensionProperty, Operation, Property,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use store::sqlx::{Row, Sqlite, Transaction};

#[derive(Clone, Serialize, Deserialize)]
struct StoredSchema {
    revision: u64,
    schema: Schema,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct StoredValues {
    revision: u64,
    #[serde(default)]
    detached: bool,
    fields: BTreeMap<String, Value>,
    #[serde(default)]
    assertions: Vec<Managed>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Managed {
    assertion: String,
    preset: Preset,
    owned: bool,
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

fn property(namespace: &str, field: &str) -> Property {
    Property::Extension(ExtensionProperty {
        namespace: namespace.into(),
        field: field.into(),
    })
}

async fn extension_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    namespace: &str,
) -> Result<Value, EngineError> {
    let raw: Option<String> = store::sqlx::query_scalar(
        "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
    )
    .bind(uid)
    .bind(namespace)
    .fetch_optional(&mut **tx)
    .await?;
    raw.map(|raw| serde_json::from_str(&raw))
        .transpose()
        .map(|value| value.unwrap_or(json!({})))
        .map_err(EngineError::Json)
}

impl Engine {
    pub async fn record_extensions(
        &self,
        target: Option<String>,
        request: Request,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        match request {
            Request::Inspect { catalog, schemas } => Ok(ActionOutcome {
                data: Some(
                    serde_json::to_value(
                        self.inspect_extensions(target.as_deref(), catalog, &schemas, actor)
                            .await?,
                    )
                    .map_err(EngineError::Json)?,
                ),
                ..Default::default()
            }),
            Request::Create { id, schema } => {
                if target.is_some() {
                    return Err(invalid("Create a schema without an existing target"));
                }
                self.save_extension_schema(None, id, 0, schema, actor, now)
                    .await
            }
            Request::Save {
                id,
                expected_revision,
                schema,
            } => {
                let uid = self
                    .resolve(
                        target
                            .as_deref()
                            .ok_or_else(|| invalid("Choose a schema Record"))?,
                    )
                    .await?;
                self.save_extension_schema(Some(uid), id, expected_revision, schema, actor, now)
                    .await
            }
            Request::Apply {
                id,
                schema,
                expected_revision,
                expected_schema_revision,
                values,
                remove,
            } => {
                let uid = self
                    .resolve(
                        target
                            .as_deref()
                            .ok_or_else(|| invalid("Choose a Record"))?,
                    )
                    .await?;
                self.apply_extension_values(
                    uid,
                    id,
                    schema,
                    expected_revision,
                    expected_schema_revision,
                    values,
                    remove,
                    actor,
                    now,
                )
                .await
            }
        }
    }

    async fn require_extension_write(
        &self,
        uid: &str,
        namespace: &str,
        field: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.authorize_action(
            &Action::EditRecordText {
                target: uid.into(),
                head: None,
                body: None,
            },
            actor,
        )
        .await?;
        self.reject_direct_transfer_record_mutation(uid).await?;
        let required = property(namespace, field);
        if !self
            .record_property_permissions(actor, uid, BTreeSet::from([required.clone()]))
            .await?
            .contains(&required)
        {
            return Err(EngineError::Forbidden(
                "These extension fields are not writable".into(),
            ));
        }
        Ok(())
    }

    async fn load_extension_schema(
        &self,
        uid: &str,
        actor: Option<&str>,
    ) -> Result<StoredSchema, EngineError> {
        if !self.may_read_record(actor, uid).await? {
            return Err(EngineError::Forbidden("This schema is unavailable".into()));
        }
        let value = store::records::get_extension(&self.store.pool, uid, SCHEMA_NAMESPACE)
            .await?
            .ok_or_else(|| invalid("Choose a saved extension schema"))?;
        let schema: StoredSchema = serde_json::from_value(value).map_err(EngineError::Json)?;
        schema.schema.validate().map_err(invalid)?;
        Ok(schema)
    }

    pub async fn inspect_extensions(
        &self,
        target: Option<&str>,
        catalog: bool,
        requested: &[String],
        actor: Option<&str>,
    ) -> Result<Inspection, EngineError> {
        self.require_permission(actor, "record:read").await?;
        let mut out = Inspection::default();
        if requested.len() > 32 || requested.iter().any(|uid| !nucleus::valid_uid(uid, "r")) {
            return Err(invalid("Choose at most 32 saved schemas"));
        }
        let mut schemas = requested.iter().cloned().collect::<BTreeSet<_>>();
        if let Some(target) = target {
            let uid = self.resolve(target).await?;
            if !self.may_read_record(actor, &uid).await? {
                return Err(EngineError::Forbidden("This Record is unavailable".into()));
            }
            out.record = Some(uid.clone());
            if let Some(record) = store::records::get(&self.store.pool, &uid).await? {
                out.labels
                    .insert(uid.clone(), record.head.chars().take(160).collect());
            }
            if store::records::get_extension(&self.store.pool, &uid, SCHEMA_NAMESPACE)
                .await?
                .is_some()
            {
                schemas.insert(uid.clone());
            }
            let all = store::records::get_extension(&self.store.pool, &uid, VALUES_NAMESPACE)
                .await?
                .unwrap_or(json!({}));
            for (schema, value) in all
                .as_object()
                .ok_or_else(|| invalid("Invalid stored extension fields"))?
            {
                let stored: StoredValues =
                    serde_json::from_value(value.clone()).map_err(EngineError::Json)?;
                out.values.insert(
                    schema.clone(),
                    Values {
                        revision: stored.revision,
                        attached: !stored.detached,
                        fields: stored.fields,
                    },
                );
                if self
                    .require_extension_write(&uid, VALUES_NAMESPACE, schema, actor)
                    .await
                    .is_ok()
                {
                    out.writable.push(schema.clone());
                }
                schemas.insert(schema.clone());
            }
        }
        if catalog {
            let catalog: Vec<String> = store::sqlx::query_scalar("SELECT e.record_uid FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.deleted_at IS NULL ORDER BY r.head,r.uid LIMIT 256").bind(SCHEMA_NAMESPACE).fetch_all(&self.store.pool).await?;
            schemas.extend(catalog);
        }
        let priority = requested
            .iter()
            .cloned()
            .chain(out.values.keys().cloned())
            .chain(out.record.iter().cloned())
            .collect::<BTreeSet<_>>();
        let mut schemas = schemas.into_iter().collect::<Vec<_>>();
        schemas.sort_by_key(|uid| (!priority.contains(uid), uid.clone()));
        let mut bytes = serde_json::to_vec(&out.values)
            .map_err(EngineError::Json)?
            .len();
        for uid in schemas {
            match self.load_extension_schema(&uid, actor).await {
                Ok(stored) => {
                    let size = serde_json::to_vec(&stored.schema)
                        .map_err(EngineError::Json)?
                        .len();
                    if bytes + size > nucleus::record_extension::MAX_BYTES * 8 {
                        out.more = true;
                        continue;
                    }
                    bytes += size;
                    let editable = self
                        .require_extension_write(&uid, SCHEMA_NAMESPACE, "schema", actor)
                        .await
                        .is_ok();
                    if let Some(record) = &out.record {
                        if !out.writable.contains(&uid)
                            && self
                                .require_extension_write(record, VALUES_NAMESPACE, &uid, actor)
                                .await
                                .is_ok()
                        {
                            out.writable.push(uid.clone());
                        }
                    }
                    out.schemas.push(Definition {
                        uid,
                        revision: stored.revision,
                        schema: stored.schema,
                        editable,
                    });
                }
                Err(EngineError::Forbidden(_) | EngineError::UnknownRecord(_)) => {}
                Err(error) => return Err(error),
            }
        }
        out.schemas.sort_by(|a, b| {
            a.schema
                .name
                .to_lowercase()
                .cmp(&b.schema.name.to_lowercase())
                .then(a.uid.cmp(&b.uid))
        });
        let presets = out
            .schemas
            .iter()
            .flat_map(|entry| &entry.schema.fields)
            .flat_map(|field| &field.choices)
            .flat_map(|choice| &choice.assertions)
            .collect::<Vec<_>>();
        let concepts = presets
            .iter()
            .flat_map(|preset| std::iter::once(&preset.predicate).chain(preset.unit.iter()))
            .collect::<BTreeSet<_>>();
        if !concepts.is_empty() {
            let rows: Vec<(String, String)> =
                store::sqlx::query_as("SELECT uid,canonical_name FROM concept")
                    .fetch_all(&self.store.pool)
                    .await?;
            for (uid, name) in rows {
                if concepts.contains(&uid) {
                    if out.labels.len() < 4096 {
                        out.labels.insert(uid, name);
                    } else {
                        out.more = true;
                    }
                }
            }
        }
        let objects = presets
            .iter()
            .filter_map(|preset| preset.object.as_ref())
            .collect::<BTreeSet<_>>();
        if objects.len() > 512 {
            out.more = true;
        }
        for uid in objects.into_iter().take(512) {
            if self.may_read_record(actor, uid).await?
                && let Some(record) = store::records::get(&self.store.pool, uid).await?
            {
                out.labels
                    .insert(uid.clone(), record.head.chars().take(160).collect());
            }
        }
        Ok(out)
    }

    async fn resolve_preset(
        &self,
        mut preset: Preset,
        actor: Option<&str>,
    ) -> Result<Preset, EngineError> {
        preset.predicate = store::concepts::resolve(&self.store.pool, &preset.predicate)
            .await?
            .ok_or_else(|| invalid("Choose an existing assertion concept"))?;
        if let Some(object) = &preset.object {
            let uid = self.resolve(object).await?;
            if !self.may_read_record(actor, &uid).await? {
                return Err(EngineError::Forbidden(
                    "The preset's object is unavailable".into(),
                ));
            }
            preset.object = Some(uid);
        }
        if let Some(unit) = &preset.unit {
            preset.unit = Some(
                store::concepts::resolve(&self.store.pool, unit)
                    .await?
                    .ok_or_else(|| invalid("Choose an existing unit concept"))?,
            );
        }
        if let Some(quantity) = &preset.quantity {
            preset.quantity = Some(
                nucleus::DecimalValue::parse_inferred(quantity)
                    .map_err(|error| invalid(error.to_string()))?
                    .to_string(),
            );
        }
        if preset.unit.is_some() && preset.quantity.is_none() {
            return Err(invalid("An assertion with a unit also needs a quantity"));
        }
        if store::concepts::resolve(&self.store.pool, "descendant-of")
            .await?
            .as_deref()
            == Some(&preset.predicate)
        {
            return Err(invalid(
                "Prompt ancestry is configured through Fiote controls",
            ));
        }
        Ok(preset)
    }

    async fn receipt(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        id: &str,
        payload: &str,
    ) -> Result<Option<Value>, EngineError> {
        let prior: Option<(String, String)> = store::sqlx::query_as(
            "SELECT payload,result FROM record_change_receipt WHERE actor=? AND change_uid=?",
        )
        .bind(actor.unwrap_or(""))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
        match prior {
            Some((before, result)) if before == payload => Ok(Some(
                serde_json::from_str(&result).map_err(EngineError::Json)?,
            )),
            Some(_) => Err(invalid(
                "This change identity was already used for another edit",
            )),
            None => Ok(None),
        }
    }

    async fn completed_extension_change(
        &self,
        actor: Option<&str>,
        id: &str,
        payload: &str,
    ) -> Result<Option<Value>, EngineError> {
        let prior: Option<(String, String)> = store::sqlx::query_as(
            "SELECT payload,result FROM record_change_receipt WHERE actor=? AND change_uid=?",
        )
        .bind(actor.unwrap_or(""))
        .bind(id)
        .fetch_optional(&self.store.pool)
        .await?;
        match prior {
            Some((before, result)) if before == payload => Ok(Some(
                serde_json::from_str(&result).map_err(EngineError::Json)?,
            )),
            Some(_) => Err(invalid(
                "This change identity was already used for another edit",
            )),
            None => Ok(None),
        }
    }

    async fn commit_extension_change(
        &self,
        mut tx: Transaction<'_, Sqlite>,
        checkpoint: Option<crate::record_policy::Checkpoint>,
        serial: tokio::sync::MutexGuard<'_, ()>,
        signer: Option<crate::trust::Signer>,
        uid: &str,
        id: &str,
        payload: &str,
        data: Value,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        store::sqlx::query("INSERT INTO record_change_receipt (actor,change_uid,record_uid,payload,result) VALUES (?,?,?,?,?)").bind(actor.unwrap_or("")).bind(id).bind(uid).bind(payload).bind(data.to_string()).execute(&mut *tx).await?;
        let fact = crate::append::append_one_in_transaction(
            &mut tx,
            nucleus::NewFact {
                uid: None,
                record_uid: uid.into(),
                delta: store::exact::zero(),
                at: None,
                actor_uid: actor.map(str::to_owned),
                cause: nucleus::Cause::user_edit(),
                payload: Some(data.to_string()),
            },
            now,
            signer.as_ref(),
        )
        .await?;
        if let Some(checkpoint) = checkpoint { checkpoint.finish(&mut tx, Default::default()).await?; }
        tx.commit().await?;
        drop(serial);
        let mut outcome = ActionOutcome {
            data: Some(data),
            ..Default::default()
        };
        if let Some(fact) = fact {
            outcome.facts = self.observe_committed_fact(fact, now).await?;
        }
        Ok(outcome)
    }

    async fn save_extension_schema(
        &self,
        target: Option<String>,
        id: String,
        expected_revision: u64,
        mut schema: Schema,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if !nucleus::valid_uid(&id, "op") {
            return Err(invalid("A schema edit needs a durable change ID"));
        }
        schema.validate().map_err(invalid)?;
        let payload =
            json!({"target":target,"schema":schema,"expected_revision":expected_revision})
                .to_string();
        let created = target.is_none();
        if let Some(uid) = &target {
            self.require_extension_write(uid, SCHEMA_NAMESPACE, "schema", actor)
                .await?;
        } else {
            self.require_permission(actor, "record:create").await?;
        }
        if let Some(data) = self
            .completed_extension_change(actor, &id, &payload)
            .await?
        {
            return Ok(ActionOutcome {
                created: created.then(|| data["schema"].as_str().unwrap_or_default().into()),
                data: Some(data),
                ..Default::default()
            });
        }
        for field in &mut schema.fields {
            for choice in &mut field.choices {
                for preset in &mut choice.assertions {
                    *preset = self.resolve_preset(preset.clone(), actor).await?;
                }
            }
        }
        schema.validate().map_err(invalid)?;
        let uid = target.unwrap_or_else(|| nucleus::new_uid("r"));
        if created {
            self.authorize_extension_create(&uid, &schema, actor)
                .await?;
        }
        let organ = if created {
            Some(
                store::organs::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| invalid("No local Organ"))?
                    .uid,
            )
        } else {
            None
        };
        let signer = self.signer.lock().await.clone();
        let serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, if created { "record:create" } else { "record:update" }).await?;
        let checkpoint = self.record_checkpoint_on(&mut tx, actor, vec![uid.clone()], if created { protein::authority::Operation::Create } else { protein::authority::Operation::Update }).await?;
        if let Some(data) = self.receipt(&mut tx, actor, &id, &payload).await? {
            return Ok(ActionOutcome {
                created: created.then(|| data["schema"].as_str().unwrap_or_default().into()),
                data: Some(data),
                ..Default::default()
            });
        }
        if created {
            if expected_revision != 0 {
                return Err(invalid("A new schema starts at revision zero"));
            }
            store::records::create_with_uid_on(
                &mut tx,
                &uid,
                store::records::NewRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: &schema.name,
                    body: "Reusable extension schema",
                    quantity: store::exact::zero(),
                },
                organ.as_deref().unwrap(),
                None,
            )
            .await?;
            store::sqlx::query("INSERT INTO visibility_rule (uid, subject_kind, subject_uid, target_uid, grant_level) VALUES (?, 'organ', ?, ?, 'visible')").bind(nucleus::new_uid("v")).bind(organ.as_deref().unwrap()).bind(&uid).execute(&mut *tx).await?;
        }
        let before = extension_tx(&mut tx, &uid, SCHEMA_NAMESPACE).await?;
        let revision = if before.get("schema").is_some() {
            let before: StoredSchema = serde_json::from_value(before).map_err(EngineError::Json)?;
            schema.update_from(&before.schema).map_err(invalid)?;
            before.revision
        } else {
            0
        };
        if revision != expected_revision {
            return Err(invalid(
                "The schema changed. Reload it before saving your draft",
            ));
        }
        let revision = revision
            .checked_add(1)
            .ok_or_else(|| invalid("Schema revision overflow"))?;
        let stored = StoredSchema { revision, schema };
        store::records::set_extension_on(
            &mut tx,
            &uid,
            SCHEMA_NAMESPACE,
            &serde_json::to_value(stored).map_err(EngineError::Json)?,
        )
        .await?;
        let mut outcome = self
            .commit_extension_change(
                tx,
                checkpoint,
                serial,
                signer,
                &uid,
                &id,
                &payload,
                json!({"schema":uid,"revision":revision}),
                actor,
                now,
            )
            .await?;
        outcome.created = created.then_some(uid);
        Ok(outcome)
    }

    async fn authorize_preset(
        &self,
        uid: &str,
        preset: &Preset,
        retract: Option<&str>,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let action = match retract {
            Some(assertion) => Action::RetractAssertion {
                assertion: assertion.into(),
            },
            None => Action::AssertRecord {
                subject: uid.into(),
                predicate: preset.predicate.clone(),
                object: preset.object.clone(),
                quantity: preset.quantity.clone(),
                unit: preset.unit.clone(),
            },
        };
        self.authorize_action(&action, actor).await?;
        let Some(actor) = actor else {
            return Ok(());
        };
        let Some(policy) = protein::role_authority::policy_for(&self.store, actor, Some(Operation::Update)).await? else { return Ok(()); };
        for grant in policy.grants {
            if grant.operation != Operation::Update {
                continue;
            }
            let assertions = if retract.is_some() {
                &grant.assertions_remove
            } else {
                &grant.assertions_add
            };
            if !assertions.iter().any(|grant| {
                grant.predicate_uid == preset.predicate
                    && grant.role == AssertionRole::Ordinary
                    && match (&grant.target, &preset.object) {
                        (AssertionTarget::Unary, None)
                        | (AssertionTarget::AnyReadableRecord, Some(_)) => true,
                        (AssertionTarget::Record(target), Some(object)) => target == object,
                        _ => false,
                    }
                    && (retract.is_some()
                        || preset.quantity.is_none()
                        || grant.properties.contains(&AssertionProperty::Quantity))
                    && (retract.is_some()
                        || preset.unit.is_none()
                        || grant.properties.contains(&AssertionProperty::Unit))
            }) {
                continue;
            }
            let query: protein::Protein = serde_json::from_value(json!({"source":"record","where":[{"uid_eq":uid}, grant.selector],"fields":["uid"],"limit":1})).map_err(EngineError::Json)?;
            if !protein::execute_for(&self.store, &query, Some(actor))
                .await?
                .is_empty()
            {
                return Ok(());
            }
        }
        Err(EngineError::Forbidden(
            "A preset assertion is not writable".into(),
        ))
    }

    async fn authorize_extension_create(
        &self,
        uid: &str,
        schema: &Schema,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        let draft = crate::record_creation::Draft {
            uid: uid.into(),
            head: schema.name.clone(),
            body: "Reusable extension schema".into(),
            ..Default::default()
        };
        let family = std::collections::HashSet::new();
        let matches = |selector: &protein::Predicate| {
            draft.matches(selector, &family, store::exact::zero()) == Some(true)
        };
        if let Some(filter) = protein::read_rules::effective_predicate(&self.store, actor).await?
            && !matches(&filter)
        {
            return Err(EngineError::Forbidden(
                "This schema would be outside your access rules".into(),
            ));
        }
        let Some(policy) = protein::role_authority::policy_for(&self.store, actor, Some(Operation::Create)).await? else { return Ok(()); };
        let allowed = policy
            .grants
            .iter()
            .filter(|grant| grant.operation == Operation::Create && matches(&grant.selector))
            .flat_map(|grant| grant.properties.iter().cloned())
            .collect::<BTreeSet<_>>();
        let required = BTreeSet::from([
            Property::Kind,
            Property::Head,
            Property::Body,
            Property::Quantity,
            property(SCHEMA_NAMESPACE, "schema"),
            property(SCHEMA_NAMESPACE, "revision"),
        ]);
        if !required.is_subset(&allowed) {
            return Err(EngineError::Forbidden(
                "Your role cannot create extension schemas".into(),
            ));
        }
        Ok(())
    }

    async fn apply_extension_values(
        &self,
        uid: String,
        id: String,
        schema_uid: String,
        expected_revision: u64,
        expected_schema_revision: u64,
        values: BTreeMap<String, Value>,
        remove: bool,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if !nucleus::valid_uid(&id, "op")
            || !nucleus::valid_uid(&schema_uid, "r")
            || values.len() > 64
        {
            return Err(invalid("Invalid extension edit identities or field count"));
        }
        if remove && !values.is_empty() {
            return Err(invalid("Remove a schema without editing its values"));
        }
        self.require_extension_write(&uid, VALUES_NAMESPACE, &schema_uid, actor)
            .await?;
        let payload = json!({"record":uid,"schema":schema_uid,"expected_revision":expected_revision,"expected_schema_revision":expected_schema_revision,"values":values,"remove":remove}).to_string();
        if let Some(data) = self
            .completed_extension_change(actor, &id, &payload)
            .await?
        {
            return Ok(ActionOutcome {
                data: Some(data),
                ..Default::default()
            });
        }
        let definition = self.load_extension_schema(&schema_uid, actor).await?;
        if definition.revision != expected_schema_revision {
            return Err(invalid(
                "The schema changed. Reload and review its choices before applying your draft",
            ));
        }
        let before = store::records::get_extension(&self.store.pool, &uid, VALUES_NAMESPACE)
            .await?
            .unwrap_or(json!({}));
        let old: StoredValues = before
            .get(&schema_uid)
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(EngineError::Json)?
            .unwrap_or_default();
        let mut next = old.clone();
        for (id, value) in &values {
            let field = definition
                .schema
                .fields
                .iter()
                .find(|field| field.id == *id)
                .ok_or_else(|| invalid("Unknown field; reload the schema"))?;
            field
                .validate_value(value, old.fields.get(id).unwrap_or(&Value::Null))
                .map_err(invalid)?;
            if value.is_null() {
                next.fields.remove(id);
            } else {
                next.fields.insert(id.clone(), value.clone());
            }
        }
        let mut wanted = Vec::new();
        if !remove {
            for field in &definition.schema.fields {
                for choice in field
                    .selection(next.fields.get(&field.id).unwrap_or(&Value::Null))
                    .map_err(invalid)?
                {
                    let choice = field
                        .choices
                        .iter()
                        .find(|entry| entry.id == choice)
                        .ok_or_else(|| invalid("A selected choice is missing"))?;
                    for preset in &choice.assertions {
                        if !wanted.contains(preset) {
                            wanted.push(self.resolve_preset(preset.clone(), actor).await?);
                        }
                    }
                }
            }
        }
        if wanted.len() > 512 {
            return Err(invalid("Select at most 512 preset assertions"));
        }
        for preset in &wanted {
            self.authorize_preset(&uid, preset, None, actor).await?;
        }
        let mut removals = Vec::new();
        for managed in &old.assertions {
            if !managed.owned || wanted.contains(&managed.preset) {
                continue;
            }
            let referenced = before
                .as_object()
                .ok_or_else(|| invalid("Invalid extension storage"))?
                .iter()
                .filter(|(key, _)| *key != &schema_uid)
                .any(|(_, value)| {
                    value["assertions"].as_array().is_some_and(|list| {
                        list.iter()
                            .any(|value| value["assertion"] == managed.assertion)
                    })
                });
            if referenced {
                continue;
            }
            if let Some(row) = store::assertions::get(&self.store.pool, &managed.assertion).await? {
                if row.subject_uid != uid {
                    continue;
                }
                let quantity = managed
                    .preset
                    .quantity
                    .as_deref()
                    .map(nucleus::DecimalValue::parse_inferred)
                    .transpose()
                    .map_err(|error| invalid(error.to_string()))?;
                if row.role != "ordinary"
                    || row.predicate_uid != managed.preset.predicate
                    || row.object_uid != managed.preset.object
                    || row.quantity != quantity
                    || row.unit_uid != managed.preset.unit
                {
                    return Err(invalid(
                        "A managed assertion changed; review it before updating this selection",
                    ));
                }
                if row.retracted_at.is_none() {
                    self.authorize_preset(&uid, &managed.preset, Some(&managed.assertion), actor)
                        .await?;
                    removals.push((managed.assertion.clone(), managed.preset.clone()));
                }
            }
        }
        let signer = self.signer.lock().await.clone();
        let serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:update").await?;
        let checkpoint = self.record_checkpoint_on(&mut tx, actor, vec![uid.clone()], protein::authority::Operation::Update).await?;
        if let Some(data) = self.receipt(&mut tx, actor, &id, &payload).await? {
            return Ok(ActionOutcome {
                data: Some(data),
                ..Default::default()
            });
        }
        let current = extension_tx(&mut tx, &uid, VALUES_NAMESPACE).await?;
        if current != before || old.revision != expected_revision {
            return Err(invalid(
                "These fields changed. Reload them before saving your draft",
            ));
        }
        let current_schema: StoredSchema =
            serde_json::from_value(extension_tx(&mut tx, &schema_uid, SCHEMA_NAMESPACE).await?)
                .map_err(EngineError::Json)?;
        if current_schema.revision != definition.revision {
            return Err(invalid(
                "The schema changed; reload it before selecting options",
            ));
        }
        for (assertion, preset) in removals {
            let row=store::sqlx::query("SELECT subject_uid,predicate_uid,object_uid,role,quantity_mantissa,quantity_scale,unit_uid FROM record_assertion WHERE uid=? AND retracted_at IS NULL").bind(&assertion).fetch_optional(&mut *tx).await?;
            if let Some(row) = row {
                let quantity = row
                    .try_get::<Option<String>, _>("quantity_mantissa")?
                    .map(|_| store::exact::read_decimal(&row, "quantity"))
                    .transpose()?;
                let expected = preset
                    .quantity
                    .as_deref()
                    .map(nucleus::DecimalValue::parse_inferred)
                    .transpose()
                    .map_err(|error| invalid(error.to_string()))?;
                if row.try_get::<String, _>("subject_uid")? != uid
                    || row.try_get::<String, _>("predicate_uid")? != preset.predicate
                    || row.try_get::<Option<String>, _>("object_uid")? != preset.object
                    || row.try_get::<String, _>("role")? != "ordinary"
                    || quantity != expected
                    || row.try_get::<Option<String>, _>("unit_uid")? != preset.unit
                {
                    return Err(invalid(
                        "A managed assertion changed; review it before updating this selection",
                    ));
                }
            }
            store::assertions::retract_tx(&mut tx, &assertion, actor).await?;
        }
        next.assertions.clear();
        for preset in wanted {
            let existing = store::sqlx::query("SELECT uid,role,quantity_mantissa,quantity_scale,unit_uid FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND object_uid IS ? AND retracted_at IS NULL ORDER BY uid LIMIT 1").bind(&uid).bind(&preset.predicate).bind(&preset.object).fetch_optional(&mut *tx).await?;
            let quantity = preset
                .quantity
                .as_deref()
                .map(nucleus::DecimalValue::parse_inferred)
                .transpose()
                .map_err(|error| invalid(error.to_string()))?;
            let (assertion, owned) = if let Some(row) = existing {
                let assertion: String = row.try_get("uid")?;
                let existing_quantity = row
                    .try_get::<Option<String>, _>("quantity_mantissa")?
                    .map(|_| store::exact::read_decimal(&row, "quantity"))
                    .transpose()?;
                if row.try_get::<String, _>("role")? != "ordinary"
                    || existing_quantity != quantity
                    || row.try_get::<Option<String>, _>("unit_uid")? != preset.unit
                {
                    return Err(invalid(
                        "An existing assertion has different properties; review it before selecting this option",
                    ));
                }
                let owned = old
                    .assertions
                    .iter()
                    .any(|entry| entry.assertion == assertion && entry.owned)
                    || before.as_object().unwrap().values().any(|value| {
                        value["assertions"].as_array().is_some_and(|list| {
                            list.iter().any(|value| {
                                value["assertion"] == assertion && value["owned"] == true
                            })
                        })
                    });
                (assertion, owned)
            } else {
                let assertion = nucleus::new_uid("a");
                store::assertions::insert_tx(
                    &mut tx,
                    &assertion,
                    store::assertions::NewAssertion {
                        subject_uid: &uid,
                        predicate_uid: &preset.predicate,
                        object_uid: preset.object.as_deref(),
                        role: store::assertions::AssertionRole::Ordinary,
                        quantity,
                        unit_uid: preset.unit.as_deref(),
                        asserted_by: actor,
                    },
                )
                .await?;
                (assertion, true)
            };
            next.assertions.push(Managed {
                assertion,
                preset,
                owned,
            });
        }
        next.revision = old
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("Field revision overflow"))?;
        let mut all = current;
        next.detached = remove;
        all[&schema_uid] = serde_json::to_value(&next).map_err(EngineError::Json)?;
        if all.as_object().unwrap().len() > 128
            || serde_json::to_vec(&all).map_err(EngineError::Json)?.len()
                > nucleus::record_extension::MAX_BYTES
        {
            return Err(invalid("This Record has too many extension fields"));
        }
        store::records::set_extension_on(&mut tx, &uid, VALUES_NAMESPACE, &all).await?;
        self.commit_extension_change(
            tx,
            checkpoint,
            serial,
            signer,
            &uid,
            &id,
            &payload,
            json!({"record":uid,"schema":schema_uid,"revision":next.revision,"removed":remove}),
            actor,
            now,
        )
        .await
    }
}
