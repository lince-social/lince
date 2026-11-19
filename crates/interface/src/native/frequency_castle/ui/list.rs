use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};
use std::sync::Weak;

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Empty,
    Frequency(String, usize),
    Pages,
}

#[derive(Clone, Copy)]
enum Part {
    Empty,
    Frequency { index: usize, deleting: bool },
    Pages { page: usize, count: usize },
}

struct Rendered {
    part: Part,
    countdown: Option<Entity>,
    body: Option<Body>,
}

impl Rendered {
    fn matches(
        &self,
        world: &World,
        entity: Entity,
        part: Part,
        source: &[Frequency],
        same_source: bool,
    ) -> bool {
        match (self.part, part) {
            (Part::Empty, Part::Empty) => world
                .get::<Text>(entity)
                .is_some_and(|text| text.0 == EMPTY),
            (
                Part::Pages {
                    page: old_page,
                    count: old_count,
                },
                Part::Pages { page, count },
            ) => old_page == page && old_count == count,
            (
                Part::Frequency {
                    index: old_index,
                    deleting: old_deleting,
                },
                Part::Frequency { index, deleting },
            ) => {
                old_deleting == deleting
                    && (same_source && old_index == index
                        || self
                            .body
                            .as_ref()
                            .is_some_and(|body| body.matches(&source[index])))
                    && self.countdown.is_some_and(|countdown| {
                        world.get::<Countdown>(countdown).is_some()
                            && world.get::<Text>(countdown).is_some()
                            && world
                                .get::<ChildOf>(countdown)
                                .is_some_and(|child| child.parent() == entity)
                    })
            }
            _ => false,
        }
    }
}

struct Body {
    uid: String,
    slug: String,
    definition: nucleus::karma::FrequencyAst,
    active: bool,
}

impl Body {
    fn new(frequency: &Frequency) -> Self {
        Self {
            uid: frequency.uid.clone(),
            slug: frequency.slug.clone(),
            definition: frequency.definition.clone(),
            active: !frequency.quantity.is_zero(),
        }
    }

    fn matches(&self, frequency: &Frequency) -> bool {
        self.uid == frequency.uid
            && self.slug == frequency.slug
            && self.definition == frequency.definition
            && self.active == !frequency.quantity.is_zero()
    }
}

#[derive(Component, Default)]
struct List {
    children: RetainedChildren<Key>,
    source: Weak<Vec<Frequency>>,
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let _prepare = nucleus::performance::Span::new("frequency-list-preparation");
    let view = world.get::<View>(owner).unwrap();
    let parent = view.list;
    let source = view.rows.values.clone();
    let query = world
        .get::<FrequencyCastle>(owner)
        .unwrap()
        .search
        .trim()
        .to_lowercase();
    let matches: Vec<_> = source
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            query.is_empty()
                || format!("{} {} {}", row.slug, row.definition.purpose, row.status)
                    .to_lowercase()
                    .contains(&query)
        })
        .map(|(index, _)| index)
        .collect();
    let count = matches.len();
    let page = view.page.min(count.saturating_sub(1) / PAGE_SIZE);
    let mut occurrences = HashMap::<&str, usize>::new();
    let mut parts: Vec<_> = matches
        .into_iter()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|index| {
            let frequency = &source[index];
            let occurrence = occurrences.entry(&frequency.uid).or_default();
            let key = Key::Frequency(frequency.uid.clone(), *occurrence);
            *occurrence += 1;
            (
                key,
                Part::Frequency {
                    index,
                    deleting: view.deleting.as_ref() == Some(&frequency.uid),
                },
            )
        })
        .collect();
    drop(occurrences);
    if parts.is_empty() {
        parts.push((Key::Empty, Part::Empty));
    }
    if count > PAGE_SIZE {
        parts.push((
            Key::Pages,
            Part::Pages {
                page,
                count: count.div_ceil(PAGE_SIZE),
            },
        ));
    }
    if view.page != page {
        world.get_mut::<View>(owner).unwrap().page = page;
    }
    let now = chrono::Utc::now().timestamp_millis();
    let mut list = world.entity_mut(parent).take::<List>().unwrap_or_default();
    let same_source = list.source.as_ptr() == Arc::as_ptr(&source);
    list.children.reconcile(
        world,
        parent,
        parts,
        |world, parent, part| {
            let entity = match part {
                Part::Empty => crate::native::edit_mode::label(world, parent, EMPTY, 14.0),
                Part::Frequency { .. } => stack(world, parent),
                Part::Pages { .. } => row(world, parent),
            };
            let countdown = if matches!(part, Part::Empty) {
                None
            } else {
                build(world, entity, owner, *part, &source, now)
            };
            remember(world, entity, *part, countdown, &source);
            nucleus::performance::add("frequency-list-parts-created", 1);
            entity
        },
        |world, entity, part| {
            let unchanged = Presentation::matches(world, entity, |old: &Rendered| {
                old.matches(world, entity, *part, &source, same_source)
            });
            if unchanged {
                let old = Presentation::<Rendered>::state(world, entity).unwrap();
                let (old_part, countdown) = (old.part, old.countdown);
                if let Part::Frequency { index, .. } = *part {
                    let next = model::Next::new(&source[index], now);
                    publish_countdown(world, countdown.unwrap(), next);
                    let moved = matches!(old_part,
                        Part::Frequency { index: old_index, .. } if old_index != index
                    );
                    if moved {
                        Presentation::<Rendered>::update_state(world, entity, |old| {
                            old.part = *part
                        });
                    }
                }
                nucleus::performance::add("frequency-list-parts-retained", 1);
            } else {
                clear(world, entity);
                let countdown = build(world, entity, owner, *part, &source, now);
                remember(world, entity, *part, countdown, &source);
                nucleus::performance::add("frequency-list-parts-rebuilt", 1);
            }
        },
    );
    list.source = Arc::downgrade(&source);
    world.entity_mut(parent).insert(list);
}

