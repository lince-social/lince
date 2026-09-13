use super::*;
use crate::{actions::ActionButton, icons::Tooltip, tokens::Token};
use chrono::Datelike;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
pub(super) const PER_DAY: usize = 3;

pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    text: &str,
    tip: &str,
    command: Command,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            Node {
                padding: UiRect::axes(px(7), px(4)),
                min_width: px(28),
                min_height: px(28),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
            ChildOf(parent),
            Tooltip(tip.into()),
            ActionButton::new(owner, crate::actions![command]),
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(tip);
    }
    let label = crate::edit_mode::label(world, entity, text, 14.0);
    world
        .entity_mut(label)
        .insert(TextLayout::linebreak(bevy::text::LineBreak::NoWrap));
    entity
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                column_gap: px(4),
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    button(
        world,
        parent,
        root,
        "Calendar",
        "Add a Calendar Castle",
        Command::Create,
    );
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(calendar) = world.get::<CalendarSand>(owner).map(|s| s.0.clone()) else {
        return;
    };
    if let Some(body) = world.get_mut::<View>(owner).unwrap().body.take() {
        world.despawn(body);
    }
    let body = world
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.body = Some(body);
    view.rendered = Some(calendar.clone());
    let selector = view.selector;
    let page = view.page;
    let error = view.error.clone();
    let picker = world.get::<Picker>(owner).is_some();
    let header = row(world, body);
    button(
        world,
        header,
        owner,
        "‹",
        "Previous year",
        Command::Move(-12),
    );
    button(
        world,
        header,
        owner,
        &calendar.year.to_string(),
        "Choose year",
        Command::Selector(Some(Selector::Year(calendar.year))),
    );
    button(world, header, owner, "›", "Next year", Command::Move(12));
    button(
        world,
        header,
        owner,
        "‹",
        "Previous month",
        Command::Move(-1),
    );
    button(
        world,
        header,
        owner,
        MONTHS[(calendar.month - 1) as usize],
        "Choose month",
        Command::Selector(Some(Selector::Month)),
    );
    button(world, header, owner, "›", "Next month", Command::Move(1));
    button(world, header, owner, "×", "Close calendar", Command::Close);
    if !picker {
        let fields = row(world, body);
        for (end, date, title) in [
            (false, &calendar.start, "Start"),
            (true, &calendar.end, "End"),
        ] {
            let text = format!("{title}: {}", date.as_deref().unwrap_or("yyyy-mm-dd"));
            let entity = button(
                world,
                fields,
                owner,
                &text,
                &format!("Select {title} date"),
                Command::Endpoint(end),
            );
            if calendar.selecting_end == end {
                world
                    .entity_mut(entity)
                    .insert(crate::token_style::background(Token::Accent));
            }
        }
        button(
            world,
            fields,
            owner,
            "×",
            "Clear selected period",
            Command::Clear,
        );
        button(
            world,
            fields,
            owner,
            "Protein",
            "Choose a Protein Spawn Area",
            Command::Selector(Some(Selector::Source)),
        );
    } else {
        picker::fields(world, owner, body);
    }
    if let Some(error) = error {
        crate::edit_mode::label(world, body, &error, 14.0);
    }
    if let Some(selector) = selector {
        selectors(world, owner, body, selector);
        return;
    }
    let weekdays = row(world, body);
    for name in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
        let label = crate::edit_mode::label(world, weekdays, name, 12.0);
        world.get_mut::<Node>(label).unwrap().width = percent(100.0 / 7.0);
    }
    world.get_mut::<Node>(weekdays).unwrap().column_gap = px(0);
    let source = area(world, owner);
    let dates = calendar.days();
    let data = source
        .and_then(|source| crate::protein_area::calendar_feed(world, source).map(|(rows, _)| rows))
        .unwrap_or(&[]);
    let records = records(&calendar, data, page);
    let maximum = records.maximum;
    let page = records.page;
    let hidden = records.hidden;
    let entries = records.days;
    world.get_mut::<View>(owner).unwrap().page = page;
    let today = chrono::Local::now().date_naive();
    let grid = world
        .spawn((
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(body),
        ))
        .id();
    for week in 0..6 {
        let week_entity = world
            .spawn((
                Node {
                    width: percent(100),
                    height: percent(100.0 / 6.0),
                    min_height: px(0),
                    flex_shrink: 1.0,
                    ..default()
                },
                ChildOf(grid),
            ))
            .id();
        for weekday in 0..7 {
            let index = week * 7 + weekday;
            let cell = world
                .spawn((
                    Node {
                        width: percent(100.0 / 7.0),
                        height: percent(100),
                        min_width: px(0),
                        min_height: px(0),
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(px(1)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    ChildOf(week_entity),
                    crate::token_style::border(Token::SandBorder),
                ))
                .id();
            let Some(date) = dates[index] else { continue };
            let value = date.format("%Y-%m-%d").to_string();
            let selected = calendar
                .start
                .as_ref()
                .or(calendar.end.as_ref())
                .is_some_and(|start| start <= &value)
                && calendar
                    .end
                    .as_ref()
                    .or(calendar.start.as_ref())
                    .is_some_and(|end| &value <= end);
            if selected {
                world
                    .entity_mut(cell)
                    .insert(crate::token_style::background(Token::Accent));
            }
            let label = if date == today {
                format!("{} •", date.day())
            } else {
                date.day().to_string()
            };
            let day = button(
                world,
                cell,
                owner,
                &label,
                &format!("Select {value}"),
                Command::Select(value),
            );
            world.get_mut::<Node>(day).unwrap().width = percent(100);
            for (uid, title, tip) in &entries[index] {
                if let Some(source) = source {
                    let source_kind = world
                        .get::<InfluenceArea>(source)
                        .and_then(|a| a.protein.as_ref())
                        .unwrap()
                        .source
                        .clone();
                    let entity = button(
                        world,
                        cell,
                        owner,
                        title,
                        tip,
                        Command::Record(crate::protein_area::RecordBinding {
                            area: source,
                            uid: uid.clone(),
                            source: source_kind,
                        }),
                    );
                    let mut node = world.get_mut::<Node>(entity).unwrap();
                    node.width = percent(100);
                    node.min_height = px(18);
                    node.padding = UiRect::axes(px(3), px(1));
                }
            }
        }
    }
    if let Some(source) = source {
        let footer = row(world, body);
        let state = crate::protein_area::calendar_status(world, source).to_string();
        let count =
            crate::protein_area::calendar_feed(world, source).map_or(0, |(data, _)| data.len());
        let label =
            crate::edit_mode::label(world, footer, &format!("{state} · {count} Records"), 12.0);
        world.entity_mut(label).insert(Tooltip(format!("{hidden} without a valid date or period. Results follow the Protein Area's filters and row limit.")));
        if maximum > PER_DAY {
            button(
                world,
                footer,
                owner,
                "‹",
                "Previous records",
                Command::Page(page.saturating_sub(1)),
            );
            crate::edit_mode::label(
                world,
                footer,
                &format!("{} / {}", page + 1, maximum.div_ceil(PER_DAY)),
                12.0,
            );
            button(
                world,
                footer,
                owner,
                "›",
                "More records",
                Command::Page((page + 1).min(maximum.saturating_sub(1) / PER_DAY)),
            );
        }
    } else if calendar.area.is_some() {
        crate::edit_mode::label(world, body, "Protein Area unavailable", 12.0);
    }
}

pub(super) struct MonthRecords {
    pub days: Vec<Vec<(String, String, String)>>,
    pub page: usize,
    pub maximum: usize,
    pub hidden: usize,
}

pub(super) fn records(
    calendar: &Calendar,
    data: &[serde_json::Value],
    requested_page: usize,
) -> MonthRecords {
    let days = calendar.days();
    let mut spans = Vec::new();
    let mut counts = [0usize; 42];
    let mut hidden = 0;
    for (index, record) in data.iter().enumerate() {
        let Some((start, end)) = model::span(record) else {
            hidden += 1;
            continue;
        };
        let first = days
            .iter()
            .position(|day| day.is_some_and(|day| start <= day && day <= end));
        let last = days
            .iter()
            .rposition(|day| day.is_some_and(|day| start <= day && day <= end));
        if let Some((first, last)) = first.zip(last) {
            for count in &mut counts[first..=last] {
                *count += 1;
            }
            spans.push((index, first, last, start, end));
        }
    }
    let maximum = counts.into_iter().max().unwrap_or(0);
    let page = requested_page.min(maximum.saturating_sub(1) / PER_DAY);
    let mut result = MonthRecords {
        days: vec![Vec::new(); 42],
        page,
        maximum,
        hidden,
    };
    let mut seen = [0usize; 42];
    for (index, first, last, start, end) in spans {
        let record = &data[index];
        let Some(uid) = record["uid"].as_str() else {
            continue;
        };
        let title = record["head"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or(uid);
        for (day, count) in seen.iter_mut().enumerate().take(last + 1).skip(first) {
            if *count / PER_DAY == page {
                result.days[day].push((
                    uid.into(),
                    title.into(),
                    format!("{title}\n{start} – {end}"),
                ));
            }
            *count += 1;
        }
    }
    result
}

fn selectors(world: &mut World, owner: Entity, body: Entity, selector: Selector) {
    let choices = row(world, body);
    match selector {
        Selector::Month => {
            for (index, name) in MONTHS.iter().enumerate() {
                button(
                    world,
                    choices,
                    owner,
                    name,
                    name,
                    Command::Month(index as u32 + 1),
                );
            }
        }
        Selector::Year(first) => {
            let first = first.clamp(1, 9988);
            button(
                world,
                choices,
                owner,
                "‹",
                "Earlier years",
                Command::Selector(Some(Selector::Year((first - 12).max(1)))),
            );
            for year in first..first + 12 {
                button(
                    world,
                    choices,
                    owner,
                    &year.to_string(),
                    &year.to_string(),
                    Command::Year(year),
                );
            }
            button(
                world,
                choices,
                owner,
                "›",
                "Later years",
                Command::Selector(Some(Selector::Year((first + 12).min(9988)))),
            );
        }
        Selector::Source => {
            button(
                world,
                choices,
                owner,
                "None",
                "Disconnect Protein Area",
                Command::Source(None),
            );
            let root = world.get::<ChildOf>(owner).unwrap().parent();
            let workspace = world.get::<WorkspaceMember>(owner).unwrap().0;
            let sources: Vec<_> = world
                .query::<(&InfluenceArea, &ChildOf, &WorkspaceMember)>()
                .iter(world)
                .filter(|(area, parent, member)| {
                    parent.parent() == root && member.0 == workspace && area.protein.is_some()
                })
                .map(|(area, _, _)| (area.id.clone(), area.name.clone()))
                .collect();
            for (id, name) in sources {
                button(
                    world,
                    choices,
                    owner,
                    &name,
                    "Use this Protein Spawn Area",
                    Command::Source(Some(id)),
                );
            }
        }
    }
    button(
        world,
        body,
        owner,
        "×",
        "Close selector",
        Command::Selector(None),
    );
}
