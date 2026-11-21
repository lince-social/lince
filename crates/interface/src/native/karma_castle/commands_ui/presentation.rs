// non-lean-yet
// Scope: enum Key, enum Part, struct Panel, fn render.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 24, 171, 182,
//   185.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   169, 220, 234.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (22, 70, 152, 167, 169, 170, 174, 235, 277).

use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Header,
    Editor,
    Result(String, usize),
}

enum Part<'a> {
    Header,
    Editor,
    Result(&'a Arc<Value>),
}

#[derive(Component, Default)]
struct Panel(RetainedChildren<Key>);

pub(super) fn render(world: &mut World, owner: Entity) {
    let _render = nucleus::performance::Span::new("karma-command-presentation");
    let state = world.get::<Commands>(owner).unwrap();
    let panel = state.panel;
    let source = state.rows.clone();
    let mut parts = vec![(Key::Header, Part::Header)];
    if state.editing {
        parts.push((Key::Editor, Part::Editor));
    }
    let mut occurrences = HashMap::<&str, usize>::new();
    for row in source.iter() {
        let Some(uid) = row["uid"].as_str() else {
            continue;
        };
        let occurrence = occurrences.entry(uid).or_default();
        parts.push((Key::Result(uid.into(), *occurrence), Part::Result(row)));
        *occurrence += 1;
    }
    let mut children = world.entity_mut(panel).take::<Panel>().unwrap_or_default();
    children.0.reconcile(
        world,
        panel,
        parts,
        |world, parent, part| {
            let entity = ui::stack(world, parent);
            draw(world, owner, entity, part);
            nucleus::performance::add("karma-command-parts-created", 1);
            entity
        },
        |world, entity, part| draw(world, owner, entity, part),
    );
    world.entity_mut(panel).insert(children);
}

fn draw(world: &mut World, owner: Entity, panel: Entity, part: &Part<'_>) {
    match part {
        Part::Header => {
            if Presentation::matches(world, panel, |_: &()| true) {
                return;
            }
            world.entity_mut(panel).despawn_children();
            header(world, owner, panel);
            Presentation::remember(world, panel, ());
        }
        Part::Editor => editor(world, owner, panel),
        Part::Result(row) => {
            if Presentation::matches(world, panel, |old: &Arc<Value>| {
                Arc::ptr_eq(old, row) || same_result(old, row)
            }) {
                if !Arc::ptr_eq(
                    Presentation::<Arc<Value>>::state(world, panel).unwrap(),
                    row,
                ) {
                    Presentation::update_state(world, panel, |old: &mut Arc<Value>| {
                        *old = Arc::clone(row)
                    });
                }
                nucleus::performance::add("karma-command-results-retained", 1);
                return;
            }
            world.entity_mut(panel).despawn_children();
            result(world, owner, panel, row);
            Presentation::remember(world, panel, Arc::clone(row));
            nucleus::performance::add("karma-command-results-built", 1);
        }
    }
}

fn invocation(value: &Value) -> (&str, &str, &str, &str) {
    (
        value["at"].as_str().unwrap_or_default(),
        value["status"].as_str().unwrap_or_default(),
        value["error"]
            .as_str()
            .or_else(|| value["value"].as_str())
            .or_else(|| value["stdout"].as_str())
            .unwrap_or_default(),
        value["stderr"].as_str().unwrap_or_default(),
    )
}

fn caption(value: &Value) -> (Option<&str>, Option<&str>, &str) {
    (
        value["uid"].as_str(),
        value["slug"].as_str().or_else(|| value["uid"].as_str()),
        value["sample"].as_str().unwrap_or("unavailable"),
    )
}

fn same_result(old: &Value, row: &Value) -> bool {
    caption(old) == caption(row)
        && old["unlimited"] == row["unlimited"]
        && old["history"]
            .as_array()
            .into_iter()
            .flatten()
            .take(5)
            .map(invocation)
            .eq(row["history"]
                .as_array()
                .into_iter()
                .flatten()
                .take(5)
                .map(invocation))
}

fn header(world: &mut World, owner: Entity, panel: Entity) {
    crate::native::edit_mode::label(world, panel, "Commands and numeric samples", 16.0);
    let line = ui::row(world, panel);
    for (label, command) in [
        ("New command", Command::New),
        ("Refresh results", Command::Refresh),
    ] {
        crate::native::castle_feed::button(world, line, owner, label, command);
    }
    crate::native::edit_mode::label(
        world,
        panel,
        "query_command(@source) runs a fresh numeric query. signal(@source) reads its saved successful sample. @target: run(@source) runs a saved command as a consequence.",
        13.0,
    );
}

