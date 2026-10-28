use super::{
    HostRequests,
    input_settings::{button, disable, feedback, field, next_request, text},
};
use crate::{
    host::{
        HostRequest,
        external::{Action as ExternalAction, Command, Request, Settings, Snapshot},
        input::InputOutcome,
        launch::{Capability, Description},
    },
    native::{actions::Action, edit_mode::label, shortcuts::Shortcut},
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::EditableText,
};

#[derive(Component)]
struct Page {
    root: Entity,
    body: Entity,
    toggle: (Entity, Entity),
    summary: Entity,
    selector: Entity,
    selector_text: Entity,
    program: Entity,
    directory: Entity,
    argument_list: Entity,
    arguments: Vec<Entity>,
    argument_rows: Vec<Entity>,
    locked: (Entity, Entity),
    binding: (Entity, Entity),
    status: Entity,
    save: Entity,
    cancel: Entity,
    selected: ExternalAction,
    draft: Settings,
    origin: Option<(u64, Settings)>,
    pending: Option<(u64, Settings)>,
    expanded: bool,
    message: String,
    alert: bool,
}

#[derive(Clone, Copy)]
enum Change {
    Toggle,
    Select(ExternalAction),
    AddArgument,
    RemoveArgument(usize),
    Locked,
    Clear,
    Binding,
    Save,
    Cancel,
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        PostUpdate,
        refresh
            .after(crate::native::actions::ApplyActions)
            .before(bevy::ui::UiSystems::Layout),
    );
}

fn empty(action: ExternalAction) -> Command {
    Command {
        command: Description {
            program: String::new(),
            arguments: Vec::new(),
            directory: None,
            capabilities: if matches!(
                action,
                ExternalAction::ScreenshotScreen | ExternalAction::ScreenshotRegion
            ) {
                vec![Capability::Capture]
            } else {
                Vec::new()
            },
        },
        allow_when_locked: false,
    }
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity) {
    let page = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            Name::new("park.external-commands"),
            ChildOf(parent),
        ))
        .id();
    label(world, page, "External commands", 20.0);
    let summary = label(world, page, "", 14.0);
    let toggle = button(world, page, page, "Edit external commands", Change::Toggle);
    let body = world
        .spawn((
            Node {
                display: Display::None,
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(page),
        ))
        .id();
    label(
        world,
        body,
        "Choose your tools here. Editing or saving never runs them. Leave the executable empty to disable a command.",
        14.0,
    );
    let selected = ExternalAction::BrightnessDown;
    let selector = crate::native::dropdown::spawn(
        world,
        body,
        page,
        "External command",
        selected.label(),
        ExternalAction::ALL
            .into_iter()
            .map(|action| {
                (
                    action.label().to_owned(),
                    crate::native::actions![Change::Select(action)],
                )
            })
            .collect(),
    );
    let selector_text = world
        .get::<Children>(selector)
        .unwrap()
        .iter()
        .find(|entity| world.get::<Text>(*entity).is_some())
        .unwrap();
    let menu = world
        .get::<crate::native::dropdown::Dropdown>(selector)
        .unwrap()
        .menu;
    world.get_mut::<Node>(menu).unwrap().max_height = px(240);
    crate::native::scroll_sand::attach(world, menu);
    let program = field(
        world,
        body,
        "Executable (empty disables this command)",
        4096,
    );
    let directory = field(
        world,
        body,
        "Working directory (absolute; empty uses Park's directory)",
        4096,
    );
    label(
        world,
        body,
        "Literal arguments · each row is one argument; quotes and shell symbols stay literal.",
        14.0,
    );
    let argument_list = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(body),
        ))
        .id();
    button(world, body, page, "Add argument", Change::AddArgument);
    let locked = button(world, body, page, "Allow while locked: off", Change::Locked);
    label(
        world,
        body,
        "Screen capture commands receive capture access. Only hardware and media controls can be enabled while locked.",
        14.0,
    );
    let binding = button(world, body, page, "Edit command binding", Change::Binding);
    crate::native::shortcuts::capture_entry(world, binding.0);
    button(
        world,
        body,
        page,
        "Clear selected command in draft",
        Change::Clear,
    );
    let status = feedback(world, body);
    let save = button(world, body, page, "Save external commands", Change::Save).0;
    let cancel = button(world, body, page, "Cancel external changes", Change::Cancel).0;
    world.entity_mut(page).insert(Page {
        root,
        body,
        toggle,
        summary,
        selector,
        selector_text,
        program,
        directory,
        argument_list,
        arguments: Vec::new(),
        argument_rows: Vec::new(),
        locked,
        binding,
        status,
        save,
        cancel,
        selected,
        draft: Settings::default(),
        origin: None,
        pending: None,
        expanded: false,
        message: String::new(),
        alert: false,
    });
}

