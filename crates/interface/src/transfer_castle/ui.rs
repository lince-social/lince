use super::*;
use bevy::text::EditableText;

#[derive(Component)]
struct Input {
    owner: Entity,
    index: Option<usize>,
}

#[derive(Clone)]
pub(super) enum Command {
    Create,
    New(String),
    Refresh,
    Select(String),
    Mine(bool),
    Filter(String),
    Sort(String),
    Tree(bool),
    Page(usize),
    Person(String),
    Toggle(String),
    Open(Form),
    Edit(bool),
    Cancel,
    Step(usize),
    Set(usize, String),
    Add(String),
    Remove(String, usize),
    Submit,
    Preview,
    SelectOccurrence(String),
    Bulk,
    Record(String),
    AcceptReview,
    Find,
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Create) {
            let workspace = world
                .get::<crate::workspace::Workspaces>(owner)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(world, owner, workspace, position, TransferCastle::default());
            return;
        }
        if world.get::<View>(owner).is_none() || crate::laboratory::suspended(world, owner) {
            return;
        }
        capture(world, owner);
        if world.get::<View>(owner).unwrap().pending.is_some() {
            status(world, owner, "Waiting for the Cell to finish this action");
            return;
        }
        match self {
            Self::Create => {}
            Self::Refresh => runtime::refresh(world, owner),
            Self::Select(uid) => {
                world.get_mut::<TransferCastle>(owner).unwrap().selected = uid.clone();
                world
                    .get_mut::<View>(owner)
                    .unwrap()
                    .selected_occurrences
                    .clear();
            }
            Self::Mine(mine) => {
                world.get_mut::<TransferCastle>(owner).unwrap().mine = *mine;
                world.get_mut::<View>(owner).unwrap().page = 0;
            }
            Self::Filter(filter) => {
                world.get_mut::<TransferCastle>(owner).unwrap().filter = filter.clone();
                world.get_mut::<View>(owner).unwrap().page = 0;
            }
            Self::Sort(sort) => world.get_mut::<TransferCastle>(owner).unwrap().sort = sort.clone(),
            Self::Tree(tree) => world.get_mut::<TransferCastle>(owner).unwrap().tree = *tree,
            Self::Page(page) => world.get_mut::<View>(owner).unwrap().page = *page,
            Self::Person(person) => {
                world.get_mut::<TransferCastle>(owner).unwrap().person = person.clone()
            }
            Self::Toggle(key) => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                if !view.expanded.remove(key) {
                    view.expanded.insert(key.clone());
                }
            }
            Self::New(preset) => {
                if !capability(&world.get::<View>(owner).unwrap().context, "create") {
                    status(
                        world,
                        owner,
                        "Creation is unavailable. Connect a Person signer to create transfers.",
                    );
                    return;
                }
                open(world, owner, model::composer(&person(world, owner), preset));
                return;
            }
            Self::Open(form) => {
                open(world, owner, form.clone());
                return;
            }
            Self::Edit(counteroffer) => {
                if let Some(row) = selected(world, owner) {
                    open(
                        world,
                        owner,
                        model::edit(&row, *counteroffer, &person(world, owner)),
                    );
                }
                return;
            }
            Self::Cancel => {
                world.get_mut::<TransferCastle>(owner).unwrap().form = None;
                runtime::cancel(world, owner, "preview");
                world.get_mut::<View>(owner).unwrap().preview_request = None;
                forms::render(world, owner);
            }
            Self::Submit => {
                runtime::submit(world, owner);
                return;
            }
            Self::Step(step) => {
                let Some(mut form) = world.get::<TransferCastle>(owner).unwrap().form.clone()
                else {
                    return;
                };
                if let Err(error) = form.commit_fields() {
                    status(world, owner, error);
                    return;
                }
                if form.step == Some(1) {
                    model::bind_preset(&mut form);
                }
                if *step == 4
                    && let Err(error) = form.payload()
                {
                    status(world, owner, error);
                    return;
                }
                form.step = Some(*step);
                world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form);
                forms::render(world, owner);
                return;
            }
            Self::Set(index, value) => {
                if let Some(form) = &mut world.get_mut::<TransferCastle>(owner).unwrap().form
                    && let Some(field) = form.fields.get_mut(*index)
                {
                    field.value = value.clone();
                }
                forms::render(world, owner);
                return;
            }
            Self::Add(key) | Self::Remove(key, _) => {
                let Some(mut form) = world.get::<TransferCastle>(owner).unwrap().form.clone()
                else {
                    return;
                };
                if let Err(error) = form.commit_fields() {
                    status(world, owner, error);
                    return;
                }
                let new = if key == "promises" {
                    model::promise(&text(&form.data, "creator"))
                } else {
                    model::dependency()
                };
                if let Some(items) = form.data[key].as_array_mut() {
                    if let Self::Remove(_, index) = self {
                        if *index < items.len() {
                            items.remove(*index);
                        }
                    } else if items.len() < 64 {
                        items.push(new);
                    }
                }
                form.fields.clear();
                form.request_id = nucleus::new_uid("transfer-ui");
                world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form);
                forms::render(world, owner);
                return;
            }
            Self::Preview => {
                forms::request_preview(world, owner);
                return;
            }
            Self::AcceptReview => {
                forms::accept_review(world, owner);
                return;
            }
            Self::Find => {
                forms::render(world, owner);
                return;
            }
            Self::SelectOccurrence(uid) => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                if !view.selected_occurrences.remove(uid) {
                    view.selected_occurrences.insert(uid.clone());
                }
            }
            Self::Bulk => {
                forms::bulk(world, owner);
                return;
            }
            Self::Record(uid) => {
                let root = world.get::<ChildOf>(owner).unwrap().parent();
                crate::full_record::open(world, root, uid, crate::protein_area::Source::Local);
                return;
            }
        }
        render(world, owner);
    }
}

