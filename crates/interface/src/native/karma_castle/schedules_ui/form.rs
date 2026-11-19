use super::*;
use crate::native::retained::Presentation;

struct Form {
    uid: Option<String>,
    range: bool,
    name: Entity,
    boundaries: [Option<Entity>; 2],
}

struct Boundary {
    kind: DateKind,
    value: Entity,
    timezone: Option<Entity>,
    consequence: Entity,
    policies: Option<Entity>,
    dates: Option<Entity>,
    transfers: Entity,
}

fn owns_input(world: &World, owner: Entity, parent: Entity, entity: Entity, index: usize) -> bool {
    world.get::<EditableText>(entity).is_some()
        && world
            .get::<Input>(entity)
            .is_some_and(|input| input.owner == owner && input.index == index)
        && world
            .get::<ChildOf>(entity)
            .is_some_and(|child| child.parent() == parent)
}

pub(super) fn render(
    world: &mut World,
    owner: Entity,
    form: Entity,
    templates: &Arc<Vec<choices::Template>>,
) {
    let _render = nucleus::performance::Span::new("karma-schedule-form-presentation");
    let draft = world
        .get::<KarmaCastle>(owner)
        .unwrap()
        .schedule
        .as_ref()
        .unwrap();
    if !Presentation::matches(world, form, |old: &Form| {
        old.uid == draft.uid
            && old.range == draft.range
            && owns_input(world, owner, form, old.name, 0)
    }) {
        let (uid, range, name) = (draft.uid.clone(), draft.range, draft.name.clone());
        presentation::clear(world, form);
        let name = input(world, form, owner, 0, "Schedule name", &name);
        let boundaries = [
            Some(ui::stack(world, form)),
            range.then(|| ui::stack(world, form)),
        ];
        let line = ui::row(world, form);
        button(world, line, owner, "Save schedule", Command::Save);
        button(world, line, owner, "Close editor", Command::Close);
        crate::native::edit_mode::label(
            world,
            form,
            "Dates and outcomes stay saved when the forecast cache is rebuilt. Cancelling future work leaves the current quantity as it is.",
            13.0,
        );
        Presentation::remember(
            world,
            form,
            Form {
                uid,
                range,
                name,
                boundaries,
            },
        );
        nucleus::performance::add("karma-schedule-forms-built", 1);
    }
    let form = Presentation::<Form>::state(world, form).unwrap();
    let (name, boundaries) = (form.name, form.boundaries);
    publish_input(world, owner, name, 0);
    for (index, root) in boundaries.into_iter().enumerate() {
        if let Some(root) = root {
            boundary(world, owner, root, index, templates);
        }
    }
}

