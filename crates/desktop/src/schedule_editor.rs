use crate::actions::Action;
use bevy::{prelude::*, text::EditableText};

#[derive(Component)]
struct Editor {
    target: Entity,
    date: Entity,
    time: Entity,
    timezone: Entity,
    status: Entity,
    choices: Entity,
    initial: [String; 3],
}

pub(crate) fn pending(world: &mut World, owner: Entity) -> bool {
    world.query::<&Editor>().iter(world).any(|editor| {
        world
            .get::<crate::protein_area::RecordBinding>(editor.target)
            .is_some_and(|binding| binding.area == owner)
            && [editor.date, editor.time, editor.timezone]
                .into_iter()
                .zip(&editor.initial)
                .any(|(field, initial)| {
                    world.get::<EditableText>(field).is_some_and(|text| {
                        text.value() != initial
                            || text.is_composing()
                            || text.pending_paste.is_some()
                    })
                })
    })
}

pub fn local_timezone() -> Option<String> {
    iana_time_zone::get_timezone()
        .ok()
        .filter(|zone| zone.parse::<nucleus::schedule::Tz>().is_ok())
}

pub fn attach(world: &mut World, parent: Entity, target: Entity) {
    crate::castle_feed::button(world, parent, target, "Date / time", Open);
}

#[derive(Clone)]
struct Open;

