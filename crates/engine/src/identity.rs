use crate::{Engine, EngineError, actions::ActionOutcome};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use nucleus::identity::{Command, PersonKey};
use serde_json::json;
mod roots;
pub use roots::sign_organ_root_proof;

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code: "identity_key_review_invalid",
        message: message.into(),
    }
}

fn validate(key: &PersonKey) -> Result<(), EngineError> {
    if key.public_key.len() != 44 {
        return Err(invalid("Use a canonical 44-character Ed25519 public key"));
    }
    let bytes = STANDARD
        .decode(&key.public_key)
        .map_err(|_| invalid("Use a standard base64 Ed25519 public key"))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| invalid("An Ed25519 public key contains 32 bytes"))?;
    if !nucleus::valid_uid(&key.subject, "r")
        || key.key_id.is_empty()
        || key.key_id.len() > 160
        || !key.key_id.bytes().all(|value| value.is_ascii_graphic())
        || STANDARD.encode(bytes) != key.public_key
        || !ed25519_dalek::VerifyingKey::from_bytes(&bytes).is_ok_and(|key| !key.is_weak())
    {
        return Err(invalid(
            "Choose a Person identity, a bounded key identifier and canonical Ed25519 public key",
        ));
    }
    Ok(())
}

impl Engine {
    pub async fn identity_command(
        &self,
        request: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.identity_command_with_authorship(request,actor,now,None).await
    }

    pub(crate) async fn identity_command_with_authorship(
        &self,
        request:Command,
        actor:Option<&str>,
        now:DateTime<Utc>,
        authorship:Option<&crate::actions::VerifiedActionAuthorship>,
    )->Result<ActionOutcome,EngineError> {
        if matches!(request,Command::OrganRoots {..}|Command::ExportOrganRoot {..}|Command::PreviewOrganRoot {..}|Command::AcceptOrganRoot {..}) {
            return self.organ_root_command(&request,actor,now,authorship).await;
        }
        self.require_permission(actor, request.permission()).await?;
        let subject = self.resolve(request.subject()).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&subject))
            .await?;
        let profile = store::records::get(&self.store.pool, &subject)
            .await?
            .ok_or_else(|| {
                invalid("Receive the Person profile Record before recognizing its signing key")
            })?;
        if profile.kind != "person" {
            return Err(invalid(
                "Choose a Person profile; this action does not create profiles or change Organ membership",
            ));
        }
        let revision =
            store::record_revisions::get_on(&mut *self.store.pool.acquire().await?, &subject)
                .await?
                .revision;
        let value = match &request {
            Command::OrganRoots {..}|Command::ExportOrganRoot {..}|Command::PreviewOrganRoot {..}|Command::AcceptOrganRoot {..} => unreachable!(),
            Command::PersonKeys { .. } => {
                let mut keys: Vec<(String,String)> = store::sqlx::query_as("SELECT key_id,public_key FROM identity_key WHERE actor_uid=? ORDER BY key_id LIMIT 33").bind(&subject).fetch_all(&self.store.pool).await?;
                let partial = keys.len() > 32;
                keys.truncate(32);
                let mut recognized = Vec::new();
                for (key_id, public_key) in keys {
                    let key = PersonKey {
                        subject: subject.clone(),
                        key_id,
                        public_key,
                    };
                    let revoked =
                        store::roster::is_revoked(&self.store.pool, &subject, &key.public_key)
                            .await?;
                    recognized.push(json!({"key":key,"fingerprint":key.fingerprint().map_err(invalid)?,"revoked":revoked}));
                }
                json!({"kind":"person_keys","subject":subject,"profile_revision":revision,"keys":recognized,"partial":partial,"checked_at":now.timestamp()})
            }
            Command::PreviewPersonKey { key } | Command::AcceptPersonKey { key, .. } => {
                validate(key)?;
                if key.subject != subject {
                    return Err(invalid(
                        "Use the canonical Person Record UID for key comparison",
                    ));
                }
                let fingerprint = key.fingerprint().map_err(invalid)?;
                let review_hash = key.review_hash(revision).map_err(invalid)?;
                if store::roster::is_revoked(&self.store.pool, &subject, &key.public_key).await? {
                    return Err(invalid("This signing key has a known revocation"));
                }
                if let Some(known) =
                    crate::trust::key_of(&self.store, &subject, &key.key_id).await?
                    && known != key.public_key
                {
                    return Err(invalid(
                        "This key identifier already names different key material; review a new identifier for rotation",
                    ));
                }
                if let Command::AcceptPersonKey {
                    expected_profile_revision,
                    expected_review,
                    ..
                } = &request
                {
                    if *expected_profile_revision != revision || expected_review != &review_hash {
                        return Err(invalid(
                            "The reviewed Person or key changed; compare a fresh preview before accepting",
                        ));
                    }
                    let mut tx = store::write_tx(&self.store.pool).await?;
                    self.require_permission_on(&mut tx, actor, "organ:update")
                        .await?;
                    let graph =
                        crate::access::policy_graph_on(&mut tx, std::slice::from_ref(&subject))
                            .await
                            .map_err(|error| invalid(error.to_string()))?;
                    if !self
                        .readable_on(&mut tx, actor, &graph)
                        .await?
                        .contains(&subject)
                    {
                        return Err(EngineError::Forbidden(
                            "The Person profile is no longer readable".into(),
                        ));
                    }
                    store::record_revisions::check_expected_on(&mut tx, &subject, revision)
                        .await
                        .map_err(|_| invalid("The Person profile changed; review its key again"))?;
                    let kind: Option<String> = store::sqlx::query_scalar(
                        "SELECT kind FROM record WHERE uid=? AND deleted_at IS NULL",
                    )
                    .bind(&subject)
                    .fetch_optional(&mut *tx)
                    .await?;
                    let revoked:bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)").bind(&subject).bind(&key.public_key).fetch_one(&mut *tx).await?;
                    if kind.as_deref() != Some("person") || revoked {
                        return Err(invalid(
                            "The Person identity or key authority changed; review again",
                        ));
                    }
                    crate::trust::adopt_key_on(&mut tx, &subject, &key.key_id, &key.public_key)
                        .await?;
                    tx.commit().await?;
                }
                json!({"kind":if request.is_mutation() {"person_key_accepted"} else {"person_key_preview"},"key":key,"profile_revision":revision,"fingerprint":fingerprint,"review_hash":review_hash,"checked_at":now.timestamp(),"meaning":"Recognizing this signing identity does not establish reliability, membership authority or physical delivery."})
            }
        };
        Ok(ActionOutcome {
            data: Some(json!({"identity":value})),
            ..Default::default()
        })
    }
}
