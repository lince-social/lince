//! Place rows (blueprint IX): the stored side of the place Instinct.

use nucleus::place::Place;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub async fn create(
    pool: &SqlitePool,
    lat: f64,
    lon: f64,
    address: Option<&str>,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("pl");
    sqlx::query("INSERT INTO place (uid, lat, lon, address) VALUES (?, ?, ?, ?)")
        .bind(&uid)
        .bind(lat)
        .bind(lon)
        .bind(address)
        .execute(pool)
        .await?;
    Ok(uid)
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Place>, StoreError> {
    Ok(sqlx::query("SELECT lat, lon FROM place WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .and_then(|r| {
            Some(Place { lat: r.get::<Option<f64>, _>("lat")?, lon: r.get::<Option<f64>, _>("lon")? })
        }))
}

pub async fn set_record_place(
    pool: &SqlitePool,
    record_uid: &str,
    place_uid: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET place_uid = ? WHERE uid = ?")
        .bind(place_uid)
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// The place of a record, if it has one.
pub async fn of_record(pool: &SqlitePool, record_uid: &str) -> Result<Option<Place>, StoreError> {
    let place_uid: Option<String> = sqlx::query("SELECT place_uid FROM record WHERE uid = ?")
        .bind(record_uid)
        .fetch_optional(pool)
        .await?
        .and_then(|r| r.get("place_uid"));
    match place_uid {
        Some(uid) => get(pool, &uid).await,
        None => Ok(None),
    }
}
