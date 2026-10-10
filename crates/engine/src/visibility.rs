use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::visibility::{Command, Context, Data, Policy};
use serde_json::{Value, json};

fn denied(message: &str) -> EngineError {
    EngineError::Forbidden(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use nucleus::visibility::Condition;

    #[tokio::test]
    async fn saved_places_and_policy_configuration_stay_out_of_foreign_sync() {
        let engine = Engine::open_memory().await.unwrap();
        let record = engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Meet".into(),
                    body: String::new(),
                    quantity: -1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        engine
            .act(
                Action::SetPlace {
                    target: record.clone(),
                    lat: -23.5,
                    lon: -46.6,
                    address: None,
                },
                None,
            )
            .await
            .unwrap();
        let peer = nucleus::new_uid("r");
        store::organs::add_contact(&engine.store.pool, &peer, None, "Passenger", "", 1)
            .await
            .unwrap();
        store::organs::set_trust(&engine.store.pool, &peer, "known")
            .await
            .unwrap();
        engine
            .act(
                Action::SetSyncPolicy {
                    target: peer.clone(),
                    sync_in: true,
                    sync_out: true,
                },
                None,
            )
            .await
            .unwrap();
        let local = store::organs::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        engine
            .set_organ_signer(crate::trust::Signer::generate(
                &local,
                &crate::roster::cell_key_id(&cell),
            ))
            .await
            .unwrap();
        let vector = vec![store::sync_ops::VectorEntry {
            actor_cell: cell,
            max_hlc: 0,
        }];
        let page = engine.export_sync_page(&peer, &vector, 2000).await.unwrap();
        assert!(!page.batch.ops.iter().any(|op| op.tbl == "data_visibility"));
        assert!(!page.batch.ops.iter().any(|op| op.uid == record));
        assert!(
            !serde_json::to_string(&page.batch)
                .unwrap()
                .contains("-23.5")
        );
        let page = engine
            .export_sync_page(&local, &vector, 2000)
            .await
            .unwrap();
        assert!(
            page.batch
                .ops
                .iter()
                .any(|op| op.tbl == "data_visibility" && op.uid == record)
        );
        assert!(
            serde_json::to_string(&page.batch)
                .unwrap()
                .contains("-23.5")
        );
        engine
            .visibility_request(
                Command::Save {
                    record_uid: record.clone(),
                    data: Data::Place,
                    policy: Policy {
                        include: vec![Condition {
                            organs: vec![peer.clone()],
                            ..Default::default()
                        }],
                        exclude: vec![],
                    },
                    expected_revision: 1,
                },
                None,
            )
            .await
            .unwrap();
        let page = engine.export_sync_page(&peer, &vector, 2000).await.unwrap();
        assert!(!page.batch.ops.iter().any(|op| op.tbl == "data_visibility"));
        assert!(
            serde_json::to_string(&page.batch)
                .unwrap()
                .contains("-23.5")
        );
        let receiver = Engine::open_memory().await.unwrap();
        store::organs::add_contact(&receiver.store.pool, &local, None, "Sender", "", 1)
            .await
            .unwrap();
        store::organs::set_trust(&receiver.store.pool, &local, "known")
            .await
            .unwrap();
        receiver
            .act(
                Action::SetSyncPolicy {
                    target: local.clone(),
                    sync_in: true,
                    sync_out: true,
                },
                None,
            )
            .await
            .unwrap();
        let private = engine
            .export_sync_page(&local, &vector, 2000)
            .await
            .unwrap()
            .batch
            .ops
            .into_iter()
            .filter(|op| op.tbl == "data_visibility")
            .collect();
        receiver
            .receive_sync_batch(
                &local,
                &crate::sync::OpBatch {
                    from_organ: local.clone(),
                    ops: private,
                },
            )
            .await
            .unwrap();
        assert!(
            store::data_visibility::policy(&receiver.store.pool, &record, Data::Place)
                .await
                .unwrap()
                .is_none()
        );
    }
}

impl Engine {
    pub async fn visibility_request(
        &self,
        request: Command,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, "record:read").await?;
        let (token, data) = match &request {
            Command::Context { record_uid, data }
            | Command::Save {
                record_uid, data, ..
            } => (record_uid, *data),
        };
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| denied("Local Organ unavailable"))?;
        let record = if token.is_empty() {
            String::new()
        } else {
            self.resolve(token).await?
        };
        if !record.is_empty() {
            self.refuse_unreadable(actor, std::slice::from_ref(&record))
                .await?;
            let row = store::records::get(&self.store.pool, &record)
                .await?
                .ok_or_else(|| denied("Record unavailable"))?;
            let replicated: bool = store::sqlx::query_scalar(
                "SELECT replica_root IS NOT NULL FROM record WHERE uid=?",
            )
            .bind(&record)
            .fetch_one(&self.store.pool)
            .await?;
            if row
                .organ_uid
                .as_deref()
                .is_some_and(|organ| organ != local.uid)
                || replicated
            {
                return Err(denied("Manage visibility at the Record's owning Organ"));
            }
        }
        self.require_permission(actor, "permission:assign").await?;
        if data == Data::LiveLocation && !record.is_empty() {
            let authority = self.location_authority(&record).await?;
            if authority != self.location_node_id()? {
                let data = self
                    .location_network()?
                    .location_request(
                        &authority,
                        nucleus::location::PeerRequest::Visibility {
                            command: request,
                            person: actor.map(str::to_owned),
                        },
                    )
                    .await?;
                return Ok(ActionOutcome {
                    data: Some(data),
                    ..Default::default()
                });
            }
        }
        let location = if data == Data::LiveLocation {
            store::location::settings(&self.store.pool, &record).await?
        } else {
            None
        };
        if location
            .as_ref()
            .is_some_and(|settings| actor.is_some_and(|actor| actor != settings.controller_uid))
        {
            return Err(denied(
                "Only the location controller can manage its visibility",
            ));
        }
        if let Command::Save {
            policy,
            expected_revision,
            ..
        } = request
        {
            if record.is_empty() {
                return Err(denied("Choose a Record first"));
            }
            policy
                .validate()
                .map_err(|error| EngineError::Consequence(error.into()))?;
            self.require_permission(actor, "record:update").await?;
            let controller = None;
            if let Some(saved) =
                store::data_visibility::policy(&self.store.pool, &record, data).await?
            {
                if saved.controller_uid != controller.unwrap_or_default() {
                    return Err(denied("Visibility belongs to another controller"));
                }
            }
            let mut tx = store::write_tx(&self.store.pool).await?;
            self.require_permission_on(&mut tx, actor, "permission:assign")
                .await?;
            self.require_permission_on(&mut tx, actor, "record:update")
                .await?;
            self.require_permission_on(&mut tx, actor, "record:read")
                .await?;
            let owned: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record r JOIN record o ON o.slug='local-organ' AND o.kind='organ' AND o.deleted_at IS NULL WHERE r.uid=? AND r.deleted_at IS NULL AND r.replica_root IS NULL AND (r.organ_uid IS NULL OR r.organ_uid=o.uid))")
                .bind(&record).fetch_one(&mut *tx).await?;
            if !owned {
                return Err(denied("Record ownership changed; reload visibility"));
            }
            if actor.is_some() {
                let graph = crate::access::policy_graph_on(&mut tx, std::slice::from_ref(&record))
                    .await
                    .map_err(|error| denied(&error.to_string()))?;
                if !self
                    .readable_on(&mut tx, actor, &graph)
                    .await?
                    .contains(&record)
                {
                    return Err(denied("The Record is no longer readable"));
                }
                if data == Data::LiveLocation {
                    let owner: Option<String> = store::sqlx::query_scalar(
                        "SELECT controller_uid FROM location_settings WHERE record_uid=?",
                    )
                    .bind(&record)
                    .fetch_optional(&mut *tx)
                    .await?;
                    if owner.as_deref().is_some_and(|owner| Some(owner) != actor) {
                        return Err(denied(
                            "Only the location controller can manage its visibility",
                        ));
                    }
                }
            }
            let revision = store::data_visibility::save_on(
                &mut tx,
                &record,
                data,
                controller,
                &policy,
                expected_revision,
            )
            .await?;
            let saved = nucleus::visibility::SavedPolicy {
                controller_uid: String::new(),
                revision,
                policy: policy.clone(),
            };
            store::sync_ops::log_local_tx(
                &mut tx,
                "data_visibility",
                &record,
                data.as_str(),
                store::sync_ops::OpKind::Set,
                Some(serde_json::to_string(&saved)?),
            )
            .await?;
            store::sqlx::query(
                "UPDATE organ_contact SET share_seen_seq=NULL WHERE share_protein IS NOT NULL",
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
        let mut records = Vec::new();
        let choices: Vec<(String, String)> = store::sqlx::query_as("SELECT uid,head FROM record WHERE deleted_at IS NULL AND replica_root IS NULL AND (organ_uid IS NULL OR organ_uid=?) ORDER BY head,uid LIMIT 256")
            .bind(&local.uid).fetch_all(&self.store.pool).await?;
        for (uid, label) in choices {
            if self.may_read_record(actor, &uid).await? {
                records.push(nucleus::location::Choice {
                    uid,
                    label,
                    node_id: None,
                });
            }
        }
        let mut organs = Vec::new();
        for organ in store::data_visibility::organs(&self.store.pool).await? {
            if self.may_read_record(actor, &organ.uid).await? {
                organs.push(organ);
            }
        }
        let saved = store::data_visibility::policy(&self.store.pool, &record, data).await?;
        Ok(ActionOutcome {
            data: Some(serde_json::to_value(Context {
                record_uid: record,
                data,
                saved,
                organs,
                records,
            })?),
            ..Default::default()
        })
    }

    pub async fn organ_visibility(
        &self,
        record: &str,
        data: Data,
        organ: &str,
    ) -> Result<nucleus::visibility::Decision, EngineError> {
        let proximity = store::data_visibility::proximity(&self.store.pool, organ).await?;
        let policy = store::data_visibility::policy(&self.store.pool, record, data)
            .await?
            .map_or_else(Policy::default, |saved| saved.policy);
        let mut decision = policy.decide(organ, proximity);
        if proximity.is_none() {
            decision.allowed = false;
        }
        Ok(decision)
    }

    pub async fn visibility_read(
        &self,
        organ: &str,
        record: &str,
        data: Data,
    ) -> Result<Value, EngineError> {
        if !self.organ_visibility(record, data, organ).await?.allowed {
            return Err(denied("This Organ cannot view the selected Record data"));
        }
        if data == Data::LiveLocation {
            return self.location_organ_view(organ, record).await;
        }
        let row = store::records::get(&self.store.pool, record)
            .await?
            .ok_or_else(|| denied("Record data unavailable"))?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| denied("Local Organ unavailable"))?;
        if row.organ_uid.as_deref().is_some_and(|uid| uid != local.uid) {
            return Err(denied("Read this data at the Record's owning Organ"));
        }
        let place = if data == Data::Place
            || self
                .organ_visibility(record, Data::Place, organ)
                .await?
                .allowed
        {
            if let Some(uid) = row.place_uid.as_deref() {
                let place: Option<(f64, f64, Option<String>)> =
                    store::sqlx::query_as("SELECT lat,lon,address FROM place WHERE uid=?")
                        .bind(uid)
                        .fetch_optional(&self.store.pool)
                        .await?;
                place.map(|(latitude, longitude, address)| json!({"latitude":latitude,"longitude":longitude,"address":address}))
            } else {
                None
            }
        } else {
            None
        };
        if data == Data::Place {
            return Ok(json!({"record_uid":record,"place":place}));
        }
        Ok(
            json!({"record":{"uid":row.uid,"head":row.head,"body":row.body,"quantity":row.quantity.to_f64()},"place":place}),
        )
    }
}