struct Form {
    target: Option<String>,
    shell: bool,
    unlimited: bool,
    fields: [Entity; 5],
}

fn editor(world: &mut World, owner: Entity, panel: Entity) {
    let _editor = nucleus::performance::Span::new("karma-command-form-presentation");
    let state = world.get::<Commands>(owner).unwrap();
    if Presentation::matches(world, panel, |old: &Form| {
        old.target == state.target
            && old.shell == state.shell
            && old.unlimited == state.unlimited
            && old.fields.iter().enumerate().all(|(index, &field)| {
                world.get::<EditableText>(field).is_some()
                    && world
                        .get::<Input>(field)
                        .is_some_and(|input| input.owner == owner && input.index == index)
                    && world
                        .get::<ChildOf>(field)
                        .is_some_and(|child| child.parent() == panel)
            })
    }) {
        let fields = Presentation::<Form>::state(world, panel).unwrap().fields;
        for (index, field) in fields.into_iter().enumerate() {
            let value = &world.get::<Commands>(owner).unwrap().values[index];
            if world.get::<EditableText>(field).unwrap().value() != value.as_str() {
                let value = value.clone();
                world
                    .get_mut::<EditableText>(field)
                    .unwrap()
                    .editor
                    .set_text(&value);
                nucleus::performance::add("karma-command-editors-published", 1);
            }
        }
        return;
    }
    let target = state.target.clone();
    let shell = state.shell;
    let unlimited = state.unlimited;
    let values = state.values.clone();
    let mut fields = [Entity::PLACEHOLDER; 5];
    world.entity_mut(panel).despawn_children();
    let line = ui::row(world, panel);
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        "Fixed shell script",
        Command::Shell(true),
    );
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        "Executable and arguments",
        Command::Shell(false),
    );
    for (index, label) in [
        (0, "Slug"),
        (1, "Name"),
        (
            2,
            if shell {
                "Fixed shell script"
            } else {
                "Executable"
            },
        ),
        (3, "Literal arguments as JSON (executable mode)"),
        (4, "Execution Cell UID (empty selects this Cell)"),
    ] {
        crate::native::edit_mode::label(world, panel, label, 13.0);
        let entity = world
            .spawn(crate::native::sand::text_editor(
                &values[index],
                world.resource::<crate::native::theme::Typography>(),
                0,
            ))
            .insert((
                ChildOf(panel),
                Input { owner, index },
                Node {
                    width: percent(100),
                    min_height: px(30),
                    ..default()
                },
            ))
            .id();
        fields[index] = entity;
        let mut text = world.get_mut::<EditableText>(entity).unwrap();
        text.allow_newlines = index == 2 || index == 3;
        text.visible_lines = Some(if index == 2 { 3.0 } else { 1.0 });
        text.max_characters = Some(if index == 2 || index == 3 {
            16_384
        } else {
            256
        });
        let mut accessible = accesskit::Node::new(accesskit::Role::TextInput);
        accessible.set_label(label);
        world
            .entity_mut(entity)
            .insert(bevy::a11y::AccessibilityNode(accessible));
    }
    let line = ui::row(world, panel);
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        if unlimited {
            "Time limit: none"
        } else {
            "Time limit: 30 seconds"
        },
        Command::ToggleUnlimited,
    );
    crate::native::castle_feed::button(world, line, owner, "Save command", Command::Save);
    crate::native::castle_feed::button(world, line, owner, "Close editor", Command::Close);
    Presentation::remember(
        world,
        panel,
        Form {
            target,
            shell,
            unlimited,
            fields,
        },
    );
    nucleus::performance::add("karma-command-forms-built", 1);
}

fn result(world: &mut World, owner: Entity, panel: Entity, row: &Value) {
    let uid = row["uid"].as_str().unwrap();
    let line = ui::row(world, panel);
    crate::native::edit_mode::label(
        world,
        line,
        &format!(
            "@{} · saved value {}",
            row["slug"].as_str().unwrap_or(uid),
            row["sample"].as_str().unwrap_or("unavailable")
        ),
        14.0,
    );
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        "Edit command",
        Command::Edit(uid.into()),
    );
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        if row["unlimited"].as_bool() == Some(true) {
            "Run command"
        } else {
            "Run numeric test"
        },
        Command::Run(uid.into()),
    );
    if let Some(history) = row["history"].as_array() {
        for entry in history.iter().take(5) {
            let (at, status, output, stderr) = invocation(entry);
            crate::native::edit_mode::label(
                world,
                panel,
                &format!(
                    "{at} · {status} · {output}{}",
                    if stderr.is_empty() {
                        String::new()
                    } else {
                        format!(" · {stderr}")
                    }
                ),
                12.0,
            );
        }
    }
}
