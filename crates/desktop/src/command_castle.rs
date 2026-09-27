mod history;
#[cfg(test)]
mod tests;

use crate::{
    actions::Action,
    protein_area::{RecordBinding, Source},
    sand_panel as panel,
};
use bevy::{prelude::*, text::EditableText};
use cell::{
    ClientMessage, ServerMessage,
    command::{Request, Response, Run},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub cwd: String,
    pub show_output: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cwd: "~".into(),
            show_output: true,
        }
    }
}

#[derive(Component)]
struct CommandCastle {
    binding: RecordBinding,
    head: Entity,
    script: Entity,
    cwd: Entity,
    visibility_label: Entity,
    settings: Settings,
    status: Entity,
    history: Entity,
    runs: Vec<Run>,
    entries: HashMap<String, history::Entry>,
    pending: Option<String>,
    history_request: String,
    waiting: bool,
    next: Instant,
}

pub struct CommandCastlePlugin;

impl Plugin for CommandCastlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update.after(crate::cell_bridge::ReceiveCell))
            .add_systems(PostUpdate, settings.after(bevy::text::EditableTextSystems));
    }
}

pub(crate) fn populate(
    world: &mut World,
    owner: Entity,
    config: &crate::protein_area::Config,
    data: &serde_json::Value,
    binding: RecordBinding,
) {
    let settings = config.command.clone().unwrap_or_default();
    let title = panel::row(world, owner);
    let local = binding.source == Source::Local;
    let head = editor(
        world,
        title,
        data["head"].as_str().unwrap_or("Command"),
        false,
        &binding,
        "head",
    );
    world.get_mut::<Node>(head).unwrap().width = px(0);
    world.get_mut::<Node>(head).unwrap().flex_grow = 1.0;
    if local {
        panel::button(world, title, owner, "Run", Control::Run);
    }
    crate::edit_mode::label(world, owner, "Bash script", 13.0);
    let script = editor(
        world,
        owner,
        data["body"].as_str().unwrap_or_default(),
        true,
        &binding,
        "body",
    );
    if !local {
        crate::edit_mode::label(
            world,
            owner,
            "Open a local Command Record to run it on this machine.",
            13.0,
        );
        return;
    }
    let cwd = panel::field(
        world,
        owner,
        "Working directory on this machine (~ is your home)",
        &settings.cwd,
    );
    let controls = panel::row(world, owner);
    world
        .entity_mut(owner)
        .insert(crate::sand_store::SandCredits(crate::terminal::CREDITS));
    panel::credits(world, controls, owner, crate::terminal::CREDITS);
    let button = panel::button(
        world,
        controls,
        owner,
        visibility_caption(settings.show_output),
        Control::Visibility,
    );
    let visibility_label = world.get::<Children>(button).unwrap()[0];
    crate::edit_mode::label(
        world,
        owner,
        "Recent runs · 10 finished runs retained · 256 MiB output limit per run",
        13.0,
    );
    let status = crate::edit_mode::label(
        world,
        owner,
        "Ready · removing this Castle keeps commands running",
        12.0,
    );
    let history = panel::column(world, owner);
    world.entity_mut(owner).insert(CommandCastle {
        binding,
        head,
        script,
        cwd,
        visibility_label,
        settings,
        status,
        history,
        runs: Vec::new(),
        entries: HashMap::new(),
        pending: None,
        history_request: nucleus::new_uid("command-history"),
        waiting: false,
        next: Instant::now(),
    });
}

