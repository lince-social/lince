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
    Transport,
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
    CopyReceipt,
    CopyText(String),
    ReceiptPage(usize),
    ImportReceiptFile,
    ExportReceiptFile,
    ReviewParticipation,
    Preview,
    ViewAs(Option<String>),
    SelectOccurrence(String),
    Bulk,
    Record(String),
    AcceptReview,
    Find,
    Simulate(String),
    SimulateLoan(String),
    SimulateBalance(Value),
    SimulateChildren,
    SimulateShared(String, usize),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Create) {
            let workspace = world
                .get::<crate::native::workspace::Workspaces>(owner)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::native::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(world, owner, workspace, position, TransferCastle::default());
            return;
        }
        if world.get::<View>(owner).is_none() || crate::native::laboratory::suspended(world, owner)
        {
            return;
        }
        capture(world, owner);
        let (Some(castle), Some(view)) =
            (world.get::<TransferCastle>(owner), world.get::<View>(owner))
        else {
            return;
        };
        if view.pending.as_ref().is_some_and(|pending| {
            !matches!(self, Self::Person(_))
                || !matches!(
                    pending.form.data["action"].as_str(),
                    Some("transfer-receipt" | "identity")
                )
        }) {
            status(world, owner, "Waiting for the Cell to finish this action");
            return;
        }
        match self {
            Self::Transport => {
                let scope = super::transport_workspace::scope(world, owner, view);
                let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
                    return;
                };
                if castle.form.is_some() {
                    status(world, owner, "Save or cancel the open form first");
                    return;
                }
                let selection = castle.selected.clone();
                castle.workflow.enter(&scope, &selection);
                castle.selected.clear();
            }
            Self::SimulateChildren => {
                let _prepare = nucleus::performance::Span::new("transfer-action-input-preparation");
                let rows = &view.rows;
                let root = &castle.selected;
                let person = person(world, owner);
                let result = crate::native::simulation_castle::transfer_entry::group_balance(
                    rows, root, &person,
                );
                drop(_prepare);
                let result = result.and_then(|balance| {
                    crate::native::simulation_castle::transfer_entry::open_balance(
                        world, owner, &balance,
                    )
                });
                if let Err(error) = result {
                    status(world, owner, error.to_string());
                }
                return;
            }
            Self::SimulateBalance(balance) => {
                if let Err(error) = crate::native::simulation_castle::transfer_entry::open_balance(
                    world, owner, balance,
                ) {
                    status(world, owner, error.to_string());
                }
                return;
            }
            Self::Simulate(uid) | Self::SimulateLoan(uid) | Self::SimulateShared(uid, _) => {
                let form = castle.form.clone();
                let row = if let Some(mut form) = form.filter(|form| form.step.is_some()) {
                    if let Err(error) = form.commit_fields() {
                        status(world, owner, error);
                        return;
                    }
                    Some(form.data)
                } else {
                    selected(world, owner)
                };
                let Some(transfer) = row else {
                    return;
                };
                let shared = if matches!(self, Self::SimulateShared(_, _)) {
                    nucleus::simulation::sharing::Shared::parse(uid)
                } else {
                    None
                };
                let index = if let Self::SimulateShared(_, index) = self {
                    *index
                } else {
                    0
                };
                let promise_uid = shared
                    .as_ref()
                    .and_then(|shared| shared.assumptions.get(index))
                    .map_or(uid.as_str(), |assumption| {
                        assumption.source.promise.as_str()
                    });
                let Some(promise) = array(&transfer, "promises")
                    .iter()
                    .find(|promise| text(promise, "uid") == promise_uid)
                else {
                    status(
                        world,
                        owner,
                        "The assumed item is unavailable in this proposal",
                    );
                    return;
                };
                let person = person(world, owner);
                if let Err(error) = crate::native::simulation_castle::transfer_entry::open(
                    world,
                    owner,
                    &transfer,
                    promise,
                    &person,
                    shared.as_ref().map(|shared| (shared, index)),
                    matches!(self, Self::SimulateLoan(_)),
                ) {
                    status(world, owner, error.to_string());
                }
                return;
            }
            Self::Create => {}
            Self::Refresh => runtime::refresh(world, owner),
            Self::ViewAs(person) => {
                let has_form = castle.form.is_some();
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    return;
                };
                view.disclosure_preview = Some(person.clone());
                if has_form {
                    forms::render(world, owner);
                    return;
                }
            }
            Self::Select(uid) => {
                let Ok((mut castle, mut view)) = world
                    .query::<(&mut TransferCastle, &mut View)>()
                    .get_mut(world, owner)
                else {
                    return;
                };
                castle.selected = uid.clone();
                view.selected_occurrences.clear();
            }
            Self::Mine(mine) => {
                let Ok((mut castle, mut view)) = world
                    .query::<(&mut TransferCastle, &mut View)>()
                    .get_mut(world, owner)
                else {
                    return;
                };
                castle.mine = *mine;
                view.page = 0;
            }
            Self::Filter(filter) => {
                let Ok((mut castle, mut view)) = world
                    .query::<(&mut TransferCastle, &mut View)>()
                    .get_mut(world, owner)
                else {
                    return;
                };
                castle.filter = filter.clone();
                view.page = 0;
            }
            Self::Sort(sort) => {
                if let Some(mut castle) = world.get_mut::<TransferCastle>(owner) {
                    castle.sort = sort.clone();
                }
            }
            Self::Tree(tree) => {
                if let Some(mut castle) = world.get_mut::<TransferCastle>(owner) {
                    castle.tree = *tree;
                }
            }
            Self::Page(page) => {
                if let Some(mut view) = world.get_mut::<View>(owner) {
                    view.page = *page;
                }
            }
            Self::ReceiptPage(page) => {
                if let Some(mut view) = world.get_mut::<View>(owner) {
                    view.receipt_page = *page;
                }
            }
            Self::Person(person) => {
                let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
                    return;
                };
                castle.person = person.clone();
                super::receipts::clear_form(world, owner);

                let Some(mut view) = world.get_mut::<View>(owner) else {
                    return;
                };
                view.receipt = None;
                if view.pending.as_ref().is_some_and(|pending| {
                    matches!(
                        pending.form.data["action"].as_str(),
                        Some("transfer-receipt" | "identity")
                    )
                }) {
                    view.pending = None;
                }
                view.receipt_page = 0;
                view.receipt_file_job = None;
                view.receipt_list = Value::Null;
                view.receipt_export = None;
            }
            Self::CopyReceipt | Self::CopyText(_) => {
                let document = if let Self::CopyText(text) = self {
                    Some(text.clone())
                } else {
                    view.receipt_export.clone()
                };
                let Some(document) = document else {
                    return;
                };
                let result = world
                    .get_resource_mut::<bevy::clipboard::Clipboard>()
                    .ok_or_else(|| "Clipboard is unavailable".to_string())
                    .and_then(|mut clipboard| {
                        clipboard
                            .set_text(&document)
                            .map_err(|error| error.to_string())
                    });
                status(
                    world,
                    owner,
                    match result {
                        Ok(()) => {
                            if matches!(self, Self::CopyText(_)) {
                                "Copied".into()
                            } else {
                                "Receipt document copied".into()
                            }
                        }
                        Err(error) => error,
                    },
                );
                return;
            }
            Self::ImportReceiptFile => {
                receipt_files::choose(world, owner, false);
                return;
            }
            Self::ExportReceiptFile => {
                receipt_files::choose(world, owner, true);
                return;
            }
            Self::Toggle(key) => {
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    return;
                };
                if !view.expanded.remove(key) {
                    view.expanded.insert(key.clone());
                }
            }
            Self::New(preset) => {
                if !capability(&view.context, "create") {
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
                let Ok((mut castle, mut view)) = world
                    .query::<(&mut TransferCastle, &mut View)>()
                    .get_mut(world, owner)
                else {
                    return;
                };
                castle.form = None;
                view.preview_request = None;
                runtime::cancel(world, owner, "preview");
                forms::render(world, owner);
            }
            Self::Submit => {
                runtime::submit(world, owner);
                return;
            }
            Self::Step(step) => {
                let Some(mut form) = castle.form.clone() else {
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
                let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
                    return;
                };
                castle.form = Some(form);
                forms::render(world, owner);
                return;
            }
            Self::Set(index, value) => {
                if let Some(mut castle) = world.get_mut::<TransferCastle>(owner)
                    && let Some(field) = castle
                        .form
                        .as_mut()
                        .and_then(|form| form.fields.get_mut(*index))
                {
                    field.value = value.clone();
                }
                forms::render(world, owner);
                return;
            }
            Self::Add(key) | Self::Remove(key, _) => {
                let Some(mut form) = castle.form.clone() else {
                    return;
                };
                if let Err(error) = form.commit_fields() {
                    status(world, owner, error);
                    return;
                }
                if key == "public_history" {
                    form.data[key] = if matches!(self, Self::Remove(_, _)) {
                        Value::Null
                    } else {
                        match serde_json::to_value(
                            nucleus::transfer::public_history::Terms::default(),
                        ) {
                            Ok(terms) => terms,
                            Err(error) => {
                                status(
                                    world,
                                    owner,
                                    format!("Could not prepare public history: {error}"),
                                );
                                return;
                            }
                        }
                    };
                }
                let new = if key == "promises" {
                    model::promise(&text(&form.data, "creator"))
                } else if key == "effects" {
                    json!({"record":"","formula":"incoming()","mode":"fulfilment"})
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
                if form.step.is_none() {
                    form.data["request_id"] = json!(form.request_id);
                }
                let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
                    return;
                };
                castle.form = Some(form);
                forms::render(world, owner);
                return;
            }
            Self::Preview => {
                forms::request_preview(world, owner);
                return;
            }
            Self::ReviewParticipation => {
                let Some(mut form) = castle.form.clone() else {
                    return;
                };
                if let Err(error) = receipts::review_participation_form(&mut form) {
                    status(world, owner, error);
                    return;
                }
                let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
                    return;
                };
                castle.form = Some(form);
                forms::render(world, owner);
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
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    return;
                };
                if !view.selected_occurrences.remove(uid) {
                    view.selected_occurrences.insert(uid.clone());
                }
            }
            Self::Bulk => {
                forms::bulk(world, owner);
                return;
            }
            Self::Record(uid) => {
                let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
                    return;
                };
                let source = crate::native::practice_cells::source(world, owner)
                    .map(crate::native::protein_area::Source::Organ)
                    .unwrap_or(crate::native::protein_area::Source::Local);
                crate::native::full_record::open(world, root, uid, source);
                return;
            }
        }
        render(world, owner);
    }
}

