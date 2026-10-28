use crate::native::{actions::Action, protein_area::Source, sand_panel as panel};
use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};
use engine::sources::Command;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[path = "source_view_persistence.rs"]
mod persistence;
pub(crate) use persistence::{SavedSourceView, snapshot};

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct SourceView {
    pub uri: String,
    pub selected: Vec<String>,
    pub bindings: BTreeMap<String, String>,
    pub current: Option<String>,
    #[serde(default)]
    pub binding: Option<String>,
}

impl SourceView {
    pub fn valid(&self) -> bool {
        self.uri.len() <= 8192
            && !self.uri.contains('\0')
            && self.selected.len() <= 128
            && self.bindings.len() <= 128
            && self.selected.iter().all(|value| value.len() <= 8192)
            && self
                .bindings
                .iter()
                .all(|(name, value)| name.len() <= 256 && value.len() <= 256)
            && self
                .current
                .as_ref()
                .is_none_or(|value| value.len() <= 256 && !value.contains('\0'))
            && self
                .binding
                .as_ref()
                .is_none_or(|value| value.len() <= 256 && !value.contains('\0'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_preserve_selection_and_only_offer_imported_rule_bindings() {
        let mut castle = SourceView::default();
        review_state(
            &mut castle,
            &serde_json::json!({"root":"r_root","binding":"r_binding","binding_ready":false,"records":[{"uid":"r_root","state":"create"}]}),
        );
        assert!(castle.current.is_none());
        assert!(castle.binding.is_none());
        review_state(
            &mut castle,
            &serde_json::json!({"root":"r_root","binding":"r_binding","created":4}),
        );
        assert_eq!(castle.current.as_deref(), Some("r_root"));
        castle.current = Some("r_plato".into());
        review_state(
            &mut castle,
            &serde_json::json!({"root":"r_root","binding":"r_binding","binding_ready":true,"records":[{"uid":"r_root","state":"reuse"}]}),
        );
        assert_eq!(castle.current.as_deref(), Some("r_plato"));
        assert_eq!(castle.binding.as_deref(), Some("r_binding"));
        review_state(
            &mut castle,
            &serde_json::json!({"root":"r_root","binding":"r_binding","binding_ready":false,"records":[]}),
        );
        assert!(castle.binding.is_none());
    }

    #[test]
    fn record_card_links_open_a_source_view_and_browsing_clears_old_cell_selection() {
        let mut app = crate::native::sand_panel::tests::app();
        let world = app.world_mut();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let card = world
            .spawn((
                Node::default(),
                ChildOf(root),
                Card {
                    reference: "r_existing".into(),
                    source: Source::Organ("example-organ".into()),
                    requested: true,
                },
            ))
            .id();
        Operation::Browse(
            lince_resources::Reference::new("https://example.org/Plato.lingua"),
            None,
        )
        .apply(world, card);
        let (browser, castle) = world
            .query::<(Entity, &SourceView)>()
            .single(world)
            .unwrap();
        assert_eq!(castle.uri, "https://example.org/Plato.lingua");
        assert_eq!(
            reference_source(world, browser),
            Source::Organ("example-organ".into())
        );
        assert!(world.get::<Card>(card).is_some());
        assert!(world.get::<SourceView>(card).is_none());
        let castle = &mut world.get_mut::<SourceView>(browser).unwrap();
        castle.current = Some("r_previous".into());
        castle.binding = Some("r_previous_binding".into());
        review_state(castle, &serde_json::json!({"browsing":true,"records":[]}));
        assert!(castle.current.is_none());
        assert!(castle.binding.is_none());
    }

    #[test]
    fn proposed_consequences_show_the_actual_action_and_assertion_target() {
        let removal = serde_json::json!({"kind":"run-action","action":serde_json::json!({"action":"delete-record","target":"r_evidence"}).to_string()});
        let label = consequence_label(&removal);
        assert!(label.contains("delete-record"));
        assert!(label.contains("r_evidence"));
        let assertion = serde_json::json!({"kind":"run-action","action":serde_json::json!({"action":"assert-record","subject":"person","predicate":"mastery","object":"particular-level","quantity":"100","unit":"percent"}).to_string()});
        let label = consequence_label(&assertion);
        assert!(label.contains("particular-level"));
        assert!(label.contains("100 percent"));
        assert_eq!(
            consequence_label(&serde_json::json!({"kind":"set-quantity","value":0})),
            "Set target quantity to 0"
        );
    }

    #[test]
    fn changing_or_clearing_a_bound_input_uses_the_current_editor_value() {
        let mut app = crate::native::sand_panel::tests::app();
        let world = app.world_mut();
        let root = world.spawn(Node::default()).id();
        let owner = spawn(
            world,
            root,
            1,
            DVec2::ZERO,
            SourceView {
                uri: "https://example.invalid/Root.lingua".into(),
                bindings: BTreeMap::from([("learner".into(), "r_first".into())]),
                ..Default::default()
            },
        );
        render(
            world,
            owner,
            serde_json::json!({"parameters":{"learner":{"label":"Person"}},"records":[],"rules":[]}),
        );
        Operation::Bind("learner".into(), "r_second".into()).apply(world, owner);
        let field = world.get::<View>(owner).unwrap().parameters["learner"];
        assert_eq!(panel::value(world, field).unwrap(), "r_second");
        assert_eq!(
            world.get::<SourceView>(owner).unwrap().bindings["learner"],
            "r_second"
        );
        world
            .get_mut::<bevy::text::EditableText>(field)
            .unwrap()
            .editor
            .set_text("");
        capture(world, owner);
        assert!(
            !world
                .get::<SourceView>(owner)
                .unwrap()
                .bindings
                .contains_key("learner")
        );
        assert!(world.get::<View>(owner).unwrap().preview.is_none());
    }

    #[test]
    fn exchange_panel_exposes_import_controls_while_source_reading_does_not() {
        let mut app = crate::native::sand_panel::tests::app();
        let world = app.world_mut();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let owner = exchange(world, root, "https://example.invalid/Philosophy.lingua");
        render(
            world,
            owner,
            serde_json::json!({"ready":true,"records":[],"rules":[]}),
        );
        assert!(
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == "Add to Cell")
        );
        assert!(world.get::<Exchange>(owner).is_some());
        assert!(snapshot(world, root).is_empty());
        world.despawn(owner);
        let owner = spawn(world, root, 1, DVec2::ZERO, SourceView::default());
        render(
            world,
            owner,
            serde_json::json!({"browsing":true,"ready":false,"records":[],"rules":[]}),
        );
        let labels = world
            .query::<&Text>()
            .iter(world)
            .map(|text| text.0.clone())
            .collect::<Vec<_>>();
        assert!(labels.iter().any(|text| text == "Import through Sync"));
        assert!(!labels.iter().any(|text| text == "Add to Cell"));
        assert!(!labels.iter().any(|text| text == "Preview import"));
    }

    #[test]
    fn remote_representations_cannot_open_local_files() {
        let mut app = crate::native::sand_panel::tests::app();
        let world = app.world_mut();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let owner = world
            .spawn((
                Node::default(),
                ChildOf(root),
                Card {
                    reference: "r_remote".into(),
                    source: Source::Organ("https://example.invalid".into()),
                    requested: true,
                },
            ))
            .id();
        Operation::Resource(lince_resources::Resource {
            uri: "file:///private/book.pdf".into(),
            media_type: "application/pdf".into(),
            ..Default::default()
        })
        .apply(world, owner);
        assert_eq!(
            world
                .query::<&crate::native::document_viewer::DocumentViewer>()
                .iter(world)
                .count(),
            0
        );
        assert!(
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0.contains("downloadable source address"))
        );
    }

    #[test]
    fn native_source_preview_and_workspace_state_use_generic_data() {
        let mut app = crate::native::sand_panel::tests::app();
        let world = app.world_mut();
        let root = world.spawn(Node::default()).id();
        let castle = SourceView {
            uri: "https://example.invalid/Philosophy.lingua".into(),
            selected: vec!["Checklist.lingua".into()],
            bindings: BTreeMap::from([("person".into(), "r_person".into())]),
            ..Default::default()
        };
        let owner = spawn(world, root, 7, DVec2::new(12.0, 34.0), castle.clone());
        render(
            world,
            owner,
            serde_json::json!({
                "ready":true,"records":[{"uid":"r_source","title":"Philosophy","state":"create","body":"","assertions":[],"resource":{"uri":"https://example.invalid/book.epub","media_type":"application/epub+zip"}}],
                "rules":[{"title":"Authored measure","condition":"@practice","consequences":"[{\"kind\":\"set-quantity\",\"value\":\"7\"}]"}]
            }),
        );
        let labels = world
            .query::<&Text>()
            .iter(world)
            .map(|text| text.0.clone())
            .collect::<Vec<_>>();
        assert!(!labels.iter().any(|text| text == "Add to Cell"));
        assert!(labels.iter().any(|text| text.contains("Authored measure")));
        assert!(labels.iter().any(|text| text == "Set target quantity to 7"));
        assert!(!labels.iter().any(|text| text == "Loading source preview…"));
        let saved = snapshot(world, root);
        assert_eq!(saved.len(), 1);
        let encoded = serde_json::to_value(&saved[0]).unwrap();
        assert_eq!(encoded["workspace"], 7);
        assert_eq!(encoded["castle"]["selected"][0], "Checklist.lingua");
        world.despawn(owner);
        let restored: SavedSourceView = serde_json::from_value(encoded).unwrap();
        assert!(restored.valid());
        restored.restore(world, root);
        let restored = world.query::<&SourceView>().single(world).unwrap();
        assert_eq!(restored.uri, castle.uri);
        assert_eq!(restored.bindings, castle.bindings);
        assert!(
            !SourceView {
                current: Some("x".repeat(257)),
                ..Default::default()
            }
            .valid()
        );
    }
}

#[derive(Component)]
struct View {
    input: Entity,
    destination: Entity,
    status: Entity,
    body: Entity,
    preview: Option<Value>,
    parameters: BTreeMap<String, Entity>,
}

#[derive(Component)]
struct Card {
    reference: String,
    source: Source,
    requested: bool,
}

#[derive(Clone)]
enum Operation {
    Preview,
    Exchange,
    Read,
    Extract,
    Audit,
    Browse(lince_resources::Reference, Option<String>),
    Import(bool),
    Select(String),
    Bind(String, String),
    Request(Command),
    Open(String, Source),
    Resource(lince_resources::Resource),
    Quantity(String, String),
    Export(bool),
    Retain(String, Entity, Entity, Option<usize>),
    LocalForm(String),
    Register(String, Entity),
    Focus(String),
    Execute(Box<engine::actions::Action>),
}

#[derive(Resource, Default)]
struct Pending(BTreeMap<String, (Entity, Source, bool)>);

pub struct SourceViewPlugin;

impl Plugin for SourceViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pending>().add_systems(
            Update,
            (
                update.after(crate::native::cell_bridge::ReceiveCell),
                crate::native::resource_preview::update,
            ),
        );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    castle: SourceView,
) -> Entity {
    let owner = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(crate::native::document_viewer::CREDITS),
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::new(680.0, 720.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            castle,
        ))
        .id();
    let frame = panel::frame(world, owner, "Source");
    populate(world, owner, frame, false);
    owner
}

