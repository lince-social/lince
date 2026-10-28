use super::*;
use nucleus::property_rule::{Property, PropertyRule};

fn column(world: &mut World, parent: Entity) -> Entity {
    world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), flex_shrink: 0.0, ..Default::default() }, ChildOf(parent))).id()
}

fn condition(world: &mut World, parent: Entity, owner: Entity, value: &PropertyRule) -> Condition {
    let row = column(world, parent);
    let choice = button(world, row, owner, value.property.name(), EditChange::Property(row));
    let input = field(world, row, "Exact matching value", 4096);
    world.get_mut::<EditableText>(input).unwrap().editor.set_text(&value.value);
    button(world, row, owner, "Remove condition", EditChange::RemoveCondition(row));
    Condition { row, property: value.property, choice, value: input }
}

fn contents(world: &mut World, parent: Entity, owner: Entity, root: Entity, payload: Option<&Payload>, window: bool, creating: bool, title: &str, app_id: Option<&str>) -> (Kind, fields::Form, Option<Entity>) {
    let name = creating.then(|| {
        let input = field(world, parent, "New rule name", 160);
        world.get_mut::<EditableText>(input).unwrap().editor.set_text(title);
        input
    });
    if !creating { wrapped(world, parent, title, 18.0); }
    let (kind, properties) = if window {
        let properties = match payload { Some(Payload::Window(window)) => window.properties.clone(), _ => Properties::default() };
        (Kind::Window, properties)
    } else {
        let rule = match payload {
            Some(Payload::Rule(rule)) => rule.clone(),
            _ => Rule { revision: 1, enabled: true, match_all: true, conditions: vec![PropertyRule { property: Property::ApplicationId, value: app_id.unwrap_or_default().into() }], properties: Properties::default() },
        };
        wrapped(world, parent, "Match application metadata", 18.0);
        wrapped(world, parent, "Conditions use the same exact-value matcher as Areas. New windows may only have a title and application ID; other Record fields match when available. Metadata cannot grant activation, launch or protocol permissions.", 14.0);
        let enabled_control = button(world, parent, owner, "Rule enabled", EditChange::Enabled);
        let all_control = button(world, parent, owner, "Match all conditions", EditChange::MatchAll);
        let list = column(world, parent);
        let conditions = rule.conditions.iter().map(|value| condition(world, list, owner, value)).collect();
        button(world, parent, owner, "Add condition", EditChange::AddCondition);
        (Kind::Rule { enabled: rule.enabled, all: rule.match_all, conditions, list, enabled_control, all_control }, rule.properties)
    };
    let form = fields::Form::spawn(world, parent, owner, root, &properties);
    (kind, form, name)
}

