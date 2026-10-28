use super::*;
use crate::i18n::Token;

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

fn caption(world: &World, root: Entity) -> &'static str {
    let tiled = world
        .get::<Workspaces>(root)
        .and_then(|spaces| {
            spaces
                .entries
                .iter()
                .find(|space| space.id == spaces.active)
        })
        .is_some_and(|space| space.desktop.mode() != WorkspaceLayout::Free);
    if tiled {
        Token::TilingOn.text()
    } else {
        Token::TilingOff.text()
    }
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
    let roots: Vec<_> = world
        .query_filtered::<Entity, (
            With<Workspaces>,
            With<crate::native::edit_mode::EditMode>,
            With<CanvasView>,
        )>()
        .iter(world)
        .collect();
    for root in roots {
        if !world
            .query::<&OverviewControl>()
            .iter(world)
            .any(|control| control.root == root)
        {
            let toolbar = crate::native::canvas_controls::toolbar(world, root);
            let entity = world
                .spawn((
                    crate::native::sand::button(0),
                    crate::native::actions::ActionButton::new(
                        root,
                        crate::native::actions![DesktopAction::ToggleOverview],
                    ),
                    crate::native::shortcuts::KeyboardModalTrigger,
                    crate::native::canvas_controls::ToolbarOrder(3),
                    OverviewControl { root },
                    Node {
                        height: px(40),
                        padding: UiRect::axes(px(10), px(8)),
                        align_items: AlignItems::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    crate::native::token_style::background(crate::native::tokens::Token::Surface),
                    ChildOf(toolbar),
                ))
                .id();
            crate::native::edit_mode::label(world, entity, Token::ShortcutOverview.text(), 14.0);
        }
        if !world
            .query::<&FocusControl>()
            .iter(world)
            .any(|control| control.root == root)
        {
            let toolbar = crate::native::canvas_controls::toolbar(world, root);
            focus_button(world, root, toolbar);
        }
        if world
            .query::<&TilingControl>()
            .iter(world)
            .any(|control| control.root == root && control.toolbar)
        {
            continue;
        }
        let toolbar = crate::native::canvas_controls::toolbar(world, root);
        button(world, root, toolbar, true);
    }
}

pub(crate) fn label_controls(world: &mut World) {
    let controls: Vec<_> = world
        .query_filtered::<Entity, With<OverviewControl>>()
        .iter(world)
        .collect();
    let chord = world
        .get_resource::<crate::native::shortcuts::Settings>()
        .cloned()
        .unwrap_or_default()
        .text(crate::native::shortcuts::Shortcut::Overview)
        .to_owned();
    for entity in controls {
        let help = format!("{} · {chord}", Token::ShortcutOverview.text());
        if world
            .get::<crate::native::icons::Tooltip>(entity)
            .is_none_or(|tooltip| tooltip.0 != help)
        {
            world
                .entity_mut(entity)
                .insert(crate::native::icons::Tooltip(help.clone()));
        }
        if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
            accessibility.set_label(help);
        }
    }
    let controls: Vec<_> = world
        .query::<(Entity, &FocusControl)>()
        .iter(world)
        .map(|(entity, control)| (entity, control.root, control.label))
        .collect();
    for (entity, root, label) in controls {
        focus::remember_selection(world, root);
        let (focused, available) = focus::status(world, root);
        let value = if focused {
            Token::UnfocusTarget.text()
        } else {
            Token::FocusTarget.text()
        };
        if let Some(mut text) = world.get_mut::<Text>(label)
            && text.0 != value
        {
            text.0 = value.into();
        }
        if available {
            world
                .entity_mut(entity)
                .remove::<bevy::ui::InteractionDisabled>();
        } else if world.get::<bevy::ui::InteractionDisabled>(entity).is_none() {
            world
                .entity_mut(entity)
                .insert(bevy::ui::InteractionDisabled);
        }
        let chord = world
            .get_resource::<crate::native::shortcuts::Settings>()
            .cloned()
            .unwrap_or_default()
            .text(crate::native::shortcuts::Shortcut::Unfocus)
            .to_owned();
        let help = format!("{value} · {chord}");
        if world
            .get::<crate::native::icons::Tooltip>(entity)
            .is_none_or(|tooltip| tooltip.0 != help)
        {
            world
                .entity_mut(entity)
                .insert(crate::native::icons::Tooltip(help.clone()));
        }
        if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
            accessibility.set_label(help);
        }
    }
    let controls: Vec<_> = world
        .query::<(Entity, &TilingControl)>()
        .iter(world)
        .map(|(entity, control)| (entity, control.root, control.label))
        .collect();
    for (entity, root, label) in controls {
        let value = caption(world, root);
        if let Some(mut text) = world.get_mut::<Text>(label)
            && text.0 != value
        {
            text.0 = value.into();
        }
        let chord = world
            .get_resource::<crate::native::shortcuts::Settings>()
            .cloned()
            .unwrap_or_default()
            .text(crate::native::shortcuts::Shortcut::Tiling)
            .to_owned();
        let help = format!("{} · {value} · {chord}", Token::ShortcutTiling.text());
        if world
            .get::<crate::native::icons::Tooltip>(entity)
            .is_none_or(|tooltip| tooltip.0 != help)
        {
            world
                .entity_mut(entity)
                .insert(crate::native::icons::Tooltip(help.clone()));
        }
        if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
            accessibility.set_label(help);
        }
    }
}

pub(crate) fn panel(world: &mut World, root: Entity, panel: Entity) {
    button(world, root, panel, false);
}

#[cfg(test)]
mod tests {
    use super::*;

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
