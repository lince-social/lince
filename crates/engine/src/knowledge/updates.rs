use super::{Engine, EngineError, invalid};
use nucleus::knowledge::{Item, UpdateField};
use protein::authority::Operation;
use serde_json::{Value, json};
use store::sqlx::Row;

impl Engine {
    pub(super) async fn review_knowledge_update(
        &self,
        record: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.refuse_unreadable(actor, &[record.to_owned()]).await?;
        let owner = store::vocabulary::terminology_owner(actor);
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
        let saved = store::knowledge::payload_on(&mut tx, record)
            .await?
            .ok_or_else(|| invalid("Save this source content before reviewing updates"))?;
        if saved["detached"] == true {
            return Err(invalid(
                "This is a separate local copy; its source attribution is retained",
            ));
        }
        let item: Item = serde_json::from_value(saved["item"].clone())?;
        let incoming = store::knowledge::cached_on(&mut tx, owner, &item.source, &item.id)
            .await?
            .ok_or_else(|| {
                invalid("Read the latest source version before reviewing its changes")
            })?;
        let metadata = store::knowledge::record_metadata_on(&mut tx, record).await?;
        let baseline = &saved["source_baseline"];
        let summary = json!({"source_revision":incoming.revision,"saved_revision":item.revision,"source_changed":json!({"title":baseline["title"]!=incoming.title,"content":baseline["content"]!=incoming.content,"claims":baseline["claims"]!=json!(incoming.claims),"attribution":baseline["credits"]!=json!(incoming.credits)||baseline["license"]!=json!(incoming.license),"assets":baseline["assets"]!=json!(incoming.assets)}),"local_changed":json!({"title":metadata["head"]!=saved["local_baseline"]["head"],"annotations":metadata["body"]!=saved["local_baseline"]["body"],"content":saved["item"]["content"]!=baseline["content"],"claims":saved["item"]["claims"]!=baseline["claims"]}),"local_record":metadata,"saved_item":item,"source_item":incoming});
        let review = nucleus::new_uid("ku");
        let content = json!({"record_uid":record,"record":metadata,"saved_hash":store::knowledge::fingerprint(saved.to_string().as_bytes()),"incoming":incoming,"summary":summary});
        if content.to_string().len() > store::knowledge::BUNDLE_LIMIT {
            return Err(invalid("This source update exceeds the review size limit"));
        }
        store::knowledge::check_review_capacity_on(&mut tx, owner, content.to_string().len())
            .await?;
        store::sqlx::query("INSERT INTO knowledge_review(uid,owner_key,kind,content,created_at) VALUES(?,?,'update',?,?)").bind(&review).bind(owner).bind(content.to_string()).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"review_uid":review,"review":summary}))
    }

    pub(super) async fn apply_knowledge_update(
        &self,
        review: &str,
        fields: &[UpdateField],
        fork: bool,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        if fields.is_empty() || fields.len() > 5 {
            return Err(invalid("Choose the source changes to accept"));
        }
        let owner = store::vocabulary::terminology_owner(actor);
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:update")
            .await?;
        let row=store::sqlx::query("SELECT content,applied FROM knowledge_review WHERE uid=? AND owner_key=? AND kind='update'").bind(review).bind(owner).fetch_optional(&mut *tx).await?.ok_or_else(||invalid("Source update review unavailable"))?;
        let content: Value = serde_json::from_str(&row.get::<String, _>("content"))?;
        let record = content["record_uid"]
            .as_str()
            .ok_or_else(|| invalid("Source review has no saved Record"))?;
        let checkpoint = self
            .record_checkpoint_on(&mut tx, actor, vec![record.into()], Operation::Update)
            .await?;
        if let Some(applied) = row.get::<Option<String>, _>("applied") {
            let result: Value = serde_json::from_str(&applied)?;
            let uid = result["record_uid"]
                .as_str()
                .ok_or_else(|| invalid("Update receipt has no Record"))?;
            let graph = crate::access::policy_graph_on(&mut tx, &[uid.to_owned()])
                .await
                .map_err(|error| invalid(&error.to_string()))?;
            if !self
                .readable_on(&mut tx, actor, &graph)
                .await?
                .contains(uid)
            {
                return Err(EngineError::Forbidden("Saved knowledge unavailable".into()));
            }
            return Ok(result);
        }
        let metadata = store::knowledge::record_metadata_on(&mut tx, record).await?;
        let mut saved = store::knowledge::payload_on(&mut tx, record)
            .await?
            .ok_or_else(|| invalid("Saved knowledge unavailable"))?;
        if metadata != content["record"]
            || store::knowledge::fingerprint(saved.to_string().as_bytes())
                != content["saved_hash"].as_str().unwrap_or("")
        {
            return Err(invalid(
                "The saved knowledge or local annotations changed; review this update again",
            ));
        }
        let incoming: Item = serde_json::from_value(content["incoming"].clone())?;
        let mut item: Item = serde_json::from_value(saved["item"].clone())?;
        for field in fields {
            match field {
                UpdateField::Title => item.title = incoming.title.clone(),
                UpdateField::Content => {
                    item.content = incoming.content.clone();
                    item.content_type = incoming.content_type.clone();
                }
                UpdateField::Claims => {
                    let previous: Vec<nucleus::knowledge::Mapping> =
                        serde_json::from_value(saved["mappings"].clone())?;
                    let confirmed =
                        store::knowledge::mappings_on(&mut tx, owner, &incoming.source).await?;
                    let mut mappings = Vec::new();
                    let mut definitions = std::collections::BTreeMap::new();
                    for mapping in previous {
                        if !incoming
                            .meanings
                            .iter()
                            .any(|meaning| meaning.id == mapping.external)
                        {
                            continue;
                        }
                        let mapping = if nucleus::knowledge::mapping_matches(&mapping, &incoming) {
                            mapping
                        } else {
                            confirmed.iter().find(|new|new.external==mapping.external && new.kind==mapping.kind && nucleus::knowledge::mapping_matches(new,&incoming)).cloned().ok_or_else(||invalid("An external meaning changed; explicitly map its new version before accepting these claims"))?
                        };
                        definitions.insert(
                            mapping.concept_uid.clone(),
                            store::vocabulary::dictionary_on(&mut tx, &mapping.concept_uid).await?,
                        );
                        mappings.push(mapping);
                    }
                    for mapping in confirmed
                        .into_iter()
                        .filter(|mapping| nucleus::knowledge::mapping_matches(mapping, &incoming))
                    {
                        if !mappings
                            .iter()
                            .any(|old| old.external == mapping.external && old.kind == mapping.kind)
                        {
                            definitions.insert(
                                mapping.concept_uid.clone(),
                                store::vocabulary::dictionary_on(&mut tx, &mapping.concept_uid)
                                    .await?,
                            );
                            mappings.push(mapping);
                        }
                    }
                    saved["mappings"] = json!(mappings);
                    saved["definitions"] = json!(definitions.into_values().collect::<Vec<_>>());
                    item.claims = incoming.claims.clone();
                    item.meanings = incoming.meanings.clone();
                }
                UpdateField::Attribution => {
                    item.credits = incoming.credits.clone();
                    item.license = incoming.license.clone();
                    item.uri = incoming.uri.clone();
                }
                UpdateField::Assets => {
                    if incoming
                        .assets
                        .iter()
                        .any(|asset| asset.required && asset.bytes.is_none())
                    {
                        return Err(invalid(
                            "Read the required assets before accepting this update",
                        ));
                    }
                    item.assets = incoming.assets.clone();
                }
            }
        }
        if fields.iter().any(|field| {
            matches!(
                field,
                UpdateField::Content | UpdateField::Claims | UpdateField::Assets
            )
        }) {
            let mut dependencies = saved["dependencies"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for required in incoming
                .dependencies
                .iter()
                .filter(|dependency| dependency.required)
            {
                let organ = metadata["organ_uid"]
                    .as_str()
                    .ok_or_else(|| invalid("Saved knowledge has no Organ"))?;
                if let Some(uid) = store::knowledge::reference_uid_on(
                    &mut tx,
                    organ,
                    &required.source,
                    &required.id,
                )
                .await?
                {
                    if !dependencies.iter().any(|known| known == &uid) {
                        dependencies.push(json!(uid));
                    }
                }
                let mut found = false;
                for dependency in &dependencies {
                    let uid = dependency
                        .as_str()
                        .ok_or_else(|| invalid("Saved dependency identity unavailable"))?;
                    if let Some(payload) = store::knowledge::payload_on(&mut tx, uid).await? {
                        if payload["item"]["source"] == required.source
                            && payload["item"]["id"] == required.id
                        {
                            let graph = crate::access::policy_graph_on(&mut tx, &[uid.to_owned()])
                                .await
                                .map_err(|error| invalid(&error.to_string()))?;
                            if !self
                                .readable_on(&mut tx, actor, &graph)
                                .await?
                                .contains(uid)
                            {
                                return Err(EngineError::Forbidden(
                                    "Required saved content unavailable".into(),
                                ));
                            }
                            found = true;
                        }
                    }
                }
                if !found {
                    return Err(invalid(
                        "Save the new required dependency before accepting this source update",
                    ));
                }
            }
            item.dependencies = incoming.dependencies.clone();
            if dependencies.len() > nucleus::knowledge::DEPENDENCY_LIMIT {
                return Err(invalid(
                    "Save at most 128 related knowledge Records together",
                ));
            }
            saved["dependencies"] = json!(dependencies);
        }
        store::knowledge::validate_item(&item)?;
        let previous: Item = serde_json::from_value(saved["item"].clone())?;
        item.revision = incoming.revision.clone();
        item.raw = incoming.raw.clone();
        saved["item"] = serde_json::to_value(&item)?;
        saved["source_baseline"] = serde_json::to_value(&incoming)?;
        let mut provenance = saved["source_history"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !provenance
            .iter()
            .any(|version| version["revision"] == previous.revision)
        {
            provenance.push(json!({"revision":previous.revision,"raw":previous.raw}));
        }
        if provenance.len() > 256 {
            return Err(invalid(
                "Keep at most 256 source versions before creating a local copy",
            ));
        }
        saved["source_history"] = json!(provenance);
        let result_uid = if fork {
            let organ = metadata["organ_uid"]
                .as_str()
                .ok_or_else(|| invalid("Saved knowledge has no Organ"))?;
            self.knowledge_destination_on(&mut tx, actor, organ).await?;
            let uid = nucleus::new_uid("r");
            let fork_checkpoint = self
                .record_checkpoint_on(&mut tx, actor, vec![uid.clone()], Operation::Create)
                .await?;
            store::records::create_with_uid_on(
                &mut tx,
                &uid,
                store::records::NewRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: metadata["head"].as_str().unwrap_or(&item.title),
                    body: metadata["body"].as_str().unwrap_or(""),
                    quantity: store::exact::zero(),
                },
                organ,
                None,
            )
            .await?;
            saved["detached"] = json!(true);
            store::knowledge::save_payload_on(&mut tx, &uid, &item, &saved).await?;
            if let Some(checkpoint) = fork_checkpoint {
                checkpoint.finish(&mut tx, Default::default()).await?;
            }
            uid
        } else {
            if fields.contains(&UpdateField::Title) {
                store::sqlx::query("UPDATE record SET head=?,updated_at=? WHERE uid=?")
                    .bind(&item.title)
                    .bind(nucleus::execution::now().to_rfc3339())
                    .bind(record)
                    .execute(&mut *tx)
                    .await?;
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "record",
                    record,
                    "head",
                    store::sync_ops::OpKind::Set,
                    Some(json!(item.title).to_string()),
                )
                .await?;
            }
            saved["local_baseline"] = json!({"head":if fields.contains(&UpdateField::Title) {json!(item.title)} else {metadata["head"].clone()},"body":metadata["body"]});
            store::knowledge::save_payload_on(&mut tx, record, &item, &saved).await?;
            record.to_owned()
        };
        if let Some(checkpoint) = checkpoint {
            checkpoint.finish(&mut tx, Default::default()).await?;
        }
        let result = json!({"record_uid":result_uid,"forked":fork,"accepted_fields":fields,"annotations_preserved":true});
        store::sqlx::query("UPDATE knowledge_review SET applied=?,content=? WHERE uid=?")
            .bind(result.to_string())
            .bind(json!({"record_uid":record}).to_string())
            .bind(review)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(result)
    }
}