fn open(world: &mut World, owner: Entity, form: Form) {
    if world.get::<TransferCastle>(owner).unwrap().form.is_some() {
        status(world, owner, "Save or cancel the open form first");
        return;
    }
    world.get_mut::<View>(owner).unwrap().preview = None;
    world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form);
    forms::render(world, owner);
}

pub(super) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                column_gap: px(8),
                row_gap: px(6),
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}
pub(super) fn stack(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}
pub(super) fn scroll(world: &mut World, parent: Entity) -> Entity {
    let entity = stack(world, parent);
    {
        let mut node = world.get_mut::<Node>(entity).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_height = px(0);
        node.min_width = px(0);
        node.padding = UiRect::all(px(6));
        node.overflow = Overflow::scroll_y();
    }
    crate::scroll_sand::attach(world, entity);
    entity
}
pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    title: &str,
    command: Command,
) -> Entity {
    crate::castle_feed::button(world, parent, owner, title, command)
}
pub(super) fn picker(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    label: &str,
    choices: Vec<(String, Command)>,
) {
    crate::dropdown::spawn(
        world,
        parent,
        owner,
        label,
        label,
        choices
            .into_iter()
            .map(|(label, command)| (label, crate::actions![command]))
            .collect(),
    );
}
pub(super) fn clear(world: &mut World, parent: Entity) {
    let children: Vec<_> = world
        .get::<Children>(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}
pub(super) fn input(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    index: Option<usize>,
    label: &str,
    value: &str,
    multiline: bool,
) {
    let group = stack(world, parent);
    if index.is_none() {
        world.get_mut::<Node>(group).unwrap().width = px(200);
    }
    crate::edit_mode::label(world, group, label, 12.0);
    let entity = world
        .spawn(crate::sand::text_editor(
            value,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    world.entity_mut(entity).insert((
        Node {
            width: percent(100),
            min_height: px(if multiline { 90 } else { 34 }),
            ..default()
        },
        ChildOf(group),
        Input { owner, index },
    ));
    let mut editor = world.get_mut::<EditableText>(entity).unwrap();
    editor.allow_newlines = multiline;
    editor.max_characters = Some(if multiline { 16_384 } else { 1024 });
    editor.visible_lines = Some(if multiline { 4.0 } else { 1.0 });
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(label);
    }
}

pub(super) fn capture(world: &mut World, owner: Entity) {
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.index, text.value().to_string()))
        .collect();
    let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
        return;
    };
    for (index, value) in values {
        if let Some(index) = index {
            if let Some(form) = &mut castle.form
                && let Some(field) = form.fields.get_mut(index)
            {
                field.value = value;
            }
        } else {
            castle.search = value;
        }
    }
}
pub(super) fn inputs(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<TransferCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        let previous = world.get::<TransferCastle>(owner).unwrap().search.clone();
        capture(world, owner);
        if previous != world.get::<TransferCastle>(owner).unwrap().search {
            world.get_mut::<View>(owner).unwrap().page = 0;
            render(world, owner);
        }
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let castle = world.get::<TransferCastle>(owner).unwrap().clone();
    let view = world.get::<View>(owner).unwrap();
    let (controls, summary, list, detail, page) = (
        view.controls,
        view.summary,
        view.list,
        view.detail,
        view.page,
    );
    let rows = view.rows.clone();
    let context = view.context.clone();
    let ready = view.ready;
    clear(world, controls);
    clear(world, summary);
    clear(world, list);
    clear(world, detail);
    for (mine, label) in [(false, "All"), (true, "Mine")] {
        let count = rows
            .iter()
            .filter(|row| !mine || model::facet(row, "mine"))
            .count();
        button(
            world,
            controls,
            owner,
            &format!(
                "{}{} {count}",
                if castle.mine == mine { "● " } else { "" },
                label
            ),
            Command::Mine(mine),
        );
    }
    let scoped: Vec<_> = rows
        .iter()
        .filter(|row| !castle.mine || model::facet(row, "mine"))
        .collect();
    for (key, label) in model::FILTERS {
        let count = scoped.iter().filter(|row| model::facet(row, key)).count();
        button(
            world,
            controls,
            owner,
            &format!(
                "{}{} {count}",
                if castle.filter == key { "● " } else { "" },
                label
            ),
            Command::Filter(key.into()),
        );
    }
    picker(
        world,
        controls,
        owner,
        &format!("Sort: {}", castle.sort),
        ["attention", "name", "status"]
            .into_iter()
            .map(|sort| (sort.into(), Command::Sort(sort.into())))
            .collect(),
    );
    button(
        world,
        controls,
        owner,
        if castle.tree { "● Tree" } else { "List" },
        Command::Tree(!castle.tree),
    );
    for (key, label) in [
        ("all", "Total"),
        ("awaiting_me", "Awaiting me"),
        ("active", "Active"),
        ("completed", "Completed"),
    ] {
        let card = stack(world, summary);
        world.get_mut::<Node>(card).unwrap().width = percent(23);
        world.get_mut::<Node>(card).unwrap().padding = UiRect::all(px(10));
        world
            .entity_mut(card)
            .insert(crate::token_style::background(
                crate::tokens::Token::CanvasBackground,
            ));
        crate::edit_mode::label(world, card, label, 12.0);
        crate::edit_mode::label(
            world,
            card,
            &scoped
                .iter()
                .filter(|row| model::facet(row, key))
                .count()
                .to_string(),
            24.0,
        );
    }
    if context["viewer"]["local"] == true {
        let people: Vec<_> = world
            .get::<View>(owner)
            .unwrap()
            .records
            .iter()
            .filter(|row| row["kind"] == "person")
            .map(|row| (title(row), Command::Person(text(row, "uid"))))
            .collect();
        picker(
            world,
            controls,
            owner,
            &format!(
                "Acting Person: {}",
                if castle.person.is_empty() {
                    "Choose"
                } else {
                    &castle.person
                }
            ),
            people,
        );
    }
    let mut visible = model::filtered(
        &rows,
        castle.mine,
        &castle.filter,
        &castle.search,
        &castle.sort,
    );
    if castle.tree {
        visible.sort_by_key(|row| {
            let mut path = vec![title(row)];
            let mut parent = text(row, "parent");
            let mut seen = HashSet::from([text(row, "uid")]);
            while !parent.is_empty() && seen.insert(parent.clone()) {
                let Some(ancestor) = rows.iter().find(|row| text(row, "uid") == parent) else {
                    break;
                };
                path.push(title(ancestor));
                parent = text(ancestor, "parent");
            }
            path.reverse();
            path
        });
    }
    let page = page.min(visible.len().saturating_sub(1) / 30);
    world.get_mut::<View>(owner).unwrap().page = page;
    if visible.is_empty() {
        crate::edit_mode::label(
            world,
            list,
            if !ready {
                "Waiting for transfers…"
            } else if rows.is_empty() {
                "No transfers yet. Create your first commitment."
            } else {
                "No transfers match this view."
            },
            14.0,
        );
    }
    for transfer in visible.iter().skip(page * 30).take(30) {
        let uid = text(transfer, "uid");
        let card = button(
            world,
            list,
            owner,
            &title(transfer),
            Command::Select(uid.clone()),
        );
        {
            let mut node = world.get_mut::<Node>(card).unwrap();
            node.width = percent(100);
            node.flex_direction = FlexDirection::Column;
            node.align_items = AlignItems::Start;
            node.row_gap = px(5);
            node.padding = UiRect::all(px(12));
            if castle.tree {
                node.margin.left = px((model::depth(transfer, &rows).min(8) * 12) as f32);
            }
        }
        if uid == castle.selected {
            world
                .entity_mut(card)
                .insert(crate::token_style::border(crate::tokens::Token::Accent));
            world.get_mut::<Node>(card).unwrap().border = UiRect::all(px(1));
        }
        crate::edit_mode::label(
            world,
            card,
            &format!(
                "{} · revision {}",
                model::human(&model::status(transfer)),
                text(transfer, "revision")
            ),
            12.0,
        );
        let parties = array(transfer, "parties")
            .iter()
            .map(|party| text(party, "actor_head"))
            .collect::<Vec<_>>()
            .join(" · ");
        crate::edit_mode::label(world, card, &parties, 12.0);
        crate::edit_mode::label(
            world,
            card,
            &format!(
                "{} promises · {} occurrences",
                array(transfer, "promises").len(),
                array(transfer, "occurrences").len()
            ),
            12.0,
        );
        if model::facet(transfer, "awaiting_me") {
            crate::edit_mode::label(world, card, "Awaiting your response", 12.0);
        }
    }
    if visible.len() > 30 {
        let pages = row(world, list);
        if page > 0 {
            button(world, pages, owner, "Previous", Command::Page(page - 1));
        }
        crate::edit_mode::label(
            world,
            pages,
            &format!("{} / {}", page + 1, visible.len().div_ceil(30)),
            12.0,
        );
        if (page + 1) * 30 < visible.len() {
            button(world, pages, owner, "Next", Command::Page(page + 1));
        }
    }
    if let Some(transfer) = rows.iter().find(|row| text(row, "uid") == castle.selected) {
        super::detail::render(world, owner, detail, transfer);
    } else {
        crate::edit_mode::label(
            world,
            detail,
            "Select a transfer to review its terms, people, progress, and proof.",
            15.0,
        );
    }
}
