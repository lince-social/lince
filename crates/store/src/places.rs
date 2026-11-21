//! Record coordinates and saved-location projections.

mod projection;
mod saved;

#[cfg(test)]
mod tests;

use nucleus::{new_uid, place::Place, vocabulary::SAVED_LOCATION_ROLE_UID};
use sqlx::{SqliteConnection, SqlitePool, query, query_as, query_scalar};

use crate::StoreError;

pub use projection::{saved_locations_for_person, saved_locations_for_person_among};
pub use saved::saved_locations;

/// Stores the coordinates and optional address of a place.
///
/// # Errors
/// Returns an error if the place cannot be written.
pub async fn create(
    pool: &SqlitePool,
    lat: f64,
    lon: f64,
    address: Option<&str>,
) -> Result<String, StoreError> {
    let uid = new_uid("pl");
    query("INSERT INTO place (uid, lat, lon, address) VALUES (?, ?, ?, ?)")
        .bind(&uid)
        .bind(lat)
        .bind(lon)
        .bind(address)
        .execute(pool)
        .await?;
    Ok(uid)
}

/// Reads a place with both coordinates, when present.
///
/// # Errors
/// Returns an error if the coordinates cannot be queried or decoded.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Place>, StoreError> {
    point(pool, "SELECT lat, lon FROM place WHERE uid = ?", uid).await
}

async fn point(
    pool: &SqlitePool,
    sql: &'static str,
    uid: &str,
) -> Result<Option<Place>, StoreError> {
    let coordinates: Option<(Option<f64>, Option<f64>)> =
        query_as(sql).bind(uid).fetch_optional(pool).await?;
    Ok(coordinates.and_then(|(lat, lon)| {
        Some(Place {
            lat: lat?,
            lon: lon?,
        })
    }))
}

/// Assigns an existing place to a record.
///
/// # Errors
/// Returns an error if the update fails.
pub async fn set_record_place(
    pool: &SqlitePool,
    record_uid: &str,
    place_uid: &str,
) -> Result<(), StoreError> {
    query("UPDATE record SET place_uid = ? WHERE uid = ?")
        .bind(place_uid)
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Reads the coordinates of a record's directly assigned place, when present.
///
/// # Errors
/// Returns an error if the place cannot be queried or decoded.
pub async fn of_record(pool: &SqlitePool, record_uid: &str) -> Result<Option<Place>, StoreError> {
    point(pool, "SELECT p.lat, p.lon FROM record r LEFT JOIN place p ON p.uid = r.place_uid WHERE r.uid = ?", record_uid).await
}

/// Requires a saved-location assertion to link distinct records without a quantity or unit.
///
/// # Errors
/// Returns an error for an invalid saved-location assertion or a failed role lookup.
pub async fn validate_location_assertion_on(
    connection: &mut SqliteConnection,
    predicate: &str,
    subject: &str,
    object: Option<&str>,
    has_quantity: bool,
    has_unit: bool,
) -> Result<(), StoreError> {
    let bound: bool = query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vocabulary_role_concept WHERE role_uid=? AND concept_uid=?)",
    )
    .bind(SAVED_LOCATION_ROLE_UID)
    .bind(predicate)
    .fetch_one(&mut *connection)
    .await?;
    if bound && (has_quantity || has_unit || object.is_none() || object == Some(subject)) {
        return Err(StoreError::Protocol(
            "A saved location needs a distinct address Record and no quantity or unit".into(),
        ));
    }
    Ok(())
}
