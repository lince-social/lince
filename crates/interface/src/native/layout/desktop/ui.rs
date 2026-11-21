// non-lean-yet
// Scope: mod controls, struct TilingControl, struct FocusControl, struct OverviewControl.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;
use crate::i18n::Token;

mod controls;

#[derive(Component)]
struct TilingControl {
    root: Entity,
    label: Entity,
    toolbar: bool,
}

#[derive(Component)]
struct FocusControl {
    root: Entity,
    label: Entity,
}

#[derive(Component)]
struct OverviewControl {
    root: Entity,
}

pub(crate) fn overview_sand(world: &mut World, root: Entity, parent: Entity) -> Entity {
    let control = world.spawn((
        crate::native::sand::button(0),
        crate::native::actions::ActionButton::new(root, crate::native::actions![DesktopAction::ToggleOverview]),
        crate::native::shortcuts::KeyboardModalTrigger,
        OverviewControl { root },
        Node {
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
        ChildOf(parent),
    )).id();
    crate::native::edit_mode::label(world, control, Token::ShortcutOverview.text(), 14.0);
    control
}

fn focus_button(world: &mut World, root: Entity, parent: Entity) {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::actions::ActionButton::new(
                root,
                crate::native::actions![DesktopAction::ToggleFocus],
            ),
            crate::native::shortcuts::KeyboardModalTrigger,
            crate::native::canvas_controls::ToolbarOrder(1),
            Node {
                height: px(40),
                padding: UiRect::axes(px(10), px(8)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ChildOf(parent),
        ))
        .id();
    let label = crate::native::edit_mode::label(world, entity, Token::FocusTarget.text(), 14.0);
    world
        .entity_mut(entity)
        .insert(FocusControl { root, label });
}

fn tiled(world: &World, root: Entity) -> bool {
    world
        .get::<Workspaces>(root)
        .and_then(|spaces| {
            spaces
                .entries
                .iter()
                .find(|space| space.id == spaces.active)
        })
        .is_some_and(|space| space.desktop.mode() != WorkspaceLayout::Free)
}

fn mode_caption(tiled: bool) -> &'static str {
    if tiled {
        Token::TilingOn.text()
    } else {
        Token::TilingOff.text()
    }
}

fn caption(world: &World, root: Entity) -> &'static str {
    mode_caption(tiled(world, root))
}

fn button(world: &mut World, root: Entity, parent: Entity, toolbar: bool) {
    let value = caption(world, root);
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::actions::ActionButton::new(
                root,
                crate::native::actions![DesktopAction::ToggleTiling],
            ),
            crate::native::shortcuts::KeyboardModalTrigger,
            Node {
                height: px(40),
                padding: UiRect::axes(px(10), px(8)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ChildOf(parent),
        ))
        .id();
    let label = crate::native::edit_mode::label(world, entity, value, 14.0);
    world.entity_mut(entity).insert(TilingControl {
        root,
        label,
        toolbar,
    });
    if toolbar {
        world
            .entity_mut(entity)
            .insert(crate::native::canvas_controls::ToolbarOrder(2));
    }
}

pub(crate) fn create_controls(world: &mut World) {
    controls::create(world);
}

pub(crate) fn label_controls(world: &mut World) {
    controls::labels(world);
}

