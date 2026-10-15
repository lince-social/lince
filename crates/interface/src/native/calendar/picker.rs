use super::*;
use crate::native::{
    actions::ActionButton,
    scoped_events::{EventListener, SandEvent},
};
use bevy::text::EditableText;
use chrono::Datelike;

#[derive(Component)]
pub(super) struct DateField {
    row: Entity,
    property: String,
}

#[derive(Clone)]
pub(super) struct OpenDate(pub Entity);

pub(crate) fn date_button(
    world: &mut World,
    row: Entity,
    editor: Entity,
    property: &str,
) -> Entity {
    world
        .entity_mut(editor)
        .insert((
            DateField {
                row,
                property: property.into(),
            },
            EventListener(vec![DATE_SELECTED.into()]),
        ))
        .observe(selected);
    let button = ui::button(
        world,
        row,
        editor,
        "▦",
        if property == "start_date" {
            "Choose start date"
        } else {
            "Choose due date"
        },
        Command::Clear,
    );
    world.entity_mut(button).insert(ActionButton::new(
        editor,
        crate::native::actions![OpenDate(editor)],
    ));
    button
}

impl Action for OpenDate {
    fn apply(&self, world: &mut World, _: Entity) {
        let editor = self.0;
        let Some(field) = world.get::<DateField>(editor) else {
            return;
        };
        let row = field.row;
        if crate::native::laboratory::suspended(world, row) {
            return;
        }
        let Some(root) = world.get::<ChildOf>(row).map(ChildOf::parent) else {
            return;
        };
        let workspace = world.get::<WorkspaceMember>(row).map_or(1, |m| m.0);
        let current = world
            .get::<EditableText>(editor)
            .map(|t| t.value().to_string())
            .unwrap_or_default();
        let mut calendar = Calendar::default();
        calendar.timezone =
            crate::native::schedule_editor::local_timezone().unwrap_or_else(|| "UTC".into());
        if let Some(date) = nucleus::schedule::timezone_date(&current, &calendar.timezone) {
            calendar.year = date.year();
            calendar.month = date.month();
            calendar.start = Some(date.to_string());
        }
        let existing = world
            .query::<(Entity, &Picker)>()
            .iter(world)
            .find(|(_, p)| p.row == row)
            .map(|(entity, _)| entity);
        let entity = if let Some(entity) = existing {
            world.get_mut::<CalendarSand>(entity).unwrap().0 = calendar;
            entity
        } else {
            world
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(360),
                        height: px(400),
                        min_width: px(0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(8)),
                        border: UiRect::all(px(1)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    GlobalZIndex(80),
                    crate::native::inspection::InspectionExcluded,
                    crate::native::sand_store::SandCredits(CREDITS),
                    crate::native::token_style::background(crate::native::tokens::Token::Surface),
                    crate::native::token_style::border(crate::native::tokens::Token::Accent),
                    ChildOf(root),
                    WorkspaceMember(workspace),
                    CalendarSand(calendar),
                    View::default(),
                ))
                .id()
        };
        world.entity_mut(entity).insert(Picker { row, editor });
        sync(world, entity);
        ui::render(world, entity);
        position(world, entity);
    }
}

pub(super) fn position(world: &mut World, owner: Entity) {
    let Some(picker) = world.get::<Picker>(owner) else {
        return;
    };
    let row = picker.row;
    let editor = picker.editor;
    let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
        return;
    };
    let viewport = world
        .get::<ComputedNode>(root)
        .map(|node| node.size() * node.inverse_scale_factor())
        .filter(|size| size.min_element() > 0.0)
        .unwrap_or(Vec2::new(1000.0, 800.0));
    let bounds = crate::native::topology::presentation::bounds(world, editor)
        .or_else(|| crate::native::topology::presentation::bounds(world, row))
        .unwrap_or(Rect::from_corners(
            Vec2::splat(16.0),
            Vec2::new(360.0, 80.0),
        ));
    let width = 360.0_f32.min((viewport.x - 16.0).max(1.0));
    let height = 400.0_f32.min((viewport.y - 16.0).max(1.0));
    let left = bounds.min.x.clamp(8.0, (viewport.x - width - 8.0).max(8.0));
    let below = bounds.max.y + 8.0;
    let top = if below + height <= viewport.y - 8.0 {
        below
    } else {
        bounds.min.y - height - 8.0
    };
    let active = world
        .get::<crate::native::workspace::Workspaces>(root)
        .is_none_or(|spaces| {
            world
                .get::<WorkspaceMember>(owner)
                .is_none_or(|member| member.0 == spaces.active)
        });
    let mut node = world.get::<Node>(owner).unwrap().clone();
    node.left = px(left);
    node.top = px(top.clamp(8.0, (viewport.y - height - 8.0).max(8.0)));
    node.width = px(width);
    node.height = px(height);
    node.display = if active { Display::Flex } else { Display::None };
    world.get_mut::<Node>(owner).unwrap().set_if_neq(node);
}

