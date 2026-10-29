use crate::native::{
    actions::Action,
    canvas::{CanvasItem, CanvasView},
    canvas_item::eligible,
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};

pub(crate) mod scratch;

#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Pinned {
    pub anchor: [f64; 2],
    pub scale: f64,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct PinnedPreview(pub Pinned);

impl Pinned {
    pub(crate) fn valid(&self) -> bool {
        DVec2::from_array(self.anchor).is_finite()
            && (CanvasView::MIN_ZOOM..=CanvasView::MAX_ZOOM).contains(&self.scale)
    }

    pub(crate) fn view(&self, item: &CanvasItem, viewport: Vec2) -> CanvasView {
        CanvasView {
            center: item.position
                - (DVec2::from_array(self.anchor) - DVec2::splat(0.5)) * viewport.as_dvec2()
                    / self.scale,
            zoom: self.scale,
        }
    }

    pub(crate) fn moved(&mut self, delta: DVec2, viewport: Vec2) {
        let anchor = DVec2::from_array(self.anchor) + delta / viewport.as_dvec2();
        if anchor.is_finite() {
            self.anchor = anchor.to_array();
        }
    }
}

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct Placement {
    #[serde(default)]
    pub room: Option<crate::native::topology::rooms::Room>,
    #[serde(default)]
    pub room_member: Option<crate::native::topology::rooms::Member>,
    #[serde(default)]
    pub shader_surface: Option<crate::native::shader_effects::Surface>,
    #[serde(default)]
    pub material: Option<crate::native::topology::appearance::Material>,
    #[serde(default)]
    pub light: Option<crate::native::topology::appearance::Light>,
    #[serde(default)]
    pub binding: Option<nucleus::terrain::SpatialBinding>,
    #[serde(default)]
    pub follow_slope: bool,
    #[serde(default)]
    pub practice_source: Option<String>,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub canvas_component: Option<nucleus::canvas::Component>,
    #[serde(default)]
    pub backend_component: Option<crate::native::component_push::Placed>,
    #[serde(default)]
    pub layout: Option<crate::native::layout::LayoutBox>,
    #[serde(default)]
    pub transient_parent: Option<[u8; 16]>,
    #[serde(default)]
    pub events: crate::native::scoped_events::EventBoundary,
    pub pinned: Option<Pinned>,
    pub order: i32,
    #[serde(default)]
    pub group: Option<crate::native::canvas_selection::SandGroup>,
    #[serde(default)]
    pub spatial: crate::native::topology::Spatial,
    #[serde(default)]
    pub attachment: Option<crate::native::topology::Attachment>,
    #[serde(default)]
    pub group_pose: Option<crate::native::topology::groups::GroupPose>,
}

