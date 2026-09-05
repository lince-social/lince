use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::error::EngineError;
use crate::trust::Signer;

pub const ROOT_KEY_ID: &str = "ed25519:root:v1";

pub fn cell_key_id(cell_uid: &str) -> String {
    format!("ed25519:cell:{cell_uid}:v1")
}

pub fn is_root_key_id(key_id: &str) -> bool {
    key_id == ROOT_KEY_ID
}

pub const ROSTER_VALIDITY_DAYS: i64 = 30;

const ROSTER_DOMAIN: &str = "lince/roster/1\n";
const SUCCESSION_DOMAIN: &str = "lince/succession/1\n";
const REVOCATION_DOMAIN: &str = "lince/revocation/1\n";

const FIELD_SEP: char = '\u{1f}';
const RECORD_SEP: char = '\u{1e}';

pub const ENROLMENT_TOKEN_TTL_MINUTES: i64 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellEntry {
    pub cell_uid: String,
    pub node_id: String,
    pub label: String,
    pub operational_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealing_key: Option<crate::seal::SealingKey>,
    #[serde(default)]
    pub front_door: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

pub const CAP_WRITE: &str = "write";
pub const CAP_KARMA: &str = "karma";
pub const CAP_REPRESENT: &str = "represent";

pub fn full_capabilities() -> Vec<String> {
    vec![
        CAP_WRITE.to_string(),
        CAP_KARMA.to_string(),
        CAP_REPRESENT.to_string(),
    ]
}

pub fn relay_capabilities() -> Vec<String> {
    Vec::new()
}

impl CellEntry {
    pub fn may(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|held| held == capability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickupPoint {
    pub organ_uid: String,
    pub node_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roster {
    pub organ_uid: String,
    pub root_key: String,
    pub version: i64,
    pub not_after: String,
    pub cells: Vec<CellEntry>,
    #[serde(default)]
    pub pickup: Vec<PickupPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedRoster {
    pub roster: Roster,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosterOutcome {
    Accepted,
    NotNewer,
    Expired,
    Refused,
}

pub fn roster_signature_is_valid(signed: &SignedRoster) -> bool {
    let Ok(payload) = roster_signing_payload(&signed.roster) else {
        return false;
    };
    if !verify_with(&signed.roster.root_key, &payload, &signed.signature) {
        return false;
    }
    DateTime::parse_from_rfc3339(&signed.roster.not_after)
        .map(|when| when.with_timezone(&Utc) > Utc::now())
        .unwrap_or(false)
}

pub fn roster_signing_payload(roster: &Roster) -> Result<Vec<u8>, EngineError> {
    fn push(out: &mut String, value: &str) -> Result<(), EngineError> {
        if value.contains(FIELD_SEP) || value.contains(RECORD_SEP) {
            return Err(EngineError::Consequence(
                "roster field contains a reserved separator".into(),
            ));
        }
        out.push_str(value);
        out.push(FIELD_SEP);
        Ok(())
    }
    let mut out = String::from(ROSTER_DOMAIN);
    push(&mut out, &roster.organ_uid)?;
    push(&mut out, &roster.root_key)?;
    push(&mut out, &roster.version.to_string())?;
    push(&mut out, &roster.not_after)?;
    for cell in &roster.cells {
        push(&mut out, &cell.cell_uid)?;
        push(&mut out, &cell.node_id)?;
        push(&mut out, &cell.label)?;
        push(&mut out, &cell.operational_key)?;
        push(&mut out, if cell.front_door { "1" } else { "0" })?;
        if let Some(key) = &cell.sealing_key {
            push(&mut out, &key.key_id)?;
            push(&mut out, &key.public)?;
            push(&mut out, &key.not_after)?;
        }
        for capability in &cell.capabilities {
            push(&mut out, capability)?;
        }
        out.push(RECORD_SEP);
    }
    for point in &roster.pickup {
        push(&mut out, &point.organ_uid)?;
        push(&mut out, &point.node_id)?;
        push(&mut out, &point.label)?;
        out.push(RECORD_SEP);
    }
    Ok(out.into_bytes())
}

pub fn needs_publishing(held: Option<&SignedRoster>, root_key: &str, cells: &[CellEntry]) -> bool {
    let Some(held) = held else {
        return true;
    };
    if held.roster.root_key != root_key {
        return true;
    }
    if held
        .roster
        .cells
        .iter()
        .any(|cell| cell.capabilities.is_empty())
    {
        return true;
    }
    let mut held_cells = held.roster.cells.clone();
    let mut next_cells = cells.to_vec();
    held_cells.sort_by(|left, right| left.cell_uid.cmp(&right.cell_uid));
    next_cells.sort_by(|left, right| left.cell_uid.cmp(&right.cell_uid));
    held_cells != next_cells
}

pub fn succession_signing_payload(
    organ_uid: &str,
    old_key: &str,
    new_key: &str,
    created_at: &str,
) -> Vec<u8> {
    format!("{SUCCESSION_DOMAIN}{organ_uid}\n{old_key}\n{new_key}\n{created_at}").into_bytes()
}

pub fn revocation_signing_payload(organ_uid: &str, revoked_key: &str) -> Vec<u8> {
    format!("{REVOCATION_DOMAIN}{organ_uid}\n{revoked_key}").into_bytes()
}

fn verify_with(public_key_b64: &str, payload: &[u8], signature_b64: &str) -> bool {
    let Ok(key_bytes) = B64.decode(public_key_b64) else {
        return false;
    };
    let Ok(key_bytes) = <[u8; 32]>::try_from(key_bytes.as_slice()) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&key_bytes) else {
        return false;
    };
    let Ok(sig_bytes) = B64.decode(signature_b64) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&sig_bytes) else {
        return false;
    };
    key.verify(payload, &signature).is_ok()
}

impl Engine {
    pub async fn publish_root_key(&self, root: &Signer) -> Result<(), EngineError> {
        crate::trust::adopt_key(
            &self.store,
            &root.actor_uid,
            ROOT_KEY_ID,
            &root.public_key_b64(),
        )
        .await
    }

    pub async fn local_organ_public_key(&self) -> Result<Option<String>, EngineError> {
        Ok(self
            .organ_signer
            .lock()
            .await
            .as_ref()
            .map(|signer| signer.public_key_b64()))
    }

    pub async fn publish_roster(
        &self,
        root: &Signer,
        cells: Vec<CellEntry>,
    ) -> Result<SignedRoster, EngineError> {
        let organ_uid = root.actor_uid.clone();
        let previous = store::roster::get(&self.store.pool, &organ_uid).await?;
        let pickup = self
            .roster_of(&organ_uid)
            .await?
            .map(|signed| signed.roster.pickup)
            .unwrap_or_default();
        let roster = Roster {
            organ_uid: organ_uid.clone(),
            root_key: root.public_key_b64(),
            version: previous.map(|row| row.version).unwrap_or(0) + 1,
            not_after: (Utc::now() + Duration::days(ROSTER_VALIDITY_DAYS)).to_rfc3339(),
            cells,
            pickup,
        };
        let payload = roster_signing_payload(&roster)?;
        let signed = SignedRoster {
            signature: root.sign_bytes(&payload),
            roster,
        };
        self.store_roster(&signed).await?;
        self.mirror_roster(&signed).await?;
        Ok(signed)
    }

    pub async fn republish_sealing_key(
        &self,
        from_node: &str,
        cell_uid: &str,
        sealing_key: crate::seal::SealingKey,
    ) -> Result<(), EngineError> {
        let Some(root) = self.root_signer().await? else {
            return Err(EngineError::Consequence(
                "this Cell does not hold the root key, so it cannot publish a roster".into(),
            ));
        };
        let organ_uid = root.actor_uid.clone();
        let Some(held) = self.roster_of(&organ_uid).await? else {
            return Err(EngineError::Consequence(
                "there is no roster to change".into(),
            ));
        };
        let mut cells = held.roster.cells.clone();
        let Some(entry) = cells.iter_mut().find(|cell| cell.cell_uid == cell_uid) else {
            return Err(EngineError::Consequence(
                "that Cell is not a member of this roster".into(),
            ));
        };
        if entry.node_id != from_node {
            return Err(EngineError::Consequence(
                "a Cell may only publish its own mail key".into(),
            ));
        }
        if entry.sealing_key.as_ref() == Some(&sealing_key) {
            return Ok(());
        }
        entry.sealing_key = Some(sealing_key);
        self.publish_roster(&root, cells).await?;
        Ok(())
    }

    async fn store_roster(&self, signed: &SignedRoster) -> Result<(), EngineError> {
        if let (Some(local_organ), Some(local_cell)) = (
            store::organs::local(&self.store.pool).await?,
            store::cells::local(&self.store.pool).await?,
        ) {
            if local_organ.uid == signed.roster.organ_uid {
                let capabilities = signed
                    .roster
                    .cells
                    .iter()
                    .find(|cell| cell.cell_uid == local_cell.uid)
                    .map(|cell| cell.capabilities.clone())
                    .unwrap_or_default();
                store::roster::project_local_capabilities(&self.store.pool, &capabilities).await?;
            }
        }
        store::roster::put(
            &self.store.pool,
            &store::roster::StoredRoster {
                organ_uid: signed.roster.organ_uid.clone(),
                root_key: signed.roster.root_key.clone(),
                version: signed.roster.version,
                not_after: signed.roster.not_after.clone(),
                payload: serde_json::to_string(&signed.roster)
                    .map_err(|error| EngineError::Consequence(error.to_string()))?,
                signature: signed.signature.clone(),
            },
        )
        .await?;
        store::organs::clear_awaiting_roster(&self.store.pool, &signed.roster.organ_uid).await?;
        Ok(())
    }

    pub async fn roster_of(&self, organ_uid: &str) -> Result<Option<SignedRoster>, EngineError> {
        let Some(stored) = store::roster::get(&self.store.pool, organ_uid).await? else {
            return Ok(None);
        };
        let roster: Roster = serde_json::from_str(&stored.payload)
            .map_err(|error| EngineError::Consequence(error.to_string()))?;
        Ok(Some(SignedRoster {
            roster,
            signature: stored.signature,
        }))
    }

    pub async fn key_chains(&self, organ_uid: &str, key: &str) -> Result<bool, EngineError> {
        if store::roster::is_revoked(&self.store.pool, organ_uid, key).await? {
            return Ok(false);
        }
        let held: HashSet<String> = crate::trust::keys_of(&self.store, organ_uid)
            .await?
            .into_iter()
            .filter(|(key_id, _)| is_root_key_id(key_id))
            .map(|(_, public_key)| public_key)
            .collect();
        if held.contains(key) {
            return Ok(true);
        }
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for (old_key, new_key) in store::roster::successions(&self.store.pool, organ_uid).await? {
            edges.entry(old_key).or_default().push(new_key);
        }
        let mut seen: HashSet<String> = held.clone();
        let mut frontier: Vec<String> = held.into_iter().collect();
        while let Some(current) = frontier.pop() {
            let Some(next) = edges.get(&current) else {
                continue;
            };
            for candidate in next {
                if store::roster::is_revoked(&self.store.pool, organ_uid, candidate).await? {
                    continue;
                }
                if candidate == key {
                    return Ok(true);
                }
                if seen.insert(candidate.clone()) {
                    frontier.push(candidate.clone());
                }
            }
        }
        Ok(false)
    }

    pub async fn adopt_roster(&self, signed: &SignedRoster) -> Result<RosterOutcome, EngineError> {
        let organ_uid = &signed.roster.organ_uid;
        let payload = roster_signing_payload(&signed.roster)?;
        if !verify_with(&signed.roster.root_key, &payload, &signed.signature) {
            return Ok(RosterOutcome::Refused);
        }
        if !self.key_chains(organ_uid, &signed.roster.root_key).await? {
            return Ok(RosterOutcome::Refused);
        }
        let fresh = DateTime::parse_from_rfc3339(&signed.roster.not_after)
            .map(|when| when.with_timezone(&Utc) > Utc::now())
            .unwrap_or(false);
        if !fresh {
            return Ok(RosterOutcome::Expired);
        }
        let previous = store::roster::get(&self.store.pool, organ_uid).await?;
        if previous
            .as_ref()
            .is_some_and(|row| row.version >= signed.roster.version)
        {
            return Ok(RosterOutcome::NotNewer);
        }
        self.store_roster(signed).await?;
        Ok(RosterOutcome::Accepted)
    }

    pub async fn roster_is_stale(&self, organ_uid: &str) -> Result<bool, EngineError> {
        let Some(stored) = store::roster::get(&self.store.pool, organ_uid).await? else {
            return Ok(false);
        };
        Ok(DateTime::parse_from_rfc3339(&stored.not_after)
            .map(|when| when.with_timezone(&Utc) <= Utc::now())
            .unwrap_or(true))
    }

    pub async fn cell_may(
        &self,
        organ_uid: &str,
        cell_uid: &str,
        capability: &str,
    ) -> Result<bool, EngineError> {
        Ok(self
            .roster_of(organ_uid)
            .await?
            .and_then(|signed| {
                signed
                    .roster
                    .cells
                    .into_iter()
                    .find(|cell| cell.cell_uid == cell_uid)
            })
            .is_some_and(|cell| cell.may(capability)))
    }

    pub async fn roster_node_ids(&self, organ_uid: &str) -> Result<Vec<String>, EngineError> {
        Ok(self
            .roster_of(organ_uid)
            .await?
            .map(|signed| {
                signed
                    .roster
                    .cells
                    .into_iter()
                    .map(|cell| cell.node_id)
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn mirror_roster(&self, signed: &SignedRoster) -> Result<(), EngineError> {
        let cells: Vec<serde_json::Value> = signed
            .roster
            .cells
            .iter()
            .map(|cell| {
                serde_json::json!({
                    "cell_uid": cell.cell_uid,
                    "node_id": cell.node_id,
                    "label": cell.label,
                    "front_door": cell.front_door,
                    "capabilities": cell.capabilities,
                    "sealing_key": cell.sealing_key,
                })
            })
            .collect();
        store::records::set_extension_raw(
            &self.store.pool,
            &signed.roster.organ_uid,
            "lince.roster",
            &serde_json::json!({
                "version": signed.roster.version,
                "not_after": signed.roster.not_after,
                "root_key": signed.roster.root_key,
                "cells": cells,
                "pickup": signed.roster.pickup,
            }),
        )
        .await?;
        Ok(())
    }

    pub fn set_root_key_path(&self, path: std::path::PathBuf) {
        *self.root_key_path.lock().expect("root key path") = Some(path);
    }

    pub fn set_sealing_keyring_path(&self, path: std::path::PathBuf) {
        *self.sealing_keyring_path.lock().expect("sealing keyring") = Some(path);
    }

    pub async fn sealing_keyring(&self) -> Result<Option<crate::seal::Keyring>, EngineError> {
        let path = self
            .sealing_keyring_path
            .lock()
            .expect("sealing keyring")
            .clone();
        let (Some(path), Some(cell)) = (path, store::cells::local(&self.store.pool).await?) else {
            return Ok(None);
        };
        Ok(Some(crate::seal::load_keyring(&path, &cell.uid)?))
    }

    pub async fn published_sealing_key(
        &self,
    ) -> Result<Option<crate::seal::SealingKey>, EngineError> {
        Ok(self
            .sealing_keyring()
            .await?
            .and_then(|keyring| keyring.current()))
    }

    pub async fn own_sealing_key_is_published(&self) -> Result<bool, EngineError> {
        let (Some(organ), Some(cell)) = (
            store::organs::local(&self.store.pool).await?,
            store::cells::local(&self.store.pool).await?,
        ) else {
            return Ok(true);
        };
        let Some(current) = self
            .sealing_keyring()
            .await?
            .and_then(|keyring| keyring.current())
        else {
            return Ok(true);
        };
        Ok(self
            .roster_of(&organ.uid)
            .await?
            .and_then(|held| {
                held.roster
                    .cells
                    .iter()
                    .find(|entry| entry.cell_uid == cell.uid)
                    .and_then(|entry| entry.sealing_key.clone())
            })
            .as_ref()
            == Some(&current))
    }

    pub async fn root_signer(&self) -> Result<Option<Signer>, EngineError> {
        let path = self.root_key_path.lock().expect("root key path").clone();
        let (Some(path), Some(organ)) = (path, store::organs::local(&self.store.pool).await?)
        else {
            return Ok(None);
        };
        if !root_key_present(&path) {
            return Ok(None);
        }
        Ok(Some(Signer::load_or_create(
            &path,
            &organ.uid,
            ROOT_KEY_ID,
        )?))
    }

    pub async fn published_revocations(
        &self,
        organ_uid: &str,
    ) -> Result<Vec<(String, String)>, EngineError> {
        Ok(store::roster::revocations_of(&self.store.pool, organ_uid).await?)
    }

    pub async fn issue_enrolment_token(&self) -> Result<String, EngineError> {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let expires_at = (Utc::now() + Duration::minutes(ENROLMENT_TOKEN_TTL_MINUTES)).to_rfc3339();
        store::roster::put_enrolment_token(&self.store.pool, &hash_token(&token), &expires_at)
            .await?;
        Ok(token)
    }

    pub async fn redeem_enrolment(
        &self,
        root: &Signer,
        token: &str,
        cell: CellEntry,
    ) -> Result<SignedRoster, EngineError> {
        if !store::roster::redeem_enrolment_token(&self.store.pool, &hash_token(token)).await? {
            return Err(EngineError::Consequence(
                "enrolment token is unknown, expired, or already used".into(),
            ));
        }
        self.enrol_cell(root, cell).await
    }

    pub async fn enrol_cell(
        &self,
        root: &Signer,
        cell: CellEntry,
    ) -> Result<SignedRoster, EngineError> {
        let mut cells = self
            .roster_of(&root.actor_uid)
            .await?
            .map(|signed| signed.roster.cells)
            .unwrap_or_default();
        cells.retain(|existing| existing.cell_uid != cell.cell_uid);
        cells.push(cell);
        self.publish_roster(root, cells).await
    }

    pub async fn set_pickup_points(
        &self,
        root: &Signer,
        pickup: Vec<PickupPoint>,
    ) -> Result<SignedRoster, EngineError> {
        let cells = self
            .roster_of(&root.actor_uid)
            .await?
            .map(|signed| signed.roster.cells)
            .unwrap_or_default();
        let organ_uid = root.actor_uid.clone();
        let previous = store::roster::get(&self.store.pool, &organ_uid).await?;
        let roster = Roster {
            organ_uid: organ_uid.clone(),
            root_key: root.public_key_b64(),
            version: previous.map(|row| row.version).unwrap_or(0) + 1,
            not_after: (Utc::now() + Duration::days(ROSTER_VALIDITY_DAYS)).to_rfc3339(),
            cells,
            pickup,
        };
        let payload = roster_signing_payload(&roster)?;
        let signed = SignedRoster {
            signature: root.sign_bytes(&payload),
            roster,
        };
        self.store_roster(&signed).await?;
        self.mirror_roster(&signed).await?;
        Ok(signed)
    }

    pub async fn own_pickup_points(&self) -> Result<Vec<PickupPoint>, EngineError> {
        let Some(local) = store::organs::local(&self.store.pool).await? else {
            return Ok(Vec::new());
        };
        Ok(self
            .roster_of(&local.uid)
            .await?
            .map(|signed| signed.roster.pickup)
            .unwrap_or_default())
    }

    pub async fn own_node_id(&self) -> Result<Option<String>, EngineError> {
        let Some(local) = store::organs::local(&self.store.pool).await? else {
            return Ok(None);
        };
        let Some(cell) = store::cells::local(&self.store.pool).await? else {
            return Ok(None);
        };
        Ok(self.roster_of(&local.uid).await?.and_then(|signed| {
            signed
                .roster
                .cells
                .into_iter()
                .find(|entry| entry.cell_uid == cell.uid)
                .map(|entry| entry.node_id)
        }))
    }

    pub async fn revoke_cell(
        &self,
        root: &Signer,
        cell_uid: &str,
    ) -> Result<SignedRoster, EngineError> {
        let mut cells = self
            .roster_of(&root.actor_uid)
            .await?
            .map(|signed| signed.roster.cells)
            .unwrap_or_default();
        cells.retain(|existing| existing.cell_uid != cell_uid);
        self.publish_roster(root, cells).await
    }

    pub async fn sign_succession(
        &self,
        old_root: &Signer,
        new_key_b64: &str,
    ) -> Result<(), EngineError> {
        let organ_uid = old_root.actor_uid.clone();
        let old_key = old_root.public_key_b64();
        let created_at = Utc::now().to_rfc3339();
        let payload = succession_signing_payload(&organ_uid, &old_key, new_key_b64, &created_at);
        let signature = old_root.sign_bytes(&payload);
        store::roster::record_succession(
            &self.store.pool,
            &organ_uid,
            &old_key,
            new_key_b64,
            &signature,
            &created_at,
        )
        .await?;
        Ok(())
    }

    pub async fn adopt_succession(
        &self,
        organ_uid: &str,
        old_key: &str,
        new_key: &str,
        created_at: &str,
        signature: &str,
    ) -> Result<bool, EngineError> {
        let payload = succession_signing_payload(organ_uid, old_key, new_key, created_at);
        if !verify_with(old_key, &payload, signature) {
            return Ok(false);
        }
        if !self.key_chains(organ_uid, old_key).await? {
            return Ok(false);
        }
        store::roster::record_succession(
            &self.store.pool,
            organ_uid,
            old_key,
            new_key,
            signature,
            created_at,
        )
        .await?;
        Ok(true)
    }

    pub async fn published_successions(
        &self,
        organ_uid: &str,
    ) -> Result<Vec<store::roster::SuccessionRow>, EngineError> {
        Ok(store::roster::published_successions(&self.store.pool, organ_uid).await?)
    }

    pub fn revocation_certificate(&self, root: &Signer) -> (String, String) {
        let key = root.public_key_b64();
        let payload = revocation_signing_payload(&root.actor_uid, &key);
        (key, root.sign_bytes(&payload))
    }

    pub async fn adopt_revocation(
        &self,
        organ_uid: &str,
        revoked_key: &str,
        signature: &str,
    ) -> Result<bool, EngineError> {
        let payload = revocation_signing_payload(organ_uid, revoked_key);
        if !verify_with(revoked_key, &payload, signature) {
            return Ok(false);
        }
        store::roster::record_revocation(&self.store.pool, organ_uid, revoked_key, signature)
            .await?;
        Ok(true)
    }
}

pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    B64.encode(hasher.finalize())
}

pub fn root_key_present(path: &std::path::Path) -> bool {
    path.exists()
}

pub fn export_root_key(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), EngineError> {
    let secret = std::fs::read(source)?;
    if secret.len() != 32 {
        return Err(EngineError::Consequence(
            "root key file is not 32 bytes".into(),
        ));
    }
    if destination.exists() {
        return Err(EngineError::Consequence(
            "a file already exists at the destination; refusing to overwrite a key".into(),
        ));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    use std::io::Write as _;
    let mut file = options.open(destination)?;
    file.write_all(&secret)?;
    file.sync_all()?;
    Ok(())
}

pub fn detach_root_key(
    path: &std::path::Path,
    copy_at: &std::path::Path,
) -> Result<(), EngineError> {
    let local = std::fs::read(path)?;
    let copy = std::fs::read(copy_at).map_err(|error| {
        EngineError::Consequence(format!("cannot read the exported copy: {error}"))
    })?;
    if local != copy {
        return Err(EngineError::Consequence(
            "the exported copy does not match the local root key; refusing to detach".into(),
        ));
    }
    std::fs::remove_file(path)?;
    Ok(())
}
