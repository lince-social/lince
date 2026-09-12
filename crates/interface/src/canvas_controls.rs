use crate::{
    actions::{Action, ActionButton, ActionsPlugin},
    canvas::{CanvasItem, CanvasView},
    sand::button,
    theme::{PURPLE, Typography},
};
use bevy::{
    a11y::AccessibilityNode,
    input_focus::{FocusCause, FocusGained, FocusLost, InputFocus, InputFocusVisible},
    math::DVec2,
    prelude::*,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasAction {
    Recenter,
    BringHere,
    ZoomOut,
    ResetZoom,
    ZoomIn,
}

impl Action for CanvasAction {
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::inspection::Connection> {
        vec![crate::inspection::Connection {
            target,
            name: format!("{} Clicked", self.label()),
        }]
    }

    fn apply(&self, world: &mut World, target: Entity) {
        world.trigger(CanvasRequest {
            entity: target,
            action: *self,
        });
    }
}

#[derive(EntityEvent)]
struct CanvasRequest {
    entity: Entity,
    action: CanvasAction,
}

impl CanvasAction {
    fn label(self) -> &'static str {
        match self {
            Self::Recenter => "Recenter",
            Self::BringHere => "Bring Sands here",
            Self::ZoomOut => "Zoom out",
            Self::ResetZoom => "Reset zoom to 100%",
            Self::ZoomIn => "Zoom in",
        }
    }
}

#[derive(Component)]
pub struct CanvasControl {
    pub view: Entity,
    pub action: CanvasAction,
}

#[derive(Component)]
struct ZoomLabel(Entity);

#[derive(Component)]
pub(crate) struct CanvasToolbar(Entity);

const CONTROL_HEIGHT: f32 = 40.0;

fn toolbar_bundle(view: Entity) -> impl Bundle {
    (
        CanvasToolbar(view),
        crate::inspection::InspectionExcluded,
        crate::castle::Castle,
        Node {
            position_type: PositionType::Absolute,
            right: px(12),
            bottom: px(12),
            max_width: percent(96),
            flex_wrap: FlexWrap::NoWrap,
            align_items: AlignItems::Center,
            column_gap: px(6),
            padding: UiRect::all(px(4)),
            overflow: Overflow::scroll_x(),
            ..default()
        },
        ScrollPosition::default(),
        GlobalZIndex(20),
        ChildOf(view),
    )
}

pub(crate) fn toolbar(world: &mut World, view: Entity) -> Entity {
    if let Some(entity) = world
        .query::<(Entity, &CanvasToolbar)>()
        .iter(world)
        .find(|(_, bar)| bar.0 == view)
        .map(|(entity, _)| entity)
    {
        entity
    } else {
        world.spawn(toolbar_bundle(view)).id()
    }
}

pub struct CanvasControlsPlugin;

impl Plugin for CanvasControlsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ActionsPlugin>() {
            app.add_plugins(ActionsPlugin);
        }
        app.init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .add_observer(activate)
            .add_observer(scroll_toolbar)
            .add_observer(
                |event: On<FocusGained>,
                 visible: Res<InputFocusVisible>,
                 mut controls: Query<&mut Outline, With<CanvasControl>>| {
                    if event.cause == FocusCause::Navigated
                        && visible.0
                        && let Ok(mut outline) = controls.get_mut(event.entity)
                    {
                        outline.width = px(2);
                    }
                },
            )
            .add_observer(
                |event: On<FocusLost>, mut controls: Query<&mut Outline, With<CanvasControl>>| {
                    if let Ok(mut outline) = controls.get_mut(event.entity) {
                        outline.width = px(0);
                    }
                },
            )
            .add_systems(Update, (create_controls, label_controls).chain())
            .add_systems(
                PostUpdate,
                reveal_toolbar_focus.after(bevy::ui::UiSystems::Layout),
            )
            .add_systems(PostUpdate, zoom_labels.after(crate::actions::ApplyActions));
    }
}

