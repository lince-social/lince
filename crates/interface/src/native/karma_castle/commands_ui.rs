use bevy::{ecs::entity_disabling::Disabled, prelude::*, text::EditableText};

use super::*;
mod presentation;

#[derive(Component)]
pub(super) struct Commands {
    panel: Entity,
    rows: Arc<Vec<Arc<Value>>>,
    pending: Option<(String, bool)>,
    target: Option<String>,
    revision: Option<i64>,
    values: [String; 5],
    shell: bool,
    unlimited: bool,
    editing: bool,
}

impl replies::Pending for Commands {
    fn pending_id(&self) -> Option<&str> {
        self.pending.as_ref().map(|pending| pending.0.as_str())
    }
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
    ToggleUnlimited,
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
        rows: Arc::default(),
        pending: None,
        target: None,
        revision: None,
        values: Default::default(),
        shell: true,
        unlimited: false,
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
    world
        .run_system_cached_with(capture_fields, owner)
        .expect("capture Karma command fields");
}

fn capture_fields(
    In(owner): In<Entity>,
    inputs: Query<(&Input, &EditableText)>,
    mut commands: Query<&mut Commands, Allow<Disabled>>,
) {
    let _capture = nucleus::performance::Span::new("karma-command-field-capture");
    let Ok(mut state) = commands.get_mut(owner) else { return };
    for (input, text) in &inputs {
        if input.owner == owner && text.value() != state.values[input.index].as_str() {
            crate::controls::copy_value(text, &mut state.values[input.index]);
            nucleus::performance::add("karma-command-fields-captured", 1);
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

impl crate::native::actions::Action for Command {
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
        if crate::native::laboratory::suspended(world, owner)
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
                state.unlimited = false;
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
                state.unlimited = row["unlimited"].as_bool().unwrap_or(false);
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
            Self::ToggleUnlimited => {
                let mut state = world.get_mut::<Commands>(owner).unwrap();
                state.unlimited = !state.unlimited;
            }
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
                    unlimited: state.unlimited,
                };
                submit(world, owner, action, false);
                return;
            }
            Self::Run(command) => {
                let numeric = !world.get::<Commands>(owner).unwrap().rows.iter().any(|row| {
                    row["uid"] == *command && row["unlimited"].as_bool() == Some(true)
                });
                submit(
                    world,
                    owner,
                    engine::actions::Action::RunKarmaCommand {
                        command: command.clone(),
                        request_id: nucleus::new_uid("command-run"),
                        numeric,
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
    presentation::render(world, owner);
}

pub(super) fn receive(world: &mut World, owner: Entity, message: &ServerMessage) {
    capture(world, owner);
    let (_, inspect) = world
        .get_mut::<Commands>(owner)
        .unwrap()
        .pending
        .take()
        .unwrap();
    match message {
        ServerMessage::ActionOk { data, .. } if inspect => {
            let rows = data
                .as_ref()
                .and_then(|data| data["commands"].as_array())
                .map_or(&[][..], Vec::as_slice);
            if incoming::publish_rows(&mut world.get_mut::<Commands>(owner).unwrap().rows, rows) {
                nucleus::performance::add("karma-command-rows-published", rows.len() as u64);
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code, reason = "Each test target uses a different subset of the included Karma fixtures")]
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
        let mut app = crate::native::sand_panel::tests::app();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .add_plugins(KarmaCastlePlugin);
        crate::native::sand_panel::tests::connect(&mut app, engine.clone());
        let root = app
            .world_mut()
            .spawn(crate::native::workspace::Workspaces::default())
            .id();
        let owner = super::super::spawn(
            app.world_mut(),
            root,
            1,
            DVec2::ZERO,
            KarmaCastle::default(),
        );
        crate::native::sand_panel::tests::settle(&mut app, |world| {
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
        crate::native::sand_panel::tests::settle(&mut app, |world| {
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
        crate::native::sand_panel::tests::settle(&mut app, |world| {
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
        crate::native::sand_panel::tests::settle(&mut app, |world| {
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
        crate::native::sand_panel::tests::settle(&mut app, |world| {
            world.get::<Commands>(owner).unwrap().pending.is_none()
        })
        .await;
        let row = &app.world().get::<Commands>(owner).unwrap().rows[0];
        assert_eq!(row["revision"], 2);
        assert_eq!(row["configuration"]["arguments"][1], "$(exit 99)");
        assert_eq!(row["history"].as_array().unwrap().len(), 1);
    }
}
