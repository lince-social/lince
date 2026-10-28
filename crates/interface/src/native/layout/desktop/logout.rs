use super::*;
use crate::{
    i18n::Token,
    native::{
        actions::{Action, KeyBinding, KeyBindings, Modifiers},
        edit_mode::label,
    },
};
use bevy::input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup};

#[derive(Component)]
struct Confirmation {
    overlay: Entity,
    previous: Option<Entity>,
    workspace: u64,
    status: Entity,
}

pub(super) fn request(world: &mut World, root: Entity) {
    if world.get::<Confirmation>(root).is_some() || crate::native::shortcuts::keyboard_owned(world)
    {
        return;
    }
    if let Some(reason) = CommandContext::new(world, root).unavailable(DesktopAction::Logout) {
        crate::native::notifications::report(world, "Park", reason);
        return;
    }
    let Some(workspace) = world.get::<Workspaces>(root).map(|spaces| spaces.active) else {
        return;
    };
    let previous = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let overlay = world
        .spawn((
            Name::new("park.logout.confirmation"),
            crate::native::inspection::InspectionExcluded,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            GlobalZIndex(100),
            TabGroup::modal(),
            ChildOf(root),
            KeyBindings(vec![KeyBinding::new(
                KeyCode::Escape,
                Modifiers::NONE,
                crate::native::actions![Decision(false)],
            )]),
        ))
        .id();
    let mut accessible = accesskit::Node::new(accesskit::Role::Dialog);
    accessible.set_label(Token::ParkLogoutTitle.text());
    world
        .entity_mut(overlay)
        .insert(bevy::a11y::AccessibilityNode::from(accessible));
    let panel = world
        .spawn((
            Node {
                width: px(400),
                max_width: percent(96),
                max_height: percent(96),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                row_gap: px(12),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ChildOf(overlay),
        ))
        .id();
    for (text, size) in [
        (Token::ParkLogoutTitle.text(), 22.0),
        (Token::ParkLogoutDescription.text(), 14.0),
    ] {
        let text = label(world, panel, text, size);
        let mut node = world.get_mut::<Node>(text).unwrap();
        node.width = percent(100);
        node.min_width = px(0);
        world.entity_mut(text).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
    }
    let status = input_settings::feedback(world, panel);
    let (cancel, _) =
        input_settings::button(world, panel, overlay, Token::Cancel.text(), Decision(false));
    input_settings::button(
        world,
        panel,
        overlay,
        Token::ShortcutLogout.text(),
        Decision(true),
    );
    world.entity_mut(root).insert(Confirmation {
        overlay,
        previous,
        workspace,
        status,
    });
    crate::native::shortcuts::own_keyboard(world, overlay);
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.set(cancel, FocusCause::Navigated);
    }
}

struct Decision(bool);

impl Action for Decision {
    fn apply(&self, world: &mut World, target: Entity) {
        let Some(root) = super::root(world, target) else {
            return;
        };
        let Some(confirmation) = world.get::<Confirmation>(root) else {
            return;
        };
        let overlay = confirmation.overlay;
        let status = confirmation.status;
        if target != overlay {
            return;
        }
        if !world
            .get_resource::<crate::native::shortcuts::KeyboardModalOwner>()
            .is_some_and(|owner| owner.0 == Some(overlay))
        {
            return;
        }
        if self.0
            && !host_request(
                world,
                crate::host::HostRequest::Action(DesktopAction::Logout),
            )
        {
            if let Some(mut text) = world.get_mut::<Text>(status) {
                text.0 = Token::ParkLogoutBusy.text().into();
            }
            return;
        }
        let confirmation = world.entity_mut(root).take::<Confirmation>().unwrap();
        crate::native::shortcuts::release_keyboard(world, confirmation.overlay);
        world.despawn(confirmation.overlay);
        let restore = confirmation
            .previous
            .filter(|entity| world.get_entity(*entity).is_ok())
            .filter(|_| {
                world
                    .get::<Workspaces>(root)
                    .is_some_and(|spaces| spaces.active == confirmation.workspace)
            });
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            if let Some(entity) = restore {
                focus.set(entity, FocusCause::Navigated);
            } else {
                focus.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests;
