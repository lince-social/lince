use super::*;
use crate::native::retained::Presentation;

fn controls(world: &mut World, owner: Entity) {
    let controls = world.get::<View>(owner).unwrap().controls;
    let mode = world
        .get::<FrequencyCastle>(owner)
        .unwrap()
        .draft
        .as_ref()
        .map(|draft| draft.uid.is_some());
    if Presentation::matches(world, controls, |old: &Option<bool>| *old == mode) {
        return;
    }
    clear(world, controls);
    if let Some(editing) = mode {
        let title = if editing { "Save" } else { "Create" };
        button(world, controls, owner, title, Command::Save);
        button(world, controls, owner, "Cancel", Command::Cancel);
    } else {
        icon_button(
            world,
            controls,
            owner,
            crate::native::icons::Icon::Plus,
            "New frequency",
            Command::New,
            22.0,
        );
    }

    Presentation::remember(world, controls, mode);
    nucleus::performance::add("frequency-controls-rebuilt", 1);
}

struct Form {
    uid: Option<String>,
    cadence_editable: bool,
    timezone: Option<String>,
    fields: [Option<Entity>; 4],
    weekdays: Option<Weekdays>,
}

#[derive(Clone, Copy)]
struct Weekdays {
    parent: Entity,
    buttons: [Day; 7],
}

#[derive(Clone, Copy)]
struct Day {
    button: Entity,
    label: Entity,
    selected: bool,
}

fn timezone(draft: &Draft) -> Option<&str> {
    if draft.cadence_editable
        && let Some(nucleus::karma::FrequencyCadenceAst::Calendar { timezone, .. }) =
            draft.original.as_ref().map(|original| &original.cadence)
    {
        Some(timezone.as_str())
    } else {
        None
    }
}

impl Form {
    fn matches(&self, world: &World, owner: Entity, form: Entity, draft: &Draft) -> bool {
        self.uid == draft.uid
            && self.cadence_editable == draft.cadence_editable
            && self.timezone.as_deref() == timezone(draft)
            && self.fields.iter().enumerate().all(|(index, field)| {
                field.is_none_or(|field| {
                    world.get::<EditableText>(field).is_some()
                        && world
                            .get::<Input>(field)
                            .is_some_and(|input| input.owner == owner && input.index == index)
                        && world
                            .get::<ChildOf>(field)
                            .is_some_and(|child| child.parent() == form)
                })
            })
            && self.weekdays.is_none_or(|days| {
                days.buttons.iter().all(|day| {
                    world
                        .get::<ChildOf>(day.button)
                        .is_some_and(|child| child.parent() == days.parent)
                        && world
                            .get::<ChildOf>(day.label)
                            .is_some_and(|child| child.parent() == day.button)
                        && world.get::<Text>(day.label).is_some()
                })
            })
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let _prepare = nucleus::performance::Span::new("frequency-form-presentation");
    controls(world, owner);
    let form = world.get::<View>(owner).unwrap().form;
    let draft = &world.get::<FrequencyCastle>(owner).unwrap().draft;
    if Presentation::matches(world, form, |old: &Option<Form>| match (old, draft) {
        (None, None) => true,
        (Some(old), Some(draft)) => old.matches(world, owner, form, draft),
        _ => false,
    }) {
        publish(world, owner, form);
        return;
    }
    let values = draft.as_ref().map(|draft| draft.fields.clone());
    let selected = super::interval::WEEKDAYS.map(|day| {
        draft
            .as_ref()
            .is_some_and(|draft| draft.weekdays.contains(&day))
    });
    let mut state = draft.as_ref().map(|draft| Form {
        uid: draft.uid.clone(),
        cadence_editable: draft.cadence_editable,
        timezone: timezone(draft).map(str::to_owned),
        fields: [None; 4],
        weekdays: None,
    });
    clear(world, form);
    if let Some(state) = &mut state {
        let values = values.as_ref().unwrap();
        crate::native::edit_mode::label(
            world,
            form,
            if state.uid.is_some() {
                "Edit frequency · existing rules keep the same reference"
            } else {
                "New frequency"
            },
            16.0,
        );
        for (index, title) in [(1, "Name"), (0, "Slug")] {
            state.fields[index] = Some(input(world, form, owner, index, title, &values[index]));
        }
        if state.cadence_editable {
            state.fields[2] = Some(input(
                world,
                form,
                owner,
                2,
                "Every · e.g. 1 month + 1 day or 1 day + 10s + 100ms",
                &values[2],
            ));
            crate::native::edit_mode::label(
                world,
                form,
                "Then move forward to · no selection means any weekday",
                13.0,
            );
            let parent = row(world, form);
            let buttons = std::array::from_fn(|index| {
                let day = super::interval::WEEKDAYS[index];
                let label = day_title(day, selected[index]);
                let button = button(world, parent, owner, &label, Command::Weekday(day));
                let label = world
                    .get::<Children>(button)
                    .unwrap()
                    .iter()
                    .find(|&child| world.get::<Text>(child).is_some())
                    .expect("weekday label");
                Day {
                    button,
                    label,
                    selected: selected[index],
                }
            });
            state.weekdays = Some(Weekdays { parent, buttons });
            let title = state.timezone.as_ref().map_or_else(
                || "Next date · date and time with offset".into(),
                |timezone| format!("Next date · {timezone} local time"),
            );
            state.fields[3] = Some(input(world, form, owner, 3, &title, &values[3]));
        } else {
            crate::native::edit_mode::label(
                world,
                form,
                "This calendar or parameter-based schedule is kept unchanged when renaming.",
                12.0,
            );
        }
    }
    Presentation::remember(world, form, state);
    nucleus::performance::add("frequency-forms-rebuilt", 1);
}

fn day_title(day: nucleus::karma::CivilWeekday, selected: bool) -> String {
    let label = super::interval::weekday_label(day);
    if selected {
        format!("[{label}]")
    } else {
        label.into()
    }
}

fn publish(world: &mut World, owner: Entity, form: Entity) {
    let Some(Some(state)) = Presentation::<Option<Form>>::state(world, form) else {
        return;
    };
    let (fields, weekdays) = (state.fields, state.weekdays);
    for (index, field) in fields.into_iter().enumerate() {
        let Some(field) = field else { continue };
        let value = &world
            .get::<FrequencyCastle>(owner)
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .fields[index];
        if world.get::<EditableText>(field).unwrap().value() != value.as_str() {
            let value = value.clone();
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(&value);
            nucleus::performance::add("frequency-form-fields-published", 1);
        }
    }
    if let Some(weekdays) = weekdays {
        for (index, day) in weekdays.buttons.into_iter().enumerate() {
            let weekday = super::interval::WEEKDAYS[index];
            let selected = world
                .get::<FrequencyCastle>(owner)
                .unwrap()
                .draft
                .as_ref()
                .unwrap()
                .weekdays
                .contains(&weekday);
            if day.selected == selected {
                continue;
            }
            let label = day_title(weekday, selected);
            world
                .get_mut::<Text>(day.label)
                .unwrap()
                .0
                .clone_from(&label);
            if let Some(mut tooltip) = world.get_mut::<crate::native::icons::Tooltip>(day.button) {
                tooltip.0.clone_from(&label);
            }
            if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(day.button) {
                node.set_label(label.as_str());
            }
            Presentation::<Option<Form>>::update_state(world, form, |state| {
                state.as_mut().unwrap().weekdays.as_mut().unwrap().buttons[index].selected =
                    selected;
            });
            nucleus::performance::add("frequency-weekdays-published", 1);
        }
    }
}
