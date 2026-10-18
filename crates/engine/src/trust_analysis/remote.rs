use super::*;
use crate::actions::ActionOutcome;
use chrono::{DateTime, Utc};
use nucleus::karma::CanonicalHash;
use nucleus::trust_analysis::{
    remote::{self as model, Command, Draft, Endpoint, Review, State, Verification},
    serving::Artifact,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};

mod commands;
mod merge;
mod worker;

pub(crate) struct Runtime {
    jobs: Mutex<BTreeMap<String, Job>>,
    reviews: Mutex<BTreeMap<String, Issued>>,
    worker: Mutex<()>,
    slots: Semaphore,
    #[cfg(feature = "trust-test-hooks")]
    verification_checkpoint: Mutex<Option<(String, Arc<VerificationCheckpoint>)>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            jobs: Mutex::new(BTreeMap::new()),
            reviews: Mutex::new(BTreeMap::new()),
            worker: Mutex::new(()),
            slots: Semaphore::new(4),
            #[cfg(feature = "trust-test-hooks")]
            verification_checkpoint: Mutex::new(None),
        }
    }
}

#[cfg(feature = "trust-test-hooks")]
#[derive(Default)]
pub struct VerificationCheckpoint {
    entered: tokio::sync::Notify,
    resumed: tokio::sync::Notify,
}

#[cfg(feature = "trust-test-hooks")]
impl VerificationCheckpoint {
    pub async fn reached(&self) {
        self.entered.notified().await;
    }

    pub fn release(&self) {
        self.resumed.notify_one();
    }
}

#[cfg(feature = "trust-test-hooks")]
impl Engine {
    pub async fn expire_trust_query_for_test(&self, job: &str) -> Result<(), EngineError> {
        let mut jobs = self.trust_remote.jobs.lock().await;
        let job = jobs.get_mut(job).ok_or_else(|| invalid("Choose an existing reader job for expiry"))?;
        job.deadline = Instant::now() - Duration::from_secs(1);
        job.expire();
        Ok(())
    }

    pub async fn pause_trust_verification_for_test(&self, job: &str) -> Result<Arc<VerificationCheckpoint>, EngineError> {
        if !self.trust_remote.jobs.lock().await.contains_key(job) {
            return Err(invalid("Choose an existing reader job for this test checkpoint"));
        }
        let mut held = self.trust_remote.verification_checkpoint.lock().await;
        if held.is_some() { return Err(invalid("A verification test checkpoint is already armed")); }
        let checkpoint = Arc::new(VerificationCheckpoint::default());
        *held = Some((job.into(), checkpoint.clone()));
        Ok(checkpoint)
    }
}

struct Issued {
    review: Review,
    actor: Option<String>,
    expires: Instant,
}
#[derive(Clone)]
struct Found {
    artifact: Arc<Artifact>,
    sources: BTreeSet<String>,
    signature: Verification,
    checked_at: i64,
    expires_at: i64,
    bytes: usize,
}
struct Job {
    review: Arc<Review>,
    actor: Option<String>,
    request_id: String,
    state: State,
    created: Instant,
    deadline: Instant,
    lease: Instant,
    progress: Vec<Progress>,
    artifacts: BTreeMap<CanonicalHash, Found>,
    bytes: usize,
    sent: usize,
    spent: usize,
    reserved: usize,
    requests: usize,
    events: usize,
    identities: BTreeSet<Identity>,
    partial: bool,
}
#[derive(Clone, serde::Serialize)]
struct Progress {
    organ: String,
    endpoint: String,
    state: String,
    questions: usize,
    documents: usize,
    error: Option<String>,
}

impl Job {
    fn active(&self) -> bool {
        matches!(self.state, State::Queued | State::Running)
            && Instant::now() < self.deadline
            && Instant::now() < self.lease
    }
    fn expire(&mut self) {
        if matches!(self.state, State::Queued | State::Running) && !self.active() {
            self.state = State::Expired;
            self.partial = true;
            self.reserved = 0;
        }
    }
    fn stop(&mut self) {
        self.state = State::Cancelled;
        self.partial = true;
        self.artifacts.clear();
        self.reserved = 0;
    }
}

