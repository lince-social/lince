use bevy::{prelude::*, text::EditableText};

use super::*;

#[derive(Component)]
struct Commands {
    panel: Entity,
    rows: Vec<Value>,
    pending: Option<(String, bool)>,
    target: Option<String>,
    revision: Option<i64>,
    values: [String; 5],
    shell: bool,
    editing: bool,
}

#[derive(Component)]
struct Input {
    owner: Entity,
    index: usize,
}

#[derive(Clone)]
enum Command {
    Refresh,
    New,
    Edit(String),
    Shell(bool),
    Save,
    Run(String),
    Close,
}

pub(crate) fn edit_shell(world: &mut World, owner: Entity, slug: &str, head: &str, script: &str) {
    Command::New.apply(world, owner);
    if let Some(mut state) = world.get_mut::<Commands>(owner) {
        state.values = [
            slug.into(),
            head.into(),
            script.into(),
            "[]".into(),
            String::new(),
        ];
        state.shell = true;
        state.editing = true;
        render(world, owner);
    }
}

pub(crate) fn save(world: &mut World, owner: Entity) {
    Command::Save.apply(world, owner);
}

pub(crate) fn saved(world: &World, owner: Entity, slug: &str) -> Option<String> {
    world
        .get::<Commands>(owner)?
        .rows
        .iter()
        .find(|row| row["slug"] == slug)?["uid"]
        .as_str()
        .map(str::to_string)
}

pub(crate) fn run(world: &mut World, owner: Entity, command: String) {
    Command::Run(command).apply(world, owner);
}

pub(crate) fn refresh(world: &mut World, owner: Entity) {
    Command::Refresh.apply(world, owner);
}

pub(crate) fn sampled(world: &World, owner: Entity, slug: &str, value: &str) -> bool {
    world.get::<Commands>(owner).is_some_and(|state| {
        state.rows.iter().any(|row| {
            row["slug"] == slug
                && row["history"].as_array().is_some_and(|history| {
                    history.iter().any(|invocation| {
                        invocation["status"] == "completed"
                            && invocation["value"].as_str() == Some(value)
                    })
                })
        })
    })
}

pub(super) fn spawn(world: &mut World, owner: Entity, parent: Entity) {
    let panel = ui::stack(world, parent);
    world.entity_mut(panel).insert(Name::new("Karma commands"));
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    world.entity_mut(owner).insert(Commands {
        panel,
        rows: Vec::new(),
        pending: None,
        target: None,
        revision: None,
        values: Default::default(),
        shell: true,
        editing: false,
    });
    render(world, owner);
}

pub(super) fn toggle(world: &mut World, owner: Entity) {
    let panel = world.get::<Commands>(owner).unwrap().panel;
    let mut node = world.get_mut::<Node>(panel).unwrap();
    node.display = if node.display == Display::None {
        Display::Flex
    } else {
        Display::None
    };
}

fn capture(world: &mut World, owner: Entity) {
    let inputs: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.index, text.value().to_string()))
        .collect();
    if let Some(mut state) = world.get_mut::<Commands>(owner) {
        for (index, value) in inputs {
            state.values[index] = value;
        }
    }
}

fn submit(world: &mut World, owner: Entity, action: engine::actions::Action, inspect: bool) {
    let id = nucleus::new_uid("commands-ui");
    match send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            world.get_mut::<Commands>(owner).unwrap().pending = Some((id, inspect));
        }
        Err(error) => status(world, owner, error),
    }
}

