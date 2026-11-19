use super::*;
use crate::native::retained::Presentation;
use std::sync::Arc;

struct Frame {
    selector: Option<Selector>,
    error: Option<String>,
    picker: Option<(Entity, Entity, u64)>,
    source: Option<Entity>,
    sources: Vec<(String, String)>,
    today: chrono::NaiveDate,
    month: Option<Arc<Month>>,
}

pub(super) struct Day {
    pub cell: Entity,
    pub week: Entity,
    pub header: Option<Entity>,
}

pub(super) struct Month {
    pub grid: Entity,
    pub days: Vec<Day>,
    pub footer: Option<Entity>,
}

impl Month {
    fn valid(&self, world: &World, body: Entity) -> bool {
        let child = |entity, parent| {
            world
                .get::<ChildOf>(entity)
                .is_some_and(|of| of.parent() == parent)
        };
        child(self.grid, body)
            && self.footer.is_none_or(|footer| child(footer, body))
            && self.days.iter().all(|day| {
                child(day.week, self.grid)
                    && child(day.cell, day.week)
                    && day.header.is_none_or(|header| child(header, day.cell))
            })
    }
}

pub(super) fn sources(world: &mut World, owner: Entity) -> Vec<(String, String)> {
    if world.get::<View>(owner).unwrap().selector != Some(Selector::Source) {
        return Vec::new();
    }
    let root = world.get::<ChildOf>(owner).unwrap().parent();
    let workspace = world.get::<WorkspaceMember>(owner).unwrap().0;
    world
        .query::<(&InfluenceArea, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(area, parent, member)| {
            parent.parent() == root && member.0 == workspace && area.protein.is_some()
        })
        .map(|(area, _, _)| (area.id.clone(), area.name.clone()))
        .collect()
}

pub(super) fn matches(
    world: &World,
    owner: Entity,
    source: Option<Entity>,
    sources: &[(String, String)],
    today: chrono::NaiveDate,
) -> bool {
    let view = world.get::<View>(owner).unwrap();
    view.rendered.as_ref() == Some(&world.get::<CalendarSand>(owner).unwrap().0)
        && view.body.is_some_and(|body| {
            world
                .get::<ChildOf>(body)
                .is_some_and(|of| of.parent() == owner)
                && Presentation::matches(world, body, |old: &Frame| {
                    old.selector == view.selector
                        && old.error == view.error
                        && old.picker == picker::signature(world, owner)
                        && old.source == source
                        && old.sources.as_slice() == sources
                        && old.today == today
                        && old
                            .month
                            .as_ref()
                            .is_none_or(|month| month.valid(world, body))
                })
        })
}

pub(super) fn remember(
    world: &mut World,
    owner: Entity,
    source: Option<Entity>,
    sources: Vec<(String, String)>,
    today: chrono::NaiveDate,
    month: Option<Month>,
) {
    let view = world.get::<View>(owner).unwrap();
    let body = view.body.unwrap();
    let state = Frame {
        selector: view.selector,
        error: view.error.clone(),
        picker: picker::signature(world, owner),
        source,
        sources,
        today,
        month: month.map(Arc::new),
    };
    Presentation::remember(world, body, state);
}

struct DayRows {
    source: Option<(Entity, crate::native::protein_area::Source)>,
    entries: Vec<(String, String, String)>,
}

