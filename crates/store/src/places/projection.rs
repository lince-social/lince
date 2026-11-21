use std::collections::{HashMap, HashSet};

use nucleus::{
    place::{Place, approximate},
    visibility::Data,
    vocabulary::SavedLocation,
};
use sqlx::SqlitePool;

use super::{saved::saved_locations_scoped, saved_locations};
use crate::{StoreError, data_visibility::decisions_for_person};

/// Reads saved locations, applying the person's visibility and location precision when supplied.
///
/// # Errors
/// Returns an error if locations, choices, or audience policies cannot be read.
pub async fn saved_locations_for_person(
    pool: &SqlitePool,
    visible: Option<&HashSet<String>>,
    person: Option<&str>,
) -> Result<HashMap<String, SavedLocation>, StoreError> {
    project(pool, saved_locations(pool, visible).await?, person).await
}

/// Reads a person's saved locations for the supplied root records.
///
/// # Errors
/// Returns an error for an oversized root selection or a failed location or policy read.
pub async fn saved_locations_for_person_among(
    pool: &SqlitePool,
    visible: Option<&HashSet<String>>,
    person: Option<&str>,
    roots: &[String],
) -> Result<HashMap<String, SavedLocation>, StoreError> {
    project(
        pool,
        saved_locations_scoped(pool, visible, Some(roots)).await?,
        person,
    )
    .await
}

async fn project(
    pool: &SqlitePool,
    mut locations: HashMap<String, SavedLocation>,
    person: Option<&str>,
) -> Result<HashMap<String, SavedLocation>, StoreError> {
    let Some(person) = person else {
        return Ok(locations);
    };
    let decisions = decisions_for_person(pool, person, Data::Place).await?;
    let record_decisions = decisions_for_person(pool, person, Data::Record).await?;
    for (record, location) in &mut locations {
        let selected = location.selected.as_ref().map(|candidate| {
            (
                candidate.record_uid.as_str(),
                candidate.assertion_uid.as_deref(),
            )
        });
        let source = decisions.get(record);
        let record_allowed = record_decisions
            .get(record)
            .is_none_or(|decision| decision.allowed);
        location.candidates.retain_mut(|candidate| {
            let target = decisions.get(&candidate.record_uid);
            if !record_allowed
                || record_decisions
                    .get(&candidate.record_uid)
                    .is_some_and(|decision| !decision.allowed)
                || source.is_none_or(|decision| !decision.allowed)
                || target.is_none_or(|decision| !decision.allowed)
            {
                return false;
            }
            let radius = source
                .and_then(|decision| decision.radius_metres)
                .max(target.and_then(|decision| decision.radius_metres))
                .max(candidate.radius_metres);
            if let Some(radius) = radius {
                let Some(point) = approximate(
                    Place {
                        lat: candidate.latitude,
                        lon: candidate.longitude,
                    },
                    radius,
                ) else {
                    return false;
                };
                candidate.latitude = point.lat;
                candidate.longitude = point.lon;
                candidate.radius_metres = Some(radius);
                candidate.head = "Approximate area".into();
                candidate.slug = None;
            }
            true
        });
        location.selected = if !location.explicit && location.candidates.len() == 1 {
            location.candidates.first().cloned()
        } else {
            selected.and_then(|(record, assertion)| {
                location
                    .candidates
                    .iter()
                    .find(|candidate| {
                        candidate.record_uid == record
                            && candidate.assertion_uid.as_deref() == assertion
                    })
                    .cloned()
            })
        };
        location.unavailable = location.selected.is_none();
    }
    Ok(locations)
}