pub(crate) fn create_controls(
    mut commands: Commands,
    views: Query<Entity, Added<CanvasView>>,
    typography: Res<Typography>,
    bars: Query<(Entity, &CanvasToolbar)>,
) {
    for view in &views {
        let toolbar = bars
            .iter()
            .find(|(_, bar)| bar.0 == view)
            .map(|(entity, _)| entity)
            .unwrap_or_else(|| commands.spawn(toolbar_bundle(view)).id());
        for action in [
            CanvasAction::Recenter,
            CanvasAction::BringHere,
            CanvasAction::ZoomOut,
            CanvasAction::ResetZoom,
            CanvasAction::ZoomIn,
        ] {
            let control = commands
                .spawn((
                    CanvasControl { view, action },
                    ActionButton::new(view, crate::actions![action]),
                    button(0),
                    Node {
                        height: px(CONTROL_HEIGHT),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::axes(px(10), px(8)),
                        border: UiRect::all(px(crate::sand::BUTTON_BORDER_WIDTH)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    crate::token_style::border(crate::tokens::Token::Accent),
                    crate::token_style::OutlineToken(crate::tokens::Token::Accent),
                    Outline {
                        width: px(0),
                        offset: px(2),
                        color: PURPLE,
                    },
                    crate::token_style::background(crate::tokens::Token::Surface),
                    ChildOf(toolbar),
                ))
                .id();
            if action != CanvasAction::ResetZoom {
                let icon = match action {
                    CanvasAction::Recenter => crate::icons::Icon::Recenter,
                    CanvasAction::BringHere => crate::icons::Icon::BringHere,
                    CanvasAction::ZoomOut => crate::icons::Icon::Minus,
                    CanvasAction::ZoomIn => crate::icons::Icon::Plus,
                    CanvasAction::ResetZoom => unreachable!(),
                };
                commands
                    .entity(control)
                    .insert(crate::icons::IconButton::new(icon, action.label()));
                continue;
            }
            commands
                .entity(control)
                .insert(crate::icons::Tooltip(action.label().into()));
            let mut label = commands.spawn((
                Text::new(if action == CanvasAction::ResetZoom {
                    "100%"
                } else {
                    action.label()
                }),
                typography.text(14.0),
                crate::token_style::text(crate::tokens::Token::Ink),
                ChildOf(control),
            ));
            if action == CanvasAction::ResetZoom {
                label.insert(ZoomLabel(view));
            }
        }
    }
}

fn scroll_toolbar(
    mut event: On<Pointer<bevy::picking::events::Scroll>>,
    mut bars: Query<(&ComputedNode, &mut ScrollPosition), With<CanvasToolbar>>,
) {
    if let Ok((node, mut scroll)) = bars.get_mut(event.entity) {
        let step = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
            24.0
        } else {
            1.0
        };
        let max = (node.content_size.x - node.size.x + node.scrollbar_size.x).max(0.0)
            * node.inverse_scale_factor();
        scroll.0.x = (scroll.0.x - (event.x + event.y) * step).clamp(0.0, max);
        event.propagate(false);
    }
}

fn reveal_toolbar_focus(
    focus: Res<InputFocus>,
    mut previous: Local<(Option<Entity>, Vec2)>,
    parents: Query<&ChildOf>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut bars: Query<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<CanvasToolbar>>,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    let changed = previous.0 != focus.get();
    previous.0 = focus.get();
    let Some(entity) = focus.get() else { return };
    let Ok(parent) = parents.get(entity) else {
        return;
    };
    let Ok((bar, bar_transform, mut scroll)) = bars.get_mut(parent.parent()) else {
        return;
    };
    if !changed && previous.1 == bar.size() {
        return;
    }
    previous.1 = bar.size();
    let Ok((node, transform)) = geometry.get(entity) else {
        return;
    };
    let left = transform.translation.x - node.size().x * 0.5;
    let right = left + node.size().x;
    let inset = 4.0 / bar.inverse_scale_factor();
    let start = bar_transform.translation.x - bar.size().x * 0.5 + inset;
    let end = bar_transform.translation.x + bar.size().x * 0.5 - inset;
    let delta = if left < start {
        left - start
    } else if right > end {
        right - end
    } else {
        0.0
    };
    let max = (bar.content_size.x - bar.size.x + bar.scrollbar_size.x).max(0.0)
        * bar.inverse_scale_factor();
    let next = (scroll.0.x + delta * bar.inverse_scale_factor()).clamp(0.0, max);
    if (next - scroll.0.x).abs() > 0.5 {
        scroll.0.x = next;
        if let Some(wake) = wake {
            wake.ring();
        }
    }
}

fn label_controls(
    mut controls: Query<(&CanvasControl, &mut AccessibilityNode), Added<CanvasControl>>,
) {
    for (control, mut node) in &mut controls {
        node.set_label(control.action.label());
    }
}

fn zoom_labels(
    views: Query<&CanvasView, Changed<CanvasView>>,
    mut labels: Query<(&ZoomLabel, &mut Text)>,
) {
    for (label, mut text) in &mut labels {
        if let Ok(view) = views.get(label.0) {
            let value = format!("{:.0}%", view.zoom * 100.0);
            if text.0 != value {
                text.0 = value;
            }
        }
    }
}

fn activate(
    event: On<CanvasRequest>,
    mut views: Query<&mut CanvasView>,
    mut items: Query<(Entity, &ChildOf, &mut CanvasItem), Without<crate::area::InfluenceArea>>,
    spaces: Query<&crate::workspace::Workspaces>,
    members: Query<&crate::workspace::WorkspaceMember>,
) {
    let Ok(mut view) = views.get_mut(event.entity) else {
        return;
    };
    match event.action {
        CanvasAction::Recenter => {
            if view.center != DVec2::ZERO {
                view.center = DVec2::ZERO;
            }
        }
        CanvasAction::BringHere => {
            if !view.center.is_finite() {
                return;
            }
            let mut sizes: Vec<_> = items
                .iter()
                .filter(|(entity, parent, item)| {
                    parent.parent() == event.entity
                        && spaces.get(event.entity).ok().is_none_or(|spaces| {
                            members
                                .get(*entity)
                                .map_or(spaces.entries[0].id, |member| member.0)
                                == spaces.active
                        })
                        && item.size.is_finite()
                        && item.size.min_element() > 0.0
                })
                .map(|(entity, _, item)| (entity, item.size))
                .collect();
            sizes.sort_unstable_by_key(|(entity, _)| entity.to_bits());
            for ((entity, _), offset) in sizes.iter().zip(circular_positions(&sizes)) {
                let position = view.center + offset;
                if let Ok((_, _, mut item)) = items.get_mut(*entity)
                    && item.position != position
                {
                    item.position = position;
                }
            }
        }
        action => {
            let zoom = match action {
                CanvasAction::ZoomOut => view.zoom / 1.2,
                CanvasAction::ZoomIn => view.zoom * 1.2,
                _ => 1.0,
            }
            .clamp(CanvasView::MIN_ZOOM, CanvasView::MAX_ZOOM);
            if view.zoom != zoom {
                view.set_zoom(zoom);
            }
        }
    }
}

fn circular_positions(items: &[(Entity, Vec2)]) -> Vec<DVec2> {
    if items.is_empty() {
        return Vec::new();
    }
    let step = items
        .iter()
        .map(|(_, size)| f64::from(size.max_element()))
        .fold(0.0, f64::max)
        + 24.0;
    let extent = (items.len() as f64).sqrt().ceil() as i64;
    let mut slots: Vec<_> = (-extent..=extent)
        .flat_map(|y| (-extent..=extent).map(move |x| (x, y)))
        .collect();
    slots.sort_unstable_by_key(|&(x, y)| (x * x + y * y, y, x));
    let mut positions: Vec<_> = slots
        .into_iter()
        .take(items.len())
        .map(|(x, y)| DVec2::new(x as f64, y as f64) * step)
        .collect();
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);
    for ((_, size), &position) in items.iter().zip(&positions) {
        let half = size.as_dvec2() * 0.5;
        min = min.min(position - half);
        max = max.max(position + half);
    }
    let center = (min + max) * 0.5;
    for position in &mut positions {
        *position -= center;
    }
    positions
}

