use crate::StoreError;
use nucleus::knowledge::{Archive, CACHE_LIMIT, DEPENDENCY_LIMIT, ITEM_LIMIT, Item};
use sqlx::{Row, SqliteConnection, SqlitePool};
mod permanent;
pub use permanent::*;

fn invalid(message: &str) -> StoreError {
    StoreError::Protocol(message.into())
}

fn bounded(value: &str, limit: usize, required: bool) -> Result<(), StoreError> {
    if value.len() > limit || value.contains('\0') || (required && value.trim().is_empty()) {
        return Err(invalid(
            "Knowledge content exceeds its limits or has a missing identifier",
        ));
    }
    Ok(())
}

pub fn validate_item(item: &Item) -> Result<(), StoreError> {
    nucleus::knowledge::validate_assets(item).map_err(invalid)?;
    for value in [&item.source, &item.id, &item.revision] {
        bounded(value, 1024, true)?;
    }
    bounded(&item.title, 65536, true)?;
    bounded(&item.uri, 4096, true)?;
    if !matches!(
        item.content_type.as_str(),
        "text/plain" | "text/markdown" | "text/x-wiki" | "application/json"
    ) || item.claims.len() > 4096
        || item.meanings.len() > 256
        || item.dependencies.len() > DEPENDENCY_LIMIT
        || item.assets.len() > 128
        || item.credits.len() > 256
    {
        return Err(invalid(
            "Choose a supported knowledge item within the content and dependency limits",
        ));
    }
    let mut claims = std::collections::HashSet::new();
    for claim in &item.claims {
        bounded(&claim.id, 1024, true)?;
        bounded(&claim.property, 1024, true)?;
        if claim.source != item.source
            || claim.revision.trim().is_empty()
            || claim.revision.len() > 1024
            || !claims.insert(&claim.id)
        {
            return Err(invalid(
                "Claims must retain distinct identities and the item's source revision",
            ));
        }
    }
    let mut meanings = std::collections::HashSet::new();
    for meaning in &item.meanings {
        bounded(&meaning.id, 1024, true)?;
        bounded(&meaning.revision, 1024, true)?;
        bounded(&meaning.definition, 65536, false)?;
        if !meanings.insert(&meaning.id) {
            return Err(invalid(
                "External meanings must have distinct source identities",
            ));
        }
    }
    for reference in &item.dependencies {
        for value in [&reference.source, &reference.id, &reference.uri] {
            bounded(value, 4096, true)?;
        }
    }
    let mut assets = std::collections::HashSet::new();
    for asset in &item.assets {
        bounded(&asset.id, 1024, true)?;
        bounded(&asset.uri, 4096, true)?;
        if !assets.insert(&asset.id) || asset.credits.len() > 256 {
            return Err(invalid(
                "Asset identities and attribution must fit within the limits",
            ));
        }
    }
    if serde_json::to_vec(item)
        .map_err(|error| invalid(&error.to_string()))?
        .len()
        > ITEM_LIMIT
    {
        return Err(invalid("A knowledge item can contain at most 4 MiB"));
    }
    Ok(())
}

pub fn read_archive(raw: &str) -> Result<Archive, StoreError> {
    if raw.len() > ITEM_LIMIT {
        return Err(invalid(
            "A local knowledge archive can contain at most 4 MiB",
        ));
    }
    let archive: Archive = serde_json::from_str(raw)
        .map_err(|error| invalid(&format!("Invalid knowledge archive: {error}")))?;
    if archive.format != 1 || archive.source.online || archive.items.len() > 256 {
        return Err(invalid(
            "Choose a supported offline archive with at most 256 items",
        ));
    }
    bounded(&archive.source.id, 128, true)?;
    bounded(&archive.source.name, 256, true)?;
    bounded(&archive.source.uri, 4096, true)?;
    let mut ids = std::collections::HashSet::new();
    for item in &archive.items {
        validate_item(item)?;
        if item.source != archive.source.id || !ids.insert(&item.id) {
            return Err(invalid(
                "Archive items must retain distinct identifiers from the same source",
            ));
        }
    }
    Ok(archive)
}

