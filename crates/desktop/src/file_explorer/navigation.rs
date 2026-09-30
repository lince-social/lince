use super::*;
use crate::actions::Action;
use bevy::input_focus::InputFocus;

pub(super) fn enter(world: &mut World, owner: Entity, directory: Option<PathBuf>, remember: bool) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    if directory
        .as_ref()
        .is_some_and(|path| !view.scopes.keys().any(|root| path.starts_with(root)))
    {
        return;
    }
    let roots = world.get::<FileExplorer>(owner).unwrap().roots.clone();
    actions::reset(world, owner);
    let mut view = world.get_mut::<View>(owner).unwrap();
    if remember && directory != view.directory {
        let previous = view.directory.clone();
        if view.history.len() == 64 {
            view.history.remove(0);
        }
        view.history.push(previous);
    }
    view.directory = directory.clone();
    view.expanded.clear();
    let roots = if view.directory.is_none() {
        roots
    } else {
        Vec::new()
    };
    view.expanded.extend(directory.into_iter().chain(roots));
    view.selected = None;
    view.confirmation = None;
    let viewport = view.viewport;
    world.get_mut::<ScrollPosition>(viewport).unwrap().y = 0.0;
}

pub(super) fn navigate(world: &mut World, owner: Entity, action: &actions::Control) {
    let view = world.get::<View>(owner).unwrap();
    match action {
        actions::Control::Back => {
            if let Some(previous) = world.get_mut::<View>(owner).unwrap().history.pop() {
                enter(world, owner, previous, false);
            }
        }
        actions::Control::Up => {
            let directory = view
                .directory
                .as_ref()
                .or_else(|| view.selected.as_ref().map(|row| &row.entry.path));
            if let Some(directory) = directory {
                let parent = directory
                    .parent()
                    .filter(|parent| view.scopes.keys().any(|root| parent.starts_with(root)))
                    .map(PathBuf::from);
                enter(world, owner, parent, true);
            }
        }
        actions::Control::OpenFolder => {
            if let Some(path) = view
                .selected
                .as_ref()
                .filter(|row| row.entry.directory)
                .map(|row| row.entry.path.clone())
            {
                enter(world, owner, Some(path), true);
            }
        }
        _ => {}
    }
}

pub(super) fn keyboard(world: &mut World) {
    let Some(focused) = world.get_resource::<InputFocus>().and_then(InputFocus::get) else {
        return;
    };
    if world.get::<bevy::text::EditableText>(focused).is_some() {
        return;
    }
    let mut current = Some(focused);
    let mut owner = None;
    while let Some(entity) = current {
        if world.get::<View>(entity).is_some() {
            owner = Some(entity);
            break;
        }
        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    let Some(owner) = owner else {
        return;
    };
    if crate::laboratory::suspended(world, owner) {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    if keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]) {
        return;
    }
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let action = if alt && keys.just_pressed(KeyCode::ArrowLeft) {
        Some(actions::Control::Back)
    } else if alt && keys.just_pressed(KeyCode::ArrowUp) {
        Some(actions::Control::Up)
    } else if keys.just_pressed(KeyCode::F5) {
        Some(actions::Control::Refresh)
    } else {
        None
    };
    if let Some(action) = action {
        action.apply(world, owner);
        return;
    }
    let Some(key) = [
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Enter,
    ]
    .into_iter()
    .find(|key| keys.just_pressed(*key)) else {
        return;
    };
    let view = world.get::<View>(owner).unwrap();
    if view.rows.is_empty() {
        return;
    }
    let index = view.selected.as_ref().and_then(|selected| {
        view.rows
            .iter()
            .position(|row| row.entry.path == selected.entry.path)
    });
    let grid = world.get::<FileExplorer>(owner).unwrap().grid;
    let columns = if grid {
        view.shown.map_or(1, |shown| shown.1)
    } else {
        1
    };
    let selected = index.map(|index| view.rows[index].clone());
    if key == KeyCode::Enter {
        if let Some(row) = selected {
            actions::Control::Entry(row).apply(world, owner);
        }
        return;
    }
    let next = match key {
        KeyCode::Home => 0,
        KeyCode::End => view.rows.len() - 1,
        KeyCode::ArrowUp => index.unwrap_or(0).saturating_sub(columns),
        KeyCode::ArrowDown => index.map_or(0, |index| (index + columns).min(view.rows.len() - 1)),
        KeyCode::ArrowRight if grid => {
            index.map_or(0, |index| (index + 1).min(view.rows.len() - 1))
        }
        KeyCode::ArrowLeft if grid => index.unwrap_or(0).saturating_sub(1),
        KeyCode::ArrowRight => {
            if let Some(row) = selected
                .filter(|row| row.entry.directory && !view.expanded.contains(&row.entry.path))
            {
                actions::Control::Entry(row).apply(world, owner);
                return;
            }
            (index.unwrap_or(0) + 1).min(view.rows.len() - 1)
        }
        KeyCode::ArrowLeft => {
            if let Some(row) = &selected {
                if row.entry.directory && view.expanded.contains(&row.entry.path) {
                    actions::Control::Entry(row.clone()).apply(world, owner);
                    return;
                }
                row.entry
                    .path
                    .parent()
                    .and_then(|parent| view.rows.iter().position(|row| row.entry.path == parent))
                    .unwrap_or(0)
            } else {
                0
            }
        }
        _ => return,
    };
    let row = view.rows[next].clone();
    let viewport = view.viewport;
    let label = view.selected_label;
    crate::sand_panel::status(world, label, row.entry.path.display().to_string());
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.selected = Some(row);
    view.confirmation = None;
    view.shown = None;
    let height = if grid { 76.0 } else { 30.0 };
    let top = (next / columns) as f32 * height;
    let visible = world
        .get::<ComputedNode>(viewport)
        .map_or(300.0, |node| node.size().y * node.inverse_scale_factor());
    if let Some(mut scroll) = world.get_mut::<ScrollPosition>(viewport) {
        if top < scroll.y {
            scroll.y = top;
        } else if top + height > scroll.y + visible {
            scroll.y = (top + height - visible).max(0.0);
        }
    }
}
