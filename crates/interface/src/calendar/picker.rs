use super::*;
use crate::{
    actions::ActionButton,
    canvas_selection::SandGroup,
    scoped_events::{EventBoundary, EventListener, SandEvent},
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

pub(crate) fn date_button(world: &mut World, row: Entity, editor: Entity, property: &str) -> Entity {
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
    world
        .entity_mut(button)
        .insert(ActionButton::new(editor, crate::actions![OpenDate(editor)]));
    button
}

impl Action for OpenDate {
    fn apply(&self, world: &mut World, _: Entity) {
        let editor = self.0;
        let Some(field) = world.get::<DateField>(editor) else {
            return;
        };
        let row = field.row;
        if crate::laboratory::suspended(world, row) {
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
        if let Some(date) = model::parse(&current) {
            calendar.year = date.year();
            calendar.month = date.month();
            calendar.start = Some(current);
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
            let group = if let Some(group) = world.get::<SandGroup>(row).copied() {
                group
            } else {
                let mut id = [0; 16];
                if getrandom::fill(&mut id).is_err() {
                    return;
                }
                let group = SandGroup(id);
                world.entity_mut(row).insert(group);
                group
            };
            let position = world
                .get::<crate::canvas::CanvasItem>(row)
                .map_or(DVec2::ZERO, |item| {
                    item.position + DVec2::new(f64::from(item.size.x) * 0.5 + 260.0, 0.0)
                });
            let entity = spawn(world, root, workspace, position, calendar);
            world.entity_mut(entity).insert((
                group,
                EventBoundary(vec![DATE_SELECTED.into()]),
                ZIndex(20),
            ));
            world
                .get_mut::<crate::canvas::CanvasItem>(entity)
                .unwrap()
                .size = Vec2::new(520.0, 440.0);
            entity
        };
        world.entity_mut(entity).insert(Picker { row, editor });
        sync(world, entity);
        ui::render(world, entity);
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
        let date = model::parse(&value).map(|_| value);
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
            if let Err(error) = crate::protein_area::save_date(world, editor, &date) {
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
        let text = format!(
            "{title}: {}",
            if value.is_empty() {
                "yyyy-mm-dd"
            } else {
                &value
            }
        );
        let button = ui::button(
            world,
            bar,
            editor,
            &text,
            &format!("Choose {title} date"),
            Command::Clear,
        );
        world
            .entity_mut(button)
            .insert(ActionButton::new(editor, crate::actions![OpenDate(editor)]));
        if editor == active {
            world
                .entity_mut(button)
                .insert(crate::token_style::background(crate::tokens::Token::Accent));
        }
    }
}
