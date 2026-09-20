use super::*;
use crate::actions::{KeyBinding, KeyBindings, Modifiers};
use bevy::input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup};

#[derive(Clone)]
pub(super) struct Add;

impl Action for Add {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(castle) = world.get::<ThreadCastle>(owner) else {
            return;
        };
        if castle.creating {
            return;
        }
        let binding = castle.binding.clone();
        let status = castle.status;
        match crate::protein_area::execute(
            world,
            &binding,
            owner,
            engine::actions::Action::CreateThread {
                target: binding.uid.clone(),
                head: String::new(),
            },
        ) {
            Ok(()) => {
                let mut castle = world.get_mut::<ThreadCastle>(owner).unwrap();
                castle.creating = true;
                world.get_mut::<Text>(status).unwrap().0 = "Creating thread…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

#[derive(Component)]
struct Confirmation {
    binding: RecordBinding,
    thread: String,
    status: Entity,
    previous: Option<Entity>,
    pending: bool,
}

#[derive(Clone)]
pub(super) struct AskDelete;

impl Action for AskDelete {
    fn apply(&self, world: &mut World, entity: Entity) {
        if world.query::<&Confirmation>().iter(world).next().is_some() {
            return;
        }
        let Some(page) = world.get::<Page>(entity) else {
            return;
        };
        let thread = page.uid.clone();
        let binding = world
            .get::<ThreadCastle>(page.castle)
            .unwrap()
            .binding
            .clone();
        request(world, page.castle, binding, thread);
    }
}

pub(super) fn request(world: &mut World, owner: Entity, binding: RecordBinding, thread: String) {
    if world.query::<&Confirmation>().iter(world).next().is_some() {
        return;
    }
    let mut root = owner;
    while let Some(parent) = world.get::<ChildOf>(root) {
        root = parent.parent();
    }
    let previous = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let overlay = world
        .spawn((
            crate::inspection::InspectionExcluded,
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
                crate::actions![Decide(false)],
            )]),
        ))
        .id();
    let panel = world
        .spawn((
            Node {
                width: px(400),
                max_width: percent(95),
                padding: UiRect::all(px(16)),
                row_gap: px(12),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            ChildOf(overlay),
        ))
        .id();
    crate::edit_mode::label(world, panel, "Delete this thread?", 22.0);
    crate::edit_mode::label(
        world,
        panel,
        "This removes the thread and its conversation from this Record.",
        14.0,
    );
    let status = crate::edit_mode::label(world, panel, "", 14.0);
    let cancel = control(world, panel, overlay, "Cancel", Decide(false));
    control(world, panel, overlay, "Delete thread", Decide(true));
    world.entity_mut(overlay).insert(Confirmation {
        binding,
        thread,
        status,
        previous,
        pending: false,
    });
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.set(cancel, FocusCause::Navigated);
    }
}

fn close(world: &mut World, entity: Entity) {
    let previous = world
        .get::<Confirmation>(entity)
        .and_then(|modal| modal.previous)
        .filter(|previous| world.get_entity(*previous).is_ok());
    world.despawn(entity);
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        if let Some(previous) = previous {
            focus.set(previous, FocusCause::Navigated);
        } else {
            focus.clear();
        }
    }
}

#[derive(Clone)]
struct Decide(bool);

impl Action for Decide {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(modal) = world.get::<Confirmation>(entity) else {
            return;
        };
        if modal.pending {
            return;
        }
        if !self.0 {
            close(world, entity);
            return;
        }
        let (binding, thread, status) = (modal.binding.clone(), modal.thread.clone(), modal.status);
        match crate::protein_area::execute(
            world,
            &binding,
            entity,
            engine::actions::Action::DeleteRecord { target: thread },
        ) {
            Ok(()) => {
                world.get_mut::<Confirmation>(entity).unwrap().pending = true;
                world.get_mut::<Text>(status).unwrap().0 = "Deleting…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

pub(super) fn finished(world: &mut World, entity: Entity, error: Option<String>) -> bool {
    if let Some(mut castle) = world.get_mut::<ThreadCastle>(entity) {
        castle.creating = false;
        if error.is_some() {
            castle.select_new = None;
        }
        let status = castle.status;
        world.get_mut::<Text>(status).unwrap().0 = error.unwrap_or_default();
        return true;
    }
    if let Some(mut modal) = world.get_mut::<Confirmation>(entity) {
        modal.pending = false;
        let status = modal.status;
        if let Some(error) = error {
            world.get_mut::<Text>(status).unwrap().0 = error;
        } else {
            close(world, entity);
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_requires_confirmation_and_preserves_thread_on_cancel_or_error() {
        let (mut world, castle, _) = super::super::tests::fixture();
        world.init_resource::<InputFocus>();
        let page = world.get::<ThreadCastle>(castle).unwrap().pages["thread-a"];
        AskDelete.apply(&mut world, page);
        let modal = world
            .query_filtered::<Entity, With<Confirmation>>()
            .single(&world)
            .unwrap();
        assert!(world.get::<TabGroup>(modal).is_some());
        assert_eq!(world.get::<Confirmation>(modal).unwrap().thread, "thread-a");
        Decide(false).apply(&mut world, modal);
        assert!(world.get_entity(modal).is_err());
        assert!(world.get_entity(page).is_ok());
        AskDelete.apply(&mut world, page);
        let modal = world
            .query_filtered::<Entity, With<Confirmation>>()
            .single(&world)
            .unwrap();
        assert!(finished(
            &mut world,
            modal,
            Some("Permission denied".into())
        ));
        assert!(world.get_entity(page).is_ok());
        assert!(world.get_entity(modal).is_ok());
        let status = world.get::<Confirmation>(modal).unwrap().status;
        assert_eq!(world.get::<Text>(status).unwrap().0, "Permission denied");
    }
}