#[derive(Component)]
struct Exchange;

pub(crate) fn exchange(world: &mut World, parent: Entity, uri: &str) -> Entity {
    let owner = panel::column(world, parent);
    world.entity_mut(owner).insert((
        SourceView {
            uri: uri.into(),
            ..Default::default()
        },
        Exchange,
        crate::native::sand_store::SandCredits(crate::native::document_viewer::CREDITS),
    ));
    crate::native::edit_mode::label(world, owner, "Sources", 18.0);
    populate(world, owner, owner, true);
    owner
}

pub(crate) fn set_source(world: &mut World, owner: Entity, uri: &str) {
    let field = world.get::<View>(owner).unwrap().input;
    if let Some(mut text) = world.get_mut::<bevy::text::EditableText>(field) {
        text.editor.set_text(uri);
    }
    Operation::Read.apply(world, owner);
}

fn populate(world: &mut World, owner: Entity, frame: Entity, exchange: bool) {
    let uri = world.get::<SourceView>(owner).unwrap().uri.clone();
    let input = panel::field(world, frame, "Source file, folder or URI", &uri);
    let row = panel::row(world, frame);
    panel::button(world, row, owner, "Open source", Operation::Read);
    if exchange {
        panel::button(world, row, owner, "Audit folder links", Operation::Audit);
        panel::button(world, row, owner, "Preview import", Operation::Preview);
    }
    let body = panel::column(world, frame);
    if !exchange {
        let mut node = world.get_mut::<Node>(body).unwrap();
        node.overflow = Overflow::scroll_y();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.flex_basis = px(0);
        crate::native::scroll_sand::attach(world, body);
    }
    let destination = if exchange {
        let destination = panel::field(world, frame, "Export or ZIP extraction folder", "");
        let row = panel::row(world, frame);
        panel::button(world, row, owner, "Extract ZIP", Operation::Extract);
        panel::button(
            world,
            row,
            owner,
            "Export source references",
            Operation::Export(false),
        );
        panel::button(
            world,
            row,
            owner,
            "Export source with media",
            Operation::Export(true),
        );
        destination
    } else {
        Entity::PLACEHOLDER
    };
    let status = crate::native::edit_mode::label(
        world,
        frame,
        "Open a source or follow a Record reference.",
        13.0,
    );
    world.entity_mut(owner).insert(View {
        input,
        destination,
        status,
        body,
        preview: None,
        parameters: BTreeMap::new(),
    });
}

