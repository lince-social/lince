use chrono::{DateTime, Utc};
use nucleus::{Cause, NewFact, RecordKind};
use serde_json::json;

use crate::Engine;
use crate::actions::ActionOutcome;
use crate::error::EngineError;
use store::communication as comm;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ParticipantRef {
    pub person: String,
    #[serde(default)]
    pub organ: Option<String>,
}

impl Engine {
    pub(crate) async fn communication_create(
        &self,
        head: &str,
        tag_slug: &str,
        participants: &[ParticipantRef],
        groups: &[String],
        actor: Option<String>,
        now: DateTime<Utc>,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        let head = head.trim();
        if head.is_empty() {
            return Err(EngineError::Consequence(
                "conversation title cannot be empty".into(),
            ));
        }
        let conversation = store::records::create(
            &self.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head,
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await?;

        let tag = comm::ensure_tag_record(&self.store.pool, tag_slug).await?;
        comm::tag(&self.store.pool, &conversation.uid, &tag.uid).await?;

        for participant in participants {
            let person_uid = self.comm_resolve(&participant.person).await?;
            comm::add_participant(&self.store.pool, &conversation.uid, &person_uid).await?;
        }
        if !groups.is_empty() {
            let group_of = store::concepts::ensure(&self.store.pool, comm::KIND_GROUP_OF).await?;
            for group in groups {
                let group_uid = self.comm_resolve(group).await?;
                store::assertions::assert(
                    &self.store.pool,
                    store::assertions::NewAssertion {
                        subject_uid: &conversation.uid,
                        predicate_uid: &group_of,
                        object_uid: Some(&group_uid),
                        role: store::assertions::AssertionRole::Ordinary,
                        quantity: None,
                        unit_uid: None,
                        asserted_by: actor.as_deref(),
                    },
                )
                .await?;
            }
        }

        comm::set_ext(
            &self.store.pool,
            &conversation.uid,
            &comm::CommunicationExt {
                room_id: conversation.uid.clone(),
                ..Default::default()
            },
        )
        .await?;

        outcome.facts = self
            .append(
                NewFact {
                    actor_uid: actor.clone(),
                    ..NewFact::quantity(
                        conversation.uid.clone(),
                        store::exact::one(),
                        Cause::user_edit(),
                    )
                },
                now,
            )
            .await?;
        outcome.facts.extend(
            self.comm_annotate(
                conversation.uid.clone(),
                actor,
                json!({ "communication": { "created": conversation.uid, "tag": tag_slug } }),
                now,
            )
            .await?,
        );
        outcome.created = Some(conversation.uid);
        Ok(())
    }

    pub(crate) async fn communication_bind_controller(
        &self,
        conversation: &str,
        controller_sand_uid: &str,
        actor: Option<String>,
        now: DateTime<Utc>,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        let conversation_uid = self.comm_resolve(conversation).await?;
        let mut ext = comm::get_ext(&self.store.pool, &conversation_uid)
            .await?
            .unwrap_or_default();
        ext.controller_sand_uid = Some(controller_sand_uid.to_string());
        comm::set_ext(&self.store.pool, &conversation_uid, &ext).await?;
        outcome.facts = self
            .comm_annotate(
                conversation_uid,
                actor,
                json!({ "communication": { "controller": controller_sand_uid } }),
                now,
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn communication_join(
        &self,
        conversation: &str,
        media: &str,
        actor: Option<String>,
        now: DateTime<Utc>,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        let conversation_uid = self.comm_resolve(conversation).await?;
        self.require_communication_participant(&conversation_uid, actor.as_deref())
            .await?;

        let mut ext = comm::get_ext(&self.store.pool, &conversation_uid)
            .await?
            .unwrap_or_default();

        let session_uid = match (ext.room.state.as_str(), &ext.room.session_record_id) {
            ("active", Some(existing)) => existing.clone(),
            _ => {
                let session =
                    comm::open_session(&self.store.pool, &conversation_uid, media).await?;
                ext = comm::get_ext(&self.store.pool, &conversation_uid)
                    .await?
                    .unwrap_or_default();
                session.uid
            }
        };

        let occupant = actor.clone().unwrap_or_else(|| "anonymous".into());
        if !ext.room.occupants.contains(&occupant) {
            ext.room.occupants.push(occupant);
        }
        comm::set_ext(&self.store.pool, &conversation_uid, &ext).await?;

        if let Some(mut sidecar) = comm::get_session(&self.store.pool, &session_uid).await? {
            let live = ext.room.occupants.len() as i64;
            if live > sidecar.peak_participants {
                sidecar.peak_participants = live;
                let fds = serde_json::to_value(&sidecar).map_err(serde_err)?;
                store::records::set_extension(
                    &self.store.pool,
                    &session_uid,
                    comm::SESSION_NAMESPACE,
                    &fds,
                )
                .await?;
            }
        }

        outcome.facts = self
            .comm_annotate(
                conversation_uid,
                actor,
                json!({ "communication": { "joined": { "session": session_uid, "media": media } } }),
                now,
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn communication_leave(
        &self,
        conversation: &str,
        actor: Option<String>,
        now: DateTime<Utc>,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        let conversation_uid = self.comm_resolve(conversation).await?;
        let mut ext = comm::get_ext(&self.store.pool, &conversation_uid)
            .await?
            .unwrap_or_default();

        let occupant = actor.clone().unwrap_or_else(|| "anonymous".into());
        ext.room.occupants.retain(|o| o != &occupant);

        if ext.room.occupants.is_empty() {
            if let Some(session_uid) = ext.room.session_record_id.clone() {
                let peak = comm::get_session(&self.store.pool, &session_uid)
                    .await?
                    .map(|s| s.peak_participants)
                    .unwrap_or(0);
                comm::close_session(&self.store.pool, &conversation_uid, &session_uid, peak)
                    .await?;
            }
        } else {
            comm::set_ext(&self.store.pool, &conversation_uid, &ext).await?;
        }

        outcome.facts = self
            .comm_annotate(
                conversation_uid,
                actor,
                json!({ "communication": { "left": occupant } }),
                now,
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn communication_close(
        &self,
        conversation: &str,
        actor: Option<String>,
        now: DateTime<Utc>,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        let conversation_uid = self.comm_resolve(conversation).await?;
        if let Some(ext) = comm::get_ext(&self.store.pool, &conversation_uid).await? {
            if let Some(session_uid) = ext.room.session_record_id.clone() {
                let peak = comm::get_session(&self.store.pool, &session_uid)
                    .await?
                    .map(|s| s.peak_participants)
                    .unwrap_or(0);
                comm::close_session(&self.store.pool, &conversation_uid, &session_uid, peak)
                    .await?;
            }
        }
        outcome.facts = self
            .comm_annotate(
                conversation_uid,
                actor,
                json!({ "communication": { "closed": true } }),
                now,
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn communication_recording(
        &self,
        conversation: &str,
        start: bool,
        actor: Option<String>,
        now: DateTime<Utc>,
        outcome: &mut ActionOutcome,
    ) -> Result<(), EngineError> {
        let conversation_uid = self.comm_resolve(conversation).await?;
        let ext = comm::get_ext(&self.store.pool, &conversation_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("conversation has no room".into()))?;
        if ext.recording_policy == "disabled" {
            return Err(EngineError::Consequence(
                "recording is disabled for this conversation".into(),
            ));
        }
        let session_uid = ext
            .room
            .session_record_id
            .ok_or_else(|| EngineError::Consequence("no active session to record".into()))?;
        let mut sidecar = comm::get_session(&self.store.pool, &session_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("session sidecar missing".into()))?;
        sidecar.recording.state = if start { "recording" } else { "processing" }.into();
        let fds = serde_json::to_value(&sidecar).map_err(serde_err)?;
        store::records::set_extension(
            &self.store.pool,
            &session_uid,
            comm::SESSION_NAMESPACE,
            &fds,
        )
        .await?;
        outcome.facts = self
            .comm_annotate(
                conversation_uid,
                actor,
                json!({ "communication": { "recording": sidecar.recording.state } }),
                now,
            )
            .await?;
        Ok(())
    }

    async fn require_communication_participant(
        &self,
        conversation_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let Some(person_uid) = self.comm_actor_person(actor).await? else {
            return Ok(());
        };
        let participants = comm::participants(&self.store.pool, conversation_uid).await?;
        if participants.iter().any(|p| p.uid == person_uid) {
            return Ok(());
        }
        if let Some(group_of) =
            store::concepts::resolve(&self.store.pool, comm::KIND_GROUP_OF).await?
        {
            if let Some(member_of) = store::concepts::resolve(&self.store.pool, "member-of").await?
            {
                for group in store::assertions::objects_from_subject(
                    &self.store.pool,
                    conversation_uid,
                    &group_of,
                )
                .await?
                {
                    let members = store::assertions::subjects_pointing_to(
                        &self.store.pool,
                        &member_of,
                        &group.uid,
                    )
                    .await?;
                    if members.iter().any(|m| m.uid == person_uid) {
                        return Ok(());
                    }
                }
            }
        }
        Err(EngineError::Consequence(
            "not a participant of this conversation".into(),
        ))
    }

    async fn comm_actor_person(&self, actor: Option<&str>) -> Result<Option<String>, EngineError> {
        Ok(actor.map(str::to_string))
    }
}

impl Engine {
    async fn comm_resolve(&self, token: &str) -> Result<String, EngineError> {
        store::records::resolve(&self.store.pool, token)
            .await?
            .map(|r| r.uid)
            .ok_or_else(|| EngineError::UnknownRecord(token.to_string()))
    }

    async fn comm_annotate(
        &self,
        record_uid: String,
        actor: Option<String>,
        payload: serde_json::Value,
        now: DateTime<Utc>,
    ) -> Result<Vec<nucleus::Fact>, EngineError> {
        self.append(
            NewFact {
                uid: None,
                record_uid,
                delta: nucleus::fact::zero_delta(),
                at: None,
                actor_uid: actor,
                cause: Cause::user_edit(),
                payload: Some(payload.to_string()),
            },
            now,
        )
        .await
    }
}

fn serde_err(e: serde_json::Error) -> EngineError {
    EngineError::Consequence(format!("serialize communication sidecar: {e}"))
}
