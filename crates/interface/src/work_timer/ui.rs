use super::*;
use crate::icons::{Icon, IconButton, Tooltip};
use bevy::input_focus::InputFocus;

#[derive(Component)]
struct Form {
    owner: Entity,
    id: Option<String>,
    start: Entity,
    end: Entity,
    duration: Entity,
    initial: [String; 2],
    attempted: Option<[String; 2]>,
}

fn input(world: &mut World, parent: Entity, title: &str, value: &str) -> Entity {
    crate::edit_mode::label(world, parent, title, 12.0);
    let entity = world
        .spawn(crate::sand::text_editor(
            value,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    world.entity_mut(entity).insert((Node {
        width: percent(100), min_width: px(0), min_height: px(30), flex_shrink: 0.0, ..default()
    }, Tooltip("Timestamp with timezone, e.g. 2026-09-19T09:00:00-03:00. Enter or leave the entry to save.".into()), ChildOf(parent)));
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(64);
    let font = world.resource::<crate::theme::Typography>().text(14.0);
    world.entity_mut(entity).insert(font);
    crate::protein_area::attach_field_history(world, entity);
    entity
}

fn entry(world: &mut World, list: Entity, owner: Entity, value: Option<&Entry>) -> Entity {
    let row = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: px(4),
                padding: UiRect::all(px(6)),
                border: UiRect::all(px(1)),
                ..default()
            },
            crate::token_style::border(crate::tokens::Token::Accent),
            ChildOf(list),
        ))
        .id();
    let heading = world
        .spawn((
            Node {
                width: percent(100),
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(row),
        ))
        .id();
    let duration = crate::edit_mode::label(
        world,
        heading,
        if value.is_some() { "" } else { "New entry" },
        14.0,
    );
    world.get_mut::<Node>(duration).unwrap().flex_grow = 1.0;
    if let Some(value) = value {
        world.spawn((
            crate::sand::Square,
            IconButton::new(Icon::Delete, "Delete this time entry"),
            ActionButton::new(owner, crate::actions![Delete(value.id.clone())]),
            ChildOf(heading),
        ));
    }
    let initial: [String; 2] = [
        value.map_or("", |value| value.start.as_str()).into(),
        value
            .and_then(|value| value.end.as_deref())
            .unwrap_or("")
            .into(),
    ];
    let start = input(world, row, "Start", &initial[0]);
    let end = input(world, row, "End (blank while running)", &initial[1]);
    world.entity_mut(row).insert(Form {
        owner,
        id: value.map(|value| value.id.clone()),
        start,
        end,
        duration,
        initial,
        attempted: None,
    });
    row
}

pub(super) fn reconcile(world: &mut World, owner: Entity) {
    let timer = world.get::<WorkTimer>(owner).unwrap();
    let Some(list) = timer.list else { return };
    let logs = timer.logs.clone();
    let enabled = timer.query.is_empty() || timer.binding.is_some();
    world.get_mut::<Node>(list).unwrap().display = if enabled {
        Display::Flex
    } else {
        Display::None
    };
    let forms: Vec<_> = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .filter(|(_, form)| form.owner == owner)
        .map(|(entity, form)| (entity, form.id.clone()))
        .collect();
    let mut ordered = Vec::new();
    let mut logs = logs;
    logs.sort_by_key(|entry| (entry.end.is_some(), std::cmp::Reverse(entry.start_time())));
    for log in &logs {
        let row = forms
            .iter()
            .find(|(_, id)| id.as_ref() == Some(&log.id))
            .map(|(entity, _)| *entity)
            .unwrap_or_else(|| entry(world, list, owner, Some(log)));
        let form = world.get::<Form>(row).unwrap();
        let values = values(world, form);
        let observed = [log.start.clone(), log.end.clone().unwrap_or_default()];
        if values == form.initial && form.initial != observed {
            let (start, end) = (form.start, form.end);
            for (field, value) in [start, end].into_iter().zip(&observed) {
                world
                    .get_mut::<EditableText>(field)
                    .unwrap()
                    .editor
                    .set_text(value);
                crate::protein_area::sync_field_history(world, field, value);
            }
            world.get_mut::<Form>(row).unwrap().initial = observed;
        }
        ordered.push(row);
    }
    for (row, id) in &forms {
        if id
            .as_ref()
            .is_some_and(|id| !logs.iter().any(|log| &log.id == id))
        {
            world.despawn(*row);
        }
    }
    let blank = forms
        .iter()
        .find(|(_, id)| id.is_none())
        .map(|(entity, _)| *entity)
        .unwrap_or_else(|| entry(world, list, owner, None));
    ordered.push(blank);
    if let Some(children) = world.get::<Children>(list) {
        ordered.extend(
            children
                .iter()
                .filter(|child| world.get::<Form>(*child).is_none()),
        );
    }
    if world
        .get::<Children>(list)
        .is_none_or(|children| !children.iter().eq(ordered.iter().copied()))
    {
        world.entity_mut(list).replace_children(&ordered);
    }
}

fn values(world: &World, form: &Form) -> [String; 2] {
    [form.start, form.end].map(|entity| {
        world
            .get::<EditableText>(entity)
            .unwrap()
            .value()
            .to_string()
    })
}

pub(super) fn reset(world: &mut World, owner: Entity) {
    let forms: Vec<_> = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .filter(|(_, form)| form.owner == owner)
        .map(|(entity, _)| entity)
        .collect();
    for entity in forms {
        world.despawn(entity);
    }
}

