use super::*;

pub(super) fn panel(world: &mut World, owner: Entity) -> Entity {
    let panel = crate::sand_panel::row(world, owner);
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    crate::edit_mode::label(world, panel, "Save changes before closing?", 14.0);
    for (caption, action) in [
        ("Save and close", actions::Control::SaveClose),
        ("Discard changes", actions::Control::DiscardClose),
        ("Cancel", actions::Control::CancelClose),
    ] {
        crate::sand_panel::button(world, panel, owner, caption, action);
    }
    panel
}

pub(super) fn show(world: &mut World, owner: Entity, path: PathBuf) {
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.closing = Some(path);
    view.close_waiting = false;
    let panel = view.close_panel;
    world.get_mut::<Node>(panel).unwrap().display = Display::Flex;
}

pub(super) fn cancel(world: &mut World, owner: Entity) {
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.closing = None;
    view.close_waiting = false;
    let panel = view.close_panel;
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
}

pub(super) fn apply(world: &mut World, owner: Entity, save: bool) {
    editing::capture_one(world, owner);
    let view = world.get::<View>(owner).unwrap();
    if save
        && (view.draft
            || world
                .get::<EditableText>(view.editor)
                .is_some_and(|input| input.is_composing()))
    {
        status(
            world,
            owner,
            "Finish composing or resolve the visible draft before saving",
        );
        return;
    }
    let Some(path) = world.get::<View>(owner).unwrap().closing.clone() else {
        return;
    };
    let Some(doc) = world.resource::<Documents>().0.get(&path) else {
        cancel(world, owner);
        return;
    };
    if doc.saving.is_some() || doc.moving || doc.reading {
        status(world, owner, "Wait for file access before closing");
        return;
    }
    if save {
        world.get_mut::<View>(owner).unwrap().close_waiting = true;
        actions::save(world, owner, path);
    } else {
        let another = world
            .query::<(Entity, &Ide)>()
            .iter(world)
            .any(|(other, ide)| other != owner && ide.paths.contains(&path));
        if !another {
            world.resource_mut::<Documents>().0.remove(&path);
        }
        actions::close_tab(world, owner, &path);
        cancel(world, owner);
    }
}

pub(super) fn update(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    if !view.close_waiting {
        return;
    }
    let Some(path) = view.closing.clone() else {
        return;
    };
    let Some(doc) = world.resource::<Documents>().0.get(&path) else {
        cancel(world, owner);
        return;
    };
    if doc.saving.is_some() {
        return;
    }
    if doc.buffer.is_dirty() || doc.buffer.conflict().is_some() || doc.file.is_none() || view.draft
    {
        world.get_mut::<View>(owner).unwrap().close_waiting = false;
        return;
    }
    actions::close_tab(world, owner, &path);
    cancel(world, owner);
}
