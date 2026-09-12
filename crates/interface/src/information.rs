use bevy::{a11y::AccessibilityNode, prelude::*};
use cell::information::{
    Availability, Information, InformationChannel, UpdateCommand, UpdatePhase,
};

use crate::{
    actions::{Action, ActionButton},
    edit_mode::{EditAction, EditMode, label},
};

#[derive(Resource, Default)]
pub struct InformationState {
    pub current: Option<Information>,
    revision: u64,
    notified: Option<String>,
    restart_requested: bool,
}

#[derive(Resource)]
struct Connection {
    channel: InformationChannel,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Component)]
struct InformationPanel {
    root: Entity,
    revision: Option<u64>,
}

#[derive(Component)]
struct UpdateNotice(Entity);

pub struct OpenInformation;

impl Action for OpenInformation {
    fn apply(&self, world: &mut World, mut target: Entity) {
        while world.get::<EditMode>(target).is_none() {
            let Some(parent) = world.get::<ChildOf>(target) else {
                return;
            };
            target = parent.parent();
        }
        EditAction::Open.apply(world, target);
        EditAction::Information.apply(world, target);
    }
}

#[derive(Clone, Copy)]
struct UpdateAction(UpdateCommand);

struct RetryRestart;

impl Action for RetryRestart {
    fn apply(&self, world: &mut World, _: Entity) {
        world.resource_mut::<InformationState>().restart_requested = false;
    }
}

impl Action for UpdateAction {
    fn apply(&self, world: &mut World, _: Entity) {
        let result = world
            .get_resource::<Connection>()
            .ok_or_else(|| "The update service is not connected.".to_string())
            .and_then(|connection| connection.channel.request(self.0));
        if let Err(error) = result {
            crate::notifications::report(world, "cell::updates", &error);
        }
    }
}

pub struct InformationPlugin;

impl Plugin for InformationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InformationState>()
            .add_systems(Startup, connect)
            .add_systems(Update, (receive, notice, render).chain())
            .add_systems(
                PostUpdate,
                restart
                    .after(crate::actions::ApplyActions)
                    .run_if(crate::laboratory::normal),
            );
    }
}

fn connect(world: &mut World) {
    let Some(channel) = world
        .get_resource::<crate::app::CellHandle>()
        .and_then(|runtime| runtime.0.information.clone())
    else {
        return;
    };
    let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned() else {
        return;
    };
    let mut changes = channel.state.clone();
    let task = tokio::spawn(async move {
        while changes.changed().await.is_ok() {
            wake.ring();
        }
    });
    world.insert_resource(Connection { channel, task });
}

fn receive(world: &mut World) {
    let Some(mut connection) = world.get_resource_mut::<Connection>() else {
        return;
    };
    let current = connection.channel.state.borrow_and_update().clone();
    if world.resource::<InformationState>().current.as_ref() == Some(&current) {
        return;
    }
    let available = current
        .update
        .as_ref()
        .filter(|update| update.availability == Availability::Available);
    if let Some(update) = available
        && world.resource::<InformationState>().notified.as_deref() != Some(&update.revision)
    {
        crate::notifications::report(
            world,
            "cell::update_available",
            &format!(
                "Lince {} is available. Open Information to update.",
                update.version
            ),
        );
        world.resource_mut::<InformationState>().notified = Some(update.revision.clone());
    }
    let mut state = world.resource_mut::<InformationState>();
    state.current = Some(current);
    state.revision += 1;
}

fn restart(world: &mut World) {
    let state = world.resource::<InformationState>();
    if state.restart_requested
        || !state
            .current
            .as_ref()
            .is_some_and(|state| state.phase == UpdatePhase::ReadyToRestart)
    {
        return;
    }
    if world
        .query::<&bevy::text::EditableText>()
        .iter(world)
        .any(|text| text.is_composing() || crate::record_view::pending_text(text))
    {
        return;
    }
    if world
        .query::<(&crate::record_view::RecordEditor, &bevy::text::EditableText)>()
        .iter(world)
        .any(|(record, text)| {
            record.pending.is_some()
                || text.is_composing()
                || crate::record_view::pending_text(text)
                || text.value().to_string() != record.confirmed
        })
    {
        return;
    }
    world.resource_mut::<InformationState>().restart_requested = true;
    world.write_message(AppExit::Success);
}