fn open(world: &mut World, owner: Entity, form: Form) {
    let Ok((mut castle, mut view)) = world
        .query::<(&mut TransferCastle, &mut View)>()
        .get_mut(world, owner)
    else {
        return;
    };
    if castle.form.is_some() {
        status(world, owner, "Save or cancel the open form first");
        return;
    }
    view.preview = None;
    castle.form = Some(form);
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
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_height = px(0);
        node.min_width = px(0);
        node.padding = UiRect::all(px(6));
        node.overflow = Overflow::scroll_y();
    }
    crate::native::scroll_sand::attach(world, entity);
    entity
}
pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    title: &str,
    command: Command,
) -> Entity {
    crate::native::castle_feed::button(world, parent, owner, title, command)
}
pub(super) fn picker(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    label: &str,
    choices: Vec<(String, Command)>,
) {
    crate::native::dropdown::spawn(
        world,
        parent,
        owner,
        label,
        label,
        choices
            .into_iter()
            .map(|(label, command)| (label, crate::native::actions![command]))
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
    if index.is_none()
        && let Some(mut node) = world.get_mut::<Node>(group)
    {
        node.width = px(200);
    }
    crate::native::edit_mode::label(world, group, label, 12.0);
    let entity = world
        .spawn(crate::native::sand::text_editor(
            value,
            world.resource::<crate::native::theme::Typography>(),
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
    let Some(mut editor) = world.get_mut::<EditableText>(entity) else {
        world.despawn(group);
        return;
    };
    editor.allow_newlines = multiline;
    editor.max_characters = Some(if multiline { 16_384 } else { 1024 });
    editor.visible_lines = Some(if multiline { 4.0 } else { 1.0 });
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(label);
    }
}

fn capture_fields(world: &mut World, owner: Option<Entity>) -> HashSet<Entity> {
    let _capture = nucleus::performance::Span::new("transfer-input-capture");
    let mut values: HashMap<(Entity, Option<usize>), String> = HashMap::new();
    for (input, text) in world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| owner.is_none_or(|owner| input.owner == owner))
    {
        let Some(castle) = world.get::<TransferCastle>(input.owner) else {
            continue;
        };
        let current = if let Some(index) = input.index {
            let Some(field) = castle.form.as_ref().and_then(|form| form.fields.get(index)) else {
                continue;
            };
            &field.value
        } else {
            &castle.search
        };
        let key = (input.owner, input.index);
        let value = text.value();
        if value == values.get(&key).map_or(current.as_str(), String::as_str) {
            continue;
        }
        nucleus::performance::add("transfer-field-values-prepared", 1);
        values.insert(key, value.to_string());
    }
    let mut searches = HashSet::new();
    for ((owner, index), value) in values {
        let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
            continue;
        };
        if let Some(index) = index {
            if let Some(form) = &mut castle.bypass_change_detection().form
                && let Some(field) = form.fields.get_mut(index)
                && field.value != value
            {
                field.value = value;
                castle.set_changed();
            }
        } else if castle.search != value {
            castle.search = value;
            searches.insert(owner);
        }
    }
    searches
}