impl crate::actions::Action for Command {
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        use lince_interface::practice::Operation;
        match self {
            Self::Save => &[Operation::SaveCommand],
            Self::Run(_) => &[Operation::RunCommand],
            _ => &[],
        }
    }

    fn tutorial_supports(&self) -> &'static [lince_interface::practice::Operation] {
        use lince_interface::practice::Operation;
        match self {
            Self::Refresh => &[Operation::SaveCommand, Operation::RunCommand],
            Self::Shell(_) => &[Operation::SaveCommand],
            _ => &[],
        }
    }

    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner)
            || world
                .get::<Commands>(owner)
                .is_none_or(|state| state.pending.is_some())
        {
            return;
        }
        capture(world, owner);
        match self {
            Self::Refresh => {
                submit(
                    world,
                    owner,
                    engine::actions::Action::InspectKarmaCommands,
                    true,
                );
                return;
            }
            Self::New => {
                let mut state = world.get_mut::<Commands>(owner).unwrap();
                state.target = None;
                state.revision = None;
                state.values = Default::default();
                state.shell = true;
                state.editing = true;
                let panel = state.panel;
                world.get_mut::<Node>(panel).unwrap().display = Display::Flex;
            }
            Self::Edit(uid) => {
                let row = world
                    .get::<Commands>(owner)
                    .unwrap()
                    .rows
                    .iter()
                    .find(|row| row["uid"] == *uid)
                    .cloned();
                let Some(row) = row else { return };
                let mut state = world.get_mut::<Commands>(owner).unwrap();
                state.target = Some(uid.clone());
                state.revision = row["revision"].as_i64();
                state.shell = row["configuration"]["kind"] == "shell";
                state.editing = true;
                state.values = [
                    row["slug"].as_str().unwrap_or_default().into(),
                    row["head"].as_str().unwrap_or_default().into(),
                    row["configuration"][if state.shell { "script" } else { "program" }]
                        .as_str()
                        .unwrap_or_default()
                        .into(),
                    row["configuration"]["arguments"].as_array().map_or_else(
                        || "[]".into(),
                        |arguments| serde_json::to_string(arguments).unwrap(),
                    ),
                    row["host"].as_str().unwrap_or_default().into(),
                ];
            }
            Self::Shell(shell) => world.get_mut::<Commands>(owner).unwrap().shell = *shell,
            Self::Close => world.get_mut::<Commands>(owner).unwrap().editing = false,
            Self::Save => {
                let state = world.get::<Commands>(owner).unwrap();
                let configuration = if state.shell {
                    nucleus::command::Command::Shell {
                        script: state.values[2].clone(),
                    }
                } else {
                    let arguments = match serde_json::from_str(&state.values[3]) {
                        Ok(arguments) => arguments,
                        Err(error) => {
                            status(
                                world,
                                owner,
                                format!("Arguments must be a JSON list of strings: {error}"),
                            );
                            return;
                        }
                    };
                    nucleus::command::Command::Process {
                        program: state.values[2].clone(),
                        arguments,
                    }
                };
                let action = engine::actions::Action::SaveKarmaCommand {
                    target: state.target.clone(),
                    expected_revision: state.revision,
                    slug: state.values[0].clone(),
                    head: state.values[1].clone(),
                    configuration,
                    host: (!state.values[4].is_empty()).then(|| state.values[4].clone()),
                };
                submit(world, owner, action, false);
                return;
            }
            Self::Run(command) => {
                submit(
                    world,
                    owner,
                    engine::actions::Action::RunKarmaCommand {
                        command: command.clone(),
                        request_id: nucleus::new_uid("command-run"),
                        numeric: true,
                    },
                    false,
                );
                return;
            }
        }
        render(world, owner);
    }
}

