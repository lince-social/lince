use crate::{canvas::CanvasItem, sand::text_editor, theme::Typography};
use bevy::{
    picking::events::Scroll,
    prelude::*,
    text::{EditableText, TextLayoutInfo},
    ui::widget::TextScroll,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextOverflow {
    #[default]
    Scroll,
    Grow,
}

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct SandText {
    #[serde(default)]
    pub tokens: crate::tokens::TokenOverrides,
    pub editable: bool,
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub overflow: TextOverflow,
}

impl SandText {
    pub fn new(editable: bool) -> Self {
        Self {
            tokens: Default::default(),
            editable,
            offset: [16.0, 16.0],
            size: [216.0, 152.0],
            overflow: TextOverflow::Scroll,
        }
    }

    pub fn validate(&self) -> bool {
        self.tokens.validate()
            && self
                .offset
                .iter()
                .all(|v| v.is_finite() && (0.0..=100_000.0).contains(v))
            && self
                .size
                .iter()
                .all(|v| v.is_finite() && (24.0..=100_000.0).contains(v))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SavedText {
    pub area: SandText,
    pub text: String,
}

impl SavedText {
    pub fn validate(&self) -> bool {
        self.area.validate() && self.text.chars().count() <= 4096
    }
}

pub fn value(world: &World, entity: Entity) -> String {
    world
        .get::<EditableText>(entity)
        .map(|text| text.value().to_string())
        .or_else(|| world.get::<Text>(entity).map(|text| text.0.clone()))
        .unwrap_or_default()
}

pub fn snapshot(world: &World, sand: Entity) -> Vec<SavedText> {
    world
        .get::<Children>(sand)
        .into_iter()
        .flatten()
        .filter_map(|entity| {
            world.get::<SandText>(*entity).map(|area| SavedText {
                area: area.clone(),
                text: value(world, *entity),
            })
        })
        .collect()
}

pub fn spawn(world: &mut World, sand: Entity, saved: SavedText) -> Entity {
    let entity = if saved.area.editable {
        let bundle = text_editor(&saved.text, world.resource::<Typography>(), 0);
        let entity = world.spawn(bundle).id();
        let mut editor = world.get_mut::<EditableText>(entity).unwrap();
        editor.max_characters = Some(4096);
        editor.visible_lines = None;
        entity
    } else {
        let font = world.resource::<Typography>().text(22.0);
        world
            .spawn((
                Text::new(saved.text),
                font,
                crate::token_style::text(crate::tokens::Token::Ink),
            ))
            .id()
    };
    let area = saved.area;
    world
        .entity_mut(entity)
        .insert((
            Node {
                position_type: PositionType::Absolute,
                left: px(area.offset[0]),
                top: px(area.offset[1]),
                width: px(area.size[0]),
                height: px(area.size[1]),
                overflow: Overflow::clip(),
                flex_shrink: 0.0,
                ..default()
            },
            area,
            ChildOf(sand),
            crate::token_style::OutlineToken(crate::tokens::Token::Accent),
            Outline {
                width: px(0),
                offset: px(0),
                color: crate::theme::PURPLE,
            },
        ))
        .observe(scroll);
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label("Editable text in Sand");
    }
    fit_sand(world, sand);
    entity
}

pub fn fit_sand(world: &mut World, sand: Entity) {
    let required = snapshot(world, sand).iter().fold(Vec2::ZERO, |size, text| {
        size.max(
            Vec2::from_array(text.area.offset)
                + Vec2::from_array(text.area.size)
                + Vec2::splat(16.0),
        )
    });
    if let Some(mut item) = world.get_mut::<CanvasItem>(sand) {
        let size = item.size.max(required);
        let delta = size - item.size;
        if delta != Vec2::ZERO {
            item.position += delta.as_dvec2() * 0.5;
            item.size = size;
        }
    }
}

fn scroll(
    mut event: On<Pointer<Scroll>>,
    mut texts: Query<(&SandText, &ComputedNode, &TextLayoutInfo, &mut TextScroll)>,
) {
    let Ok((area, node, layout, mut scroll)) = texts.get_mut(event.entity) else {
        return;
    };
    if area.overflow != TextOverflow::Scroll {
        return;
    }
    let multiplier = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
        24.0
    } else {
        1.0
    };
    let maximum = (layout.size.y - node.content_box().size().y).max(0.0);
    scroll.0.y =
        (scroll.0.y - event.y * multiplier / node.inverse_scale_factor()).clamp(0.0, maximum);
    event.propagate(false);
}

pub struct SandTextPlugin;

impl Plugin for SandTextPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (grow, boundaries).after(bevy::ui::UiSystems::PostLayout),
        );
    }
}

fn boundaries(
    modes: Query<&crate::edit_mode::EditMode>,
    parents: Query<&ChildOf>,
    mut texts: Query<(&ChildOf, &mut Outline), With<SandText>>,
) {
    for (parent, mut outline) in &mut texts {
        let enabled = parents
            .get(parent.parent())
            .ok()
            .and_then(|root| modes.get(root.parent()).ok())
            .is_some_and(|mode| mode.enabled);
        let width = px(if enabled { 1 } else { 0 });
        if outline.width != width {
            outline.width = width;
        }
    }
}

fn grow(
    mut texts: Query<(
        &SandText,
        &ChildOf,
        &TextLayoutInfo,
        &ComputedNode,
        &mut Node,
        Option<&mut TextScroll>,
    )>,
    mut sands: Query<&mut CanvasItem>,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    let mut changed = false;
    for (area, parent, layout, computed, mut node, scroll) in &mut texts {
        if area.overflow != TextOverflow::Grow || !area.editable {
            continue;
        }
        let height = (layout.size.y * computed.inverse_scale_factor())
            .ceil()
            .max(area.size[1]);
        if !height.is_finite() {
            continue;
        }
        if node.height != px(height) {
            node.height = px(height);
            changed = true;
        }
        if let Some(mut scroll) = scroll {
            scroll.set_if_neq(TextScroll(Vec2::ZERO));
        }
        if let Ok(mut sand) = sands.get_mut(parent.parent()) {
            let bottom = area.offset[1] + height + 16.0;
            if bottom > sand.size.y {
                let delta = bottom - sand.size.y;
                sand.size.y = bottom;
                sand.position.y += f64::from(delta) * 0.5;
                changed = true;
            }
        }
    }
    if changed && let Some(wake) = wake {
        wake.ring();
    }
}