fn packet_profiles(artifact: &Artifact) -> Vec<Identity> {
    match artifact {
        Artifact::Contact(document) => vec![
            document.statement.issuer.clone(),
            document.statement.contact.clone(),
        ],
        Artifact::Claim(document) => vec![
            document.statement.issuer.clone(),
            document.statement.descriptor.subject.clone(),
        ],
        Artifact::Receipt(document) => document
            .public_bundle()
            .map(|bundle| {
                let scope = &bundle.publication.scope;
                [&scope.source.origin, &scope.giver, &scope.receiver]
                    .into_iter()
                    .map(|uid| Identity::Organ(uid.clone()))
                    .chain(
                        scope
                            .public_persons
                            .iter()
                            .map(|person| Identity::Person(person.person.clone())),
                    )
                    .collect()
            })
            .unwrap_or_default(),
    }
}

impl Engine {
    async fn remote_stamp(
        &self,
        artifact: &Artifact,
    ) -> Result<super::serving::AuthorityStamp, EngineError> {
        super::serving::authority_stamp(&mut *self.store.pool.acquire().await?, artifact).await
    }
    async fn remote_endpoints(
        &self,
        draft: &Draft,
        actor: Option<&str>,
    ) -> Result<Vec<Endpoint>, EngineError> {
        draft.questions().map_err(invalid)?;
        let settings = self.social_ask_settings().await?;
        if !settings.enabled {
            return Err(invalid(
                "Enable contact queries on this Cell before asking around",
            ));
        }
        let mut endpoints = Vec::new();
        let mut tx = self.store.pool.begin().await?;
        for choice in settings
            .peers
            .iter()
            .filter(|peer| peer.ask)
            .take(model::MAX_PEERS)
        {
            let contact:Option<String>=store::sqlx::query_scalar("SELECT node_id FROM organ_contact c JOIN record r ON r.uid=c.record_uid WHERE c.record_uid=? AND c.trust='known' AND c.unreachable_since IS NULL AND r.deleted_at IS NULL").bind(&choice.organ).fetch_optional(&mut *tx).await?.flatten();
            let Some(endpoint) =
                contact.filter(|endpoint| endpoint.parse::<iroh::EndpointId>().is_ok())
            else {
                continue;
            };
            let identity = Identity::Organ(choice.organ.clone());
            let context = self
                .contact_identity_context_on(&mut tx, &identity, actor)
                .await?;
            if context.readable
                && !context.blocked
                && !draft.query.exclude_everywhere.contains(&identity)
                && draft.query.endpoints.allows(&identity, &context)
            {
                let label =
                    store::sqlx::query_scalar("SELECT substr(head,1,160) FROM record WHERE uid=?")
                        .bind(&choice.organ)
                        .fetch_one(&mut *tx)
                        .await?;
                endpoints.push(Endpoint {
                    organ: choice.organ.clone(),
                    endpoint,
                    label,
                    profile_revision: store::record_revisions::get_on(&mut tx, &choice.organ)
                        .await?
                        .revision,
                });
            }
        }
        endpoints.sort_by(|a, b| a.organ.cmp(&b.organ));
        Ok(endpoints)
    }

    async fn remote_access(
        &self,
        review: &Review,
        actor: Option<&str>,
        peer: Option<&Endpoint>,
    ) -> Result<(), EngineError> {
        let mut tx = self.store.pool.begin().await?;
        self.remote_access_on(&mut tx, review, actor, peer).await
    }

