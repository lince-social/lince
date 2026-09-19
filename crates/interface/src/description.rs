mod diagram;
mod markup;
mod pictures;
pub(crate) mod tests;

use crate::{
    actions::{Action, ActionButton},
    protein_area::Source,
    tokens::Token,
};
use bevy::{prelude::*, text::EditableText};

pub const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::SYMBOLS,
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

#[derive(Component)]
struct Editor {
    input: Entity,
    preview: Entity,
}

#[derive(Component)]
pub(crate) struct Rendered;

pub struct DescriptionPlugin;
impl Plugin for DescriptionPlugin {
    fn build(&self, app: &mut App) {
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
    markup::render(world, entity, source, &context);
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
    if let Some(children) = world.get::<Children>(entity) {
        let children: Vec<_> = children.iter().collect();
        for child in children {
            world.despawn(child);
        }
    }
    world.get_mut::<Description>(entity).unwrap().source = source.into();
    markup::render(world, entity, source, &context);
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
    let preview = spawn(world, parent, &source, context);
    world.entity_mut(parent).insert(Editor { input, preview });
    world.get_mut::<Node>(input).unwrap().display = Display::Flex;
}

pub(crate) fn refresh_readonly(world: &mut World, container: Entity, source: &str) {
    if world.get::<Editor>(container).is_some() {
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
struct Link {
    reference: String,
    context: Context,
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
                include_bytes!("../../../assets/fonts/Lato/Lato-Bold.ttf").to_vec(),
            )),
            italic: assets.add(Font::from_bytes(
                include_bytes!("../../../assets/fonts/Lato/Lato-Italic.ttf").to_vec(),
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
