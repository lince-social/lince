pub(crate) mod preview;

use crate::{
    canvas::CanvasItem,
    sand::{InBox, Square},
    sand_text::{self, SandText, SavedText},
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandKind {
    Square,
    Text,
    EditableText,
    Operation,
    WorkTimer,
    AccessControl,
    Sync,
}

impl SandKind {
    pub const ALL: [Self; 7] = [
        Self::Square,
        Self::Text,
        Self::EditableText,
        Self::Operation,
        Self::WorkTimer,
        Self::AccessControl,
        Self::Sync,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Text => "Plain text",
            Self::EditableText => "Editable text",
            Self::Operation => "Operation",
            Self::WorkTimer => "Time Castle",
            Self::AccessControl => "Access Control",
            Self::Sync => "Sync",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Square => "An empty space to compose with.",
            Self::Text => "A simple text label.",
            Self::EditableText => "A note you can write in.",
            Self::Operation => "Run commands or set a Record quantity to zero by slug.",
            Self::WorkTimer => "A standalone stopwatch or a Record’s editable work log.",
            Self::AccessControl => "Manage local users, Roles and permissions.",
            Self::Sync => "Sync a Protein to a directory as .lingua or Markdown.",
        }
    }
}

#[derive(Component)]
pub struct StoredSand {
    pub kind: SandKind,
    pub content: Option<Entity>,
}

#[derive(Component)]
pub struct SandCredits(pub &'static [crate::credits::Attribution]);

#[derive(Resource)]
struct StoreFont(Handle<Font>);

impl FromWorld for StoreFont {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource_mut::<Assets<Font>>().add(Font::from_bytes(
            include_bytes!("../../../assets/fonts/Lato/Lato-Bold.ttf").to_vec(),
        )))
    }
}

#[derive(Component)]
pub struct SandPreview;

#[derive(Component)]
pub struct StoreEntry(pub SandKind);

#[derive(Component)]
pub(crate) struct PreviewSource(pub Entity);

#[derive(Component)]
pub(crate) struct PreviewKind(pub crate::tokens::SandStyleKind);

#[derive(Component)]
pub(crate) struct Dimensions(Entity);

fn dimensions(size: Vec2) -> String {
    format!("{:.0} × {:.0}", size.x, size.y)
}

pub(crate) fn refresh(
    items: Query<(&CanvasItem, &StoredSand)>,
    mut previews: Query<(&PreviewSource, &mut Node, &mut UiTransform)>,
    mut labels: Query<(&Dimensions, &mut Text)>,
) {
    for (source, mut node, mut transform) in &mut previews {
        if let Ok((item, sand)) = items.get(source.0) {
            let scale = (68.0 / item.size.x).min(60.0 / item.size.y).min(1.0);
            if node.width != px(item.size.x) || node.height != px(item.size.y) {
                node.width = px(item.size.x);
                node.height = px(item.size.y);
                node.left = px((72.0 - item.size.x) * 0.5);
                node.top = px((64.0 - item.size.y) * 0.5);
                if sand.kind == SandKind::Square {
                    node.border = UiRect::all(px(crate::sand::BUTTON_BORDER_WIDTH / scale));
                }
                transform.scale = Vec2::splat(scale);
            }
        }
    }
    for (source, mut text) in &mut labels {
        if let Ok((item, _)) = items.get(source.0) {
            let value = dimensions(item.size);
            if text.0 != value {
                text.0 = value;
            }
        }
    }
}

fn short_name(name: &str) -> String {
    let mut chars = name.chars();
    let mut value: String = chars.by_ref().take(22).collect();
    if chars.next().is_some() {
        value.push('…');
    }
    value
}

