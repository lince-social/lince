use super::{
    Castle, EditMode, ImageSand, Inspection, InspectionExcluded, Mark, Preview, Setting, Square,
    bounds, drawing, excluded, family, hidden, visit_connections,
};
use bevy::{ecs::query::QueryState, prelude::*, text::EditableText};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
struct Member {
    hidden: bool,
    castle_depth: usize,
}

#[derive(Default)]
struct Scene {
    members: HashMap<Entity, Member>,
    entities: Vec<Entity>,
    stack: Vec<(Entity, Member)>,
    live_workspaces: HashMap<u64, bool>,
    bounds: HashMap<Entity, Option<Rect>>,
    related: HashSet<Entity>,
    contoured: HashSet<Entity>,
    previews: HashSet<Entity>,
    preview: Preview,
}

impl Scene {
    fn prepare(&mut self, world: &World, root: Entity, focus: Option<Entity>) {
        self.members.clear();
        self.entities.clear();
        self.stack.clear();
        self.live_workspaces.clear();
        self.bounds.clear();
        self.related.clear();
        self.contoured.clear();
        self.previews.clear();
        if excluded(world, root) {
            return;
        }
        if let Some(focus) = focus {
            family(world, root, focus, &mut self.related);
        }
        let active = world
            .get::<crate::native::workspace::Workspaces>(root)
            .map(|spaces| spaces.active);
        let mut parent = world.get::<ChildOf>(root).map(ChildOf::parent);
        let mut castle_depth = 0;
        while let Some(entity) = parent {
            castle_depth += usize::from(world.get::<Castle>(entity).is_some());
            parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        self.stack.push((
            root,
            Member {
                hidden: hidden(world, root),
                castle_depth,
            },
        ));
        while let Some((entity, inherited)) = self.stack.pop() {
            if self.members.contains_key(&entity)
                || world.get::<InspectionExcluded>(entity).is_some()
            {
                continue;
            }
            if entity != root
                && let Some(member) = world.get::<crate::native::workspace::WorkspaceMember>(entity)
                && active.is_some_and(|active| member.0 != active)
                && !*self
                    .live_workspaces
                    .entry(member.0)
                    .or_insert_with(|| crate::native::topology::holes::live(world, root, member.0))
            {
                continue;
            }
            let state = Member {
                hidden: inherited.hidden
                    || world.get::<Visibility>(entity) == Some(&Visibility::Hidden)
                    || world
                        .get::<Node>(entity)
                        .is_some_and(|node| node.display == Display::None),
                castle_depth: inherited.castle_depth
                    + usize::from(world.get::<Castle>(entity).is_some()),
            };
            self.members.insert(entity, state);
            self.entities.push(entity);
            if let Some(children) = world.get::<Children>(entity) {
                self.stack
                    .extend(children.iter().map(|child| (child, state)));
            }
        }
        self.entities.sort();
        engine::performance::add("inspection-prepared-nodes", self.entities.len() as u64);
    }

    fn bounds(&mut self, world: &World, entity: Entity) -> Option<Rect> {
        *self
            .bounds
            .entry(entity)
            .or_insert_with(|| bounds(world, entity))
    }

    fn marks(&mut self, world: &World, root: Entity, state: &Inspection, marks: &mut Vec<Mark>) {
        let inspect_connections = state.events || state.hidden || !self.related.is_empty();
        for index in 0..self.entities.len() {
            let entity = self.entities[index];
            if entity == root
                || world
                    .get::<crate::native::area::InfluenceArea>(entity)
                    .is_some()
                || world.get::<Setting>(entity).is_some()
            {
                continue;
            }
            let member = self.members[&entity];
            let local = self.related.contains(&entity);
            let castle = world.get::<Castle>(entity).is_some();
            let sand = castle
                || world.get::<Square>(entity).is_some()
                || world.get::<ImageSand>(entity).is_some()
                || world.get::<Text>(entity).is_some()
                || world.get::<EditableText>(entity).is_some()
                || world
                    .get::<crate::native::canvas::CanvasItem>(entity)
                    .is_some();
            let contour = sand
                && (((state.contours || local) && !member.hidden)
                    || (member.hidden && (state.hidden || local)));
            let tooltip = (state.hidden || local)
                .then(|| world.get::<crate::native::icons::Tooltip>(entity))
                .flatten()
                .filter(|tip| !member.hidden && !tip.0.is_empty());
            if !contour && !inspect_connections && tooltip.is_none() {
                continue;
            }
            let Some(source) = self.bounds(world, entity) else {
                continue;
            };
            if contour && self.contoured.insert(entity) {
                let radius = world
                    .get::<Node>(entity)
                    .map(|node| node.border_radius)
                    .unwrap_or_default();
                let margin = if castle {
                    6.0 / member.castle_depth as f32
                } else {
                    0.0
                };
                marks.push(Mark::Contour(
                    Rect::from_corners(
                        source.min - Vec2::splat(margin),
                        source.max + Vec2::splat(margin),
                    ),
                    radius,
                    member.hidden,
                ));
            }
            // The tree pass already established current exclusion and workspace
            // membership. Pure contour mode does not need action descriptions.
            if inspect_connections {
                visit_connections(world, entity, |target_entity, name| {
                    let Some(target) = self.members.get(&target_entity).copied() else {
                        return;
                    };
                    if !(state.events
                        || state.hidden
                        || local
                        || self.related.contains(&target_entity))
                    {
                        return;
                    }
                    let destination =
                        self.bounds(world, target_entity)
                            .unwrap_or(Rect::from_center_size(
                                source.center() + Vec2::new(-100.0, -60.0),
                                Vec2::new(160.0, 50.0),
                            ));
                    if state.events || local || self.related.contains(&target_entity) {
                        let radius = world
                            .get::<Node>(target_entity)
                            .map(|node| node.border_radius)
                            .unwrap_or_default();
                        if self.contoured.insert(target_entity) {
                            marks.push(Mark::Contour(destination, radius, target.hidden));
                        }
                        marks.push(Mark::Line(source.center(), destination.center()));
                        marks.push(Mark::Label(
                            (source.center() + destination.center()) * 0.5,
                            name.into_owned(),
                            false,
                        ));
                    }
                    if target.hidden
                        && (state.hidden || local)
                        && self.previews.insert(target_entity)
                    {
                        if self.contoured.insert(target_entity) {
                            marks.push(Mark::Contour(destination, BorderRadius::all(px(4)), true));
                        }
                        marks.push(Mark::Label(
                            destination.min,
                            self.preview.text(world, target_entity),
                            true,
                        ));
                    }
                });
            }
            if let Some(tip) = tooltip {
                let position = source.min + Vec2::new(0.0, -48.0);
                marks.push(Mark::Label(position, format!("Tooltip: {}", tip.0), true));
                if state.events || local {
                    marks.push(Mark::Line(source.center(), position));
                    marks.push(Mark::Label(
                        (source.center() + position) * 0.5,
                        "Info Icon Hovered Show Tooltip".into(),
                        false,
                    ));
                }
            }
        }
    }
}

pub(super) struct Preparation {
    roots: QueryState<(Entity, &'static EditMode, &'static Inspection)>,
    scene: Scene,
    marks: Vec<Mark>,
}

impl FromWorld for Preparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            roots: world.query(),
            scene: Scene::default(),
            marks: Vec::new(),
        }
    }
}

pub(super) fn draw(world: &mut World, mut preparation: Local<Preparation>) {
    let Preparation {
        roots,
        scene,
        marks,
    } = &mut *preparation;
    {
        let _preparation = engine::performance::Span::new("inspection-overlay-preparation");
        marks.clear();
        for (root, mode, state) in roots.iter(world) {
            if !mode.enabled {
                continue;
            }
            let focus = state
                .selected
                .filter(|entity| state.click && world.get_entity(*entity).is_ok())
                .or(if state.hover { state.hovered } else { None });
            if focus.is_none() && !state.contours && !state.events && !state.hidden {
                continue;
            }
            scene.prepare(world, root, focus);
            scene.marks(world, root, state, marks);
        }
        engine::performance::add("inspection-marks", marks.len() as u64);
    }
    drawing::apply(world, marks);
}