fn editor(
    world: &mut World,
    parent: Entity,
    value: &str,
    multiline: bool,
    binding: &RecordBinding,
    property: &str,
) -> Entity {
    if binding.source != Source::Local {
        return crate::edit_mode::label(world, parent, value, 14.0);
    }
    let entity = world
        .spawn(crate::sand::text_editor(
            value,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.max_characters = Some(65536);
    text.allow_newlines = multiline;
    text.visible_lines = Some(if multiline { 10.0 } else { 1.0 });
    world.entity_mut(entity).insert((
        Node {
            width: percent(100),
            min_width: px(0),
            height: px(if multiline { 240 } else { 36 }),
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(parent),
        crate::sand::Unsaved(false),
    ));
    let status = multiline.then(|| crate::edit_mode::label(world, parent, "Opening Record…", 12.0));
    crate::record_binding::attach(world, entity, binding.clone(), property, status);
    if multiline {
        let font = crate::terminal::code_font(world);
        world.entity_mut(entity).insert(font);
    }
    entity
}

fn visibility_caption(show: bool) -> &'static str {
    if show {
        "Show output on Run: on"
    } else {
        "Show output on Run: off"
    }
}

#[derive(Clone)]
enum Control {
    Run,
    Visibility,
    Stop(String),
    Toggle(String),
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(castle) = world.get::<CommandCastle>(owner) else {
            return;
        };
        if castle.binding.source != Source::Local || crate::laboratory::active(world) {
            return;
        }
        let status = castle.status;
        let command = castle.binding.uid.clone();
        let result = match self {
            Self::Run => {
                if castle.pending.is_some()
                    || castle.runs.iter().any(|run| run.finished_ms.is_none())
                {
                    return;
                }
                let script = value(world, castle.script);
                let cwd = value(world, castle.cwd);
                script.and_then(|script| {
                    cwd.and_then(|cwd| {
                        send(
                            world,
                            owner,
                            Request::Run {
                                command,
                                script,
                                cwd,
                            },
                        )
                    })
                })
            }
            Self::Stop(run) => send(
                world,
                owner,
                Request::Stop {
                    command,
                    run: run.clone(),
                },
            ),
            Self::Visibility => {
                let show = !castle.settings.show_output;
                let label = castle.visibility_label;
                world
                    .get_mut::<CommandCastle>(owner)
                    .unwrap()
                    .settings
                    .show_output = show;
                panel::status(world, label, visibility_caption(show));
                save_settings(world, owner);
                Ok(())
            }
            Self::Toggle(run) => history::toggle(world, owner, run),
        };
        if let Err(error) = result {
            panel::status(world, status, error);
        }
    }
}

fn value(world: &World, entity: Entity) -> Result<String, String> {
    let text = world
        .get::<EditableText>(entity)
        .ok_or("The field is unavailable")?;
    if text.is_composing() || text.pending_paste.is_some() {
        return Err("Finish typing or pasting before running".into());
    }
    Ok(text.value().to_string())
}

fn send(world: &mut World, owner: Entity, request: Request) -> Result<(), String> {
    let id = nucleus::new_uid("command-request");
    panel::send(
        world,
        ClientMessage::Command {
            id: id.clone(),
            request,
        },
    )?;
    let mut castle = world.get_mut::<CommandCastle>(owner).unwrap();
    castle.pending = Some(id);
    let status = castle.status;
    panel::status(world, status, "Sending…");
    Ok(())
}

fn save_settings(world: &mut World, owner: Entity) {
    let castle = world.get::<CommandCastle>(owner).unwrap();
    let area = castle.binding.area;
    let settings = castle.settings.clone();
    if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(area)
        && let Some(config) = area.protein.as_mut()
    {
        config.command = Some(settings);
    }
}

fn settings(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<CommandCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        let castle = world.get::<CommandCastle>(owner).unwrap();
        let fields = [castle.head, castle.script];
        for field in fields {
            let dirty = world
                .get::<crate::record_binding::TextBinding>(field)
                .zip(world.get::<EditableText>(field))
                .is_some_and(|(binding, text)| binding.unsaved(&text.value().to_string()));
            if let Some(mut unsaved) = world.get_mut::<crate::sand::Unsaved>(field) {
                unsaved.0 = dirty;
            }
        }
        let castle = world.get::<CommandCastle>(owner).unwrap();
        if let Ok(cwd) = value(world, castle.cwd)
            && cwd != castle.settings.cwd
            && cwd.len() <= 4096
        {
            world.get_mut::<CommandCastle>(owner).unwrap().settings.cwd = cwd;
            save_settings(world, owner);
        }
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() else {
        return;
    };
    let messages: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        let id = match &message {
            ServerMessage::Command { id, .. } | ServerMessage::Error { id, .. } => id,
            _ => continue,
        };
        let owners: Vec<_> = world
            .query::<(Entity, &CommandCastle)>()
            .iter(world)
            .filter(|(_, castle)| {
                castle.pending.as_ref() == Some(id) || castle.history_request == *id
            })
            .map(|(owner, _)| owner)
            .collect();
        for owner in owners {
            let mut castle = world.get_mut::<CommandCastle>(owner).unwrap();
            if castle.pending.as_ref() == Some(id) {
                castle.pending = None;
            }
            if castle.history_request == *id {
                castle.waiting = false;
            }
            let status = castle.status;
            match &message {
                ServerMessage::Command {
                    response: Response::Started { run },
                    ..
                } => {
                    castle.runs.insert(0, run.clone());
                    castle.next = Instant::now();
                    let show = castle.settings.show_output;
                    history::reconcile(world, owner);
                    panel::status(world, status, "Running on this machine");
                    if show && let Err(error) = history::toggle(world, owner, &run.id) {
                        panel::status(
                            world,
                            status,
                            format!("Running · output unavailable: {error}"),
                        );
                    }
                }
                ServerMessage::Command {
                    response: Response::History { runs },
                    ..
                } => {
                    if castle.runs != *runs {
                        castle.runs = runs.clone();
                        history::reconcile(world, owner);
                    }
                }
                ServerMessage::Command {
                    response: Response::Ok,
                    ..
                } => {
                    castle.next = Instant::now();
                    panel::status(world, status, "Stop requested");
                }
                ServerMessage::Error { message, .. } => panel::status(world, status, message),
                _ => {}
            }
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<CommandCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        let castle = world.get::<CommandCastle>(owner).unwrap();
        if castle.waiting || Instant::now() < castle.next {
            continue;
        }
        if panel::send(
            world,
            ClientMessage::Command {
                id: castle.history_request.clone(),
                request: Request::History {
                    command: castle.binding.uid.clone(),
                },
            },
        )
        .is_ok()
        {
            let mut castle = world.get_mut::<CommandCastle>(owner).unwrap();
            castle.waiting = true;
            castle.next = Instant::now() + Duration::from_secs(1);
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.after(Duration::from_secs(1));
            }
        }
    }
}