pub(super) fn open(world: &mut World, root: Entity, uid: Option<String>, is_window: Option<bool>, app_id: Option<String>) {
    if shortcuts::keyboard_owned(world) || world.get::<Workspaces>(root).is_none() { return; }
    let window = is_window.unwrap_or(false);
    let creating = uid.is_none() && !window;
    let spaces = world.get::<Workspaces>(root).unwrap();
    let (canvas, workspace) = (spaces.canvas_id.clone(), spaces.active);
    let output = world.get_resource::<crate::native::output_views::OutputViews>().filter(|views| views.applies_to(root)).map(|views| views.focused);
    if window && uid.is_none() { return; }
    let previous = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let entry = uid.as_deref().and_then(|uid| record(world, uid, window));
    let payload = entry.as_ref().and_then(|entry| entry.payload.as_ref().ok()).filter(|payload| matches!((window, payload), (true, Payload::Window(_)) | (false, Payload::Rule(_)))).cloned();
    let title = entry.as_ref().map(|entry| entry.title.clone()).unwrap_or_else(|| if creating { String::new() } else { uid.clone().unwrap_or_default() });
    let overlay = world.spawn((
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..Default::default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), GlobalZIndex(100), TabGroup::modal(),
        crate::native::inspection::InspectionExcluded, Name::new("park.opening-editor"), ChildOf(root),
        KeyBindings(vec![KeyBinding::new(KeyCode::Escape, Modifiers::NONE, crate::native::actions![EditChange::Cancel])]),
    )).id();
    let mut accessible = accesskit::Node::new(accesskit::Role::Dialog);
    accessible.set_label(if window { "Window opening overrides" } else { "Application opening rule" });
    world.entity_mut(overlay).insert(bevy::a11y::AccessibilityNode::from(accessible));
    let panel = column(world, overlay);
    world.entity_mut(panel).insert((
        Node { width: px(640), max_width: percent(96), height: percent(90), padding: UiRect::all(px(14)), row_gap: px(8), flex_direction: FlexDirection::Column, min_height: px(0), ..Default::default() },
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    wrapped(world, panel, if window { "Window opening overrides" } else { "Application opening rule" }, 22.0);
    let content = column(world, panel);
    world.entity_mut(content).insert((
        Node { width: percent(100), min_width: px(0), min_height: px(0), flex_grow: 1.0, flex_shrink: 1.0, flex_direction: FlexDirection::Column, row_gap: px(8), overflow: Overflow::scroll_y(), ..Default::default() },
        ScrollPosition::default(), crate::native::edit_mode::FocusScroll,
    )).observe(|mut event: On<Pointer<Scroll>>| event.propagate(false));
    let (kind, form, name) = contents(world, content, overlay, root, payload.as_ref(), window, creating, &title, app_id.as_deref());
    let status = feedback(world, panel);
    let save = button(world, panel, overlay, "Save opening preferences", EditChange::Save).0;
    let actions = column(world, panel);
    let reload = button(world, actions, overlay, "Reload saved Record", EditChange::Reload).0;
    let open = button(world, actions, overlay, "Open Record", EditChange::Open).0;
    let cancel = button(world, actions, overlay, "Cancel", EditChange::Cancel).0;
    let loaded = creating || payload.is_some();
    let revision = payload.as_ref().map_or(0, Payload::revision);
    world.entity_mut(overlay).insert(Editor { root, canvas, workspace, output, content, previous, uid, title, name, kind, form, origin: payload, raw: entry.map(|entry| entry.value), revision, creating, needs_link: creating, loaded, reloading: false, pending: false, reload: false, message: String::new(), status, save, secondary: [reload, open] });
    shortcuts::own_keyboard(world, overlay);
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(name.unwrap_or(cancel), FocusCause::Navigated); }
    refresh(world);
}

fn record(world: &World, uid: &str, window: bool) -> Option<Entry> {
    let catalog = world.get_resource::<Catalog>()?;
    let feed = if window { &catalog.windows } else { &catalog.rules };
    feed.entries.get(uid).cloned()
}

fn input(world: &World, entity: Entity) -> Result<String, String> {
    let input = world.get::<EditableText>(entity).ok_or("The rule field is unavailable.")?;
    if input.is_composing() || !input.pending_edits.is_empty() || input.pending_paste.is_some() { return Err("Finish text composition or the pending paste before saving.".into()); }
    Ok(input.value().to_string())
}

fn candidate(world: &World, state: &Editor) -> Result<Payload, String> {
    let revision = state.revision.checked_add(1).ok_or("The Record revision is exhausted.")?;
    let properties = state.form.properties(world)?;
    let payload = match &state.kind {
        Kind::Window => Payload::Window(Window { revision, properties }),
        Kind::Rule { enabled, all, conditions, .. } => {
            let conditions = conditions.iter().map(|condition| Ok(PropertyRule { property: condition.property, value: input(world, condition.value)? })).collect::<Result<Vec<_>, String>>()?;
            Payload::Rule(Rule { revision, enabled: *enabled, match_all: *all, conditions, properties })
        }
    };
    payload.validate()?;
    if state.creating {
        let name = input(world, state.name.ok_or("Give the new rule a name.")?)?;
        if name.trim().is_empty() || name.chars().count() > 160 || name.chars().any(char::is_control) { return Err("Give the new rule a name of 1–160 characters.".into()); }
    }
    Ok(payload)
}

fn eligible(world: &World, owner: Entity, state: &Editor) -> bool {
    shortcuts::input_eligible(world, state.root, &state.canvas, owner)
        && world.get::<Workspaces>(state.root).is_some_and(|spaces| spaces.active == state.workspace)
        && state.output.is_none_or(|output| world.get_resource::<crate::native::output_views::OutputViews>().is_some_and(|views| {
            views.applies_to(state.root) && views.focused == output && views.outputs.iter().any(|view| view.id == output && view.workspace == state.workspace)
        }))
}

