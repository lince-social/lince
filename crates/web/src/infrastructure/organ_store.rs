use crate::domain::lince_package::slugify;
use persistence::write_coordinator::{SqlParameter, WriteCoordinatorHandle};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Sqlite};
use std::sync::Arc;

const DEFAULT_LOCAL_ORGAN_ID: &str = "local-dev";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Organ {
    pub id: String,
    pub name: String,
    pub base_url: String,
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
            "SELECT id, name, base_url FROM organ ORDER BY LOWER(name), id",
        )
        .fetch_all(&*self.db)
        .await
        .map_err(|error| format!("Nao consegui listar os orgaos: {error}"))?;
        organs.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(organs)
    }

    pub async fn get(&self, organ_id: &str) -> Result<Option<Organ>, String> {
        let organ_id = organ_id.trim();
        if organ_id.is_empty() {
            return Ok(None);
        }

        sqlx::query_as::<_, Organ>("SELECT id, name, base_url FROM organ WHERE id = ? LIMIT 1")
            .bind(organ_id)
            .fetch_optional(&*self.db)
            .await
            .map_err(|error| format!("Nao consegui carregar o orgao: {error}"))
    }

    pub async fn upsert(&self, organ: Organ) -> Result<Organ, String> {
        let organ = normalize_organ(organ)?;
        self.writer
            .execute_statement(
                "
                INSERT INTO organ(id, name, base_url)
                VALUES (?, ?, ?)
                ON CONFLICT(id) DO UPDATE
                SET name = excluded.name,
                    base_url = excluded.base_url
                "
                .to_string(),
                vec![
                    SqlParameter::Text(organ.id.clone()),
                    SqlParameter::Text(organ.name.clone()),
                    SqlParameter::Text(organ.base_url.clone()),
                ],
            )
            .await
            .map_err(|error| format!("Nao consegui salvar o orgao: {error}"))?;
        Ok(organ)
    }

    pub async fn delete(&self, organ_id: &str) -> Result<bool, String> {
        let organ_id = organ_id.trim();
        if organ_id.is_empty() {
            return Ok(false);
        }

        let outcome = self
            .writer
            .execute_statement(
                "DELETE FROM organ WHERE id = ?".to_string(),
                vec![SqlParameter::Text(organ_id.to_string())],
            )
            .await
            .map_err(|error| format!("Nao consegui apagar o orgao: {error}"))?;
        Ok(outcome.rows_affected > 0)
    }
}

pub fn is_default_local_organ(organ_id: &str) -> bool {
    organ_id.trim() == DEFAULT_LOCAL_ORGAN_ID
}

pub fn organ_requires_auth(organ: &Organ, local_auth_required: bool) -> bool {
    !is_default_local_organ(&organ.id) || local_auth_required
}

fn normalize_organ(organ: Organ) -> Result<Organ, String> {
    let name = organ.name.trim().to_string();
    let base_url = organ.base_url.trim().trim_end_matches('/').to_string();
    if name.is_empty() {
        return Err("Orgao precisa definir um name.".into());
    }
    if base_url.is_empty() {
        return Err("Orgao precisa definir um base_url.".into());
    }

    let id = {
        let raw_id = organ.id.trim();
        if raw_id.is_empty() {
            slugify(&name)
        } else {
            slugify(raw_id)
        }
    };

    if id.is_empty() {
        return Err("Orgao precisa definir um id valido.".into());
    }

    Ok(Organ { id, name, base_url })
}
