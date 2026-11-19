use crate::native::actions::Action;
use bevy::{prelude::*, text::EditableText};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

#[derive(Clone, PartialEq)]
pub(crate) struct Command {
    pub name: String,
    pub description: String,
    pub hint: String,
    pub target: String,
    pub insert: String,
}

#[derive(Clone, Copy)]
pub(crate) struct AgentCommand<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub hint: &'a str,
}

#[derive(Resource, Default)]
pub(crate) struct Catalog(HashMap<String, Arc<[PreparedCommand]>>);

struct PreparedCommand {
    command: Command,
    search: String,
}

impl PreparedCommand {
    fn new(command: Command) -> Self {
        let search = format!(
            "{} {} {}",
            command.name, command.description, command.target
        )
        .to_lowercase();
        Self { command, search }
    }
}

enum CommandIndex {
    Builtin(usize),
    Agent(usize),
}

struct Selection {
    catalog: Option<Arc<[PreparedCommand]>>,
    matches: Vec<CommandIndex>,
}

impl Selection {
    fn uses(&self, catalog: Option<&Arc<[PreparedCommand]>>) -> bool {
        match (self.catalog.as_ref(), catalog) {
            (None, None) => true,
            (Some(previous), Some(current)) => Arc::ptr_eq(previous, current),
            _ => false,
        }
    }

    fn new(catalog: Option<Arc<[PreparedCommand]>>, search: &str) -> Self {
        let _preparation = nucleus::performance::Span::new("message-command-selection");
        nucleus::performance::add("message-command-selections-prepared", 1);
        let search = search.to_lowercase();
        let matches = matching(catalog.as_deref(), &search)
            .take(64)
            .map(|(index, _)| index)
            .collect();
        Self { catalog, matches }
    }

    fn commands(&self) -> impl Iterator<Item = &Command> {
        self.matches.iter().map(|index| match index {
            CommandIndex::Builtin(index) => &builtins()[*index].command,
            CommandIndex::Agent(index) => &self.catalog.as_ref().unwrap()[*index].command,
        })
    }
}

fn catalog<'a>(world: &'a World, thread: &str) -> Option<&'a Arc<[PreparedCommand]>> {
    world
        .get_resource::<Catalog>()
        .and_then(|catalog| catalog.0.get(thread))
}

#[derive(Component)]
struct Picker {
    thread: String,
    input: Entity,
    search: Entity,
    list: Entity,
    panel: Entity,
    selection: Option<Arc<Selection>>,
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
    crate::native::scroll_sand::attach(world, panel);
    crate::native::description::button(world, parent, owner, "Commands", Open);
    let toggle =
        crate::native::description::button(world, parent, owner, "Literal text: off", Literal);
    let search = world
        .spawn((
            crate::native::sand::text_editor(
                "",
                world.resource::<crate::native::theme::Typography>(),
                0,
            ),
            ChildOf(panel),
            crate::native::icons::Tooltip(
                "Search commands · Tab or Enter inserts the first match".into(),
            ),
        ))
        .id();
    world
        .get_mut::<EditableText>(search)
        .unwrap()
        .allow_newlines = false;
    crate::native::edit_mode::label(
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
        selection: None,
        filter: String::new(),
        literal: false,
        toggle,
    });
}

pub(crate) fn update(world: &mut World, thread: &str, commands: Vec<Command>) {
    if catalog(world, thread).is_some_and(|previous| {
        previous
            .iter()
            .map(|entry| &entry.command)
            .eq(commands.iter())
    }) {
        nucleus::performance::add("message-command-catalogs-retained", 1);
        return;
    }
    publish(world, thread, commands.into_iter());
}

pub(crate) fn update_agent<'a>(
    world: &mut World,
    thread: &str,
    title: &str,
    commands: impl Iterator<Item = AgentCommand<'a>> + Clone,
) {
    if catalog(world, thread).is_some_and(|previous| {
        previous
            .iter()
            .map(|entry| {
                let command = &entry.command;
                (
                    command.name.as_str(),
                    command.description.as_str(),
                    command.hint.as_str(),
                    command.target.strip_prefix("Agent · "),
                    command.insert.strip_prefix("/agent:"),
                )
            })
            .eq(commands.clone().map(|command| {
                (
                    command.name,
                    command.description,
                    command.hint,
                    Some(title),
                    Some(command.name),
                )
            }))
    }) {
        nucleus::performance::add("message-command-catalogs-retained", 1);
        return;
    }
    let target = format!("Agent · {title}");
    publish(
        world,
        thread,
        commands.map(|command| Command {
            name: command.name.into(),
            description: command.description.into(),
            hint: command.hint.into(),
            target: target.clone(),
            insert: format!("/agent:{}", command.name),
        }),
    );
}