impl Placement {
    pub(crate) fn capture(world: &World, entity: Entity) -> Self {
        let mut backend_component = world
            .get::<crate::native::component_push::Placed>(entity)
            .cloned();
        if let Some(component) = &mut backend_component
            && let Some(composition) =
                crate::native::component_push::composition::capture(world, entity)
        {
            component.component = nucleus::component::ComponentState::Composition { composition };
        }
        Self {
            room: world
                .get::<crate::native::topology::rooms::Room>(entity)
                .cloned(),
            room_member: world
                .get::<crate::native::topology::rooms::Member>(entity)
                .cloned(),
            shader_surface: world
                .get::<crate::native::shader_effects::Surface>(entity)
                .cloned(),
            material: world
                .get::<crate::native::topology::appearance::Material>(entity)
                .cloned(),
            light: world
                .get::<crate::native::topology::appearance::Light>(entity)
                .cloned(),
            binding: world
                .get::<crate::native::topology::terrain::Bound>(entity)
                .map(|binding| binding.0.clone()),
            follow_slope: world
                .get::<crate::native::topology::terrain::FollowSlope>(entity)
                .is_some(),
            practice_source: world
                .get::<crate::native::practice_cells::PracticeSource>(entity)
                .map(|source| source.0.clone()),
            identity: world
                .get::<crate::native::canvas_host::Identity>(entity)
                .map(|id| id.0.clone()),
            canvas_component: world
                .get::<crate::native::canvas_host::Content>(entity)
                .filter(|content| matches!(content.0, nucleus::canvas::Component::Authored { .. }))
                .map(|content| content.0.clone())
                .or_else(|| {
                    crate::native::canvas_host::composition::capture(world, entity)
                        .map(|composition| nucleus::canvas::Component::Composition { composition })
                })
                .or_else(|| {
                    world
                        .get::<crate::native::canvas_host::Content>(entity)
                        .map(|content| content.0.clone())
                }),
            backend_component,
            layout: world
                .get::<crate::native::layout::LayoutBox>(entity)
                .copied(),
            transient_parent: world.get::<crate::native::layout::desktop::TransientParent>(entity)
                .filter(|parent| world.get::<crate::native::layout::LayoutBox>(entity).is_some() && world.get::<crate::native::layout::desktop::ApplicationWindow>(parent.0).is_none())
                .and_then(|parent| world.get::<crate::native::layout::LayoutBox>(parent.0)).map(|layout| layout.id)
                .or_else(|| world.get::<crate::native::layout::desktop::RestoredTransient>(entity).map(|parent| parent.0)),
            events: world
                .get::<crate::native::scoped_events::EventBoundary>(entity)
                .cloned()
                .unwrap_or_default(),
            pinned: world.get::<Pinned>(entity).copied(),
            spatial: crate::native::topology::spatial(world, entity),
            attachment: world
                .get::<crate::native::topology::Attachment>(entity)
                .copied(),
            group_pose: world
                .get::<crate::native::topology::groups::GroupPose>(entity)
                .copied(),
            group: world
                .get::<crate::native::canvas_selection::SandGroup>(entity)
                .copied(),
            order: world.get::<ZIndex>(entity).map_or(0, |index| index.0),
        }
    }

    pub(crate) fn restore(self, world: &mut World, entity: Entity) {
        world.entity_mut(entity).remove::<(
            crate::native::topology::rooms::Room,
            crate::native::topology::rooms::Member,
            crate::native::topology::appearance::Material,
            crate::native::topology::appearance::Light,
        )>();
        if let Some(room) = self.room {
            world.entity_mut(entity).insert(room);
        }
        if let Some(member) = self.room_member {
            world.entity_mut(entity).insert(member);
        }
        if let Some(material) = self.material {
            world.entity_mut(entity).insert(material);
        }
        if let Some(light) = self.light {
            world.entity_mut(entity).insert(light);
        }
        if let Some(surface) = self.shader_surface {
            world.entity_mut(entity).insert(surface);
        }
        if let Some(binding) = self.binding {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::terrain::Bound(binding));
        }
        if self.follow_slope {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::terrain::FollowSlope);
        }
        if let Some(source) = self.practice_source {
            world
                .entity_mut(entity)
                .insert(crate::native::practice_cells::PracticeSource(source));
        }
        if let Some(identity) = self.identity {
            world
                .entity_mut(entity)
                .insert(crate::native::canvas_host::Identity(identity));
        }
        if let Some(component) = self.canvas_component {
            if let Err(error) =
                crate::native::canvas_host::restore_content(world, entity, &component)
            {
                crate::native::notifications::report(world, "interface::canvas", &error);
            }
            world
                .entity_mut(entity)
                .insert(crate::native::canvas_host::Content(component));
        }
        if let Some(component) = self.backend_component {
            if let nucleus::component::ComponentState::Composition { composition } =
                &component.component
            {
                if let Err(error) = crate::native::component_push::composition::populate(
                    world,
                    entity,
                    composition.clone(),
                ) {
                    crate::native::notifications::report(world, "interface::components", &error);
                }
            }
            world.entity_mut(entity).insert(component);
        }
        if let Some(layout) = self.layout {
            world.entity_mut(entity).insert(layout);
            if world
                .get::<crate::native::area::InfluenceArea>(entity)
                .is_some()
            {
                world.entity_mut(entity).insert(Pickable::default());
            }
        }
        if let Some(parent) = self.transient_parent {
            world.entity_mut(entity).insert(crate::native::layout::desktop::RestoredTransient(parent));
        }
        world.entity_mut(entity).insert(self.events);
        world.entity_mut(entity).insert(ZIndex(self.order));
        world.entity_mut(entity).insert(self.spatial);
        if let Some(pose) = self.group_pose {
            world.entity_mut(entity).insert(pose);
        }
        if let Some(attachment) = self.attachment {
            world.entity_mut(entity).insert(attachment);
        }
        if let Some(group) = self.group {
            world.entity_mut(entity).insert(group);
        }
        if let Some(pinned) = self.pinned {
            world.entity_mut(entity).insert((pinned, GlobalZIndex(1)));
        }
    }

    pub(crate) fn valid(&self) -> bool {
        self.room.as_ref().is_none_or(|value| value.0.valid())
            && self
                .room_member
                .as_ref()
                .is_none_or(|value| value.0.valid())
            && !(self.room.is_some() && self.room_member.is_some())
            && !((self.room.is_some() || self.room_member.is_some()) && self.pinned.is_some())
            && !(self.room_member.is_some() && (self.binding.is_some() || self.follow_slope))
            && self.material.as_ref().is_none_or(|value| value.0.valid())
            && self.light.as_ref().is_none_or(|value| value.0.valid())
            && self
                .shader_surface
                .as_ref()
                .is_none_or(crate::native::shader_effects::Surface::valid)
            && self
                .binding
                .as_ref()
                .is_none_or(nucleus::terrain::SpatialBinding::valid)
            && self
                .practice_source
                .as_ref()
                .is_none_or(|source| nucleus::valid_uid(source, "g"))
            && self
                .identity
                .as_ref()
                .is_none_or(|id| nucleus::valid_uid(id, "placement"))
            && self.canvas_component.as_ref().is_none_or(|component| {
                component
                    .validate_snapshot(&crate::native::canvas_host::registry())
                    .is_ok()
            })
            && self
                .backend_component
                .as_ref()
                .is_none_or(crate::native::component_push::Placed::valid)
            && self.pinned.is_none_or(|pin| pin.valid())
            && self.layout.is_none_or(|layout| layout.valid())
            && self.transient_parent.is_none_or(|parent| parent != [0; 16] && self.layout.is_some_and(|layout| layout.id != parent))
            && self.events.valid()
            && self.spatial.valid()
            && self.group_pose.is_none_or(|pose| pose.valid())
            && self.attachment.is_none_or(|attachment| attachment.valid())
    }
}

