use super::*;
use crate::{host::WindowInspection, native::protein_area::RecordBinding};

pub(super) enum Target {
    Native {
        workspace: u64,
        position: bevy::math::DVec2,
        binding: RecordBinding,
    },
    Application(WindowInspection),
}

pub(super) fn capture(world: &World, root: Entity) -> Result<Target, &'static str> {
    let spaces = world
        .get::<Workspaces>(root)
        .ok_or("The workspace is not ready.")?;
    let space = spaces
        .entries
        .iter()
        .find(|space| space.id == spaces.active)
        .ok_or("The workspace is not ready.")?;
    let entity = world
        .get::<Children>(root)
        .and_then(|children| {
            children.iter().find(|entity| {
                world
                    .get::<Managed>(*entity)
                    .is_some_and(|managed| Some(managed.id) == space.desktop.focused())
                    && focus::eligible(world, root, space.id, *entity)
            })
        })
        .ok_or("Focus an item with a Record to inspect its properties.")?;
    let position = world
        .get::<CanvasView>(root)
        .map_or(bevy::math::DVec2::ZERO, |view| view.center);
    if !position.is_finite() || position.abs().max_element() > 1_000_000_000.0 {
        return Err("The current view position is unavailable.");
    }
    let binding = world
        .get::<RecordBinding>(entity)
        .filter(|binding| nucleus::valid_uid(&binding.uid, "r"));
    if let Some(window) = world.get::<ApplicationWindow>(entity) {
        if !world.contains_resource::<HostRequests>() {
            return Err("This application command requires a Park session.");
        }
        if world.get::<TransientParent>(entity).is_some() && binding.is_none() {
            return Err("This dialog does not have its own Record association.");
        }
        return Ok(Target::Application(WindowInspection {
            owner: root.to_bits(),
            workspace: space.id,
            position: position.to_array(),
            surface: window.surface,
            placement: world.get::<Managed>(entity).unwrap().id.0,
        }));
    }
    Ok(Target::Native {
        workspace: space.id,
        position,
        binding: binding
            .cloned()
            .ok_or("This item does not have a valid Record binding.")?,
    })
}

pub(super) fn request(world: &mut World, root: Entity) {
    let result = match capture(world, root) {
        Ok(Target::Native {
            workspace,
            position,
            binding,
        }) => crate::native::full_record::open_in(
            world,
            root,
            workspace,
            position,
            &binding.uid,
            binding.source,
        )
        .map(|_| ())
        .ok_or("The original workspace is unavailable."),
        Ok(Target::Application(target)) => {
            host_request(world, crate::host::HostRequest::Inspect(target));
            Ok(())
        }
        Err(reason) => Err(reason),
    };
    if let Err(reason) = result {
        crate::native::notifications::report(world, "Window properties", reason);
    }
}

#[cfg(test)]
mod tests;
