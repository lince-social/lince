use super::*;
use std::collections::HashMap;

#[derive(Component)]
pub(super) struct Row {
    pub(super) workspace: u64,
    pub(super) number: Entity,
}

pub(super) fn reorder(world: &mut World, root: Entity) {
    let Some(mode) = world.get::<EditMode>(root).filter(|mode| mode.enabled) else {
        return;
    };
    let panel = mode.panel;
    let Some(children) = world.get::<Children>(panel) else {
        return;
    };
    let children: Vec<_> = children.iter().collect();
    let rows: HashMap<_, _> = children
        .iter()
        .filter_map(|entity| {
            world
                .get::<Row>(*entity)
                .map(|row| (row.workspace, *entity))
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return;
    };
    let Some(ordered) = spaces
        .entries
        .iter()
        .map(|entry| rows.get(&entry.id).copied())
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    if ordered.len() != rows.len() {
        return;
    }
    let numbers = workspace::numbers(world, root);
    for (id, entity) in &rows {
        let label = world.get::<Row>(*entity).unwrap().number;
        let value = numbers
            .iter()
            .position(|number| number == id)
            .map_or_else(|| "·".to_owned(), |index| (index + 1).to_string());
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = value;
        }
    }
    let mut ordered = ordered.into_iter();
    let children: Vec<_> = children
        .into_iter()
        .map(|entity| {
            if world.get::<Row>(entity).is_some() {
                ordered.next().unwrap()
            } else {
                entity
            }
        })
        .collect();
    world.entity_mut(panel).replace_children(&children);
    let revision = world
        .get::<FocusScrollRevision>(panel)
        .map_or(0, |revision| revision.0);
    world
        .entity_mut(panel)
        .insert(FocusScrollRevision(revision.wrapping_add(1)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reordered_rows_retain_draft_entities_caret_focus_scroll_and_fixed_panel_children() {
        let (mut app, root) = super::super::tests::fixture();
        workspace::create(app.world_mut(), root);
        EditAction::Open.apply(app.world_mut(), root);
        let mode = app.world().get::<EditMode>(root).unwrap();
        let panel = mode.panel;
        let field = mode.name.unwrap();
        let toggle = mode.toggle;
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("Unsettled name");
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        let scroll = Vec2::new(0.0, 17.0);
        app.world_mut().get_mut::<ScrollPosition>(panel).unwrap().0 = scroll;
        let children: Vec<_> = app.world().get::<Children>(panel).unwrap().iter().collect();
        let rows: Vec<_> = children
            .iter()
            .filter(|entity| app.world().get::<Row>(**entity).is_some())
            .copied()
            .collect();
        let before_caret = {
            let input = app.world().get::<EditableText>(field).unwrap();
            [
                input.editor.raw_selection().anchor().index(),
                input.editor.raw_selection().focus().index(),
            ]
        };
        workspace::navigation::reorder(app.world_mut(), root, -1).unwrap();
        assert_eq!(workspace::numbers(app.world(), root), vec![2, 1]);
        assert_eq!(app.world().get::<EditMode>(root).unwrap().name, Some(field));
        assert_eq!(app.world().get::<EditMode>(root).unwrap().toggle, toggle);
        assert_eq!(
            app.world().get::<EditableText>(field).unwrap().value(),
            "Unsettled name"
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        assert_eq!(app.world().get::<ScrollPosition>(panel).unwrap().0, scroll);
        let input = app.world().get::<EditableText>(field).unwrap();
        assert_eq!(
            [
                input.editor.raw_selection().anchor().index(),
                input.editor.raw_selection().focus().index()
            ],
            before_caret
        );
        let after: Vec<_> = app.world().get::<Children>(panel).unwrap().iter().collect();
        let after_rows: Vec<_> = after
            .iter()
            .filter(|entity| app.world().get::<Row>(**entity).is_some())
            .copied()
            .collect();
        assert_eq!(after_rows, vec![rows[1], rows[0]]);
        for (before, after) in children.iter().zip(after.iter()) {
            if app.world().get::<Row>(*before).is_none() {
                assert_eq!(before, after);
            }
        }
        for (index, entity) in after_rows.iter().enumerate() {
            let label = app.world().get::<Row>(*entity).unwrap().number;
            assert_eq!(
                app.world().get::<Text>(label).unwrap().0,
                (index + 1).to_string()
            );
        }
        app.update();
        assert_eq!(
            app.world().get::<Workspaces>(root).unwrap().entries[0].name,
            "Unsettled name"
        );
        assert_ne!(
            app.world().get::<Workspaces>(root).unwrap().entries[1].name,
            "Unsettled name"
        );
    }
}