fn notice(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, (With<EditMode>, Without<UpdateNotice>)>()
        .iter(world)
        .collect();
    for root in roots {
        let toolbar = crate::canvas_controls::toolbar(world, root);
        let button = action_button(
            world,
            toolbar,
            root,
            "Update available",
            crate::actions![OpenInformation],
        );
        world.entity_mut(root).insert(UpdateNotice(button));
    }
    let available = world
        .resource::<InformationState>()
        .current
        .as_ref()
        .and_then(|state| state.update.as_ref())
        .is_some_and(|update| update.availability == Availability::Available);
    let buttons: Vec<_> = world
        .query::<&UpdateNotice>()
        .iter(world)
        .map(|notice| notice.0)
        .collect();
    for button in buttons {
        if let Some(mut node) = world.get_mut::<Node>(button) {
            let display = if available {
                Display::Flex
            } else {
                Display::None
            };
            if node.display != display {
                node.display = display;
            }
        }
    }
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity) {
    world.spawn((
        InformationPanel {
            root,
            revision: None,
        },
        ChildOf(parent),
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            flex_shrink: 0.0,
            width: percent(100),
            ..default()
        },
    ));
}

pub(crate) fn open_button(world: &mut World, parent: Entity, target: Entity) {
    action_button(
        world,
        parent,
        target,
        "Open Information",
        crate::actions![OpenInformation],
    );
}

pub(crate) fn action_button(
    world: &mut World,
    parent: Entity,
    root: Entity,
    name: &str,
    actions: crate::actions::ActionSequence,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            ActionButton::new(root, actions),
            ChildOf(parent),
            Node {
                padding: UiRect::axes(px(10), px(7)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            crate::token_style::border(crate::tokens::Token::Accent),
        ))
        .id();
    world
        .get_mut::<AccessibilityNode>(entity)
        .unwrap()
        .set_label(name);
    label(world, entity, name, 15.0);
    entity
}