fn status(world: &mut World, owner: Entity, text: impl Into<String>) {
    if let Some(view) = world.get::<View>(owner) {
        let target = view.status;
        world.get_mut::<Text>(target).unwrap().0 = text.into();
    } else if world.entities().contains(owner) {
        crate::native::edit_mode::label(world, owner, &text.into(), 12.0);
    }
}

fn root(world: &World, mut entity: Entity) -> Option<Entity> {
    loop {
        if world
            .get::<crate::native::workspace::Workspaces>(entity)
            .is_some()
        {
            return Some(entity);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

fn request(world: &mut World, owner: Entity, source: Source, command: Command, card: bool) {
    world.init_resource::<Pending>();
    let Some(sender) = crate::native::description::source_sender(world, &source) else {
        status(
            world,
            owner,
            "Connect to the Cell before opening this source.",
        );
        return;
    };
    let id = nucleus::new_uid("source");
    let message = ClientMessage::Act {
        id: id.clone(),
        action: engine::actions::Action::Sources { request: command },
    };
    match sender.try_send(message) {
        Ok(()) => {
            let mut pending = world.resource_mut::<Pending>();
            if !card {
                pending.0.retain(|_, (previous, _, _)| *previous != owner);
            }
            pending.0.insert(id, (owner, source, card));
            status(world, owner, "Working…");
        }
        Err(error) => status(world, owner, error.to_string()),
    }
}

fn capture(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let uri = panel::value(world, view.input)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let bindings = view
        .parameters
        .iter()
        .map(|(name, input)| {
            (
                name.clone(),
                panel::value(world, *input).unwrap_or_default(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let source_changed = world.get::<SourceView>(owner).unwrap().uri != uri;
    let mut inputs_changed = false;
    {
        let mut castle = world.get_mut::<SourceView>(owner).unwrap();
        castle.uri = uri;
        if source_changed {
            castle.selected.clear();
            castle.bindings.clear();
            castle.current = None;
            castle.binding = None;
        } else {
            for (name, value) in bindings {
                let value = value.trim();
                if value.is_empty() {
                    inputs_changed |= castle.bindings.remove(&name).is_some();
                } else if castle
                    .bindings
                    .get(&name)
                    .is_none_or(|current| current != value)
                {
                    castle.bindings.insert(name, value.into());
                    inputs_changed = true;
                }
            }
            if inputs_changed {
                castle.current = None;
                castle.binding = None;
            }
        }
    }
    if source_changed || inputs_changed {
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.preview = None;
        if source_changed {
            let body = view.body;
            view.parameters.clear();
            panel::clear(world, body);
        }
    }
}

impl Action for Operation {
    fn apply(&self, world: &mut World, owner: Entity) {
        match self {
            Self::Exchange => {
                let uri = world
                    .get::<SourceView>(owner)
                    .map(|view| view.uri.clone())
                    .unwrap_or_default();
                if let Some(root) = root(world, owner) {
                    crate::native::sync_castle::open_source(world, root, &uri);
                }
            }
            Self::Browse(reference, record) => {
                let source = reference_source(world, owner);
                let destination = if world.get::<SourceView>(owner).is_some() {
                    let mut castle = world.get_mut::<SourceView>(owner).unwrap();
                    castle.current = None;
                    castle.binding = None;
                    castle.selected.clear();
                    castle.bindings.clear();
                    owner
                } else {
                    let Some(root) = root(world, owner) else {
                        return;
                    };
                    let workspace = world
                        .get::<crate::native::workspace::Workspaces>(root)
                        .map_or(1, |spaces| spaces.active);
                    let position = world
                        .get::<crate::native::canvas::CanvasView>(root)
                        .map_or(DVec2::ZERO, |view| view.center);
                    let destination = spawn(
                        world,
                        root,
                        workspace,
                        position,
                        SourceView {
                            uri: reference.target.clone(),
                            ..Default::default()
                        },
                    );
                    world
                        .entity_mut(destination)
                        .insert(ReferenceSource(source.clone()));
                    destination
                };
                request(
                    world,
                    destination,
                    source,
                    Command::Open {
                        reference: reference.clone(),
                        record: record.clone(),
                    },
                    false,
                );
            }
            Self::Audit
            | Self::Read
            | Self::Extract
            | Self::Preview
            | Self::Import(_)
            | Self::Export(_) => {
                capture(world, owner);
                let castle = world.get::<SourceView>(owner).unwrap().clone();
                let command = match self {
                    Self::Audit => Command::Audit {
                        uri: castle.uri,
                        inverses: Vec::new(),
                    },
                    Self::Read => Command::Open {
                        reference: lince_resources::Reference::new(castle.uri),
                        record: None,
                    },
                    Self::Extract => Command::Extract {
                        uri: castle.uri,
                        destination: panel::value(
                            world,
                            world.get::<View>(owner).unwrap().destination,
                        )
                        .unwrap_or_default(),
                    },
                    Self::Preview => Command::Preview {
                        uri: castle.uri,
                        selected: castle.selected,
                        bindings: castle.bindings,
                        refresh: true,
                    },
                    Self::Import(update) => {
                        let Some(revision) = world
                            .get::<View>(owner)
                            .and_then(|view| view.preview.as_ref())
                            .and_then(|value| value["revision"].as_str())
                        else {
                            status(
                                world,
                                owner,
                                "Preview the source before adding it to the Cell.",
                            );
                            return;
                        };
                        Command::Import {
                            uri: castle.uri,
                            selected: castle.selected,
                            bindings: castle.bindings,
                            revision: revision.into(),
                            update: *update,
                        }
                    }
                    Self::Export(media) => Command::Export {
                        uri: castle.uri,
                        selected: castle.selected,
                        destination: panel::value(
                            world,
                            world.get::<View>(owner).unwrap().destination,
                        )
                        .unwrap_or_default(),
                        media: *media,
                    },
                    _ => unreachable!(),
                };
                let source = if matches!(self, Self::Read) {
                    reference_source(world, owner)
                } else {
                    Source::Local
                };
                request(world, owner, source, command, false);
            }
            Self::Select(uri) => {
                capture(world, owner);
                let mut castle = world.get_mut::<SourceView>(owner).unwrap();
                castle.current = None;
                castle.binding = None;
                if castle.selected.contains(uri) {
                    castle.selected.retain(|value| value != uri);
                } else {
                    castle.selected.push(uri.clone());
                }
                Self::Preview.apply(world, owner);
            }
            Self::Bind(name, uid) => {
                if let Some(field) = world
                    .get::<View>(owner)
                    .and_then(|view| view.parameters.get(name))
                    .copied()
                    && let Some(mut text) = world.get_mut::<bevy::text::EditableText>(field)
                {
                    text.editor.set_text(uid);
                }
                world
                    .get_mut::<SourceView>(owner)
                    .unwrap()
                    .bindings
                    .insert(name.clone(), uid.clone());
                let mut castle = world.get_mut::<SourceView>(owner).unwrap();
                castle.current = None;
                castle.binding = None;
                world.get_mut::<View>(owner).unwrap().preview = None;
                Self::Preview.apply(world, owner);
            }
            Self::Request(command) => {
                if let Command::Activate { record } = command
                    && world
                        .get::<SourceView>(owner)
                        .is_some_and(|castle| castle.binding.as_deref() != Some(record.as_str()))
                {
                    status(
                        world,
                        owner,
                        "Add the reviewed source before enabling its Karma.",
                    );
                    return;
                }
                let source = world
                    .get::<Card>(owner)
                    .map_or(Source::Local, |card| card.source.clone());
                request(
                    world,
                    owner,
                    source,
                    command.clone(),
                    world.get::<Card>(owner).is_some(),
                );
            }
            Self::Open(uid, source) => {
                if let Some(root) = root(world, owner) {
                    crate::native::full_record::open(world, root, uid, source.clone());
                }
            }
            Self::Resource(resource) => {
                if !matches!(reference_source(world, owner), Source::Local)
                    && lince_resources::address(&resource.uri)
                        .is_ok_and(|uri| uri.scheme() == "file")
                {
                    status(
                        world,
                        owner,
                        "This remote reference needs a downloadable source address.",
                    );
                    return;
                }
                let Some(root) = root(world, owner) else {
                    return;
                };
                let workspace = world
                    .get::<crate::native::workspace::Workspaces>(root)
                    .unwrap()
                    .active;
                let position = world
                    .get::<crate::native::canvas::CanvasView>(root)
                    .map_or(DVec2::ZERO, |view| view.center);
                if resource.media_type == "text/x-lingua" {
                    spawn(
                        world,
                        root,
                        workspace,
                        position,
                        SourceView {
                            uri: resource.uri.clone(),
                            ..default()
                        },
                    );
                } else if resource.media_type.starts_with("video/") {
                    let sand = crate::native::sand_store::spawn_sand(
                        world,
                        root,
                        workspace,
                        crate::native::sand_store::SandKind::Video,
                        "",
                        position,
                    );
                    crate::native::video_castle::restore(
                        world,
                        sand,
                        crate::native::video_castle::Settings {
                            source: resource.uri.clone(),
                            file: true,
                            position_ms: resource.locator.start_ms.unwrap_or(0),
                            resource: Some(resource.clone()),
                            ..default()
                        },
                    );
                } else if resource.media_type.starts_with("image/") {
                    crate::native::media_sand::spawn(
                        world,
                        root,
                        workspace,
                        position,
                        crate::native::media_sand::MediaSand::ImageSource {
                            resource: resource.clone(),
                        },
                    );
                } else {
                    crate::native::document_viewer::spawn(
                        world,
                        root,
                        workspace,
                        position,
                        crate::native::document_viewer::DocumentViewer::with_resource(
                            resource.clone(),
                        ),
                    );
                }
            }
            Self::Quantity(uid, amount) => {
                Self::Execute(Box::new(engine::actions::Action::SetQuantityExact {
                    target: uid.clone(),
                    amount: amount.clone(),
                }))
                .apply(world, owner);
            }
            Self::Execute(action) => {
                let source = world
                    .get::<Card>(owner)
                    .map_or(Source::Local, |card| card.source.clone());
                if let Some(sender) = crate::native::description::source_sender(world, &source) {
                    let id = nucleus::new_uid("source-control");
                    if sender
                        .try_send(ClientMessage::Act {
                            id: id.clone(),
                            action: *action.clone(),
                        })
                        .is_ok()
                    {
                        world
                            .resource_mut::<Pending>()
                            .0
                            .insert(id, (owner, source, true));
                    }
                }
            }
            Self::Retain(record, start, end, representation) => {
                let seconds = |entity| {
                    panel::value(world, entity)
                        .ok()
                        .and_then(|value| value.parse::<f64>().ok())
                        .filter(|value| value.is_finite() && *value >= 0.0 && *value < 1e12)
                        .map(|value| (value * 1000.0) as u64)
                };
                if let (Some(start_ms), Some(end_ms)) = (seconds(*start), seconds(*end)) {
                    let source = world
                        .get::<Card>(owner)
                        .map_or(Source::Local, |card| card.source.clone());
                    request(
                        world,
                        owner,
                        source,
                        Command::RetainInterval {
                            record: record.clone(),
                            start_ms,
                            end_ms,
                            representation: *representation,
                        },
                        true,
                    );
                } else {
                    status(
                        world,
                        owner,
                        "Enter the interval's start and end in seconds.",
                    );
                }
            }
            Self::LocalForm(record) => {
                let field = panel::field(world, owner, "Existing local file", "");
                panel::button(
                    world,
                    owner,
                    owner,
                    "Register this file",
                    Self::Register(record.clone(), field),
                );
            }
            Self::Register(record, field) => {
                if let Ok(path) = panel::value(world, *field) {
                    Self::Request(Command::Register {
                        record: record.clone(),
                        path,
                    })
                    .apply(world, owner);
                }
            }
            Self::Focus(uid) => {
                world.get_mut::<SourceView>(owner).unwrap().current = Some(uid.clone());
                let preview = world
                    .get::<View>(owner)
                    .and_then(|view| view.preview.clone());
                if let Some(preview) = preview {
                    render(world, owner, preview);
                }
            }
        }
    }
}

fn render(world: &mut World, owner: Entity, data: Value) {
    let exchange = world.get::<Exchange>(owner).is_some();
    let body = world.get::<View>(owner).unwrap().body;
    panel::clear(world, body);
    let castle = world.get::<SourceView>(owner).unwrap().clone();
    if data["audit"].is_object() {
        crate::native::edit_mode::label(
            world,
            body,
            &format!(
                "Checked {} documents and {} links",
                data["audit"]["documents"], data["audit"]["links"]
            ),
            14.0,
        );
        for issue in data["audit"]["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(data["audit"]["unchecked"].as_array().into_iter().flatten())
        {
            if let Some(issue) = issue.as_str() {
                crate::native::edit_mode::label(world, body, issue, 12.0);
            }
        }
    }
    world.get_mut::<View>(owner).unwrap().parameters.clear();
    if let Some(current) = &castle.current {
        crate::native::edit_mode::label(world, body, "Selected Record", 16.0);
        attach(world, body, current, Source::Local);
    }
    for file in data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let row = panel::row(world, body);
        let link = panel::button(
            world,
            row,
            owner,
            &link_label(file),
            Operation::Browse(lince_resources::Reference::new(file), None),
        );
        world
            .entity_mut(link)
            .insert(crate::native::icons::Tooltip(file.into()));
        panel::button(
            world,
            row,
            owner,
            if castle.selected.iter().any(|selected| selected == file) {
                "Selected"
            } else {
                "Select"
            },
            Operation::Select(file.into()),
        );
    }
    if let Some(parameters) = data["parameters"].as_object() {
        for (name, input) in parameters {
            let value = castle
                .bindings
                .get(name)
                .cloned()
                .or_else(|| data["bindings"][name].as_str().map(str::to_owned))
                .unwrap_or_default();
            let field = panel::field(world, body, input["label"].as_str().unwrap_or(name), &value);
            world
                .get_mut::<View>(owner)
                .unwrap()
                .parameters
                .insert(name.clone(), field);
            for person in data["actors"].as_array().into_iter().flatten() {
                if let Some(uid) = person["uid"].as_str() {
                    panel::button(
                        world,
                        body,
                        owner,
                        person["title"].as_str().unwrap_or(uid),
                        Operation::Bind(name.clone(), uid.into()),
                    );
                }
            }
        }
    }
    for option in data["options"].as_array().into_iter().flatten() {
        if option["required"] == true {
            continue;
        }
        let Some(uri) = option["uri"].as_str() else {
            continue;
        };
        let label = format!(
            "{} {}",
            if castle.selected.iter().any(|value| value == uri) {
                "☑"
            } else {
                "☐"
            },
            option["label"].as_str().unwrap_or(uri)
        );
        panel::button(world, body, owner, &label, Operation::Select(uri.into()));
    }
    let titles = data["records"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|record| Some((record["uid"].as_str()?, record["title"].as_str()?)))
        .collect::<BTreeMap<_, _>>();
    for record in data["records"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|record| {
            data["browsing"] != true
                || data["fragment"]
                    .as_str()
                    .is_none_or(|fragment| record["uid"] == fragment)
        })
    {
        if record["reference_review"] == true {
            crate::native::edit_mode::label(
                world,
                body,
                "The source or location changed. Review this reference before accepting the update.",
                13.0,
            );
        }
        let title = record["title"].as_str().unwrap_or("Record");
        crate::native::edit_mode::label(
            world,
            body,
            &format!(
                "{title} · {}{}",
                match record["state"].as_str() {
                    Some("create") => "New Record",
                    Some("reuse") => "Already in Cell",
                    Some("update_available") => "Updated source",
                    Some("source") =>
                        if record["saved"] == true {
                            "Saved source snapshot"
                        } else if record["cached"] == true {
                            "HTTP cache"
                        } else {
                            "Source document"
                        },
                    _ => "",
                },
                if record["personal"] == true {
                    " · personal"
                } else {
                    ""
                }
            ),
            15.0,
        );
        let content = panel::column(world, body);
        if let Some(base) = record["base"].as_str() {
            world.entity_mut(content).insert(ReferenceBase(base.into()));
        }
        if let Some(text) = record["body"].as_str().filter(|body| !body.is_empty()) {
            let source = reference_source(world, owner);
            crate::native::description::readonly(
                world,
                content,
                text,
                crate::native::description::Context {
                    owner: content,
                    source,
                },
            );
        }
        if let Some(assertions) = record["assertions"].as_array() {
            for assertion in assertions {
                let predicate = assertion["predicate"].as_str().unwrap_or("assertion");
                let object = assertion["object"]
                    .as_str()
                    .and_then(|uid| titles.get(uid))
                    .copied();
                let label = object.map_or_else(
                    || format!("#{predicate}"),
                    |object| format!("#{predicate} → {object}"),
                );
                if let Some(uri) = assertion["uri"].as_str() {
                    let link = panel::button(
                        world,
                        content,
                        owner,
                        &format!("#{predicate} → {}", link_label(uri)),
                        Operation::Browse(lince_resources::Reference::new(uri), None),
                    );
                    world
                        .entity_mut(link)
                        .insert(crate::native::icons::Tooltip(uri.into()));
                } else {
                    crate::native::edit_mode::label(world, content, &label, 12.0);
                }
            }
        }
        if let Some(uid) = record["uid"]
            .as_str()
            .filter(|_| record["state"] != "create" && record["state"] != "source")
        {
            panel::button(
                world,
                body,
                owner,
                "Open Record",
                Operation::Open(uid.into(), Source::Local),
            );
            panel::button(
                world,
                body,
                owner,
                "Select Record",
                Operation::Focus(uid.into()),
            );
        }
        if let Ok(resource) =
            serde_json::from_value::<lince_resources::Resource>(record["resource"].clone())
        {
            media(world, body, owner, &resource);
        }
    }
    for concept in data["vocabulary"].as_array().into_iter().flatten() {
        if let Some(definition) = concept["definition"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            crate::native::edit_mode::label(
                world,
                body,
                &format!(
                    "#{}: {definition}",
                    concept["name"].as_str().unwrap_or("Meaning")
                ),
                13.0,
            );
        }
    }
    for rule in data["rules"].as_array().into_iter().flatten() {
        crate::native::edit_mode::label(
            world,
            body,
            &format!(
                "Proposed Karma: {} · {} · gate {} · carry {}",
                rule["title"].as_str().unwrap_or("Rule"),
                rule["condition"].as_str().unwrap_or("Scheduled condition"),
                rule["gate"].as_str().unwrap_or("always"),
                rule["carry"].as_str().unwrap_or("value")
            ),
            12.0,
        );
        if let Some(cadence) = rule["cadence"].as_str() {
            crate::native::edit_mode::label(world, body, &format!("Cadence: {cadence}"), 12.0);
        }
        if let Some(consequences) = rule["consequences"]
            .as_str()
            .and_then(|value| serde_json::from_str::<Vec<Value>>(value).ok())
        {
            for consequence in consequences {
                crate::native::edit_mode::label(
                    world,
                    body,
                    &consequence_label(&consequence),
                    12.0,
                );
            }
        }
    }
    for conflict in data["conflicts"].as_array().into_iter().flatten() {
        crate::native::edit_mode::label(world, body, &conflict.to_string(), 13.0);
    }
    if data["browsing"] == true {
        panel::button(
            world,
            body,
            owner,
            if exchange {
                "Preview import"
            } else {
                "Import through Sync"
            },
            if exchange {
                Operation::Preview
            } else {
                Operation::Exchange
            },
        );
    }
    if exchange && data["ready"] != false {
        let row = panel::row(world, body);
        panel::button(world, row, owner, "Add to Cell", Operation::Import(false));
        panel::button(
            world,
            row,
            owner,
            "Accept source updates",
            Operation::Import(true),
        );
    }
    if let Some(binding) = castle.binding.filter(|_| {
        data["rules"]
            .as_array()
            .is_some_and(|rules| !rules.is_empty())
    }) {
        panel::button(
            world,
            body,
            owner,
            "Enable reviewed Karma",
            Operation::Request(Command::Activate { record: binding }),
        );
    }
    world.get_mut::<View>(owner).unwrap().preview = Some(data);
}

fn consequence_label(consequence: &Value) -> String {
    if consequence["kind"] == "run-action" {
        if let Some(action) = consequence["action"]
            .as_str()
            .and_then(|action| serde_json::from_str::<Value>(action).ok())
        {
            if matches!(
                action["action"].as_str(),
                Some("assert-record" | "retract-record")
            ) {
                return format!(
                    "{} {}: #{} {} {}{}",
                    if action["action"] == "assert-record" {
                        "Assert"
                    } else {
                        "Remove"
                    },
                    action["subject"].as_str().unwrap_or("subject"),
                    action["predicate"].as_str().unwrap_or("predicate"),
                    action["quantity"].as_str().unwrap_or(""),
                    action["unit"].as_str().unwrap_or(""),
                    action["object"]
                        .as_str()
                        .map_or(String::new(), |object| format!(" → {object}"))
                );
            }
            if action["action"] == "set-quantity-exact" {
                return format!(
                    "Set {} quantity to {}",
                    action["target"].as_str().unwrap_or("target Record"),
                    action["amount"].as_str().unwrap_or("declared value")
                );
            }
            return serde_json::to_string_pretty(&action).unwrap_or_else(|_| action.to_string());
        }
    }
    if consequence["kind"] == "set-quantity" {
        return format!(
            "Set target quantity to {}",
            if consequence["value"].is_null() {
                "the condition's value".into()
            } else {
                consequence["value"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| consequence["value"].to_string())
            }
        );
    }
    serde_json::to_string_pretty(consequence).unwrap_or_else(|_| consequence.to_string())
}

fn link_label(uri: &str) -> String {
    let name = lince_resources::address(uri).ok().and_then(|url| {
        url.to_file_path()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .or_else(|| {
                url.path_segments()
                    .and_then(|mut parts| parts.next_back())
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
            })
    });
    name.unwrap_or_else(|| uri.into())
        .trim_end_matches(".lingua")
        .replace(['-', '_'], " ")
}

pub(crate) fn attach(world: &mut World, parent: Entity, reference: &str, source: Source) {
    let entity = panel::column(world, parent);
    world.entity_mut(entity).insert((
        Card {
            reference: reference.into(),
            source,
            requested: false,
        },
        crate::native::sand_store::SandCredits(crate::native::document_viewer::CREDITS),
    ));
}

fn media(world: &mut World, parent: Entity, owner: Entity, resource: &lince_resources::Resource) {
    if world
        .get::<Card>(owner)
        .is_some_and(|card| !matches!(card.source, Source::Local))
        && lince_resources::address(&resource.uri).is_ok_and(|uri| uri.scheme() == "file")
    {
        crate::native::edit_mode::label(
            world,
            parent,
            "This remote reference needs a downloadable source address.",
            13.0,
        );
        return;
    }
    if world.get::<Card>(owner).is_some() {
        crate::native::resource_preview::spawn(world, parent, resource);
    }
    crate::native::edit_mode::label(
        world,
        parent,
        &format!("{} · {}", resource.title, resource.media_type),
        13.0,
    );
    for text in [&resource.attribution, &resource.license] {
        if !text.is_empty() {
            crate::native::edit_mode::label(world, parent, text, 12.0);
        }
    }
    if let Some(page) = resource.locator.page {
        let label = resource
            .locator
            .end_page
            .filter(|end| *end != page)
            .map_or_else(
                || format!("Page {page}"),
                |end| format!("Pages {page}–{end}"),
            );
        crate::native::edit_mode::label(world, parent, &label, 12.0);
    }
    let row = panel::row(world, parent);
    panel::credits(world, row, parent, crate::native::document_viewer::CREDITS);
    panel::button(
        world,
        row,
        owner,
        "Open source",
        Operation::Resource(resource.clone()),
    );
    for (index, representation) in resource.representations.iter().enumerate() {
        let Ok(selected) = resource.representation(index) else {
            continue;
        };
        panel::button(
            world,
            row,
            owner,
            &format!("Open {}", representation.label),
            Operation::Resource(selected),
        );
        if let Some(reference) = world.get::<Card>(owner).map(|card| card.reference.clone()) {
            panel::button(
                world,
                row,
                owner,
                &format!("Keep {} offline", representation.label),
                Operation::Request(Command::Download {
                    record: reference,
                    representation: Some(index),
                }),
            );
        }
    }
    if world.get::<Card>(owner).is_some() {
        let reference = world.get::<Card>(owner).unwrap().reference.clone();
        panel::button(
            world,
            row,
            owner,
            "Download / keep offline",
            Operation::Request(Command::Download {
                record: reference.clone(),
                representation: None,
            }),
        );
        panel::button(
            world,
            row,
            owner,
            "Use local copy…",
            Operation::LocalForm(reference),
        );
    }
    if resource.media_type.starts_with("video/")
        && let Some(reference) = world.get::<Card>(owner).map(|card| card.reference.clone())
    {
        let start = panel::field(
            world,
            parent,
            "Interval start (seconds)",
            &(resource.locator.start_ms.unwrap_or(0) as f64 / 1000.0).to_string(),
        );
        let end = panel::field(
            world,
            parent,
            "Interval end (seconds)",
            &resource
                .locator
                .end_ms
                .map(|end| (end as f64 / 1000.0).to_string())
                .unwrap_or_default(),
        );
        let row = panel::row(world, parent);
        panel::button(
            world,
            row,
            owner,
            "Keep interval",
            Operation::Retain(reference.clone(), start, end, None),
        );
        for (index, representation) in resource.representations.iter().enumerate() {
            panel::button(
                world,
                row,
                owner,
                &format!("Keep {} interval", representation.label),
                Operation::Retain(reference.clone(), start, end, Some(index)),
            );
        }
        crate::native::edit_mode::label(
            world,
            parent,
            "Some origins require the complete source before an interval can be retained.",
            12.0,
        );
    }
    if !resource.attribution.is_empty() {
        crate::native::edit_mode::label(
            world,
            parent,
            &format!("{} {}", resource.attribution, resource.license),
            12.0,
        );
    }
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    if matches!(message,ServerMessage::Error{id,code,..} if id==crate::native::cell_bridge::CONNECTION || id=="connection" || code.as_deref()==Some("session_expired"))
    {
        let cards = world
            .query::<(Entity, &Card)>()
            .iter(world)
            .filter(|(_, card)| &card.source == source)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        for card in cards {
            panel::clear(world, card);
            world.get_mut::<Card>(card).unwrap().requested = false;
        }
    }
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return,
    };
    let Some((owner, origin, card)) = world
        .get_resource::<Pending>()
        .and_then(|pending| pending.0.get(id))
        .cloned()
    else {
        return;
    };
    if &origin != source {
        return;
    }
    world.resource_mut::<Pending>().0.remove(id);
    if !world.entities().contains(owner) {
        return;
    }
    if world.get::<Embedded>(owner).is_some() {
        match message {
            ServerMessage::ActionOk {
                data: Some(data), ..
            } => crate::native::description::external_transclusion(world, owner, data),
            ServerMessage::Error { message, .. } => status(world, owner, message),
            _ => {}
        }
        return;
    }
    match message {
        ServerMessage::ActionOk { data: None, .. } if card => {
            let reference = world.get::<Card>(owner).unwrap().reference.clone();
            request(
                world,
                owner,
                source.clone(),
                Command::Inspect { record: reference },
                true,
            );
        }
        ServerMessage::ActionOk {
            data: Some(data), ..
        } if card => {
            if !data["related"].is_array() {
                let reference = world.get::<Card>(owner).unwrap().reference.clone();
                request(
                    world,
                    owner,
                    source.clone(),
                    Command::Inspect { record: reference },
                    true,
                );
                return;
            }
            panel::clear(world, owner);
            if (data["resource"].is_object()
                || data["related"]
                    .as_array()
                    .is_some_and(|related| !related.is_empty())
                || data["controls"].is_object())
                && let Some(record) = data["record"].as_str()
            {
                panel::button(
                    world,
                    owner,
                    owner,
                    "Refresh state",
                    Operation::Request(Command::Inspect {
                        record: record.into(),
                    }),
                );
            }
            if let Ok(resource) =
                serde_json::from_value::<lince_resources::Resource>(data["resource"].clone())
            {
                media(world, owner, owner, &resource);
                crate::native::edit_mode::label(
                    world,
                    owner,
                    if data["local"].is_object() {
                        "Available offline"
                    } else if data["cached"]["bytes"]
                        .as_u64()
                        .is_some_and(|bytes| bytes > 0)
                    {
                        "Partial source cached · Keep offline retains the complete file"
                    } else {
                        "Referenced source · requested parts are cached when supported"
                    },
                    12.0,
                );
            }
            for related in data["related"].as_array().into_iter().flatten() {
                let arrow = if related["direction"] == "in" {
                    "←"
                } else {
                    "→"
                };
                if let Some(uri) = related["uri"].as_str() {
                    let link = panel::button(
                        world,
                        owner,
                        owner,
                        &format!(
                            "{} {arrow} {}",
                            related["predicate"].as_str().unwrap_or(""),
                            link_label(uri)
                        ),
                        Operation::Browse(lince_resources::Reference::new(uri), None),
                    );
                    world
                        .entity_mut(link)
                        .insert(crate::native::icons::Tooltip(uri.into()));
                }
                if let Some(uid) = related["uid"].as_str() {
                    let row = panel::row(world, owner);
                    let label = format!(
                        "{} {arrow} {} ({})",
                        related["predicate"].as_str().unwrap_or(""),
                        related["title"].as_str().unwrap_or(uid),
                        related["quantity"].as_str().unwrap_or("0")
                    );
                    panel::button(
                        world,
                        row,
                        owner,
                        &label,
                        Operation::Open(uid.into(), source.clone()),
                    );
                    if let Ok(resource) = serde_json::from_value::<lince_resources::Resource>(
                        related["resource"].clone(),
                    ) {
                        let card = panel::column(world, owner);
                        world.entity_mut(card).insert(Card {
                            reference: uid.into(),
                            source: source.clone(),
                            requested: true,
                        });
                        media(world, card, card, &resource);
                    }
                }
            }
            for control in data["controls"]["quantities"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let (Some(label), Some(value), Some(uid)) = (
                    control["label"].as_str(),
                    control["value"].as_str(),
                    data["record"].as_str(),
                ) {
                    panel::button(
                        world,
                        owner,
                        owner,
                        label,
                        Operation::Quantity(uid.into(), value.into()),
                    );
                }
            }
            for control in data["controls"]["actions"]
                .as_array()
                .into_iter()
                .flatten()
                .take(32)
            {
                if let Ok(action) =
                    serde_json::from_value::<engine::actions::Action>(control["action"].clone())
                    && matches!(
                        action,
                        engine::actions::Action::AssertRecord { .. }
                            | engine::actions::Action::RetractRecord { .. }
                            | engine::actions::Action::SetQuantityExact { .. }
                    )
                {
                    panel::button(
                        world,
                        owner,
                        owner,
                        control["label"].as_str().unwrap_or("Apply declared change"),
                        Operation::Execute(Box::new(action)),
                    );
                }
            }
            if data["local"].is_object() && data["resource"].is_null() {
                let reference = world.get::<Card>(owner).unwrap().reference.clone();
                request(
                    world,
                    owner,
                    source.clone(),
                    Command::Inspect { record: reference },
                    true,
                );
            }
        }
        ServerMessage::ActionOk {
            data: Some(data), ..
        } => {
            if card {
                let reference = world.get::<Card>(owner).unwrap().reference.clone();
                request(
                    world,
                    owner,
                    source.clone(),
                    Command::Inspect { record: reference },
                    true,
                );
                return;
            }
            if let Some(uid) = data["local_record"].as_str() {
                Operation::Open(uid.into(), source.clone()).apply(world, owner);
                if world.get::<ReferenceSource>(owner).is_some() {
                    world.despawn(owner);
                }
                return;
            }
            if let Some(uri) = data["web"].as_str() {
                crate::native::description::open_web(world, owner, uri);
                return;
            }
            if let Ok(resource) =
                serde_json::from_value::<lince_resources::Resource>(data["resource"].clone())
            {
                Operation::Resource(resource).apply(world, owner);
                return;
            }
            if let Some(entry) = data["entry"].as_str() {
                let mut target = lince_resources::address(entry).ok();
                if let Some(target) = &mut target {
                    target.set_fragment(data["fragment"].as_str());
                }
                let uri = target.map_or_else(|| entry.to_owned(), |target| target.to_string());
                world.get_mut::<SourceView>(owner).unwrap().uri = uri.clone();
                let field = world.get::<View>(owner).unwrap().input;
                if let Some(mut text) = world.get_mut::<bevy::text::EditableText>(field) {
                    text.editor.set_text(&uri);
                }
            }
            review_state(&mut world.get_mut::<SourceView>(owner).unwrap(), data);
            if data["extracted"] == true {
                Operation::Read.apply(world, owner);
            }
            if data["records"].is_array() {
                render(world, owner, data.clone());
            } else if data["preview"].is_object() {
                Operation::Preview.apply(world, owner);
            }
            status(world, owner, "Ready.");
        }
        ServerMessage::Error { message, .. } => status(world, owner, message),
        _ => {}
    }
}

fn review_state(castle: &mut SourceView, data: &Value) {
    if data["browsing"] == true {
        castle.current = None;
        castle.binding = None;
    }
    let preview = data["records"].is_array();
    if preview {
        castle.binding = data["binding"]
            .as_str()
            .filter(|_| data["binding_ready"] == true)
            .map(str::to_owned);
    } else if let Some(binding) = data["binding"].as_str() {
        castle.binding = Some(binding.into());
    }
    if castle.current.is_none()
        && let Some(root) = data["root"].as_str()
        && (!preview
            || data["records"].as_array().is_some_and(|records| {
                records
                    .iter()
                    .any(|record| record["uid"] == root && record["state"] != "create")
            }))
    {
        castle.current = Some(root.into());
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let messages = world
        .get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for message in messages {
        receive(world, &Source::Local, &message);
    }
    let cards = world
        .query::<(Entity, &Card)>()
        .iter(world)
        .filter(|(_, card)| !card.requested)
        .map(|(entity, card)| (entity, card.reference.clone(), card.source.clone()))
        .collect::<Vec<_>>();
    for (entity, reference, source) in cards {
        if crate::native::description::source_sender(world, &source).is_none() {
            continue;
        }
        world.get_mut::<Card>(entity).unwrap().requested = true;
        request(
            world,
            entity,
            source,
            Command::Inspect { record: reference },
            true,
        );
    }
    let mut pending = world.remove_resource::<Pending>().unwrap();
    pending
        .0
        .retain(|_, (owner, _, _)| world.entities().contains(*owner));
    world.insert_resource(pending);
}

#[derive(Component, Clone)]
pub(crate) struct ReferenceBase(pub String);

pub(crate) fn open_reference(
    world: &mut World,
    context: &crate::native::description::Context,
    target: &str,
) {
    let mut cursor = Some(context.owner);
    let mut base = None;
    let mut record = None;
    let mut root = None;
    while let Some(entity) = cursor {
        if base.is_none() {
            base = world
                .get::<ReferenceBase>(entity)
                .map(|base| base.0.clone());
        }
        if record.is_none() {
            record = world
                .get::<crate::native::protein_area::RecordBinding>(entity)
                .map(|binding| binding.uid.clone());
        }
        if world
            .get::<crate::native::workspace::Workspaces>(entity)
            .is_some()
        {
            root = Some(entity);
            break;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    let Some(root) = root else {
        return;
    };
    let reference = lince_resources::Reference {
        target: target.into(),
        base,
        ..Default::default()
    };
    if !matches!(context.source, Source::Local)
        && reference
            .base
            .as_deref()
            .is_some_and(|base| base.starts_with("file:"))
    {
        return;
    }
    let workspace = world
        .get::<crate::native::workspace::Workspaces>(root)
        .map_or(1, |spaces| spaces.active);
    let position = world
        .get::<crate::native::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |view| view.center);
    let owner = spawn(
        world,
        root,
        workspace,
        position,
        SourceView {
            uri: target.into(),
            ..Default::default()
        },
    );
    world
        .entity_mut(owner)
        .insert(ReferenceSource(context.source.clone()));
    request(
        world,
        owner,
        context.source.clone(),
        Command::Open { reference, record },
        false,
    );
}

#[derive(Component, Clone)]
struct ReferenceSource(Source);

fn reference_source(world: &World, owner: Entity) -> Source {
    world
        .get::<Card>(owner)
        .map(|card| card.source.clone())
        .or_else(|| {
            world
                .get::<ReferenceSource>(owner)
                .map(|source| source.0.clone())
        })
        .unwrap_or(Source::Local)
}

#[derive(Component)]
struct Embedded;

pub(crate) fn embed_reference(
    world: &mut World,
    entity: Entity,
    target: &str,
    context: &crate::native::description::Context,
) {
    let mut cursor = Some(context.owner);
    let mut base = None;
    let mut record = None;
    while let Some(owner) = cursor {
        if base.is_none() {
            base = world.get::<ReferenceBase>(owner).map(|base| base.0.clone());
        }
        if record.is_none() {
            record = world
                .get::<crate::native::protein_area::RecordBinding>(owner)
                .map(|binding| binding.uid.clone());
        }
        cursor = world.get::<ChildOf>(owner).map(ChildOf::parent);
    }
    world
        .entity_mut(entity)
        .insert((Embedded, ReferenceSource(context.source.clone())));
    request(
        world,
        entity,
        context.source.clone(),
        Command::Open {
            reference: lince_resources::Reference {
                target: target.into(),
                base,
                ..Default::default()
            },
            record,
        },
        false,
    );
}

pub(crate) fn embedded_media(
    world: &mut World,
    entity: Entity,
    resource: &lince_resources::Resource,
) {
    if !matches!(reference_source(world, entity), Source::Local)
        && resource.uri.starts_with("file:")
    {
        return;
    }
    crate::native::resource_preview::spawn(world, entity, resource);
    media(world, entity, entity, resource);
}
