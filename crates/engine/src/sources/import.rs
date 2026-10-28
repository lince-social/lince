use super::*;
use crate::actions::Action;
use lince_resources::{INSTANCE_NAMESPACE, SOURCE_NAMESPACE};
use nucleus::{Cause, DecimalValue, NewFact, RecordKind};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};

fn identity(prefix: &str, value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!(
        "{prefix}_{}",
        nucleus::ulid_from(0, u128::from_be_bytes(digest[..16].try_into().unwrap()))
    )
}

fn decimal(value: &str) -> Result<DecimalValue, EngineError> {
    DecimalValue::parse_inferred(value).map_err(invalid)
}

fn origin(document: &lince_resources::publication::Document) -> Option<&Value> {
    document
        .project
        .extensions
        .iter()
        .filter(|extension| extension.namespace == SOURCE_NAMESPACE)
        .find_map(|extension| {
            extension
                .fields
                .get("origin")
                .filter(|value| !value.is_null())
        })
}

struct Prepared {
    organ: String,
    vocabulary: String,
    binding: String,
    key: String,
    bound_state: bool,
    bindings: BTreeMap<String, String>,
    records: BTreeMap<String, String>,
    concepts: BTreeMap<String, String>,
    definitions: BTreeMap<String, String>,
    personal: BTreeSet<String>,
    bound: BTreeSet<String>,
    slugs: BTreeMap<String, String>,
}

impl Prepared {
    fn body(&self, text: &str) -> String {
        let mut body = text.to_owned();
        for (slug, uid) in &self.slugs {
            body = body.replace(&format!("[[{slug}]]"), &format!("[[{uid}]]"));
        }
        for (original, uid) in &self.records {
            body = body.replace(&format!("[[{original}]]"), &format!("[[{uid}]]"));
        }
        body
    }
}

fn selected_record(source: &Publication) -> Option<&anicca::ProjectedRecord> {
    source.fragment.as_ref().and_then(|uid| {
        source
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .find(|record| &record.uid == uid)
    })
}

fn label(source: &Publication) -> &str {
    selected_record(source).map_or("Source documents", |record| record.head.as_str())
}

