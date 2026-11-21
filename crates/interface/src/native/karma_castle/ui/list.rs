// non-lean-yet
// Scope: struct Snapshot, struct List, struct Position, struct Presented.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 92, 99, 101, 108,
//   142, 235, 339, 345.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   41, 45, 48, 73, 79, 146, 226, 271, 272, 305, 323.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (89, 91, 94, 122, 129, 133, 201, 261, 279, 351, 366).

use super::*;

#[derive(Default)]
struct Snapshot {
    rules: Arc<Vec<Rule>>,
    drafts: Arc<Vec<Draft>>,
    busy: bool,
    creating: bool,
    font: Handle<Font>,
}

#[derive(Component, Default)]
struct List {
    children: crate::native::retained::Children<(String, usize)>,
    snapshot: Snapshot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Position {
    source: usize,
    binding: usize,
    draft: Option<usize>,
    active: Option<usize>,
}

#[derive(Component)]
struct Presented {
    position: Position,
    checkbox: Entity,
    children: Vec<Entity>,
}

struct Content<'a> {
    name: &'a str,
    slug: &'a str,
    fields: [&'a str; 3],
}

impl Snapshot {
    fn content(&self, position: Position) -> Content<'_> {
        if let Some(draft) = position.draft.map(|index| &self.drafts[index]) {
            Content {
                name: &draft.name,
                slug: &draft.slug,
                fields: std::array::from_fn(|index| draft.fields[index].text.as_str()),
            }
        } else {
            let rule = &self.rules[position.source];
            Content {
                name: &rule.name,
                slug: &rule.slug,
                fields: RuleFieldKind::ALL.map(|kind| {
                    rule.fields
                        .iter()
                        .find(|field| field.kind == kind)
                        .map_or("", |field| field.source.as_str())
                }),
            }
        }
    }

    fn unchanged(&self, before: &Snapshot, previous: Position, current: Position) -> bool {
        if previous.draft != current.draft
            || previous.active != current.active
            || self.busy != before.busy
            || self.creating != before.creating
            || self.font != before.font
        {
            return false;
        }
        let same_rules = Arc::ptr_eq(&self.rules, &before.rules);
        let same_source = same_rules && previous.source == current.source
            || before.rules.get(previous.source) == Some(&self.rules[current.source]);
        let same_draft = Arc::ptr_eq(&self.drafts, &before.drafts)
            || previous.draft.and_then(|index| before.drafts.get(index))
                == current.draft.and_then(|index| self.drafts.get(index));
        let same_binding = same_rules && previous.binding == current.binding
            || before.rules.get(previous.binding).is_some_and(|rule| {
                let current = &self.rules[current.binding];
                rule.bindings == current.bindings && rule.record == current.record
            });
        same_source && same_draft && same_binding
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let _prepare = nucleus::performance::Span::new("karma-list-preparation");
    reset_columns(world, owner);
    let parent = world.get::<View>(owner).unwrap().list;
    let mut list = world.entity_mut(parent).take::<List>().unwrap_or_default();
    let view = world.get::<View>(owner).unwrap();
    let rules = view.rules.clone();
    let busy = view.pending.is_some();
    let castle = world.get::<KarmaCastle>(owner).unwrap();
    let query = castle.search.trim().to_lowercase();
    let snapshot = Snapshot {
        rules,
        drafts: if castle.edits.as_slice() == list.snapshot.drafts.as_slice() {
            list.snapshot.drafts.clone()
        } else {
            Arc::new(castle.edits.clone())
        },
        busy,
        creating: castle.draft.is_some(),
        font: world
            .resource::<crate::native::theme::Typography>()
            .0
            .clone(),
    };
    let mut first = HashMap::new();
    for (index, rule) in snapshot.rules.iter().enumerate() {
        first.entry(rule.uid.as_str()).or_insert(index);
    }
    let mut edits = HashMap::new();
    for (index, draft) in snapshot.drafts.iter().enumerate() {
        if let Some(uid) = draft.rule.as_deref() {
            edits.entry(uid).or_insert(index);
        }
    }
    if world
        .get::<View>(owner)
        .unwrap()
        .selected
        .iter()
        .any(|uid| !first.contains_key(uid.as_str()))
    {
        world
            .get_mut::<View>(owner)
            .unwrap()
            .selected
            .retain(|uid| first.contains_key(uid.as_str()));
    }
    let view = world.get::<View>(owner).unwrap();
    let mut occurrences = HashMap::<&str, usize>::new();
    let rows: Vec<_> = snapshot
        .rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| matches_rule(rule, &query))
        .map(|(index, rule)| {
            let occurrence = occurrences.entry(&rule.uid).or_default();
            let key = (rule.uid.clone(), *occurrence);
            *occurrence += 1;
            let position = Position {
                source: index,
                binding: first[rule.uid.as_str()],
                draft: edits.get(rule.uid.as_str()).copied(),
                active: view
                    .editing
                    .as_ref()
                    .filter(|(uid, _)| uid == &rule.uid)
                    .map(|(_, index)| *index),
            };
            (key, (position, view.selected.contains(&rule.uid)))
        })
        .collect();
    drop((first, edits, occurrences));
    let count = rows.len();
    let selected = rows.iter().filter(|(_, (_, selected))| *selected).count();
    let previous = &list.snapshot;
    list.children.reconcile(
        world,
        parent,
        rows,
        |world, parent, &(position, selected)| {
            let entity = grid(world, parent);
            rebuild(world, entity, owner, &snapshot, position, selected);
            nucleus::performance::add("karma-list-rows-created", 1);
            entity
        },
        |world, entity, &(position, selected)| {
            let retained = world
                .get::<Presented>(entity)
                .filter(|row| {
                    snapshot.unchanged(previous, row.position, position)
                        && world.get::<Children>(entity).is_some_and(|children| {
                            children.iter().eq(row.children.iter().copied())
                        })
                        && world
                            .get::<bevy::a11y::AccessibilityNode>(row.checkbox)
                            .is_some()
                        && world
                            .get::<ChildOf>(row.checkbox)
                            .is_some_and(|child| row.children.first() == Some(&child.parent()))
                        && world
                            .get::<Children>(row.checkbox)
                            .is_some_and(|children| !children.is_empty())
                })
                .map(|row| (row.position, row.checkbox));
            if let Some((old, checkbox)) = retained {
                update_checkbox(
                    world,
                    checkbox,
                    if selected {
                        accesskit::Toggled::True
                    } else {
                        accesskit::Toggled::False
                    },
                );
                if old != position {
                    world.get_mut::<Presented>(entity).unwrap().position = position;
                }
                nucleus::performance::add("karma-list-rows-retained", 1);
            } else {
                clear(world, entity);
                rebuild(world, entity, owner, &snapshot, position, selected);
                nucleus::performance::add("karma-list-rows-rebuilt", 1);
            }
        },
    );
    list.snapshot = snapshot;
    world.entity_mut(parent).insert(list);
    update_selection(world, owner, count, selected);
    render_controls(world, owner);
}

