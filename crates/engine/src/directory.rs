use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use pkarr::dns::Name;
use pkarr::dns::rdata::{RData, TXT};
use pkarr::{Keypair, PublicKey, SignedPacket};

use crate::error::EngineError;
use crate::roster::{ROOT_KEY_ID, SignedRoster};
use crate::trust::Signer;

const RECORD_NAME: &str = "_lince";

const RECORD_TTL_SECS: u32 = 1800;

const RECORD_VERSION: &str = "1";

pub const MAX_PUBLIC_CELLS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicRecord {
    pub organ_uid: String,
    pub roster_version: i64,
    pub node_ids: Vec<String>,
}

pub fn public_record(signed: &SignedRoster) -> PublicRecord {
    PublicRecord {
        organ_uid: signed.roster.organ_uid.clone(),
        roster_version: signed.roster.version,
        node_ids: signed
            .roster
            .cells
            .iter()
            .filter(|cell| cell.front_door)
            .map(|cell| cell.node_id.clone())
            .take(MAX_PUBLIC_CELLS)
            .collect(),
    }
}

pub fn encode(record: &PublicRecord, root_secret: &[u8; 32]) -> Result<Vec<u8>, EngineError> {
    if record.node_ids.is_empty() {
        return Err(EngineError::Consequence(
            "a public record needs at least one front-door Cell".into(),
        ));
    }
    let keypair = Keypair::from_secret_key(root_secret);
    let mut strings = vec![
        format!("v={RECORD_VERSION}"),
        format!("o={}", record.organ_uid),
        format!("r={}", record.roster_version),
    ];
    for (index, node_id) in record.node_ids.iter().take(MAX_PUBLIC_CELLS).enumerate() {
        strings.push(format!("c{index}={node_id}"));
    }
    let mut txt = TXT::new();
    for string in &strings {
        txt.add_string(string)
            .map_err(|error| EngineError::Consequence(format!("public record: {error}")))?;
    }
    let name = Name::new(RECORD_NAME)
        .map_err(|error| EngineError::Consequence(format!("public record name: {error}")))?;
    let packet = SignedPacket::builder()
        .txt(name, txt, RECORD_TTL_SECS)
        .sign(&keypair)
        .map_err(|error| EngineError::Consequence(format!("public record: {error}")))?;
    Ok(packet.serialize())
}

pub fn decode(packet: &SignedPacket) -> Result<PublicRecord, EngineError> {
    let mut version: Option<String> = None;
    let mut organ_uid: Option<String> = None;
    let mut roster_version: i64 = 0;
    let mut cells: Vec<(usize, String)> = Vec::new();
    for record in packet.all_resource_records() {
        let RData::TXT(txt) = &record.rdata else {
            continue;
        };
        for (key, value) in txt.attributes() {
            let Some(value) = value else { continue };
            match key.as_str() {
                "v" => version = Some(value),
                "o" => organ_uid = Some(value),
                "r" => roster_version = value.parse().unwrap_or(0),
                other => {
                    if let Some(index) = other.strip_prefix('c') {
                        if let Ok(index) = index.parse::<usize>() {
                            cells.push((index, value));
                        }
                    }
                }
            }
        }
    }
    if version.as_deref() != Some(RECORD_VERSION) {
        return Err(EngineError::Consequence(
            "public record is not a version this build understands".into(),
        ));
    }
    let Some(organ_uid) = organ_uid.filter(|uid| !uid.is_empty()) else {
        return Err(EngineError::Consequence(
            "public record names no Organ".into(),
        ));
    };
    cells.sort_by_key(|(index, _)| *index);
    Ok(PublicRecord {
        organ_uid,
        roster_version,
        node_ids: cells.into_iter().map(|(_, node_id)| node_id).collect(),
    })
}

pub fn decode_stored(bytes: &[u8]) -> Result<SignedPacket, EngineError> {
    let parsed = SignedPacket::deserialize(bytes)
        .map_err(|error| EngineError::Consequence(format!("stored public record: {error}")))?;
    SignedPacket::from_relay_payload(&parsed.public_key(), &parsed.to_relay_payload()).map_err(
        |error| EngineError::Consequence(format!("stored public record is not signed: {error}")),
    )
}

pub fn packet_root_key(packet: &SignedPacket) -> String {
    B64.encode(packet.public_key().as_bytes())
}

