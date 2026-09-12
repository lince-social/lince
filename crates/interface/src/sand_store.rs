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
}

impl SandKind {
    pub const ALL: [Self; 3] = [Self::Square, Self::Text, Self::EditableText];
    pub fn name(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Text => "Plain text",
            Self::EditableText => "Editable text",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Square => "An empty space to compose with.",
            Self::Text => "A simple text label.",
            Self::EditableText => "A note you can write in.",
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
pub(crate) struct Dimensions(Entity, usize);

fn dimensions(size: Vec2, areas: usize) -> String {
    format!(
        "{:.0} × {:.0} · {} text area{}",
        size.x,
        size.y,
        areas,
        if areas == 1 { "" } else { "s" }
    )
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
                node.border = UiRect::all(px(if sand.kind == SandKind::Square {
                    crate::sand::BUTTON_BORDER_WIDTH / scale
                } else {
                    0.0
                }));
                transform.scale = Vec2::splat(scale);
            }
        }
    }
    for (source, mut text) in &mut labels {
        if let Ok((item, _)) = items.get(source.0) {
            let value = dimensions(item.size, source.1);
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
    use crate::{
        edit_mode::{EditAction, control, label},
        theme::Typography,
    };
    world.init_resource::<StoreFont>();
    let style_kind = match kind {
        SandKind::Square => crate::tokens::SandStyleKind::Square,
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
        .unwrap_or(Vec2::new(width, height));
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
                padding: UiRect::axes(px(0), px(8)),
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
    let scale = (68.0 / size.x).min(60.0 / size.y).min(1.0);
    let miniature = world
        .spawn((
            Square,
            Outline {
                width: px(0),
                ..default()
            },
            ChildOf(preview),
            Pickable::IGNORE,
            PreviewKind(style_kind),
            crate::token_style::background(crate::tokens::Token::SandBackground),
            crate::token_style::border(crate::tokens::Token::SandBorder),
            UiTransform::from_scale(Vec2::splat(scale)),
            Node {
                position_type: PositionType::Absolute,
                left: px((72.0 - size.x) * 0.5),
                top: px((64.0 - size.y) * 0.5),
                width: px(size.x),
                height: px(size.y),
                border: UiRect::all(px(if kind == SandKind::Square {
                    crate::sand::BUTTON_BORDER_WIDTH / scale
                } else {
                    0.0
                })),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    if let Some((source, _)) = existing {
        world.entity_mut(miniature).insert(PreviewSource(source));
    }
    if kind != SandKind::Square {
        world.entity_mut(miniature).remove::<(Square, Outline)>();
    }
    for saved in &texts {
        let font = world.resource::<Typography>().text(22.0);
        world.spawn((
            Text::new(saved.text.chars().take(160).collect::<String>()),
            font,
            crate::token_style::text(crate::tokens::Token::Ink),
            Pickable::IGNORE,
            saved.area.tokens.clone(),
            ChildOf(miniature),
            Node {
                position_type: PositionType::Absolute,
                left: px(saved.area.offset[0]),
                top: px(saved.area.offset[1]),
                width: px(saved.area.size[0]),
                height: px(saved.area.size[1]),
                overflow: Overflow::clip(),
                ..default()
            },
        ));
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
    let measurement = label(world, metadata, &dimensions(size, texts.len()), 12.0);
    if let Some((source, _)) = existing {
        world
            .entity_mut(measurement)
            .insert(Dimensions(source, texts.len()));
    }
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
    if let Some((entity, index)) = existing {
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
    } else {
        label(world, buttons, "+", 22.0);
    }
}

pub fn spawn_sand(
    world: &mut World,
    root: Entity,
    workspace: u64,
    kind: SandKind,
    text: &str,
    position: DVec2,
) -> Entity {
    let sand = world
        .spawn((
            Square,
            InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            SandCredits(crate::credits::ATTRIBUTIONS),
            CanvasItem {
                position,
                size: Vec2::new(248.0, 184.0),
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
        SandKind::Square => None,
        SandKind::Text | SandKind::EditableText => Some(sand_text::spawn(
            world,
            sand,
            SavedText {
                area: SandText::new(kind == SandKind::EditableText),
                text: text.into(),
            },
        )),
    };
    world.entity_mut(sand).insert(StoredSand { kind, content });
    if kind != SandKind::Square {
        world.entity_mut(sand).remove::<(Square, Outline)>();
    }
    sand
}
