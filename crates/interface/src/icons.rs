use crate::{
    container::BoxRoot,
    effect::{HoverEvents, SandHoveredOff, SandHoveredOn},
    sand::{InBox, Square},
    theme::{INK, PAPER, PURPLE, Typography},
    time_limit::{TimeLimit, TimeLimitSystems},
    wake::WakeSignal,
};
use bevy::{
    a11y::AccessibilityNode,
    asset::RenderAssetUsages,
    input_focus::{FocusCause, FocusGained, FocusLost},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Icon {
    Minus,
    Plus,
    Reset,
    Recenter,
    BringHere,
    Paintbrush,
    Close,
    Check,
    Workspaces,
    Palette,
    Store,
    Save,
    Pencil,
    Text,
    EditableText,
    Scroll,
    Grow,
    Circle,
    Square,
    Info,
    Bell,
    Pin,
    Forward,
    Backward,
    Back,
    Delete,
    General,
    Group,
    Ungroup,
}

#[derive(Component, Clone)]
#[require(
    bevy::ui_widgets::Button,
    bevy::input_focus::tab_navigation::TabIndex,
    IconStyle,
    Square,
    Tooltip,
    crate::castle::Castle,
    crate::castle::Pending
)]
pub struct IconButton {
    pub icon: Icon,
    pub label: String,
}

impl IconButton {
    pub fn new(icon: Icon, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
        }
    }
}

#[derive(Component, Clone, Copy)]
pub struct IconStyle {
    pub size: f32,
    pub padding: f32,
    pub color: Color,
    pub background: Color,
    pub border_color: Color,
    pub radius: f32,
}

impl Default for IconStyle {
    fn default() -> Self {
        Self {
            size: 24.0,
            padding: 7.0,
            color: INK,
            background: PAPER,
            border_color: PURPLE,
            radius: 4.0,
        }
    }
}

#[derive(Component, Default, Clone)]
#[require(HoverEvents, TooltipDuration)]
pub struct Tooltip(pub String);

#[derive(Component, Clone, Copy)]
pub struct TooltipDuration(pub std::time::Duration);

impl Default for TooltipDuration {
    fn default() -> Self {
        Self(TimeLimit::default().0)
    }
}

#[derive(Component)]
struct Glyph;

#[derive(Resource)]
struct IconAtlas {
    image: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
}

