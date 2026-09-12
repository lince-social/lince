use bevy::{input_focus::InputFocus, math::DVec2, prelude::*, ui::UiSystems};

#[derive(Component, Clone, Copy, Reflect)]
#[reflect(Component)]
#[require(Node)]
pub struct CanvasView {
    pub center: DVec2,
    pub zoom: f64,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self {
            center: DVec2::ZERO,
            zoom: 1.0,
        }
    }
}

#[derive(Component, Clone, Copy, Reflect)]
#[reflect(Component)]
#[require(Node)]
pub struct CanvasItem {
    pub position: DVec2,
    pub size: Vec2,
}

impl CanvasView {
    pub const MIN_ZOOM: f64 = 0.1;
    pub const MAX_ZOOM: f64 = 3.0;

    pub fn set_zoom(&mut self, zoom: f64) {
        if zoom.is_finite() {
            self.zoom = zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        }
    }

    pub fn screen_position(&self, item: &CanvasItem, viewport: Vec2) -> Option<Vec2> {
        if !self.center.is_finite()
            || !self.zoom.is_finite()
            || self.zoom <= 0.0
            || !item.position.is_finite()
            || !item.size.is_finite()
            || item.size.min_element() <= 0.0
            || !viewport.is_finite()
            || viewport.min_element() <= 0.0
        {
            return None;
        }
        let size = item.size.as_dvec2() * self.zoom;
        let top_left =
            (item.position - self.center) * self.zoom + (viewport.as_dvec2() - size) * 0.5;
        let bottom_right = top_left + size;
        if !top_left.is_finite()
            || bottom_right.x < 0.0
            || bottom_right.y < 0.0
            || top_left.x > f64::from(viewport.x)
            || top_left.y > f64::from(viewport.y)
        {
            return None;
        }
        Some(top_left.as_vec2())
    }
}

pub struct CanvasPlugin;

impl Plugin for CanvasPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::canvas_clip::CanvasClipPlugin,
            crate::canvas_pan::CanvasPanPlugin,
            crate::canvas_selection::CanvasSelectionPlugin,
            crate::canvas_background::CanvasBackgroundPlugin,
            crate::sand_placement::PlacementPlugin,
        ))
        .register_type::<CanvasView>()
        .register_type::<CanvasItem>()
        .init_resource::<InputFocus>()
        .add_systems(
            PostUpdate,
            project_canvas
                .after(UiSystems::Propagate)
                .before(UiSystems::Content),
        );
    }
}

