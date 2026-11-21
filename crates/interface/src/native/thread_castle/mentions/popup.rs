// non-lean-yet
// Scope: fn title, fn select, fn container, fn render.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 54, 59, 129, 137,
//   187.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (195).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (97, 119, 122, 139, 192, 209).

use super::*;

impl Rendered {
    fn current(&self, world: &World, form: Entity, input: Entity, popup: Entity) -> bool {
        if self.form != form
            || world.get::<Node>(popup).is_none()
            || self.font != world.resource::<crate::native::theme::Typography>().0
            || world.get::<ChildOf>(popup).map(ChildOf::parent)
                != world.get::<ChildOf>(input).map(ChildOf::parent)
        {
            return false;
        }
        let mut children = world
            .get::<Children>(popup)
            .into_iter()
            .flat_map(|c| c.iter());
        self.children
            .iter()
            .all(|expected| children.by_ref().any(|child| child == *expected))
            && self.choices.iter().all(|(button, label)| {
                world
                    .get::<ChildOf>(*label)
                    .is_some_and(|child| child.parent() == *button)
                    && world.get::<Text>(*label).is_some()
            })
    }

    fn same_contents(&self, draft: &Draft) -> bool {
        self.error == draft.error
            && self.loaded == draft.loaded
            && (Arc::ptr_eq(&self.options, &draft.options) || self.options == draft.options)
    }
}

fn title(row: &Candidate, selected: bool) -> String {
    let slug = if row.slug.is_empty() {
        &row.uid
    } else {
        &row.slug
    };
    format!(
        "{}@{} · {}",
        if selected { "› " } else { "  " },
        slug,
        row.head
    )
}

fn select(world: &mut World, control: (Entity, Entity), title: String, selected: bool) {
    let (button, label) = control;
    if let Some(mut text) = world.get_mut::<Text>(label)
        && text.0 != title
    {
        text.0 = title.clone();
    }
    if let Some(mut tooltip) = world.get_mut::<crate::native::icons::Tooltip>(button)
        && tooltip.0 != title
    {
        tooltip.0 = title.clone();
    }
    if let Some(mut accessibility) = world.get_mut::<AccessibilityNode>(button)
        && accessibility.label() != Some(title.as_str())
    {
        accessibility.set_label(title.as_str());
    }
    if selected {
        world.entity_mut(button).insert((
            SelectedOption,
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
        ));
    } else {
        world
            .entity_mut(button)
            .remove::<(SelectedOption, crate::native::token_style::BorderToken)>()
            .insert(BorderColor::default());
    }
    nucleus::performance::add("mention-popup-selection-updates", 1);
}

fn container() -> Node {
    Node {
        position_type: PositionType::Absolute,
        bottom: percent(100),
        left: px(0),
        width: percent(100),
        max_height: px(260),
        flex_direction: FlexDirection::Column,
        border: UiRect::all(px(1)),
        padding: UiRect::all(px(4)),
        overflow: Overflow::scroll_y(),
        ..default()
    }
}

pub(super) fn render(world: &mut World, form: Entity, input: Entity) {
    let _presentation = nucleus::performance::Span::new("mention-popup-presentation");
    let draft = world.get::<Draft>(input).unwrap();
    let popup = draft.popup.filter(|popup| world.get_entity(*popup).is_ok());
    if let Some(old) = draft.rendered.as_ref().filter(|old| {
        popup.is_some_and(|popup| old.current(world, form, input, popup))
            && old.same_contents(draft)
    }) {
        let selected = draft.selected;
        if old.selected == selected {
            return;
        }
        let changes = [(old.selected, false), (selected, true)].map(|(index, selected)| {
            old.choices
                .get(index)
                .copied()
                .zip(draft.options.get(index))
                .map(|(control, row)| (control, title(row, selected), selected))
        });
        for (control, title, selected) in changes.into_iter().flatten() {
            select(world, control, title, selected);
        }
        world
            .get_mut::<Draft>(input)
            .unwrap()
            .rendered
            .as_mut()
            .unwrap()
            .selected = selected;
        return;
    }
    let mut rendered = Rendered {
        options: Arc::clone(&draft.options),
        selected: draft.selected,
        error: draft.error.clone(),
        loaded: draft.loaded,
        form,
        choices: Vec::new(),
        children: Vec::new(),
        font: world
            .resource::<crate::native::theme::Typography>()
            .0
            .clone(),
    };
    let parent = world.get::<ChildOf>(input).unwrap().parent();
    let popup = if let Some(popup) = popup {
        if world.get::<Node>(popup).is_none() {
            world.entity_mut(popup).insert(container());
        }
        if world.get::<ChildOf>(popup).map(ChildOf::parent) != Some(parent) {
            world.entity_mut(popup).insert(ChildOf(parent));
        }
        popup
    } else {
        world
            .spawn((
                container(),
                ScrollPosition::default(),
                crate::native::scroll_sand::ScrollSand,
                crate::native::token_style::background(crate::native::tokens::Token::Surface),
                crate::native::token_style::border(crate::native::tokens::Token::Accent),
                GlobalZIndex(80),
                ChildOf(parent),
            ))
            .id()
    };
    if let Some(children) = world.get::<Children>(popup) {
        for child in children.iter().collect::<Vec<_>>() {
            world.despawn(child);
        }
    }
    if rendered.options.is_empty() {
        let label = crate::native::edit_mode::label(
            world,
            popup,
            rendered.error.as_deref().unwrap_or(if rendered.loaded {
                "No matching Records"
            } else {
                "Searching Records…"
            }),
            14.0,
        );
        rendered.children.push(label);
    }
    for (index, row) in rendered.options.iter().enumerate() {
        let button = control(
            world,
            popup,
            form,
            &title(row, index == rendered.selected),
            Choose {
                input,
                uid: row.uid.clone(),
            },
        );
        let label = world
            .get::<Children>(button)
            .unwrap()
            .iter()
            .find(|child| world.get::<Text>(*child).is_some())
            .expect("mention choice button has a caption");
        world
            .entity_mut(button)
            .insert(crate::native::sand::Borderless);
        if index == rendered.selected {
            world.entity_mut(button).insert((
                SelectedOption,
                crate::native::token_style::border(crate::native::tokens::Token::Accent),
            ));
        }
        rendered.children.push(button);
        rendered.choices.push((button, label));
    }
    nucleus::performance::add("mention-popup-rows-built", rendered.children.len() as u64);
    let mut draft = world.get_mut::<Draft>(input).unwrap();
    draft.popup = Some(popup);
    draft.rendered = Some(rendered);
}