fn boundary(
    world: &mut World,
    owner: Entity,
    root: Entity,
    index: usize,
    templates: &Arc<Vec<choices::Template>>,
) {
    let draft = world
        .get::<KarmaCastle>(owner)
        .unwrap()
        .schedule
        .as_ref()
        .unwrap();
    let date = &draft.dates[index];
    if !Presentation::matches(world, root, |old: &Boundary| {
        old.kind == date.kind
            && owns_input(world, owner, root, old.value, 1 + index * 3)
            && owns_input(world, owner, root, old.consequence, 3 + index * 3)
            && old
                .timezone
                .is_none_or(|field| owns_input(world, owner, root, field, 2 + index * 3))
    }) {
        let (kind, value, timezone, consequence) = (
            date.kind,
            date.value.clone(),
            (date.kind == DateKind::Local).then(|| date.timezone.clone()),
            draft.consequences[index].clone(),
        );
        presentation::clear(world, root);
        crate::native::edit_mode::label(
            world,
            root,
            if index == 0 {
                "Change / start"
            } else {
                "End (the example sets the current quantity to zero)"
            },
            15.0,
        );
        let line = ui::row(world, root);
        for (label, kind) in [
            ("After a duration", DateKind::After),
            ("Date with offset", DateKind::Instant),
            ("Local date", DateKind::Local),
        ] {
            button(world, line, owner, label, Command::Mode(index, kind));
        }
        let value = input(
            world,
            root,
            owner,
            1 + index * 3,
            match kind {
                DateKind::After => "Duration from saving (for example 3d)",
                DateKind::Instant => "Date and time with UTC offset",
                DateKind::Local => "Local date and time",
            },
            &value,
        );
        let (timezone, policies, dates) = if let Some(timezone) = timezone {
            (
                Some(input(
                    world,
                    root,
                    owner,
                    2 + index * 3,
                    "Timezone",
                    &timezone,
                )),
                Some(ui::row(world, root)),
                Some(ui::stack(world, root)),
            )
        } else {
            (None, None, None)
        };
        let consequence = input(
            world,
            root,
            owner,
            3 + index * 3,
            "Consequence (for example @room = -1)",
            &consequence,
        );
        let transfers = ui::stack(world, root);
        Presentation::remember(
            world,
            root,
            Boundary {
                kind,
                value,
                timezone,
                consequence,
                policies,
                dates,
                transfers,
            },
        );
        nucleus::performance::add("karma-schedule-boundaries-built", 1);
    }
    let state = Presentation::<Boundary>::state(world, root).unwrap();
    let (value, timezone, consequence, policies, dates, transfers) = (
        state.value,
        state.timezone,
        state.consequence,
        state.policies,
        state.dates,
        state.transfers,
    );
    publish_input(world, owner, value, 1 + index * 3);
    publish_input(world, owner, consequence, 3 + index * 3);
    if let Some(timezone) = timezone {
        publish_input(world, owner, timezone, 2 + index * 3);
    }
    if let Some(policies) = policies {
        render_policies(world, owner, policies, index);
    }
    if let Some(dates) = dates {
        choices::dates(world, owner, dates, index);
    }
    choices::transfers(world, owner, transfers, index, templates);
}

fn publish_input(world: &mut World, owner: Entity, entity: Entity, index: usize) {
    let draft = world
        .get::<KarmaCastle>(owner)
        .unwrap()
        .schedule
        .as_ref()
        .unwrap();
    let value = match index {
        0 => &draft.name,
        1 | 4 => &draft.dates[(index - 1) / 3].value,
        2 | 5 => &draft.dates[(index - 2) / 3].timezone,
        3 | 6 => &draft.consequences[(index - 3) / 3],
        _ => unreachable!("schedule field"),
    };
    if world.get::<EditableText>(entity).unwrap().value() != value.as_str() {
        let value = value.clone();
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(&value);
        nucleus::performance::add("karma-schedule-editors-published", 1);
    }
}

fn render_policies(world: &mut World, owner: Entity, parent: Entity, index: usize) {
    let date = &world
        .get::<KarmaCastle>(owner)
        .unwrap()
        .schedule
        .as_ref()
        .unwrap()
        .dates[index];
    let state = (date.gap, date.fold);
    if Presentation::matches(
        world,
        parent,
        |old: &(nucleus::karma::GapPolicy, nucleus::karma::FoldPolicy)| *old == state,
    ) {
        return;
    }
    presentation::clear(world, parent);
    button(
        world,
        parent,
        owner,
        if state.0 == nucleus::karma::GapPolicy::Pause {
            "Missing time: refuse"
        } else {
            "Missing time: shift forward"
        },
        Command::Gap(index),
    );
    button(
        world,
        parent,
        owner,
        if state.1 == nucleus::karma::FoldPolicy::First {
            "Repeated time: first"
        } else {
            "Repeated time: second"
        },
        Command::Fold(index),
    );
    button(
        world,
        parent,
        owner,
        "Resolve local date",
        Command::Resolve(index),
    );
    Presentation::remember(world, parent, state);
    nucleus::performance::add("karma-schedule-policies-built", 1);
}