fn stale(world: &World, owner: Entity, state: &Editor) -> Option<String> {
    if !eligible(world, owner, state) { return Some("The original workspace or output view is unavailable. Your draft is kept; return to that view or cancel.".into()); }
    if !state.loaded || state.reloading { return Some("Loading the selected Record…".into()); }
    if state.reload { return Some("The Record or Cell changed. Your draft is kept; Reload saved Record before saving again.".into()); }
    if world.get::<Workspaces>(state.root).is_none_or(|spaces| spaces.canvas_id != state.canvas) { return Some("The original canvas is unavailable. The draft remains attached to its captured Record.".into()); }
    if state.creating { return None; }
    let entry = state.uid.as_deref().and_then(|uid| record(world, uid, matches!(&state.kind, Kind::Window)));
    match entry {
        Some(entry) if entry.exists && state.raw.as_ref() == Some(&entry.value) => None,
        Some(entry) if !entry.exists => Some("The selected Record is missing or unreadable. Your draft is kept.".into()),
        Some(_) => Some("Saved opening preferences changed. Your draft is kept; Reload saved Record to review them.".into()),
        None => Some("The selected Record is loading or unavailable. Your draft is kept.".into()),
    }
}

fn unchanged(state: &Editor, payload: &Payload) -> bool {
    let mut value = payload.clone();
    match &mut value { Payload::Rule(rule) => rule.revision = state.revision, Payload::Window(window) => window.revision = state.revision }
    state.origin.as_ref() == Some(&value)
}

fn close(world: &mut World, owner: Entity, state: Editor) {
    let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner));
    let previous = state.previous.filter(|entity| eligible(world, owner, &state) && previous_input(world, &state, *entity));
    shortcuts::release_keyboard(world, owner);
    world.despawn(owner);
    if owns && let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        if let Some(previous) = previous { focus.set(previous, FocusCause::Navigated); }
        else { focus.clear(); }
    }
}

fn previous_input(world: &World, state: &Editor, previous: Entity) -> bool {
    shortcuts::input_eligible(world, state.root, &state.canvas, previous)
}

fn reload(world: &mut World, owner: Entity, state: &mut Editor, entry: Entry) {
    state.loaded = true;
    if !entry.exists {
        if state.creating {
            state.loaded = true; state.reload = false; state.raw = None;
            state.message = "This proposed Record does not exist in the refreshed snapshot. Retry keeps the same proposed identity.".into();
        } else { state.reload = true; state.message = "The original Record is missing or unreadable; your draft is kept.".into(); }
        state.reloading = false;
        return;
    }
    if entry.value.is_null() && state.needs_link && matches!(&state.kind, Kind::Rule { .. }) {
        state.loaded = true; state.reloading = false; state.creating = false; state.reload = false;
        state.revision = 0; state.origin = None; state.raw = Some(Value::Null); state.title = entry.title;
        state.message = "The rule Record exists without opening preferences. Save retries this same Record.".into();
        return;
    }
    let payload = match entry.payload {
        Ok(payload) if matches!((&state.kind, &payload), (Kind::Window, Payload::Window(_)) | (Kind::Rule { .. }, Payload::Rule(_))) => payload,
        Ok(_) => { state.reload = true; state.reloading = false; state.message = "The Record has the wrong opening-preference type.".into(); return; }
        Err(error) => { state.reload = true; state.reloading = false; state.message = error; return; }
    };
    let children: Vec<_> = world.get::<Children>(state.content).map(|children| children.iter().collect()).unwrap_or_default();
    for child in children { world.despawn(child); }
    let window = matches!(&state.kind, Kind::Window);
    let (kind, form, name) = contents(world, state.content, owner, state.root, Some(&payload), window, false, &entry.title, None);
    state.kind = kind; state.form = form; state.name = name; state.title = entry.title;
    state.revision = payload.revision(); state.origin = Some(payload); state.raw = Some(entry.value);
    state.loaded = true; state.reloading = false; state.creating = false; state.reload = false;
    state.message = "Saved Record loaded. Existing windows keep their placement until Reapply.".into();
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(state.save, FocusCause::Navigated); }
}

fn link(world: &mut World, state: &mut Editor, uid: &str) -> Result<(), String> {
    if !state.needs_link { return Ok(()); }
    let old = active_order(world);
    if old.iter().any(|current| current == uid) { state.needs_link = false; return Ok(()); }
    if old.len() >= nucleus::window_opening::MAX_RULES { return Err("Record saved. The rule list is full; add this UID after removing another reference.".into()); }
    let mut layout = world.get_resource::<LayoutSettings>().cloned().unwrap_or_default();
    layout.opening_rules.push(uid.into());
    let settings = world.get_resource::<Settings>().cloned().unwrap_or_default();
    configuration::commit(world, settings, layout)?;
    let active = active_order(world);
    let pages: Vec<_> = world.query_filtered::<Entity, With<Page>>().iter(world).collect();
    for page in pages {
        let mut page = world.get_mut::<Page>(page).unwrap();
        if page.origin == old && page.order == old { page.order = active.clone(); page.origin = active.clone(); }
    }
    state.needs_link = false;
    Ok(())
}

