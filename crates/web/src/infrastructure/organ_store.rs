use persistence::write_coordinator::{SqlParameter, WriteCoordinatorHandle};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Sqlite};
use std::sync::Arc;

const DEFAULT_LOCAL_ORGAN_ID: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Organ {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub trust_state: String,
    pub contact_discovery_enabled: i64,
    pub last_seen_at: Option<String>,
    pub last_transfer_polled_at: Option<String>,
    pub proximity: i64,
    pub transfer_send_received_receipts: i64,
    pub transfer_send_seen_receipts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct OrganSyncPolicy {
    pub organ_id: i64,
    pub sync_resources: String,
    pub record_sync_mode: String,
}

#[derive(Clone)]
pub struct OrganStore {
    db: Arc<Pool<Sqlite>>,
    writer: WriteCoordinatorHandle,
}

impl OrganStore {
    pub fn new(db: Arc<Pool<Sqlite>>, writer: WriteCoordinatorHandle) -> Self {
        Self { db, writer }
    }

    pub async fn list(&self) -> Result<Vec<Organ>, String> {
        let mut organs = sqlx::query_as::<_, Organ>(
            "SELECT id, name, base_url, trust_state, contact_discovery_enabled, last_seen_at, last_transfer_polled_at, proximity, transfer_send_received_receipts, transfer_send_seen_receipts FROM organ ORDER BY LOWER(name), id",
        )
        .fetch_all(&*self.db)
        .await
        .map_err(|error| format!("Nao consegui listar os orgaos: {error}"))?;
        organs.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(organs)
    }

    pub async fn get(&self, organ_id: impl ToString) -> Result<Option<Organ>, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(None);
        };

        sqlx::query_as::<_, Organ>(
            "SELECT id, name, base_url, trust_state, contact_discovery_enabled, last_seen_at, last_transfer_polled_at, proximity, transfer_send_received_receipts, transfer_send_seen_receipts FROM organ WHERE id = ? LIMIT 1",
        )
            .bind(organ_id)
            .fetch_optional(&*self.db)
            .await
            .map_err(|error| format!("Nao consegui carregar o orgao: {error}"))
    }

    pub async fn create(&self, name: String, base_url: String) -> Result<Organ, String> {
        self.create_with_options(name, base_url, "known", false)
            .await
    }

    pub async fn create_discovered(&self, name: String, base_url: String) -> Result<Organ, String> {
        let (name, base_url) = normalize_organ_fields(name, base_url)?;
        if let Some(existing) = self.find_by_base_url(&base_url).await? {
            return Ok(existing);
        }
        self.create_with_options(name, base_url, "unknown", false)
            .await
    }

    async fn create_with_options(
        &self,
        name: String,
        base_url: String,
        trust_state: &str,
        contact_discovery_enabled: bool,
    ) -> Result<Organ, String> {
        let (name, base_url) = normalize_organ_fields(name, base_url)?;
        let trust_state = normalize_trust_state(trust_state)?;
        let contact_discovery_enabled = if contact_discovery_enabled {
            1_i64
        } else {
            0_i64
        };
        let outcome = self
            .writer
            .execute_statement_returning_id(
                "INSERT INTO organ(name, base_url, trust_state, contact_discovery_enabled) VALUES (?, ?, ?, ?)".to_string(),
                vec![
                    SqlParameter::Text(name.clone()),
                    SqlParameter::Text(base_url.clone()),
                    SqlParameter::Text(trust_state.clone()),
                    SqlParameter::Integer(contact_discovery_enabled),
                ],
            )
            .await
            .map_err(|error| format!("Nao consegui salvar o orgao: {error}"))?;
        let Some(id) = outcome.last_insert_rowid else {
            return Err("Nao consegui obter o id do orgao criado.".into());
        };
        self.set_record_sync_policy(id, "none").await?;
        Ok(Organ {
            id,
            name,
            base_url,
            trust_state,
            contact_discovery_enabled,
            last_seen_at: None,
            last_transfer_polled_at: None,
            proximity: 100,
            transfer_send_received_receipts: 1,
            transfer_send_seen_receipts: 1,
        })
    }

    pub async fn update(
        &self,
        organ_id: impl ToString,
        name: String,
        base_url: String,
    ) -> Result<Organ, String> {
        let organ_id = parse_organ_id(organ_id).ok_or_else(|| "Orgao invalido.".to_string())?;
        let (name, base_url) = normalize_organ_fields(name, base_url)?;
        let outcome = self
            .writer
            .execute_statement(
                "UPDATE organ SET name = ?, base_url = ? WHERE id = ?".to_string(),
                vec![
                    SqlParameter::Text(name.clone()),
                    SqlParameter::Text(base_url.clone()),
                    SqlParameter::Integer(organ_id),
                ],
            )
            .await
            .map_err(|error| format!("Nao consegui salvar o orgao: {error}"))?;
        if outcome.rows_affected == 0 {
            return Err("Orgao nao encontrado.".into());
        }
        self.get(organ_id)
            .await?
            .ok_or_else(|| "Orgao nao encontrado.".into())
    }

    pub async fn set_trust_state(
        &self,
        organ_id: impl ToString,
        trust_state: &str,
    ) -> Result<bool, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(false);
        };
        let trust_state = normalize_trust_state(trust_state)?;
        let outcome = self
            .writer
            .execute_statement(
                "UPDATE organ SET trust_state = ? WHERE id = ?".to_string(),
                vec![
                    SqlParameter::Text(trust_state),
                    SqlParameter::Integer(organ_id),
                ],
            )
            .await
            .map_err(|error| format!("Nao consegui atualizar confianca do orgao: {error}"))?;
        Ok(outcome.rows_affected > 0)
    }

    pub async fn set_contact_discovery_enabled(
        &self,
        organ_id: impl ToString,
        enabled: bool,
    ) -> Result<bool, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(false);
        };
        let enabled = if enabled { 1_i64 } else { 0_i64 };
        let outcome = self
            .writer
            .execute_statement(
                "UPDATE organ SET contact_discovery_enabled = ? WHERE id = ?".to_string(),
                vec![
                    SqlParameter::Integer(enabled),
                    SqlParameter::Integer(organ_id),
                ],
            )
            .await
            .map_err(|error| {
                format!("Nao consegui atualizar descoberta de contatos do orgao: {error}")
            })?;
        Ok(outcome.rows_affected > 0)
    }

    pub async fn set_proximity(
        &self,
        organ_id: impl ToString,
        proximity: i64,
    ) -> Result<bool, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(false);
        };
        if proximity < 0 {
            return Err("Proximity cannot be negative.".into());
        }
        let outcome = self
            .writer
            .execute_statement(
                "UPDATE organ SET proximity = ? WHERE id = ?".to_string(),
                vec![
                    SqlParameter::Integer(proximity),
                    SqlParameter::Integer(organ_id),
                ],
            )
            .await
            .map_err(|error| format!("Nao consegui atualizar proximidade do orgao: {error}"))?;
        Ok(outcome.rows_affected > 0)
    }

    pub async fn set_transfer_receipts(
        &self,
        organ_id: impl ToString,
        send_received: bool,
        send_seen: bool,
    ) -> Result<bool, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(false);
        };
        let outcome = self
            .writer
            .execute_statement(
                "UPDATE organ
                 SET transfer_send_received_receipts = ?,
                     transfer_send_seen_receipts = ?
                 WHERE id = ?"
                    .to_string(),
                vec![
                    SqlParameter::Integer(if send_received { 1 } else { 0 }),
                    SqlParameter::Integer(if send_seen { 1 } else { 0 }),
                    SqlParameter::Integer(organ_id),
                ],
            )
            .await
            .map_err(|error| {
                format!("Nao consegui atualizar recibos de Transfer do orgao: {error}")
            })?;
        Ok(outcome.rows_affected > 0)
    }

    pub async fn get_sync_policy(
        &self,
        organ_id: impl ToString,
    ) -> Result<Option<OrganSyncPolicy>, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(None);
        };
        sqlx::query_as::<_, OrganSyncPolicy>(
            "SELECT organ_id, sync_resources, record_sync_mode
             FROM organ_sync_policy
             WHERE organ_id = ?
             LIMIT 1",
        )
        .bind(organ_id)
        .fetch_optional(&*self.db)
        .await
        .map_err(|error| format!("Nao consegui carregar politica de sync do orgao: {error}"))
    }

    pub async fn set_record_sync_policy(
        &self,
        organ_id: impl ToString,
        record_sync_mode: &str,
    ) -> Result<bool, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(false);
        };
        let record_sync_mode = normalize_record_sync_mode(record_sync_mode)?;
        let sync_resources = if record_sync_mode == "none" {
            "[]"
        } else {
            r#"["record"]"#
        };
        let outcome = self
            .writer
            .execute_statement(
                "INSERT INTO organ_sync_policy(
                    organ_id,
                    sync_resources,
                    record_sync_mode
                 ) VALUES (?, ?, ?)
                 ON CONFLICT(organ_id) DO UPDATE SET
                    sync_resources = excluded.sync_resources,
                    record_sync_mode = excluded.record_sync_mode,
                    updated_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Integer(organ_id),
                    SqlParameter::Text(sync_resources.to_string()),
                    SqlParameter::Text(record_sync_mode),
                ],
            )
            .await
            .map_err(|error| format!("Nao consegui atualizar sync do orgao: {error}"))?;
        Ok(outcome.rows_affected > 0)
    }

    pub async fn mark_seen_by_base_url(&self, base_url: &str) -> Result<bool, String> {
        let Some(organ) = self.find_by_base_url(base_url).await? else {
            return Ok(false);
        };
        if organ.trust_state == "blocked" {
            return Ok(false);
        }
        let outcome = self
            .writer
            .execute_statement(
                "UPDATE organ SET last_seen_at = CURRENT_TIMESTAMP WHERE id = ?".to_string(),
                vec![SqlParameter::Integer(organ.id)],
            )
            .await
            .map_err(|error| format!("Nao consegui atualizar presenca do orgao: {error}"))?;
        Ok(outcome.rows_affected > 0)
    }

    pub async fn mark_transfer_polled(&self, organ_id: i64) -> Result<(), String> {
        self.writer
            .execute_statement(
                "UPDATE organ SET last_transfer_polled_at = CURRENT_TIMESTAMP WHERE id = ?"
                    .to_string(),
                vec![SqlParameter::Integer(organ_id)],
            )
            .await
            .map_err(|error| format!("Nao consegui atualizar poll de Transfer: {error}"))?;
        Ok(())
    }

    pub async fn find_by_base_url(&self, base_url: &str) -> Result<Option<Organ>, String> {
        let base_url = base_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return Ok(None);
        }
        for organ in self.list().await? {
            if same_organ_base_url(&organ.base_url, base_url) {
                return Ok(Some(organ));
            }
        }
        Ok(None)
    }

    pub async fn known_transfer_poll_targets(&self) -> Result<Vec<Organ>, String> {
        let mut organs = self
            .list()
            .await?
            .into_iter()
            .filter(|organ| {
                !is_default_local_organ(organ.id)
                    && organ.trust_state == "known"
                    && !same_organ_base_url(&organ.base_url, "")
            })
            .collect::<Vec<_>>();
        organs.sort_by(|left, right| {
            left.proximity
                .cmp(&right.proximity)
                .then_with(|| left.base_url.cmp(&right.base_url))
        });
        organs.dedup_by(|left, right| same_organ_base_url(&left.base_url, &right.base_url));
        Ok(organs)
    }

    pub async fn discoverable_contacts(
        &self,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Organ>, String> {
        let search = search
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase);
        let limit = limit.clamp(1, 100) as usize;
        let offset = offset.max(0) as usize;
        let contacts = self
            .list()
            .await?
            .into_iter()
            .filter(|organ| {
                !is_default_local_organ(organ.id)
                    && organ.contact_discovery_enabled != 0
                    && organ.trust_state != "blocked"
            })
            .filter(|organ| {
                search.as_ref().is_none_or(|query| {
                    organ.name.to_lowercase().contains(query)
                        || organ.base_url.to_lowercase().contains(query)
                })
            })
            .skip(offset)
            .take(limit)
            .collect();
        Ok(contacts)
    }

    pub async fn delete(&self, organ_id: impl ToString) -> Result<bool, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(false);
        };

        let outcome = self
            .writer
            .execute_statement(
                "DELETE FROM organ WHERE id = ?".to_string(),
                vec![SqlParameter::Integer(organ_id)],
            )
            .await
            .map_err(|error| format!("Nao consegui apagar o orgao: {error}"))?;
        Ok(outcome.rows_affected > 0)
    }
}

