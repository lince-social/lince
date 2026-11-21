use std::collections::{HashMap, HashSet};

use nucleus::vocabulary::{LocationCandidate, SAVED_LOCATION_ROLE_UID, SavedLocation};
use serde_json::{Value, from_str, to_string};
use sqlx::{Row, SqlitePool, query};

use crate::StoreError;

/// Reads saved-location candidates and choices, restricted to visible source and target records.
///
/// # Errors
/// Returns an error if a query or saved choice cannot be decoded.
pub async fn saved_locations(
    pool: &SqlitePool,
    visible: Option<&HashSet<String>>,
) -> Result<HashMap<String, SavedLocation>, StoreError> {
    saved_locations_scoped(pool, visible, None).await
}

#[derive(Default)]
struct CandidateCount {
    direct: bool,
    total: usize,
}

pub(super) async fn saved_locations_scoped(
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
        .map(to_string)
        .transpose()
        .map_err(|error| StoreError::Protocol(error.to_string()))?;
    let roots = roots.map(|roots| roots.iter().map(String::as_str).collect::<HashSet<_>>());
    let mut out = HashMap::<String, SavedLocation>::new();
    let mut direct = HashMap::new();
    let mut candidate_counts = HashMap::<String, CandidateCount>::new();
    for row in query("SELECT r.uid,r.head,r.slug,p.lat,p.lon FROM record r LEFT JOIN place p ON p.uid=r.place_uid WHERE r.deleted_at IS NULL AND (?1 IS NULL OR r.uid IN (SELECT value FROM json_each(?1)) OR r.uid IN (SELECT a.object_uid FROM record_assertion a JOIN vocabulary_role_concept b ON b.concept_uid=a.predicate_uid AND b.role_uid=?2 WHERE a.subject_uid IN (SELECT value FROM json_each(?1)) AND a.retracted_at IS NULL AND a.role='ordinary' AND a.object_uid IS NOT NULL AND a.quantity_mantissa IS NULL AND a.unit_uid IS NULL))")
        .bind(&ids).bind(SAVED_LOCATION_ROLE_UID).fetch_all(pool).await?
    {
        let uid: String = row.try_get("uid")?;
        let coordinates = row.try_get::<Option<f64>, _>("lat")?.zip(row.try_get::<Option<f64>, _>("lon")?)
            .filter(|(lat, lon)| lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(lat) && (-180.0..=180.0).contains(lon));
        if coordinates.is_some() {
            candidate_counts.insert(uid.clone(), CandidateCount { direct: true, total: 1 });
        }
        if visible.is_some_and(|visible| !visible.contains(&uid)) {
            continue;
        }
        let candidate = if let Some((latitude, longitude)) = coordinates {
            Some(LocationCandidate {
                assertion_uid: None, record_uid: uid.clone(), head: row.try_get("head")?, slug: row.try_get("slug")?, latitude, longitude, radius_metres: None,
            })
        } else {
            None
        };
        if roots.as_ref().is_none_or(|roots| roots.contains(uid.as_str())) {
            out.insert(uid.clone(), SavedLocation {
                record_uid: uid.clone(), candidates: candidate.iter().cloned().collect(), selected: None,
                explicit: false, unavailable: false, revision: 0,
            });
        }
        if let Some(candidate) = candidate {
            direct.insert(uid, candidate);
        }
    }
    for row in query("SELECT a.uid,a.subject_uid,a.object_uid FROM record_assertion a JOIN vocabulary_role_concept b ON b.concept_uid=a.predicate_uid AND b.role_uid=?1 WHERE a.retracted_at IS NULL AND a.role='ordinary' AND a.object_uid IS NOT NULL AND a.quantity_mantissa IS NULL AND a.unit_uid IS NULL AND (?2 IS NULL OR a.subject_uid IN (SELECT value FROM json_each(?2))) ORDER BY a.subject_uid,a.uid")
        .bind(SAVED_LOCATION_ROLE_UID).bind(&ids).fetch_all(pool).await?
    {
        let subject: &str = row.try_get("subject_uid")?;
        let object: &str = row.try_get("object_uid")?;
        // Count all viable targets before applying visibility to the output.
        if candidate_counts.get(object).is_some_and(|count| count.direct) {
            candidate_counts.entry(subject.to_owned()).or_default().total += 1;
        }
        if let Some(location) = out.get_mut(subject)
            && let Some(target) = direct.get(object)
        {
            location.candidates.push(LocationCandidate {
                assertion_uid: Some(row.try_get("uid")?),
                ..target.clone()
            });
        }
    }
    for row in
        query("SELECT record_uid,assertion_uid,revision FROM record_location_choice WHERE (?1 IS NULL OR record_uid IN (SELECT value FROM json_each(?1)))").bind(&ids)
            .fetch_all(pool)
            .await?
    {
        let uid: &str = row.try_get("record_uid")?;
        if let Some(location) = out.get_mut(uid) {
            choose(location, row.try_get("assertion_uid")?, row.try_get("revision")?);
        }
    }
    for row in query(
        "SELECT record_uid,value,clock FROM record_property WHERE property='location.source' AND (?1 IS NULL OR record_uid IN (SELECT value FROM json_each(?1)))",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?
    {
        let uid: &str = row.try_get("record_uid")?;
        if let Some(location) = out.get_mut(uid) {
            let value: Value = from_str(row.try_get("value")?)
                .map_err(|error| StoreError::Protocol(error.to_string()))?;
            let assertion = match value.get("assertion") {
                None | Some(Value::Null) => None,
                Some(Value::String(assertion)) => Some(assertion.as_str()),
                _ => return Err(StoreError::Protocol("Invalid saved location source".into())),
            };
            choose(location, assertion, row.try_get("clock")?);
        }
    }
    for location in out.values_mut() {
        if !location.explicit
            && location.candidates.len() == 1
            && candidate_counts
                .get(&location.record_uid)
                .is_some_and(|count| count.total == 1)
        {
            location.selected = location.candidates.first().cloned();
        }
    }
    Ok(out)
}

fn choose(location: &mut SavedLocation, assertion: Option<&str>, revision: i64) {
    location.explicit = true;
    location.revision = revision;
    location.selected = location
        .candidates
        .iter()
        .find(|candidate| candidate.assertion_uid.as_deref() == assertion)
        .cloned();
    location.unavailable = location.selected.is_none();
}
