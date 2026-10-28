use super::{HostRequests, HostViewport, Workspaces, control, input_settings::{button, disable, feedback, field, next_request, text}};
use crate::{
    host::{HostRequest, OutputId, control::Context, magnification::{Adjustment, Outcome, Reload, Request, Snapshot, valid}},
    native::{actions::Action, edit_mode::label, shortcuts},
};
use bevy::{input_focus::{FocusCause, InputFocus}, prelude::*, text::EditableText};

#[derive(Component)]
struct Page {
    root: Entity,
    body: Entity,
    toggle: (Entity, Entity),
    summary: Entity,
    selector: Entity,
    selector_text: Entity,
    menu: Entity,
    choices: Vec<(OutputId, String)>,
    controls: Vec<Entity>,
    factor: Entity,
    save: Entity,
    cancel: Entity,
    reload: Entity,
    status: Entity,
    selected: Option<OutputId>,
    origin: Option<(u64, f64, Context)>,
    pending: Option<u64>,
    expanded: bool,
}

#[derive(Clone, Copy)]
enum Change { Toggle, Select(OutputId), Adjust(Adjustment), Save, Cancel, Reload }

#[derive(Resource, Default)]
struct ObservedResponse(u64);

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Snapshot>().init_resource::<ObservedResponse>().add_systems(PostUpdate, refresh.after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout));
}

fn column(world: &mut World, parent: Entity, display: Display) -> Entity {
    world.spawn((Node { display, width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..default() }, ChildOf(parent))).id()
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity) {
    let page = column(world, parent, Display::Flex);
    world.entity_mut(page).insert(Name::new("park.magnification"));
    label(world, page, "Accessibility", 20.0);
    let summary = label(world, page, "", 14.0);
    let toggle = button(world, page, page, "Edit display magnification", Change::Toggle);
    let body = column(world, page, Display::None);
    label(world, body, "Display magnification enlarges this Park output. Canvas zoom and application sizes stay independent. Use your chosen system screen reader for spoken controls.", 14.0);
    let selector = crate::native::dropdown::spawn(world, body, page, "Output to magnify", "No active outputs", Vec::new());
    let selector_text = world.get::<Children>(selector).unwrap().iter().find(|entity| world.get::<Text>(*entity).is_some()).unwrap();
    let menu = world.get::<crate::native::dropdown::Dropdown>(selector).unwrap().menu;
    world.get_mut::<Node>(menu).unwrap().max_height = px(200);
    crate::native::scroll_sand::attach(world, menu);
    let factor = field(world, body, "Display magnification factor (1–4)", 32);
    let controls = [
        ("Decrease draft by 0.25×", Change::Adjust(Adjustment::Decrease)),
        ("Increase draft by 0.25×", Change::Adjust(Adjustment::Increase)),
        ("Reset draft to 1×", Change::Adjust(Adjustment::Reset)),
    ].into_iter().map(|(caption, action)| button(world, body, page, caption, action).0).collect::<Vec<_>>();
    let save = button(world, body, page, "Save magnification", Change::Save).0;
    let cancel = button(world, body, page, "Cancel draft", Change::Cancel).0;
    let reload = button(world, body, page, "Discard draft and reload saved magnification", Change::Reload).0;
    let status = feedback(world, body);
    crate::native::accessibility::status(world, status);
    for entity in [summary, status] {
        world.get_mut::<Node>(entity).unwrap().min_width = px(0);
        world.entity_mut(entity).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    }
    world.entity_mut(page).insert(Page { root, body, toggle, summary, selector, selector_text, menu, choices: Vec::new(), controls, factor, save, cancel, reload, status,
        selected: None, origin: None, pending: None, expanded: false });
    refresh(world);
}

fn read(world: &World, field: Entity) -> Result<f64, String> {
    let input = world.get::<EditableText>(field).ok_or("The magnification field is unavailable.")?;
    if input.is_composing() || !input.pending_edits.is_empty() || input.pending_paste.is_some() { return Err("Finish typing before changing or saving magnification.".into()); }
    let factor = input.value().to_string().trim().parse::<f64>().map_err(|_| "Enter a magnification factor between 1 and 4.")?;
    if !valid(factor) { return Err("Enter a finite magnification factor between 1 and 4.".into()); }
    Ok(factor)
}

fn write(world: &mut World, entity: Entity, factor: f64) {
    if let Some(mut input) = world.get_mut::<EditableText>(entity) {
        input.pending_edits.clear();
        input.pending_paste = None;
        let value = factor.to_string();
        if input.value() != value.as_str() {
            input.editor.set_text(&value);
            input.queue_edit(bevy::text::TextEdit::TextEnd(false));
        }
    }
}

fn eligible(world: &World, page: Entity, root: Entity) -> bool {
    world.get::<Workspaces>(root).is_some_and(|spaces| shortcuts::input_eligible(world, root, &spaces.canvas_id, page))
        && !world.get_resource::<crate::host::control::Status>().is_some_and(|status| status.available && (status.locked || !status.active))
}