fn publish(world: &mut World, thread: &str, commands: impl Iterator<Item = Command>) {
    let _preparation = nucleus::performance::Span::new("message-command-catalog-preparation");
    nucleus::performance::add("message-command-catalogs-prepared", 1);
    let commands: Arc<[PreparedCommand]> = commands
        .map(PreparedCommand::new)
        .collect::<Vec<_>>()
        .into();
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

fn builtins() -> &'static [PreparedCommand; 2] {
    static COMMANDS: OnceLock<[PreparedCommand; 2]> = OnceLock::new();
    COMMANDS.get_or_init(|| {
        [
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
        ]
        .map(PreparedCommand::new)
    })
}

fn matching<'a>(
    catalog: Option<&'a [PreparedCommand]>,
    search: &'a str,
) -> impl Iterator<Item = (CommandIndex, &'a Command)> {
    builtins()
        .iter()
        .enumerate()
        .map(|(index, command)| (CommandIndex::Builtin(index), command))
        .chain(
            catalog
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(index, command)| (CommandIndex::Agent(index), command)),
        )
        .filter(move |(_, command)| search.is_empty() || command.search.contains(search))
        .map(|(index, command)| (index, &command.command))
}

pub(crate) fn refresh(world: &mut World) {
    let _presentation = nucleus::performance::Span::new("message-command-presentation");
    let updates: Vec<_> = world
        .query::<(Entity, &Picker)>()
        .iter(world)
        .filter_map(|(owner, picker)| {
            let editor = world.get::<EditableText>(picker.search);
            let same_search = editor.map_or(picker.filter.is_empty(), |editor| {
                editor.value() == picker.filter.as_str()
            });
            let catalog = catalog(world, &picker.thread);
            if same_search
                && picker
                    .selection
                    .as_ref()
                    .is_some_and(|selection| selection.uses(catalog))
            {
                return None;
            }
            let search = editor
                .map(|editor| editor.value().to_string())
                .unwrap_or_default();
            let selection = Arc::new(Selection::new(catalog.cloned(), &search));
            let redraw = picker
                .selection
                .as_ref()
                .is_none_or(|previous| !previous.commands().eq(selection.commands()));
            Some((owner, picker.list, selection, search, redraw))
        })
        .collect();
    for (owner, list, selection, search, redraw) in updates {
        let mut picker = world.get_mut::<Picker>(owner).unwrap();
        picker.selection = Some(Arc::clone(&selection));
        picker.filter = search;
        if !redraw {
            continue;
        }
        world.entity_mut(list).despawn_children();
        for command in selection.commands() {
            crate::native::description::button(
                world,
                list,
                owner,
                &format!("{} · {}", command.name, command.target),
                Select(command.insert.clone()),
            );
            crate::native::edit_mode::label(
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
            let search = world
                .get::<EditableText>(picker.search)
                .map(|text| text.value().to_cow().to_lowercase())
                .unwrap_or_default();
            matching(
                catalog(world, &picker.thread).map(|catalog| catalog.as_ref()),
                &search,
            )
            .next()
            .map(|(_, command)| (owner, command.insert.clone()))
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
            .get::<crate::native::message_content::Draft>(owner)
            .is_some_and(|draft| draft.locked)
        {
            return;
        }
        let Some(picker) = world.get::<Picker>(owner) else {
            return;
        };
        let (input, panel) = (picker.input, picker.panel);
        if let Some(mut text) = world.get_mut::<EditableText>(input) {
            let next = {
                let old = text.value().to_cow();
                let suffix = if old.starts_with('/') {
                    old.split_once(' ').map(|(_, suffix)| suffix).unwrap_or("")
                } else {
                    old.as_ref()
                };
                format!("{} {suffix}", self.0)
            };
            text.editor.set_text(&next);
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
