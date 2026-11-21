// non-lean-yet
// Scope: fn update.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 90, 102, 103,
//   107, 110.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (5, 14, 38, 39, 146).

use super::*;
use rows::{Command, Key, Row};

pub(super) fn update(world: &mut World, owner: Entity, now: i64) {
    let view = world.get::<View>(owner).unwrap();
    if view.detail_revision == view.revision {
        return;
    }
    let span = nucleus::performance::Span::new("time-detail-preparation");
    let parent = view.details;
    let selected = !view.selected.is_empty();
    let selected_ids: std::collections::HashSet<_> =
        view.selected.iter().map(String::as_str).collect();
    let settings = &world.get::<TimeSettings>(owner).unwrap().0;
    let mut entries: Vec<_> = view
        .entries
        .iter()
        .filter_map(|entry| {
            if selected && !selected_ids.contains(entry.id.as_str()) {
                return None;
            }
            let category = (!selected).then(|| entry.category_at(now, &settings.timezone));
            (selected
                || category != Some(model::Category::Timed)
                || entry
                    .effective_time(now)
                    .is_some_and(|time| time.overlaps(now, now + settings.horizon_ms)))
            .then_some((entry, category))
        })
        .collect();
    if !selected {
        let rank = |category| match category {
            model::Category::Overdue => 0,
            model::Category::AllDay => 1,
            model::Category::Timed => 2,
        };
        entries.sort_by(|(a, a_category), (b, b_category)| {
            rank(a_category.unwrap())
                .cmp(&rank(b_category.unwrap()))
                .then_with(|| {
                    a.time
                        .as_ref()
                        .map(|time| time.from_ms)
                        .cmp(&b.time.as_ref().map(|time| time.from_ms))
                })
                .then_with(|| a.id.cmp(&b.id))
        });
    }
    let pages = entries.len().div_ceil(20).max(1);
    let page = view.page.min(pages - 1);
    let mut parts = vec![
        (
            Key::Controls,
            Row::Controls {
                all: true,
                pages: pages > 1,
            },
        ),
        (
            Key::Summary,
            Row::Label(
                format!(
                    "{} occurrence(s) · page {} / {pages}",
                    entries.len(),
                    page + 1
                ),
                12.0,
            ),
        ),
    ];
    let mut previous_category = None;
    let mut occurrences = HashMap::<&str, usize>::new();
    for (index, (entry, category)) in entries.iter().skip(page * 20).take(20).enumerate() {
        let category = category.unwrap_or_else(|| entry.category_at(now, &settings.timezone));
        if previous_category != Some(category) {
            let caption = match category {
                model::Category::Timed => "Future schedule",
                model::Category::AllDay => "All-day work",
                model::Category::Overdue => "Overdue work",
            };
            parts.push((Key::Category(index), Row::Label(caption.into(), 15.0)));
            previous_category = Some(category);
        }
        let category = match category {
            model::Category::Timed => "Scheduled",
            model::Category::AllDay => "All day",
            model::Category::Overdue => "Overdue",
        };
        let occurrence = occurrences.entry(&entry.id).or_default();
        let id = entry.id.clone();
        let caption = format!(
            "{category} · {}\n{}{}",
            headline(&entry.head),
            time_label(entry, settings, now),
            if entry.preview {
                " · projection preview (read only)"
            } else {
                ""
            }
        );
        parts.push((
            Key::Event(id.clone(), *occurrence),
            Row::Button(caption, Command::Select(id.clone()), true),
        ));
        if !entry.preview && matches!(view.source, Source::Local) {
            parts.push((
                Key::Date(id.clone(), *occurrence),
                Row::Button(
                    "Try another date".into(),
                    Command::TryDate(entry.record_uid.clone()),
                    false,
                ),
            ));
        }
        if selected {
            parts.push((
                Key::Origin(id, *occurrence),
                Row::Label(origin_label(entry, settings), 11.0),
            ));
        }
        *occurrence += 1;
    }
    if entries.is_empty() {
        parts.push((
            Key::Empty,
            Row::Label("No scheduled work in this window".into(), 12.0),
        ));
    }
    let peek = if selected {
        chrome::prepare_peek(
            world,
            owner,
            entries.iter().map(|(entry, _)| *entry),
            settings,
        )
    } else {
        Vec::new()
    };
    crate::host::performance::add(
        crate::host::performance::Count::TimeDetailRowsPrepared,
        parts.len() as u64,
    );
    drop(span);
    rows::present(world, parent, owner, parts);
    chrome::present_peek(world, owner, peek);
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.page = page;
    view.detail_revision = view.revision;
}