fn rebuild(
    world: &mut World,
    cells: Entity,
    owner: Entity,
    snapshot: &Snapshot,
    position: Position,
    selected: bool,
) {
    let _row = nucleus::performance::Span::new("karma-rule-row-construction");
    let rule = &snapshot.rules[position.source];
    let value = snapshot.content(position);
    let busy = snapshot.busy;
    let creating = snapshot.creating;
    let selection = cell(world, cells);
    let checkbox = checkbox(
        world,
        selection,
        owner,
        Command::Select(rule.uid.clone()),
        &format!("Select {}", rule_label(rule)),
        selected,
    );
    if busy {
        world
            .entity_mut(checkbox)
            .insert(bevy::ui::InteractionDisabled);
    }
    for index in [3, 4, 0, 1, 2] {
        let cell = cell(world, cells);
        let active = position.active == Some(index);
        let content = if index >= 3 {
            let identity = row(world, cell);
            let text = if index == 3 {
                value.name.to_owned()
            } else if value.slug.is_empty() {
                String::new()
            } else {
                format!("@{}", value.slug)
            };
            {
                let label = crate::native::edit_mode::label(world, identity, &text, 16.0);
                world.entity_mut(label).insert(TextLayout::linebreak(
                    bevy::text::LineBreak::WordOrCharacter,
                ));
                let mut node = world.get_mut::<Node>(label).unwrap();
                node.min_width = Val::Auto;
                node.flex_shrink = 1.0;
            }
            identity
        } else {
            rich_text(
                world,
                cell,
                owner,
                &value.fields[index],
                &snapshot.rules[position.binding],
                index,
            )
        };
        if active && let Some(edited) = position.draft {
            world.entity_mut(content).insert(Visibility::Hidden);
            let overlay = row(world, cell);
            *world.get_mut::<Node>(overlay).unwrap() = Node {
                position_type: PositionType::Absolute,
                left: px(6),
                right: px(6),
                top: px(6),
                bottom: px(6),
                min_width: px(0),
                column_gap: px(8),
                ..default()
            };
            if index >= 3 {
                editor(
                    world,
                    overlay,
                    owner,
                    edited + 1,
                    index,
                    if index == 3 { &value.name } else { &value.slug },
                );
            } else {
                editor(
                    world,
                    overlay,
                    owner,
                    edited + 1,
                    index,
                    &value.fields[index],
                );
            }
        } else if !busy && !creating {
            world.entity_mut(cell).insert((
                crate::native::sand::button(0),
                crate::native::sand::Borderless,
                ActionButton::new(
                    owner,
                    crate::native::actions![Command::EditCell(rule.uid.clone(), index)],
                ),
            ));
            accessible(
                world,
                cell,
                accesskit::Role::Button,
                &format!(
                    "Edit {} for {}",
                    ["condition", "threshold", "consequence", "name", "slug"][index],
                    rule_label(rule)
                ),
            );
        }
    }
    let paused = rule.state == "paused";
    for (left, icon, label, command) in [
        (
            true,
            if paused { Icon::Play } else { Icon::Stop },
            if paused {
                "Resume rule".to_owned()
            } else {
                "Pause rule".to_owned()
            },
            Command::RequestPause(rule.uid.clone(), rule.revision, !paused),
        ),
        (
            false,
            Icon::Close,
            format!("Delete {}", rule_label(rule)),
            Command::Delete(rule.uid.clone()),
        ),
    ] {
        let entity = icon_button(world, cells, owner, icon, &label, command, !busy);
        size_icon(world, entity, 16.0);
        {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            node.position_type = PositionType::Absolute;
            node.top = px(8);
            if left {
                node.left = px(2);
            } else {
                node.right = px(2);
            }
        }
        world.entity_mut(entity).insert((
            RowAction { row: cells, owner },
            Visibility::Hidden,
            bevy::ui::InteractionDisabled,
        ));
    }
    let children = world.get::<Children>(cells).unwrap().iter().collect();
    world.entity_mut(cells).insert((
        RuleRow,
        Presented {
            position,
            checkbox,
            children,
        },
    ));
}
