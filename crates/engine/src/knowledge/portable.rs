use super::{Engine, EngineError, invalid};
use nucleus::knowledge::Item;
use protein::authority::Operation;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet, VecDeque};

impl Engine {
    pub(super) async fn export_knowledge(
        &self,
        record: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.refuse_unreadable(actor, &[record.to_owned()]).await?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:read")
            .await?;
        let mut queue = VecDeque::from([record.to_owned()]);
        let mut visited = HashSet::new();
        let mut records = Vec::new();
        while let Some(uid) = queue.pop_front() {
            if !visited.insert(uid.clone()) {
                continue;
            }
            if visited.len() > nucleus::knowledge::DEPENDENCY_LIMIT {
                return Err(invalid(
                    "Export at most 128 related knowledge Records together",
                ));
            }
            let graph = crate::access::policy_graph_on(&mut tx, &[uid.clone()])
                .await
                .map_err(|error| invalid(&error.to_string()))?;
            if !self
                .readable_on(&mut tx, actor, &graph)
                .await?
                .contains(&uid)
            {
                return Err(EngineError::Forbidden(
                    "A required knowledge Record is unavailable".into(),
                ));
            }
            let payload = store::knowledge::payload_on(&mut tx, &uid)
                .await?
                .ok_or_else(|| invalid("Save the required source content before exporting it"))?;
            let mut payload = payload;
            if let Some(dependencies) = payload["dependencies"].as_array() {
                for dependency in dependencies {
                    queue.push_back(
                        dependency
                            .as_str()
                            .ok_or_else(|| invalid("Saved knowledge has an invalid dependency"))?
                            .to_owned(),
                    );
                }
            }
            payload
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid saved knowledge"))?
                .remove("local_baseline");
            let entry = json!({"uid":uid,"payload":payload});
            records.push(entry);
        }
        let origin: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL",
        )
        .bind(store::organs::LOCAL_ORGAN_SLUG)
        .fetch_one(&mut *tx)
        .await?;
        Ok(
            json!({"package":{"format":1,"kind":"knowledge_bundle","origin_organ":origin,"root":record,"records":records},"includes_local_annotations":false}),
        )
    }

    pub(super) async fn import_knowledge(
        &self,
        raw: &str,
        organ: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let package: Value = serde_json::from_str(raw)?;
        if package["format"] != 1 || package["kind"] != "knowledge_bundle" {
            return Err(invalid("Choose a supported knowledge dependency bundle"));
        }
        let origin = package["origin_organ"]
            .as_str()
            .filter(|uid| nucleus::valid_uid(uid, "r"))
            .ok_or_else(|| invalid("Knowledge bundle origin unavailable"))?;
        let root = package["root"]
            .as_str()
            .ok_or_else(|| invalid("Knowledge bundle has no selected root"))?;
        let records = package["records"]
            .as_array()
            .filter(|records| {
                !records.is_empty() && records.len() <= nucleus::knowledge::DEPENDENCY_LIMIT
            })
            .ok_or_else(|| invalid("Import 1–128 related knowledge Records together"))?;
        let mut incoming = BTreeMap::new();
        let mut identities = HashSet::new();
        for record in records {
            let uid = record["uid"]
                .as_str()
                .filter(|uid| nucleus::valid_uid(uid, "r"))
                .ok_or_else(|| invalid("Knowledge bundle has an invalid Record identity"))?;
            let item: Item = serde_json::from_value(record["payload"]["item"].clone())?;
            store::knowledge::validate_item(&item)?;
            if record["payload"]["format"] != 1
                || !identities.insert((item.source.clone(), item.id.clone()))
                || incoming
                    .insert(uid.to_owned(), (item, record["payload"].clone()))
                    .is_some()
            {
                return Err(invalid(
                    "Knowledge bundle items must retain distinct source identities",
                ));
            }
        }
        if !incoming.contains_key(root) {
            return Err(invalid("Knowledge bundle root is missing"));
        }
        for (item, payload) in incoming.values() {
            if item
                .assets
                .iter()
                .any(|asset| asset.required && asset.bytes.is_none())
            {
                return Err(invalid(
                    "A required asset is missing from the knowledge bundle",
                ));
            }
            for required in item
                .dependencies
                .iter()
                .filter(|dependency| dependency.required)
            {
                if !payload["dependencies"]
                    .as_array()
                    .is_some_and(|dependencies| {
                        dependencies.iter().any(|uid| {
                            uid.as_str().and_then(|uid| incoming.get(uid)).is_some_and(
                                |(item, _)| {
                                    item.source == required.source && item.id == required.id
                                },
                            )
                        })
                    })
                {
                    return Err(invalid(
                        "Knowledge bundle is missing required source content",
                    ));
                }
            }
            for dependency in payload["dependencies"]
                .as_array()
                .ok_or_else(|| invalid("Knowledge bundle has no dependency list"))?
            {
                if !dependency
                    .as_str()
                    .is_some_and(|uid| incoming.contains_key(uid))
                {
                    return Err(invalid(
                        "Knowledge bundle is missing a required related Record",
                    ));
                }
            }
            for definition in payload["definitions"]
                .as_array()
                .ok_or_else(|| invalid("Knowledge bundle has no local meaning definitions"))?
            {
                let dictionary: nucleus::vocabulary::Dictionary =
                    serde_json::from_value(definition.clone())?;
                store::vocabulary::validate_dictionary(&dictionary)?;
            }
            let mappings: Vec<nucleus::knowledge::Mapping> =
                serde_json::from_value(payload["mappings"].clone())?;
            for mapping in mappings {
                if !nucleus::knowledge::mapping_matches(&mapping, item) {
                    return Err(invalid(
                        "A knowledge mapping does not match its external meaning version",
                    ));
                }
                let known = payload["definitions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|dictionary| {
                        dictionary["source"]["concept_uid"] == mapping.concept_uid
                            && dictionary["definitions"]
                                .as_array()
                                .is_some_and(|versions| {
                                    versions.iter().any(|version| {
                                        version["revision"] == mapping.definition_revision
                                    })
                                })
                    });
                if !known {
                    return Err(invalid(
                        "A mapped meaning version is missing from the knowledge bundle",
                    ));
                }
            }
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.knowledge_destination_on(&mut tx, actor, organ).await?;
        let mut resolved = BTreeMap::new();
        let mut checkpoints = Vec::new();
        for (old, (item, payload)) in &incoming {
            for definition in payload["definitions"].as_array().unwrap() {
                let dictionary: nucleus::vocabulary::Dictionary =
                    serde_json::from_value(definition.clone())?;
                let exists: bool = store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=?)",
                )
                .bind(&dictionary.source.concept_uid)
                .fetch_one(&mut *tx)
                .await?;
                if exists {
                    let current =
                        store::vocabulary::dictionary_on(&mut tx, &dictionary.source.concept_uid)
                            .await?;
                    if current.source != dictionary.source
                        || dictionary.definitions.iter().any(|version| {
                            !current.definitions.iter().any(|known| {
                                known.revision == version.revision
                                    && known.definition == version.definition
                            })
                        })
                    {
                        return Err(invalid(
                            "Review the conflicting local meaning before importing knowledge",
                        ));
                    }
                } else {
                    self.require_permission_on(&mut tx, actor, "permission:assign")
                        .await?;
                    store::vocabulary::apply_dictionary_on(&mut tx, &dictionary, origin, false)
                        .await?;
                    store::vocabulary::log_shared_on(&mut tx, &dictionary.source.concept_uid)
                        .await?;
                }
            }
            let detached = payload["detached"] == true;
            let existing = if detached {
                store::knowledge::imported_copy_on(&mut tx, organ, origin, old).await?
            } else {
                store::knowledge::reference_uid_on(&mut tx, organ, &item.source, &item.id).await?
            };
            let uid = existing.clone().unwrap_or_else(|| nucleus::new_uid("r"));
            let operation = if existing.is_some() {
                self.require_permission_on(&mut tx, actor, "record:update")
                    .await?;
                Operation::Update
            } else {
                Operation::Create
            };
            let checkpoint = self
                .record_checkpoint_on(&mut tx, actor, vec![uid.clone()], operation)
                .await?;
            if let Some(checkpoint) = checkpoint {
                checkpoints.push(checkpoint);
            }
            if detached && existing.is_none() {
                store::records::create_with_uid_on(
                    &mut tx,
                    &uid,
                    store::records::NewRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: &item.title,
                        body: "",
                        quantity: store::exact::zero(),
                    },
                    organ,
                    None,
                )
                .await?;
                store::records::set_extension_on(
                    &mut tx,
                    &uid,
                    store::knowledge::IMPORT_NAMESPACE,
                    &json!({"origin_organ":origin,"record_uid":old}),
                )
                .await?;
            } else if !detached {
                store::knowledge::create_reference_on(&mut tx, &uid, organ, item).await?;
            }
            resolved.insert(old.clone(), uid);
        }
        for (old, (item, payload)) in &incoming {
            let uid = &resolved[old];
            let previous = store::knowledge::payload_on(&mut tx, uid).await?;
            if let Some(previous) = previous {
                if previous["item"] != payload["item"] {
                    return Err(invalid(
                        "Review the source update before replacing saved knowledge",
                    ));
                }
                continue;
            }
            let mut payload = payload.clone();
            payload["dependencies"] = json!(
                payload["dependencies"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|dependency| resolved[dependency.as_str().unwrap()].clone())
                    .collect::<Vec<_>>()
            );
            let metadata = store::knowledge::record_metadata_on(&mut tx, uid).await?;
            payload["local_baseline"] = json!({"head":metadata["head"],"body":metadata["body"]});
            store::knowledge::save_payload_on(&mut tx, uid, item, &payload).await?;
        }
        for checkpoint in checkpoints {
            checkpoint.finish(&mut tx, Default::default()).await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"record_uid":resolved[root],"saved_records":resolved.into_values().collect::<Vec<_>>(),"offline":true}),
        )
    }
}
