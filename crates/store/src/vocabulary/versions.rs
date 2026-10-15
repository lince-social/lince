use super::*;
use nucleus::vocabulary::{DefinitionVersion, Dictionary};
use serde_json::{Value, json};

pub async fn definitions_on(
    connection: &mut SqliteConnection,
    concept: &str,
) -> Result<Vec<DefinitionVersion>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM concept_definition_version WHERE concept_uid=? ORDER BY revision",
    )
    .bind(concept)
    .fetch_all(connection)
    .await?
    .into_iter()
    .map(|row| DefinitionVersion {
        concept_uid: row.get("concept_uid"),
        revision: row.get("revision"),
        definition: row.get("definition"),
        created_at: row.get("created_at"),
    })
    .collect())
}

pub async fn definition_versions(
    pool: &SqlitePool,
) -> Result<HashMap<String, Vec<DefinitionVersion>>, StoreError> {
    let mut versions: HashMap<String, Vec<DefinitionVersion>> = HashMap::new();
    for row in sqlx::query("SELECT * FROM concept_definition_version ORDER BY concept_uid,revision")
        .fetch_all(pool)
        .await?
    {
        let concept_uid: String = row.get("concept_uid");
        versions
            .entry(concept_uid.clone())
            .or_default()
            .push(DefinitionVersion {
                concept_uid,
                revision: row.get("revision"),
                definition: row.get("definition"),
                created_at: row.get("created_at"),
            });
    }
    Ok(versions)
}

pub async fn revise_definition_on(
    connection: &mut SqliteConnection,
    concept: &str,
    expected_revision: i64,
    definition: &str,
) -> Result<i64, StoreError> {
    let definition = definition.trim();
    if definition.is_empty() || definition.len() > 16384 {
        return Err(invalid("Describe the meaning in 1–16384 bytes"));
    }
    let current: Option<(i64,String)> = sqlx::query_as("SELECT revision,definition FROM concept_definition_version WHERE concept_uid=? ORDER BY revision DESC LIMIT 1")
        .bind(concept).fetch_optional(&mut *connection).await?;
    let (revision, previous) = current.ok_or_else(|| invalid("Choose a saved source Concept"))?;
    if expected_revision != revision {
        return Err(invalid("The definition changed; refresh before saving"));
    }
    if previous == definition {
        return Ok(revision);
    }
    if revision >= 256 {
        return Err(invalid(
            "This Concept has 256 definition versions; create a distinct meaning",
        ));
    }
    sqlx::query("INSERT INTO concept_definition_version VALUES(?,?,?,?)")
        .bind(concept)
        .bind(revision + 1)
        .bind(definition)
        .bind(nucleus::execution::now().to_rfc3339())
        .execute(&mut *connection)
        .await?;
    sqlx::query("UPDATE lingua SET revision=revision+1 WHERE uid IN (SELECT lingua_uid FROM lingua_concept WHERE concept_uid=?)")
        .bind(concept).execute(connection).await?;
    Ok(revision + 1)
}

pub(super) fn validate_definitions(dictionary: &Dictionary) -> Result<(), StoreError> {
    if dictionary.definitions.len() > 256 {
        return Err(invalid("Too many definition versions"));
    }
    for (index, version) in dictionary.definitions.iter().enumerate() {
        if version.concept_uid != dictionary.source.concept_uid
            || version.revision != index as i64 + 1
            || version.definition.len() > 16384
            || version.created_at.is_empty()
            || version.created_at.len() > 64
            || index == 0 && version.definition != dictionary.source.definition
        {
            return Err(invalid("Invalid saved definition history"));
        }
    }
    Ok(())
}

pub(super) async fn save_definitions_on(
    connection: &mut SqliteConnection,
    dictionary: &Dictionary,
) -> Result<(), StoreError> {
    for version in &dictionary.definitions {
        let previous: Option<String> = sqlx::query_scalar(
            "SELECT definition FROM concept_definition_version WHERE concept_uid=? AND revision=?",
        )
        .bind(&version.concept_uid)
        .bind(version.revision)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(previous) = previous {
            if previous != version.definition {
                return Err(invalid(
                    "An incoming definition conflicts with a saved version",
                ));
            }
        } else {
            sqlx::query("INSERT INTO concept_definition_version VALUES(?,?,?,?)")
                .bind(&version.concept_uid)
                .bind(version.revision)
                .bind(&version.definition)
                .bind(&version.created_at)
                .execute(&mut *connection)
                .await?;
        }
    }
    Ok(())
}

pub async fn record_history_on(
    connection: &mut SqliteConnection,
    record: &str,
    offset: u32,
) -> Result<Value, StoreError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM record_content_version WHERE record_uid=?")
            .bind(record)
            .fetch_one(&mut *connection)
            .await?;
    let mut versions = Vec::new();
    for row in sqlx::query("SELECT revision,content,saved_at FROM record_content_version WHERE record_uid=? ORDER BY revision DESC LIMIT 30 OFFSET ?")
        .bind(record).bind(offset).fetch_all(connection).await? {
        let content: String=row.get("content");
        let content: Value=serde_json::from_str(&content).map_err(|error| invalid(&error.to_string()))?;
        versions.push(json!({"revision":row.get::<i64,_>("revision"),"saved_at":row.get::<String,_>("saved_at"),"content":content}));
    }
    Ok(json!({"record":record,"count":count,"offset":offset,"versions":versions}))
}