pub(super) fn refresh(world: &mut World, owner: Entity, source: Option<Entity>) {
    let _refresh = nucleus::performance::Span::new("calendar-month-publication");
    let view = world.get::<View>(owner).unwrap();
    let body = view.body.unwrap();
    let Some(month) =
        Presentation::<Frame>::state(world, body).and_then(|frame| frame.month.clone())
    else {
        return;
    };
    let calendar = &world.get::<CalendarSand>(owner).unwrap().0;
    let data = projection::data(world, owner)
        .or_else(|| {
            source.and_then(|source| {
                crate::native::protein_area::calendar_feed(world, source).map(|(rows, _)| rows)
            })
        })
        .unwrap_or(&[]);
    let records = records(calendar, data, view.page);
    if let Some(footer) = month.footer {
        footer_update(world, owner, source.unwrap(), footer, &records);
    }
    world.get_mut::<View>(owner).unwrap().page = records.page;
    let binding = source.and_then(|source| {
        let kind = &world.get::<InfluenceArea>(source)?.protein.as_ref()?.source;
        Some((source, kind.clone()))
    });
    for (day, entries) in month.days.iter().zip(records.days) {
        if day.header.is_none() {
            continue;
        }
        let entries = if binding.is_some() {
            entries
        } else {
            Vec::new()
        };
        if Presentation::matches(world, day.cell, |old: &DayRows| {
            old.source == binding && old.entries == entries
        }) {
            nucleus::performance::add("calendar-days-retained", 1);
            continue;
        }
        let children: Vec<_> = world
            .get::<Children>(day.cell)
            .into_iter()
            .flat_map(|children| children.iter())
            .filter(|child| Some(*child) != day.header)
            .collect();
        for child in children {
            world.despawn(child);
        }
        if let Some((source, kind)) = &binding {
            for (uid, title, tip) in &entries {
                let entity = button(
                    world,
                    day.cell,
                    owner,
                    title,
                    tip,
                    Command::Record(crate::native::protein_area::RecordBinding {
                        area: *source,
                        uid: uid.clone(),
                        source: kind.clone(),
                    }),
                );
                let mut node = world.get_mut::<Node>(entity).unwrap();
                node.width = percent(100);
                node.min_height = px(18);
                node.padding = UiRect::axes(px(3), px(1));
            }
        }
        Presentation::remember(
            world,
            day.cell,
            DayRows {
                source: binding.clone(),
                entries,
            },
        );
        nucleus::performance::add("calendar-days-rebuilt", 1);
    }
}

struct Footer {
    state: String,
    count: usize,
    timezone: String,
    hidden: usize,
    maximum: usize,
    page: usize,
}

fn footer_update(
    world: &mut World,
    owner: Entity,
    source: Entity,
    footer: Entity,
    records: &MonthRecords,
) {
    let state = world.get::<projection::Feed>(owner).map_or_else(
        || crate::native::protein_area::calendar_status(world, source),
        |feed| feed.status.as_str(),
    );
    let count =
        crate::native::protein_area::calendar_feed(world, source).map_or(0, |(data, _)| data.len());
    let timezone = &world.get::<CalendarSand>(owner).unwrap().0.timezone;
    if Presentation::matches(world, footer, |old: &Footer| {
        old.state == state
            && old.count == count
            && &old.timezone == timezone
            && old.hidden == records.hidden
            && old.maximum == records.maximum
            && old.page == records.page
    }) {
        return;
    }
    let state = Footer {
        state: state.to_owned(),
        count,
        timezone: timezone.clone(),
        hidden: records.hidden,
        maximum: records.maximum,
        page: records.page,
    };
    crate::native::sand_panel::clear(world, footer);
    let label = crate::native::edit_mode::label(
        world,
        footer,
        &format!("{} · {} Records · {}", state.state, count, state.timezone),
        12.0,
    );
    world.entity_mut(label).insert(Tooltip(format!("{} without a valid date or period. Results follow the Protein Area's filters and row limit.", records.hidden)));
    if records.maximum > PER_DAY {
        button(
            world,
            footer,
            owner,
            "‹",
            "Previous records",
            Command::Page(records.page.saturating_sub(1)),
        );
        crate::native::edit_mode::label(
            world,
            footer,
            &format!(
                "{} / {}",
                records.page + 1,
                records.maximum.div_ceil(PER_DAY)
            ),
            12.0,
        );
        button(
            world,
            footer,
            owner,
            "›",
            "More records",
            Command::Page((records.page + 1).min(records.maximum.saturating_sub(1) / PER_DAY)),
        );
    }
    Presentation::remember(world, footer, state);
    nucleus::performance::add("calendar-footers-rebuilt", 1);
}