pub(crate) fn entry(
    world: &mut World,
    root: Entity,
    parent: Entity,
    kind: SandKind,
    existing: Option<(Entity, usize)>,
) {
    use crate::edit_mode::{EditAction, control, label};
    world.init_resource::<StoreFont>();
    let style_kind = match kind {
        SandKind::Square
        | SandKind::Operation
        | SandKind::WorkTimer
        | SandKind::AccessControl
        | SandKind::Sync => crate::tokens::SandStyleKind::Square,
        SandKind::Text => crate::tokens::SandStyleKind::Text,
        SandKind::EditableText => crate::tokens::SandStyleKind::EditableText,
    };
    let settings = world.resource::<crate::tokens::ThemeSettings>();
    let width = settings
        .resolve(
            crate::tokens::Token::Width,
            Some(style_kind),
            &Default::default(),
        )
        .0
        .number();
    let height = settings
        .resolve(
            crate::tokens::Token::Height,
            Some(style_kind),
            &Default::default(),
        )
        .0
        .number();
    let size = existing
        .and_then(|(entity, _)| world.get::<CanvasItem>(entity).map(|item| item.size))
        .unwrap_or(
            if kind == SandKind::Operation {
                crate::operation::SIZE
            } else if matches!(kind, SandKind::AccessControl | SandKind::Sync) {
                Vec2::new(520.0, 540.0)
            } else {
                Vec2::new(width, height)
            },
        );
    let texts = existing
        .map(|(entity, _)| sand_text::snapshot(world, entity))
        .unwrap_or_else(|| {
            if kind == SandKind::Square {
                Vec::new()
            } else {
                vec![SavedText {
                    area: SandText::new(kind == SandKind::EditableText),
                    text: kind.name().into(),
                }]
            }
        });
    let name = existing.map_or_else(
        || kind.name().to_string(),
        |(_, index)| format!("{} {}", kind.name(), index + 1),
    );
    let row = world
        .spawn((
            crate::castle::Castle,
            StoreEntry(kind),
            ChildOf(parent),
            Node {
                width: percent(100),
                column_gap: px(10),
                padding: UiRect::all(px(10)),
                align_items: AlignItems::FlexStart,
                justify_content: JustifyContent::FlexStart,
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    if existing.is_none() {
        world.entity_mut(row).insert((
            crate::sand::button(0),
            crate::token_style::border(crate::tokens::Token::Accent),
            crate::edit_mode::EditControl {
                root,
                action: EditAction::AddSand(kind),
            },
            crate::actions::ActionButton::new(root, crate::actions![EditAction::AddSand(kind)]),
        ));
    }
    let preview = world
        .spawn((
            SandPreview,
            ChildOf(row),
            Pickable::IGNORE,
            Node {
                width: px(72),
                height: px(64),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    let (mut scene, scene_root) = preview::scene(world);
    let source = spawn_sand(&mut scene, scene_root, 1, kind, kind.name(), DVec2::ZERO);
    if matches!(
        kind,
        SandKind::Square | SandKind::Text | SandKind::EditableText | SandKind::WorkTimer
    ) {
        if let Some(children) = scene.get::<Children>(source) {
            let children: Vec<_> = children.iter().collect();
            for child in children {
                scene.despawn(child);
            }
        }
        for text in texts {
            sand_text::spawn(&mut scene, source, text);
        }
    }
    if kind == SandKind::WorkTimer {
        let input = scene
            .get::<Children>(source)
            .and_then(|children| children.first().copied());
        crate::work_timer::populate(&mut scene, source, None, &serde_json::Value::Null, input);
    }
    let miniature = preview::snapshot(&scene, source, world, preview);
    preview::fit(world, miniature, size);
    world.entity_mut(miniature).insert(PreviewKind(style_kind));
    if kind == SandKind::Square {
        let scale = (68.0 / size.x).min(60.0 / size.y).min(1.0);
        world.get_mut::<Node>(miniature).unwrap().border =
            UiRect::all(px(crate::sand::BUTTON_BORDER_WIDTH / scale));
        world.entity_mut(miniature).insert((
            crate::token_style::background(crate::tokens::Token::SandBackground),
            crate::token_style::border(crate::tokens::Token::SandBorder),
        ));
    }
    if let Some((source, _)) = existing {
        world.entity_mut(miniature).insert(PreviewSource(source));
    }
    let metadata = world
        .spawn((
            ChildOf(row),
            Node {
                min_width: px(0),
                flex_grow: 1.0,
                flex_basis: px(0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                row_gap: px(4),
                ..default()
            },
        ))
        .id();
    let font = TextFont {
        font: world.resource::<StoreFont>().0.clone().into(),
        font_size: FontSize::Px(16.0),
        ..default()
    };
    world.spawn((
        Text::new(short_name(&name)),
        font,
        crate::token_style::text(crate::tokens::Token::Ink),
        ChildOf(metadata),
        Node {
            width: percent(100),
            overflow: Overflow::clip(),
            ..default()
        },
    ));
    label(world, metadata, kind.description(), 13.0);
    let measurement = label(world, metadata, &dimensions(size), 12.0);
    if let Some((source, _)) = existing {
        world.entity_mut(measurement).insert(Dimensions(source));
    }
    if let Some((entity, index)) = existing {
        let buttons = world
            .spawn((
                ChildOf(row),
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    flex_shrink: 0.0,
                    ..default()
                },
            ))
            .id();
        control(
            world,
            root,
            buttons,
            EditAction::EditSand(entity),
            &format!("Edit text in {} {}", kind.name(), index + 1),
        );
        control(
            world,
            root,
            buttons,
            EditAction::RemoveSand(entity),
            &format!("Remove {} {}", kind.name(), index + 1),
        );
    }
}

pub(crate) fn castle_entry(
    world: &mut World,
    root: Entity,
    parent: Entity,
    title: &str,
    description: &str,
    action: impl crate::actions::Action,
    populate: impl FnOnce(&mut World, Entity) -> Entity,
) -> Entity {
    let (mut scene, scene_root) = preview::scene(world);
    let source = populate(&mut scene, scene_root);
    let size = scene
        .get::<CanvasItem>(source)
        .map_or(Vec2::new(560.0, 720.0), |item| item.size);
    let row = world
        .spawn((
            crate::castle::Castle,
            crate::sand::button(0),
            crate::actions::ActionButton::new(root, crate::actions![action]),
            crate::token_style::border(crate::tokens::Token::Accent),
            ChildOf(parent),
            Node {
                width: percent(100),
                padding: UiRect::all(px(10)),
                column_gap: px(10),
                align_items: AlignItems::FlexStart,
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let preview = world
        .spawn((
            SandPreview,
            ChildOf(row),
            Pickable::IGNORE,
            Node {
                width: px(72),
                height: px(64),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    let miniature = preview::snapshot(&scene, source, world, preview);
    preview::fit(world, miniature, size);
    let metadata = world
        .spawn((
            ChildOf(row),
            Node {
                min_width: px(0),
                flex_grow: 1.0,
                flex_basis: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
        ))
        .id();
    world.init_resource::<StoreFont>();
    let font = TextFont {
        font: world.resource::<StoreFont>().0.clone().into(),
        font_size: FontSize::Px(16.0),
        ..default()
    };
    world.spawn((
        Text::new(title),
        font,
        crate::token_style::text(crate::tokens::Token::Ink),
        ChildOf(metadata),
    ));
    crate::edit_mode::label(world, metadata, description, 13.0);
    crate::edit_mode::label(world, metadata, &dimensions(size), 12.0);
    row
}

pub fn spawn_sand(
    world: &mut World,
    root: Entity,
    workspace: u64,
    kind: SandKind,
    text: &str,
    position: DVec2,
) -> Entity {
    let elevation = world
        .get::<crate::topology::view::View>(root)
        .map_or(0.0, |view| view.plane);
    let sand = world
        .spawn((
            Square,
            InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::topology::Spatial {
                elevation,
                ..default()
            },
            SandCredits(crate::credits::ATTRIBUTIONS),
            CanvasItem {
                position,
                size: if kind == SandKind::Operation {
                    crate::operation::SIZE
                } else if matches!(kind, SandKind::AccessControl | SandKind::Sync) {
                    Vec2::new(520.0, 540.0)
                } else if kind == SandKind::WorkTimer {
                    Vec2::new(360.0, 520.0)
                } else {
                    Vec2::new(248.0, 184.0)
                },
            },
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(if kind == SandKind::Square {
                crate::theme::PAPER
            } else {
                Color::NONE
            }),
        ))
        .id();
    let content = match kind {
        SandKind::Operation => Some(crate::operation::populate(world, root, sand)),
        SandKind::AccessControl => Some(crate::access_control::populate(world, root, sand)),
        SandKind::Sync => Some(crate::sync_castle::populate(world, root, sand)),
        SandKind::Square => None,
        SandKind::Text | SandKind::EditableText | SandKind::WorkTimer => Some(sand_text::spawn(
            world,
            sand,
            SavedText {
                area: SandText::new(matches!(kind, SandKind::EditableText | SandKind::WorkTimer)),
                text: text.into(),
            },
        )),
    };
    world.entity_mut(sand).insert(StoredSand { kind, content });
    if !matches!(
        kind,
        SandKind::Square
            | SandKind::Operation
            | SandKind::WorkTimer
            | SandKind::AccessControl
            | SandKind::Sync
    ) {
        world.entity_mut(sand).remove::<(Square, Outline)>();
    }
    sand
}
