use crate::i18n::Token as Tr;
pub(crate) mod preview;

use crate::native::{
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
    Visibility,
    Trust,
    HandCamera,
    Video,
    Sync,
    #[cfg(feature = "stuff")]
    Freedoom,
    Terminal,
    Configuration,
    Organ,
    Todo,
    Ontology,
}

impl SandKind {
    pub const ALL: [Self; 16 + cfg!(feature = "stuff") as usize] = [
        Self::Square,
        Self::Text,
        Self::EditableText,
        Self::Operation,
        Self::WorkTimer,
        Self::AccessControl,
        Self::Visibility,
        Self::Trust,
        Self::HandCamera,
        Self::Video,
        Self::Sync,
        #[cfg(feature = "stuff")]
        Self::Freedoom,
        Self::Terminal,
        Self::Configuration,
        Self::Organ,
        Self::Todo,
        Self::Ontology,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Square => Tr::Square.text(),
            Self::Text => Tr::PlainText.text(),
            Self::EditableText => Tr::EditableText.text(),
            Self::Operation => Tr::Operation.text(),
            Self::WorkTimer => Tr::TimeCastle.text(),
            Self::AccessControl => Tr::AccessControl.text(),
            Self::Visibility => Tr::Visibility.text(),
            Self::Trust => Tr::Trust.text(),
            Self::HandCamera => Tr::HandCamera.text(),
            Self::Video => Tr::Video.text(),
            Self::Sync => Tr::Sync.text(),
            #[cfg(feature = "stuff")]
            Self::Freedoom => "Freedoom",
            Self::Terminal => Tr::Terminal.text(),
            Self::Configuration => Tr::Configuration.text(),
            Self::Organ => Tr::Organ.text(),
            Self::Todo => Tr::Todo.text(),
            Self::Ontology => Tr::Ontology.text(),
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Square => Tr::EmptySpaceHelp.text(),
            Self::Text => Tr::PlainTextHelp.text(),
            Self::EditableText => Tr::EditableTextHelp.text(),
            Self::Operation => Tr::OperationHelp.text(),
            Self::WorkTimer => Tr::TimeCastleHelp.text(),
            Self::AccessControl => Tr::AccessControlHelp.text(),
            Self::Visibility => Tr::VisibilityHelp.text(),
            Self::Trust => Tr::TrustHelp.text(),
            Self::HandCamera => Tr::HandCameraHelp.text(),
            Self::Video => Tr::VideoHelp.text(),
            Self::Sync => Tr::SyncHelp.text(),
            #[cfg(feature = "stuff")]
            Self::Freedoom => "Play Freedoom locally with keyboard controls.",
            Self::Terminal => Tr::TerminalHelp.text(),
            Self::Configuration => Tr::ConfigurationHelp.text(),
            Self::Organ => Tr::OrganHelp.text(),
            Self::Ontology => Tr::OntologyHelp.text(),
            Self::Todo => Tr::TodoHelp.text(),
        }
    }

    fn is_panel(self) -> bool {
        match self {
            #[cfg(feature = "stuff")]
            Self::Freedoom => true,
            Self::AccessControl
            | Self::Visibility
            | Self::Trust
            | Self::HandCamera
            | Self::Video
            | Self::Sync
            | Self::Terminal
            | Self::Organ
            | Self::Configuration
            | Self::Ontology
            | Self::Todo => true,
            _ => false,
        }
    }
}

#[derive(Component)]
pub struct StoredSand {
    pub kind: SandKind,
    pub content: Option<Entity>,
}