pub(super) fn capture(world: &mut World, owner: Entity) {
    audience::capture(world, owner);
    capture_fields(world, Some(owner));
}
pub(super) fn inputs(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<TransferCastle>>()
        .iter(world)
        .collect();
    for &owner in &owners {
        audience::capture(world, owner);
    }
    let searches = capture_fields(world, None);
    for owner in owners.into_iter().filter(|owner| searches.contains(owner)) {
        if let Some(mut view) = world.get_mut::<View>(owner) {
            view.page = 0;
            render(world, owner);
        }
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    audience::capture(world, owner);
    if world.get::<TransferCastle>(owner).is_none() || world.get::<View>(owner).is_none() {
        return;
    }
    if super::transport_workspace::render(world, owner) {
        return;
    }
    let (Some(castle), Some(view)) = (world.get::<TransferCastle>(owner), world.get::<View>(owner))
    else {
        return;
    };
    let (controls, summary, list, detail) = (view.controls, view.summary, view.list, view.detail);
    let mut prepared = super::presentation::prepare(owner, castle, view);
    if view.page != prepared.page
        && let Some(mut view) = world.get_mut::<View>(owner)
    {
        view.page = prepared.page;
    }
    let selected = prepared.detail.take();
    super::presentation::render(world, controls, summary, list, prepared);
    clear(world, detail);
    super::receipts::render(world, owner, detail);
    if let Some(transfer) = selected {
        super::detail::render(world, owner, detail, &transfer);
    } else {
        crate::native::edit_mode::label(
            world,
            detail,
            "Select a transfer to review its terms, people, progress, and proof.",
            15.0,
        );
    }
}
