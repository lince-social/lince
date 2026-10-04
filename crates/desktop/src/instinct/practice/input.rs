use super::*;
use bevy::{
    input_focus::{InputFocus, tab_navigation::TabIndex},
    ui::InteractionDisabled,
};
use lince_interface::practice::{Resolution, Target, resolve};

#[derive(Component)]
struct Disabled {
    root: Entity,
    pickable: Option<Pickable>,
    tab: Option<i32>,
}

fn restore(world: &mut World, entity: Entity) {
    let Some(state) = world.entity_mut(entity).take::<Disabled>() else {
        return;
    };
    world.entity_mut(entity).remove::<InteractionDisabled>();
    if world.get::<Pickable>(entity) == Some(&Pickable::IGNORE) {
        if let Some(pickable) = state.pickable {
            world.entity_mut(entity).insert(pickable);
        } else {
            world.entity_mut(entity).remove::<Pickable>();
        }
    }
    if world.get::<TabIndex>(entity).is_some_and(|tab| tab.0 == -1) {
        if let Some(tab) = state.tab {
            world.entity_mut(entity).insert(TabIndex(tab));
        } else {
            world.entity_mut(entity).remove::<TabIndex>();
        }
    }
}

pub(super) fn release(world: &mut World, root: Entity) {
    let entities: Vec<_> = world
        .query::<(Entity, &Disabled)>()
        .iter(world)
        .filter(|(_, state)| state.root == root)
        .map(|(entity, _)| entity)
        .collect();
    for entity in entities {
        restore(world, entity);
    }
}

fn semantic(world: &mut World, root: Entity) -> Result<Entity, Resolution> {
    let practice = world.get::<Practice>(root).unwrap();
    let operation = practice
        .runner
        .current()
        .and_then(|step| step.operation)
        .ok_or(Resolution::Missing)?;
    if operation == Operation::OpenEdit {
        let controls: Vec<_> = world
            .query::<(Entity, &crate::edit_mode::EditControl)>()
            .iter(world)
            .filter(|(_, control)| {
                control.root == root && control.action == crate::edit_mode::EditAction::Toggle
            })
            .map(|(entity, _)| entity)
            .collect();
        return match controls.as_slice() {
            [entity] => Ok(*entity),
            [] => Err(Resolution::Missing),
            _ => Err(Resolution::Ambiguous),
        };
    }
    let owner = match operation {
        Operation::MoveSand | Operation::SetAppearance | Operation::ResetAppearance => {
            Some(Role::Square)
        }
        Operation::EditText => Some(Role::Text),
        Operation::Attract
        | Operation::Repel
        | Operation::ScaleArea
        | Operation::DisableAreaEffect
        | Operation::InspectArea => Some(Role::Area),
        Operation::PreviewProtein | Operation::PresentProperties | Operation::ArrangeProtein => {
            Some(Role::Spawn)
        }
        Operation::EnterArea | Operation::LeaveArea => Some(Role::Changes),
        _ => None,
    };
    if let Some(role) = owner {
        let practice = world.get::<Practice>(root).unwrap();
        let session = practice.runner.session;
        let workspace = practice.workspace;
        let controls: Vec<_> = world
            .query::<(Entity, &Owned, &WorkspaceMember)>()
            .iter(world)
            .filter(|(entity, owned, member)| {
                owned.session == session
                    && owned.role == role
                    && member.0 == workspace
                    && super::root(world, *entity) == Some(root)
            })
            .map(|(entity, _, _)| entity)
            .collect();
        return match controls.as_slice() {
            [entity] => Ok(*entity),
            [] => Err(Resolution::Missing),
            _ => Err(Resolution::Ambiguous),
        };
    }
    let target = Target {
        window: root.to_bits(),
        workspace: practice.workspace,
        owner: practice.runner.session.to_string(),
        role: format!("{operation:?}"),
    };
    let candidates: Vec<_> = world
        .query::<(Entity, &SemanticControl)>()
        .iter(world)
        .filter(|(entity, _)| super::root(world, *entity) == Some(root))
        .map(|(entity, control)| {
            (
                Target {
                    window: root.to_bits(),
                    workspace: target.workspace,
                    owner: control.session.to_string(),
                    role: format!("{:?}", control.operation),
                },
                entity,
            )
        })
        .collect();
    resolve(&target, candidates)
}

pub(crate) fn refresh(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Practice>>()
        .iter(world)
        .collect();
    for root in roots {
        let resolution = semantic(world, root);
        world
            .get_mut::<Practice>(root)
            .unwrap()
            .runner
            .set_target(resolution.map(|_| ()));
        if visible(world, root)
            && let Ok(target) = resolution
        {
            crate::tutorial::highlight::update_entities(world, root, vec![target]);
        } else {
            crate::tutorial::highlight::clear(world, root);
        }
        if !world.get::<Practice>(root).unwrap().runner.restricted() || !visible(world, root) {
            release(world, root);
            if !visible(world, root) {
                crate::tutorial::highlight::clear(world, root);
            }
            continue;
        }
        let candidates: Vec<_> = world
            .query_filtered::<Entity, Or<(
                With<crate::actions::ActionButton>,
                With<bevy::text::EditableText>,
                With<CanvasItem>,
                With<bevy::ui_widgets::Button>,
                With<crate::slider::SliderSand>,
            )>>()
            .iter(world)
            .filter(|entity| super::root(world, *entity) == Some(root))
            .collect();
        for entity in candidates {
            let allowed = permits_target(world, entity)
                || world
                    .get::<crate::actions::ActionButton>(entity)
                    .is_some_and(|button| button.actions.permitted(world, button.target));
            if allowed {
                if world.get::<Disabled>(entity).is_some() {
                    restore(world, entity);
                }
            } else if world.get::<Disabled>(entity).is_none()
                && world.get::<InteractionDisabled>(entity).is_none()
            {
                let state = Disabled {
                    root,
                    pickable: world.get::<Pickable>(entity).copied(),
                    tab: world.get::<TabIndex>(entity).map(|tab| tab.0),
                };
                world.entity_mut(entity).insert((
                    state,
                    InteractionDisabled,
                    Pickable::IGNORE,
                    TabIndex(-1),
                ));
            }
        }
        if world
            .get_resource::<InputFocus>()
            .and_then(InputFocus::get)
            .is_some_and(|entity| {
                super::root(world, entity) == Some(root) && world.get::<Disabled>(entity).is_some()
            })
        {
            world.resource_mut::<InputFocus>().clear();
        }
    }
}