#[derive(Clone, Copy)]
pub enum PlacementAction {
    Pin,
    Front,
    Forward,
    Backward,
    Back,
}

impl Action for PlacementAction {
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::native::inspection::Connection> {
        vec![crate::native::inspection::Connection {
            target,
            name: match self {
                Self::Pin => "Sand Clicked Toggle Pin",
                Self::Front => "Sand Clicked Bring To Front",
                Self::Forward => "Sand Clicked Bring Forward",
                Self::Backward => "Sand Clicked Send Backward",
                Self::Back => "Sand Clicked Send To Back",
            }
            .into(),
        }]
    }

    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
            return;
        };
        if world
            .get::<crate::native::area::InfluenceArea>(entity)
            .is_some()
        {
            return;
        }
        if world.get::<CanvasItem>(entity).is_none() {
            return;
        }
        let Some(view) = world.get::<CanvasView>(parent).copied() else {
            return;
        };
        if !view.center.is_finite()
            || !view.zoom.is_finite()
            || view.zoom <= 0.0
            || !eligible(world, parent, entity)
        {
            return;
        }
        if !world
            .get::<crate::native::edit_mode::EditMode>(parent)
            .is_some_and(|mode| mode.enabled)
        {
            return;
        }
        if (world
            .get::<crate::native::topology::rooms::Room>(entity)
            .is_some()
            || world
                .get::<crate::native::topology::rooms::Member>(entity)
                .is_some())
            && world
                .get::<crate::native::workspace_sync::HostedReplica>(entity)
                .is_some()
        {
            crate::native::notifications::report(
                world,
                "Rooms",
                "Use the acting Actor's Workspace Sync to edit this hosted placement.",
            );
            return;
        }
        if matches!(self, Self::Pin) {
            if world
                .get::<crate::native::topology::rooms::Room>(entity)
                .is_some()
                || world
                    .get::<crate::native::topology::rooms::Member>(entity)
                    .is_some()
            {
                crate::native::notifications::report(
                    world,
                    "Rooms",
                    "Room placements use world coordinates and cannot be pinned to the screen.",
                );
                return;
            }
            if world
                .get::<crate::native::topology::presentation::SpatialRoot>(parent)
                .is_some()
                && (world
                    .get::<crate::native::canvas_selection::SandGroup>(entity)
                    .is_some()
                    || world
                        .get::<crate::native::topology::assets::ImportedAsset>(entity)
                        .is_some())
            {
                crate::native::notifications::report(
                    world,
                    "Topology",
                    "Screen pinning is available for individual content Sands. Use world pinning for this object.",
                );
                return;
            }
            if crate::native::layout::linked(world, entity) && world.get::<Pinned>(entity).is_none()
            {
                return;
            }
            let Some(viewport) =
                crate::native::inspection::bounds(world, parent).map(|rect| rect.size())
            else {
                return;
            };
            if !viewport.is_finite() || viewport.min_element() <= 0.0 {
                return;
            }
            let members: Vec<_> =
                crate::native::canvas_selection::companions(world, parent, entity)
                    .into_iter()
                    .filter(|member| {
                        world
                            .get::<crate::native::area::InfluenceArea>(*member)
                            .is_none()
                    })
                    .collect();
            let unpin = members
                .iter()
                .all(|member| world.get::<Pinned>(*member).is_some());
            for member in members {
                if unpin {
                    let pin = world.get::<Pinned>(member).unwrap();
                    let position = view.center
                        + (DVec2::from_array(pin.anchor) - DVec2::splat(0.5)) * viewport.as_dvec2()
                            / view.zoom;
                    world.get_mut::<CanvasItem>(member).unwrap().position = position;
                    world.entity_mut(member).remove::<Pinned>();
                    world.entity_mut(member).remove::<GlobalZIndex>();
                } else if world.get::<Pinned>(member).is_none() {
                    let item = world.get::<CanvasItem>(member).unwrap();
                    let anchor = DVec2::splat(0.5)
                        + (item.position - view.center) * view.zoom / viewport.as_dvec2();
                    let pin = Pinned {
                        anchor: anchor.to_array(),
                        scale: view.zoom,
                    };
                    if pin.valid() {
                        world.entity_mut(member).insert((pin, GlobalZIndex(1)));
                    }
                }
            }
            return;
        }
        let workspace = world.get::<WorkspaceMember>(entity).map(|member| member.0);
        let pinned = world.get::<Pinned>(entity).is_some();
        let mut query =
            world.query::<(Entity, &ChildOf, Option<&WorkspaceMember>, Option<&ZIndex>)>();
        let mut siblings: Vec<_> = query
            .iter(world)
            .filter(|(candidate, child, member, _)| {
                child.parent() == parent
                    && world.get::<CanvasItem>(*candidate).is_some()
                    && world
                        .get::<crate::native::area::InfluenceArea>(*candidate)
                        .is_none()
                    && world.get::<Pinned>(*candidate).is_some() == pinned
                    && member.map(|member| member.0) == workspace
            })
            .map(|(candidate, _, _, index)| (candidate, index.map_or(0, |index| index.0)))
            .collect();
        let children = world.get::<Children>(parent).unwrap();
        siblings.sort_by_key(|(candidate, index)| {
            (
                *index,
                children
                    .iter()
                    .position(|child| child == *candidate)
                    .unwrap_or(0),
            )
        });
        let Some(index) = siblings
            .iter()
            .position(|(candidate, _)| *candidate == entity)
        else {
            return;
        };
        let target = match self {
            Self::Front => siblings.len() - 1,
            Self::Forward => (index + 1).min(siblings.len() - 1),
            Self::Backward => index.saturating_sub(1),
            Self::Back => 0,
            Self::Pin => unreachable!(),
        };
        let entry = siblings.remove(index);
        siblings.insert(target, entry);
        for (index, (candidate, _)) in siblings.into_iter().enumerate() {
            world.entity_mut(candidate).insert(ZIndex(index as i32));
        }
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn appearance_capture_restore_and_clear_preserve_settings() {
        let mut world = World::new();
        let material = nucleus::canvas::appearance::Material {
            lit: true,
            texture: nucleus::canvas::appearance::Texture::Grain,
            ..default()
        };
        let light = nucleus::canvas::appearance::Light::default();
        let entity = world
            .spawn((
                crate::native::topology::appearance::Material(material.clone()),
                crate::native::topology::appearance::Light(light.clone()),
            ))
            .id();
        let placement = Placement::capture(&world, entity);
        assert!(placement.valid());
        let restored: Placement =
            serde_json::from_str(&serde_json::to_string(&placement).unwrap()).unwrap();
        let target = world.spawn_empty().id();
        restored.restore(&mut world, target);
        assert_eq!(
            world
                .get::<crate::native::topology::appearance::Material>(target)
                .unwrap()
                .0,
            material
        );
        assert_eq!(
            world
                .get::<crate::native::topology::appearance::Light>(target)
                .unwrap()
                .0,
            light
        );
        Placement::default().restore(&mut world, target);
        assert!(
            world
                .get::<crate::native::topology::appearance::Material>(target)
                .is_none()
        );
        assert!(
            world
                .get::<crate::native::topology::appearance::Light>(target)
                .is_none()
        );
    }

    #[cfg_attr(test, test)]
    fn pinned_projection_keeps_screen_fraction_and_scale_after_viewport_resize() {
        let item = CanvasItem {
            position: DVec2::splat(1e12),
            size: Vec2::splat(100.0),
        };
        let pin = Pinned {
            anchor: [0.25, 0.75],
            scale: 2.0,
        };
        for viewport in [Vec2::new(800.0, 600.0), Vec2::new(1600.0, 1000.0)] {
            let position = pin
                .view(&item, viewport)
                .screen_position(&item, viewport)
                .unwrap();
            assert_eq!(position + item.size, Vec2::new(0.25, 0.75) * viewport);
        }
        let mut moved = pin;
        moved.moved(DVec2::new(80.0, -60.0), Vec2::new(800.0, 600.0));
        assert_eq!(moved.anchor, [0.35, 0.65]);
        assert!(
            !Pinned {
                anchor: [f64::NAN, 0.0],
                ..pin
            }
            .valid()
        );
        assert!(!Pinned { scale: 0.0, ..pin }.valid());
    }

    #[cfg_attr(test, test)]
    fn layer_commands_reorder_whole_sands_without_crossing_workspaces_or_screen_layer() {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::native::workspace::WorkspacePlugin,
                crate::native::edit_mode::EditModePlugin,
            ));
        let root = app
            .world_mut()
            .spawn(crate::native::container::BoxRoot)
            .id();
        app.update();
        crate::native::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        let mut sands = Vec::new();
        for _ in 0..4 {
            sands.push(
                app.world_mut()
                    .spawn((
                        CanvasItem {
                            position: DVec2::ZERO,
                            size: Vec2::splat(100.0),
                        },
                        ChildOf(root),
                        WorkspaceMember(1),
                    ))
                    .id(),
            );
        }
        let child = app
            .world_mut()
            .spawn((Node::default(), ChildOf(sands[0])))
            .id();
        app.world_mut()
            .entity_mut(sands[2])
            .insert(WorkspaceMember(2));
        app.world_mut().entity_mut(sands[3]).insert(Pinned {
            anchor: [0.5, 0.5],
            scale: 1.0,
        });
        for (action, expected) in [
            (PlacementAction::Front, 1),
            (PlacementAction::Backward, 0),
            (PlacementAction::Forward, 1),
            (PlacementAction::Back, 0),
        ] {
            action.apply(app.world_mut(), sands[0]);
            assert_eq!(app.world().get::<ZIndex>(sands[0]).unwrap().0, expected);
            assert_eq!(
                app.world().get::<ChildOf>(child).unwrap().parent(),
                sands[0]
            );
            assert_eq!(app.world().get::<ZIndex>(sands[2]).unwrap().0, 0);
            assert_eq!(app.world().get::<ZIndex>(sands[3]).unwrap().0, 0);
        }
    }

    crate::native::laboratory_cases! {
        appearance_capture_restore_and_clear_preserve_settings,
        pinned_projection_keeps_screen_fraction_and_scale_after_viewport_resize,
        layer_commands_reorder_whole_sands_without_crossing_workspaces_or_screen_layer,
    }
}