fn render(world: &mut World) {
    let revision = world.resource::<InformationState>().revision;
    let panels: Vec<_> = world
        .query::<(Entity, &InformationPanel)>()
        .iter(world)
        .filter(|(_, panel)| panel.revision != Some(revision))
        .map(|(entity, panel)| (entity, panel.root))
        .collect();
    if panels.is_empty() {
        return;
    }
    let state = world.resource::<InformationState>().current.clone();
    for (panel, root) in panels {
        world.entity_mut(panel).despawn_children();
        world.get_mut::<InformationPanel>(panel).unwrap().revision = Some(revision);
        label(world, panel, "Information", 22.0);
        action_button(
            world,
            panel,
            root,
            "Laboratory",
            crate::actions![crate::laboratory::LaboratoryAction::Open],
        );
        let Some(state) = &state else {
            label(
                world,
                panel,
                "The Cell information service is not connected.",
                15.0,
            );
            continue;
        };
        for (name, value) in [
            ("Version", state.version.clone()),
            (
                "Commit",
                if state.revision == "unknown" {
                    "Unstamped build — updates cannot be compared".into()
                } else {
                    state.revision.clone()
                },
            ),
            (
                "Port / address",
                state
                    .address
                    .clone()
                    .unwrap_or_else(|| "Not listening for browser connections".into()),
            ),
            ("Lince directory", state.directory.display().to_string()),
            ("Program", state.executable.display().to_string()),
            (
                "Last updated (file date)",
                state
                    .last_updated
                    .clone()
                    .unwrap_or_else(|| "Unknown".into()),
            ),
            (
                "Last checked",
                state
                    .last_checked
                    .clone()
                    .unwrap_or_else(|| "Not checked yet".into()),
            ),
        ] {
            label(world, panel, &format!("{name}: {value}"), 14.0);
        }
        let busy = matches!(
            state.phase,
            UpdatePhase::Checking | UpdatePhase::Downloading
        );
        let status = match state.phase {
            UpdatePhase::Checking => "Checking for updates…",
            UpdatePhase::Downloading => "Downloading and verifying the update…",
            UpdatePhase::ReadyToRestart => "Update installed. Restarting after your work is saved…",
            UpdatePhase::Idle => match state.update.as_ref().map(|update| update.availability) {
                Some(Availability::UpToDate) => "Lince is up to date.",
                Some(Availability::Unstamped) => {
                    "This build has no commit stamp, so it cannot be compared."
                }
                Some(Availability::Available) => "An update is available.",
                None => "Check for a published update.",
            },
        };
        label(world, panel, status, 15.0);
        if state.phase == UpdatePhase::ReadyToRestart {
            action_button(
                world,
                panel,
                root,
                "Save and restart",
                crate::actions![RetryRestart],
            );
        }
        if let Some(update) = &state.update {
            if update.availability == Availability::Available {
                label(
                    world,
                    panel,
                    &format!("Available: {} ({})", update.version, update.revision),
                    14.0,
                );
            }
            label(world, panel, &update.self_apply_note, 14.0);
            if update.can_self_apply && !busy && state.phase != UpdatePhase::ReadyToRestart {
                action_button(
                    world,
                    panel,
                    root,
                    "Download and restart",
                    crate::actions![UpdateAction(UpdateCommand::DownloadAndRestart)],
                );
            }
        } else {
            label(world, panel, &state.note, 14.0);
        }
        if let Some(error) = &state.error {
            label(world, panel, error, 14.0);
        }
        if !state.checks_enabled {
            label(world, panel, "Checks disabled by LINCE_UPDATE_CHECK.", 14.0);
        } else if !busy && state.phase != UpdatePhase::ReadyToRestart {
            action_button(
                world,
                panel,
                root,
                "Check for updates",
                crate::actions![UpdateAction(UpdateCommand::Check)],
            );
            if !state.server {
                action_button(
                    world,
                    panel,
                    root,
                    if state.automatic {
                        "Automatic updates: On"
                    } else {
                        "Automatic updates: Off"
                    },
                    crate::actions![UpdateAction(UpdateCommand::Automatic(!state.automatic))],
                );
            }
        }
    }
}

pub(crate) mod tests {
    use super::*;

    fn state() -> Information {
        Information {
            version: "0.7.0".into(),
            revision: "unknown".into(),
            directory: "/data/lince".into(),
            executable: "/nix/store/lince/bin/lince".into(),
            address: Some("127.0.0.1:6174".into()),
            last_updated: Some("2026-09-12".into()),
            last_checked: None,
            automatic: false,
            server: false,
            checks_enabled: true,
            phase: UpdatePhase::Idle,
            update: None,
            note: "Update with nixos-rebuild switch".into(),
            error: None,
        }
    }

