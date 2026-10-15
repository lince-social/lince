use crate::native::actions::Action;
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
            .get::<crate::native::protein_area::RecordBinding>(editor.target)
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
    crate::native::castle_feed::button(world, parent, target, "Date / time", Open);
}

#[derive(Clone)]
struct Open;

impl Action for Open {
    fn apply(&self, world: &mut World, target: Entity) {
        if crate::native::laboratory::suspended(world, target) {
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
        let mut cursor = target;
        let parent = loop {
            if let Some(root) = world.get::<crate::native::sand::InBox>(cursor) {
                break root.0;
            }
            if world
                .get::<crate::native::container::BoxRoot>(cursor)
                .is_some()
            {
                break cursor;
            }
            let Some(parent) = world.get::<ChildOf>(cursor) else {
                return;
            };
            cursor = parent.parent();
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
                    min_width: px(0),
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(34),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    padding: UiRect::all(px(10)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                GlobalZIndex(90),
                crate::native::inspection::InspectionExcluded,
                crate::native::token_style::background(crate::native::tokens::Token::Surface),
                crate::native::token_style::border(crate::native::tokens::Token::Accent),
                ChildOf(parent),
            ))
            .id();
        crate::native::edit_mode::label(world, panel, "Scheduled time", 16.0);
        let initial = [date.clone(), time.clone(), timezone.clone()];
        let date = crate::native::sand_panel::field(world, panel, "Date (YYYY-MM-DD)", &date);
        let time = crate::native::sand_panel::field(
            world,
            panel,
            "Time (HH:MM, optional seconds; blank for all day)",
            &time,
        );
        let timezone = crate::native::sand_panel::field(world, panel, "IANA timezone", &timezone);
        crate::native::sand_panel::button(
            world,
            panel,
            panel,
            "Show saved time in this zone",
            ShowZone,
        );
        let status = crate::native::edit_mode::label(
            world,
            panel,
            if local_timezone().is_some() {
                ""
            } else {
                "Device timezone unavailable; using UTC."
            },
            12.0,
        );
        let choices = crate::native::sand_panel::column(world, panel);
        let controls = crate::native::sand_panel::row(world, panel);
        crate::native::sand_panel::button(world, controls, panel, "Apply", Apply);
        crate::native::sand_panel::button(world, controls, panel, "All day", AllDay);
        crate::native::sand_panel::button(world, controls, panel, "Clear", Save(String::new()));
        crate::native::sand_panel::button(world, controls, panel, "Close", Close);
        world.entity_mut(panel).insert(Editor {
            target,
            date,
            time,
            timezone,
            status,
            choices,
            initial,
        });
        crate::native::scroll_sand::attach(world, panel);
        maintain(world);
    }
}

pub(crate) fn maintain(world: &mut World) {
    let panels: Vec<_> = world
        .query::<(Entity, &Editor)>()
        .iter(world)
        .map(|(entity, editor)| (entity, editor.target))
        .collect();
    for (panel, target) in panels {
        if world.get::<EditableText>(target).is_none() {
            world.despawn(panel);
            continue;
        }
        let root = world.get::<ChildOf>(panel).unwrap().parent();
        let viewport = world
            .get::<ComputedNode>(root)
            .map(|node| node.size() * node.inverse_scale_factor())
            .filter(|size| size.min_element() > 0.0)
            .unwrap_or(Vec2::new(1000.0, 800.0));
        let bounds = crate::native::topology::presentation::bounds(world, target).unwrap_or(
            Rect::from_corners(Vec2::splat(16.0), Vec2::new(340.0, 80.0)),
        );
        let width = 340.0_f32.min((viewport.x - 16.0).max(1.0));
        let height = world.get::<ComputedNode>(panel).map_or(480.0, |node| {
            (node.size().y * node.inverse_scale_factor()).max(480.0)
        });
        let mut node = world.get::<Node>(panel).unwrap().clone();
        node.width = px(width);
        node.max_height = px((viewport.y - 16.0).max(1.0));
        node.overflow = Overflow::scroll_y();
        node.left = px(bounds.min.x.clamp(8.0, (viewport.x - width - 8.0).max(8.0)));
        node.top = px((bounds.max.y + 4.0).clamp(8.0, (viewport.y - height - 8.0).max(8.0)));
        world.get_mut::<Node>(panel).unwrap().set_if_neq(node);
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
                    crate::native::sand_panel::button(
                        world,
                        choices,
                        owner,
                        &value,
                        Save(value.clone()),
                    );
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
            .get::<crate::native::protein_area::RecordBinding>(target)
            .is_some()
        {
            if let Err(error) = crate::native::protein_area::save_date(world, target, &self.0) {
                status(world, owner, &error);
                return;
            }
        } else if let Some(mut text) = world.get_mut::<EditableText>(target) {
            text.editor.set_text(&self.0);
        } else {
            status(world, owner, "Scheduling field is closed");
            return;
        }
        crate::native::protein_area::sync_field_history(world, target, &self.0);
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

pub(crate) mod tests {
    use super::*;

    crate::native::laboratory_cases! {
        time_editor_escapes_property_clipping_and_reuses_its_target,
    }

    #[cfg_attr(test, test)]
    fn time_editor_escapes_property_clipping_and_reuses_its_target() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        let root = world
            .spawn((crate::native::container::BoxRoot, Node::default()))
            .id();
        let property = world
            .spawn((
                Node {
                    width: px(240),
                    height: px(32),
                    overflow: Overflow::clip(),
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        let target = world
            .spawn((
                crate::native::sand::text_editor(
                    "2026-10-08",
                    world.resource::<crate::native::theme::Typography>(),
                    0,
                ),
                ChildOf(property),
            ))
            .id();
        for _ in 0..5 {
            Open.apply(&mut world, target);
            let (panel, _) = world.query::<(Entity, &Editor)>().single(&world).unwrap();
            assert_eq!(world.get::<ChildOf>(panel).unwrap().parent(), root);
            assert_eq!(
                world.get::<Node>(panel).unwrap().position_type,
                PositionType::Absolute
            );
            assert_eq!(world.get::<Node>(property).unwrap().height, px(32));
            assert_eq!(
                world.get::<EditableText>(target).unwrap().value(),
                "2026-10-08"
            );
            assert!(
                world
                    .get::<crate::native::scroll_sand::ScrollSand>(panel)
                    .is_some()
            );
        }
        let panel = world
            .query_filtered::<Entity, With<Editor>>()
            .single(&world)
            .unwrap();
        Close.apply(&mut world, panel);
        assert!(world.get::<EditableText>(target).is_some());
        assert_eq!(world.query::<&Editor>().iter(&world).count(), 0);
    }
}
