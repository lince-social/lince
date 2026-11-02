use super::*;
use crate::{host::{HostRequest, input::InputOutcome, launch::{Description, Program, Programs, ProgramsRequest, ProgramsSnapshot}},
    native::{layout::desktop::HostRequests, sand_panel}};

#[derive(Resource, Default)]
struct Requests(u64);

#[derive(Component, Clone)]
struct Page {
    program: Program,
    executable: Entity,
    directory: Entity,
    arguments: Entity,
    fields: Vec<Entity>,
    status: Entity,
    origin: Programs,
    pending: Option<(u64, bool)>,
}

#[derive(Clone, Copy)]
enum Change { Add, Remove(usize), Save, Reload }

fn field(world: &mut World, page: Entity, value: &str) -> Entity {
    let index = world.get::<Page>(page).unwrap().fields.len();
    let parent = world.get::<Page>(page).unwrap().arguments;
    let row = world.spawn((Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }, ChildOf(parent))).id();
    let caption = format!("Literal argument {}", index + 1);
    let input = sand_panel::field(world, row, &caption, value);
    if let Some(mut text) = world.get_mut::<EditableText>(input) {
        text.allow_newlines = true;
        text.visible_lines = Some(2.0);
    }
    crate::native::accessibility::input(world, input, &caption, true);
    sand_panel::button(world, row, page, "Remove argument", Change::Remove(index));
    world.get_mut::<Page>(page).unwrap().fields.push(input);
    input
}

fn populate(world: &mut World, page: Entity, program: Program, snapshot: &ProgramsSnapshot) {
    let children = world.get::<Children>(page).map(|children| children.iter().collect::<Vec<_>>()).unwrap_or_default();
    for child in children { world.despawn(child); }
    let body = world.spawn((Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..default() }, ChildOf(page))).id();
    let description = snapshot.settings.get(program);
    let executable = sand_panel::field(world, body, "Program name (empty uses the automatic default)", description.map_or("", |value| value.program.as_str()));
    let directory = sand_panel::field(world, body, "Working directory (optional)", description.and_then(|value| value.directory.as_ref()).and_then(|path| path.to_str()).unwrap_or(""));
    label(world, body, "Arguments are passed literally, without a shell. Add one field per argument.", 13.0);
    let arguments = world.spawn((Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }, ChildOf(body))).id();
    let status = label(world, body, if snapshot.available { "" } else { "Session programs are configured by Park. Sandbox uses its host desktop." }, 13.0);
    world.entity_mut(page).insert(Page { program, executable, directory, arguments, fields: Vec::new(), status,
        origin: snapshot.settings.clone(), pending: None });
    if let Some(value) = description {
        for argument in &value.arguments { field(world, page, argument); }
    }
    sand_panel::button(world, body, page, "Add literal argument", Change::Add);
    sand_panel::button(world, body, page, "Save session program", Change::Save);
    sand_panel::button(world, body, page, "Reload session programs", Change::Reload);
    label(world, body, if program == Program::Startup { "Startup changes take effect at the next Park login." }
        else { "Saved changes apply the next time this program is opened. Program names are resolved through PATH; a full binary path is optional." }, 13.0);
}

pub(super) fn panel(world: &mut World, root: Entity, parent: Entity) {
    let snapshot = world.get_resource::<ProgramsSnapshot>().cloned().unwrap_or_default();
    for program in Program::ALL {
        let body = super::sections::accordion(world, root, parent, program.key(), program.label());
        let page = world.spawn((Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }, ChildOf(body))).id();
        populate(world, page, program, &snapshot);
    }
}

fn draft(world: &World, page: &Page) -> Result<Programs, String> {
    let read = |entity| world.get::<EditableText>(entity).filter(|text| !text.is_composing() && text.pending_paste.is_none())
        .map(|text| text.value().to_string()).ok_or_else(|| "Finish typing or pasting before saving.".to_owned());
    let executable = read(page.executable)?;
    let directory = read(page.directory)?;
    let mut settings = page.origin.clone();
    let description = if executable.is_empty() { None } else {
        let arguments = page.fields.iter().map(|field| read(*field)).collect::<Result<Vec<_>, _>>()?;
        Some(Description { program: executable, arguments, directory: (!directory.is_empty()).then(|| directory.into()),
            capabilities: page.origin.get(page.program).map(|value| value.capabilities.clone()).unwrap_or_default() })
    };
    settings.set(page.program, description);
    settings.validate()?;
    Ok(settings)
}

