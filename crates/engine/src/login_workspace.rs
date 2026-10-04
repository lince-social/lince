use serde::{Deserialize, Serialize};

use super::*;
use store::session_access::{AuthenticationSource, ContactTrust, PeerContact};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Origin {
    person: String,
    generation: i64,
    organ: Option<OrganOrigin>,
    device: Option<DeviceOrigin>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct OrganOrigin {
    organ: String,
    node: String,
    generation: i64,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct DeviceOrigin {
    node: String,
    revision: i64,
    peer: Option<PeerOrigin>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PeerOrigin {
    organ: String,
    node: String,
    generation: i64,
    known: bool,
}

fn peer_origin(peer: &PeerContact) -> Result<PeerOrigin, EngineError> {
    if peer.trust == ContactTrust::Blocked {
        return Err(unavailable());
    }
    Ok(PeerOrigin {
        organ: peer.organ_uid.clone(),
        node: peer.node_id.clone(),
        generation: peer.generation,
        known: peer.trust == ContactTrust::Known,
    })
}

fn origin(authentication: &AuthenticationState) -> Origin {
    Origin {
        person: authentication.person_uid().into(),
        generation: authentication.generation(),
        organ: match authentication.source() {
            AuthenticationSource::Password => None,
            AuthenticationSource::OrganLogin {
                organ_uid,
                node_id,
                generation,
            } => Some(OrganOrigin {
                organ: organ_uid.clone(),
                node: node_id.clone(),
                generation: *generation,
            }),
        },
        device: None,
    }
}

fn unavailable() -> EngineError {
    EngineError::Forbidden(
        "Proposal author authentication was revoked. Request a new proposal.".into(),
    )
}

pub(crate) fn capture_workspace_origin(actor: Option<&str>) -> Result<Option<String>, EngineError> {
    let Ok(login) = CURRENT_LOGIN.try_with(Clone::clone) else {
        return Ok(None);
    };
    if actor != Some(login.person_uid()) {
        return Err(unavailable());
    }
    let captured = match &login.admission {
        Admission::Password(authentication) => origin(authentication),
        Admission::Device(admission) => {
            let mut captured = origin(admission.authentication());
            captured.device = Some(DeviceOrigin {
                node: admission.device().node_id.clone(),
                revision: admission.device().revision,
                peer: admission.peer_contact().map(peer_origin).transpose()?,
            });
            captured
        }
    };
    Ok(Some(serde_json::to_string(&captured)?))
}

pub(crate) async fn require_workspace_origin_on(
    connection: &mut SqliteConnection,
    actor: &str,
    raw: Option<&str>,
) -> Result<(), EngineError> {
    let Some(raw) = raw else {
        return Ok(());
    };
    let expected: Origin = serde_json::from_str(raw).map_err(|_| unavailable())?;
    if expected.person != actor {
        return Err(unavailable());
    }
    let authentication = match &expected.organ {
        Some(organ) => session_access::granted_login_on(connection, &organ.organ, &organ.node)
            .await
            .map_err(|_| unavailable())?,
        None => {
            let username: Option<String> = store::sqlx::query_scalar(
                "SELECT username FROM person_credential WHERE person_uid=?",
            )
            .bind(actor)
            .fetch_optional(&mut *connection)
            .await?;
            let username = username.ok_or_else(unavailable)?;
            session_access::password_on(connection, &username)
                .await
                .map_err(|_| unavailable())?
                .map(|credential| credential.authentication().clone())
        }
    }
    .ok_or_else(unavailable)?;
    let mut current = origin(&authentication);
    if let Some(device) = &expected.device {
        let admitted = session_access::device_on(connection, actor, &device.node)
            .await
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        if admitted.revoked {
            return Err(unavailable());
        }
        current.device = Some(DeviceOrigin {
            node: admitted.node_id,
            revision: admitted.revision,
            peer: session_access::peer_contact_on(connection, &device.node)
                .await
                .map_err(|_| unavailable())?
                .as_ref()
                .map(peer_origin)
                .transpose()?,
        });
    }
    if current != expected {
        return Err(unavailable());
    }
    Ok(())
}
