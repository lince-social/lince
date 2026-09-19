use crate::{
    actions::Action,
    protein_area::{RecordBinding, Source},
};
use bevy::{prelude::*, text::EditableText};
use cell::{FioteProvider, FioteRequest, FioteSecret, FioteSettings, FioteStatus};

#[derive(Component)]
struct Panel {
    binding: RecordBinding,
    form: Entity,
    status: Entity,
    provider: FioteProvider,
    provider_label: Entity,
    model: Entity,
    endpoint: Entity,
    key: Entity,
    directory: Entity,
    pending: Option<String>,
    loaded: bool,
    enabled: bool,
}

#[derive(Component)]
struct Mask(Entity);

#[derive(Component)]
struct ThreadControl {
    record: String,
    thread: String,
    status: Entity,
    pending: Option<String>,
}

pub struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (receive.after(crate::cell_bridge::ReceiveCell), masks),
        )
        .add_systems(
            PreUpdate,
            protect_keys.before(bevy::text::EditableTextSystems),
        );
    }
}

fn field(world: &mut World, parent: Entity, title: &str) -> Entity {
    crate::edit_mode::label(world, parent, title, 14.0);
    let bundle = crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0);
    let entity = world.spawn((bundle, ChildOf(parent))).id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(4096);
    entity
}

pub fn populate(world: &mut World, parent: Entity, binding: RecordBinding) {
    if binding.source != Source::Local || !nucleus::valid_uid(&binding.uid, "r") {
        return;
    }
    let owner = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: percent(100),
                row_gap: px(6),
                ..default()
            },
            ChildOf(parent),
            crate::sand_store::SandCredits(super::CREDITS),
        ))
        .id();
    crate::description::button(world, owner, owner, "Fiote settings", Toggle);
    let status = crate::edit_mode::label(world, owner, "", 14.0);
    let form = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    crate::edit_mode::label(
        world,
        form,
        "The Record description is Fiote's system prompt. Each thread is a separate session. Enabling sends those messages and the description to your provider.",
        14.0,
    );
    let provider_label =
        crate::edit_mode::label(world, form, FioteProvider::default().name(), 16.0);
    crate::description::button(world, form, owner, "Change provider", ChangeProvider);
    let model = field(world, form, "Model");
    let endpoint = field(
        world,
        form,
        "API base URL · blank uses the provider default",
    );
    let key = field(
        world,
        form,
        "API key · blank keeps the saved key for this endpoint",
    );
    world
        .entity_mut(key)
        .remove::<crate::token_style::TextToken>()
        .insert((
            TextColor(Color::NONE),
            bevy::a11y::AccessibilityNode(accesskit::Node::new(accesskit::Role::PasswordInput)),
        ));
    let font = world.resource::<crate::theme::Typography>().text(18.0);
    world.spawn((
        Text::new(""),
        font,
        crate::token_style::text(crate::tokens::Token::Ink),
        Mask(key),
        Pickable::IGNORE,
        Node {
            position_type: PositionType::Absolute,
            left: px(6),
            top: px(6),
            ..default()
        },
        ChildOf(key),
    ));
    let directory = field(
        world,
        form,
        "Folder for new files · full path to an existing folder",
    );
    crate::edit_mode::label(
        world,
        form,
        "Fiote can create new files here. It cannot replace existing files. Model requests use your provider account.",
        14.0,
    );
    crate::description::button(world, form, owner, "Save and enable Fiote", Save(true));
    crate::description::button(world, form, owner, "Disable Fiote", Save(false));
    world.entity_mut(owner).insert(Panel {
        binding,
        form,
        status,
        provider: FioteProvider::default(),
        provider_label,
        model,
        endpoint,
        key,
        directory,
        pending: None,
        loaded: false,
        enabled: false,
    });
    inspect(world, owner);
}

fn send(world: &World, entity: Entity, request: FioteRequest) -> Result<String, String> {
    if crate::laboratory::suspended(world, entity) {
        return Err("Workspace is suspended.".into());
    }
    let id = nucleus::new_uid("fiote");
    world
        .get_non_send::<crate::cell_bridge::CellBridge>()
        .ok_or("The local Cell is not connected.")?
        .outgoing
        .try_send(cell::ClientMessage::Fiote {
            id: id.clone(),
            request,
        })
        .map_err(|_| "The local Cell is busy or disconnected.")?;
    Ok(id)
}

fn inspect(world: &mut World, entity: Entity) {
    let panel = world.get::<Panel>(entity).unwrap();
    let request = FioteRequest::Inspect {
        record: panel.binding.uid.clone(),
    };
    let status = panel.status;
    match send(world, entity, request) {
        Ok(id) => world.get_mut::<Panel>(entity).unwrap().pending = Some(id),
        Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
    }
}