fn value(world: &World, entity: Entity) -> Result<String, String> {
    let field = world
        .get::<EditableText>(entity)
        .ok_or("The command field is unavailable.")?;
    if field.is_composing() || !field.pending_edits.is_empty() || field.pending_paste.is_some() {
        return Err("Finish typing before changing or saving commands.".into());
    }
    Ok(field.value().to_string())
}

fn read(world: &World, page: &mut Page) -> Result<Settings, String> {
    let mut command = page
        .draft
        .0
        .get(&page.selected)
        .cloned()
        .unwrap_or_else(|| empty(page.selected));
    command.command.program = value(world, page.program)?;
    let directory = value(world, page.directory)?;
    command.command.directory = (!directory.is_empty()).then(|| directory.into());
    command.command.arguments = page
        .arguments
        .iter()
        .map(|entity| value(world, *entity))
        .collect::<Result<_, _>>()?;
    page.draft.0.insert(page.selected, command);
    Ok(Settings(
        page.draft
            .0
            .iter()
            .filter(|(_, command)| !command.command.program.is_empty())
            .map(|(action, command)| (*action, command.clone()))
            .collect(),
    ))
}

fn set_value(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut text) = world.get_mut::<EditableText>(entity) {
        text.pending_edits.clear();
        text.pending_paste = None;
        if text.value() != value {
            text.editor.set_text(value);
            text.queue_edit(bevy::text::TextEdit::TextEnd(false));
        }
    }
}