impl Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        let Some(page) = world.get::<Page>(target).cloned() else { return; };
        if page.pending.is_some() { return; }
        let status = page.status;
        let result: Result<(), String> = (|| {
            match *self {
                Self::Add => {
                    if page.fields.len() >= 64 { return Err("Use at most 64 arguments.".into()); }
                    field(world, target, "");
                }
                Self::Remove(index) => {
                    let values = page.fields.iter().enumerate().filter(|(i, _)| *i != index)
                        .map(|(_, entity)| world.get::<EditableText>(*entity).unwrap().value().to_string()).collect::<Vec<_>>();
                    let parent = page.arguments;
                    let children = world.get::<Children>(parent).map(|children| children.iter().collect::<Vec<_>>()).unwrap_or_default();
                    for child in children { world.despawn(child); }
                    world.get_mut::<Page>(target).unwrap().fields.clear();
                    for value in values { field(world, target, &value); }
                }
                Self::Save | Self::Reload => {
                    let snapshot = world.get_resource::<ProgramsSnapshot>().cloned().unwrap_or_default();
                    if !snapshot.available { return Err("Session programs are configured by Park.".into()); }
                    let settings = if matches!(self, Self::Save) {
                        let draft = draft(world, &page)?;
                        if snapshot.settings.get(page.program) != page.origin.get(page.program) {
                            return Err("This session program changed; reload it before saving this draft.".into());
                        }
                        let mut settings = snapshot.settings.clone();
                        settings.set(page.program, draft.get(page.program).cloned());
                        Some(settings)
                    } else { None };
                    let revision = snapshot.revision;
                    world.init_resource::<Requests>();
                    let id = world.resource::<Requests>().0.max(snapshot.last_request).checked_add(1).ok_or("Session program request counter exhausted.")?;
                    world.resource_mut::<Requests>().0 = id;
                    let request = settings.map_or(HostRequest::ProgramsReload(id), |settings| HostRequest::Programs(ProgramsRequest { id, revision, settings }));
                    world.get_resource::<HostRequests>().ok_or("The desktop host is unavailable.")?.0.try_send(request)
                        .map_err(|_| "The desktop host is busy; retry.".to_owned())?;
                    world.get_mut::<Page>(target).unwrap().pending = Some((id, matches!(self, Self::Reload)));
                }
            }
            Ok(())
        })();
        if let Some(mut text) = world.get_mut::<Text>(status) { text.0 = result.err().unwrap_or_default(); }
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() { wake.ring(); }
    }
}

pub(crate) fn refresh(world: &mut World) {
    if world.query::<&Page>().iter(world).next().is_none() { return; }
    let snapshot = world.get_resource::<ProgramsSnapshot>().cloned().unwrap_or_default();
    let completed = world.query::<(Entity, &Page)>().iter(world).filter_map(|(entity, page)| {
        let (request, reload) = page.pending?;
        let response = snapshot.response.as_ref().filter(|response| response.request == request)?;
        Some((entity, page.program, page.status, reload, response.outcome.clone()))
    }).collect::<Vec<_>>();
    for (entity, program, status, reload, outcome) in completed {
        if outcome == InputOutcome::Pending { continue; }
        if reload && matches!(outcome, InputOutcome::Saved { .. }) {
            populate(world, entity, program, &snapshot);
            continue;
        }
        let mut page = world.get_mut::<Page>(entity).unwrap();
        page.pending = None;
        if matches!(outcome, InputOutcome::Saved { .. }) { page.origin = snapshot.settings.clone(); }
        let message = match outcome { InputOutcome::Saved { warning } => warning.unwrap_or_else(|| "Saved.".into()),
            InputOutcome::Failed(error) => error, InputOutcome::Pending => unreachable!() };
        if let Some(mut text) = world.get_mut::<Text>(status) { text.0 = message; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{input::{InputResponse, InputOutcome}, launch::Capability};

    #[test]
    fn graphical_session_program_changes_preserve_literal_arguments_permissions_and_other_programs() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        let initial = Description { program: "existing-terminal".into(), arguments: vec!["$(literal)".into(), String::new(), "two\nlines".into()], directory: None,
            capabilities: vec![Capability::Clipboard] };
        let programs = Programs { terminal: Some(initial.clone()), launcher: Some(Description { program: "existing-launcher".into(), arguments: vec![], directory: None, capabilities: vec![] }), ..default() };
        world.insert_resource(ProgramsSnapshot { available: true, revision: 7, settings: programs.clone(), ..default() });
        let (sender, requests) = std::sync::mpsc::sync_channel(4);
        world.insert_resource(HostRequests(sender));
        let root = world.spawn_empty().id();
        let parent = world.spawn_empty().id();
        panel(&mut world, root, parent);
        let page = world.query::<(Entity, &Page)>().iter(&world).find(|(_, page)| page.program == Program::Terminal).unwrap().0;
        let executable = world.get::<Page>(page).unwrap().executable;
        world.get_mut::<EditableText>(executable).unwrap().editor.set_text("new-terminal");
        Change::Save.apply(&mut world, page);
        let HostRequest::Programs(request) = requests.try_recv().unwrap() else { panic!("Expected a settings request"); };
        assert_eq!(request.revision, 7);
        let mut expected = programs.clone();
        expected.terminal.as_mut().unwrap().program = "new-terminal".into();
        assert_eq!(request.settings, expected);
        assert_eq!(request.settings.terminal.as_ref().unwrap().arguments, initial.arguments);
        assert_eq!(request.settings.terminal.as_ref().unwrap().capabilities, initial.capabilities);
        world.insert_resource(ProgramsSnapshot { available: true, revision: 8, settings: expected, last_request: request.id,
            response: Some(InputResponse { request: request.id, outcome: InputOutcome::Saved { warning: None } }) });
        refresh(&mut world);
        assert!(world.get::<Page>(page).unwrap().pending.is_none());
        let status = world.get::<Page>(page).unwrap().status;
        assert_eq!(world.get::<Text>(status).unwrap().0, "Saved.");
        world.get_mut::<EditableText>(executable).unwrap().editor.set_text("");
        Change::Save.apply(&mut world, page);
        let HostRequest::Programs(request) = requests.try_recv().unwrap() else { panic!("Expected automatic-default settings"); };
        assert!(request.settings.terminal.is_none());
        assert_eq!(request.settings.launcher, programs.launcher);
    }
}