fn reset(world: &mut World, page: &mut Page, snapshot: &Snapshot) {
    if !page.selected.is_some_and(|output| snapshot.head(output).is_some()) {
        page.selected = world.get::<HostViewport>(page.root).map(|viewport| viewport.0.id).filter(|output| snapshot.head(*output).is_some())
            .or_else(|| snapshot.heads.first().map(|head| head.output));
    }
    page.origin = page.selected.and_then(|output| snapshot.head(output)).and_then(|head| {
        write(world, page.factor, head.requested);
        control::context(world, page.root).map(|context| (snapshot.revision, head.requested, context))
    });
}

fn send(world: &mut World, request: Request) -> Result<(), String> {
    queue(world, HostRequest::Magnification(request))
}

fn queue(world: &mut World, request: HostRequest) -> Result<(), String> {
    world.get_resource::<HostRequests>().ok_or("This control requires Park. Sandbox cannot magnify its parent desktop.")?
        .0.try_send(request).map_err(|_| "Park is busy or unavailable; the draft is preserved.")?;
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
    Ok(())
}

pub(crate) fn adjust(world: &mut World, root: Entity, adjustment: Adjustment) {
    let result = (|| {
        if world.query::<&Page>().iter(world).any(|page| page.pending.is_some()) { return Err("Wait for the pending magnification request before changing it again.".to_owned()); }
        let snapshot = world.get_resource::<Snapshot>().filter(|snapshot| snapshot.available).ok_or("Display magnification requires an available Park output.")?.clone();
        let output = world.get::<HostViewport>(root).ok_or("The original output is unavailable.")?.0.id;
        let factor = adjustment.factor(snapshot.head(output).ok_or("The original output is unavailable.")?.requested);
        let context = control::context(world, root).ok_or("The original Park view is unavailable.")?;
        let id = next_request(world, snapshot.last_request)?;
        send(world, Request { id, revision: snapshot.revision, output, factor, context })
    })();
    if let Err(reason) = result { crate::native::notifications::report(world, "Display magnification", &reason); }
}

impl Action for Change {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(mut page) = world.entity_mut(entity).take::<Page>() else { return; };
        if !eligible(world, entity, page.root) {
            text(world, page.status, "The original settings view is unavailable. Your draft is preserved.");
            world.entity_mut(entity).insert(page);
            return;
        }
        let snapshot = world.get_resource::<Snapshot>().cloned().unwrap_or_default();
        if matches!(self, Self::Toggle) {
            page.expanded = !page.expanded;
            world.get_mut::<Node>(page.body).unwrap().display = if page.expanded { Display::Flex } else { Display::None };
            let caption = if page.expanded { "Hide display magnification" } else { "Edit display magnification" };
            text(world, page.toggle.1, caption);
            world.get_mut::<bevy::a11y::AccessibilityNode>(page.toggle.0).unwrap().set_label(caption);
            if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(if page.expanded && snapshot.available { page.factor } else { page.toggle.0 }, FocusCause::Navigated); }
        } else if page.pending.is_none() {
            let result = (|| {
                match *self {
                    Self::Cancel => {
                        reset(world, &mut page, &snapshot);
                        text(world, page.status, "Draft discarded; current Park settings restored.");
                    }
                    Self::Reload => {
                        if world.query::<&Page>().iter(world).any(|page| page.pending.is_some()) { return Err("Another magnification request is pending.".into()); }
                        let context = control::context(world, page.root).ok_or("The current Park view is unavailable.")?;
                        let id = next_request(world, snapshot.last_request)?;
                        queue(world, HostRequest::MagnificationReload(Reload { id, context }))?;
                        page.pending = Some(id);
                        text(world, page.status, "Reading saved magnification.");
                    }
                    Self::Select(output) => {
                        if page.origin.as_ref().is_some_and(|(_, origin, _)| read(world, page.factor).ok() != Some(*origin)) {
                            return Err("Save or cancel this draft before selecting another output.".to_owned());
                        }
                        if snapshot.head(output).is_none() { return Err("That output is no longer available.".into()); }
                        page.selected = Some(output);
                        reset(world, &mut page, &snapshot);
                        world.get_mut::<Node>(page.menu).unwrap().display = Display::None;
                        world.get_mut::<bevy::a11y::AccessibilityNode>(page.selector).unwrap().set_expanded(false);
                        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(page.factor, FocusCause::Navigated); }
                    }
                    Self::Adjust(adjustment) => {
                        let factor = read(world, page.factor)?;
                        write(world, page.factor, adjustment.factor(factor));
                    }
                    Self::Save => {
                        if world.query::<&Page>().iter(world).any(|page| page.pending.is_some()) { return Err("Another magnification request is pending.".into()); }
                        let factor = read(world, page.factor)?;
                        let (revision, _, context) = page.origin.clone().ok_or("The magnification settings are still loading.")?;
                        if control::context(world, page.root).as_ref() != Some(&context) { return Err("The original Park view changed. Cancel reloads the preserved draft.".into()); }
                        let output = page.selected.ok_or("Choose an available output.")?;
                        if snapshot.revision != revision || snapshot.head(output).is_none() { return Err("Outputs or saved settings changed. Cancel reloads the preserved draft.".into()); }
                        let id = next_request(world, snapshot.last_request)?;
                        send(world, Request { id, revision, output, factor, context })?;
                        page.pending = Some(id);
                        text(world, page.status, "Saving magnification.");
                    }
                    Self::Toggle => {}
                }
                Ok::<(), String>(())
            })();
            if let Err(reason) = result { text(world, page.status, reason); }
        }
        world.entity_mut(entity).insert(page);
    }
}