impl Engine {
    async fn prepare_source(
        &self,
        source: &Publication,
        mut bindings: BTreeMap<String, String>,
        actor: Option<&str>,
    ) -> Result<Prepared, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("The local Organ is unavailable"))?
            .uid;
        let records: Vec<_> = source
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .collect();
        let mut bound = BTreeSet::new();
        bindings.retain(|name, _| source.parameters.contains_key(name));
        for (name, input) in &source.parameters {
            if !bindings.contains_key(name) && input.default == "actor" {
                if let Some(actor) = actor {
                    bindings.insert(name.clone(), actor.into());
                } else {
                    let people = store::auth::list_actors(&self.store.pool).await?;
                    if people.len() == 1 {
                        bindings.insert(name.clone(), people[0].0.clone());
                    }
                }
            }
            let uid = bindings
                .get_mut(name)
                .ok_or_else(|| invalid(format!("Choose {}", input.label)))?;
            *uid = self.resolve(uid).await?;
            self.refuse_unreadable(actor, std::slice::from_ref(uid))
                .await?;
            let row = store::records::get(&self.store.pool, uid)
                .await?
                .ok_or_else(|| invalid("The chosen input is unavailable"))?;
            if input.kind == "person" {
                if row.kind != "person" {
                    return Err(invalid("Choose a Person for this input"));
                }
                if actor.is_some_and(|actor| actor != uid) {
                    return Err(EngineError::Forbidden(
                        "Bind personal source data to your own Person".into(),
                    ));
                }
            }
            let record = records
                .iter()
                .find(|record| record.slug.as_deref() == Some(name))
                .ok_or_else(|| {
                    invalid(format!(
                        "The source input @{name} has no placeholder Record"
                    ))
                })?;
            bound.insert(record.uid.clone());
        }
        let inputs: BTreeMap<_, _> = records
            .iter()
            .filter_map(|record| {
                Some((
                    record.uid.as_str(),
                    bindings.get(record.slug.as_deref()?)?.as_str(),
                ))
            })
            .collect();
        let binding_key = format!("{organ}:{}", serde_json::to_string(&inputs)?);
        let vocabulary = identity("g", &format!("source-vocabulary:{organ}"));
        let modules: BTreeSet<_> = source
            .documents
            .iter()
            .flat_map(|document| &document.project.extensions)
            .filter(|extension| {
                extension.namespace == SOURCE_NAMESPACE
                    && (extension.fields["instances"]
                        .as_array()
                        .is_some_and(|instances| !instances.is_empty())
                        || extension.fields["parameters"]
                            .as_object()
                            .is_some_and(|parameters| !parameters.is_empty()))
            })
            .filter_map(|extension| extension.target_uid.as_deref())
            .collect();
        let binding = identity(
            "r",
            &format!(
                "binding:{binding_key}:{}",
                modules.into_iter().collect::<Vec<_>>().join("|")
            ),
        );
        let mut mapping = BTreeMap::new();
        let mut personal = BTreeSet::new();
        for record in &records {
            let name = record.slug.as_deref().unwrap_or("");
            let local = if let Some(value) = bindings.get(name) {
                value.clone()
            } else if source.instances.contains(name) {
                personal.insert(record.uid.clone());
                identity("r", &format!("{binding_key}:{}", record.uid))
            } else {
                identity("r", &format!("source:{organ}:{}", record.uid))
            };
            mapping.insert(record.uid.clone(), local);
        }
        let mut definitions = BTreeMap::new();
        let mut definition_origins = BTreeMap::new();
        for document in &source.documents {
            for extension in &document.project.extensions {
                if extension.namespace == "lince.vocabulary"
                    && let Some(values) = extension.fields["definitions"].as_object()
                {
                    for (name, value) in values {
                        let definition = value
                            .as_str()
                            .ok_or_else(|| invalid("A Vocabulary definition must be text"))?;
                        if definition_origins.contains_key(name)
                            && definitions
                                .get(name)
                                .is_some_and(|current| current != definition)
                        {
                            return Err(invalid(format!(
                                "Conflicting Vocabulary definitions for #{name}"
                            )));
                        }
                        definitions.insert(name.clone(), definition.into());
                        definition_origins.entry(name.clone()).or_insert_with(|| {
                            extension
                                .target_uid
                                .clone()
                                .unwrap_or_else(|| document.uri.clone())
                        });
                    }
                }
            }
            for record in &document.project.records {
                for assertion in &record.assertions {
                    definitions.entry(assertion.predicate.clone()).or_default();
                    if let Some(unit) = &assertion.unit {
                        definitions.entry(unit.clone()).or_default();
                    }
                }
                if let Some((_, Some(unit))) = &record.quantity {
                    definitions.entry(unit.clone()).or_default();
                }
            }
        }
        let concepts = definitions
            .iter()
            .map(|(name, definition)| {
                (
                    name.clone(),
                    identity(
                        "c",
                        &format!(
                            "source:{organ}:{}:{name}:{definition}",
                            definition_origins
                                .get(name)
                                .map_or("inferred", String::as_str)
                        ),
                    ),
                )
            })
            .collect();
        let slugs = records
            .iter()
            .filter_map(|record| {
                record
                    .slug
                    .as_ref()
                    .map(|slug| (slug.clone(), mapping[&record.uid].clone()))
            })
            .collect();
        Ok(Prepared {
            key: lince_resources::fingerprint(binding_key.as_bytes()),
            bound_state: !source.instances.is_empty()
                || !source.parameters.is_empty()
                || source
                    .documents
                    .iter()
                    .any(|document| !document.project.rules.is_empty()),
            organ,
            vocabulary,
            binding,
            bindings,
            records: mapping,
            concepts,
            definitions,
            personal,
            bound,
            slugs,
        })
    }

    pub(super) async fn preview_source(
        &self,
        source: &Publication,
        bindings: BTreeMap<String, String>,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let actors: Vec<Value> = if let Some(actor) = actor {
            vec![
                json!({"uid":actor,"title":store::records::get(&self.store.pool,actor).await?.map(|record|record.head)}),
            ]
        } else {
            store::auth::list_actors(&self.store.pool)
                .await?
                .into_iter()
                .map(|(uid, _, title, _)| json!({"uid":uid,"title":title}))
                .collect()
        };
        if source.parameters.iter().any(|(name, input)| {
            !bindings.contains_key(name) && (input.default != "actor" || actors.len() != 1)
        }) {
            return Ok(
                json!({"entry":source.entry,"revision":source.revision,"ready":false,"parameters":source.parameters,
                "bindings":bindings,"actors":actors,"options":source.options,"records":[],"rules":[],"conflicts":[]}),
            );
        }
        let prepared = self.prepare_source(source, bindings, actor).await?;
        let mut records = Vec::new();
        let mut conflicts = Vec::new();
        for document in &source.documents {
            for record in &document.project.records {
                if prepared.bound.contains(&record.uid) {
                    continue;
                }
                let uid = &prepared.records[&record.uid];
                let existing = store::records::get(&self.store.pool, uid).await?;
                let metadata =
                    store::records::get_extension(&self.store.pool, uid, IMPORT_NAMESPACE).await?;
                if existing.is_some() {
                    self.refuse_unreadable(actor, std::slice::from_ref(uid))
                        .await?;
                    if metadata
                        .as_ref()
                        .and_then(|value| value["original"].as_str())
                        != Some(record.uid.as_str())
                    {
                        conflicts
                            .push(format!("{} already has an unrelated identity", record.head));
                    }
                }
                let resource = document
                    .project
                    .extensions
                    .iter()
                    .find(|value| {
                        value.target_uid.as_deref() == Some(&record.uid)
                            && value.namespace == RESOURCE_NAMESPACE
                    })
                    .map(|value| value.fields.clone());
                let previous_resource =
                    store::records::get_extension(&self.store.pool, uid, RESOURCE_NAMESPACE)
                        .await?;
                records.push(json!({
                    "uid":uid,"source_uid":record.uid,"title":record.head,"body":prepared.body(&record.body),"quantity":record.quantity,
                    "slug":record.slug,"personal":prepared.personal.contains(&record.uid),
                    "state":if existing.is_none() {"create"} else if metadata.as_ref().is_some_and(|value| value["hash"] == document.hash && value["base"] == document.base) {"reuse"} else {"update_available"},
                    "resource":resource,"reference_review":existing.is_some()&&previous_resource!=resource,"assertions":record.assertions.iter().map(|assertion|json!({
                        "predicate":assertion.predicate,"object":assertion.object_uid.as_ref().and_then(|uid|prepared.records.get(uid)),"uri":assertion.object_uri,
                        "quantity":assertion.quantity,"identity":assertion.identity,
                    })).collect::<Vec<_>>(),
                }));
            }
        }
        Ok(json!({
            "ready":true,
            "binding_ready":store::records::get_extension(&self.store.pool, &prepared.binding, BINDING_NAMESPACE).await?.is_some_and(|binding| binding["revision"] == source.revision),
            "entry":source.entry,"revision":source.revision,"binding":prepared.bound_state.then_some(&prepared.binding),"root":selected_record(source).and_then(|record| prepared.records.get(&record.uid)),
            "records":records,"conflicts":conflicts,"parameters":source.parameters,"bindings":prepared.bindings,"actors":actors,
            "options":source.options,"rules":source.documents.iter().flat_map(|document| &document.project.rules).map(|rule|json!({"title":rule.slug,"condition":rule.condition,"gate":rule.gate.as_deref().unwrap_or("always"),"carry":rule.carry.as_deref().unwrap_or("value"),"cadence":rule.cadence,"consequences":rule.consequences_json})).collect::<Vec<_>>(),
            "vocabulary":prepared.definitions.iter().map(|(name,definition)|json!({"name":name,"definition":definition,"uid":prepared.concepts[name]})).collect::<Vec<_>>(),
        }))
    }

    pub(super) async fn import_source(
        &self,
        source: &Publication,
        bindings: BTreeMap<String, String>,
        update: bool,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let preview = self.preview_source(source, bindings.clone(), actor).await?;
        if preview["ready"] != true {
            return Err(invalid("Choose the source inputs before importing"));
        }
        if !preview["conflicts"].as_array().is_some_and(Vec::is_empty) {
            return Err(invalid(
                "Resolve the source identity conflicts before importing",
            ));
        }
        if !update
            && preview["records"].as_array().is_some_and(|records| {
                records.iter().any(|record| {
                    record["state"] == "update_available" && record["personal"] != true
                })
            })
        {
            return Err(invalid(
                "Review and accept source updates before using this changed definition",
            ));
        }
        let prepared = self.prepare_source(source, bindings, actor).await?;
        if update {
            self.require_permission(actor, "record:update").await?;
        }
        if !prepared.concepts.is_empty() {
            self.require_permission(actor, "permission:assign").await?;
        }
        let library = library()?;
        let snapshots: Vec<_> = source
            .documents
            .iter()
            .map(|document| {
                (
                    document.raw.clone(),
                    document.uri.clone(),
                    document.base.clone(),
                )
            })
            .collect();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            for (raw, uri, base) in snapshots {
                library.retain_bytes(raw.as_bytes(), &uri, &base)?;
            }
            Ok(())
        })
        .await
        .map_err(invalid)?
        .map_err(invalid)?;
        let _serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:create")
            .await?;
        if update {
            self.require_permission_on(&mut tx, actor, "record:update")
                .await?;
        }
        if !prepared.concepts.is_empty() {
            self.require_permission_on(&mut tx, actor, "permission:assign")
                .await?;
        }
        let now = nucleus::execution::now();
        let previous_binding: Option<String> = store::sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
        )
        .bind(&prepared.binding)
        .bind(BINDING_NAMESPACE)
        .fetch_optional(&mut *tx)
        .await?;
        let previous_binding: Value = previous_binding
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or(Value::Null);
        store::sqlx::query("INSERT INTO lingua(uid,name,owner_organ,visibility,created_at) VALUES(?,?,?,'private',?) ON CONFLICT(uid) DO NOTHING")
            .bind(&prepared.vocabulary).bind(&label(source)).bind(&prepared.organ).bind(now.to_rfc3339()).execute(&mut *tx).await?;
        for (name, uid) in &prepared.concepts {
            let created = store::sqlx::query("INSERT INTO concept(uid,canonical_name,created_at) VALUES(?,?,?) ON CONFLICT(uid) DO NOTHING")
                .bind(uid).bind(name).bind(now.to_rfc3339()).execute(&mut *tx).await?.rows_affected() > 0;
            store::sqlx::query("INSERT INTO lingua_concept(lingua_uid,concept_uid,adopted_at) VALUES(?,?,?) ON CONFLICT DO NOTHING")
                .bind(&prepared.vocabulary).bind(uid).bind(now.to_rfc3339()).execute(&mut *tx).await?;
            if created {
                store::sqlx::query("INSERT INTO concept_source(concept_uid,lingua_uid,original_name,original_language,definition) VALUES(?,?,?,'en',?)")
                    .bind(uid).bind(&prepared.vocabulary).bind(name).bind(&prepared.definitions[name]).execute(&mut *tx).await?;
                store::vocabulary::add_term_on(&mut tx, uid, name, "en", None, true).await?;
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "concept",
                    uid,
                    "canonical_name",
                    store::sync_ops::OpKind::Set,
                    Some(json!(name).to_string()),
                )
                .await?;
                store::vocabulary::log_shared_on(&mut tx, uid).await?;
            }
        }
        let mut created = 0;
        let mut changed = BTreeSet::new();
        for document in &source.documents {
            for record in &document.project.records {
                if prepared.bound.contains(&record.uid) {
                    continue;
                }
                let uid = &prepared.records[&record.uid];
                let existing: Option<(String, String)> = store::sqlx::query_as(
                    "SELECT head,body FROM record WHERE uid=? AND deleted_at IS NULL",
                )
                .bind(uid)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some((head, body)) = existing {
                    if update && !prepared.personal.contains(&record.uid) {
                        let raw: Option<String> = store::sqlx::query_scalar(
                            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
                        )
                        .bind(uid)
                        .bind(IMPORT_NAMESPACE)
                        .fetch_optional(&mut *tx)
                        .await?;
                        let baseline: Value = serde_json::from_str(
                            raw.as_deref()
                                .ok_or_else(|| invalid("The imported baseline is unavailable"))?,
                        )?;
                        if baseline["hash"] == document.hash && baseline["base"] == document.base {
                            continue;
                        }
                        let resource: Option<String> = store::sqlx::query_scalar(
                            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
                        )
                        .bind(uid)
                        .bind(RESOURCE_NAMESPACE)
                        .fetch_optional(&mut *tx)
                        .await?;
                        store::records::set_extension_on(&mut tx,uid,&format!("lince.source-history.{}",baseline["revision"].as_str().unwrap_or("initial").chars().take(32).collect::<String>()),
                            &json!({"baseline":baseline,"resource":resource.as_deref().map(serde_json::from_str::<Value>).transpose()?,"head":head,"body_hash":lince_resources::fingerprint(body.as_bytes())})).await?;
                        store::records::set_authoring_text_on(
                            &mut tx,
                            uid,
                            (baseline["head"] == head).then_some(record.head.as_str()),
                            (baseline["body_hash"]
                                == lince_resources::fingerprint(body.as_bytes()))
                            .then_some(prepared.body(&record.body).as_str()),
                        )
                        .await?;
                        if let Some(assertions) = baseline["assertions"].as_array() {
                            for assertion in assertions.iter().filter_map(Value::as_str) {
                                store::assertions::retract_tx(&mut tx, assertion, actor).await?;
                            }
                        }
                        changed.insert(record.uid.clone());
                    }
                } else {
                    let slug = record.slug.as_ref().map(|slug| {
                        format!(
                            "{}-{}",
                            &slug[..slug.len().min(110)],
                            uid[uid.len() - 10..].to_ascii_lowercase()
                        )
                    });
                    store::records::create_with_uid_on(
                        &mut tx,
                        uid,
                        store::records::NewRecord {
                            slug: slug.as_deref(),
                            kind: RecordKind::Plain,
                            head: &record.head,
                            body: &prepared.body(&record.body),
                            quantity: store::exact::zero(),
                        },
                        &prepared.organ,
                        None,
                    )
                    .await?;
                    changed.insert(record.uid.clone());
                    created += 1;
                    if prepared.personal.contains(&record.uid) {
                        let person = actor
                            .map(str::to_owned)
                            .or_else(|| prepared.bindings.values().next().cloned());
                        if let Some(person) = person {
                            store::sqlx::query("INSERT INTO visibility_rule(uid,subject_kind,subject_uid,target_uid,grant_level) VALUES(?,'actor',?,?,'visible')")
                                .bind(nucleus::new_uid("v")).bind(person).bind(uid).execute(&mut *tx).await?;
                        }
                    }
                }
            }
        }
        let signer = self.signer.lock().await.clone();
        let mut facts = Vec::new();
        for document in &source.documents {
            for record in &document.project.records {
                if !changed.contains(&record.uid) {
                    continue;
                }
                let uid = &prepared.records[&record.uid];
                let mut assertions = Vec::new();
                for assertion in &record.assertions {
                    let id = nucleus::new_uid("a");
                    let new = store::assertions::NewAssertion {
                        subject_uid: uid,
                        predicate_uid: &prepared.concepts[&assertion.predicate],
                        object_uid: assertion
                            .object_uid
                            .as_ref()
                            .and_then(|uid| prepared.records.get(uid))
                            .map(String::as_str),
                        role: if assertion.identity {
                            store::assertions::AssertionRole::Identity
                        } else {
                            store::assertions::AssertionRole::Ordinary
                        },
                        quantity: assertion.quantity.as_deref().map(decimal).transpose()?,
                        unit_uid: assertion
                            .unit
                            .as_ref()
                            .map(|name| prepared.concepts[name].as_str()),
                        asserted_by: actor,
                    };
                    if let Some(uri) = &assertion.object_uri {
                        store::assertions::insert_uri_tx(&mut tx, &id, new, uri).await?;
                    } else {
                        store::assertions::insert_tx(&mut tx, &id, new).await?;
                    }
                    assertions.push(id);
                }
                let desired = decimal(record.quantity.as_ref().map_or("0", |(amount, _)| amount))?;
                let row = store::sqlx::query(
                    "SELECT quantity_mantissa,quantity_scale FROM record WHERE uid=?",
                )
                .bind(uid)
                .fetch_one(&mut *tx)
                .await?;
                let current = store::exact::read_decimal(&row, "quantity")?;
                let amount = store::exact::difference(desired, current)?;
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
                store::records::set_extension_on(&mut tx, uid, IMPORT_NAMESPACE, &json!({
                    "entry":source.entry,"revision":source.revision,"original":record.uid,"slug":record.slug,
                    "document":document.uri,"base":document.base,"hash":document.hash,
                    "origin":origin(document),
                    "head":record.head,"body_hash":lince_resources::fingerprint(prepared.body(&record.body).as_bytes()),"assertions":assertions,
                    "instance":prepared.personal.contains(&record.uid),
                })).await?;
            }
            for extension in &document.project.extensions {
                let original = extension
                    .target_uid
                    .as_ref()
                    .ok_or_else(|| invalid("An extension has no Record identity"))?;
                if prepared.bound.contains(original) || !changed.contains(original) {
                    continue;
                }
                if matches!(
                    extension.namespace.as_str(),
                    SOURCE_NAMESPACE | INSTANCE_NAMESPACE | "lince.vocabulary"
                ) {
                    continue;
                }
                if !matches!(
                    extension.namespace.as_str(),
                    RESOURCE_NAMESPACE
                        | "lince.controls"
                        | "nutrition"
                        | "demeter.reference"
                        | "demeter.production"
                ) {
                    return Err(invalid(format!(
                        "Source metadata namespace {} is not a supported source capability",
                        extension.namespace
                    )));
                }
                let uid = &prepared.records[original];
                let mut fields = extension.fields.clone();
                match extension.namespace.as_str() {
                    "nutrition" => {
                        if !fields["basis_g"]
                            .as_f64()
                            .is_some_and(|v| v.is_finite() && v > 0.0)
                            || !fields["nutrients"]
                                .as_object()
                                .is_some_and(|v| v.len() <= 256)
                        {
                            return Err(invalid(
                                "Nutrition data needs a positive gram basis and a bounded nutrient table",
                            ));
                        }
                    }
                    "demeter.reference" => {
                        let reference: nucleus::demeter::Reference =
                            serde_json::from_value(fields.clone()).map_err(invalid)?;
                        nucleus::demeter::Unit::for_field(&reference.nutrient)
                            .map_err(invalid)?
                            .factor(reference.unit)
                            .map_err(invalid)?;
                        if reference.cases.is_empty() || reference.cases.len() > 128 {
                            return Err(invalid("Nutrient reference cases exceed their limits"));
                        }
                        for case in reference.cases {
                            nucleus::demeter::Bounds {
                                lower: case
                                    .lower
                                    .as_deref()
                                    .map(nucleus::demeter::exact)
                                    .transpose()
                                    .map_err(invalid)?,
                                upper: case
                                    .upper
                                    .as_deref()
                                    .map(nucleus::demeter::exact)
                                    .transpose()
                                    .map_err(invalid)?,
                            }
                            .validate()
                            .map_err(invalid)?;
                        }
                    }
                    "demeter.production" => {
                        let template: nucleus::demeter::production::Template =
                            serde_json::from_value(fields.clone()).map_err(invalid)?;
                        if template.inputs.len() > 100
                            || template.stages.len() > 100
                            || template.harvest_count == 0
                            || template.harvest_count > 3660
                            || nucleus::demeter::exact(&template.usable_g_per_harvest)
                                .map_err(invalid)?
                                .to_f64()
                                <= 0.0
                        {
                            return Err(invalid(
                                "Production template parameters exceed their limits",
                            ));
                        }
                    }
                    _ => {}
                }
                if extension.namespace == "lince.controls" {
                    let mut identities: HashMap<_, _> = prepared
                        .slugs
                        .iter()
                        .map(|(name, uid)| (name.clone(), uid.clone()))
                        .collect();
                    identities.extend(
                        prepared
                            .records
                            .iter()
                            .map(|(name, uid)| (name.clone(), uid.clone())),
                    );
                    remap(&mut fields, &identities, &prepared.concepts);
                }
                store::records::set_extension_on(&mut tx, uid, &extension.namespace, &fields)
                    .await?;
            }
        }
        for document in &source.documents {
            for record in &document.project.records {
                if prepared.bound.contains(&record.uid) || prepared.personal.contains(&record.uid) {
                    continue;
                }
                let local = &prepared.records[&record.uid];
                for uri in [&document.uri, &document.base] {
                    store::source_locations::remember_on(
                        &mut tx,
                        local,
                        &record.uid,
                        uri,
                        &document.hash,
                    )
                    .await?;
                }
            }
        }
        let existing = store::sqlx::query_scalar::<_, String>(
            "SELECT uid FROM record WHERE uid=? AND deleted_at IS NULL",
        )
        .bind(&prepared.binding)
        .fetch_optional(&mut *tx)
        .await?;
        if prepared.bound_state && existing.is_none() {
            store::records::create_with_uid_on(
                &mut tx,
                &prepared.binding,
                store::records::NewRecord {
                    slug: None,
                    kind: RecordKind::Plain,
                    head: &format!("{} · personal binding", label(source)),
                    body: "",
                    quantity: store::exact::zero(),
                },
                &prepared.organ,
                None,
            )
            .await?;
            if let Some(person) = actor
                .map(str::to_owned)
                .or_else(|| prepared.bindings.values().next().cloned())
            {
                store::sqlx::query("INSERT INTO visibility_rule(uid,subject_kind,subject_uid,target_uid,grant_level) VALUES(?,'actor',?,?,'visible')")
                    .bind(nucleus::new_uid("v")).bind(person).bind(&prepared.binding).execute(&mut *tx).await?;
            }
        }
        let lookup: HashMap<_, _> = source
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                record
                    .slug
                    .as_ref()
                    .map(|slug| (slug.clone(), prepared.records[&record.uid].clone()))
            })
            .collect();
        let mut identities = lookup.clone();
        identities.extend(
            prepared
                .records
                .iter()
                .map(|(name, uid)| (name.clone(), uid.clone())),
        );
        let mut rules = Vec::new();
        for rule in source
            .documents
            .iter()
            .flat_map(|document| &document.project.rules)
        {
            let mut consequences: Value = serde_json::from_str(&rule.consequences_json)?;
            remap(&mut consequences, &identities, &prepared.concepts);
            let condition = rule
                .condition
                .as_ref()
                .map(|condition| rewrite_condition(condition, &identities, &prepared.concepts));
            let target = lookup
                .get(&rule.record_slug)
                .ok_or_else(|| invalid("A rule target is missing"))?;
            rules.push(json!({
                "id":rule.uid,"target":target,"consequences":consequences,"condition":condition,
                "gate":rule.gate.clone().unwrap_or_else(||"always".into()),"carry":rule.carry.clone().unwrap_or_else(||"value".into()),
                "note":rule.note,"cadence":rule.cadence,
            }));
        }
        if prepared.bound_state {
            store::records::set_extension_on(&mut tx, &prepared.binding, BINDING_NAMESPACE, &json!({
            "key":prepared.key,
            "entry":source.entry,"revision":source.revision,"root":selected_record(source).and_then(|record| prepared.records.get(&record.uid)),
            "documents":source.documents.iter().map(|document|json!({"uri":document.uri,"base":document.base,"hash":document.hash,"origin":origin(document)})).collect::<Vec<_>>(),
            "records":prepared.records,"parameters":prepared.bindings,"concepts":prepared.concepts,"rules":rules,
            "active_rules":previous_binding["active_rules"],"active_revision":previous_binding["active_revision"],
        })).await?;
        }
        tx.commit().await?;
        for fact in facts {
            self.publish_committed_fact(fact);
        }
        self.query_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(
            json!({"created":created,"binding":prepared.bound_state.then_some(&prepared.binding),"root":selected_record(source).and_then(|record| prepared.records.get(&record.uid)),"preview":preview}),
        )
    }

    pub(super) async fn activate_source(
        &self,
        record: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let uid = self.resolve(record).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&uid))
            .await?;
        let mut binding = store::records::get_extension(&self.store.pool, &uid, BINDING_NAMESPACE)
            .await?
            .ok_or_else(|| invalid("This Record has no reviewed source binding"))?;
        let mut actions = Vec::new();
        for rule in binding["rules"]
            .as_array()
            .ok_or_else(|| invalid("The source has no rule list"))?
        {
            let consequences: Vec<nucleus::karma::Consequence> =
                serde_json::from_value(rule["consequences"].clone())?;
            let target = rule["target"]
                .as_str()
                .ok_or_else(|| invalid("A rule has no target"))?;
            self.authorize_rule_target(target, actor).await?;
            let condition = rule["condition"].as_str().map(str::to_string);
            let gate = rule["gate"].as_str().map(str::to_string);
            let carry = rule["carry"].as_str().map(str::to_string);
            let declared = self.resolve_consequences(consequences.clone()).await?;
            let declared_condition = condition
                .as_ref()
                .map(
                    |source| -> Result<store::recurrence::RuleCondition, EngineError> {
                        Ok(store::recurrence::RuleCondition {
                            source: source.clone(),
                            bindings: Vec::new(),
                            gate: nucleus::karma::Gate::parse(gate.as_deref().unwrap_or("!=0"))
                                .map_err(invalid)?,
                            carry: nucleus::karma::Carry::parse(
                                carry.as_deref().unwrap_or("value"),
                            )
                            .map_err(invalid)?,
                        })
                    },
                )
                .transpose()?;
            self.validate_automatic_rule(&declared, declared_condition.as_ref(), actor)
                .await?;
            let cadence = rule["cadence"]
                .as_str()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_else(nucleus::karma::Cadence::once);
            actions.push(Action::CreateRecurrence {
                target: target.into(),
                consequences,
                condition,
                gate,
                carry,
                cadence,
                note: rule["note"].as_str().map(str::to_owned),
                anchor_at: None,
                request_id: Some(format!(
                    "source-rule:{}:{}:{}",
                    binding["key"].as_str().unwrap_or(""),
                    rule["id"].as_str().unwrap_or(""),
                    lince_resources::fingerprint(rule.to_string().as_bytes())
                )),
            });
        }
        let mut created = Vec::new();
        for action in actions {
            let outcome = Box::pin(self.act(action, actor.map(str::to_owned))).await?;
            created.extend(outcome.created);
        }
        for rule in binding["active_rules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if created.iter().any(|current| current == rule) {
                continue;
            }
            if let Some(current) = store::recurrence::get(&self.store.pool, rule).await? {
                Box::pin(self.act(
                    Action::SetRecurrencePaused {
                        recurrence: rule.into(),
                        expected_revision: current.revision,
                        request_id: format!("source-replace:{uid}:{rule}:{}", binding["revision"]),
                        paused: true,
                    },
                    actor.map(str::to_owned),
                ))
                .await?;
            }
        }
        for rule in &created {
            if let Some(current) = store::recurrence::get(&self.store.pool, rule).await?
                && current.is_paused()
            {
                Box::pin(self.act(
                    Action::SetRecurrencePaused {
                        recurrence: rule.clone(),
                        expected_revision: current.revision,
                        request_id: format!("source-enable:{uid}:{rule}:{}", binding["revision"]),
                        paused: false,
                    },
                    actor.map(str::to_owned),
                ))
                .await?;
            }
        }
        binding["active_revision"] = binding["revision"].clone();
        binding["active_rules"] = json!(created);
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "frequency:create")
            .await?;
        store::records::set_extension_on(&mut tx, &uid, BINDING_NAMESPACE, &binding).await?;
        tx.commit().await?;
        Ok(json!({"rules":created,"binding":uid}))
    }
}

