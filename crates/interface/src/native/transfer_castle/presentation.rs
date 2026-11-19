use super::*;
use crate::native::retained::Presentation;
use ui::{Command, button, clear, picker, stack};

mod rows;

#[derive(PartialEq)]
struct Controls {
    owner: Entity,
    mine: bool,
    filter: String,
    sort: String,
    tree: bool,
    total: usize,
    mine_count: usize,
    counts: [usize; 7],
    person: Option<(String, Vec<(String, String)>)>,
}

pub(super) struct Prepared {
    controls: Controls,
    cards: Vec<rows::Card>,
    pub(super) page: usize,
    total: usize,
    empty: &'static str,
    selected: String,
    pub(super) detail: Option<Value>,
}

pub(super) fn prepare(owner: Entity, castle: &TransferCastle, view: &View) -> Prepared {
    let _span = nucleus::performance::Span::new("transfer-page-preparation");
    let mut counts = [0; 7];
    let mut mine_count = 0;
    for row in &view.rows {
        let mine = model::facet(row, "mine");
        mine_count += usize::from(mine);
        if !castle.mine || mine {
            for (count, (key, _)) in counts.iter_mut().zip(model::FILTERS) {
                *count += usize::from(model::facet(row, key));
            }
        }
    }
    let controls = Controls {
        owner,
        mine: castle.mine,
        filter: castle.filter.clone(),
        sort: castle.sort.clone(),
        tree: castle.tree,
        total: view.rows.len(),
        mine_count,
        counts,
        person: (view.context["viewer"]["local"] == true).then(|| {
            (
                castle.person.clone(),
                view.records
                    .iter()
                    .filter(|row| row["kind"] == "person")
                    .map(|row| (title(row), text(row, "uid")))
                    .collect(),
            )
        }),
    };
    let mut visible = model::filtered_ref(
        &view.rows,
        castle.mine,
        &castle.filter,
        &castle.search,
        &castle.sort,
    );
    let hierarchy = castle.tree.then(|| model::Hierarchy::new(&view.rows));
    if let Some(hierarchy) = &hierarchy {
        visible.sort_by_cached_key(|&row| hierarchy.path(row));
    }
    let page = view.page.min(visible.len().saturating_sub(1) / 30);
    let cards: Vec<_> = visible
        .iter()
        .skip(page * 30)
        .take(30)
        .map(|row| rows::Card {
            uid: text(row, "uid"),
            title: title(row),
            status: format!(
                "{} · revision {}",
                model::human(&model::status_ref(row)),
                text(row, "revision")
            ),
            parties: array(row, "parties")
                .iter()
                .map(|party| model::display_ref(&party["actor_head"]))
                .collect::<Vec<_>>()
                .join(" · "),
            counts: format!(
                "{} promises · {} occurrences",
                array(row, "promises").len(),
                array(row, "occurrences").len()
            ),
            awaiting: model::facet(row, "awaiting_me"),
            depth: hierarchy
                .as_ref()
                .map_or(0, |hierarchy| hierarchy.depth(row).min(8)),
        })
        .collect();
    nucleus::performance::add("transfer-page-cards-prepared", cards.len() as u64);
    Prepared {
        controls,
        cards,
        page,
        total: visible.len(),
        empty: if !view.ready {
            "Waiting for transfers…"
        } else if view.rows.is_empty() {
            "No transfers yet. Create your first commitment."
        } else {
            "No transfers match this view."
        },
        selected: castle.selected.clone(),
        detail: view
            .rows
            .iter()
            .find(|row| model::display_ref(&row["uid"]) == castle.selected)
            .cloned(),
    }
}

pub(super) fn render(
    world: &mut World,
    controls: Entity,
    summary: Entity,
    list: Entity,
    prepared: Prepared,
) {
    let _span = nucleus::performance::Span::new("transfer-list-presentation");
    let Prepared {
        controls: state,
        cards,
        page,
        total,
        empty,
        selected,
        ..
    } = prepared;
    let owner = state.owner;
    let summary_counts = [
        state.counts[0],
        state.counts[1],
        state.counts[3],
        state.counts[4],
    ];
    if Presentation::matches(world, controls, |old: &Controls| old == &state) {
        nucleus::performance::add("transfer-regions-retained", 1);
    } else {
        clear(world, controls);
        button(world, controls, owner, "Transport", Command::Transport);
        crate::native::trust_castle::entry_controls(world, controls);
        for (mine, label, count) in [
            (false, "All", state.total),
            (true, "Mine", state.mine_count),
        ] {
            button(
                world,
                controls,
                owner,
                &format!(
                    "{}{} {count}",
                    if state.mine == mine { "● " } else { "" },
                    label
                ),
                Command::Mine(mine),
            );
        }
        for ((key, label), count) in model::FILTERS.into_iter().zip(state.counts) {
            button(
                world,
                controls,
                owner,
                &format!(
                    "{}{} {count}",
                    if state.filter == key { "● " } else { "" },
                    label
                ),
                Command::Filter(key.into()),
            );
        }
        picker(
            world,
            controls,
            owner,
            &format!("Sort: {}", state.sort),
            ["attention", "name", "status"]
                .into_iter()
                .map(|sort| (sort.into(), Command::Sort(sort.into())))
                .collect(),
        );
        button(
            world,
            controls,
            owner,
            if state.tree { "● Tree" } else { "List" },
            Command::Tree(!state.tree),
        );
        if let Some((person, people)) = &state.person {
            picker(
                world,
                controls,
                owner,
                &format!(
                    "Acting Person: {}",
                    if person.is_empty() { "Choose" } else { person }
                ),
                people
                    .iter()
                    .map(|(title, uid)| (title.clone(), Command::Person(uid.clone())))
                    .collect(),
            );
        }
        Presentation::remember(world, controls, state);
        nucleus::performance::add("transfer-regions-rebuilt", 1);
    }
    if Presentation::matches(world, summary, |old: &[usize; 4]| old == &summary_counts) {
        nucleus::performance::add("transfer-regions-retained", 1);
    } else {
        clear(world, summary);
        for (label, count) in ["Total", "Awaiting me", "Active", "Completed"]
            .into_iter()
            .zip(summary_counts)
        {
            let card = stack(world, summary);
            let mut node = world.get_mut::<Node>(card).unwrap();
            node.width = percent(23);
            node.padding = UiRect::all(px(10));
            world
                .entity_mut(card)
                .insert(crate::native::token_style::background(
                    crate::native::tokens::Token::CanvasBackground,
                ));
            crate::native::edit_mode::label(world, card, label, 12.0);
            crate::native::edit_mode::label(world, card, &count.to_string(), 24.0);
        }
        Presentation::remember(world, summary, summary_counts);
        nucleus::performance::add("transfer-regions-rebuilt", 1);
    }
    rows::render(world, list, owner, cards, &selected, page, total, empty);
}