#[derive(Clone)]
struct Toggle;
impl Action for Toggle {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Panel>(owner) else {
            return;
        };
        let form = panel.form;
        let loaded = panel.loaded;
        let mut node = world.get_mut::<Node>(form).unwrap();
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
        if !loaded {
            inspect(world, owner);
        }
    }
}

#[derive(Clone)]
struct ChangeProvider;
impl Action for ChangeProvider {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut panel) = world.get_mut::<Panel>(owner) else {
            return;
        };
        if panel.pending.is_some() {
            return;
        }
        let index = FioteProvider::ALL
            .iter()
            .position(|provider| *provider == panel.provider)
            .unwrap_or(0);
        panel.provider = FioteProvider::ALL[(index + 1) % FioteProvider::ALL.len()];
        let (label, provider, endpoint, key) = (
            panel.provider_label,
            panel.provider,
            panel.endpoint,
            panel.key,
        );
        world.get_mut::<Text>(label).unwrap().0 = provider.name().into();
        for field in [endpoint, key] {
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text("");
        }
    }
}

fn value(world: &World, entity: Entity) -> Result<String, String> {
    let text = world
        .get::<EditableText>(entity)
        .ok_or("Field is closed.")?;
    if text.is_composing() || crate::record_view::pending_text(text) {
        return Err("Finish editing this field first.".into());
    }
    Ok(text.value().to_string())
}

