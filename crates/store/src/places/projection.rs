use crate::StoreError;
use nucleus::{visibility::Data, vocabulary::SavedLocation};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

pub async fn saved_locations_for_person(
    pool: &SqlitePool,
    visible: Option<&HashSet<String>>,
    person: Option<&str>,
) -> Result<HashMap<String, SavedLocation>, StoreError> {
    let mut locations = super::saved_locations(pool, visible).await?;
    let Some(person) = person else {
        return Ok(locations);
    };
    let decisions = crate::data_visibility::decisions_for_person(pool, person, Data::Place).await?;
    let record_decisions =
        crate::data_visibility::decisions_for_person(pool, person, Data::Record).await?;
    for (record, location) in &mut locations {
        let selected = location.selected.as_ref().map(|candidate| {
            (
                candidate.record_uid.clone(),
                candidate.assertion_uid.clone(),
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
                let Some(point) = nucleus::place::approximate(
                    nucleus::place::Place {
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
            selected.and_then(|selected| {
                location
                    .candidates
                    .iter()
                    .find(|candidate| {
                        candidate.record_uid == selected.0 && candidate.assertion_uid == selected.1
                    })
                    .cloned()
            })
        };
        location.unavailable = location.selected.is_none();
    }
    Ok(locations)
}