fn remember(
    world: &mut World,
    entity: Entity,
    part: Part,
    countdown: Option<Entity>,
    source: &[Frequency],
) {
    let body = match part {
        Part::Frequency { index, .. } => Some(Body::new(&source[index])),
        _ => None,
    };
    Presentation::remember(
        world,
        entity,
        Rendered {
            part,
            countdown,
            body,
        },
    );
}

const EMPTY: &str = "No frequencies found. Use + to create one.";

fn build(
    world: &mut World,
    entity: Entity,
    owner: Entity,
    part: Part,
    source: &[Frequency],
    now: i64,
) -> Option<Entity> {
    match part {
        Part::Empty => {
            let font = world
                .resource::<crate::native::theme::Typography>()
                .text(14.0);
            world.entity_mut(entity).insert((Text::new(EMPTY), font));
            None
        }
        Part::Frequency { index, deleting } => Some(frequency(
            world,
            entity,
            owner,
            &source[index],
            deleting,
            now,
        )),
        Part::Pages { page, count } => {
            pages(world, entity, owner, page, count);
            None
        }
    }
}

fn frequency(
    world: &mut World,
    block: Entity,
    owner: Entity,
    frequency: &Frequency,
    deleting: bool,
    now: i64,
) -> Entity {
    let _row = nucleus::performance::Span::new("frequency-row-construction");
    let heading = row(world, block);
    world.get_mut::<Node>(heading).unwrap().flex_wrap = FlexWrap::NoWrap;
    let active = !frequency.quantity.is_zero();
    let indicator = world
        .spawn((
            Node {
                width: px(17),
                height: px(17),
                flex_shrink: 0.0,
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(percent(50)),
                ..default()
            },
            crate::native::token_style::border(crate::native::tokens::Token::Ink),
            crate::native::icons::Tooltip(
                if active {
                    "Active"
                } else {
                    "Inactive · quantity 0"
                }
                .into(),
            ),
            ChildOf(heading),
        ))
        .id();
    if active {
        world
            .entity_mut(indicator)
            .insert(crate::native::token_style::background(
                crate::native::tokens::Token::Ink,
            ));
    }
    let name = crate::native::edit_mode::label(world, heading, &frequency.definition.purpose, 17.0);
    let slug =
        crate::native::edit_mode::label(world, heading, &format!("@{}", frequency.slug), 17.0);
    for label in [name, slug] {
        world
            .entity_mut(label)
            .insert(TextLayout::linebreak(bevy::text::LineBreak::NoWrap));
        let mut node = world.get_mut::<Node>(label).unwrap();
        node.flex_shrink = 1.0;
        node.min_width = px(0);
        node.overflow = Overflow::clip();
    }
    icon_button(
        world,
        heading,
        owner,
        crate::native::icons::Icon::Pencil,
        "Edit",
        Command::Edit(frequency.uid.clone()),
        17.0,
    );
    icon_button(
        world,
        heading,
        owner,
        crate::native::icons::Icon::Delete,
        "Delete",
        Command::Delete(frequency.uid.clone()),
        17.0,
    );
    let schedule = model::schedule(frequency);
    let label = format!("{schedule} | Next: {}", model::Next::new(frequency, now));
    let countdown = crate::native::edit_mode::label(world, block, &label, 13.0);
    world.entity_mut(countdown).insert(Countdown {
        owner,
        uid: frequency.uid.clone(),
        schedule,
        presented: None,
    });
    if deleting {
        crate::native::edit_mode::label(
            world,
            block,
            "Delete this frequency? Frequencies used by rules, Signals, or Programs cannot be deleted.",
            13.0,
        );
        let confirm = row(world, block);
        button(
            world,
            confirm,
            owner,
            "Confirm delete",
            Command::ConfirmDelete(frequency.uid.clone()),
        );
        button(
            world,
            confirm,
            owner,
            "Keep frequency",
            Command::CancelDelete,
        );
    }
    countdown
}

fn pages(world: &mut World, pages: Entity, owner: Entity, page: usize, pages_count: usize) {
    if page > 0 {
        button(world, pages, owner, "Previous", Command::Page(page - 1));
    }
    crate::native::edit_mode::label(
        world,
        pages,
        &format!("Page {} / {}", page + 1, pages_count),
        13.0,
    );
    if page + 1 < pages_count {
        button(world, pages, owner, "Next", Command::Page(page + 1));
    }
}