pub async fn install_archive_on(
    connection: &mut SqliteConnection,
    owner: &str,
    archive: &Archive,
) -> Result<String, StoreError> {
    let raw = serde_json::to_string(archive).map_err(|error| invalid(&error.to_string()))?;
    if let Some(uid) =
        sqlx::query_scalar("SELECT uid FROM knowledge_source WHERE owner_key=? AND package=?")
            .bind(owner)
            .bind(&raw)
            .fetch_optional(&mut *connection)
            .await?
    {
        return Ok(uid);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_source WHERE owner_key=?")
        .bind(owner)
        .fetch_one(&mut *connection)
        .await?;
    if count >= 16 {
        return Err(invalid(
            "Keep at most 16 offline knowledge sources per Person",
        ));
    }
    let uid = nucleus::new_uid("ks");
    sqlx::query(
        "INSERT INTO knowledge_source(uid,owner_key,name,package,created_at) VALUES(?,?,?,?,?)",
    )
    .bind(&uid)
    .bind(owner)
    .bind(&archive.source.name)
    .bind(raw)
    .bind(nucleus::execution::now().to_rfc3339())
    .execute(connection)
    .await?;
    Ok(uid)
}

pub async fn archives(
    pool: &SqlitePool,
    owner: &str,
) -> Result<Vec<nucleus::knowledge::Source>, StoreError> {
    let rows =
        sqlx::query("SELECT uid,json_extract(package,'$.source') AS source FROM knowledge_source WHERE owner_key=? ORDER BY name,uid")
            .bind(owner)
            .fetch_all(pool)
            .await?;
    let mut sources = Vec::new();
    for row in rows {
        let mut source: nucleus::knowledge::Source =
            serde_json::from_str(&row.get::<String, _>("source"))
                .map_err(|error| invalid(&error.to_string()))?;
        source.id = row.get("uid");
        sources.push(source);
    }
    Ok(sources)
}

pub async fn archive(
    pool: &SqlitePool,
    owner: &str,
    source: &str,
) -> Result<Option<Archive>, StoreError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT package FROM knowledge_source WHERE uid=? AND owner_key=?")
            .bind(source)
            .bind(owner)
            .fetch_optional(pool)
            .await?;
    raw.map(|raw| {
        let mut archive = read_archive(&raw)?;
        let original = archive.source.id.clone();
        archive.source.id = source.into();
        for item in &mut archive.items {
            item.source = source.into();
            for claim in &mut item.claims {
                claim.source = source.into();
            }
            for reference in &mut item.dependencies {
                if reference.source == original {
                    reference.source = source.into();
                }
            }
        }
        Ok(archive)
    })
    .transpose()
}

pub async fn cache_on(
    connection: &mut SqliteConnection,
    owner: &str,
    item: &Item,
) -> Result<(), StoreError> {
    validate_item(item)?;
    let raw = serde_json::to_string(item).map_err(|error| invalid(&error.to_string()))?;
    if let Some(saved)=sqlx::query_scalar::<_,String>("SELECT content FROM knowledge_cache WHERE owner_key=? AND source_uid=? AND item_id=? AND revision=?")
        .bind(owner).bind(&item.source).bind(&item.id).bind(&item.revision).fetch_optional(&mut *connection).await? {
        if saved!=raw {return Err(invalid("A source changed content without changing its revision"));}
        return Ok(());
    }
    let now = nucleus::execution::now().to_rfc3339();
    sqlx::query("INSERT INTO knowledge_cache(owner_key,source_uid,item_id,revision,content,fetched_at,accessed_at) VALUES(?,?,?,?,?,?,?)")
        .bind(owner).bind(&item.source).bind(&item.id).bind(&item.revision).bind(raw).bind(&now).bind(&now).execute(&mut *connection).await?;
    loop {
        let (personal,total,personal_count,total_count):(i64,i64,i64,i64)=sqlx::query_as("SELECT COALESCE(SUM(CASE WHEN owner_key=? THEN length(CAST(content AS BLOB)) ELSE 0 END),0),COALESCE(SUM(length(CAST(content AS BLOB))),0),SUM(CASE WHEN owner_key=? THEN 1 ELSE 0 END),COUNT(*) FROM knowledge_cache").bind(owner).bind(owner).fetch_one(&mut *connection).await?;
        if personal <= CACHE_LIMIT as i64
            && total <= 8 * CACHE_LIMIT as i64
            && personal_count <= 256
            && total_count <= 2048
        {
            break;
        }
        if personal > CACHE_LIMIT as i64 || personal_count > 256 {
            sqlx::query("DELETE FROM knowledge_cache WHERE sequence=(SELECT sequence FROM knowledge_cache WHERE owner_key=? ORDER BY accessed_at,sequence LIMIT 1)").bind(owner).execute(&mut *connection).await?;
        } else {
            sqlx::query("DELETE FROM knowledge_cache WHERE sequence=(SELECT sequence FROM knowledge_cache ORDER BY accessed_at,sequence LIMIT 1)").execute(&mut *connection).await?;
        }
    }
    Ok(())
}

