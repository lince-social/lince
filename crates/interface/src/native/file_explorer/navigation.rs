use super::{
    FileExplorer, View,
    actions::{Control, reset},
};
use crate::native::{actions::Action, laboratory::blocked, sand_panel::status};
use bevy::{input_focus::InputFocus, prelude::*, text::EditableText};
use std::{mem::replace, path::PathBuf};

pub(super) fn enter(world: &mut World, owner: Entity, directory: Option<PathBuf>, remember: bool) {
    let (Some(view), Some(explorer)) = (world.get::<View>(owner), world.get::<FileExplorer>(owner))
    else {
        return;
    };
    if directory
        .as_ref()
        .is_some_and(|path| !view.scopes.keys().any(|root| path.starts_with(root)))
    {
        return;
    }
    let roots = if directory.is_none() {
        explorer.roots.clone()
    } else {
        Vec::new()
    };
    reset(world, owner);
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    let previous = replace(&mut view.directory, directory);
    if remember && previous != view.directory {
        if view.history.len() == 64 {
            view.history.remove(0);
        }
        view.history.push(previous);
    }
    view.expanded.clear();
    let directory = view.directory.clone();
    view.expanded.extend(directory.into_iter().chain(roots));
    view.selected = None;
    view.confirmation = None;
    let viewport = view.viewport;
    if let Some(mut scroll) = world.get_mut::<ScrollPosition>(viewport) {
        scroll.y = 0.0;
    }
}

pub(super) fn navigate(world: &mut World, owner: Entity, action: &Control) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    match action {
        Control::Back => {
            if let Some(previous) = world
                .get_mut::<View>(owner)
                .and_then(|mut view| view.history.pop())
            {
                enter(world, owner, previous, false);
            }
        }
        Control::Up => {
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
        Control::OpenFolder => {
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
    use KeyCode::{
        AltLeft, AltRight, ArrowDown, ArrowLeft, ArrowRight, ArrowUp, ControlLeft, ControlRight,
        End, Enter, F5, Home, SuperLeft, SuperRight,
    };

    let Some(focused) = world.get_resource::<InputFocus>().and_then(InputFocus::get) else {
        return;
    };
    if world.get::<EditableText>(focused).is_some() {
        return;
    }
    let mut owner = focused;
    let view = loop {
        if let Some(view) = world.get::<View>(owner) {
            break view;
        }
        let Some(parent) = world.get::<ChildOf>(owner) else {
            return;
        };
        owner = parent.parent();
    };
    if blocked(world, owner) {
        return;
    }
    let (Some(keys), Some(explorer)) = (
        world.get_resource::<ButtonInput<KeyCode>>(),
        world.get::<FileExplorer>(owner),
    ) else {
        return;
    };
    if keys.any_pressed([ControlLeft, ControlRight, SuperLeft, SuperRight]) {
        return;
    }
    let alt = keys.any_pressed([AltLeft, AltRight]);
    let action = if alt && keys.just_pressed(ArrowLeft) {
        Some(Control::Back)
    } else if alt && keys.just_pressed(ArrowUp) {
        Some(Control::Up)
    } else if keys.just_pressed(F5) {
        Some(Control::Refresh)
    } else {
        None
    };
    if let Some(action) = action {
        action.apply(world, owner);
        return;
    }
    let Some(key) = [ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Home, End, Enter]
        .into_iter()
        .find(|key| keys.just_pressed(*key))
    else {
        return;
    };
    let Some(last) = view.rows.len().checked_sub(1) else {
        return;
    };
    let index = view.selected.as_ref().and_then(|selected| {
        view.rows
            .iter()
            .position(|row| row.entry.path == selected.entry.path)
    });
    let grid = explorer.grid;
    let columns = if grid {
        view.shown.map_or(1, |shown| shown.1.max(1))
    } else {
        1
    };
    let selected = index.and_then(|index| view.rows.get(index));
    if let Some(row) = selected.filter(|row| match key {
        Enter => true,
        ArrowRight if !grid && row.entry.directory => !view.expanded.contains(&row.entry.path),
        ArrowLeft if !grid && row.entry.directory => view.expanded.contains(&row.entry.path),
        _ => false,
    }) {
        Control::Entry(row.clone()).apply(world, owner);
        return;
    }
    let next = match key {
        Home => 0,
        End => last,
        ArrowUp => index.unwrap_or(0).saturating_sub(columns),
        ArrowDown => index.map_or(0, |index| index.saturating_add(columns).min(last)),
        ArrowRight if grid => index.map_or(0, |index| index.saturating_add(1).min(last)),
        ArrowLeft if grid => index.unwrap_or(0).saturating_sub(1),
        ArrowRight => index.unwrap_or(0).saturating_add(1).min(last),
        ArrowLeft => selected
            .and_then(|row| row.entry.path.parent())
            .and_then(|parent| view.rows.iter().position(|row| row.entry.path == parent))
            .unwrap_or(0),
        _ => return,
    };
    let Some(row) = view.rows.get(next).cloned() else {
        return;
    };
    let viewport = view.viewport;
    let label = view.selected_label;
    status(world, label, row.entry.path.display().to_string());
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
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