pub(super) fn finished(world: &mut World, form: Entity, submitted: [String; 2], success: bool) {
    let Some(value) = world.get::<Form>(form) else {
        return;
    };
    if !success {
        return;
    }
    if value.id.is_some() {
        world.get_mut::<Form>(form).unwrap().initial = submitted;
    } else if values(world, value) == submitted {
        let fields = [value.start, value.end];
        for field in fields {
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text("");
            crate::protein_area::attach_field_history(world, field);
        }
        let mut value = world.get_mut::<Form>(form).unwrap();
        value.initial = [String::new(), String::new()];
        value.attempted = None;
    }
}

#[derive(Clone)]
struct Delete(String);
impl Action for Delete {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Err(error) = change(world, owner, &self.0, None, None) {
            status(world, owner, &error);
        }
    }
}

pub(super) fn edits(world: &mut World, mut previous: Local<Option<Entity>>) {
    let focus = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    let enter = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| {
            keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter)
        });
    let forms: Vec<_> = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .filter_map(|(entity, form)| {
            let fields = [form.start, form.end];
            if fields.iter().any(|field| {
                world
                    .get::<EditableText>(*field)
                    .is_none_or(|text| text.is_composing() || text.pending_paste.is_some())
            }) {
                return None;
            }
            let focused = focus.is_some_and(|focus| fields.contains(&focus));
            let blurred = previous.is_some_and(|focus| fields.contains(&focus)) && !focused;
            let values = values(world, form);
            (values != form.initial
                && form.attempted.as_ref() != Some(&values)
                && (form.id.is_some() || blurred || focused && enter)
                && world
                    .get::<WorkTimer>(form.owner)
                    .is_some_and(|timer| timer.pending.is_none()))
            .then_some((entity, form.owner, form.id.clone(), values))
        })
        .collect();
    *previous = focus;
    for (form, owner, id, values) in forms {
        world.get_mut::<Form>(form).unwrap().attempted = Some(values.clone());
        let id = id.unwrap_or_else(|| format!("work.log:{}", nucleus::new_uid("op")));
        let value = Entry {
            id: id.clone(),
            start: values[0].trim().into(),
            end: (!values[1].trim().is_empty()).then(|| values[1].trim().into()),
        };
        if let Err(error) = change(world, owner, &id, Some(value), Some((form, values))) {
            status(world, owner, &error);
        }
    }
}

pub(super) fn tick(world: &mut World, now: chrono::DateTime<chrono::Utc>) {
    let rows: Vec<_> = world
        .query::<&Form>()
        .iter(world)
        .filter_map(|form| {
            let id = form.id.as_ref()?;
            let entry = world
                .get::<WorkTimer>(form.owner)?
                .logs
                .iter()
                .find(|entry| &entry.id == id)?;
            Some((
                form.duration,
                format!(
                    "{}{}",
                    if entry.end.is_none() {
                        "Current · "
                    } else {
                        ""
                    },
                    formatted(entry.seconds(now))
                ),
            ))
        })
        .collect();
    for (entity, value) in rows {
        if world.get::<Text>(entity).unwrap().0 != value {
            world.get_mut::<Text>(entity).unwrap().0 = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input_focus::FocusCause;

    #[test]
    fn standalone_form_adds_autosaves_deletes_and_keeps_scroll_credits() {
        let mut app = App::new();
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins(WorkTimerPlugin);
        let world = app.world_mut();
        let owner = world.spawn(Node::default()).id();
        let input = world.spawn(EditableText::new("")).id();
        populate(world, owner, None, &Value::Null, Some(input));
        let list = world.get::<WorkTimer>(owner).unwrap().list.unwrap();
        let (blank, start, end) = world
            .query::<(Entity, &Form)>()
            .iter(world)
            .map(|(entity, form)| (entity, form.start, form.end))
            .next()
            .unwrap();
        for (field, value) in [
            (start, "2026-09-19T10:00:00Z"),
            (end, "2026-09-19T10:02:00Z"),
        ] {
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(value);
        }
        world
            .resource_mut::<InputFocus>()
            .set(end, FocusCause::Pressed);
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
        let world = app.world_mut();
        assert_eq!(world.get::<LocalTimer>(owner).unwrap().logs.len(), 1);
        assert_eq!(
            world
                .get::<Text>(world.get::<WorkTimer>(owner).unwrap().label)
                .unwrap()
                .0,
            "Total 00:02:00"
        );
        assert_eq!(values(world, world.get::<Form>(blank).unwrap()), ["", ""]);
        let (row, end, id) = world
            .query::<(Entity, &Form)>()
            .iter(world)
            .find_map(|(entity, form)| form.id.clone().map(|id| (entity, form.end, id)))
            .unwrap();
        world
            .get_mut::<EditableText>(end)
            .unwrap()
            .editor
            .set_text("2026-09-19T10:03:00Z");
        app.update();
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world
                .get::<Text>(world.get::<WorkTimer>(owner).unwrap().label)
                .unwrap()
                .0,
            "Total 00:03:00"
        );
        assert_eq!(world.get::<Form>(row).unwrap().end, end);
        Delete(id).apply(world, owner);
        assert!(world.get::<LocalTimer>(owner).unwrap().logs.is_empty());
        assert!(world.get::<Form>(row).is_none());
        assert!(
            world
                .get::<Children>(list)
                .unwrap()
                .iter()
                .any(|child| world.get::<crate::sand_store::SandCredits>(child).is_some())
        );
        Toggle.apply(world, owner);
        assert!(
            world.get::<LocalTimer>(owner).unwrap().logs[0]
                .end
                .is_none()
        );
        Toggle.apply(world, owner);
        assert!(
            world.get::<LocalTimer>(owner).unwrap().logs[0]
                .end
                .is_some()
        );
    }
}