fn render(world: &mut World, owner: Entity) {
    let panel = world.get::<Commands>(owner).unwrap().panel;
    if let Some(children) = world.get::<Children>(panel) {
        let children: Vec<_> = children.iter().collect();
        for child in children {
            world.despawn(child);
        }
    }
    crate::edit_mode::label(world, panel, "Commands and numeric samples", 16.0);
    let line = ui::row(world, panel);
    for (label, command) in [
        ("New command", Command::New),
        ("Refresh results", Command::Refresh),
    ] {
        crate::castle_feed::button(world, line, owner, label, command);
    }
    crate::edit_mode::label(
        world,
        panel,
        "query_command(@source) runs a fresh numeric query. signal(@source) reads its saved successful sample. @target: run(@source) runs a saved command as a consequence.",
        13.0,
    );
    let state = world.get::<Commands>(owner).unwrap();
    let editing = state.editing;
    let shell = state.shell;
    let values = state.values.clone();
    let rows = state.rows.clone();
    if editing {
        let line = ui::row(world, panel);
        crate::castle_feed::button(
            world,
            line,
            owner,
            "Fixed shell script",
            Command::Shell(true),
        );
        crate::castle_feed::button(
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
            crate::edit_mode::label(world, panel, label, 13.0);
            let entity = world
                .spawn(crate::sand::text_editor(
                    &values[index],
                    world.resource::<crate::theme::Typography>(),
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
        crate::castle_feed::button(world, line, owner, "Save command", Command::Save);
        crate::castle_feed::button(world, line, owner, "Close editor", Command::Close);
    }
    for row in rows {
        let Some(uid) = row["uid"].as_str() else {
            continue;
        };
        let line = ui::row(world, panel);
        crate::edit_mode::label(
            world,
            line,
            &format!(
                "@{} · saved value {}",
                row["slug"].as_str().unwrap_or(uid),
                row["sample"].as_str().unwrap_or("unavailable")
            ),
            14.0,
        );
        crate::castle_feed::button(
            world,
            line,
            owner,
            "Edit command",
            Command::Edit(uid.into()),
        );
        crate::castle_feed::button(
            world,
            line,
            owner,
            "Run numeric test",
            Command::Run(uid.into()),
        );
        if let Some(history) = row["history"].as_array() {
            for invocation in history.iter().take(5) {
                crate::edit_mode::label(
                    world,
                    panel,
                    &format!(
                        "{} · {} · {}{}",
                        invocation["at"].as_str().unwrap_or_default(),
                        invocation["status"].as_str().unwrap_or_default(),
                        invocation["error"]
                            .as_str()
                            .or_else(|| invocation["value"].as_str())
                            .or_else(|| invocation["stdout"].as_str())
                            .unwrap_or_default(),
                        invocation["stderr"]
                            .as_str()
                            .filter(|text| !text.is_empty())
                            .map_or_else(String::new, |text| format!(" · {text}"))
                    ),
                    12.0,
                );
            }
        }
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &Commands)>()
        .iter(world)
        .find(|(_, state)| {
            state
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| pending == id)
        })
        .map(|(owner, _)| owner);
    let Some(owner) = owner else { return false };
    capture(world, owner);
    let (_, inspect) = world
        .get_mut::<Commands>(owner)
        .unwrap()
        .pending
        .take()
        .unwrap();
    match message {
        ServerMessage::ActionOk { data, .. } if inspect => {
            world.get_mut::<Commands>(owner).unwrap().rows = data
                .as_ref()
                .and_then(|data| data["commands"].as_array())
                .cloned()
                .unwrap_or_default();
            render(world, owner);
        }
        ServerMessage::ActionOk { created, .. } => {
            if let Some(uid) = created {
                let mut state = world.get_mut::<Commands>(owner).unwrap();
                state.target = Some(uid.clone());
                state.editing = false;
            }
            status(
                world,
                owner,
                "Command accepted. Refresh results to inspect its outcome.",
            );
            submit(
                world,
                owner,
                engine::actions::Action::InspectKarmaCommands,
                true,
            );
        }
        ServerMessage::Error { message, .. } => status(world, owner, message),
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    mod karma {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../engine/tests/support/karma.rs"
        ));
    }

    #[tokio::test]
    async fn authoring_saves_literal_configuration_and_only_run_collects_a_sample() {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        karma::authorize(&engine).await;
        let mut app = crate::sand_panel::tests::app();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .add_plugins(KarmaCastlePlugin);
        crate::sand_panel::tests::connect(&mut app, engine.clone());
        let root = app
            .world_mut()
            .spawn(crate::workspace::Workspaces::default())
            .id();
        let owner = super::super::spawn(
            app.world_mut(),
            root,
            1,
            DVec2::ZERO,
            KarmaCastle::default(),
        );
        crate::sand_panel::tests::settle(&mut app, |world| {
            world.get::<View>(owner).is_some_and(|view| view.ready)
        })
        .await;
        Command::New.apply(app.world_mut(), owner);
        let inputs: Vec<_> = app
            .world_mut()
            .query::<(Entity, &Input)>()
            .iter(app.world())
            .filter(|(_, input)| input.owner == owner)
            .map(|(entity, input)| (entity, input.index))
            .collect();
        for (entity, index) in inputs {
            let value = ["reader", "Reader", "printf -- -3.25", "[]", ""][index];
            app.world_mut()
                .get_mut::<EditableText>(entity)
                .unwrap()
                .editor
                .set_text(value);
        }
        assert!(
            store::misc::due_effects(&engine.store.pool)
                .await
                .unwrap()
                .is_empty()
        );
        Command::Save.apply(app.world_mut(), owner);
        crate::sand_panel::tests::settle(&mut app, |world| {
            world
                .get::<Commands>(owner)
                .is_some_and(|state| state.pending.is_none() && state.rows.len() == 1)
        })
        .await;
        let row = app.world().get::<Commands>(owner).unwrap().rows[0].clone();
        assert_eq!(row["configuration"]["script"], "printf -- -3.25");
        assert!(row["sample"].is_null());
        let uid = row["uid"].as_str().unwrap().to_string();
        Command::Run(uid.clone()).apply(app.world_mut(), owner);
        crate::sand_panel::tests::settle(&mut app, |world| {
            world.get::<Commands>(owner).unwrap().pending.is_none()
        })
        .await;
        assert!(
            engine
                .run_due_effects()
                .await
                .unwrap()
                .iter()
                .all(|result| result.ok)
        );
        Command::Refresh.apply(app.world_mut(), owner);
        crate::sand_panel::tests::settle(&mut app, |world| {
            world.get::<Commands>(owner).unwrap().pending.is_none()
        })
        .await;
        assert_eq!(
            app.world().get::<Commands>(owner).unwrap().rows[0]["sample"],
            "-3.25"
        );
        Command::Edit(uid).apply(app.world_mut(), owner);
        Command::Shell(false).apply(app.world_mut(), owner);
        let inputs: Vec<_> = app
            .world_mut()
            .query::<(Entity, &Input)>()
            .iter(app.world())
            .filter(|(_, input)| input.owner == owner && matches!(input.index, 2 | 3))
            .map(|(entity, input)| (entity, input.index))
            .collect();
        for (entity, index) in inputs {
            app.world_mut()
                .get_mut::<EditableText>(entity)
                .unwrap()
                .editor
                .set_text(if index == 2 {
                    "printf"
                } else {
                    "[\"%s\",\"$(exit 99)\"]"
                });
        }
        Command::Save.apply(app.world_mut(), owner);
        crate::sand_panel::tests::settle(&mut app, |world| {
            world.get::<Commands>(owner).unwrap().pending.is_none()
        })
        .await;
        let row = &app.world().get::<Commands>(owner).unwrap().rows[0];
        assert_eq!(row["revision"], 2);
        assert_eq!(row["configuration"]["arguments"][1], "$(exit 99)");
        assert_eq!(row["history"].as_array().unwrap().len(), 1);
    }
}