#[derive(Clone)]
struct Save(bool);
impl Action for Save {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Panel>(owner) else {
            return;
        };
        if panel.pending.is_some() || !panel.loaded {
            return;
        }
        let (status, key) = (panel.status, panel.key);
        let result = (|| {
            let settings = FioteSettings {
                enabled: self.0,
                provider: panel.provider,
                model: value(world, panel.model)?,
                endpoint: value(world, panel.endpoint)?,
                directory: value(world, panel.directory)?.into(),
            };
            let api_key = value(world, key)?;
            send(
                world,
                owner,
                FioteRequest::Configure {
                    record: panel.binding.uid.clone(),
                    settings,
                    api_key: (!api_key.is_empty()).then_some(FioteSecret(api_key)),
                },
            )
        })();
        match result {
            Ok(id) => {
                world.get_mut::<Panel>(owner).unwrap().pending = Some(id);
                world
                    .get_mut::<EditableText>(key)
                    .unwrap()
                    .editor
                    .set_text("");
                world.get_mut::<Text>(status).unwrap().0 = "Saving…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

pub fn thread_controls(world: &mut World, parent: Entity, thread: &str, binding: &RecordBinding) {
    if binding.source != Source::Local {
        return;
    }
    let enabled = world
        .query::<&Panel>()
        .iter(world)
        .any(|panel| panel.binding.uid == binding.uid && panel.enabled);
    let owner = world
        .spawn((
            Node {
                display: if enabled {
                    Display::Flex
                } else {
                    Display::None
                },
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let status = crate::edit_mode::label(world, owner, "", 12.0);
    crate::description::button(world, owner, owner, "Stop Fiote", Stop);
    world.entity_mut(owner).insert(ThreadControl {
        record: binding.uid.clone(),
        thread: thread.into(),
        status,
        pending: None,
    });
}

#[derive(Clone)]
struct Stop;
impl Action for Stop {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(control) = world.get::<ThreadControl>(owner) else {
            return;
        };
        let (thread, status) = (control.thread.clone(), control.status);
        match send(world, owner, FioteRequest::Stop { thread }) {
            Ok(id) => world.get_mut::<ThreadControl>(owner).unwrap().pending = Some(id),
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

fn apply_status(world: &mut World, entity: Entity, status: FioteStatus) {
    let controls: Vec<_> = world
        .query::<(Entity, &ThreadControl)>()
        .iter(world)
        .filter(|(_, control)| control.record == status.record)
        .map(|(entity, _)| entity)
        .collect();
    for control in controls {
        world.get_mut::<Node>(control).unwrap().display = if status.settings.enabled {
            Display::Flex
        } else {
            Display::None
        };
    }
    let mut panel = world.get_mut::<Panel>(entity).unwrap();
    panel.pending = None;
    panel.loaded = true;
    panel.enabled = status.settings.enabled;
    panel.provider = status.settings.provider;
    let (label, provider_label) = (panel.status, panel.provider_label);
    let fields = [
        (panel.model, status.settings.model.clone()),
        (panel.endpoint, status.settings.endpoint),
        (
            panel.directory,
            status.settings.directory.to_string_lossy().into_owned(),
        ),
    ];
    world.get_mut::<Text>(provider_label).unwrap().0 = status.settings.provider.name().into();
    for (field, value) in fields {
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text(&value);
    }
    world.get_mut::<Text>(label).unwrap().0 = if status.settings.enabled {
        format!(
            "Fiote enabled · {} · {} · {}",
            status.settings.provider.name(),
            status.settings.model,
            if status.has_key {
                "key saved"
            } else {
                "no API key"
            }
        )
    } else {
        "Fiote disabled".into()
    };
}

fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() else {
        return;
    };
    let events: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    for event in events {
        let (id, result) = match event {
            cell::ServerMessage::Fiote { id, status } => (id, Ok(status)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let panels: Vec<_> = world
            .query::<(Entity, &Panel)>()
            .iter(world)
            .filter(|(_, panel)| {
                panel.pending.as_deref() == Some(&id)
                    || (id == crate::cell_bridge::CONNECTION && panel.pending.is_some())
            })
            .map(|(entity, _)| entity)
            .collect();
        for entity in panels {
            match &result {
                Ok(status) => apply_status(world, entity, status.clone()),
                Err(error) => {
                    let mut panel = world.get_mut::<Panel>(entity).unwrap();
                    panel.pending = None;
                    let label = panel.status;
                    world.get_mut::<Text>(label).unwrap().0 = error.clone();
                }
            }
        }
        let controls: Vec<_> = world
            .query::<(Entity, &ThreadControl)>()
            .iter(world)
            .filter(|(_, control)| control.pending.as_deref() == Some(&id))
            .map(|(entity, _)| entity)
            .collect();
        for entity in controls {
            let mut control = world.get_mut::<ThreadControl>(entity).unwrap();
            control.pending = None;
            let label = control.status;
            world.get_mut::<Text>(label).unwrap().0 = result
                .as_ref()
                .map(|_| "Stop requested.".into())
                .unwrap_or_else(|error| error.clone());
        }
    }
}

fn masks(fields: Query<&EditableText>, mut masks: Query<(&Mask, &mut Text)>) {
    for (mask, mut text) in &mut masks {
        let count = fields
            .get(mask.0)
            .map_or(0, |field| field.value().chars().count());
        text.set_if_neq(Text::new("•".repeat(count.min(32))));
    }
}

fn protect_keys(panels: Query<&Panel>, mut fields: Query<&mut EditableText>) {
    for panel in &panels {
        if let Ok(mut field) = fields.get_mut(panel.key) {
            field.pending_edits.retain(|edit| {
                !matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn deliver(app: &mut App) {
        let bridge = app
            .world_mut()
            .non_send_mut::<crate::cell_bridge::CellBridge>()
            .into_inner();
        let message =
            tokio::time::timeout(std::time::Duration::from_secs(10), bridge.incoming.recv())
                .await
                .unwrap()
                .unwrap();
        app.world_mut()
            .write_message(crate::cell_bridge::CellMessage(message));
        app.update();
    }

    #[tokio::test]
    async fn settings_form_configures_the_host_and_clears_the_key() {
        let directory = tempfile::tempdir().unwrap();
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let record = engine
            .act(
                engine::actions::Action::CreateAgent {
                    head: "Fiote".into(),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let host = std::sync::Arc::new(
            cell::fiote::Host::open(engine.clone(), directory.path().join("settings"))
                .await
                .unwrap(),
        );
        let runtime = cell::CellRuntime {
            store: engine.store.clone(),
            engine,
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
            fiote: Some(host),
        };
        let bridge = crate::cell_bridge::connect(runtime, crate::wake::WakeSignal::new(|| {}));
        let mut app = App::new();
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_plugins(Plugin);
        app.world_mut().insert_non_send(bridge);
        let root = app.world_mut().spawn_empty().id();
        populate(
            app.world_mut(),
            root,
            RecordBinding {
                area: root,
                uid: record,
                source: Source::Local,
            },
        );
        deliver(&mut app).await;
        let owner = app
            .world_mut()
            .query_filtered::<Entity, With<Panel>>()
            .single(app.world())
            .unwrap();
        let panel = app.world().get::<Panel>(owner).unwrap();
        assert!(panel.loaded);
        let (model, directory_field, key, status) =
            (panel.model, panel.directory, panel.key, panel.status);
        app.world_mut()
            .get_mut::<EditableText>(model)
            .unwrap()
            .editor
            .set_text("test-model");
        app.world_mut()
            .get_mut::<EditableText>(directory_field)
            .unwrap()
            .editor
            .set_text(directory.path().to_str().unwrap());
        app.world_mut()
            .get_mut::<EditableText>(key)
            .unwrap()
            .editor
            .set_text("test-secret-key");
        Save(true).apply(app.world_mut(), owner);
        assert_eq!(
            app.world()
                .get::<EditableText>(key)
                .unwrap()
                .value()
                .to_string(),
            ""
        );
        deliver(&mut app).await;
        let label = &app.world().get::<Text>(status).unwrap().0;
        assert!(
            label.contains("Fiote enabled") && label.contains("key saved"),
            "{label}"
        );
        assert!(
            !app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0.contains("test-secret-key"))
        );
        Save(false).apply(app.world_mut(), owner);
        deliver(&mut app).await;
        assert_eq!(app.world().get::<Text>(status).unwrap().0, "Fiote disabled");
    }
}
