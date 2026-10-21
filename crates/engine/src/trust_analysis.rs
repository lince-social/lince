mod matching;
use crate::{Engine, EngineError, trust::Signer};
use nucleus::trust_analysis::{Identity, contacts::Document};
use std::collections::BTreeSet;
mod claim_snapshots;
mod claims;
mod commands;
pub(crate) mod custody;
mod loading;
mod queries;
pub(crate) mod remote;
mod serving;
mod summaries;
pub use claims::sign_claim_document;
#[cfg(feature = "trust-test-hooks")]
pub use remote::VerificationCheckpoint;
pub use summaries::sign_recipe_permit;

pub fn prepare_contact_action(
    action: &mut crate::actions::Action,
    signer: &Signer,
) -> Result<(), EngineError> {
    use nucleus::trust_analysis::commands::Command;
    let crate::actions::Action::TrustAnalysis {
        person, request, ..
    } = action
    else {
        return Ok(());
    };
    match request {
        Command::Claims { request } => {
            return claims::prepare_claim_action(person.as_deref(), request, signer);
        }
        Command::PreviewContact { draft } if matches!(draft.issuer, Identity::Person(_)) => {
            if draft.issuer.uid() != signer.actor_uid
                || person
                    .as_ref()
                    .is_some_and(|person| person != &signer.actor_uid)
            {
                return Err(EngineError::Forbidden(
                    "Use your own Person signing identity".into(),
                ));
            }
            draft.key_id = signer.key_id.clone();
            draft.public_key = signer.public_key_b64();
        }
        Command::PublishContact { review, .. }
            if matches!(review.document.statement.issuer, Identity::Person(_)) =>
        {
            if review.person != signer.actor_uid
                || person
                    .as_ref()
                    .is_some_and(|person| person != &signer.actor_uid)
            {
                return Err(EngineError::Forbidden(
                    "Publish only your own Person disclosure".into(),
                ));
            }
            sign_contact_document(&mut review.document, signer)?;
        }
        _ => {}
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code: "contact_disclosure_invalid",
        message: message.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityAuthenticity {
    Accepted,
    WithheldAuthority,
}

pub(crate) struct SignedIdentity<'a> {
    pub issuer: &'a Identity,
    pub key_id: &'a str,
    pub public_key: &'a str,
    pub signature: &'a str,
    pub payload: &'a [u8],
}

pub fn sign_contact_document(document: &mut Document, signer: &Signer) -> Result<(), EngineError> {
    if document.statement.issuer.uid() != signer.actor_uid
        || (!document.key_id.is_empty() && document.key_id != signer.key_id)
        || (!document.public_key.is_empty() && document.public_key != signer.public_key_b64())
        || (matches!(document.statement.issuer, Identity::Organ(_))
            && !crate::roster::is_root_key_id(&signer.key_id))
    {
        return Err(EngineError::Forbidden(
            "Sign only your own Person disclosure or use the Organ's accepted owner key".into(),
        ));
    }
    if document.signature.is_empty() {
        document.key_id = signer.key_id.clone();
        document.public_key = signer.public_key_b64();
        document.signature = signer.sign_bytes(&document.signing_bytes().map_err(invalid)?);
    }
    Ok(())
}

impl Engine {
    pub(crate) async fn root_identity_authority_on(
        connection: &mut store::sqlx::SqliteConnection,
        organ: &str,
        key: &str,
    ) -> Result<(bool, bool), EngineError> {
        let state = store::identity_roots::state_on(connection, organ).await?;
        let history = store::identity_roots::history_on(connection, organ).await?;
        let changes: Vec<(String, String, String, String, bool)> = store::sqlx::query_as(
            "SELECT s.old_key,s.new_key,s.created_at,s.signature,EXISTS(SELECT 1 FROM identity_revocation r WHERE r.organ_uid=s.organ_uid AND r.revoked_key=s.new_key) FROM identity_succession s WHERE s.organ_uid=? ORDER BY s.created_at,s.old_key,s.new_key LIMIT 65",
        ).bind(organ).fetch_all(&mut *connection).await?;
        if changes.len() > 64 {
            return Err(invalid(
                "Contact identity history exceeds its bounded proof budget",
            ));
        }
        let valid = changes
            .into_iter()
            .filter(|(old, new, at, signature, _)| {
                crate::trust::verify_bytes_strict(
                    old,
                    &crate::roster::succession_signing_payload(organ, old, new, at),
                    signature,
                )
            })
            .collect::<Vec<_>>();
        let mut known = BTreeSet::new();
        if let Some(original) = state.original {
            known.insert(original.public_key);
        }
        known.extend(
            history
                .into_iter()
                .map(|saved| saved.review.proof.public_key),
        );
        let mut current = BTreeSet::new();
        if state.available
            && let Some(anchor) = state.current
            && !anchor.revoked
        {
            current.insert(anchor.public_key);
        }
        for _ in 0..8 {
            let mut next = known.clone();
            let mut next_current = current.clone();
            for (old, new, _, _, revoked) in &valid {
                if known.contains(old) {
                    next.insert(new.clone());
                }
                if current.contains(old) && !revoked {
                    next_current.insert(new.clone());
                }
            }
            if next == known && next_current == current {
                break;
            }
            known = next;
            current = next_current;
        }
        Ok((known.contains(key), current.contains(key)))
    }

    pub async fn authenticate_contact_disclosure(
        &self,
        document: &Document,
        now: i64,
    ) -> Result<IdentityAuthenticity, EngineError> {
        self.authenticate_contact_disclosure_on(
            &mut *self.store.pool.acquire().await?,
            document,
            now,
        )
        .await
    }

    pub(crate) async fn authenticate_contact_disclosure_on(
        &self,
        connection: &mut store::sqlx::SqliteConnection,
        document: &Document,
        now: i64,
    ) -> Result<IdentityAuthenticity, EngineError> {
        document.validate().map_err(invalid)?;
        let bytes = serde_json::to_vec(document)?;
        let document = Document::parse(&bytes).map_err(invalid)?;
        if document.statement.issued_at > now {
            return Err(invalid("This disclosure is not effective yet"));
        }
        let payload = document.signing_bytes().map_err(invalid)?;
        self.authenticate_identity_signature_on(
            connection,
            SignedIdentity {
                issuer: &document.statement.issuer,
                key_id: &document.key_id,
                public_key: &document.public_key,
                signature: &document.signature,
                payload: &payload,
            },
        )
        .await
    }

    pub(crate) async fn authenticate_identity_signature_on(
        &self,
        connection: &mut store::sqlx::SqliteConnection,
        signed: SignedIdentity<'_>,
    ) -> Result<IdentityAuthenticity, EngineError> {
        let issuer = signed.issuer;
        let kind: String =
            store::sqlx::query_scalar("SELECT kind FROM record WHERE uid=? AND deleted_at IS NULL")
                .bind(issuer.uid())
                .fetch_optional(&mut *connection)
                .await?
                .ok_or_else(|| EngineError::Conflict {
                    code: "contact_disclosure_unknown_identity",
                    message: "Independently recognize the issuer profile and signing key".into(),
                })?;
        if !matches!(
            (issuer, kind.as_str()),
            (Identity::Person(_), "person") | (Identity::Organ(_), "organ")
        ) {
            return Err(invalid(
                "The disclosed issuer kind does not match its recognized profile",
            ));
        }
        if matches!(issuer, Identity::Organ(_)) && !crate::roster::is_root_key_id(signed.key_id) {
            return Err(invalid(
                "Use the Organ's independently recognized owner authority for this signature purpose",
            ));
        }
        let (recognized, accepted) =
            Self::identity_key_authority_on(connection, issuer, signed.key_id, signed.public_key)
                .await?;
        if !recognized {
            return Err(EngineError::Conflict {
                code: "contact_disclosure_unknown_identity",
                message: "Independently recognize this signing key or its signed Organ succession before accepting the disclosure".into(),
            });
        }
        if !crate::trust::verify_bytes_strict(signed.public_key, signed.payload, signed.signature) {
            return Err(invalid("Invalid identity-bound signature for this payload"));
        }
        Ok(if accepted {
            IdentityAuthenticity::Accepted
        } else {
            IdentityAuthenticity::WithheldAuthority
        })
    }

    async fn identity_key_authority_on(
        connection: &mut store::sqlx::SqliteConnection,
        issuer: &Identity,
        key_id: &str,
        public_key: &str,
    ) -> Result<(bool, bool), EngineError> {
        match issuer {
            Identity::Person(_) => {
                let key: Option<String> = store::sqlx::query_scalar(
                    "SELECT public_key FROM identity_key WHERE actor_uid=? AND key_id=?",
                )
                .bind(issuer.uid())
                .bind(key_id)
                .fetch_optional(&mut *connection)
                .await?;
                let revoked: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)")
                    .bind(issuer.uid()).bind(public_key).fetch_one(&mut *connection).await?;
                let recognized = key.is_some_and(|key| key == public_key);
                Ok((recognized, recognized && !revoked))
            }
            Identity::Organ(_) => {
                if !crate::roster::is_root_key_id(key_id) {
                    return Ok((false, false));
                }
                Self::root_identity_authority_on(connection, issuer.uid(), public_key).await
            }
        }
    }
}