    fn fixture() -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .insert_resource(crate::notifications::Notifications::new(
                cell::Diagnostics::default(),
            ))
            .add_plugins((
                crate::theme::ThemePlugin,
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
                crate::notifications::NotificationsPlugin,
                InformationPlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        app.world_mut().resource_mut::<InformationState>().current = Some(state());
        (app, root)
    }

    fn texts(app: &mut App) -> Vec<String> {
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect()
    }

    #[cfg_attr(test, test)]
    fn information_shows_runtime_details_and_preserves_its_nodes_while_idle() {
        let (mut app, root) = fixture();
        OpenInformation.apply(app.world_mut(), root);
        app.update();
        let text = texts(&mut app).join("\n");
        for expected in [
            "Version: 0.7.0",
            "Unstamped build",
            "127.0.0.1:6174",
            "/data/lince",
            "2026-09-12",
            "nixos-rebuild",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert!(!text.contains("Download and restart"));
        let children: Vec<_> = app
            .world_mut()
            .query::<(Entity, &Text)>()
            .iter(app.world())
            .map(|(entity, _)| entity)
            .collect();
        app.update();
        assert!(
            children
                .iter()
                .all(|entity| app.world().get_entity(*entity).is_ok())
        );
        EditAction::General.apply(app.world_mut(), root);
        app.update();
        assert!(
            !texts(&mut app)
                .iter()
                .any(|text| text.starts_with("Version:"))
        );
    }

    #[cfg_attr(test, test)]
    fn update_notification_opens_information_and_shows_only_supported_actions() {
        let (mut app, root) = fixture();
        let mut update = state();
        update.revision = "old".into();
        update.update = Some(cell::information::UpdateStatus {
            availability: Availability::Available,
            version: "0.7.1".into(),
            revision: "new".into(),
            channel: "rolling".into(),
            asset_name: Some("lince.AppImage".into()),
            asset_url: Some("https://example.com/lince.AppImage".into()),
            asset_sha256: Some("ab".repeat(32)),
            can_self_apply: true,
            self_apply_note: "This build can replace itself.".into(),
        });
        app.world_mut().resource_mut::<InformationState>().current = Some(update);
        crate::notifications::report(
            app.world(),
            "cell::update_available",
            "Lince 0.7.1 is available.",
        );
        EditAction::Open.apply(app.world_mut(), root);
        EditAction::Notifications.apply(app.world_mut(), root);
        app.update();
        let button = app
            .world_mut()
            .query::<(Entity, &AccessibilityNode)>()
            .iter(app.world())
            .find(|(_, node)| node.label() == Some("Open Information"))
            .unwrap()
            .0;
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: button });
        app.update();
        app.update();
        let text = texts(&mut app).join("\n");
        assert!(text.contains("Available: 0.7.1 (new)"));
        assert!(text.contains("Download and restart"));
        assert!(text.contains("Automatic updates: Off"));
        {
            let mut state = app.world_mut().resource_mut::<InformationState>();
            state.current.as_mut().unwrap().phase = UpdatePhase::Downloading;
            state.revision += 1;
        }
        app.update();
        let text = texts(&mut app).join("\n");
        assert!(text.contains("Downloading and verifying"));
        assert!(!text.contains("Download and restart"));
        assert!(!text.contains("Check for updates"));
    }

    #[cfg_attr(test, test)]
    fn restart_waits_for_record_confirmation_then_uses_the_normal_exit_path() {
        let (mut app, _) = fixture();
        app.world_mut()
            .resource_mut::<InformationState>()
            .current
            .as_mut()
            .unwrap()
            .phase = UpdatePhase::ReadyToRestart;
        let status = app.world_mut().spawn_empty().id();
        let editor = app
            .world_mut()
            .spawn((
                crate::record_view::RecordEditor {
                    uid: "record".into(),
                    confirmed: "old".into(),
                    pending: Some(("save".into(), "new".into())),
                    status,
                },
                bevy::text::EditableText::default(),
            ))
            .id();
        restart(app.world_mut());
        assert!(!app.world().resource::<InformationState>().restart_requested);
        app.world_mut()
            .get_mut::<crate::record_view::RecordEditor>(editor)
            .unwrap()
            .pending = None;
        restart(app.world_mut());
        assert!(!app.world().resource::<InformationState>().restart_requested);
        app.world_mut()
            .get_mut::<crate::record_view::RecordEditor>(editor)
            .unwrap()
            .confirmed
            .clear();
        restart(app.world_mut());
        assert!(app.world().resource::<InformationState>().restart_requested);
        assert_eq!(app.world().resource::<Messages<AppExit>>().len(), 1);
    }

    crate::laboratory_cases! {
        information_shows_runtime_details_and_preserves_its_nodes_while_idle,
        update_notification_opens_information_and_shows_only_supported_actions,
        restart_waits_for_record_confirmation_then_uses_the_normal_exit_path,
    }
}