fn refresh(world: &mut World) {
    let snapshot = world.get_resource::<Snapshot>().cloned().unwrap_or_default();
    if let Some(response) = snapshot.response.as_ref()
        && response.request > world.resource::<ObservedResponse>().0
    {
        world.resource_mut::<ObservedResponse>().0 = response.request;
        let inline = world.query::<&Page>().iter(world).any(|page| page.pending == Some(response.request));
        if !inline {
            let message = match &response.outcome { Outcome::Failed(reason) => Some(reason.as_str()), Outcome::Saved { warning } => warning.as_deref(), Outcome::Loaded => None };
            if let Some(message) = message { crate::native::notifications::report(world, "Display magnification", message); }
        }
    }
    let pages = world.query_filtered::<Entity, With<Page>>().iter(world).collect::<Vec<_>>();
    for entity in pages {
        let mut page = world.entity_mut(entity).take::<Page>().unwrap();
        let choices = snapshot.heads.iter().map(|head| (head.output, head.name.clone())).collect::<Vec<_>>();
        if page.choices != choices {
            let children = world.get::<Children>(page.menu).map(|children| children.iter().collect::<Vec<_>>()).unwrap_or_default();
            let focus = world.get_resource::<InputFocus>().and_then(InputFocus::get);
            let returning = focus.is_some_and(|focus| children.contains(&focus));
            for child in children { world.despawn(child); }
            for (output, name) in &choices { button(world, page.menu, entity, name, Change::Select(*output)); }
            page.choices = choices;
            if returning && let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(page.selector, FocusCause::Navigated); }
        }
        if page.origin.is_none() && page.pending.is_none() { reset(world, &mut page, &snapshot); }
        if let Some(request) = page.pending
            && let Some(response) = snapshot.response.as_ref().filter(|response| response.request == request)
        {
            match &response.outcome {
                Outcome::Saved { warning } => {
                    page.pending = None;
                    reset(world, &mut page, &snapshot);
                    text(world, page.status, warning.as_deref().unwrap_or("Magnification saved; applying to the output."));
                }
                Outcome::Loaded => { page.pending = None; reset(world, &mut page, &snapshot); text(world, page.status, "Saved magnification loaded. The output follows the validated saved factor."); }
                Outcome::Failed(reason) => { page.pending = None; text(world, page.status, reason); }
            }
        }
        let head = page.selected.and_then(|output| snapshot.head(output));
        text(world, page.selector_text, head.map_or("Unavailable output", |head| head.name.as_str()));
        let summary = head.map_or_else(|| "Display magnification requires an active Park output. Sandbox keeps its own window and cannot magnify the parent desktop.".to_owned(), |head| {
            format!("{} · saved {:.2}× · {}", head.name, head.requested, head.effective.map_or_else(|| "waiting for the output".into(), |factor| format!("current {factor:.2}×")))
        });
        text(world, page.summary, summary);
        let stale = page.origin.as_ref().is_some_and(|(revision, _, context)| *revision != snapshot.revision || control::context(world, page.root).as_ref() != Some(context));
        if stale && page.pending.is_none() { text(world, page.status, "The original settings or view changed. Your draft is preserved; Cancel reloads current values."); }
        let pending = page.pending.is_some();
        let available = snapshot.available && head.is_some() && world.contains_resource::<HostRequests>();
        let editable = available && !pending && eligible(world, entity, page.root);
        disable(world, page.factor, !editable);
        disable(world, page.selector, pending || !available);
        for control in &page.controls { disable(world, *control, !editable); }
        let changed = page.origin.as_ref().is_some_and(|(_, origin, _)| read(world, page.factor).ok().is_some_and(|factor| factor != *origin));
        disable(world, page.save, !editable || stale || !changed);
        disable(world, page.cancel, pending);
        disable(world, page.reload, pending || !world.contains_resource::<HostRequests>());
        world.entity_mut(entity).insert(page);
    }
}

#[cfg(test)]
mod tests;