pub(super) fn completed(world: &mut World, owner: Entity, completion: catalog::Completion) {
    let Some(mut state) = world.get_entity_mut(owner).ok().and_then(|mut entity| entity.take::<Editor>()) else {
        crate::native::notifications::report(world, "Opening preferences", &format!("Record {}: {}", completion.uid, completion.result.map(|_| "saved; its closed editor did not change rule order".to_owned()).unwrap_or_else(|error| error)));
        return;
    };
    if !state.pending || state.uid.as_deref() != Some(completion.uid.as_str()) { world.entity_mut(owner).insert(state); return; }
    state.pending = false;
    if completion.created { state.creating = false; }
    match completion.result {
        Ok(()) => {
            state.creating = false; state.loaded = true; state.reload = false;
            state.revision = completion.payload.revision(); state.raw = Some(completion.payload.value()); state.origin = Some(completion.payload);
            state.message = if eligible(world, owner, &state) && shortcuts::keyboard_owner(world) == Some(owner) {
                match link(world, &mut state, &completion.uid) {
                    Ok(()) => "Opening preferences saved. Existing windows keep their placement until Reapply.".into(),
                    Err(error) => format!("Record {} saved, but its rule reference is not active: {error}. Add the UID in the rule list or retry Save.", completion.uid),
                }
            } else { format!("Record {} saved. Its original view is no longer active, so rule order was not changed. Return to that view and Save to activate its reference.", completion.uid) };
        }
        Err(error) => { state.reload = true; state.message = format!("{error} Record: {}. Your draft is kept.", completion.uid); }
    }
    world.entity_mut(owner).insert(state);
}

impl Action for EditChange {
    fn apply(&self, world: &mut World, owner: Entity) {
        if !world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner)) || world.get::<Editor>(owner).is_none() { return; }
        let Some(mut state) = world.entity_mut(owner).take::<Editor>() else { return; };
        if matches!(self, Self::Cancel) { close(world, owner, state); return; }
        if state.pending { world.entity_mut(owner).insert(state); return; }
        if !eligible(world, owner, &state) {
            state.message = "The original workspace or output view is unavailable. Your draft is kept; return to that view or cancel.".into();
            world.entity_mut(owner).insert(state);
            refresh(world);
            return;
        }
        state.message.clear();
        match self {
            Self::Save => {
                let result = (|| {
                    if let Some(error) = stale(world, owner, &state) { return Err(error); }
                    let payload = candidate(world, &state)?;
                    if unchanged(&state, &payload) && state.needs_link {
                        let uid = state.uid.clone().ok_or("The saved Record identity is unavailable.")?;
                        return link(world, &mut state, &uid);
                    }
                    if state.creating { state.title = input(world, state.name.unwrap())?.trim().into(); }
                    let uid = catalog::save(world, owner, state.uid.clone(), state.title.clone(), payload, state.creating)?;
                    state.uid = Some(uid); state.pending = true;
                    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(owner, FocusCause::Navigated); }
                    Ok(())
                })();
                state.message = result.map(|_| if state.pending { "Saving the captured Record…".into() } else { "Rule reference saved.".into() }).unwrap_or_else(|error| error);
            }
            Self::Reload => {
                if state.uid.is_none() && state.creating {
                    state.reload = false;
                    state.loaded = true;
                    state.message = "New rule draft reviewed for the current local Cell. No Record has been created.".into();
                } else { state.reloading = true; state.reload = true; catalog::refresh(world); }
            }
            Self::Open => {
                if let Some(uid) = state.uid.clone() {
                    let root = state.root;
                    close(world, owner, state);
                    open_record(world, root, &uid);
                    return;
                }
                state.message = "Save the new rule before opening its Record.".into();
            }
            Self::Enabled => if let Kind::Rule { enabled, .. } = &mut state.kind { *enabled = !*enabled; },
            Self::MatchAll => if let Kind::Rule { all, .. } = &mut state.kind { *all = !*all; },
            Self::AddCondition => if let Kind::Rule { conditions, list, .. } = &mut state.kind {
                if conditions.len() < 32 { conditions.push(condition(world, *list, owner, &PropertyRule { property: Property::ApplicationId, value: String::new() })); }
                else { state.message = "A rule can contain at most 32 conditions.".into(); }
            },
            Self::RemoveCondition(row) => if let Kind::Rule { conditions, .. } = &mut state.kind {
                if conditions.len() > 1 && conditions.iter().any(|condition| condition.row == *row) {
                    conditions.retain(|condition| condition.row != *row); world.despawn(*row);
                    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(state.save, FocusCause::Navigated); }
                } else { state.message = "Keep at least one matching condition.".into(); }
            },
            Self::Property(row) => if let Kind::Rule { conditions, .. } = &mut state.kind
                && let Some(condition) = conditions.iter_mut().find(|condition| condition.row == *row) {
                let index = Property::ALL.iter().position(|property| *property == condition.property).unwrap_or(0);
                condition.property = Property::ALL[(index + 1) % Property::ALL.len()];
            },
            Self::Workspace(workspace) => { state.form.workspace = *workspace; state.form.close_workspace(world); }
            Self::Flag(index) => if let Some(flag) = state.form.flags.get_mut(*index) { *flag = match *flag { None => Some(true), Some(true) => Some(false), Some(false) => None }; },
            Self::Cancel => unreachable!(),
        }
        world.entity_mut(owner).insert(state);
        refresh(world);
    }
}