impl FromWorld for IconAtlas {
    fn from_world(world: &mut World) -> Self {
        let image = world.resource_mut::<Assets<Image>>().add(Image::new(
            Extent3d {
                width: 640,
                height: 768,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            include_bytes!(concat!(env!("OUT_DIR"), "/icons.rgba")).to_vec(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        ));
        let layout =
            world
                .resource_mut::<Assets<TextureAtlasLayout>>()
                .add(TextureAtlasLayout::from_grid(
                    UVec2::splat(128),
                    5,
                    6,
                    None,
                    None,
                ));
        Self { image, layout }
    }
}

#[derive(Resource, Default)]
struct Hints {
    hovered: Option<Entity>,
    keyboard: Option<Entity>,
    tip: Option<(Entity, Entity)>,
    generation: u64,
    shown: Option<(Entity, u64)>,
    positioning: bool,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SyncIcons;

pub struct IconPlugin;

impl Plugin for IconPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<Image>>()
            .init_resource::<crate::tokens::ThemeSettings>()
            .init_resource::<Assets<TextureAtlasLayout>>()
            .init_resource::<IconAtlas>()
            .init_resource::<Hints>()
            .add_systems(
                PostUpdate,
                sync.in_set(SyncIcons)
                    .after(crate::token_style::ApplyTokenStyles)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                hints
                    .after(bevy::ui::UiSystems::Layout)
                    .before(TimeLimitSystems),
            )
            .add_observer(
                |event: On<SandHoveredOn>,
                 labels: Query<(), With<Tooltip>>,
                 mut hints: ResMut<Hints>| {
                    if labels.contains(event.entity) && hints.hovered != Some(event.entity) {
                        hints.hovered = Some(event.entity);
                        hints.keyboard = None;
                        hints.generation = hints.generation.wrapping_add(1);
                    }
                },
            )
            .add_observer(|event: On<SandHoveredOff>, mut hints: ResMut<Hints>| {
                if hints.hovered == Some(event.entity) {
                    hints.hovered = None;
                    hints.generation = hints.generation.wrapping_add(1);
                }
            })
            .add_observer(
                |event: On<FocusGained>,
                 labels: Query<(), With<Tooltip>>,
                 visible: Option<Res<bevy::input_focus::InputFocusVisible>>,
                 mut hints: ResMut<Hints>| {
                    if event.entity != event.original_event_target() {
                        return;
                    }
                    hints.keyboard = (event.cause == FocusCause::Navigated
                        && visible.is_none_or(|visible| visible.0)
                        && labels.contains(event.entity))
                    .then_some(event.entity);
                    if hints.keyboard.is_some() {
                        hints.hovered = None;
                    }
                    hints.generation = hints.generation.wrapping_add(1);
                },
            )
            .add_observer(|event: On<FocusLost>, mut hints: ResMut<Hints>| {
                if hints.keyboard == Some(event.entity) {
                    hints.keyboard = None;
                    hints.generation = hints.generation.wrapping_add(1);
                }
            });
    }
}

fn sync(
    mut commands: Commands,
    settings: Res<crate::tokens::ThemeSettings>,
    atlas: Res<IconAtlas>,
    buttons: Query<(Entity, Ref<IconButton>, Ref<IconStyle>, Option<&Children>)>,
    glyphs: Query<(), With<Glyph>>,
    mut accessibility: Query<&mut AccessibilityNode>,
) {
    for (entity, button, style, children) in &buttons {
        if !button.is_changed() && !style.is_changed() && !settings.is_changed() {
            continue;
        }
        let border = settings
            .resolve(
                crate::tokens::Token::ControlBorder,
                None,
                &Default::default(),
            )
            .0
            .number();
        let extent = style.size + 2.0 * (style.padding + border);
        commands.entity(entity).insert((
            Node {
                width: px(extent),
                height: px(extent),
                padding: UiRect::all(px(style.padding)),
                border: UiRect::all(px(border)),
                border_radius: BorderRadius::all(px(style.radius)),
                flex_shrink: 0.0,
                align_self: AlignSelf::Start,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(style.background),
            BorderColor::all(style.border_color),
            Tooltip(button.label.clone()),
        ));
        commands.entity(entity).remove::<crate::castle::Pending>();
        if let Ok(mut node) = accessibility.get_mut(entity) {
            node.set_label(button.label.clone());
        }
        let image = ImageNode::from_atlas_image(
            atlas.image.clone(),
            TextureAtlas {
                layout: atlas.layout.clone(),
                index: button.icon as usize,
            },
        )
        .with_color(style.color);
        let node = Node {
            width: px(style.size),
            height: px(style.size),
            flex_shrink: 0.0,
            ..default()
        };
        if let Some(glyph) =
            children.and_then(|children| children.iter().find(|child| glyphs.contains(*child)))
        {
            commands.entity(glyph).insert((image, node));
        } else {
            commands.spawn((
                Glyph,
                crate::sand::ImageSand,
                image,
                node,
                Pickable::IGNORE,
                ChildOf(entity),
            ));
        }
    }
}

fn hints(world: &mut World) {
    let state = world.resource::<Hints>();
    let target = state
        .hovered
        .filter(|entity| world.get::<Tooltip>(*entity).is_some())
        .or(state.keyboard);
    let previous = state.tip;
    let generation = state.generation;
    let shown = state.shown;
    let content = target.and_then(|target| {
        let text = world.get::<Tooltip>(target)?.0.clone();
        let node = world.get::<ComputedNode>(target)?;
        if text.is_empty() || node.size().min_element() <= 0.0 {
            return None;
        }
        let size = node.size();
        let position = world.get::<UiGlobalTransform>(target)?.translation;
        let mut root = target;
        loop {
            if world.get::<Visibility>(root) == Some(&Visibility::Hidden)
                || world
                    .get::<Node>(root)
                    .is_some_and(|node| node.display == Display::None)
            {
                return None;
            }
            if world.get::<BoxRoot>(root).is_some() {
                break;
            }
            root = world.get::<ChildOf>(root)?.parent();
        }
        let viewport = world.get::<ComputedNode>(root)?;
        let scale = viewport.inverse_scale_factor();
        let point = (position - world.get::<UiGlobalTransform>(root)?.translation
            + viewport.size() * 0.5)
            * scale;
        Some((root, text, point, size * scale, viewport.size() * scale))
    });
    let Some((root, text, point, size, viewport)) = content else {
        world.resource_mut::<Hints>().shown = None;
        if let Some((_, tip)) = previous
            && let Some(mut node) = world.get_mut::<Node>(tip)
            && node.display != Display::None
        {
            node.display = Display::None;
            world
                .get_mut::<Visibility>(tip)
                .unwrap()
                .set_if_neq(Visibility::Hidden);
            ring(world);
        }
        return;
    };
    let tip = if let Some((owner, tip)) = previous
        && owner == root
        && world.get_entity(tip).is_ok()
    {
        tip
    } else {
        if let Some((_, tip)) = previous
            && world.get_entity(tip).is_ok()
        {
            world.despawn(tip);
        }
        let font = world.resource::<Typography>().text(14.0);
        let tip = world
            .spawn((
                Square,
                InBox(root),
                Visibility::Hidden,
                Text::new(""),
                font,
                crate::token_style::text(crate::tokens::Token::Ink),
                crate::token_style::background(crate::tokens::Token::Surface),
                crate::token_style::border(crate::tokens::Token::Accent),
                Pickable::IGNORE,
                GlobalZIndex(100),
                crate::inspection::InspectionExcluded,
                crate::token_metrics::WidthToken(crate::tokens::Token::TooltipWidth, true),
                crate::token_metrics::RadiusToken(crate::tokens::Token::TooltipRoundness),
                Node {
                    position_type: PositionType::Absolute,
                    max_width: px(280),
                    padding: UiRect::all(px(8)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        world.resource_mut::<Hints>().tip = Some((root, tip));
        tip
    };
    let target = target.unwrap();
    if shown != Some((target, generation)) {
        world
            .entity_mut(tip)
            .insert(Visibility::Hidden)
            .remove::<TimeLimit>();
        world.resource_mut::<Hints>().positioning = true;
        world.resource_mut::<Hints>().shown = Some((target, generation));
    } else if !world.resource::<Hints>().positioning
        && world.get::<Visibility>(tip) == Some(&Visibility::Hidden)
    {
        let mut node = world.get_mut::<Node>(tip).unwrap();
        if node.display != Display::None {
            node.display = Display::None;
            ring(world);
        }
        return;
    }
    let bounds = world
        .get::<ComputedNode>(tip)
        .map(|node| node.size() * node.inverse_scale_factor())
        .filter(|size| size.min_element() > 0.0)
        .unwrap_or(Vec2::new(200.0, 36.0));
    let left = px((point.x - bounds.x * 0.5).clamp(4.0, (viewport.x - bounds.x - 4.0).max(4.0)));
    let top = px(if point.y - size.y * 0.5 - bounds.y - 8.0 >= 4.0 {
        point.y - size.y * 0.5 - bounds.y - 8.0
    } else {
        (point.y + size.y * 0.5 + 8.0).min((viewport.y - bounds.y - 4.0).max(4.0))
    });
    let mut changed = false;
    let mut label = world.get_mut::<Text>(tip).unwrap();
    if label.0 != text {
        label.0 = text;
        changed = true;
    }
    let mut node = world.get_mut::<Node>(tip).unwrap();
    if node.display != Display::Flex || node.left != left || node.top != top {
        node.display = Display::Flex;
        node.left = left;
        node.top = top;
        changed = true;
    }
    if changed {
        world
            .entity_mut(tip)
            .insert(Visibility::Hidden)
            .remove::<TimeLimit>();
        world.resource_mut::<Hints>().positioning = true;
        ring(world);
    } else if world.resource::<Hints>().positioning {
        let duration = world
            .get::<TooltipDuration>(target)
            .copied()
            .unwrap_or_default();
        world
            .entity_mut(tip)
            .insert((Visibility::Inherited, TimeLimit(duration.0)));
        world.resource_mut::<Hints>().positioning = false;
        ring(world);
    }
}

fn ring(world: &World) {
    if let Some(wake) = world.get_resource::<WakeSignal>() {
        wake.ring();
    }
}

pub(crate) mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<Typography>()
            .add_plugins((crate::effect::EffectPlugin, IconPlugin));
        app
    }

    #[cfg_attr(test, test)]
    fn icons_share_one_atlas_and_style_changes_reuse_the_glyph() {
        let mut app = app();
        let first = app
            .world_mut()
            .spawn(IconButton::new(Icon::Plus, "New workspace"))
            .id();
        app.world_mut()
            .spawn(IconButton::new(Icon::Close, "Remove workspace"));
        app.update();
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        let glyph = app.world().get::<Children>(first).unwrap()[0];
        assert!(app.world().get::<crate::castle::Castle>(first).is_some());
        assert!(app.world().get::<Square>(first).is_some());
        assert!(app.world().get::<crate::sand::ImageSand>(glyph).is_some());
        let image = app.world().get::<ImageNode>(glyph).unwrap().image.clone();
        app.world_mut()
            .get_mut::<IconStyle>(first)
            .unwrap()
            .background = Color::WHITE;
        app.world_mut().get_mut::<IconStyle>(first).unwrap().color = Color::BLACK;
        app.world_mut().get_mut::<IconButton>(first).unwrap().label = "Add workspace".into();
        app.update();
        assert_eq!(app.world().get::<Children>(first).unwrap()[0], glyph);
        assert_eq!(app.world().get::<ImageNode>(glyph).unwrap().image, image);
        assert_eq!(
            app.world().get::<ImageNode>(glyph).unwrap().color,
            Color::BLACK
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(first).unwrap().0,
            Color::WHITE
        );
        assert_eq!(
            app.world().get::<Node>(first).unwrap().border.left,
            px(crate::sand::BUTTON_BORDER_WIDTH)
        );
        assert_eq!(
            app.world().get::<Tooltip>(first).unwrap().0,
            "Add workspace"
        );
        assert_eq!(
            app.world().get::<AccessibilityNode>(first).unwrap().label(),
            Some("Add workspace")
        );
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<Entity, With<Glyph>>()
                .iter(world)
                .count(),
            2
        );
    }

    #[cfg_attr(test, test)]
    fn atlas_contains_every_icon_and_straight_alpha_for_tinting() {
        let pixels = include_bytes!(concat!(env!("OUT_DIR"), "/icons.rgba"));
        for index in 0..=Icon::Ungroup as usize {
            let mut ink = false;
            for y in index / 5 * 128..(index / 5 + 1) * 128 {
                for x in index % 5 * 128..(index % 5 + 1) * 128 {
                    let offset = (y * 640 + x) * 4;
                    assert_eq!(&pixels[offset..offset + 3], &[255; 3]);
                    ink |= pixels[offset + 3] > 0;
                }
            }
            assert!(ink, "empty icon tile {index}");
        }
    }

    #[cfg_attr(test, test)]
    fn keyboard_labels_show_and_hide_without_intercepting_pointer_input() {
        let mut app = app();
        let root = app
            .world_mut()
            .spawn((
                BoxRoot,
                UiGlobalTransform::default(),
                ComputedNode {
                    size: Vec2::new(800.0, 640.0),
                    ..default()
                },
            ))
            .id();
        let button = app
            .world_mut()
            .spawn((
                IconButton::new(Icon::Paintbrush, "Edit mode"),
                ChildOf(root),
                ComputedNode {
                    size: Vec2::splat(40.0),
                    ..default()
                },
                UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
                    300.0, 250.0,
                ))),
            ))
            .id();
        let hovered = app
            .world_mut()
            .spawn((
                Tooltip("Another Sand".into()),
                ChildOf(root),
                ComputedNode {
                    size: Vec2::splat(40.0),
                    ..default()
                },
            ))
            .id();
        app.update();
        app.world_mut().trigger(SandHoveredOn { entity: hovered });
        app.update();
        let tip = app.world().resource::<Hints>().tip.unwrap().1;
        assert_eq!(app.world().get::<Text>(tip).unwrap().0, "Another Sand");
        app.world_mut().trigger(FocusGained {
            entity: button,
            cause: FocusCause::Navigated,
        });
        app.update();
        let tip = app.world().resource::<Hints>().tip.unwrap().1;
        assert_eq!(app.world().get::<Text>(tip).unwrap().0, "Edit mode");
        assert_eq!(app.world().get::<Node>(tip).unwrap().display, Display::Flex);
        assert!(!app.world().get::<Pickable>(tip).unwrap().should_block_lower);
        app.world_mut().trigger(FocusLost { entity: button });
        app.update();
        assert_eq!(app.world().get::<Node>(tip).unwrap().display, Display::None);
        app.insert_resource(bevy::input_focus::InputFocusVisible(false));
        app.world_mut().trigger(FocusGained {
            entity: button,
            cause: FocusCause::Navigated,
        });
        app.update();
        assert_eq!(app.world().get::<Node>(tip).unwrap().display, Display::None);
    }

