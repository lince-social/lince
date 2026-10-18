use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use crate::{Engine, EngineError, actions::TransferPromiseInput};
use nucleus::transfer::transport::Direction;
use nucleus::transfer::transport::{Event, JOURNEY_DOMAIN, Step};
use serde_json::json;

fn own_journey(
    state: Option<nucleus::transfer::transport::Journey>,
    person: &str,
) -> Option<nucleus::transfer::transport::Journey> {
    state.map(|mut state| {
        state
            .participants
            .retain(|progress| progress.person == person);
        state
    })
}

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "transport_terms_invalid",
        message: message.to_string(),
    }
}

impl Engine {
    pub(crate) async fn social_transport_draft(
        &self,
        source: &str,
        actor: Option<&str>,
    ) -> Result<Option<nucleus::social::PostDraft>, EngineError> {
        let Some(record) = store::records::resolve(&self.store.pool, source).await? else {
            return Ok(None);
        };
        if record.kind != nucleus::RecordKind::Transfer.as_str() {
            return Ok(None);
        }
        self.social_own_record(&record.uid, actor).await?;
        let row: Option<(String, Option<String>, Option<String>)> = store::sqlx::query_as("SELECT item_json,window_start,window_end FROM promise WHERE transfer_uid=? AND (state='open' OR (state='proposed' AND json_extract(item_json,'$.transport.direction')='offer' AND json_extract(item_json,'$.exchange') IS NULL)) AND json_extract(item_json,'$.transport.activity') IS NOT NULL ORDER BY uid LIMIT 1")
            .bind(&record.uid).fetch_optional(&self.store.pool).await?;
        let (item, start, end) = row
            .ok_or_else(|| invalid("Choose an open transport Need or an uncommitted Offer without a selected recipient"))?;
        let item: nucleus::transfer::disclosure::TransferItem = serde_json::from_str(&item)?;
        let terms = item
            .transport
            .ok_or_else(|| invalid("Transport terms unavailable"))?;
        let mut areas = Vec::new();
        for uid in [&terms.origin, &terms.destination] {
            let place = self
                .saved_location_of(uid, actor)
                .await?
                .selected
                .ok_or_else(|| invalid("Transport address unavailable"))?;
            areas.push(
                nucleus::social::transport::Area::approximate(
                    place.latitude,
                    place.longitude,
                    place.radius_metres.unwrap_or(1000),
                )
                .map_err(invalid)?,
            );
        }
        let parse = |time: Option<String>| {
            chrono::DateTime::parse_from_rfc3339(time.as_deref().unwrap_or_default())
                .map(|time| time.timestamp())
                .map_err(invalid)
        };
        let transport = nucleus::social::transport::Announcement {
            activity: terms.activity,
            origin: areas.remove(0),
            destination: areas.remove(0),
            window_start: parse(start)?,
            window_end: parse(end)?,
            immediate: false,
        };
        transport.validate().map_err(invalid)?;
        Ok(Some(nucleus::social::PostDraft {
            title: format!(
                "{} {}",
                if terms.activity == nucleus::transfer::transport::Activity::Ride {
                    "Ride"
                } else {
                    "Delivery"
                },
                if terms.direction == Direction::Need {
                    "needed"
                } else {
                    "offered"
                }
            ),
            direction: if terms.direction == Direction::Need {
                nucleus::social::Direction::Need
            } else {
                nucleus::social::Direction::Contribution
            },
            transport: Some(transport),
            ..Default::default()
        }))
    }

