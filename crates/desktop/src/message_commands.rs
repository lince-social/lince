use crate::actions::Action;
use bevy::{prelude::*, text::EditableText};
use std::collections::HashMap;

#[derive(Clone, PartialEq)]
pub(crate) struct Command {
    pub name: String,
    pub description: String,
    pub hint: String,
    pub target: String,
    pub insert: String,
}

#[derive(Resource, Default)]
pub(crate) struct Catalog(HashMap<String, Vec<Command>>);

#[derive(Component)]
struct Picker {
    thread: String,
    input: Entity,
    search: Entity,
    list: Entity,
    panel: Entity,
    previous: Vec<Command>,
    filter: String,
    literal: bool,
    toggle: Entity,
}

pub(crate) fn create(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    thread: &str,
    input: Entity,
) {
    let panel = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                max_height: px(220),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::scroll_sand::attach(world, panel);
    crate::description::button(world, parent, owner, "Commands", Open);
    let toggle = crate::description::button(world, parent, owner, "Literal text: off", Literal);
    let search = world
        .spawn((
            crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0),
            ChildOf(panel),
            crate::icons::Tooltip("Search commands · Tab or Enter inserts the first match".into()),
        ))
        .id();
    world
        .get_mut::<EditableText>(search)
        .unwrap()
        .allow_newlines = false;
    crate::edit_mode::label(
        world,
        panel,
        "Selection only fills the draft. Sending an agent command can use tokens or tools.",
        12.0,
    );
    let list = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(panel),
        ))
        .id();
    world.entity_mut(owner).insert(Picker {
        thread: thread.into(),
        input,
        search,
        list,
        panel,
        previous: Vec::new(),
        filter: String::new(),
        literal: false,
        toggle,
    });
}

pub(crate) fn update(world: &mut World, thread: &str, commands: Vec<Command>) {
    if !world.contains_resource::<Catalog>() {
        world.init_resource::<Catalog>();
    }
    let mut catalog = world.resource_mut::<Catalog>();
    if catalog.0.len() >= 128 && !catalog.0.contains_key(thread) {
        if let Some(key) = catalog.0.keys().next().cloned() {
            catalog.0.remove(&key);
        }
    }
    catalog.0.insert(thread.into(), commands);
}

pub(crate) fn literal(world: &World, owner: Entity) -> bool {
    world
        .get::<Picker>(owner)
        .is_some_and(|picker| picker.literal)
}

fn commands(world: &World, picker: &Picker) -> Vec<Command> {
    let mut commands = vec![
        Command {
            name: "login".into(),
            description: "Open Fiote account setup".into(),
            hint: String::new(),
            target: "Lince".into(),
            insert: "/login".into(),
        },
        Command {
            name: "lock".into(),
            description: "Lock Fiote credentials".into(),
            hint: String::new(),
            target: "Lince".into(),
            insert: "/lock".into(),
        },
    ];
    if let Some(catalog) = world
        .get_resource::<Catalog>()
        .and_then(|catalog| catalog.0.get(&picker.thread))
    {
        commands.extend(catalog.clone());
    }
    let search = world
        .get::<EditableText>(picker.search)
        .map(|text| text.value().to_string().to_lowercase())
        .unwrap_or_default();
    commands.retain(|command| {
        format!(
            "{} {} {}",
            command.name, command.description, command.target
        )
        .to_lowercase()
        .contains(&search)
    });
    commands
}

pub(crate) fn refresh(world: &mut World) {
    let updates: Vec<_> = world
        .query::<(Entity, &Picker)>()
        .iter(world)
        .filter_map(|(owner, picker)| {
            let commands = commands(world, picker);
            let search = world
                .get::<EditableText>(picker.search)
                .map(|text| text.value().to_string())
                .unwrap_or_default();
            (commands != picker.previous || search != picker.filter).then_some((
                owner,
                picker.list,
                commands,
                search,
            ))
        })
        .collect();
    for (owner, list, commands, search) in updates {
        let mut picker = world.get_mut::<Picker>(owner).unwrap();
        picker.previous = commands.clone();
        picker.filter = search;
        world.entity_mut(list).despawn_children();
        for command in commands.iter().take(64) {
            crate::description::button(
                world,
                list,
                owner,
                &format!("{} · {}", command.name, command.target),
                Select(command.insert.clone()),
            );
            crate::edit_mode::label(
                world,
                list,
                &format!("{}\n{}", command.description, command.hint),
                12.0,
            );
        }
    }
}

pub(crate) fn keyboard(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    if !keys.just_pressed(KeyCode::Tab) && !keys.just_pressed(KeyCode::Enter) {
        return;
    }
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let selected = world
        .query::<(Entity, &Picker)>()
        .iter(world)
        .find(|(_, picker)| Some(picker.search) == focus)
        .and_then(|(owner, picker)| {
            commands(world, picker)
                .first()
                .map(|command| (owner, command.insert.clone()))
        });
    if let Some((owner, text)) = selected {
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset(KeyCode::Enter);
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset(KeyCode::Tab);
        Select(text).apply(world, owner);
    }
}

#[derive(Clone)]
struct Select(String);
impl Action for Select {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world
            .get::<crate::message_content::Draft>(owner)
            .is_some_and(|draft| draft.locked)
        {
            return;
        }
        let Some(picker) = world.get::<Picker>(owner) else {
            return;
        };
        let (input, panel) = (picker.input, picker.panel);
        if let Some(mut text) = world.get_mut::<EditableText>(input) {
            let old = text.value().to_string();
            let suffix = if old.starts_with('/') {
                old.split_once(' ').map(|(_, suffix)| suffix).unwrap_or("")
            } else {
                &old
            };
            text.editor.set_text(&format!("{} {suffix}", self.0));
        }
        world.get_mut::<Node>(panel).unwrap().display = Display::None;
        if world
            .get::<Picker>(owner)
            .is_some_and(|picker| picker.literal)
        {
            Literal.apply(world, owner);
        }
    }
}

#[derive(Clone)]
struct Open;
impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Picker>(owner).map(|picker| picker.panel) else {
            return;
        };
        let mut node = world.get_mut::<Node>(panel).unwrap();
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
    }
}

#[derive(Clone)]
struct Literal;
impl Action for Literal {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut picker) = world.get_mut::<Picker>(owner) else {
            return;
        };
        picker.literal = !picker.literal;
        let (toggle, literal) = (picker.toggle, picker.literal);
        let label = world
            .get::<Children>(toggle)
            .and_then(|children| children.first())
            .copied();
        if let Some(mut text) = label.and_then(|label| world.get_mut::<Text>(label)) {
            text.0 = format!("Literal text: {}", if literal { "on" } else { "off" });
        }
    }
}