fn project_canvas(
    views: Query<(
        &CanvasView,
        &ComputedUiRenderTargetInfo,
        Option<&crate::workspace::Workspaces>,
    )>,
    mut items: Query<(
        Entity,
        &ChildOf,
        &CanvasItem,
        &mut Node,
        &mut UiTransform,
        Option<&crate::workspace::WorkspaceMember>,
        Option<&crate::sand_placement::Pinned>,
    )>,
    parents: Query<&ChildOf>,
    mut focus: ResMut<InputFocus>,
) {
    for (entity, parent, item, mut node, mut transform, member, pinned) in &mut items {
        let view = views.get(parent.parent()).ok();
        let position = view.and_then(|(view, target, spaces)| {
            if spaces.is_some_and(|spaces| {
                member.map_or(spaces.entries[0].id, |member| member.0) != spaces.active
            }) {
                return None;
            }
            pinned
                .map_or(*view, |pin| pin.view(item, target.logical_size()))
                .screen_position(item, target.logical_size())
        });
        let display = if position.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        if let Some(position) = position {
            if node.overflow != Overflow::clip() {
                node.overflow = Overflow::clip();
            }
            let zoom = pinned.map_or(view.unwrap().0.zoom, |pin| pin.scale) as f32;
            let position = position + item.size * ((zoom - 1.0) * 0.5);
            if transform.scale != Vec2::splat(zoom) {
                transform.scale = Vec2::splat(zoom);
            }
            if node.position_type != PositionType::Absolute
                || node.left != px(position.x)
                || node.top != px(position.y)
                || node.width != px(item.size.x)
                || node.height != px(item.size.y)
            {
                node.position_type = PositionType::Absolute;
                node.left = px(position.x);
                node.top = px(position.y);
                node.width = px(item.size.x);
                node.height = px(item.size.y);
            }
        } else {
            let mut focused = focus.get();
            while let Some(candidate) = focused {
                if candidate == entity {
                    focus.clear();
                    break;
                }
                focused = parents.get(candidate).ok().map(ChildOf::parent);
            }
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use bevy::{
        app::{HierarchyPropagatePlugin, PropagateSet},
        camera::{ComputedCameraValues, RenderTargetInfo},
        input_focus::FocusCause,
        ui::update::propagate_ui_target_cameras,
    };

    fn fixture() -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins((
            CanvasPlugin,
            HierarchyPropagatePlugin::<ComputedUiRenderTargetInfo>::new(PostUpdate),
        ))
        .init_resource::<UiScale>()
        .add_systems(
            PostUpdate,
            propagate_ui_target_cameras.in_set(UiSystems::Prepare),
        )
        .configure_sets(
            PostUpdate,
            (UiSystems::Prepare, UiSystems::Propagate).chain(),
        )
        .configure_sets(
            PostUpdate,
            PropagateSet::<ComputedUiRenderTargetInfo>::default().in_set(UiSystems::Propagate),
        );
        let camera = app
            .world_mut()
            .spawn(Camera {
                computed: ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size: UVec2::new(1600, 1200),
                        scale_factor: 2.0,
                    }),
                    ..default()
                },
                ..default()
            })
            .id();
        let view = app
            .world_mut()
            .spawn((CanvasView::default(), UiTargetCamera(camera)))
            .id();
        let item = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                Visibility::Hidden,
                ChildOf(view),
            ))
            .id();
        app.update();
        (app, camera, view, item)
    }

    #[cfg_attr(test, test)]
    fn pinned_sands_do_not_change_layout_when_camera_moves_or_zooms() {
        let (mut app, _, view, item) = fixture();
        app.world_mut()
            .entity_mut(item)
            .insert(crate::sand_placement::Pinned {
                anchor: [0.25, 0.75],
                scale: 1.0,
            });
        app.update();
        assert_eq!(app.world().get::<Node>(item).unwrap().left, px(150));
        assert_eq!(app.world().get::<Node>(item).unwrap().top, px(400));
        app.world_mut().clear_trackers();
        *app.world_mut().get_mut::<CanvasView>(view).unwrap() = CanvasView {
            center: DVec2::splat(1e12),
            zoom: 3.0,
        };
        app.update();
        assert!(
            !app.world()
                .entity(item)
                .get_ref::<Node>()
                .unwrap()
                .is_changed()
        );
        assert!(
            !app.world()
                .entity(item)
                .get_ref::<UiTransform>()
                .unwrap()
                .is_changed()
        );
        assert_eq!(
            app.world().get::<Node>(item).unwrap().display,
            Display::Flex
        );
    }

    #[cfg_attr(test, test)]
    fn zoom_scales_contents_and_culling_without_changing_world_coordinates() {
        let (mut app, _, view, item) = fixture();
        app.world_mut()
            .get_mut::<CanvasItem>(item)
            .unwrap()
            .position = DVec2::new(260.0, 0.0);
        app.world_mut()
            .get_mut::<CanvasView>(view)
            .unwrap()
            .set_zoom(2.0);
        app.update();
        assert_eq!(
            app.world().get::<Node>(item).unwrap().display,
            Display::None
        );
        app.world_mut()
            .get_mut::<CanvasView>(view)
            .unwrap()
            .set_zoom(0.5);
        app.update();
        let node = app.world().get::<Node>(item).unwrap();
        assert_eq!(node.display, Display::Flex);
        assert_eq!(node.left, px(480));
        assert_eq!(node.width, px(100));
        assert_eq!(
            app.world().get::<UiTransform>(item).unwrap().scale,
            Vec2::splat(0.5)
        );
        assert_eq!(
            app.world().get::<CanvasItem>(item).unwrap().position,
            DVec2::new(260.0, 0.0)
        );
        app.update();
        assert!(
            !app.world()
                .entity(item)
                .get_ref::<UiTransform>()
                .unwrap()
                .is_changed()
        );
    }

    #[cfg_attr(test, test)]
    fn zoom_keeps_distant_fractional_positions_and_rejects_invalid_values() {
        let mut view = CanvasView {
            center: DVec2::splat(1e12),
            zoom: 2.0,
        };
        let item = CanvasItem {
            position: view.center + DVec2::new(0.25, -0.5),
            size: Vec2::splat(100.0),
        };
        assert_eq!(
            view.screen_position(&item, Vec2::new(800.0, 600.0)),
            Some(Vec2::new(300.5, 199.0))
        );
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            view.set_zoom(invalid);
            assert_eq!(view.zoom, 2.0);
            let invalid_view = CanvasView {
                zoom: invalid,
                ..view
            };
            assert!(
                invalid_view
                    .screen_position(&item, Vec2::splat(800.0))
                    .is_none()
            );
        }
    }

    #[cfg_attr(test, test)]
    fn projection_uses_logical_pixels_and_updates_after_resize() {
        let (mut app, camera, _, item) = fixture();
        assert_eq!(app.world().get::<Node>(item).unwrap().left, px(350));
        app.world_mut()
            .get_mut::<Camera>(camera)
            .unwrap()
            .computed
            .target_info
            .as_mut()
            .unwrap()
            .physical_size = UVec2::splat(800);
        app.update();
        let node = app.world().get::<Node>(item).unwrap();
        assert_eq!(node.left, px(150));
        assert_eq!(node.top, px(150));
    }

    #[cfg_attr(test, test)]
    fn leaving_view_releases_focus_and_returning_preserves_content_and_visibility() {
        let (mut app, _, view, item) = fixture();
        let editor = app
            .world_mut()
            .spawn((Text::new("keep this"), ChildOf(item)))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(editor, FocusCause::Navigated);
        app.world_mut().get_mut::<CanvasView>(view).unwrap().center = DVec2::splat(1e12);
        app.update();
        assert_eq!(
            app.world().get::<Node>(item).unwrap().display,
            Display::None
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
        app.world_mut().get_mut::<CanvasView>(view).unwrap().center = DVec2::ZERO;
        app.update();
        assert_eq!(
            app.world().get::<Node>(item).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            *app.world().get::<Visibility>(item).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(app.world().get::<Text>(editor).unwrap().0, "keep this");
        assert_eq!(
            app.world().get::<CanvasItem>(item).unwrap().position,
            DVec2::ZERO
        );
    }

    #[cfg_attr(test, test)]
    fn quiet_frames_do_not_invalidate_layout() {
        let (mut app, _, _, item) = fixture();
        app.world_mut().clear_trackers();
        for _ in 0..100 {
            app.update();
            assert!(
                !app.world()
                    .entity(item)
                    .get_ref::<Node>()
                    .unwrap()
                    .is_changed()
            );
        }
    }

    #[cfg_attr(test, test)]
    fn distant_positions_keep_local_precision_in_every_direction() {
        for direction in [DVec2::ONE, -DVec2::ONE, DVec2::new(-1.0, 1.0)] {
            let center = direction * 1_000_000_000_000.0;
            let item = CanvasItem {
                position: center + DVec2::new(0.25, -0.5),
                size: Vec2::splat(100.0),
            };
            assert_eq!(
                CanvasView {
                    center,
                    ..default()
                }
                .screen_position(&item, Vec2::new(800.0, 600.0)),
                Some(Vec2::new(350.25, 249.5))
            );
        }
    }

    #[cfg_attr(test, test)]
    fn resize_and_camera_movement_preserve_item_coordinates() {
        let item = CanvasItem {
            position: DVec2::new(-100.0, 50.0),
            size: Vec2::splat(100.0),
        };
        let mut view = CanvasView::default();
        assert_eq!(
            view.screen_position(&item, Vec2::splat(400.0)),
            Some(Vec2::new(50.0, 200.0))
        );
        assert_eq!(
            view.screen_position(&item, Vec2::splat(800.0)),
            Some(Vec2::new(250.0, 400.0))
        );
        view.center = item.position;
        assert_eq!(
            view.screen_position(&item, Vec2::splat(800.0)),
            Some(Vec2::splat(350.0))
        );
        assert_eq!(item.position, DVec2::new(-100.0, 50.0));
    }

    #[cfg_attr(test, test)]
    fn offscreen_and_invalid_items_never_reach_layout() {
        let view = CanvasView::default();
        let mut item = CanvasItem {
            position: DVec2::new(249.0, 0.0),
            size: Vec2::splat(100.0),
        };
        assert!(view.screen_position(&item, Vec2::splat(400.0)).is_some());
        for position in [
            DVec2::splat(251.0),
            DVec2::splat(-251.0),
            DVec2::splat(f64::MAX),
            DVec2::NAN,
            DVec2::INFINITY,
        ] {
            item.position = position;
            assert!(view.screen_position(&item, Vec2::splat(400.0)).is_none());
        }
        item.position = DVec2::ZERO;
        for size in [Vec2::ZERO, Vec2::splat(-1.0), Vec2::NAN, Vec2::INFINITY] {
            item.size = size;
            assert!(view.screen_position(&item, Vec2::splat(400.0)).is_none());
        }
    }

    crate::laboratory_cases! {
        pinned_sands_do_not_change_layout_when_camera_moves_or_zooms,
        zoom_scales_contents_and_culling_without_changing_world_coordinates,
        zoom_keeps_distant_fractional_positions_and_rejects_invalid_values,
        projection_uses_logical_pixels_and_updates_after_resize,
        leaving_view_releases_focus_and_returning_preserves_content_and_visibility,
        quiet_frames_do_not_invalidate_layout,
        distant_positions_keep_local_precision_in_every_direction,
        resize_and_camera_movement_preserve_item_coordinates,
        offscreen_and_invalid_items_never_reach_layout,
    }
}