pub(crate) mod tests {
    use super::*;
    use bevy::ui_widgets::Activate;
    use bevy::{input_focus::FocusCause, text::EditableText};

    fn fixture() -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<Typography>()
            .add_plugins(CanvasControlsPlugin);
        let view = app.world_mut().spawn(CanvasView::default()).id();
        app.update();
        (app, view)
    }

    fn press(app: &mut App, view: Entity, action: CanvasAction) {
        let entity = app
            .world_mut()
            .query::<(Entity, &CanvasControl)>()
            .iter(app.world())
            .find(|(_, control)| control.view == view && control.action == action)
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity });
        app.update();
    }

    #[cfg_attr(test, test)]
    fn controls_share_a_height_and_only_keyboard_focus_is_highlighted() {
        let (mut app, view) = fixture();
        let controls: Vec<_> = app
            .world_mut()
            .query::<(Entity, &CanvasControl)>()
            .iter(app.world())
            .filter(|(_, control)| control.view == view)
            .map(|(entity, _)| entity)
            .collect();
        assert_eq!(controls.len(), 5);
        for control in &controls {
            assert_eq!(
                app.world().get::<Node>(*control).unwrap().height,
                px(CONTROL_HEIGHT)
            );
        }
        let control = controls[0];
        app.world_mut().trigger(FocusGained {
            entity: control,
            cause: FocusCause::Pressed,
        });
        assert_eq!(app.world().get::<Outline>(control).unwrap().width, px(0));
        app.world_mut().resource_mut::<InputFocusVisible>().0 = true;
        app.world_mut().trigger(FocusGained {
            entity: control,
            cause: FocusCause::Navigated,
        });
        assert_eq!(app.world().get::<Outline>(control).unwrap().width, px(2));
        app.world_mut().trigger(FocusLost { entity: control });
        assert_eq!(app.world().get::<Outline>(control).unwrap().width, px(0));
    }

    #[cfg_attr(test, test)]
    fn recenter_keeps_zoom_and_sands_and_only_moves_its_camera() {
        let (mut app, view) = fixture();
        let other = app
            .world_mut()
            .spawn(CanvasView {
                center: DVec2::splat(-1000.0),
                zoom: 0.5,
            })
            .id();
        let item = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::splat(70.0),
                    size: Vec2::splat(200.0),
                },
                ChildOf(view),
            ))
            .id();
        *app.world_mut().get_mut::<CanvasView>(view).unwrap() = CanvasView {
            center: DVec2::splat(1e12),
            zoom: 2.0,
        };
        press(&mut app, view, CanvasAction::Recenter);
        let camera = app.world().get::<CanvasView>(view).unwrap();
        assert_eq!(camera.center, DVec2::ZERO);
        assert_eq!(camera.zoom, 2.0);
        assert_eq!(
            app.world().get::<CanvasItem>(item).unwrap().position,
            DVec2::splat(70.0)
        );
        assert_eq!(
            app.world().get::<CanvasView>(other).unwrap().center,
            DVec2::splat(-1000.0)
        );
    }

    #[cfg_attr(test, test)]
    fn bring_here_keeps_entities_content_visibility_and_other_views() {
        let (mut app, view) = fixture();
        let center = DVec2::new(1e12, -1e12);
        app.world_mut().get_mut::<CanvasView>(view).unwrap().center = center;
        let mut entities = Vec::new();
        for index in 0..25 {
            let size = Vec2::new(100.0 + index as f32 * 5.0, 80.0);
            let item = app
                .world_mut()
                .spawn((
                    CanvasItem {
                        position: DVec2::splat(-1e12),
                        size,
                    },
                    Visibility::Hidden,
                    ChildOf(view),
                ))
                .id();
            let editor = app
                .world_mut()
                .spawn((crate::sand::editable("Unsaved draft"), ChildOf(item)))
                .id();
            entities.push((item, editor, size));
        }
        let other_view = app.world_mut().spawn(CanvasView::default()).id();
        let other_item = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::ONE,
                },
                ChildOf(other_view),
            ))
            .id();
        press(&mut app, view, CanvasAction::BringHere);
        let mut min = DVec2::splat(f64::INFINITY);
        let mut max = DVec2::splat(f64::NEG_INFINITY);
        let mut positions: Vec<(DVec2, Vec2)> = Vec::new();
        for &(entity, editor, size) in &entities {
            let item = app.world().get::<CanvasItem>(entity).unwrap();
            assert_eq!(item.size, size);
            assert_eq!(
                app.world()
                    .get::<EditableText>(editor)
                    .unwrap()
                    .value()
                    .to_string(),
                "Unsaved draft"
            );
            assert_eq!(
                *app.world().get::<Visibility>(entity).unwrap(),
                Visibility::Hidden
            );
            let half = size.as_dvec2() * 0.5;
            min = min.min(item.position - half);
            max = max.max(item.position + half);
            for &(position, other_size) in &positions {
                let delta: DVec2 = item.position - position;
                let required: DVec2 = (size + other_size).as_dvec2() * 0.5 + DVec2::splat(23.99);
                assert!(delta.abs().x >= required.x || delta.abs().y >= required.y);
            }
            positions.push((item.position, size));
        }
        assert_eq!((min + max) * 0.5, center);
        assert_eq!(app.world().get::<CanvasView>(view).unwrap().center, center);
        assert_eq!(
            app.world().get::<CanvasItem>(other_item).unwrap().position,
            DVec2::ZERO
        );
        press(&mut app, view, CanvasAction::BringHere);
        for ((entity, _, _), (position, _)) in entities.iter().zip(positions) {
            assert_eq!(
                app.world().get::<CanvasItem>(*entity).unwrap().position,
                position
            );
        }
    }

    #[cfg_attr(test, test)]
    fn empty_and_single_sand_arrangements_are_centered() {
        let (mut app, view) = fixture();
        press(&mut app, view, CanvasAction::BringHere);
        let item = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::INFINITY,
                    size: Vec2::new(50.0, 200.0),
                },
                ChildOf(view),
            ))
            .id();
        press(&mut app, view, CanvasAction::BringHere);
        assert_eq!(
            app.world().get::<CanvasItem>(item).unwrap().position,
            DVec2::ZERO
        );
    }

    #[cfg_attr(test, test)]
    fn zoom_buttons_keep_center_clamp_reset_and_update_the_percentage() {
        let (mut app, view) = fixture();
        app.world_mut().get_mut::<CanvasView>(view).unwrap().center = DVec2::splat(1e12);
        press(&mut app, view, CanvasAction::ZoomIn);
        assert_eq!(app.world().get::<CanvasView>(view).unwrap().zoom, 1.2);
        let label = app
            .world_mut()
            .query_filtered::<Entity, With<ZoomLabel>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<Text>(label).unwrap().0, "120%");
        press(&mut app, view, CanvasAction::ZoomOut);
        assert_eq!(app.world().get::<CanvasView>(view).unwrap().zoom, 1.0);
        for _ in 0..30 {
            press(&mut app, view, CanvasAction::ZoomOut);
        }
        assert_eq!(
            app.world().get::<CanvasView>(view).unwrap().zoom,
            CanvasView::MIN_ZOOM
        );
        for _ in 0..40 {
            press(&mut app, view, CanvasAction::ZoomIn);
        }
        assert_eq!(
            app.world().get::<CanvasView>(view).unwrap().zoom,
            CanvasView::MAX_ZOOM
        );
        press(&mut app, view, CanvasAction::ResetZoom);
        let camera = app.world().get::<CanvasView>(view).unwrap();
        assert_eq!(camera.zoom, 1.0);
        assert_eq!(camera.center, DVec2::splat(1e12));
        assert_eq!(app.world().get::<Text>(label).unwrap().0, "100%");
        app.update();
        assert!(
            !app.world()
                .entity(label)
                .get_ref::<Text>()
                .unwrap()
                .is_changed()
        );
    }

    crate::laboratory_cases! {
        controls_share_a_height_and_only_keyboard_focus_is_highlighted,
        recenter_keeps_zoom_and_sands_and_only_moves_its_camera,
        bring_here_keeps_entities_content_visibility_and_other_views,
        empty_and_single_sand_arrangements_are_centered,
        zoom_buttons_keep_center_clamp_reset_and_update_the_percentage,
    }
}