pub fn lookup_key(root_key_b64: &str) -> Result<PublicKey, EngineError> {
    let bytes = B64
        .decode(root_key_b64)
        .map_err(|_| EngineError::Consequence("root key is not base64".into()))?;
    let bytes: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| EngineError::Consequence("root key is not 32 bytes".into()))?;
    PublicKey::try_from(&bytes)
        .map_err(|error| EngineError::Consequence(format!("root key is not a key: {error}")))
}

pub fn verify_for(record: &PublicRecord, expected_organ_uid: &str) -> Result<(), EngineError> {
    if record.organ_uid != expected_organ_uid {
        return Err(EngineError::Consequence(format!(
            "public record for {expected_organ_uid} names a different Organ ({})",
            record.organ_uid
        )));
    }
    Ok(())
}

pub struct Directory {
    client: pkarr::Client,
}

impl Directory {
    pub fn new(relays: &[String]) -> Result<Directory, EngineError> {
        let mut builder = pkarr::Client::builder();
        builder.no_dht();
        if !relays.is_empty() {
            builder.relays(relays).map_err(|error| {
                EngineError::Consequence(format!("directory relay url: {error}"))
            })?;
        }
        let client = builder
            .build()
            .map_err(|error| EngineError::Consequence(format!("directory client: {error}")))?;
        Ok(Directory { client })
    }

    pub async fn publish(&self, packet: &SignedPacket) -> Result<(), EngineError> {
        self.client
            .publish(packet)
            .await
            .map(|_| ())
            .map_err(|error| EngineError::Consequence(format!("publishing the record: {error}")))
    }

    pub async fn resolve(&self, key: &PublicKey) -> Option<SignedPacket> {
        self.client
            .resolve(key, pkarr::ResolvePolicy::CacheFirst)
            .await
            .ok()
    }
}

impl crate::Engine {
    async fn directory_relays(&self, organ_uid: &str) -> Vec<String> {
        let cell = store::cells::config(&self.store.pool, "lince.discovery")
            .await
            .ok()
            .flatten();
        let fields = match cell {
            Some(fields) => fields,
            None => {
                let Ok(Some(fields)) =
                    store::records::get_extension(&self.store.pool, organ_uid, "lince.discovery")
                        .await
                else {
                    return Vec::new();
                };
                fields
            }
        };
        fields
            .get("relays")
            .and_then(serde_json::Value::as_array)
            .map(|relays| {
                relays
                    .iter()
                    .filter_map(|relay| relay.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn directory(&self, organ_uid: &str) -> Result<&Directory, EngineError> {
        self.directory
            .get_or_try_init(|| async {
                let relays = self.directory_relays(organ_uid).await;
                Directory::new(&relays)
            })
            .await
    }

    pub async fn sign_public_record(&self, root: &Signer) -> Result<(), EngineError> {
        let Some(signed) = self.roster_of(&root.actor_uid).await? else {
            return Err(EngineError::Consequence(
                "no roster to cut a public record from".into(),
            ));
        };
        let record = public_record(&signed);
        if let Some(stored) =
            store::roster::public_packet(&self.store.pool, &root.actor_uid).await?
        {
            if let Ok(previous) = decode_stored(&stored).and_then(|packet| decode(&packet)) {
                if previous == record {
                    return Ok(());
                }
            }
        }
        if record.node_ids.is_empty() {
            store::roster::clear_public_packet(&self.store.pool, &root.actor_uid).await?;
            return Ok(());
        }
        let bytes = encode(&record, &root.secret_bytes())?;
        store::roster::put_public_packet(&self.store.pool, &root.actor_uid, &bytes).await?;
        Ok(())
    }

    pub async fn republish_public_record(&self, organ_uid: &str) -> Result<bool, EngineError> {
        let Some(bytes) = store::roster::public_packet(&self.store.pool, organ_uid).await? else {
            return Ok(false);
        };
        let packet = decode_stored(&bytes)?;
        self.directory(organ_uid).await?.publish(&packet).await?;
        Ok(true)
    }

    pub async fn resolve_public_record(
        &self,
        organ_uid: &str,
    ) -> Result<Option<PublicRecord>, EngineError> {
        let Some(root_key) = crate::trust::key_of(&self.store, organ_uid, ROOT_KEY_ID).await?
        else {
            return Ok(None);
        };
        let key = lookup_key(&root_key)?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.uid)
            .unwrap_or_default();
        let Some(packet) = self.directory(&local).await?.resolve(&key).await else {
            return Ok(None);
        };
        let record = decode(&packet)?;
        verify_for(&record, organ_uid)?;
        Ok(Some(record))
    }
}
