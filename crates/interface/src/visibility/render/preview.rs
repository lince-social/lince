use bevy::prelude::*;

use super::{Panel, label};
use crate::visibility::draft::prepared_policy;

pub(super) fn draw(world: &mut World, entity: Entity, panel: &Panel) {
    match prepared_policy(panel) {
        Ok(policy) => {
            if let Some(context) = &panel.context {
                for organ in &context.organs {
                    let decision = policy.decide(&organ.uid, organ.proximity);
                    let allowed = !organ.blocked && organ.proximity.is_some() && decision.allowed;
                    let numbered = |values: &[usize]| {
                        values
                            .iter()
                            .map(|value| (value + 1).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    label(
                        world,
                        entity,
                        format!(
                            "{} · {} · proximity {} · include [{}], exclude [{}]",
                            organ.label,
                            if allowed {
                                decision.radius_metres.map_or_else(
                                    || "Allowed · exact point".to_string(),
                                    |radius| {
                                        format!("Allowed · approximate area, radius {radius} m")
                                    },
                                )
                            } else {
                                "Denied by these rules".to_string()
                            },
                            organ
                                .proximity
                                .map_or_else(|| "unavailable".into(), |value| value.to_string()),
                            numbered(&decision.included_by),
                            numbered(&decision.excluded_by)
                        ),
                    );
                }
                let people = context.persons.iter().filter(|person| {
                    policy
                        .include
                        .iter()
                        .chain(&policy.exclude)
                        .any(|rule| rule.persons.contains(&person.uid))
                });
                let mut previews = 0;
                for (person, organ) in people
                    .flat_map(|person| context.organs.iter().map(move |organ| (person, organ)))
                    .take(64)
                {
                    let decision =
                        policy.decide_principal(&organ.uid, Some(&person.uid), organ.proximity);
                    let named = decision.included_by.iter().any(|index| {
                        policy
                            .include
                            .get(*index)
                            .is_some_and(|rule| !rule.persons.is_empty())
                    });
                    label(
                        world,
                        entity,
                        format!(
                            "Person {} via {} · {}",
                            person.label,
                            organ.label,
                            if !organ.blocked
                                && decision.allowed
                                && (organ.proximity.is_some() || named)
                            {
                                "Allowed by these rules"
                            } else {
                                "Denied by these rules"
                            }
                        ),
                    );
                    previews += 1;
                }
                if previews == 64 {
                    label(
                        world,
                        entity,
                        "Showing at most 64 Person/Organ combinations",
                    );
                }
            }
        }
        Err(error) => label(world, entity, error),
    }
}
