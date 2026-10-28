use crate::native::{
    area::InfluenceArea, canvas::CanvasItem, protein_area::Config, workspace::WorkspaceMember,
};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Component)]
pub(crate) struct Frame {
    pub viewport: Entity,
    pub area: Entity,
    pub header: Entity,
    pub status: Entity,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct Viewport {
    pub center: DVec2,
    pub zoom: f64,
}

#[derive(Component)]
pub(crate) struct HiddenCard;

pub(crate) struct FeedPlugin;

impl Plugin for FeedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            project
                .after(crate::native::canvas::project_canvas)
                .before(bevy::ui::UiSystems::Content),
        );
    }
}

pub(crate) fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    size: Vec2,
    title: &str,
    config: Config,
) -> Entity {
    let owner = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(crate::native::credits::ATTRIBUTIONS),
            CanvasItem { position, size },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                row_gap: px(6),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
        ))
        .id();
    let header = world
        .spawn((
            Node {
                column_gap: px(6),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    crate::native::edit_mode::label(world, header, title, 18.0);
    button(world, header, owner, "Protein", OpenQuery);
    let status = crate::native::edit_mode::label(world, owner, "Connecting", 12.0);
    let body = world
        .spawn((
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::clip(),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    let viewport = world
        .spawn((
            Viewport {
                center: DVec2::ZERO,
                zoom: 1.0,
            },
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                height: percent(100),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::CanvasBackground),
            ChildOf(body),
        ))
        .id();
    let mut area = InfluenceArea::new(
        crate::native::area::AreaShape::Square,
        DVec2::ZERO,
        DVec2::splat(20_000.0),
    );
    area.name = title.into();
    area.protein = Some(config);
    let area = world
        .spawn((
            area,
            ChildOf(viewport),
            WorkspaceMember(workspace),
            Node {
                display: Display::None,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    world.entity_mut(owner).insert(Frame {
        viewport,
        area,
        header,
        status,
    });
    owner
}

pub(crate) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    title: &str,
    action: impl crate::native::actions::Action,
) -> Entity {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::sand::Square,
            Node {
                padding: UiRect::axes(px(8), px(4)),
                min_height: px(30),
                flex_shrink: 0.0,
                ..default()
            },
            crate::native::actions::ActionButton::new(owner, crate::native::actions![action]),
            crate::native::icons::Tooltip(title.into()),
            ChildOf(parent),
        ))
        .id();
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(title);
    }
    crate::native::edit_mode::label(world, entity, title, 14.0);
    entity
}

#[derive(Clone)]
struct OpenQuery;
impl crate::native::actions::Action for OpenQuery {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner) {
            return;
        }
        let Some(frame) = world.get::<Frame>(owner) else {
            return;
        };
        let area = frame.area;
        let Some(config) = world
            .get::<InfluenceArea>(area)
            .and_then(|area| area.protein.clone())
        else {
            return;
        };
        let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
            return;
        };
        let workspace = world
            .get::<WorkspaceMember>(owner)
            .map_or(1, |member| member.0);
        let existing: Vec<_> = world
            .query::<(Entity, &crate::native::protein_area::QueryEditor)>()
            .iter(world)
            .filter(|(_, link)| link.0 == area)
            .map(|(entity, _)| entity)
            .collect();
        for entity in existing {
            world.despawn(entity);
        }
        let item = *world.get::<CanvasItem>(owner).unwrap();
        let editor = crate::native::protein_castle::spawn(
            world,
            root,
            workspace,
            item.position + DVec2::new(f64::from(item.size.x) + 40.0, 0.0),
            config.draft,
        );
        world
            .entity_mut(editor)
            .insert(crate::native::protein_area::QueryEditor(area));
        crate::native::protein_castle::refresh_editor(world, editor);
    }
}

pub(crate) fn cards(world: &mut World, area: Entity) -> Vec<Entity> {
    world.query_filtered::<(Entity, &crate::native::protein_area::RecordBinding), With<crate::native::full_record::RecordCard>>()
        .iter(world).filter(|(_, binding)| binding.area == area).map(|(entity, _)| entity).collect()
}

