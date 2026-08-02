//! Communication sand actions (S3 of `notes/institute/Communication.md`).
//!
//! The typed write surface for conversations-with-rooms. Everything here is an
//! `impl Engine` method so it terminates in the same `append()`/annotate
//! provenance path as every other Action. The dispatch arms and the `Action`
//! enum variants that call these live in `actions.rs` (see the S3 snippets in
//! Communication.md — they are kept there while `actions.rs` is mid-refactor by
//! the transfer work, to avoid colliding with it).
//!
//! Scope of S3: conversation creation, controller binding, the server-side
//! room state machine (idle → active on first join, active → idle on last
//! leave/close), recording start/stop flags, and participant-only join
//! authorization. NO call-intent / Karma machinery — that is the Far-Future
//! part (F2). NO media — that starts at S7 on the browser side.

use chrono::{DateTime, Utc};
use nucleus::{Cause, NewFact, RecordKind};
use serde_json::json;

use crate::Engine;
use crate::actions::ActionOutcome;
use crate::error::EngineError;
use store::communication as comm;

/// A participant reference as it arrives from the sand: a local person/user
/// Record token (slug or uid) or a remote-organ identity. Cross-organ identity
/// resolution rides the organ network (SD-open: which organ is signaling
/// authority); for now a remote ref is stored as a participant link the same
/// way, and the media/signaling layer (S7+) enforces reachability.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ParticipantRef {
    /// Person/user Record token to link as a `participant`.
    pub person: String,
    /// Origin organ of that person, when it is not this cell. Advisory at S3.
    #[serde(default)]
    pub organ: Option<String>,
}

impl Engine {
    /// `conversation-create`: create the conversation Record, tag it (default
    /// `@communication`), and link every participant. Returns the new Record's
    /// uid in `outcome.created`. Groups are linked `group-of`; membership
    /// expansion for room authorization happens at join time.
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

        // Tag for discovery (`@communication` unless the sand chose another).
        let tag = comm::ensure_tag_record(&self.store.pool, tag_slug).await?;
        comm::tag(&self.store.pool, &conversation.uid, &tag.uid).await?;

        // Link participants (resolved to their Record uids) and groups.
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

        // Seed the room extension (idle, no controller, manual recording).
        comm::set_ext(
            &self.store.pool,
            &conversation.uid,
            &comm::CommunicationExt {
                room_id: conversation.uid.clone(),
                ..Default::default()
            },
        )
        .await?;

        // Activate the Record and record provenance.
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

    /// `room-bind-controller`: bind a widget instance as the conversation's
    /// room controller (the single sand that drives room lifecycle and, later,
    /// claims Karma intents in F2).
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

    /// `room-join`: authorize the actor as a participant, then run the room
    /// state machine. First join of an idle room opens a `call_session`
    /// (idle → active); subsequent joins just extend the occupant mirror and
    /// bump the peak participant count.
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

        // First join opens the session (state machine: idle -> active).
        let session_uid = match (ext.room.state.as_str(), &ext.room.session_record_id) {
            ("active", Some(existing)) => existing.clone(),
            _ => {
                let session =
                    comm::open_session(&self.store.pool, &conversation_uid, media).await?;
                // open_session re-reads/writes the ext; refresh our copy.
                ext = comm::get_ext(&self.store.pool, &conversation_uid)
                    .await?
                    .unwrap_or_default();
                session.uid
            }
        };

        // Extend the advisory occupant mirror.
        let occupant = actor.clone().unwrap_or_else(|| "anonymous".into());
        if !ext.room.occupants.contains(&occupant) {
            ext.room.occupants.push(occupant);
        }
        comm::set_ext(&self.store.pool, &conversation_uid, &ext).await?;

        // Track the peak occupancy on the session sidecar.
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

    /// `room-leave`: remove the actor from the occupant mirror; the last leave
    /// closes the session (active → idle).
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
            // Last one out: close the session.
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

    /// `room-close`: force the room idle regardless of occupants (controller
    /// or Karma consequence in F2). Closes any open session.
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

    /// `recording-start` / `recording-stop`: flip the current session's
    /// recording state. The artifact flow (resource ref → `call-recording`
    /// link → thread message) is S11; this only moves the visible flag so all
    /// occupants can see a recording is running.
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

    /// Authorization: the actor must be a linked participant of the
    /// conversation (or a member of a linked group). Local-no-auth mode
    /// (`actor == None`) is allowed, matching the rest of the engine's
    /// single-user path. Cross-organ membership resolution is SD-open and is
    /// enforced again at the signaling layer (S7+).
    ///
    /// The `actor` string is an `app_user.id` (the fact `actor_uid`
    /// namespace), which is DISTINCT from the Ledger's Person record uids that
    /// participant links point at. It must be resolved app_user → Person before
    /// comparison — the same resolution `require_transfer_thread_writer` does
    /// via `actor_person`. Comparing the raw actor id against Person uids would
    /// reject every real participant.
    async fn require_communication_participant(
        &self,
        conversation_uid: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let Some(person_uid) = self.comm_actor_person(actor).await? else {
            return Ok(()); // no-auth mode: single user, always authorized
        };
        // A participant link whose target IS this actor's Person record.
        let participants = comm::participants(&self.store.pool, conversation_uid).await?;
        if participants.iter().any(|p| p.uid == person_uid) {
            return Ok(());
        }
        // Group membership: any linked group whose members include this Person.
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

    /// Resolve an `app_user.id` actor string to its Person Record uid, or
    /// `None` for the local no-auth Cell. A local twin of the private
    /// `Engine::actor_person`, built on the public `store::auth` resolvers.
    async fn comm_actor_person(&self, actor: Option<&str>) -> Result<Option<String>, EngineError> {
        let Some(actor) = actor else {
            return Ok(None);
        };
        let user_id: i64 = actor
            .parse()
            .map_err(|_| EngineError::Consequence("unrecognized actor".into()))?;
        let user = store::auth::user_by_id(&self.store.pool, user_id)
            .await?
            .ok_or_else(|| EngineError::Consequence("unrecognized actor".into()))?;
        store::auth::person_for_user(&self.store.pool, user.id)
            .await?
            .map(Some)
            .ok_or_else(|| {
                EngineError::Consequence(
                    "authenticated user has no assigned person identity".into(),
                )
            })
    }
}

impl Engine {
    /// Resolve a slug/uid token to a record uid, erroring if unknown. A local
    /// twin of the private `Engine::resolve` in `actions.rs` (that method is
    /// not visible across modules and its file is mid-refactor), built on the
    /// public store resolver.
    async fn comm_resolve(&self, token: &str) -> Result<String, EngineError> {
        store::records::resolve(&self.store.pool, token)
            .await?
            .map(|r| r.uid)
            .ok_or_else(|| EngineError::UnknownRecord(token.to_string()))
    }

    /// Append a zero-delta annotation fact (provenance) on a record. A local
    /// twin of the private `Engine::annotate`, built on the public `append`.
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
