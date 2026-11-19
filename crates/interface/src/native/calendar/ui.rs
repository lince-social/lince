use super::*;
use crate::native::{actions::ActionButton, icons::Tooltip, tokens::Token};
use chrono::Datelike;

mod presentation;

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
            crate::native::sand::button(0),
            crate::native::sand::Square,
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
            ActionButton::new(owner, crate::native::actions![command]),
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(tip);
    }
    let label = crate::native::edit_mode::label(world, entity, text, 14.0);
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
    crate::native::sand_store::castle_entry(
        world,
        root,
        parent,
        "Calendar",
        "Browse dates and plan Records.",
        Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, Calendar::default()),
    );
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let _present = nucleus::performance::Span::new("calendar-presentation");
    if world.get::<CalendarSand>(owner).is_none() {
        return;
    }
    if !crate::native::output_views::presents_entity(world, owner) {
        world.get_mut::<View>(owner).unwrap().dirty = true;
        return;
    }
    let source = area(world, owner);
    let sources = presentation::sources(world, owner);
    let today = chrono::Local::now().date_naive();
    if presentation::matches(world, owner, source, &sources, today) {
        world.get_mut::<View>(owner).unwrap().dirty = false;
        nucleus::performance::add("calendar-frames-retained", 1);
        presentation::refresh(world, owner, source);
        return;
    }
    let calendar = world.get::<CalendarSand>(owner).unwrap().0.clone();
    nucleus::performance::add("calendar-frames-built", 1);
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
    view.dirty = false;
    let selector = view.selector;
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
                    .insert(crate::native::token_style::background(Token::Accent));
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
        button(
            world,
            fields,
            owner,
            &calendar.timezone,
            "Choose the timezone for projected dates",
            Command::Selector(Some(Selector::Timezone)),
        );
    } else {
        picker::fields(world, owner, body);
    }
    if let Some(error) = error {
        crate::native::edit_mode::label(world, body, &error, 14.0);
    }
    if let Some(selector) = selector {
        selectors(world, owner, body, selector, &sources);
        presentation::remember(world, owner, source, sources, today, None);
        return;
    }
    let weekdays = row(world, body);
    for name in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
        let label = crate::native::edit_mode::label(world, weekdays, name, 12.0);
        world.get_mut::<Node>(label).unwrap().width = percent(100.0 / 7.0);
    }
    world.get_mut::<Node>(weekdays).unwrap().column_gap = px(0);
    let dates = calendar.days();
    let grid = world
        .spawn((
            Node {
                width: percent(100),
                flex_grow: 1.0,
                flex_basis: px(0),
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(body),
        ))
        .id();
    let mut cells = Vec::with_capacity(42);
    for week in 0..6 {
        let week_entity = world
            .spawn((
                Node {
                    width: percent(100),
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    min_height: px(if picker { 28 } else { 0 }),
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
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(px(1)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    ChildOf(week_entity),
                    crate::native::token_style::border(Token::SandBorder),
                ))
                .id();
            cells.push(presentation::Day {
                cell,
                week: week_entity,
                header: None,
            });
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
                    .insert(crate::native::token_style::background(Token::Accent));
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
            if picker {
                let mut node = world.get_mut::<Node>(day).unwrap();
                node.height = percent(100);
                node.justify_content = JustifyContent::Center;
                node.align_items = AlignItems::Center;
            }
            cells.last_mut().unwrap().header = Some(day);
        }
    }
    let footer = if source.is_some() {
        Some(row(world, body))
    } else {
        if calendar.area.is_some() {
            crate::native::edit_mode::label(world, body, "Protein Area unavailable", 12.0);
        }
        None
    };
    presentation::remember(
        world,
        owner,
        source,
        sources,
        today,
        Some(presentation::Month {
            grid,
            days: cells,
            footer,
        }),
    );
    presentation::refresh(world, owner, source);
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
    let _prepare = nucleus::performance::Span::new("calendar-month-records");
    nucleus::performance::add("calendar-month-input-rows", data.len() as u64);
    let days = calendar.days();
    let dates = std::cell::OnceCell::new();
    let mut spans = Vec::new();
    let mut counts = [0usize; 42];
    let mut hidden = 0;
    for (index, record) in data.iter().enumerate() {
        if record["kind"] == "projection-status" {
            continue;
        }
        let dates = dates.get_or_init(|| {
            nucleus::performance::add("calendar-month-timezones", 1);
            calendar
                .timezone
                .parse()
                .ok()
                .map(nucleus::projection::Dates::new)
        });
        let Some((start, end)) = dates
            .as_ref()
            .and_then(|dates| model::span_with_dates(record, dates))
        else {
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
        let Some(uid) = record["record_uid"].as_str().or(record["uid"].as_str()) else {
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
                    if record["origin"]["kind"] == "projection" {
                        format!(
                            "{title} · {} projected",
                            record["quantity"].as_str().unwrap_or("")
                        )
                    } else {
                        title.into()
                    },
                    format!(
                        "{title}\n{start} – {end}\n{}",
                        if record["origin"]["kind"] == "projection" {
                            "Projected from Karma; may change when your data changes."
                        } else {
                            "Scheduled work"
                        }
                    ),
                ));
            }
            *count += 1;
        }
    }
    result
}

fn selectors(
    world: &mut World,
    owner: Entity,
    body: Entity,
    selector: Selector,
    sources: &[(String, String)],
) {
    let choices = row(world, body);
    match selector {
        Selector::Timezone => {
            let value = world.get::<CalendarSand>(owner).unwrap().0.timezone.clone();
            crate::native::edit_mode::label(
                world,
                body,
                "Timezone, for example America/Sao_Paulo or UTC",
                13.0,
            );
            world
                .spawn(crate::native::sand::text_editor(
                    &value,
                    world.resource::<crate::native::theme::Typography>(),
                    0,
                ))
                .insert((
                    projection::TimezoneInput(owner),
                    ChildOf(choices),
                    Node {
                        min_width: px(260),
                        ..default()
                    },
                ));
            button(
                world,
                choices,
                owner,
                "Apply",
                "Use this timezone",
                Command::ApplyTimezone,
            );
        }
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
            for (id, name) in sources {
                button(
                    world,
                    choices,
                    owner,
                    &name,
                    "Use this Protein Spawn Area",
                    Command::Source(Some(id.clone())),
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
