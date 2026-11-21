// non-lean-yet
// Scope: enum Key, struct Parts, struct Heading, struct Retry.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 193, 236.

use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};
use std::borrow::Cow;

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Heading,
    Boundary(Option<String>, usize),
    Limit,
}

#[derive(Component, Default)]
struct Parts(RetainedChildren<Key>);

#[derive(Clone, PartialEq, Eq)]
struct Heading<'a> {
    uid: Cow<'a, str>,
    revision: i64,
    name: Cow<'a, str>,
    cancelled: bool,
}

impl Heading<'_> {
    fn into_owned(self) -> Heading<'static> {
        Heading {
            uid: self.uid.into_owned().into(),
            revision: self.revision,
            name: self.name.into_owned().into(),
            cancelled: self.cancelled,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Retry<'a> {
    schedule: Cow<'a, str>,
    revision: i64,
    boundary: Cow<'a, str>,
}

#[derive(Clone, PartialEq, Eq)]
struct Boundary<'a> {
    purpose: Cow<'a, str>,
    at: Option<i64>,
    status: Cow<'a, str>,
    reason: Option<Cow<'a, str>>,
    rule: Option<Cow<'a, str>>,
    retry: Option<Retry<'a>>,
}

impl Boundary<'_> {
    fn into_owned(self) -> Boundary<'static> {
        Boundary {
            purpose: self.purpose.into_owned().into(),
            at: self.at,
            status: self.status.into_owned().into(),
            reason: self.reason.map(|value| value.into_owned().into()),
            rule: self.rule.map(|value| value.into_owned().into()),
            retry: self.retry.map(|retry| Retry {
                schedule: retry.schedule.into_owned().into(),
                revision: retry.revision,
                boundary: retry.boundary.into_owned().into(),
            }),
        }
    }
}

enum Part<'a> {
    Heading(Heading<'a>),
    Boundary(Boundary<'a>),
    Limit,
}

pub(super) fn render(world: &mut World, owner: Entity, group: Entity, value: &Value) {
    let _prepare = nucleus::performance::Span::new("karma-schedule-outcome-preparation");
    let Some(uid) = value["uid"].as_str() else {
        return;
    };
    let revision = value["revision"].as_i64().unwrap_or(0);
    let mut rows = vec![(
        Key::Heading,
        Part::Heading(Heading {
            uid: uid.into(),
            revision,
            name: value["name"].as_str().unwrap_or("Schedule").into(),
            cancelled: value["cancelled"] == true,
        }),
    )];
    if let Some(boundaries) = value["boundaries"].as_array() {
        let mut occurrences = HashMap::<&str, usize>::new();
        for (index, boundary) in boundaries
            .iter()
            .enumerate()
            .skip(boundaries.len().saturating_sub(50))
        {
            let boundary_uid = boundary["uid"].as_str();
            let key = if let Some(uid) = boundary_uid {
                let occurrence = occurrences.entry(uid).or_default();
                let key = Key::Boundary(Some(uid.into()), *occurrence);
                *occurrence += 1;
                key
            } else {
                Key::Boundary(None, index)
            };
            let status = boundary["status"].as_str().unwrap_or("unknown");
            let retry = (boundary["current"] == true && matches!(status, "failed" | "blocked"))
                .then_some(boundary_uid)
                .flatten()
                .map(|boundary| Retry {
                    schedule: uid.into(),
                    revision,
                    boundary: boundary.into(),
                });
            rows.push((
                key,
                Part::Boundary(Boundary {
                    purpose: boundary["input"]["purpose"]
                        .as_str()
                        .unwrap_or("change")
                        .into(),
                    at: boundary["intended_at_ms"].as_i64(),
                    status: status.into(),
                    reason: boundary["reason"].as_str().map(Cow::Borrowed),
                    rule: boundary["rule"].as_str().map(Cow::Borrowed),
                    retry,
                }),
            ));
        }
        if boundaries.len() > 50 {
            rows.push((Key::Limit, Part::Limit));
        }
    }
    let mut parts = world.entity_mut(group).take::<Parts>().unwrap_or_default();
    parts.0.reconcile(
        world,
        group,
        rows,
        |world, parent, part| {
            let entity = if matches!(part, Part::Limit) {
                crate::native::edit_mode::label(world, parent, LIMIT, 13.0)
            } else {
                ui::row(world, parent)
            };
            draw(world, owner, entity, part);
            entity
        },
        |world, entity, part| draw(world, owner, entity, part),
    );
    world.entity_mut(group).insert(parts);
}

const LIMIT: &str = "Showing the latest 50 boundary outcomes.";

fn draw(world: &mut World, owner: Entity, entity: Entity, part: &Part<'_>) {
    let _present = nucleus::performance::Span::new("karma-schedule-outcome-part-presentation");
    match part {
        Part::Heading(heading) => {
            if Presentation::matches(world, entity, |old: &Heading<'static>| old == heading) {
                return;
            }
            presentation::clear(world, entity);
            crate::native::edit_mode::label(
                world,
                entity,
                &format!(
                    "{} · revision {}{}",
                    heading.name,
                    heading.revision,
                    if heading.cancelled {
                        " · cancelled"
                    } else {
                        ""
                    }
                ),
                15.0,
            );
            if !heading.cancelled {
                button(
                    world,
                    entity,
                    owner,
                    "Edit dates and changes",
                    Command::Edit(heading.uid.to_string()),
                );
                button(
                    world,
                    entity,
                    owner,
                    "Cancel future changes",
                    Command::Cancel(heading.uid.to_string(), heading.revision),
                );
            }
            Presentation::remember(world, entity, heading.clone().into_owned());
        }
        Part::Boundary(boundary) => {
            if Presentation::matches(world, entity, |old: &Boundary<'static>| old == boundary) {
                return;
            }
            presentation::clear(world, entity);
            let at = boundary
                .at
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map_or_else(|| "Unknown date".into(), |date| date.to_rfc3339());
            let reason = boundary
                .reason
                .as_ref()
                .map_or(String::new(), |reason| format!(" · {reason}"));
            crate::native::edit_mode::label(
                world,
                entity,
                &format!("{} · {at} · {}{reason}", boundary.purpose, boundary.status),
                13.0,
            );
            if let Some(rule) = &boundary.rule {
                button(
                    world,
                    entity,
                    owner,
                    "Open Rule",
                    Command::Rule(rule.to_string()),
                );
            }
            if let Some(retry) = &boundary.retry {
                button(
                    world,
                    entity,
                    owner,
                    "Retry original occurrence",
                    Command::Retry(
                        retry.schedule.to_string(),
                        retry.revision,
                        retry.boundary.to_string(),
                    ),
                );
            }
            Presentation::remember(world, entity, boundary.clone().into_owned());
        }
        Part::Limit => {
            if Presentation::matches(world, entity, |_: &()| true)
                && world
                    .get::<Text>(entity)
                    .is_some_and(|text| text.0 == LIMIT)
            {
                return;
            }
            let font = world
                .resource::<crate::native::theme::Typography>()
                .text(13.0);
            world.entity_mut(entity).insert((Text::new(LIMIT), font));
            Presentation::remember(world, entity, ());
        }
    }
    nucleus::performance::add("karma-schedule-outcome-parts-built", 1);
}
