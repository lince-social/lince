// non-lean-yet
// Scope: struct Controls, fn controls, enum Confirmation, struct Form.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (16, 19, 144, 165).

use super::*;
use crate::native::retained::Presentation;
use std::borrow::Cow;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Controls {
    busy: bool,
    creating: bool,
    editing: bool,
    selected: bool,
    can_pause: bool,
}

pub(super) fn controls(world: &mut World, owner: Entity) {
    let _prepare = nucleus::performance::Span::new("karma-control-presentation");
    let view = world.get::<View>(owner).unwrap();
    let controls = view.controls;
    let busy = view.pending.is_some();
    let castle = world.get::<KarmaCastle>(owner).unwrap();
    let creating = castle.draft.is_some();
    let editing = !castle.edits.is_empty();
    let (mut selected, mut can_pause) = (false, false);
    if !creating && !editing && !view.selected.is_empty() {
        let query = castle.search.trim().to_lowercase();
        for rule in view
            .rules
            .iter()
            .filter(|rule| view.selected.contains(&rule.uid))
        {
            if matches_rule(rule, &query) {
                selected = true;
                can_pause |= rule.state != "paused";
                if can_pause {
                    break;
                }
            }
        }
    }
    let state = Controls {
        busy,
        creating,
        editing,
        selected,
        can_pause,
    };
    if Presentation::matches(world, controls, |old: &Controls| *old == state) {
        return;
    }
    clear(world, controls);
    if selected && !creating && !editing {
        for (command, label) in [
            (Command::PauseSelected, "Pause selected"),
            (Command::DeleteSelected, "Delete selected"),
        ] {
            let disabled = busy || (matches!(command, Command::PauseSelected) && !can_pause);
            let entity = button(world, controls, owner, command, label);
            if disabled {
                world
                    .entity_mut(entity)
                    .insert(bevy::ui::InteractionDisabled);
            }
        }
    }
    if creating || editing {
        for (command, title) in [
            (Command::Save, if creating { "Create" } else { "Save" }),
            (Command::Cancel, "Cancel"),
        ] {
            let entity = button(world, controls, owner, command, title);
            if busy {
                world
                    .entity_mut(entity)
                    .insert(bevy::ui::InteractionDisabled);
            }
        }
    } else {
        icon_button(
            world,
            controls,
            owner,
            Icon::Plus,
            "Create a rule",
            Command::New,
            !busy,
        );
    }
    Presentation::remember(world, controls, state);
    nucleus::performance::add("karma-controls-rebuilt", 1);
}

#[derive(PartialEq, Eq)]
enum Confirmation<'a> {
    One(Cow<'a, str>),
    Many(usize),
}

impl<'a> Confirmation<'a> {
    fn new(count: usize, name: &'a str) -> Self {
        if count == 1 {
            Self::One(name.into())
        } else {
            Self::Many(count)
        }
    }

    fn into_owned(self) -> Confirmation<'static> {
        match self {
            Self::One(name) => Confirmation::One(name.into_owned().into()),
            Self::Many(count) => Confirmation::Many(count),
        }
    }

    fn message(&self, verb: &str) -> String {
        match self {
            Self::One(name) => format!("{verb} {name}?"),
            Self::Many(count) => format!("{verb} {count} selected rules?"),
        }
    }
}

#[derive(PartialEq, Eq)]
struct Form<'a> {
    draft: Option<Cow<'a, Draft>>,
    busy: bool,
    deleting: Option<Confirmation<'a>>,
    pausing: Option<(bool, Confirmation<'a>)>,
}

impl Form<'_> {
    fn into_owned(self) -> Form<'static> {
        Form {
            draft: self.draft.map(|draft| Cow::Owned(draft.into_owned())),
            busy: self.busy,
            deleting: self.deleting.map(Confirmation::into_owned),
            pausing: self
                .pausing
                .map(|(paused, message)| (paused, message.into_owned())),
        }
    }
}

pub(super) fn form(world: &mut World, owner: Entity) {
    let _prepare = nucleus::performance::Span::new("karma-form-presentation");
    let view = world.get::<View>(owner).unwrap();
    let form = view.form;
    let deleting = view
        .deleting
        .first()
        .and_then(|uid| view.rules.iter().find(|rule| &rule.uid == uid))
        .map(|rule| Confirmation::new(view.deleting.len(), rule_label(rule)));
    let pausing = if !view.pause_active && !view.pausing.is_empty() {
        let name = view
            .rules
            .iter()
            .find(|rule| rule.uid == view.pausing[0].0)
            .map(rule_label)
            .unwrap_or("rule");
        Some((
            view.pausing[0].2,
            Confirmation::new(view.pausing.len(), name),
        ))
    } else {
        None
    };
    let draft = world.get::<KarmaCastle>(owner).unwrap().draft.as_ref();
    let state = Form {
        draft: draft.map(Cow::Borrowed),
        busy: draft.is_some() && view.pending.is_some(),
        deleting,
        pausing,
    };
    if Presentation::matches(world, form, |old: &Form<'static>| *old == state) {
        controls(world, owner);
        return;
    }
    let state = state.into_owned();
    clear(world, form);
    if let Some(draft) = state.draft.as_deref() {
        let cells = grid(world, form);
        cell(world, cells);
        editor_cells(world, cells, owner, 0, draft);
    }
    if let Some(deleting) = &state.deleting {
        let line = row(world, form);
        crate::native::edit_mode::label(world, line, &deleting.message("Delete"), 14.0);
        button(world, line, owner, Command::ConfirmDelete, "Delete");
        button(world, line, owner, Command::CancelDelete, "Cancel");
    }
    if let Some((paused, message)) = &state.pausing {
        let verb = if *paused { "Pause" } else { "Resume" };
        let line = row(world, form);
        crate::native::edit_mode::label(world, line, &message.message(verb), 14.0);
        button(world, line, owner, Command::ConfirmPause, verb);
        button(world, line, owner, Command::CancelPause, "Cancel");
    }
    Presentation::remember(world, form, state);
    reset_columns(world, owner);
    nucleus::performance::add("karma-forms-rebuilt", 1);
    controls(world, owner);
}