fn rewrite_condition(
    source: &str,
    identities: &HashMap<String, String>,
    concepts: &BTreeMap<String, String>,
) -> String {
    let mut output = String::new();
    let mut chars = source.chars().peekable();
    let mut quoted = false;
    let mut escaped = false;
    while let Some(character) = chars.next() {
        if character == '"' && !escaped {
            quoted = !quoted;
        }
        if !quoted && matches!(character, '@' | '#') {
            let mut name = String::new();
            while chars.peek().is_some_and(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
            }) {
                name.push(chars.next().unwrap());
            }
            output.push(character);
            output.push_str(
                if character == '#' {
                    concepts.get(&name)
                } else {
                    identities.get(&name)
                }
                .map_or(name.as_str(), String::as_str),
            );
        } else {
            output.push(character);
        }
        escaped = character == '\\' && !escaped;
    }
    output
}

fn remap(
    value: &mut Value,
    identities: &HashMap<String, String>,
    concepts: &BTreeMap<String, String>,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                remap(value, identities, concepts);
            }
        }
        Value::Object(values) => {
            for (name, value) in values {
                if matches!(
                    name.as_str(),
                    "subject"
                        | "target"
                        | "object"
                        | "predicate"
                        | "unit"
                        | "concept"
                        | "record"
                        | "assertion"
                ) {
                    let concept = matches!(
                        name.as_str(),
                        "predicate" | "unit" | "concept" | "assertion"
                    );
                    if let Some(name) = value.as_str().and_then(|name| {
                        if concept {
                            concepts.get(name)
                        } else {
                            identities.get(name)
                        }
                    }) {
                        *value = json!(name);
                    }
                } else if name == "action" && value.is_string() {
                    if let Ok(mut action) = serde_json::from_str::<Value>(value.as_str().unwrap()) {
                        remap(&mut action, identities, concepts);
                        *value = json!(action.to_string());
                    }
                } else {
                    remap(value, identities, concepts);
                }
            }
        }
        _ => {}
    }
}