pub async fn cached_on(
    connection: &mut SqliteConnection,
    owner: &str,
    source: &str,
    item: &str,
) -> Result<Option<Item>, StoreError> {
    let row=sqlx::query("SELECT sequence,content FROM knowledge_cache WHERE owner_key=? AND source_uid=? AND item_id=? ORDER BY sequence DESC LIMIT 1")
        .bind(owner).bind(source).bind(item).fetch_optional(&mut *connection).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    sqlx::query("UPDATE knowledge_cache SET accessed_at=? WHERE sequence=?")
        .bind(nucleus::execution::now().to_rfc3339())
        .bind(row.get::<i64, _>("sequence"))
        .execute(&mut *connection)
        .await?;
    let item: Item = serde_json::from_str(&row.get::<String, _>("content"))
        .map_err(|error| invalid(&error.to_string()))?;
    validate_item(&item)?;
    Ok(Some(item))
}

pub async fn clear_cache_on(
    connection: &mut SqliteConnection,
    owner: &str,
    source: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM knowledge_cache WHERE owner_key=? AND (? IS NULL OR source_uid=?)")
        .bind(owner)
        .bind(source)
        .bind(source)
        .execute(connection)
        .await?;
    Ok(())
}

pub async fn check_review_capacity_on(
    connection: &mut SqliteConnection,
    owner: &str,
    bytes: usize,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM knowledge_review WHERE applied IS NULL AND created_at<?")
        .bind((nucleus::execution::now() - chrono::Duration::hours(1)).to_rfc3339())
        .execute(&mut *connection)
        .await?;
    let (personal,total,personal_bytes,total_bytes):(i64,i64,i64,i64)=sqlx::query_as("SELECT COALESCE(SUM(CASE WHEN owner_key=? THEN 1 ELSE 0 END),0),COUNT(*),COALESCE(SUM(CASE WHEN owner_key=? THEN length(CAST(content AS BLOB)) ELSE 0 END),0),COALESCE(SUM(length(CAST(content AS BLOB))),0) FROM knowledge_review WHERE applied IS NULL").bind(owner).bind(owner).fetch_one(connection).await?;
    if personal >= 16
        || total >= 128
        || personal_bytes + bytes as i64 > CACHE_LIMIT as i64
        || total_bytes + bytes as i64 > 8 * CACHE_LIMIT as i64
    {
        return Err(invalid(
            "Pending knowledge reviews exceed their count or storage limit; save or clear temporary reviews first",
        ));
    }
    Ok(())
}