#[derive(Clone)]
struct Endpoint(Entity);

impl Action for Endpoint {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world.get::<DateField>(self.0).is_none() {
            return;
        }
        if let Some(mut picker) = world.get_mut::<Picker>(owner) {
            picker.editor = self.0;
        }
        sync(world, owner);
        ui::render(world, owner);
    }
}

pub(super) fn sync(world: &mut World, owner: Entity) -> String {
    let Some(picker) = world.get::<Picker>(owner) else {
        return String::new();
    };
    let row = picker.row;
    let editor = picker.editor;
    let mut start = None;
    let mut end = None;
    let mut selecting_end = false;
    let mut key = String::new();
    for (entity, field, text) in world
        .query::<(Entity, &DateField, &EditableText)>()
        .iter(world)
    {
        if field.row != row {
            continue;
        }
        let value = text.value().to_string();
        key.push_str(&value);
        key.push('|');
        let date = nucleus::schedule::timezone_date(
            &value,
            &world.get::<CalendarSand>(owner).unwrap().0.timezone,
        )
        .map(|date| date.to_string());
        if field.property == "start_date" {
            start = date;
        } else {
            end = date;
        }
        if entity == editor {
            selecting_end = field.property == "due_date";
        }
    }
    let current = &world.get::<CalendarSand>(owner).unwrap().0;
    if current.start != start || current.end != end || current.selecting_end != selecting_end {
        let mut sand = world.get_mut::<CalendarSand>(owner).unwrap();
        sand.0.start = start;
        sand.0.end = end;
        sand.0.selecting_end = selecting_end;
    }
    key
}

fn selected(event: On<SandEvent>, mut commands: Commands) {
    if event.name != DATE_SELECTED {
        return;
    }
    let Some(date) = event
        .value
        .as_str()
        .filter(|v| model::parse(v).is_some())
        .map(str::to_string)
    else {
        return;
    };
    let editor = event.entity;
    let source = event.source;
    commands.queue(move |world: &mut World| {
        if world
            .get::<Picker>(source)
            .is_some_and(|p| p.editor == editor)
        {
            let current = world
                .get::<EditableText>(editor)
                .map(|text| text.value().to_string())
                .unwrap_or_default();
            let timezone = world
                .get::<CalendarSand>(source)
                .unwrap()
                .0
                .timezone
                .clone();
            let result = if current.is_empty() {
                Ok(date.clone())
            } else {
                nucleus::schedule::on_date(&current, &date, &timezone)
            };
            if let Err(error) = result
                .and_then(|value| crate::native::protein_area::save_date(world, editor, &value))
            {
                world.get_mut::<View>(source).unwrap().error = Some(error);
            }
            sync(world, source);
            ui::render(world, source);
        }
    });
}

pub(super) fn fields(world: &mut World, owner: Entity, parent: Entity) {
    let picker = world.get::<Picker>(owner).unwrap();
    let row = picker.row;
    let active = picker.editor;
    let timezone = world.get::<CalendarSand>(owner).unwrap().0.timezone.clone();
    let fields: Vec<_> = world
        .query::<(Entity, &DateField, &EditableText)>()
        .iter(world)
        .filter(|(_, field, _)| field.row == row)
        .map(|(entity, field, text)| (entity, field.property.clone(), text.value().to_string()))
        .collect();
    let bar = world
        .spawn((
            Node {
                column_gap: px(4),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    for (editor, property, value) in fields {
        let title = if property == "start_date" {
            "Start"
        } else {
            "End"
        };
        let date = nucleus::schedule::timezone_date(&value, &timezone).map(|date| date.to_string());
        let text = format!(
            "{title}: {}",
            if value.is_empty() {
                "yyyy-mm-dd"
            } else {
                date.as_deref().unwrap_or("Invalid date")
            }
        );
        let button = ui::button(
            world,
            bar,
            owner,
            &text,
            &format!("Choose {title} date"),
            Command::Clear,
        );
        world.entity_mut(button).insert(ActionButton::new(
            owner,
            crate::native::actions![Endpoint(editor)],
        ));
        if editor == active {
            world
                .entity_mut(button)
                .insert(crate::native::token_style::background(
                    crate::native::tokens::Token::Accent,
                ));
        }
    }
}