impl Action for Open {
    fn apply(&self, world: &mut World, target: Entity) {
        if crate::laboratory::suspended(world, target) {
            return;
        }
        let existing: Vec<_> = world
            .query::<(Entity, &Editor)>()
            .iter(world)
            .filter(|(_, editor)| editor.target == target)
            .map(|(entity, _)| entity)
            .collect();
        for entity in existing {
            world.despawn(entity);
        }
        let Some(parent) = world.get::<ChildOf>(target).map(ChildOf::parent) else {
            return;
        };
        let value = world
            .get::<EditableText>(target)
            .map(|text| text.value().to_string())
            .unwrap_or_default();
        let timezone = local_timezone().unwrap_or_else(|| "UTC".into());
        let zone: nucleus::schedule::Tz = timezone.parse().unwrap();
        let (date, time) = match nucleus::schedule::TimeValue::parse(&value) {
            Ok(nucleus::schedule::TimeValue::Date(date)) => (date.to_string(), String::new()),
            Ok(nucleus::schedule::TimeValue::Instant(time)) => {
                let local = time.with_timezone(&zone);
                (
                    local.format("%Y-%m-%d").to_string(),
                    local.format("%H:%M:%S%.f").to_string(),
                )
            }
            Err(_) => (
                chrono::Utc::now()
                    .with_timezone(&zone)
                    .date_naive()
                    .to_string(),
                String::new(),
            ),
        };
        let panel = world
            .spawn((
                Node {
                    width: px(340),
                    min_width: px(280),
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(34),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    padding: UiRect::all(px(10)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                ZIndex(80),
                crate::token_style::background(crate::tokens::Token::Surface),
                crate::token_style::border(crate::tokens::Token::Accent),
                ChildOf(parent),
            ))
            .id();
        crate::edit_mode::label(world, panel, "Scheduled time", 16.0);
        let initial = [date.clone(), time.clone(), timezone.clone()];
        let date = crate::sand_panel::field(world, panel, "Date (YYYY-MM-DD)", &date);
        let time = crate::sand_panel::field(
            world,
            panel,
            "Time (HH:MM, optional seconds; blank for all day)",
            &time,
        );
        let timezone = crate::sand_panel::field(world, panel, "IANA timezone", &timezone);
        crate::sand_panel::button(
            world,
            panel,
            panel,
            "Show saved time in this zone",
            ShowZone,
        );
        let status = crate::edit_mode::label(
            world,
            panel,
            if local_timezone().is_some() {
                ""
            } else {
                "Device timezone unavailable; using UTC."
            },
            12.0,
        );
        let choices = crate::sand_panel::column(world, panel);
        let controls = crate::sand_panel::row(world, panel);
        crate::sand_panel::button(world, controls, panel, "Apply", Apply);
        crate::sand_panel::button(world, controls, panel, "All day", AllDay);
        crate::sand_panel::button(world, controls, panel, "Clear", Save(String::new()));
        crate::sand_panel::button(world, controls, panel, "Close", Close);
        world.entity_mut(panel).insert(Editor {
            target,
            date,
            time,
            timezone,
            status,
            choices,
            initial,
        });
    }
}

fn value(world: &World, entity: Entity) -> String {
    world
        .get::<EditableText>(entity)
        .map(|text| text.value().to_string().trim().to_owned())
        .unwrap_or_default()
}

fn status(world: &mut World, owner: Entity, error: &str) {
    if let Some(entity) = world.get::<Editor>(owner).map(|editor| editor.status)
        && let Some(mut text) = world.get_mut::<Text>(entity)
    {
        text.0 = error.into();
    }
}

#[derive(Clone)]
struct ShowZone;
impl Action for ShowZone {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(editor) = world.get::<Editor>(owner) else {
            return;
        };
        let target = value(world, editor.target);
        let Ok(zone) = value(world, editor.timezone).parse::<nucleus::schedule::Tz>() else {
            status(world, owner, "Enter a valid IANA timezone");
            return;
        };
        let Ok(nucleus::schedule::TimeValue::Instant(at)) =
            nucleus::schedule::TimeValue::parse(&target)
        else {
            status(world, owner, "The saved endpoint has no precise time");
            return;
        };
        let at = at.with_timezone(&zone);
        let fields = [editor.date, editor.time];
        let values = [
            at.format("%Y-%m-%d").to_string(),
            at.format("%H:%M:%S%.f").to_string(),
        ];
        for (field, value) in fields.into_iter().zip(values) {
            if let Some(mut text) = world.get_mut::<EditableText>(field) {
                text.editor.set_text(&value);
            }
        }
        status(
            world,
            owner,
            "Showing the saved instant in the selected zone",
        );
    }
}

#[derive(Clone)]
struct Apply;
impl Action for Apply {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(editor) = world.get::<Editor>(owner) else {
            return;
        };
        let (date, time, timezone, choices) = (
            value(world, editor.date),
            value(world, editor.time),
            value(world, editor.timezone),
            editor.choices,
        );
        world.entity_mut(choices).despawn_children();
        if time.is_empty() {
            match nucleus::schedule::TimeValue::parse(&date) {
                Ok(nucleus::schedule::TimeValue::Date(_)) => Save(date).apply(world, owner),
                _ => status(world, owner, "Enter an exact YYYY-MM-DD date"),
            }
            return;
        }
        match nucleus::schedule::civil_instants(&date, &time, &timezone) {
            Ok(values) if values.len() == 1 => Save(values[0].clone()).apply(world, owner),
            Ok(values) => {
                status(
                    world,
                    owner,
                    "This time occurs twice. Choose its timezone offset:",
                );
                for value in values {
                    crate::sand_panel::button(world, choices, owner, &value, Save(value.clone()));
                }
            }
            Err(error) => status(world, owner, &error),
        }
    }
}

#[derive(Clone)]
struct AllDay;
impl Action for AllDay {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(time) = world.get::<Editor>(owner).map(|editor| editor.time)
            && let Some(mut text) = world.get_mut::<EditableText>(time)
        {
            text.editor.set_text("");
        }
        Apply.apply(world, owner);
    }
}

#[derive(Clone)]
struct Save(String);
impl Action for Save {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(target) = world.get::<Editor>(owner).map(|editor| editor.target) else {
            return;
        };
        if world
            .get::<crate::protein_area::RecordBinding>(target)
            .is_some()
        {
            if let Err(error) = crate::protein_area::save_date(world, target, &self.0) {
                status(world, owner, &error);
                return;
            }
        } else if let Some(mut text) = world.get_mut::<EditableText>(target) {
            text.editor.set_text(&self.0);
        } else {
            status(world, owner, "Scheduling field is closed");
            return;
        }
        crate::protein_area::sync_field_history(world, target, &self.0);
        world.despawn(owner);
    }
}

#[derive(Clone)]
struct Close;
impl Action for Close {
    fn apply(&self, world: &mut World, owner: Entity) {
        world.despawn(owner);
    }
}