    async fn remote_access_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        review: &Review,
        actor: Option<&str>,
        peer: Option<&Endpoint>,
    ) -> Result<(), EngineError> {
        self.require_permission_on(tx, actor, "record:read").await?;
        if !store::people::is_active_on(tx, &review.person).await? {
            return Err(invalid("This query's reader is no longer active"));
        }
        self.social_require_local_write_on(tx).await?;
        let reader = self.contact_reader_on(tx, &review.person, actor, review.draft.as_organ)
            .await?;
        for selected in &review.draft.linked_claims {
            if let Some(document) = &selected.snapshot {
                if !document.statement.audience.allows(&reader, &document.statement.issuer) {
                    return Err(invalid("The reviewed claim does not grant this reader access"));
                }
                self.authenticate_profile_claim_on(tx, document, nucleus::execution::now().timestamp()).await?;
            }
        }
        self.trust_profiles_on(
            tx,
            review
                .profiles
                .iter()
                .map(|profile| profile.identity.clone())
                .collect(),
            actor,
        )
        .await?;
        let peers: Vec<_> = peer
            .map(|peer| vec![peer])
            .unwrap_or_else(|| review.endpoints.iter().collect());
        for peer in peers {
            self.social_ask_peer_on(tx, &peer.endpoint, &peer.organ, true)
                .await?;
            let identity = Identity::Organ(peer.organ.clone());
            let context = self
                .contact_identity_context_on(tx, &identity, actor)
                .await?;
            if !context.readable
                || context.blocked
                || review.draft.query.exclude_everywhere.contains(&identity)
                || !review.draft.query.endpoints.allows(&identity, &context)
            {
                return Err(invalid("A reviewed query endpoint is no longer permitted"));
            }
        }
        Ok(())
    }

    async fn remote_packet_access(
        &self,
        review: &Review,
        actor: Option<&str>,
        artifact: &Artifact,
    ) -> Result<(), EngineError> {
        let mut tx = self.store.pool.begin().await?;
        self.remote_packet_access_on(&mut tx, review, actor, artifact)
            .await
    }

    async fn remote_packet_access_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        review: &Review,
        actor: Option<&str>,
        artifact: &Artifact,
    ) -> Result<(), EngineError> {
        self.require_permission_on(tx, actor, "record:read").await?;
        let reader = self
            .contact_reader_on(tx, &review.person, actor, review.draft.as_organ)
            .await?;
        if !artifact.allows_reader(&reader) {
            return Err(invalid(
                "The original artifact does not grant this reader access",
            ));
        }
        for identity in packet_profiles(artifact) {
            let exists: bool =
                store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=?)")
                    .bind(identity.uid())
                    .fetch_one(&mut **tx)
                    .await?;
            if exists {
                let context = self
                    .contact_identity_context_on(tx, &identity, actor)
                    .await?;
                if !context.readable || context.blocked {
                    return Err(invalid("An evidence profile is no longer readable"));
                }
            }
        }
        Ok(())
    }

    pub(super) async fn remote_signature(&self, artifact: &Artifact, now: DateTime<Utc>) -> Verification {
        let signature_valid = match artifact {
            Artifact::Contact(document) => document.signing_bytes().is_ok_and(|payload| {
                crate::trust::verify_bytes_strict(
                    &document.public_key,
                    &payload,
                    &document.signature,
                )
            }),
            Artifact::Claim(document) => {
                document.signing_bytes().is_ok_and(|payload| {
                    crate::trust::verify_bytes_strict(
                        &document.public_key,
                        &payload,
                        &document.signature,
                    )
                }) && match &document.statement.authorship {
                    nucleus::trust_analysis::claims::Authorship::Generated { permit, .. } => {
                        permit.signing_bytes().is_ok_and(|payload| {
                            crate::trust::verify_bytes_strict(
                                &permit.public_key,
                                &payload,
                                &permit.signature,
                            )
                        })
                    }
                    _ => true,
                }
            }
            Artifact::Receipt(_) => true,
        };
        if !signature_valid {
            return Verification::Invalid;
        }
        let result = match artifact {
            Artifact::Contact(document) => {
                self.authenticate_contact_disclosure(document, now.timestamp())
                    .await
            }
            Artifact::Claim(document) => {
                self.authenticate_profile_claim(document, now.timestamp())
                    .await
            }
            Artifact::Receipt(document) => self
                .verify_transfer_receipt_document(document, now)
                .await
                .map(|_| IdentityAuthenticity::Accepted),
        };
        match result {
            Ok(IdentityAuthenticity::Accepted) => Verification::Accepted,
            Ok(IdentityAuthenticity::WithheldAuthority) => Verification::WithheldAuthority,
            Err(error)
                if matches!(
                    error.code(),
                    Some(
                        "contact_disclosure_unknown_identity"
                            | "profile_claim_unknown_identity"
                            | "transfer_receipt_unknown_identity"
                    )
                ) =>
            {
                Verification::UnknownIdentity
            }
            Err(error) if error.code() == Some("transfer_receipt_authority_withheld") => {
                Verification::WithheldAuthority
            }
            Err(error) if error.code() == Some("transfer_receipt_incomplete") => {
                Verification::Incomplete
            }
            Err(_) => Verification::Invalid,
        }
    }

    async fn remote_active(&self, id: &str) -> bool {
        let mut jobs = self.trust_remote.jobs.lock().await;
        let Some(job) = jobs.get_mut(id) else {
            return false;
        };
        job.expire();
        job.active()
    }
}