pub(crate) fn panel(world: &mut World, root: Entity, panel: Entity) {
    button(world, root, panel, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_is_placeable_without_being_recreated_in_the_canvas_toolbar() {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        create_controls(app.world_mut());
        create_controls(app.world_mut());
        assert_eq!(app.world_mut().query::<&OverviewControl>().iter(app.world()).count(), 0);
        let sand = crate::native::sand_store::spawn_sand(app.world_mut(), root, 1,
            crate::native::sand_store::SandKind::Overview, "", bevy::math::DVec2::ZERO);
        let content = app.world().get::<crate::native::sand_store::StoredSand>(sand).unwrap().content.unwrap();
        assert!(app.world().get::<OverviewControl>(content).is_some());
        assert_eq!(app.world().get::<crate::native::actions::ActionButton>(content).unwrap().target, root);
        create_controls(app.world_mut());
        label_controls(app.world_mut());
        assert_eq!(app.world_mut().query::<&OverviewControl>().iter(app.world()).count(), 1);
        assert_eq!(app.world().get::<ChildOf>(content).unwrap().parent(), sand);
        assert!(app.world().get::<crate::native::icons::Tooltip>(content).unwrap().0.contains(Token::ShortcutOverview.text()));
    }

    #[test]
    fn focus_button_retains_its_selected_target_when_the_toolbar_takes_keyboard_focus() {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        app.world_mut().entity_mut(root).insert(ComputedNode {
            size: Vec2::new(1000.0, 700.0),
            inverse_scale_factor: 1.0,
            ..default()
        });
        create_controls(app.world_mut());
        label_controls(app.world_mut());
        let (button, label) = app
            .world_mut()
            .query::<(Entity, &FocusControl)>()
            .iter(app.world())
            .map(|(entity, control)| (entity, control.label))
            .next()
            .unwrap();
        assert!(
            app.world()
                .get::<bevy::ui::InteractionDisabled>(button)
                .is_some()
        );
        let target = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: bevy::math::DVec2::new(750.0, -300.0),
                    size: Vec2::new(300.0, 200.0),
                },
                LayoutBox::new(Vec2::new(300.0, 200.0)),
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        app.world_mut()
            .entity_mut(root)
            .insert(crate::native::canvas_selection::SandSelection(vec![target]));
        app.world_mut().spawn((
            CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size: Vec2::splat(100.0),
            },
            LayoutBox::new(Vec2::splat(100.0)),
            WorkspaceMember(1),
            ChildOf(root),
        ));
        label_controls(app.world_mut());
        focus::resolve(app.world_mut());
        assert!(
            app.world()
                .get::<bevy::ui::InteractionDisabled>(button)
                .is_none()
        );
        app.world_mut()
            .get_mut::<crate::native::canvas_selection::SandSelection>(root)
            .unwrap()
            .0
            .clear();
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(button, bevy::input_focus::FocusCause::Navigated);
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: button });
        app.update();
        focus::resolve(app.world_mut());
        label_controls(app.world_mut());
        assert_eq!(
            app.world().get::<Text>(label).unwrap().0,
            Token::UnfocusTarget.text()
        );
        assert!(focus::following(app.world(), root));
        assert_eq!(
            app.world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(target)
        );
        assert_eq!(
            app.world()
                .get::<crate::native::canvas_controls::ToolbarOrder>(button)
                .unwrap()
                .0,
            1
        );
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: button });
        app.update();
        label_controls(app.world_mut());
        assert_eq!(
            app.world().get::<Text>(label).unwrap().0,
            Token::FocusTarget.text()
        );
        assert!(!focus::following(app.world(), root));
    }

    #[test]
    fn the_existing_toolbar_has_one_stateful_tiling_control_with_the_effective_binding() {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        app.world_mut().entity_mut(root).insert(ComputedNode {
            size: Vec2::new(1000.0, 700.0),
            inverse_scale_factor: 1.0,
            ..default()
        });
        create_controls(app.world_mut());
        create_controls(app.world_mut());
        label_controls(app.world_mut());
        let controls: Vec<_> = app
            .world_mut()
            .query::<(Entity, &TilingControl)>()
            .iter(app.world())
            .filter(|(_, control)| control.toolbar)
            .map(|(entity, control)| (entity, control.label))
            .collect();
        assert_eq!(controls.len(), 1);
        let (button, label) = controls[0];
        assert_eq!(
            app.world()
                .get::<crate::native::canvas_controls::ToolbarOrder>(button)
                .unwrap()
                .0,
            2
        );
        assert_eq!(
            app.world().get::<Text>(label).unwrap().0,
            Token::TilingOff.text()
        );
        assert_eq!(
            app.world()
                .get::<bevy::a11y::AccessibilityNode>(button)
                .unwrap()
                .role(),
            accesskit::Role::Button
        );
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: button });
        app.update();
        label_controls(app.world_mut());
        assert_eq!(
            app.world().get::<Text>(label).unwrap().0,
            Token::TilingOn.text()
        );
        app.world_mut()
            .resource_mut::<crate::native::shortcuts::Settings>()
            .set(crate::native::shortcuts::Shortcut::Tiling, "Mod+F2")
            .unwrap();
        label_controls(app.world_mut());
        assert!(
            app.world()
                .get::<crate::native::icons::Tooltip>(button)
                .unwrap()
                .0
                .contains("Mod+F2")
        );
    }
}
