use crate::{Engine, EngineError, actions::ActionOutcome};
use chrono::{DateTime, Utc};
use nucleus::vocabulary::{SavedLocation, SavedLocationCommand};
use serde_json::json;
use std::collections::HashSet;

impl Engine {
    async fn saved_location_readable(
        &self,
        actor: Option<&str>,
    ) -> Result<Option<HashSet<String>>, EngineError> {
        if actor.is_none() {
            return Ok(None);
        }
        let query: protein::Protein =
            serde_json::from_value(json!({"source":"record","fields":["uid"]}))?;
        Ok(Some(
            protein::execute_for(&self.store, &query, actor)
                .await?
                .into_iter()
                .filter_map(|row| row["uid"].as_str().map(str::to_owned))
                .collect(),
        ))
    }

    pub async fn saved_location_of(
        &self,
        record: &str,
        actor: Option<&str>,
    ) -> Result<SavedLocation, EngineError> {
        let record = self.resolve(record).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&record))
            .await?;
        if let Some(row) = store::records::get(&self.store.pool, &record).await?
            && let Some(organ) = row.organ_uid
            && store::organs::local(&self.store.pool)
                .await?
                .is_some_and(|local| local.uid != organ)
        {
            let node = store::organs::contact(&self.store.pool, &organ)
                .await?
                .filter(|contact| contact.trust == "known")
                .and_then(|contact| contact.node_id)
                .ok_or_else(|| {
                    EngineError::Forbidden("Read the saved location at its owning Organ".into())
                })?;
            return self.observe_saved_location(&record, &node, actor).await;
        }
        let visible = self.saved_location_readable(actor).await?;
        store::places::saved_locations(&self.store.pool, visible.as_ref())
            .await?
            .remove(&record)
            .ok_or_else(|| EngineError::Forbidden("Record location unavailable".into()))
    }

    pub(crate) async fn saved_location_command(
        &self,
        command: SavedLocationCommand,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        match command {
            SavedLocationCommand::Observe { record, node_id } => Ok(ActionOutcome {
                data: Some(serde_json::to_value(
                    self.observe_saved_location(&record, &node_id, actor)
                        .await?,
                )?),
                ..Default::default()
            }),
            SavedLocationCommand::Inspect { record } => Ok(ActionOutcome {
                data: Some(serde_json::to_value(
                    self.saved_location_of(&record, actor).await?,
                )?),
                ..Default::default()
            }),
            SavedLocationCommand::Search { text } => {
                if text.len() > 256 {
                    return Err(EngineError::Consequence(
                        "Search for an address with at most 256 bytes".into(),
                    ));
                }
                let visible = self.saved_location_readable(actor).await?;
                let mut choices: Vec<_> =
                    store::places::saved_locations(&self.store.pool, visible.as_ref())
                        .await?
                        .into_values()
                        .flat_map(|location| {
                            location
                                .candidates
                                .into_iter()
                                .filter(|candidate| candidate.assertion_uid.is_none())
                        })
                        .filter(|candidate| {
                            let text = text.trim().trim_start_matches('@').to_lowercase();
                            candidate.head.to_lowercase().contains(&text)
                                || candidate
                                    .slug
                                    .as_ref()
                                    .is_some_and(|slug| slug.to_lowercase().contains(&text))
                                || candidate.record_uid.eq_ignore_ascii_case(&text)
                        })
                        .collect();
                choices.sort_by(|a, b| a.head.cmp(&b.head).then(a.record_uid.cmp(&b.record_uid)));
                choices.truncate(30);
                Ok(ActionOutcome {
                    data: Some(json!({"addresses": choices})),
                    ..Default::default()
                })
            }
            SavedLocationCommand::Bind { record, address } => {
                let record = self.resolve(&record).await?;
                let address = self.resolve(address.trim_start_matches('@')).await?;
                let location = self.saved_location_of(&address, actor).await?;
                if !location.candidates.iter().any(|candidate| {
                    candidate.assertion_uid.is_none() && candidate.radius_metres.is_none()
                }) {
                    return Err(EngineError::Consequence(
                        "Choose an address Record with its own saved coordinates".into(),
                    ));
                }
                self.reject_direct_transfer_record_mutation(&record).await?;
                let (predicate, revision, _) =
                    store::vocabulary::location_bindings(&self.store.pool).await?;
                let mut outcome = self
                    .change_location_binding(
                        crate::record_change::Request {
                            id: nucleus::new_uid("op"),
                            record_uid: record.clone(),
                            mutation: crate::record_change::Mutation::Assertion {
                                predicate,
                                object: Some(address),
                                quantity: None,
                                unit: None,
                            },
                        },
                        actor,
                        revision,
                    )
                    .await?;
                outcome.data = Some(serde_json::to_value(
                    self.saved_location_of(&record, actor).await?,
                )?);
                Ok(outcome)
            }
            SavedLocationCommand::Select {
                record,
                assertion,
                expected_revision,
            } => {
                let record = self.resolve(&record).await?;
                self.reject_direct_transfer_record_mutation(&record).await?;
                self.authorize_action(
                    &crate::actions::Action::SetPlace {
                        target: record.clone(),
                        lat: 0.0,
                        lon: 0.0,
                        address: None,
                    },
                    actor,
                )
                .await?;
                let location = self.saved_location_of(&record, actor).await?;
                if !location
                    .candidates
                    .iter()
                    .any(|candidate| candidate.assertion_uid == assertion)
                {
                    return Err(EngineError::Consequence(
                        "Choose an available saved location".into(),
                    ));
                }
                let signer = self.signer.lock().await.clone();
                let identity = store::cells::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("Local Cell unavailable".into()))?;
                let serial = self.import_lock.lock().await;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:update")
                    .await?;
                let checkpoint = self
                    .record_checkpoint_on(
                        &mut tx,
                        actor,
                        vec![record.clone()],
                        protein::authority::Operation::Update,
                    )
                    .await?;
                let current: i64 = store::sqlx::query_scalar("SELECT COALESCE((SELECT clock FROM record_property WHERE record_uid=? AND property='location.source'),(SELECT revision FROM record_location_choice WHERE record_uid=?),0)").bind(&record).bind(&record).fetch_one(&mut *tx).await?;
                if current != expected_revision {
                    return Err(EngineError::Consequence(
                        "The selected location changed; refresh before choosing".into(),
                    ));
                }
                let valid: bool = if let Some(assertion) = &assertion {
                    store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_assertion a JOIN vocabulary_role_concept b ON b.concept_uid=a.predicate_uid AND b.role_uid=? JOIN record r ON r.uid=a.object_uid AND r.deleted_at IS NULL JOIN place p ON p.uid=r.place_uid WHERE a.uid=? AND a.subject_uid=? AND a.retracted_at IS NULL AND a.role='ordinary' AND a.quantity_mantissa IS NULL AND a.unit_uid IS NULL)")
                        .bind(nucleus::vocabulary::SAVED_LOCATION_ROLE_UID).bind(assertion).bind(&record).fetch_one(&mut *tx).await?
                } else {
                    store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record r JOIN place p ON p.uid=r.place_uid WHERE r.uid=? AND r.deleted_at IS NULL)").bind(&record).fetch_one(&mut *tx).await?
                };
                if !valid {
                    return Err(EngineError::Consequence(
                        "The selected location is no longer available".into(),
                    ));
                }
                let register = crate::record_change::Register {
                    clock: nucleus::hlc::next(),
                    peer: identity.uid,
                    change_uid: nucleus::new_uid("op"),
                    value: json!({"assertion":assertion}),
                };
                crate::record_change::apply_register(
                    &mut tx,
                    &record,
                    "location.source",
                    &register,
                )
                .await?;
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "record",
                    &record,
                    "property:location.source",
                    store::sync_ops::OpKind::Set,
                    Some(serde_json::to_string(&register)?),
                )
                .await?;
                store::sqlx::query("INSERT INTO record_location_choice(record_uid,source,assertion_uid,revision) VALUES(?,?,?,?) ON CONFLICT(record_uid) DO UPDATE SET source=excluded.source,assertion_uid=excluded.assertion_uid,revision=excluded.revision")
                    .bind(&record).bind(if assertion.is_some() { "assertion" } else { "direct" }).bind(&assertion).bind(register.clock).execute(&mut *tx).await?;
                if let Some(checkpoint) = checkpoint {
                    checkpoint
                        .finish(&mut tx, [protein::authority::Property::Place].into())
                        .await?;
                }
                let fact = crate::append::append_one_in_transaction(
                    &mut tx,
                    nucleus::NewFact {
                        uid: None,
                        record_uid: record.clone(),
                        delta: store::exact::zero(),
                        at: None,
                        actor_uid: actor.map(str::to_owned),
                        cause: nucleus::Cause::user_edit(),
                        payload: Some(json!({"saved_location_selected": assertion}).to_string()),
                    },
                    now,
                    signer.as_ref(),
                )
                .await?;
                tx.commit().await?;
                drop(serial);
                let facts = if let Some(fact) = fact {
                    self.observe_committed_fact(fact, now).await?
                } else {
                    Vec::new()
                };
                Ok(ActionOutcome {
                    data: Some(serde_json::to_value(
                        self.saved_location_of(&record, actor).await?,
                    )?),
                    facts,
                    ..Default::default()
                })
            }
        }
    }

    async fn observe_saved_location(
        &self,
        record: &str,
        node: &str,
        actor: Option<&str>,
    ) -> Result<SavedLocation, EngineError> {
        self.require_permission(actor, "record:read").await?;
        nucleus::location::Invitation {
            record_uid: record.into(),
            authority_node_id: node.into(),
        }
        .reference()
        .map_err(|message| EngineError::Consequence(message.into()))?;
        let data = self
            .location_network()?
            .location_request(
                node,
                nucleus::location::PeerRequest::Read {
                    record_uid: record.into(),
                    data: nucleus::visibility::Data::Place,
                },
            )
            .await?;
        if data["record_uid"].as_str() != Some(record) {
            return Err(EngineError::Consequence(
                "The saved location reply belongs to another Record".into(),
            ));
        }
        #[derive(serde::Deserialize)]
        struct SharedPlace {
            latitude: f64,
            longitude: f64,
            address: Option<String>,
            #[serde(default)]
            radius_metres: Option<u32>,
        }
        let selected = if data["place"].is_null() {
            None
        } else {
            let place: SharedPlace = serde_json::from_value(data["place"].clone())?;
            if !place.latitude.is_finite()
                || !place.longitude.is_finite()
                || !(-90.0..=90.0).contains(&place.latitude)
                || !(-180.0..=180.0).contains(&place.longitude)
                || place
                    .radius_metres
                    .is_some_and(|radius| !nucleus::place::valid_area_radius(radius))
            {
                return Err(EngineError::Consequence(
                    "Invalid shared location geometry".into(),
                ));
            }
            Some(nucleus::vocabulary::LocationCandidate {
                assertion_uid: None,
                record_uid: record.into(),
                head: if place.radius_metres.is_some() {
                    "Approximate area".into()
                } else {
                    place.address.unwrap_or_else(|| "Saved location".into())
                },
                slug: None,
                latitude: place.latitude,
                longitude: place.longitude,
                radius_metres: place.radius_metres,
            })
        };
        Ok(SavedLocation {
            record_uid: record.into(),
            candidates: selected.clone().into_iter().collect(),
            selected,
            explicit: false,
            unavailable: data["place"].is_null(),
            revision: 0,
        })
    }
}