    pub(crate) async fn record_journey_progress(
        &self,
        transfer: String,
        expected_revision: u64,
        person: String,
        request_id: String,
        step: Step,
        actor: Option<&str>,
        now: chrono::DateTime<chrono::Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        let transfer = self.resolve(&transfer).await?;
        let person = self
            .transfer_action_person(actor, Some(&person), None)
            .await?;
        for permission in ["transfer:read", "transfer:update"] {
            self.require_permission(actor, permission).await?;
        }
        self.refuse_unreadable(actor, std::slice::from_ref(&transfer))
            .await?;
        self.require_transfer_origin_authority(&transfer).await?;
        self.require_verified_transfer_revision(&transfer, expected_revision)
            .await?;
        let signer = self.transfer_person_signer(&person, authorship).await?;
        let event = Event {
            domain: JOURNEY_DOMAIN.into(),
            transfer: transfer.clone(),
            revision: expected_revision,
            person: person.clone(),
            step,
            request_id,
        };
        event.validate().map_err(invalid)?;
        let hash = nucleus::karma::canonical_hash("lince.journey-request.v1", &event.request_id)
            .map_err(invalid)?;
        let hash = hash
            .as_str()
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("Invalid journey request hash"))?;
        let fact_uid = format!(
            "f_{}",
            nucleus::ulid_from(
                u64::from_str_radix(&hash[..12], 16).map_err(invalid)?,
                u128::from_str_radix(&hash[12..32], 16).map_err(invalid)?
            )
        );
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "transfer:update")
            .await?;
        let source = store::transfers::karma_snapshot::read_on(&mut tx, &transfer).await?;
        if !source.participants.contains_key(&person) {
            return Err(invalid(
                "Only a current journey participant can report progress",
            ));
        }
        if let Some(previous) = store::facts::get_on(&mut tx, &fact_uid).await? {
            if previous.payload.as_deref().and_then(Event::parse).as_ref() != Some(&event)
                || previous.actor_uid.as_deref() != Some(person.as_str())
                || previous.record_uid != transfer
            {
                return Err(invalid(
                    "This request ID already belongs to another journey event",
                ));
            }
            let state = store::transport::journey_on(&mut tx, &transfer).await?;
            tx.commit().await?;
            return Ok(ActionOutcome {
                created: Some(previous.uid),
                data: Some(json!({"journey":own_journey(state,&person),"replayed":true})),
                ..Default::default()
            });
        }
        if source.revision != expected_revision
            || source
                .participants
                .values()
                .any(|party| party.guard.level != 2)
            || source
                .participants
                .get(&person)
                .is_none_or(|party| party.guard.level != 2)
        {
            return Err(invalid(
                "Agree to the current journey terms before recording progress",
            ));
        }
        let state = store::transport::journey_on(&mut tx, &transfer)
            .await?
            .ok_or_else(|| invalid("This Transfer has no transport journey"))?;
        let own = state
            .participants
            .iter()
            .find(|progress| progress.person == person)
            .ok_or_else(|| invalid("Only a journey participant can report progress"))?;
        if own.step == Some(step) {
            let created = own.fact.clone();
            tx.commit().await?;
            return Ok(ActionOutcome {
                created,
                data: Some(json!({"journey":own_journey(Some(state),&person),"replayed":true})),
                ..Default::default()
            });
        }
        if own
            .step
            .is_some_and(|previous| previous.terminal() || step < previous)
            || (state.expired || state.capacity_released) && !step.terminal()
        {
            return Err(invalid(
                "This journey already ended or advanced beyond that step",
            ));
        }
        if signer.is_none() && authorship.is_none() {
            return Err(invalid("Journey progress needs this Person's signature"));
        }
        let fact = crate::append::append_one_in_transaction(
            &mut tx,
            nucleus::NewFact {
                uid: Some(fact_uid),
                record_uid: transfer.clone(),
                delta: store::exact::zero(),
                at: Some(now),
                actor_uid: Some(person.clone()),
                cause: nucleus::Cause::user_edit(),
                payload: Some(json!({"journey":event}).to_string()),
            },
            now,
            signer.as_ref(),
        )
        .await?
        .ok_or_else(|| invalid("Journey event changed concurrently"))?;
        if let Some(authorship) = authorship {
            store::action_intents::link_pending_fact(&mut tx, &authorship.intent_uid, &fact)
                .await?;
        }
        let state = store::transport::journey_on(&mut tx, &transfer).await?;
        tx.commit().await?;
        if step.terminal() {
            self.stop_transfer_location(&person, &transfer, actor)
                .await?;
        }
        for policy in
            store::transfer_delivery::policies_for_transfer(&self.store.pool, &transfer).await?
        {
            if policy.state == "active" {
                self.enqueue_transfer_delivery(
                    &transfer,
                    &policy.uid,
                    &format!("journey:{}:{}", fact.uid, policy.uid),
                    now,
                )
                .await?;
            }
        }
        Ok(ActionOutcome {
            created: Some(fact.uid.clone()),
            facts: self.publish_committed_fact(fact),
            data: Some(json!({"journey":own_journey(state,&person),"replayed":false})),
            ..Default::default()
        })
    }

    pub(crate) async fn normalize_transport_item(
        &self,
        input: &mut TransferPromiseInput,
        creator: &str,
        preserve_hidden: bool,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let Some(item) = &mut input.item else {
            return Ok(());
        };
        let Some(transport) = &mut item.transport else {
            return Ok(());
        };
        if !preserve_hidden {
            transport.origin = self.resolve(&transport.origin).await?;
            transport.destination = self.resolve(&transport.destination).await?;
            for record in [&transport.origin, &transport.destination] {
                let location = self.saved_location_of(record, actor).await?;
                if location.unavailable || location.selected.is_none() {
                    return Err(invalid(
                        "Select an available saved location or #address for each endpoint",
                    ));
                }
            }
        }
        transport.validate().map_err(invalid)?;
        if input.delta.abs() != f64::from(transport.count)
            || input.delta > 0.0 && item.exchange.is_none()
        {
            return Err(invalid(
                "Transport quantity must match its passenger or delivery count",
            ));
        }
        match transport.direction {
            Direction::Offer
                if item
                    .exchange
                    .as_ref()
                    .map(|route| route.giver.as_str())
                    .or(input.party.as_deref())
                    != Some(creator) =>
            {
                return Err(invalid("The offering Person must provide this transport"));
            }
            Direction::Need
                if item
                    .exchange
                    .as_ref()
                    .is_some_and(|route| route.receiver != creator) =>
            {
                return Err(invalid("The requesting Person must receive this transport"));
            }
            _ => {}
        }
        let from = input
            .window_start
            .as_deref()
            .ok_or_else(|| invalid("Choose the pickup-window start"))?;
        let until = input
            .window_end
            .as_deref()
            .ok_or_else(|| invalid("Choose the pickup-window end"))?;
        let from = chrono::DateTime::parse_from_rfc3339(from).map_err(invalid)?;
        let until = chrono::DateTime::parse_from_rfc3339(until).map_err(invalid)?;
        if from >= until {
            return Err(invalid("Pickup-window end must be after its start"));
        }
        input.reserve_from = Some(crate::actions::TransferReservePoint::Agreed);
        Ok(())
    }
}