    #[cfg_attr(test, test)]
    fn hover_events_show_a_square_timeout_without_reopening_and_reenter_reuses_it() {
        let mut app = app();
        let root = app
            .world_mut()
            .spawn((
                BoxRoot,
                ComputedNode {
                    size: Vec2::new(800.0, 640.0),
                    ..default()
                },
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                Tooltip("A Sand label".into()),
                TooltipDuration(std::time::Duration::ZERO),
                ChildOf(root),
                ComputedNode {
                    size: Vec2::splat(40.0),
                    ..default()
                },
            ))
            .id();
        app.update();
        app.world_mut().trigger(SandHoveredOn { entity: source });
        app.update();
        let tip = app.world().resource::<Hints>().tip.unwrap().1;
        assert!(app.world().get::<Square>(tip).is_some());
        assert_eq!(app.world().get::<InBox>(tip).unwrap().0, root);
        assert_eq!(
            *app.world().get::<Visibility>(tip).unwrap(),
            Visibility::Hidden
        );
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(app.world().get::<Node>(tip).unwrap().display, Display::None);
        app.world_mut().trigger(SandHoveredOff { entity: source });
        app.update();
        app.world_mut()
            .entity_mut(source)
            .insert(TooltipDuration::default());
        app.world_mut().trigger(SandHoveredOn { entity: source });
        app.update();
        assert_eq!(app.world().resource::<Hints>().tip.unwrap().1, tip);
        assert_eq!(
            *app.world().get::<Visibility>(tip).unwrap(),
            Visibility::Hidden
        );
        let position = app.world().get::<Node>(tip).unwrap().clone();
        app.update();
        assert_eq!(app.world().get::<Node>(tip).unwrap().left, position.left);
        assert_eq!(app.world().get::<Node>(tip).unwrap().top, position.top);
        assert_eq!(
            *app.world().get::<Visibility>(tip).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(app.world().get::<Node>(tip).unwrap().display, Display::Flex);
        app.world_mut().trigger(SandHoveredOff { entity: source });
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(tip).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(app.world().get::<Node>(tip).unwrap().display, Display::None);
    }

    crate::laboratory_cases! {
        icons_share_one_atlas_and_style_changes_reuse_the_glyph,
        atlas_contains_every_icon_and_straight_alpha_for_tinting,
        keyboard_labels_show_and_hide_without_intercepting_pointer_input,
        hover_events_show_a_square_timeout_without_reopening_and_reenter_reuses_it,
    }
}
