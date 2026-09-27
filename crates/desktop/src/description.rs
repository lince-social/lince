mod diagram;
mod markup;
mod pictures;
mod shader;
pub(crate) mod tests;

pub const SHADER_EXAMPLE: &str = shader::BALL;

use crate::{
    actions::{Action, ActionButton},
    protein_area::Source,
    tokens::Token,
};
use bevy::{prelude::*, text::EditableText};

pub const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "Naga",
        author: "The gfx-rs developers",
        license: include_str!("../licenses/naga-MIT.txt"),
    },
    crate::credits::SYMBOLS,
    crate::credits::DEJAVU,
    crate::credits::FONTIQUE,
    crate::credits::Attribution {
        name: "reqwest",
        author: "Sean McArthur and contributors",
        license: include_str!("../licenses/reqwest-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "image",
        author: "The image-rs developers",
        license: include_str!("../licenses/image-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "base64",
        author: "The base64 contributors",
        license: include_str!("../licenses/base64-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "pulldown-cmark",
        author: "Raph Levien and contributors",
        license: include_str!("../licenses/pulldown-cmark-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "mermaid-rs-renderer",
        author: "1jehuang and contributors",
        license: include_str!("../licenses/mermaid-rs-renderer-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "resvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../licenses/resvg-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::credits::BEVY_LICENSE,
    },
    crate::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::credits::LATO_LICENSE,
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
        app.add_systems(Update, (sync_editors, diagram::update, pictures::update));
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
            crate::sand_store::SandCredits(CREDITS),
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
    let input = crate::edit_mode::label(world, parent, source, 14.0);
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
                column_gap: px(6),
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
            crate::sand::Square,
            crate::sand::button(0),
            Node {
                padding: UiRect::axes(px(7), px(5)),
                min_height: px(30),
                ..default()
            },
            crate::icons::Tooltip(text.into()),
            ActionButton::new(owner, crate::actions![action]),
            ChildOf(parent),
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(text);
    }
    crate::edit_mode::label(world, entity, text, 14.0);
    entity
}

#[derive(Clone)]
pub(crate) struct Link {
    pub(crate) reference: String,
    pub(crate) context: Context,
}
impl Action for Link {
    fn apply(&self, world: &mut World, _: Entity) {
        let reference = self
            .reference
            .trim()
            .trim_start_matches("record:")
            .trim_start_matches(['@', '#']);
        if reference.contains("://") {
            return;
        }
        let mut cursor = Some(self.context.owner);
        while let Some(entity) = cursor {
            if world.get::<crate::instinct::Instinct>(entity).is_some()
                && crate::instinct::follow(world, entity, reference)
            {
                return;
            }
            if world.get::<crate::workspace::Workspaces>(entity).is_some() {
                crate::full_record::open(world, entity, reference, self.context.source.clone());
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
                include_bytes!("../../../institute/assets/fonts/Lato/Lato-Bold.ttf").to_vec(),
            )),
            italic: assets.add(Font::from_bytes(
                include_bytes!("../../../institute/assets/fonts/Lato/Lato-Italic.ttf").to_vec(),
            )),
        }
    }
}

pub fn heading(world: &mut World, parent: Entity, text: &str, size: f32) {
    world.init_resource::<Fonts>();
    let font = world.resource::<Fonts>().bold.clone();
    let entity = crate::edit_mode::label(world, parent, text, size);
    world.get_mut::<TextFont>(entity).unwrap().font = font.into();
    world.get_mut::<Node>(entity).unwrap().width = percent(100);
}