pub fn is_default_local_organ(organ_id: i64) -> bool {
    organ_id == DEFAULT_LOCAL_ORGAN_ID
}

pub fn organ_requires_auth(organ: &Organ, local_auth_required: bool) -> bool {
    !is_default_local_organ(organ.id) || local_auth_required
}

fn normalize_organ_fields(name: String, base_url: String) -> Result<(String, String), String> {
    let name = name.trim().to_string();
    let base_url = base_url.trim().trim_end_matches('/').to_string();
    if name.is_empty() {
        return Err("Orgao precisa definir um name.".into());
    }
    if base_url.is_empty() {
        return Err("Orgao precisa definir um base_url.".into());
    }

    Ok((name, base_url))
}

fn normalize_trust_state(value: &str) -> Result<String, String> {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "unknown" | "known" | "blocked" => Ok(value),
        _ => Err("Estado de confianca do orgao invalido.".into()),
    }
}

fn normalize_record_sync_mode(value: &str) -> Result<String, String> {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "none" | "sync_outgoing" | "sync_incoming" | "sync_both" => Ok(value),
        _ => Err("Modo de sync de record invalido.".into()),
    }
}

fn same_organ_base_url(left: &str, right: &str) -> bool {
    left.trim().trim_end_matches('/') == right.trim().trim_end_matches('/')
}

fn parse_organ_id(organ_id: impl ToString) -> Option<i64> {
    let organ_id = organ_id.to_string();
    let organ_id = organ_id.trim();
    if organ_id.is_empty() {
        return None;
    }
    if organ_id.eq_ignore_ascii_case("local-dev") {
        return Some(DEFAULT_LOCAL_ORGAN_ID);
    }
    organ_id.parse::<i64>().ok().filter(|value| *value > 0)
}