pub(crate) fn place(world: &mut World, entity: Entity, size: Vec2) -> bool {
    if !world
        .get::<ChildOf>(entity)
        .is_some_and(|parent| world.get::<Viewport>(parent.parent()).is_some())
    {
        return false;
    }
    if let Some(mut item) = world.get_mut::<CanvasItem>(entity) {
        item.size = size;
    }
    world
        .entity_mut(entity)
        .remove::<crate::native::protein_area::placement::Pending>()
        .insert((Visibility::Visible, ZIndex(1)));
    true
}

fn project(
    views: Query<(&Viewport, &ComputedNode)>,
    mut cards: Query<(
        &ChildOf,
        &CanvasItem,
        &mut Node,
        &mut UiTransform,
        Option<&HiddenCard>,
    )>,
) {
    for (parent, item, mut node, mut transform, hidden) in &mut cards {
        let Ok((view, computed)) = views.get(parent.parent()) else {
            continue;
        };
        if hidden.is_some() {
            if node.display != Display::None {
                node.display = Display::None;
            }
            continue;
        }
        let viewport = computed.size() * computed.inverse_scale_factor();
        let zoom = view.zoom as f32;
        let center = (item.position - view.center).as_vec2() * zoom + viewport * 0.5;
        node.display = Display::Flex;
        node.position_type = PositionType::Absolute;
        node.left = px(center.x - item.size.x * 0.5);
        node.top = px(center.y - item.size.y * 0.5);
        node.width = px(item.size.x);
        node.height = px(item.size.y);
        transform.scale = Vec2::splat(zoom);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub workspace: u64,
    pub config: Config,
    pub position: [f64; 2],
    pub size: [f32; 2],
    pub center: [f64; 2],
    pub zoom: f64,
    pub placement: crate::native::sand_placement::Placement,
    pub tokens: crate::native::tokens::TokenOverrides,
}

crate::native::sand_placement::scratch::saved!(Saved);

impl Saved {
    pub fn valid(&self) -> bool {
        self.config.valid()
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() >= 80.0
            && Vec2::from_array(self.size).max_element() <= 100_000.0
            && DVec2::from_array(self.center).is_finite()
            && self.zoom.is_finite()
            && (0.08..=3.0).contains(&self.zoom)
            && self.placement.valid()
            && self.tokens.validate()
    }

    pub fn capture(world: &World, owner: Entity) -> Option<Self> {
        let frame = world.get::<Frame>(owner)?;
        let config = world.get::<InfluenceArea>(frame.area)?.protein.clone()?;
        let item = world.get::<CanvasItem>(owner)?;
        let view = world.get::<Viewport>(frame.viewport)?;
        Some(Self {
            workspace: world.get::<WorkspaceMember>(owner)?.0,
            config,
            position: item.position.to_array(),
            size: item.size.to_array(),
            center: view.center.to_array(),
            zoom: view.zoom,
            placement: crate::native::sand_placement::Placement::capture(world, owner),
            tokens: crate::native::token_style::overrides(world, owner),
        })
    }

    pub fn apply(&self, world: &mut World, owner: Entity) {
        let frame = world.get::<Frame>(owner).unwrap();
        let (area, viewport) = (frame.area, frame.viewport);
        world.get_mut::<InfluenceArea>(area).unwrap().protein = Some(self.config.clone());
        world.entity_mut(viewport).insert(Viewport {
            center: DVec2::from_array(self.center),
            zoom: self.zoom,
        });
        world.get_mut::<CanvasItem>(owner).unwrap().size = Vec2::from_array(self.size);
        self.placement.clone().restore(world, owner);
        world.entity_mut(owner).insert((
            self.tokens.clone(),
            crate::native::token_style::AppliedSize(Vec2::from_array(self.size)),
        ));
    }
}
