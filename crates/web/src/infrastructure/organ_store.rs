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

    pub async fn get(&self, organ_id: impl ToString) -> Result<Option<Organ>, String> {
        let Some(organ_id) = parse_organ_id(organ_id) else {
            return Ok(None);
        };

        sqlx::query_as::<_, Organ>("SELECT id, name, base_url FROM organ WHERE id = ? LIMIT 1")
            .bind(organ_id)
            .fetch_optional(&*self.db)
            .await
            .map_err(|error| format!("Nao consegui carregar o orgao: {error}"))
    }

    pub async fn create(&self, name: String, base_url: String) -> Result<Organ, String> {
        let (name, base_url) = normalize_organ_fields(name, base_url)?;
        let outcome = self
            .writer
            .execute_statement_returning_id(
                "INSERT INTO organ(name, base_url) VALUES (?, ?)".to_string(),
                vec![
                    SqlParameter::Text(name.clone()),
                    SqlParameter::Text(base_url.clone()),
                ],
            )
            .await
            .map_err(|error| format!("Nao consegui salvar o orgao: {error}"))?;
        let Some(id) = outcome.last_insert_rowid else {
            return Err("Nao consegui obter o id do orgao criado.".into());
        };
        Ok(Organ { id, name, base_url })
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
        Ok(Organ {
            id: organ_id,
            name,
            base_url,
        })
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
