use crate::StoreError;
use nucleus::sand_package::{Entry, Identity, Manifest, PAGE_SIZE, Package};
use sqlx::{Row, SqlitePool};

pub struct Stored {
    pub package: Package,
    pub public: bool,
    pub origin_verified: bool,
}

pub async fn list(
    pool: &SqlitePool,
    public_only: bool,
    offset: u32,
) -> Result<Vec<Entry>, StoreError> {
    let query = if public_only {
        "SELECT manifest, public, origin_verified FROM sand_package WHERE public = 1 ORDER BY origin, id, version DESC LIMIT ? OFFSET ?"
    } else {
        "SELECT manifest, public, origin_verified FROM sand_package ORDER BY origin, id, version DESC LIMIT ? OFFSET ?"
    };
    sqlx::query(query)
        .bind(i64::from(PAGE_SIZE + 1))
        .bind(i64::from(offset))
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            let manifest: Manifest = serde_json::from_str(&row.get::<String, _>("manifest"))
                .map_err(|error| StoreError::Decode(Box::new(error)))?;
            Ok(Entry {
                identity: manifest.identity,
                name: manifest.name,
                kind: manifest.kind,
                author: manifest.author,
                execution: manifest.execution,
                public: row.get("public"),
                origin_verified: row.get("origin_verified"),
            })
        })
        .collect()
}

pub async fn get(
    pool: &SqlitePool,
    identity: &Identity,
    public_only: bool,
) -> Result<Option<Stored>, StoreError> {
    sqlx::query("SELECT document, public, origin_verified FROM sand_package WHERE origin = ? AND id = ? AND version = ? AND (? = 0 OR public = 1)")
        .bind(&identity.origin).bind(&identity.id).bind(i64::from(identity.version)).bind(public_only)
        .fetch_optional(pool).await?
        .map(|row| {
            Ok(Stored { package: serde_json::from_str(&row.get::<String, _>("document"))
                .map_err(|error| StoreError::Decode(Box::new(error)))?, public: row.get("public"), origin_verified: row.get("origin_verified") })
        }).transpose()
}

pub async fn save(
    pool: &SqlitePool,
    package: &Package,
    verified: bool,
    received_from: Option<&str>,
) -> Result<bool, StoreError> {
    package.validate().map_err(StoreError::Protocol)?;
    let document =
        serde_json::to_string(package).map_err(|error| StoreError::Encode(Box::new(error)))?;
    let manifest = serde_json::to_string(&package.manifest)
        .map_err(|error| StoreError::Encode(Box::new(error)))?;
    let identity = &package.manifest.identity;
    let mut tx = crate::write_tx(pool).await?;
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT digest FROM sand_package WHERE origin = ? AND id = ? AND version = ?",
    )
    .bind(&identity.origin)
    .bind(&identity.id)
    .bind(i64::from(identity.version))
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(existing) = existing {
        return Ok(existing == package.digest);
    }
    sqlx::query("INSERT INTO sand_package (origin, id, version, document, manifest, digest, origin_verified, received_from) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&identity.origin).bind(&identity.id).bind(i64::from(identity.version)).bind(document).bind(manifest).bind(&package.digest).bind(verified).bind(received_from)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn next_version(pool: &SqlitePool, origin: &str, id: &str) -> Result<i64, StoreError> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM sand_package WHERE origin = ? AND id = ?",
    )
    .bind(origin)
    .bind(id)
    .fetch_one(pool)
    .await
}

pub async fn set_public(
    pool: &SqlitePool,
    identity: &Identity,
    public: bool,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE sand_package SET public = ? WHERE origin = ? AND id = ? AND version = ?")
        .bind(public)
        .bind(&identity.origin)
        .bind(&identity.id)
        .bind(i64::from(identity.version))
        .execute(pool)
        .await?;
    Ok(())
}