pub async fn map_meaning_on(
    connection: &mut SqliteConnection,
    owner: &str,
    source: &str,
    external: &str,
    kind: nucleus::knowledge::MeaningKind,
    concept: &str,
) -> Result<nucleus::knowledge::Mapping, StoreError> {
    bounded(external, 1024, true)?;
    let raw:Option<String>=sqlx::query_scalar("SELECT m.value FROM knowledge_cache c,json_each(c.content,'$.meanings') m WHERE c.owner_key=? AND c.source_uid=? AND json_extract(m.value,'$.id')=? ORDER BY c.sequence DESC LIMIT 1").bind(owner).bind(source).bind(external).fetch_optional(&mut *connection).await?;
    let meaning: nucleus::knowledge::Meaning = serde_json::from_str(&raw.ok_or_else(|| {
        invalid("Read the external meaning before mapping it to a local Concept")
    })?)
    .map_err(|error| invalid(&error.to_string()))?;
    let external_hash = fingerprint(
        serde_json::to_string(&meaning)
            .map_err(|error| invalid(&error.to_string()))?
            .as_bytes(),
    );
    let dictionary = crate::vocabulary::dictionary_on(connection, concept).await?;
    let version = dictionary
        .definitions
        .last()
        .filter(|version| !version.definition.trim().is_empty())
        .ok_or_else(|| invalid("Save a definition for the local meaning before mapping it"))?;
    let kind_name = match kind {
        nucleus::knowledge::MeaningKind::Predicate => "predicate",
        nucleus::knowledge::MeaningKind::Unit => "unit",
    };
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_mapping WHERE owner_key=? AND NOT(source_uid=? AND external_id=? AND kind=?)")
        .bind(owner)
        .bind(source).bind(external).bind(kind_name)
        .fetch_one(&mut *connection)
        .await?;
    if count >= 4096 {
        return Err(invalid(
            "Keep at most 4096 explicit knowledge meaning mappings",
        ));
    }
    sqlx::query("INSERT INTO knowledge_mapping(owner_key,source_uid,external_id,kind,concept_uid,definition_revision,external_revision,external_hash) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(owner_key,source_uid,external_id,kind) DO UPDATE SET concept_uid=excluded.concept_uid,definition_revision=excluded.definition_revision,external_revision=excluded.external_revision,external_hash=excluded.external_hash")
        .bind(owner).bind(source).bind(external).bind(kind_name).bind(concept).bind(version.revision).bind(&meaning.revision).bind(&external_hash).execute(connection).await?;
    Ok(nucleus::knowledge::Mapping {
        source: source.into(),
        external: external.into(),
        external_revision: meaning.revision,
        external_hash,
        kind,
        concept_uid: concept.into(),
        definition_revision: version.revision,
    })
}

pub async fn mappings_on(
    connection: &mut SqliteConnection,
    owner: &str,
    source: &str,
) -> Result<Vec<nucleus::knowledge::Mapping>, StoreError> {
    let rows=sqlx::query("SELECT * FROM knowledge_mapping WHERE owner_key=? AND source_uid=? ORDER BY external_id,kind").bind(owner).bind(source).fetch_all(connection).await?;
    Ok(rows
        .into_iter()
        .map(|row| nucleus::knowledge::Mapping {
            source: row.get("source_uid"),
            external: row.get("external_id"),
            external_revision: row.get("external_revision"),
            external_hash: row.get("external_hash"),
            kind: if row.get::<String, _>("kind") == "unit" {
                nucleus::knowledge::MeaningKind::Unit
            } else {
                nucleus::knowledge::MeaningKind::Predicate
            },
            concept_uid: row.get("concept_uid"),
            definition_revision: row.get("definition_revision"),
        })
        .collect())
}

pub async fn replace_archive_on(
    connection: &mut SqliteConnection,
    owner: &str,
    source: &str,
    archive: &Archive,
) -> Result<(), StoreError> {
    let raw: String =
        sqlx::query_scalar("SELECT package FROM knowledge_source WHERE uid=? AND owner_key=?")
            .bind(source)
            .bind(owner)
            .fetch_optional(&mut *connection)
            .await?
            .ok_or_else(|| invalid("Offline source unavailable"))?;
    let previous = read_archive(&raw)?;
    if previous.source.id != archive.source.id || previous.source.uri != archive.source.uri {
        return Err(invalid(
            "Keep the offline source identity and origin when replacing its archive",
        ));
    }
    for incoming in &archive.items {
        if previous.items.iter().any(|known| {
            known.id == incoming.id && known.revision == incoming.revision && known != incoming
        }) {
            return Err(invalid(
                "Change the item revision before replacing source content",
            ));
        }
    }
    sqlx::query("UPDATE knowledge_source SET name=?,package=? WHERE uid=? AND owner_key=?")
        .bind(&archive.source.name)
        .bind(serde_json::to_string(archive).map_err(|error| invalid(&error.to_string()))?)
        .bind(source)
        .bind(owner)
        .execute(connection)
        .await?;
    Ok(())
}
