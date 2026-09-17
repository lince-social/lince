use super::*;

pub(super) fn selected(world: &mut World, root: Entity) {
    let Some(session) = world.get::<Session>(root) else {
        return;
    };
    if session.hidden
        || session.completed
        || !world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled && mode.areas)
    {
        return;
    }
    let Some(selected) = world
        .get::<crate::area_panel::AreaEditor>(root)
        .and_then(|editor| editor.selected)
    else {
        return;
    };
    if session.existing.contains(&selected) || owned(world, root, Some(selected)).is_none() {
        return;
    }
    let mut session = world.get_mut::<Session>(root).unwrap();
    match session.step {
        0 if Some(selected) != session.force && Some(selected) != session.change => {
            session.spawn = Some(selected)
        }
        1 if Some(selected) != session.spawn && Some(selected) != session.change => {
            session.force = Some(selected)
        }
        3 if Some(selected) != session.spawn
            && Some(selected) != session.force
            && Some(selected) != session.change =>
        {
            session.change = Some(selected);
            session.entered = false;
            session.left = false;
        }
        _ => {}
    }
}

pub(super) fn matching(world: &World, root: Entity, area: &InfluenceArea) -> Result<(), String> {
    let session = world.get::<Session>(root).unwrap();
    let title = format!("{} 1", session.sample_prefix);
    if area.filter.is_some()
        || area.change_filter.is_some()
        || area.rules.len() != 1
        || !area.rules.first().is_some_and(|rule| {
            (rule.property == crate::area::Property::Title && rule.value == title)
                || (rule.property == crate::area::Property::Identity
                    && session.records.first() == Some(&rule.value))
        })
    {
        return Err(format!(
            "Under Filter, keep one property: choose Title and type {title} in Equals. This limits the area to sample 1."
        ));
    }
    Ok(())
}
