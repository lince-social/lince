use nucleus::place::Place;
use sqlx::{Row, SqlitePool};

use crate::StoreError;
mod projection;
use nucleus::vocabulary::{LocationCandidate, SAVED_LOCATION_ROLE_UID, SavedLocation};
pub use projection::{saved_locations_for_person, saved_locations_for_person_among};
use std::collections::{HashMap, HashSet};

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
            Some(Place {
                lat: r.get::<Option<f64>, _>("lat")?,
                lon: r.get::<Option<f64>, _>("lon")?,
            })
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

pub async fn saved_locations(
    pool: &SqlitePool,
    visible: Option<&HashSet<String>>,
) -> Result<HashMap<String, SavedLocation>, StoreError> {
    saved_locations_scoped(pool, visible, None).await
}

async fn saved_locations_scoped(
    pool: &SqlitePool,
    visible: Option<&HashSet<String>>,
    roots: Option<&[String]>,
) -> Result<HashMap<String, SavedLocation>, StoreError> {
    if roots.is_some_and(|roots| roots.len() > 10_000) {
        return Err(StoreError::Protocol(
            "Choose at most 10000 location Records".into(),
        ));
    }
    let ids = roots
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| StoreError::Protocol(error.to_string()))?;
    let roots = roots.map(|roots| roots.iter().map(String::as_str).collect::<HashSet<_>>());
    let mut out = HashMap::<String, SavedLocation>::new();
    let mut direct = HashMap::new();
    let mut all_direct = HashSet::new();
    let mut candidate_counts = HashMap::<String, usize>::new();
    for row in sqlx::query("SELECT r.uid,r.head,r.slug,p.lat,p.lon FROM record r LEFT JOIN place p ON p.uid=r.place_uid WHERE r.deleted_at IS NULL AND (?1 IS NULL OR r.uid IN (SELECT value FROM json_each(?1)) OR r.uid IN (SELECT a.object_uid FROM record_assertion a JOIN vocabulary_role_concept b ON b.concept_uid=a.predicate_uid AND b.role_uid=?2 WHERE a.subject_uid IN (SELECT value FROM json_each(?1)) AND a.retracted_at IS NULL AND a.role='ordinary' AND a.object_uid IS NOT NULL AND a.quantity_mantissa IS NULL AND a.unit_uid IS NULL))")
        .bind(&ids).bind(SAVED_LOCATION_ROLE_UID).fetch_all(pool).await?
    {
        let uid: String = row.get("uid");
        let coordinates = row.get::<Option<f64>, _>("lat").zip(row.get::<Option<f64>, _>("lon"))
            .filter(|(lat, lon)| lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(lat) && (-180.0..=180.0).contains(lon));
        if coordinates.is_some() {
            all_direct.insert(uid.clone());
            candidate_counts.insert(uid.clone(), 1);
        }
        if visible.is_some_and(|visible| !visible.contains(&uid)) {
            continue;
        }
        if roots.as_ref().is_none_or(|roots| roots.contains(uid.as_str())) {
        out.insert(uid.clone(), SavedLocation {
            record_uid: uid.clone(), candidates: Vec::new(), selected: None,
            explicit: false, unavailable: false, revision: 0,
        });
        }
        if let Some((latitude, longitude)) = coordinates {
            direct.insert(uid.clone(), LocationCandidate {
                assertion_uid: None, record_uid: uid, head: row.get("head"), slug: row.get("slug"), latitude, longitude, radius_metres: None,
            });
        }
    }
    for (uid, candidate) in &direct {
        if let Some(location) = out.get_mut(uid) {
            location.candidates.push(candidate.clone());
        }
    }
    for row in sqlx::query("SELECT a.uid,a.subject_uid,a.object_uid FROM record_assertion a JOIN vocabulary_role_concept b ON b.concept_uid=a.predicate_uid AND b.role_uid=?1 WHERE a.retracted_at IS NULL AND a.role='ordinary' AND a.object_uid IS NOT NULL AND a.quantity_mantissa IS NULL AND a.unit_uid IS NULL AND (?2 IS NULL OR a.subject_uid IN (SELECT value FROM json_each(?2))) ORDER BY a.subject_uid,a.uid")
        .bind(SAVED_LOCATION_ROLE_UID).bind(&ids).fetch_all(pool).await?
    {
        let subject: String = row.get("subject_uid");
        let object: String = row.get("object_uid");
        if all_direct.contains(&object) { *candidate_counts.entry(subject.clone()).or_default() += 1; }
        if let Some(location) = out.get_mut(&subject)
            && let Some(target) = direct.get(&object)
        {
            let mut target = target.clone();
            target.assertion_uid = Some(row.get("uid"));
            location.candidates.push(target);
        }
    }
    for row in
        sqlx::query("SELECT record_uid,source,assertion_uid,revision FROM record_location_choice WHERE (?1 IS NULL OR record_uid IN (SELECT value FROM json_each(?1)))").bind(&ids)
            .fetch_all(pool)
            .await?
    {
        let uid: String = row.get("record_uid");
        if let Some(location) = out.get_mut(&uid) {
            location.explicit = true;
            location.revision = row.get("revision");
            let assertion: Option<String> = row.get("assertion_uid");
            location.selected = location
                .candidates
                .iter()
                .find(|candidate| candidate.assertion_uid == assertion)
                .cloned();
            location.unavailable = location.selected.is_none();
        }
    }
    for row in sqlx::query(
        "SELECT record_uid,value,clock FROM record_property WHERE property='location.source' AND (?1 IS NULL OR record_uid IN (SELECT value FROM json_each(?1)))",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?
    {
        let uid: String = row.get("record_uid");
        if let Some(location) = out.get_mut(&uid) {
            let value: serde_json::Value = serde_json::from_str(row.get("value"))
                .map_err(|error| StoreError::Protocol(error.to_string()))?;
            let assertion = match &value["assertion"] {
                serde_json::Value::Null => None,
                serde_json::Value::String(assertion) => Some(assertion.as_str()),
                _ => return Err(StoreError::Protocol("Invalid saved location source".into())),
            };
            location.explicit = true;
            location.revision = row.get("clock");
            location.selected = location
                .candidates
                .iter()
                .find(|candidate| candidate.assertion_uid.as_deref() == assertion)
                .cloned();
            location.unavailable = location.selected.is_none();
        }
    }
    for location in out.values_mut() {
        if !location.explicit
            && location.candidates.len() == 1
            && candidate_counts.get(&location.record_uid) == Some(&1)
        {
            location.selected = location.candidates.first().cloned();
        }
    }
    Ok(out)
}

pub async fn validate_location_assertion_on(
    connection: &mut sqlx::SqliteConnection,
    predicate: &str,
    subject: &str,
    object: Option<&str>,
    has_quantity: bool,
    has_unit: bool,
) -> Result<(), StoreError> {
    let bound: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vocabulary_role_concept WHERE role_uid=? AND concept_uid=?)",
    )
    .bind(SAVED_LOCATION_ROLE_UID)
    .bind(predicate)
    .fetch_one(&mut *connection)
    .await?;
    if !bound {
        return Ok(());
    }
    if has_quantity || has_unit || object.is_none() || object == Some(subject) {
        return Err(StoreError::Protocol(
            "A saved location needs a distinct address Record and no quantity or unit".into(),
        ));
    }
    Ok(())
}