impl Action for OutputChoice {
    fn apply(&self, world: &mut World, owner: Entity) {
        if !world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner)) || world.get::<Editor>(owner).is_none() { return; }
        let Some(mut state) = world.entity_mut(owner).take::<Editor>() else { return; };
        if !state.pending && eligible(world, owner, &state) { state.form.output = self.0.clone(); state.form.close_output(world); }
        world.entity_mut(owner).insert(state);
        refresh(world);
    }
}

pub(super) fn context_changed(world: &mut World) {
    for mut state in world.query::<&mut Editor>().iter_mut(world) {
        state.reload = true;
        state.message = "The Cell connection or login changed. Reload this Record before saving; your draft is kept.".into();
    }
}

fn enable_content(world: &mut World, root: Entity, disabled: bool) {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(children) = world.get::<Children>(entity) { stack.extend(children.iter()); }
        if world.get::<EditableText>(entity).is_some() || world.get::<ActionButton>(entity).is_some() { disable(world, entity, disabled); }
    }
}

pub(super) fn refresh(world: &mut World) {
    let owners: Vec<_> = world.query_filtered::<Entity, With<Editor>>().iter(world).collect();
    for owner in owners {
        let mut state = world.entity_mut(owner).take::<Editor>().unwrap();
        if !state.pending && (!state.loaded || state.reloading)
            && let Some(entry) = state.uid.as_deref().and_then(|uid| record(world, uid, matches!(&state.kind, Kind::Window))) {
            reload(world, owner, &mut state, entry);
        }
        if !state.pending && (!state.loaded || state.reloading) {
            let error = world.get_resource::<Catalog>().and_then(|catalog| if matches!(&state.kind, Kind::Window) { catalog.windows.error.clone() } else { catalog.rules.error.clone() });
            if let Some(error) = error { state.message = error; state.reloading = false; state.loaded = true; state.reload = true; }
        }
        state.form.refresh(world, state.root);
        if let Kind::Rule { enabled, all, conditions, enabled_control, all_control, .. } = &state.kind {
            caption(world, *enabled_control, if *enabled { "Rule enabled: Yes" } else { "Rule enabled: No" });
            caption(world, *all_control, if *all { "Match all conditions" } else { "Match any condition" });
            for condition in conditions { caption(world, condition.choice, condition.property.name()); }
        }
        enable_content(world, state.content, state.pending || !state.loaded || state.reloading);
        disable(world, state.secondary[0], state.pending || state.reloading);
        disable(world, state.secondary[1], state.pending || state.uid.is_none());
        if !state.creating && let Some(name) = state.name { disable(world, name, true); }
        let payload = candidate(world, &state);
        let error = stale(world, owner, &state).or_else(|| payload.as_ref().err().cloned());
        let dirty = payload.as_ref().is_ok_and(|payload| !unchanged(&state, payload));
        let message = if state.pending { state.message.clone() } else if let Some(error) = &error {
            if state.message.is_empty() { error.clone() } else { format!("{error}\n{}", state.message) }
        } else { state.message.clone() };
        text(world, state.status, message);
        disable(world, state.save, state.pending || error.is_some() || (!dirty && !state.needs_link));
        world.entity_mut(owner).insert(state);
    }
}