#[derive(Component)]
pub struct SandCredits(pub &'static [crate::native::credits::Attribution]);

#[derive(Resource, Clone)]
struct StoreFont(Handle<Font>);

impl FromWorld for StoreFont {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource_mut::<Assets<Font>>().add(Font::from_bytes(
            include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Bold.ttf").to_vec(),
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
pub(crate) struct PreviewKind(pub crate::native::tokens::SandStyleKind);

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
                    node.border = UiRect::all(px(crate::native::sand::BUTTON_BORDER_WIDTH / scale));
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
    use crate::native::edit_mode::{EditAction, control, label};
    world.init_resource::<StoreFont>();
    let style_kind = match kind {
        #[cfg(feature = "stuff")]
        SandKind::Freedoom => crate::native::tokens::SandStyleKind::Square,
        SandKind::Square
        | SandKind::Operation
        | SandKind::AccessControl
        | SandKind::Visibility
        | SandKind::Trust
        | SandKind::HandCamera
        | SandKind::Video
        | SandKind::Sync
        | SandKind::Terminal
        | SandKind::Organ
        | SandKind::Configuration
        | SandKind::Ontology
        | SandKind::Todo => crate::native::tokens::SandStyleKind::Square,
        SandKind::WorkTimer => crate::native::tokens::SandStyleKind::TimeCastle,
        SandKind::Text => crate::native::tokens::SandStyleKind::Text,
        SandKind::EditableText => crate::native::tokens::SandStyleKind::EditableText,
    };
    let settings = world.resource::<crate::native::tokens::ThemeSettings>();
    let width = settings
        .resolve(
            crate::native::tokens::Token::Width,
            Some(style_kind),
            &Default::default(),
        )
        .0
        .number();
    let height = settings
        .resolve(
            crate::native::tokens::Token::Height,
            Some(style_kind),
            &Default::default(),
        )
        .0
        .number();
    let size = existing
        .and_then(|(entity, _)| world.get::<CanvasItem>(entity).map(|item| item.size))
        .unwrap_or(if kind == SandKind::Operation {
            crate::native::operation::SIZE
        } else if kind == SandKind::Terminal {
            Vec2::new(820.0, 540.0)
        } else if kind.is_panel() {
            Vec2::new(520.0, 540.0)
        } else {
            Vec2::new(width, height)
        });
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
            crate::native::castle::Castle,
            StoreEntry(kind),
            StoreComponent {
                title: kind.name().into(),
                description: kind.description().into(),
                size,
            },
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
            crate::native::sand::button(0),
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
            crate::native::edit_mode::EditControl {
                root,
                action: EditAction::AddSand(kind),
            },
            crate::native::actions::ActionButton::new(
                root,
                crate::native::actions![EditAction::AddSand(kind)],
            ),
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
    if kind == SandKind::Operation {
        let input = scene.get::<StoredSand>(source).unwrap().content.unwrap();
        scene
            .get_mut::<bevy::text::EditableText>(input)
            .unwrap()
            .editor
            .set_text("@record");
    }

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
        crate::native::work_timer::populate(
            &mut scene,
            source,
            None,
            &serde_json::Value::Null,
            input,
        );
        if let Some((viewport, image)) = crate::native::time_castle::preview(&scene, source) {
            scene.init_resource::<Assets<Image>>();
            let handle = scene.resource_mut::<Assets<Image>>().add(image);
            scene.entity_mut(viewport).insert(ImageNode::new(handle));
        }
    }
    let miniature = preview::snapshot(&scene, source, world, preview);
    preview::fit(world, miniature, size);
    world.entity_mut(miniature).insert(PreviewKind(style_kind));
    if kind == SandKind::Square {
        let scale = (68.0 / size.x).min(60.0 / size.y).min(1.0);
        world.get_mut::<Node>(miniature).unwrap().border =
            UiRect::all(px(crate::native::sand::BUTTON_BORDER_WIDTH / scale));
        world
            .entity_mut(miniature)
            .insert(preview::PreviewBorder(UiRect::all(px(
                crate::native::sand::BUTTON_BORDER_WIDTH,
            ))));
        world.entity_mut(miniature).insert((
            crate::native::token_style::background(crate::native::tokens::Token::SandBackground),
            crate::native::token_style::border(crate::native::tokens::Token::SandBorder),
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
        crate::native::token_style::text(crate::native::tokens::Token::Ink),
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

#[derive(Component, Clone)]
pub struct StoreComponent {
    pub title: String,
    pub description: String,
    pub size: Vec2,
}

pub(crate) fn castle_entry(
    world: &mut World,
    root: Entity,
    parent: Entity,
    title: &str,
    description: &str,
    action: impl crate::native::actions::Action,
    populate: impl FnOnce(&mut World, Entity) -> Entity,
) -> Entity {
    let (mut scene, scene_root) = preview::scene(world);
    let source = populate(&mut scene, scene_root);
    let size = scene
        .get::<CanvasItem>(source)
        .map_or(Vec2::new(560.0, 720.0), |item| item.size);
    let row = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::button(0),
            crate::native::actions::ActionButton::new(root, crate::native::actions![action]),
            StoreComponent {
                title: title.into(),
                description: description.into(),
                size,
            },
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
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
        crate::native::token_style::text(crate::native::tokens::Token::Ink),
        ChildOf(metadata),
    ));
    crate::native::edit_mode::label(world, metadata, description, 13.0);
    crate::native::edit_mode::label(world, metadata, &dimensions(size), 12.0);
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
    spawn_with_source(world, root, workspace, kind, text, position, None)
}

pub(crate) fn spawn_scoped_sand(
    world: &mut World,
    root: Entity,
    workspace: u64,
    kind: SandKind,
    position: DVec2,
    source: String,
) -> Entity {
    spawn_with_source(world, root, workspace, kind, "", position, Some(source))
}

fn spawn_with_source(
    world: &mut World,
    root: Entity,
    workspace: u64,
    kind: SandKind,
    text: &str,
    position: DVec2,
    source: Option<String>,
) -> Entity {
    let elevation = world
        .get::<crate::native::topology::view::View>(root)
        .map_or(0.0, |view| view.plane);
    let sand = world
        .spawn((
            Square,
            InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::native::topology::Spatial {
                elevation,
                ..default()
            },
            SandCredits(crate::native::credits::ATTRIBUTIONS),
            CanvasItem {
                position,
                size: if kind == SandKind::Operation {
                    crate::native::operation::SIZE
                } else if kind == SandKind::Terminal {
                    Vec2::new(820.0, 540.0)
                } else if kind.is_panel() {
                    Vec2::new(520.0, 540.0)
                } else if kind == SandKind::WorkTimer {
                    Vec2::splat(420.0)
                } else {
                    Vec2::new(248.0, 184.0)
                },
            },
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(if kind == SandKind::Square {
                crate::native::theme::PAPER
            } else {
                Color::NONE
            }),
        ))
        .id();
    if crate::native::laboratory::temporary(world, root, workspace) {
        world
            .entity_mut(sand)
            .insert(crate::native::laboratory::LaboratoryOwner);
    }
    if let Some(source) = source {
        world
            .entity_mut(sand)
            .insert(crate::native::practice_cells::PracticeSource(source));
    }
    let content = match kind {
        SandKind::Operation => Some(crate::native::operation::populate(world, root, sand)),
        SandKind::AccessControl => Some(crate::native::access_control::populate(world, root, sand)),
        SandKind::Trust => Some(crate::native::trust_castle::populate(world, root, sand)),
        SandKind::Visibility => Some(crate::native::visibility_castle::populate(
            world, root, sand,
        )),
        SandKind::HandCamera => Some(crate::native::hand_camera::populate(world, root, sand)),
        SandKind::Video => Some(crate::native::video_castle::populate(world, root, sand)),
        SandKind::Sync => Some(crate::native::sync_castle::populate(world, root, sand)),
        #[cfg(feature = "stuff")]
        SandKind::Freedoom => Some(crate::native::freedoom::populate(world, root, sand)),
        SandKind::Terminal => Some(crate::native::terminal::populate(world, root, sand)),
        SandKind::Organ => Some(crate::native::organ_castle::populate(world, root, sand)),
        SandKind::Configuration => Some(crate::native::configuration::populate(world, root, sand)),
        SandKind::Ontology => Some(crate::native::ontology::populate(world, root, sand)),
        SandKind::Todo => Some(crate::native::todo::populate(world, root, sand)),
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
    if !kind.is_panel()
        && !matches!(
            kind,
            SandKind::Square | SandKind::Operation | SandKind::WorkTimer
        )
    {
        world.entity_mut(sand).remove::<(Square, Outline)>();
    }
    sand
}
