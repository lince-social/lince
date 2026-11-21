use super::{Grant, Visit};
use bevy::prelude::Entity;
use engine::area_transition::RecordChanges;
use std::collections::HashMap;

pub(super) fn merge(
    grants: &HashMap<Entity, Grant>,
    contributions: &[(Entity, bool)],
) -> (RecordChanges, bool) {
    let entering_quantity = contributions
        .iter()
        .filter(|(_, inside)| *inside)
        .filter_map(|(area, _)| grants.get(area))
        .any(|grant| grant.area.changes.enter.quantity.is_some());
    let mut changes = RecordChanges::default();
    let conflict = contributions.iter().any(|(area, inside)| {
        let Some(grant) = grants.get(area) else {
            return true;
        };
        let change = grant.area.changes.crossing(*inside);
        if !inside && entering_quantity && change.quantity.is_some() {
            let mut exit = change.clone();
            exit.quantity = None;
            !changes.merge(&exit)
        } else {
            !changes.merge(change)
        }
    });
    (changes, conflict)
}

pub(super) fn defer(
    grants: &mut HashMap<Entity, Grant>,
    contributions: &[(Entity, bool)],
    uid: &str,
) {
    for (entity, inside) in contributions {
        if let Some(grant) = grants.get_mut(entity) {
            grant.visits.insert(
                uid.to_owned(),
                Visit {
                    inside: !inside,
                    eligible: !inside,
                    immune: false,
                    deferred: true,
                },
            );
        }
    }
}
