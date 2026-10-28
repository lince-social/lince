mod assets;
mod diagram;
mod live;
mod markup;
mod pictures;
pub(crate) mod shader;
pub(crate) mod tests;
mod transclusion;

pub(crate) fn source_sender(
    world: &World,
    source: &Source,
) -> Option<tokio::sync::mpsc::Sender<cell::ClientMessage>> {
    live::sender(world, source)
}

pub const SHADER_EXAMPLE: &str = shader::BALL;

use crate::native::{
    actions::{Action, ActionButton},
    protein_area::Source,
    tokens::Token,
};
use bevy::{prelude::*, text::EditableText};

pub const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "Chrono",
        author: "Kang Seonghoon and contributors",
        license: crate::native::credits::CHRONO_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Tempfile",
        author: "Steven Allen and contributors",
        license: include_str!("../../licenses/native/document/tempfile-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "URL",
        author: "The rust-url developers",
        license: include_str!("../../licenses/native/url-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "BLAKE3",
        author: "Jack O'Connor, Samuel Neves and contributors",
        license: include_str!("../../licenses/native/document/blake3-CC0.txt"),
    },
    crate::native::credits::Attribution {
        name: "Naga",
        author: "The gfx-rs developers",
        license: include_str!("../../licenses/native/naga-MIT.txt"),
    },
    crate::native::credits::SYMBOLS,
    crate::native::credits::DEJAVU,
    crate::native::credits::FONTIQUE,
    crate::native::credits::Attribution {
        name: "reqwest",
        author: "Sean McArthur and contributors",
        license: include_str!("../../licenses/native/reqwest-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "image",
        author: "The image-rs developers",
        license: include_str!("../../licenses/native/image-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "base64",
        author: "The base64 contributors",
        license: include_str!("../../licenses/native/base64-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "PNG codec",
        author: "nwin and image-rs contributors",
        license: include_str!("../../licenses/native/png-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "image-webp",
        author: "The image-rs developers",
        license: include_str!("../../licenses/native/image-webp-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "pulldown-cmark",
        author: "Raph Levien and contributors",
        license: include_str!("../../licenses/native/pulldown-cmark-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "mermaid-rs-renderer",
        author: "1jehuang and contributors",
        license: include_str!("../../licenses/native/mermaid-rs-renderer-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "resvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../../licenses/native/resvg-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::native::credits::LATO_LICENSE,
    },
];

#[derive(Clone)]
pub struct Context {
    pub owner: Entity,
    pub source: Source,
}

#[derive(Component)]
pub struct Description {
    pub source: String,
    context: Context,
}

#[derive(Component, Clone, Copy)]
struct Editor {
    input: Entity,
    preview: Entity,
    panes: Entity,
    editable: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum Mode {
    Raw,
    Pretty,
    Split,
}

#[derive(Component)]
pub(crate) struct Rendered;

pub struct DescriptionPlugin;
impl Plugin for DescriptionPlugin {
    fn build(&self, app: &mut App) {
        shader::install(app);
        crate::native::embedded_media::install(app);
        app.add_systems(
            Update,
            (
                sync_editors,
                diagram::update,
                pictures::update,
                live::update,
                assets::expire,
            ),
        );
    }
}

pub fn spawn(world: &mut World, parent: Entity, source: &str, context: Context) -> Entity {
    let entity = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            Description {
                source: source.into(),
                context: context.clone(),
            },
            crate::native::sand_store::SandCredits(CREDITS),
            Rendered,
            ChildOf(parent),
        ))
        .id();
    markup::render(world, entity, source, &context, Vec::new());
    entity
}

pub fn set(world: &mut World, entity: Entity, source: &str) {
    let Some(description) = world.get::<Description>(entity) else {
        return;
    };
    if description.source == source {
        return;
    }
    let context = description.context.clone();
    let mut shaders = Vec::new();
    if let Some(children) = world.get::<Children>(entity) {
        let children: Vec<_> = children.iter().collect();
        for child in children {
            if world.get::<shader::Preview>(child).is_some() {
                world.entity_mut(child).remove::<ChildOf>();
                shaders.push(child);
            } else {
                world.despawn(child);
            }
        }
    }
    world.get_mut::<Description>(entity).unwrap().source = source.into();
    markup::render(world, entity, source, &context, shaders);
}

pub(crate) fn protects(world: &World, mut entity: Entity) -> bool {
    loop {
        if world.get::<Rendered>(entity).is_some() {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
}

pub(crate) fn attach_editor(world: &mut World, parent: Entity, input: Entity, context: Context) {
    let source = world
        .get::<EditableText>(input)
        .unwrap()
        .value()
        .to_string();
    attach(world, parent, input, &source, context, true);
}

pub(crate) fn readonly(world: &mut World, parent: Entity, source: &str, context: Context) {
    let input = crate::native::edit_mode::label(world, parent, source, 14.0);
    attach(world, parent, input, source, context, false);
    set_mode(world, parent, Mode::Pretty);
}

fn attach(
    world: &mut World,
    parent: Entity,
    input: Entity,
    source: &str,
    context: Context,
    editable: bool,
) {
    let controls = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            },
            Rendered,
            ChildOf(parent),
        ))
        .id();
    for (title, mode) in [
        ("Raw", Mode::Raw),
        ("Pretty", Mode::Pretty),
        ("Side by side", Mode::Split),
    ] {
        button(world, controls, parent, title, mode);
    }
    if editable {
        button(
            world,
            controls,
            parent,
            "Draw…",
            crate::native::drawing::DrawDescription(input),
        );
        button(
            world,
            controls,
            parent,
            "Transclude…",
            InsertTransclusion(input),
        );
    }
    let panes = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                align_items: AlignItems::Start,
                column_gap: px(12),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world.entity_mut(input).insert(ChildOf(panes));
    let preview = spawn(world, panes, source, context);
    world.entity_mut(parent).insert(Editor {
        input,
        preview,
        panes,
        editable,
    });
    if !editable {
        world.entity_mut(input).insert(Rendered);
    }
    set_mode(world, parent, Mode::Split);
}

pub(crate) fn receive_live(world: &mut World, source: &Source, message: &cell::ServerMessage) {
    live::receive(world, source, message);
}

pub(crate) fn insert_asset(
    world: &mut World,
    panel: Entity,
    input: Entity,
    kind: nucleus::description_asset::Kind,
    bytes: Vec<u8>,
    replace: Option<String>,
) -> Result<(), String> {
    assets::insert(world, panel, input, kind, bytes, replace)
}

#[derive(Clone, Copy)]
struct InsertTransclusion(Entity);

impl Action for InsertTransclusion {
    fn apply(&self, world: &mut World, parent: Entity) {
        if !crate::native::record_binding::can_edit(world, self.0) {
            return;
        }
        let panel = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
                Rendered,
                ChildOf(parent),
            ))
            .id();
        crate::native::edit_mode::label(
            world,
            panel,
            "Transclude a Record by UID or slug. Its current description appears read only.",
            14.0,
        );
        let bundle = crate::native::sand::text_editor(
            "",
            world.resource::<crate::native::theme::Typography>(),
            0,
        );
        let input = world.spawn((bundle, ChildOf(panel))).id();
        world.get_mut::<EditableText>(input).unwrap().max_characters = Some(256);
        button(
            world,
            panel,
            panel,
            "Insert transclusion",
            ConfirmTransclusion {
                input,
                destination: self.0,
            },
        );
        button(world, panel, panel, "Cancel", CloseInsertion);
    }
}

#[derive(Clone, Copy)]
struct ConfirmTransclusion {
    input: Entity,
    destination: Entity,
}

impl Action for ConfirmTransclusion {
    fn apply(&self, world: &mut World, panel: Entity) {
        let Some(text) = world.get::<EditableText>(self.input) else {
            return;
        };
        let reference = text.value().to_string();
        let reference = reference.trim().trim_start_matches(['@', '#']);
        if reference.is_empty() || reference.contains(['[', ']', '|', '\n', '\r', ':', '/', '\\']) {
            crate::native::edit_mode::label(world, panel, "Enter a Record UID or slug.", 14.0);
            return;
        }
        match assets::append(world, self.destination, &format!("![[{reference}]]")) {
            Ok(()) => {
                world.despawn(panel);
            }
            Err(error) => {
                crate::native::edit_mode::label(world, panel, &error, 14.0);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct CloseInsertion;
impl Action for CloseInsertion {
    fn apply(&self, world: &mut World, panel: Entity) {
        world.despawn(panel);
    }
}

pub(crate) fn set_mode(world: &mut World, parent: Entity, mode: Mode) {
    let Some(editor) = world.get::<Editor>(parent).copied() else {
        return;
    };
    for (entity, visible) in [
        (editor.input, !matches!(mode, Mode::Pretty)),
        (editor.preview, !matches!(mode, Mode::Raw)),
    ] {
        let mut node = world.get_mut::<Node>(entity).unwrap();
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        node.width = if matches!(mode, Mode::Split) {
            px(0)
        } else {
            percent(100)
        };
        node.min_width = px(0);
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
    }
    world.get_mut::<Node>(editor.panes).unwrap().flex_direction = FlexDirection::Row;
    if matches!(mode, Mode::Pretty)
        && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>()
        && focus.get() == Some(editor.input)
    {
        focus.clear();
    }
}

impl Action for Mode {
    fn apply(&self, world: &mut World, owner: Entity) {
        set_mode(world, owner, *self);
    }
}

pub(crate) fn refresh_readonly(world: &mut World, container: Entity, source: &str) {
    if let Some(editor) = world.get::<Editor>(container).copied() {
        if !editor.editable {
            world.get_mut::<Text>(editor.input).unwrap().0 = source.into();
            set(world, editor.preview, source);
        }
        return;
    }
    let preview = world
        .get::<Children>(container)
        .into_iter()
        .flatten()
        .find(|child| world.get::<Description>(**child).is_some())
        .copied();
    if let Some(preview) = preview {
        set(world, preview, source);
    }
}

fn sync_editors(world: &mut World) {
    let changes: Vec<_> = world
        .query::<&Editor>()
        .iter(world)
        .filter_map(|editor| {
            if !editor.editable {
                return None;
            }
            let source = world.get::<EditableText>(editor.input)?.value().to_string();
            (world.get::<Description>(editor.preview)?.source != source)
                .then_some((editor.preview, source))
        })
        .collect();
    for (preview, source) in changes {
        set(world, preview, &source);
    }
}

pub(crate) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    text: &str,
    action: impl Action,
) -> Entity {
    let entity = world
        .spawn((
            crate::native::sand::Square,
            crate::native::sand::button(0),
            Node {
                padding: UiRect::axes(px(7), px(5)),
                min_height: px(30),
                ..default()
            },
            crate::native::icons::Tooltip(text.into()),
            ActionButton::new(owner, crate::native::actions![action]),
            ChildOf(parent),
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(text);
    }
    crate::native::edit_mode::label(world, entity, text, 14.0);
    entity
}

#[derive(Clone)]
pub(crate) struct Link {
    pub(crate) reference: String,
    pub(crate) context: Context,
}
impl Action for Link {
    fn practice_intent(&self) -> crate::native::actions::PracticeIntent {
        crate::native::actions::PracticeIntent::Navigation
    }
    fn apply(&self, world: &mut World, _: Entity) {
        let reference = self
            .reference
            .trim()
            .trim_start_matches("record:")
            .trim_start_matches('@');
        if lince_resources::reference::is_address(reference)
            || reference.contains(".lingua#")
            || reference.starts_with('#')
        {
            crate::native::source_view::open_reference(world, &self.context, reference);
            return;
        }
        let mut cursor = Some(self.context.owner);
        while let Some(entity) = cursor {
            if crate::native::instinct::practice::follow(world, entity, reference) {
                return;
            }
            if world
                .get::<crate::native::instinct::Instinct>(entity)
                .is_some()
                && crate::native::instinct::follow(world, entity, reference)
            {
                return;
            }
            if world
                .get::<crate::native::workspace::Workspaces>(entity)
                .is_some()
            {
                let mut current = Some(self.context.owner);
                let mut scoped = false;
                while let Some(owner) = current {
                    scoped |= world
                        .get::<crate::native::source_view::ReferenceBase>(owner)
                        .is_some()
                        || world
                            .get::<crate::native::protein_area::RecordBinding>(owner)
                            .is_some();
                    current = world.get::<ChildOf>(owner).map(ChildOf::parent);
                }
                if scoped || lince_resources::reference::record_uid(reference) {
                    crate::native::source_view::open_reference(world, &self.context, reference);
                } else {
                    crate::native::full_record::open(
                        world,
                        entity,
                        reference,
                        self.context.source.clone(),
                    );
                }
                return;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

#[derive(Resource, Clone)]
struct Fonts {
    bold: Handle<Font>,
    italic: Handle<Font>,
}

pub(crate) fn preview_fonts(world: &mut World, preview: &mut World) {
    world.init_resource::<Fonts>();
    preview.insert_resource(world.resource::<Fonts>().clone());
}
impl FromWorld for Fonts {
    fn from_world(world: &mut World) -> Self {
        let mut assets = world.resource_mut::<Assets<Font>>();
        Self {
            bold: assets.add(Font::from_bytes(
                include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Bold.ttf").to_vec(),
            )),
            italic: assets.add(Font::from_bytes(
                include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Italic.ttf").to_vec(),
            )),
        }
    }
}

pub fn heading(world: &mut World, parent: Entity, text: &str, size: f32) {
    world.init_resource::<Fonts>();
    let font = world.resource::<Fonts>().bold.clone();
    let entity = crate::native::edit_mode::label(world, parent, text, size);
    world.get_mut::<TextFont>(entity).unwrap().font = font.into();
    world.get_mut::<Node>(entity).unwrap().width = percent(100);
}

pub(crate) fn open_web(world: &mut World, owner: Entity, uri: &str) {
    let mut cursor = Some(owner);
    let mut root = None;
    while let Some(entity) = cursor {
        if world
            .get::<crate::native::workspace::Workspaces>(entity)
            .is_some()
        {
            root = Some(entity);
            break;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    if let Some(root) = root {
        let workspace = world
            .get::<crate::native::workspace::Workspaces>(root)
            .map_or(1, |spaces| spaces.active);
        let position = world
            .get::<crate::native::canvas::CanvasView>(root)
            .map_or(bevy::math::DVec2::ZERO, |view| view.center);
        crate::native::media_sand::spawn(
            world,
            root,
            workspace,
            position,
            crate::native::media_sand::MediaSand::Link { url: uri.into() },
        );
    }
}

pub(crate) fn external_transclusion(world: &mut World, entity: Entity, data: &serde_json::Value) {
    transclusion::external(world, entity, data);
}