fn arguments(world: &mut World, page_entity: Entity, page: &mut Page, values: &[String]) {
    for row in page.argument_rows.drain(..) {
        world.despawn(row);
    }
    page.arguments.clear();
    for (index, value) in values.iter().enumerate() {
        let row = world
            .spawn((
                Node {
                    width: percent(100),
                    min_width: px(0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(page.argument_list),
            ))
            .id();
        let input = field(world, row, &format!("Argument {}", index + 1), 4096);
        set_value(world, input, value);
        button(
            world,
            row,
            page_entity,
            &format!("Remove argument {}", index + 1),
            Change::RemoveArgument(index),
        );
        page.argument_rows.push(row);
        page.arguments.push(input);
    }
}

fn selected(world: &mut World, entity: Entity, page: &mut Page) {
    let command = page
        .draft
        .0
        .get(&page.selected)
        .cloned()
        .unwrap_or_else(|| empty(page.selected));
    set_value(world, page.program, &command.command.program);
    set_value(
        world,
        page.directory,
        &command
            .command
            .directory
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    arguments(world, entity, page, &command.command.arguments);
    text(world, page.selector_text, page.selected.label());
    let menu = world
        .get::<crate::native::dropdown::Dropdown>(page.selector)
        .unwrap()
        .menu;
    world.get_mut::<Node>(menu).unwrap().display = Display::None;
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(page.selector) {
        accessibility.set_expanded(false);
    }
}

fn load(world: &mut World, entity: Entity, page: &mut Page, snapshot: &Snapshot) {
    page.draft = snapshot.settings.clone();
    page.origin = Some((snapshot.revision, snapshot.settings.clone()));
    selected(world, entity, page);
}

fn caption(world: &mut World, control: (Entity, Entity), value: impl Into<String>) {
    let value = value.into();
    text(world, control.1, &value);
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(control.0) {
        accessibility.set_label(value);
    }
}

impl Action for Change {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(mut page) = world.entity_mut(entity).take::<Page>() else {
            return;
        };
        let snapshot = world
            .get_resource::<Snapshot>()
            .cloned()
            .unwrap_or_default();
        let mut binding = None;
        let result = (|| {
            match *self {
                Self::Toggle => {
                    page.expanded = !page.expanded;
                    world.get_mut::<Node>(page.body).unwrap().display = if page.expanded {
                        Display::Flex
                    } else {
                        Display::None
                    };
                    caption(
                        world,
                        page.toggle,
                        if page.expanded {
                            "Hide external commands"
                        } else {
                            "Edit external commands"
                        },
                    );
                    if page.origin.is_none() && snapshot.available {
                        load(world, entity, &mut page, &snapshot);
                    }
                    world.resource_mut::<InputFocus>().set(
                        if page.expanded && snapshot.available {
                            page.program
                        } else {
                            page.toggle.0
                        },
                        FocusCause::Navigated,
                    );
                }
                Self::Binding => binding = Some((page.root, page.selected)),
                Self::Cancel => {
                    if page.pending.is_some() {
                        return Ok(());
                    }
                    load(world, entity, &mut page, &snapshot);
                    world
                        .get_resource::<HostRequests>()
                        .ok_or("Park is unavailable.")?
                        .0
                        .try_send(HostRequest::ExternalReload)
                        .map_err(|_| "Park is busy; the loaded settings are shown.")?;
                    page.message = "Draft discarded; reloading the saved commands.".into();
                    page.alert = false;
                }
                Self::Save => {
                    if page.pending.is_some() {
                        return Ok(());
                    }
                    let settings = read(world, &mut page)?;
                    settings.validate()?;
                    if !snapshot.available {
                        return Err("External commands require a Park session.".into());
                    }
                    let revision = page.origin.as_ref().ok_or("Commands are still loading.")?.0;
                    if revision != snapshot.revision {
                        return Err(
                            "Commands changed elsewhere; cancel to reload before saving.".into(),
                        );
                    }
                    let id = next_request(world, snapshot.last_request)?;
                    world
                        .get_resource::<HostRequests>()
                        .ok_or("Park is unavailable.")?
                        .0
                        .try_send(HostRequest::External(Request {
                            id,
                            revision,
                            settings: settings.clone(),
                        }))
                        .map_err(|_| "Park is busy; your draft is preserved.")?;
                    page.pending = Some((id, settings));
                    page.message = "Saving external commands. No program will be run.".into();
                    page.alert = false;
                }
                action => {
                    read(world, &mut page)?;
                    match action {
                        Self::Select(action) => {
                            page.message.clear();
                            page.alert = false;
                            page.selected = action;
                            selected(world, entity, &mut page);
                            world
                                .resource_mut::<InputFocus>()
                                .set(page.program, FocusCause::Navigated);
                        }
                        Self::AddArgument => {
                            let command = page
                                .draft
                                .0
                                .entry(page.selected)
                                .or_insert_with(|| empty(page.selected));
                            if command.command.arguments.len() >= 64 {
                                return Err("Use at most 64 arguments.".into());
                            }
                            command.command.arguments.push(String::new());
                            let values = command.command.arguments.clone();
                            arguments(world, entity, &mut page, &values);
                            if let Some(input) = page.arguments.last() {
                                world
                                    .resource_mut::<InputFocus>()
                                    .set(*input, FocusCause::Navigated);
                            }
                        }
                        Self::RemoveArgument(index) => {
                            let command = page
                                .draft
                                .0
                                .entry(page.selected)
                                .or_insert_with(|| empty(page.selected));
                            if index < command.command.arguments.len() {
                                command.command.arguments.remove(index);
                            }
                            let values = command.command.arguments.clone();
                            arguments(world, entity, &mut page, &values);
                            world
                                .resource_mut::<InputFocus>()
                                .set(page.program, FocusCause::Navigated);
                        }
                        Self::Locked => {
                            if page.selected.lockable() {
                                let command = page
                                    .draft
                                    .0
                                    .entry(page.selected)
                                    .or_insert_with(|| empty(page.selected));
                                command.allow_when_locked = !command.allow_when_locked;
                            }
                        }
                        Self::Clear => {
                            page.draft.0.remove(&page.selected);
                            selected(world, entity, &mut page);
                            page.message = "Command cleared in the draft. Save to apply.".into();
                            page.alert = false;
                        }
                        _ => {}
                    }
                }
            }
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            page.message = error;
            page.alert = true;
        }
        world.entity_mut(entity).insert(page);
        if let Some((root, action)) = binding {
            crate::native::shortcuts::edit_binding(world, root, Shortcut::External(action));
        }
        if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
            wake.ring();
        }
    }
}

fn refresh(world: &mut World) {
    let snapshot = world
        .get_resource::<Snapshot>()
        .cloned()
        .unwrap_or_default();
    let pages: Vec<_> = world
        .query_filtered::<Entity, With<Page>>()
        .iter(world)
        .collect();
    for entity in pages {
        let mut page = world.entity_mut(entity).take::<Page>().unwrap();
        if page.origin.is_none() && snapshot.available {
            load(world, entity, &mut page, &snapshot);
        }
        text(
            world,
            page.summary,
            if snapshot.available {
                format!(
                    "{} of {} commands configured. Programs are chosen locally.",
                    snapshot.settings.0.len(),
                    ExternalAction::ALL.len()
                )
            } else {
                "External commands are configured by a Park session.".into()
            },
        );
        if !page.expanded && page.pending.is_none() {
            world.entity_mut(entity).insert(page);
            continue;
        }
        let candidate = read(world, &mut page).and_then(|settings| {
            settings.validate()?;
            Ok(settings)
        });
        if let Some((id, saved)) = page.pending.clone() {
            if let Some(response) = snapshot
                .response
                .as_ref()
                .filter(|response| response.request == id)
            {
                page.pending = None;
                match &response.outcome {
                    InputOutcome::Saved { warning } => {
                        page.alert = warning.is_some();
                        page.origin = Some((snapshot.revision, snapshot.settings.clone()));
                        page.message = warning.clone().unwrap_or_else(|| {
                            if candidate.as_ref() == Ok(&saved) {
                                "External commands saved.".into()
                            } else {
                                "Saved; newer edits remain unsaved.".into()
                            }
                        });
                    }
                    InputOutcome::Failed(error) => {
                        page.message = error.clone();
                        page.alert = true;
                    }
                    InputOutcome::Pending => page.pending = Some((id, saved)),
                }
            } else if snapshot.last_request > id {
                page.pending = None;
                page.message =
                    "Another editor changed commands; your draft is preserved. Cancel to reload."
                        .into();
                page.alert = true;
            }
        } else if let Some((revision, origin)) = &page.origin
            && *revision != snapshot.revision
        {
            if candidate.as_ref() == Ok(origin) {
                load(world, entity, &mut page, &snapshot);
            } else {
                page.message =
                    "Commands changed elsewhere; your draft is preserved. Cancel to reload.".into();
                page.alert = true;
            }
        }
        let allow_locked = page
            .draft
            .0
            .get(&page.selected)
            .is_some_and(|command| command.allow_when_locked);
        caption(
            world,
            page.locked,
            if !page.selected.lockable() {
                "Unavailable while locked"
            } else if allow_locked {
                "Allow while locked: on"
            } else {
                "Allow while locked: off"
            },
        );
        disable(
            world,
            page.locked.0,
            !snapshot.available || !page.selected.lockable(),
        );
        let settings = world
            .get_resource::<crate::native::shortcuts::Settings>()
            .cloned()
            .unwrap_or_default();
        let binding = Shortcut::External(page.selected).caption(
            &settings,
            world.get_resource::<crate::host::input::KeyboardSnapshot>(),
        );
        caption(world, page.binding, format!("Edit binding · {binding}"));
        let stale = page
            .origin
            .as_ref()
            .is_none_or(|(revision, _)| *revision != snapshot.revision);
        disable(
            world,
            page.save,
            !snapshot.available || page.pending.is_some() || candidate.is_err() || stale,
        );
        disable(world, page.cancel, page.pending.is_some());
        for input in [page.program, page.directory]
            .into_iter()
            .chain(page.arguments.iter().copied())
        {
            disable(world, input, !snapshot.available);
        }
        let dirty = candidate.as_ref().is_ok_and(|candidate| {
            page.origin
                .as_ref()
                .is_some_and(|(_, origin)| candidate != origin)
        });
        let message = snapshot
            .message
            .as_ref()
            .or_else(|| candidate.as_ref().err())
            .cloned()
            .unwrap_or_else(|| {
                if page.alert || page.pending.is_some() {
                    page.message.clone()
                } else if dirty {
                    "Unsaved command changes. Save to apply; no program has been run.".into()
                } else if page
                    .draft
                    .0
                    .get(&page.selected)
                    .is_none_or(|command| command.command.program.is_empty())
                {
                    format!("{} is not configured.", page.selected.label())
                } else if page.message.is_empty() {
                    "Saved locally. Editing never runs the command.".into()
                } else {
                    page.message.clone()
                }
            });
        text(world, page.status, message);
        world.entity_mut(entity).insert(page);
    }
}

#[cfg(test)]
mod tests;
