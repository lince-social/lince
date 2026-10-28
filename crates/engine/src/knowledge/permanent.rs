use super::{Engine, EngineError, invalid};
use nucleus::knowledge::{DEPENDENCY_LIMIT, Item, Selection};
use protein::authority::Operation;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet, VecDeque};
use store::sqlx::{Row, Sqlite, Transaction};

impl Engine {
    pub(super) async fn knowledge_destination_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        organ: &str,
    ) -> Result<(), EngineError> {
        self.require_permission_on(tx, actor, "record:create")
            .await?;
        self.require_permission_on(tx, actor, "organ:update")
            .await?;
        store::vocabulary::require_local_organ_on(tx, organ).await?;
        let graph = crate::access::policy_graph_on(tx, &[organ.to_owned()])
            .await
            .map_err(|error| invalid(&error.to_string()))?;
        if !self.readable_on(tx, actor, &graph).await?.contains(organ) {
            return Err(EngineError::Forbidden(
                "Destination Organ unavailable".into(),
            ));
        }
        Ok(())
    }

    pub(super) async fn knowledge_reference(
        &self,
        source: &str,
        item: &str,
        organ: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let owner = store::vocabulary::terminology_owner(actor);
        self.knowledge_provider(owner, source).await?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.knowledge_destination_on(&mut tx, actor, organ).await?;
        let item = store::knowledge::cached_on(&mut tx, owner, source, item)
            .await?
            .ok_or_else(|| invalid("Read the source item before keeping its reference"))?;
        if let Some(uid) =
            store::knowledge::reference_uid_on(&mut tx, organ, source, &item.id).await?
        {
            let graph = crate::access::policy_graph_on(&mut tx, &[uid.clone()])
                .await
                .map_err(|error| invalid(&error.to_string()))?;
            if !self
                .readable_on(&mut tx, actor, &graph)
                .await?
                .contains(&uid)
            {
                return Err(EngineError::Forbidden("Saved reference unavailable".into()));
            }
            return Ok(json!({"record_uid":uid,"reference_only":true}));
        }
        let uid = nucleus::new_uid("r");
        let checkpoint = self
            .record_checkpoint_on(&mut tx, actor, vec![uid.clone()], Operation::Create)
            .await?;
        store::knowledge::create_reference_on(&mut tx, &uid, organ, &item).await?;
        if let Some(checkpoint) = checkpoint {
            checkpoint.finish(&mut tx, Default::default()).await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        Ok(json!({"record_uid":uid,"reference_only":true}))
    }

    pub(super) async fn inspect_saved_knowledge(
        &self,
        record: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.refuse_unreadable(actor, &[record.to_owned()]).await?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:read")
            .await?;
        let graph = crate::access::policy_graph_on(&mut tx, &[record.to_owned()])
            .await
            .map_err(|error| invalid(&error.to_string()))?;
        if !self
            .readable_on(&mut tx, actor, &graph)
            .await?
            .contains(record)
        {
            return Err(EngineError::Forbidden("Saved knowledge unavailable".into()));
        }
        let metadata = store::knowledge::record_metadata_on(&mut tx, record).await?;
        let payload = store::knowledge::payload_on(&mut tx, record).await?;
        let reference: Option<String> = store::sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
        )
        .bind(record)
        .bind(store::knowledge::REFERENCE_NAMESPACE)
        .fetch_optional(&mut *tx)
        .await?;
        let reference = reference.and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
        Ok(
            json!({"record":metadata,"saved":payload.is_some(),"item":payload.as_ref().map(|payload|&payload["item"]),"mappings":payload.as_ref().map(|payload|&payload["mappings"]),"definitions":payload.as_ref().map(|payload|&payload["definitions"]),"dependencies":payload.as_ref().map(|payload|&payload["dependencies"]),"reference":reference,"offline":true}),
        )
    }

    pub(super) async fn preview_knowledge_save(
        &self,
        source: &str,
        item: &str,
        organ: &str,
        selection: &Selection,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        if selection.optional_assets.len() > 128 || selection.optional_dependencies.len() > 128 {
            return Err(invalid(
                "Select at most 128 optional assets or related items",
            ));
        }
        let owner = store::vocabulary::terminology_owner(actor);
        self.knowledge_provider(owner, source).await?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.knowledge_destination_on(&mut tx, actor, organ).await?;
        let mut pending = VecDeque::from([(source.to_owned(), item.to_owned())]);
        let mut visited = HashSet::new();
        let mut entries = Vec::new();
        let mut missing = Vec::new();
        let mut total_bytes = 0;
        while let Some((source, id)) = pending.pop_front() {
            if !visited.insert((source.clone(), id.clone())) {
                continue;
            }
            if visited.len() > DEPENDENCY_LIMIT {
                return Err(invalid("Save at most 128 related knowledge items together"));
            }
            let Some(mut item) = store::knowledge::cached_on(&mut tx, owner, &source, &id).await?
            else {
                missing.push(json!({"source":source,"id":id,"kind":"content"}));
                continue;
            };
            for dependency in &item.dependencies {
                if dependency.required || selection.optional_dependencies.contains(&dependency.id) {
                    pending.push_back((dependency.source.clone(), dependency.id.clone()));
                }
            }
            item.assets
                .retain(|asset| asset.required || selection.optional_assets.contains(&asset.id));
            for asset in &item.assets {
                if asset.bytes.is_none() {
                    missing.push(json!({"source":source,"id":asset.id,"kind":"asset"}));
                }
            }
            let mappings = store::knowledge::mappings_on(&mut tx, owner, &source).await?;
            let relevant: Vec<_> = mappings
                .into_iter()
                .filter(|mapping| {
                    item.meanings
                        .iter()
                        .any(|meaning| meaning.id == mapping.external)
                })
                .collect();
            let mut definitions = BTreeMap::new();
            for mapping in &relevant {
                if !nucleus::knowledge::mapping_matches(mapping, &item) {
                    return Err(invalid(
                        "An external meaning changed; read and explicitly map its new version before saving",
                    ));
                }
                let dictionary =
                    store::vocabulary::dictionary_on(&mut tx, &mapping.concept_uid).await?;
                if !dictionary
                    .definitions
                    .iter()
                    .any(|version| version.revision == mapping.definition_revision)
                {
                    return Err(invalid("A selected local meaning version is unavailable"));
                }
                definitions.insert(mapping.concept_uid.clone(), dictionary);
            }
            let existing = store::knowledge::reference_uid_on(&mut tx, organ, &source, &id).await?;
            if let Some(uid) = &existing {
                let graph = crate::access::policy_graph_on(&mut tx, &[uid.clone()])
                    .await
                    .map_err(|error| invalid(&error.to_string()))?;
                if !self
                    .readable_on(&mut tx, actor, &graph)
                    .await?
                    .contains(uid)
                {
                    return Err(EngineError::Forbidden(
                        "A required saved Record is unavailable".into(),
                    ));
                }
            }
            let record_uid = existing.unwrap_or_else(|| nucleus::new_uid("r"));
            let record = store::sqlx::query(
                "SELECT head,body FROM record WHERE uid=? AND deleted_at IS NULL",
            )
            .bind(&record_uid)
            .fetch_optional(&mut *tx)
            .await?;
            let baseline=record.map(|row|json!({"head":row.get::<String,_>("head"),"body":row.get::<String,_>("body")}));
            let saved = store::knowledge::payload_on(&mut tx, &record_uid).await?;
            let entry = json!({"record_uid":record_uid,"item":item,"mappings":relevant,"definitions":definitions.into_values().collect::<Vec<_>>(),"baseline":baseline,"previous_payload":saved.as_ref().map(|saved|store::knowledge::fingerprint(saved.to_string().as_bytes()))});
            total_bytes += serde_json::to_vec(&entry)?.len();
            entries.push(entry);
        }
        let review = nucleus::new_uid("kp");
        let content = json!({"organ_uid":organ,"source":source,"item_id":item,"entries":entries,"missing":missing,"estimated_bytes":total_bytes,"can_save":missing.is_empty()});
        store::knowledge::check_review_capacity_on(&mut tx, owner, content.to_string().len())
            .await?;
        store::sqlx::query("INSERT INTO knowledge_review(uid,owner_key,kind,content,created_at) VALUES(?,?,'save',?,?)").bind(&review).bind(owner).bind(content.to_string()).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"preview_uid":review,"preview":content}))
    }

    pub(super) async fn save_knowledge_review(
        &self,
        review: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let owner = store::vocabulary::terminology_owner(actor);
        let mut tx = store::write_tx(&self.store.pool).await?;
        let row=store::sqlx::query("SELECT content,applied FROM knowledge_review WHERE uid=? AND owner_key=? AND kind='save'").bind(review).bind(owner).fetch_optional(&mut *tx).await?.ok_or_else(||invalid("Knowledge save preview unavailable"))?;
        let preview: Value = serde_json::from_str(&row.get::<String, _>("content"))?;
        let organ = preview["organ_uid"]
            .as_str()
            .ok_or_else(|| invalid("Knowledge preview has no destination"))?;
        self.knowledge_destination_on(&mut tx, actor, organ).await?;
        if let Some(applied) = row.get::<Option<String>, _>("applied") {
            let result: Value = serde_json::from_str(&applied)?;
            let ids: Vec<_> = result["saved_records"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let graph = crate::access::policy_graph_on(&mut tx, &ids)
                .await
                .map_err(|error| invalid(&error.to_string()))?;
            let readable = self.readable_on(&mut tx, actor, &graph).await?;
            if ids.iter().any(|uid| !readable.contains(uid)) {
                return Err(EngineError::Forbidden(
                    "Saved knowledge is no longer available to this Person".into(),
                ));
            }
            return Ok(result);
        }
        if preview["can_save"] != true {
            return Err(invalid(
                "Read the missing required content and assets before saving",
            ));
        }
        let entries = preview["entries"]
            .as_array()
            .ok_or_else(|| invalid("Knowledge preview has no selected content"))?;
        let ids = entries
            .iter()
            .map(|entry| {
                entry["record_uid"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("Knowledge preview has an invalid Record"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut checkpoints = Vec::new();
        for entry in entries {
            let uid = entry["record_uid"].as_str().unwrap();
            let item: Item = serde_json::from_value(entry["item"].clone())?;
            store::knowledge::validate_item(&item)?;
            let current = store::sqlx::query(
                "SELECT head,body FROM record WHERE uid=? AND deleted_at IS NULL",
            )
            .bind(uid)
            .fetch_optional(&mut *tx)
            .await?;
            let baseline=current.as_ref().map(|row|json!({"head":row.get::<String,_>("head"),"body":row.get::<String,_>("body")}));
            if baseline
                != entry["baseline"]
                    .as_object()
                    .map(|_| entry["baseline"].clone())
            {
                return Err(invalid(
                    "A destination Record changed; review the save again",
                ));
            }
            let existing =
                store::knowledge::reference_uid_on(&mut tx, organ, &item.source, &item.id).await?;
            if existing.as_deref().is_some_and(|existing| existing != uid) {
                return Err(invalid(
                    "Another save created this reference; preview again to reuse it",
                ));
            }
            let operation = if current.is_some() {
                self.require_permission_on(&mut tx, actor, "record:update")
                    .await?;
                Operation::Update
            } else {
                Operation::Create
            };
            let checkpoint = self
                .record_checkpoint_on(&mut tx, actor, vec![uid.into()], operation)
                .await?;
            let saved = store::knowledge::payload_on(&mut tx, uid).await?;
            let hash = saved
                .as_ref()
                .map(|saved| store::knowledge::fingerprint(saved.to_string().as_bytes()));
            if hash.as_deref() != entry["previous_payload"].as_str() {
                return Err(invalid(
                    "Saved knowledge changed; review source updates separately",
                ));
            }
            if saved.is_some() {
                if saved.as_ref().unwrap()["item"] != entry["item"] {
                    return Err(invalid(
                        "Review this source update before replacing saved content",
                    ));
                }
            } else {
                store::knowledge::create_reference_on(&mut tx, uid, organ, &item).await?;
                let dependencies: Vec<_> = entries
                    .iter()
                    .filter(|other| {
                        item.dependencies.iter().any(|dependency| {
                            dependency.source == other["item"]["source"]
                                && dependency.id == other["item"]["id"]
                        })
                    })
                    .map(|other| other["record_uid"].clone())
                    .collect();
                let payload = json!({"format":1,"item":item,"dependencies":dependencies,"mappings":entry["mappings"],"definitions":entry["definitions"],"source_baseline":entry["item"],"local_baseline":baseline.unwrap_or_else(||json!({"head":item.title,"body":""}))});
                store::knowledge::save_payload_on(&mut tx, uid, &item, &payload).await?;
            }
            if let Some(checkpoint) = checkpoint {
                checkpoints.push(checkpoint);
            }
        }
        for checkpoint in checkpoints {
            checkpoint.finish(&mut tx, Default::default()).await?;
        }
        let root = entries
            .iter()
            .find(|entry| {
                entry["item"]["source"] == preview["source"]
                    && entry["item"]["id"] == preview["item_id"]
            })
            .ok_or_else(|| invalid("Selected knowledge item is missing"))?;
        let result = json!({"record_uid":root["record_uid"],"saved_records":ids,"offline":true});
        store::sqlx::query("UPDATE knowledge_review SET applied=?,content=? WHERE uid=?")
            .bind(result.to_string())
            .bind(json!({"organ_uid":organ}).to_string())
            .bind(review)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(result)
    }
}
