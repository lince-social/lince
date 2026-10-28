use super::input_settings::{button, disable, feedback, field, text};
use crate::{
    host::input::GestureSettings,
    native::{
        actions::Action,
        edit_mode::label,
        shortcuts::{
            Settings,
            configuration::{self, LayoutSettings},
        },
    },
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::EditableText,
};

mod viewport;
pub use viewport::{active, cancel, deliver};

#[derive(Resource, Default)]
pub struct GestureSupport(pub bool);

#[derive(Component)]
struct Page {
    body: Entity,
    toggle: (Entity, Entity),
    expanded: bool,
    summary: Entity,
    availability: Entity,
    enabled: (Entity, Entity),
    sensitivity: Entity,
    threshold: Entity,
    status: Entity,
    save: Entity,
    origin: GestureSettings,
    draft_enabled: bool,
    validation_error: Option<String>,
}

#[derive(Clone, Copy)]
enum Change {
    Toggle,
    Enabled,
    Save,
    Cancel,
    Reload,
    Rebase,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<GestureSupport>().add_systems(
        PostUpdate,
        refresh
            .after(crate::native::actions::ApplyActions)
            .before(bevy::ui::UiSystems::Layout),
    );
}

pub fn settings(world: &World) -> GestureSettings {
    world
        .get_resource::<LayoutSettings>()
        .map_or_else(GestureSettings::default, |settings| settings.gestures)
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    let origin = settings(world);
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
            Name::new("park.gestures"),
            ChildOf(parent),
        ))
        .id();
    label(world, page, "Desktop gestures", 20.0);
    let summary = label(world, page, "", 14.0);
    let availability = label(world, page, "", 14.0);
    for entity in [summary, availability] {
        world.get_mut::<Node>(entity).unwrap().min_width = px(0);
        world.entity_mut(entity).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
    }
    let toggle = button(world, page, page, "Edit desktop gestures", Change::Toggle);
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
        "Three fingers: horizontal pans the current row or canvas; vertical changes workspace. Four fingers: down opens overview, up closes it. Workspace and overview changes commit when the gesture ends.",
        14.0,
    );
    let enabled = button(world, body, page, "", Change::Enabled);
    let sensitivity = field(
        world,
        body,
        "Gesture sensitivity (10–800%; 100% is normal)",
        3,
    );
    let threshold = field(
        world,
        body,
        "Workspace/overview threshold (16–400 logical units)",
        3,
    );
    let status = feedback(world, body);
    let save = button(world, body, page, "Save desktop gestures", Change::Save).0;
    button(world, body, page, "Cancel gesture changes", Change::Cancel);
    button(world, body, page, "Reload active gesture values", Change::Reload);
    button(world, body, page, "Review gesture draft against file", Change::Rebase);
    world.entity_mut(page).insert(Page {
        body,
        toggle,
        expanded: false,
        summary,
        availability,
        enabled,
        sensitivity,
        threshold,
        status,
        save,
        origin,
        draft_enabled: origin.enabled,
        validation_error: None,
    });
    load(world, page, origin);
    refresh(world);
}

fn caption(world: &mut World, control: (Entity, Entity), value: &str) {
    text(world, control.1, value);
    if world
        .get::<bevy::a11y::AccessibilityNode>(control.0)
        .is_some_and(|node| node.label() != Some(value))
    {
        world
            .get_mut::<bevy::a11y::AccessibilityNode>(control.0)
            .unwrap()
            .set_label(value);
    }
}

fn load(world: &mut World, page: Entity, value: GestureSettings) {
    let state = world.get::<Page>(page).unwrap();
    let fields = [
        (state.sensitivity, value.sensitivity_percent),
        (state.threshold, value.commit_threshold),
    ];
    for (entity, value) in fields {
        let value = value.to_string();
        let mut input = world.get_mut::<EditableText>(entity).unwrap();
        if input.value() != value.as_str() {
            input.editor.set_text(&value);
        }
    }
    let mut state = world.get_mut::<Page>(page).unwrap();
    state.origin = value;
    state.draft_enabled = value.enabled;
}

fn candidate(world: &World, page: Entity) -> Result<GestureSettings, String> {
    let state = world
        .get::<Page>(page)
        .ok_or("The gesture settings are unavailable.")?;
    let value = |entity| -> Result<u16, String> {
        let input = world
            .get::<EditableText>(entity)
            .ok_or("A gesture field is unavailable.")?;
        if input.is_composing() || !input.pending_edits.is_empty() || input.pending_paste.is_some()
        {
            return Err("Finish typing before saving gesture settings.".into());
        }
        input
            .value()
            .to_string()
            .trim()
            .parse()
            .map_err(|_| "Use whole numbers for gesture sensitivity and threshold.".into())
    };
    let result = GestureSettings {
        enabled: state.draft_enabled,
        sensitivity_percent: value(state.sensitivity)?,
        commit_threshold: value(state.threshold)?,
    };
    result.validate()?;
    Ok(result)
}

impl Action for Change {
    fn apply(&self, world: &mut World, page: Entity) {
        let Some(state) = world.get::<Page>(page) else {
            return;
        };
        if crate::native::shortcuts::keyboard_owned(world) {
            return;
        }
        let (body, toggle, expanded, status) =
            (state.body, state.toggle, state.expanded, state.status);
        match self {
            Self::Toggle => {
                world.get_mut::<Page>(page).unwrap().expanded = !expanded;
                world.get_mut::<Node>(body).unwrap().display = if expanded {
                    Display::None
                } else {
                    Display::Flex
                };
                caption(
                    world,
                    toggle,
                    if expanded {
                        "Edit desktop gestures"
                    } else {
                        "Hide gesture settings"
                    },
                );
                if expanded {
                    world
                        .resource_mut::<InputFocus>()
                        .set(toggle.0, FocusCause::Navigated);
                }
            }
            Self::Enabled => {
                let mut state = world.get_mut::<Page>(page).unwrap();
                state.draft_enabled = !state.draft_enabled;
            }
            Self::Cancel | Self::Reload => {
                let state = world.get::<Page>(page).unwrap();
                if let Err(error) = configuration::review::settled(world, [state.sensitivity, state.threshold]) { text(world, status, error); return; }
                let current = settings(world);
                load(world, page, current);
                text(world, status, "Gesture draft discarded.");
            }
            Self::Rebase => {
                if let Err(error) = configuration::review::open(world, page, "gesture settings", review_snapshot, review_apply) { text(world, status, error); }
            }
            Self::Save => {
                let result = (|| {
                    let candidate = candidate(world, page)?;
                    let current = settings(world);
                    if current != world.get::<Page>(page).unwrap().origin {
                        return Err("Gesture settings changed while you were editing. Reload active values or Review the draft against the file; your draft is preserved.".into());
                    }
                    let mut layout = world
                        .get_resource::<LayoutSettings>()
                        .cloned()
                        .unwrap_or_default();
                    layout.gestures = candidate;
                    let shortcuts = world
                        .get_resource::<Settings>()
                        .cloned()
                        .unwrap_or_default();
                    let message = configuration::commit(world, shortcuts, layout)?;
                    world.get_mut::<Page>(page).unwrap().origin = candidate;
                    Ok::<String, String>(message)
                })();
                text(
                    world,
                    status,
                    match result {
                        Ok(message) => message,
                        Err(error) => error,
                    },
                );
            }
        }
        refresh(world);
    }
}

fn refresh(world: &mut World) {
    let active = settings(world);
    let supported = world
        .get_resource::<GestureSupport>()
        .is_some_and(|support| support.0);
    let pages: Vec<_> = world
        .query_filtered::<Entity, With<Page>>()
        .iter(world)
        .collect();
    for page in pages {
        let state = world.get::<Page>(page).unwrap();
        let (summary, availability, enabled, save, origin, status, previous_error) = (
            state.summary,
            state.availability,
            state.enabled,
            state.save,
            state.origin,
            state.status,
            state.validation_error.clone(),
        );
        let mut draft = candidate(world, page);
        if draft.as_ref().is_ok_and(|draft| *draft == origin) && origin != active {
            load(world, page, active);
            draft = candidate(world, page);
        }
        text(
            world,
            summary,
            format!(
                "{} · Sensitivity {}% · Commit threshold {}",
                if active.enabled {
                    "Enabled"
                } else {
                    "Disabled"
                },
                active.sensitivity_percent,
                active.commit_threshold
            ),
        );
        text(
            world,
            availability,
            if supported {
                "A desktop touchpad gesture stream is available."
            } else {
                "This host has no desktop touchpad gesture stream. Saved preferences apply when available; keyboard commands remain usable."
            },
        );
        let draft_enabled = world.get::<Page>(page).unwrap().draft_enabled;
        caption(
            world,
            enabled,
            if draft_enabled {
                "Desktop gestures: On"
            } else {
                "Desktop gestures: Off"
            },
        );
        let origin = world.get::<Page>(page).unwrap().origin;
        let error = draft.as_ref().err().cloned().or_else(|| (origin != active).then(|| "Gesture settings changed. Reload active values or Review the draft against the file; your draft is preserved.".into()));
        if let Some(error) = &error {
            text(world, status, error.clone());
        } else if previous_error.as_ref().is_some_and(|previous| {
            world
                .get::<Text>(status)
                .is_some_and(|text| &text.0 == previous)
        }) {
            text(world, status, "");
        }
        if error != previous_error {
            world.get_mut::<Page>(page).unwrap().validation_error = error.clone();
        }
        disable(
            world,
            save,
            error.is_some() || draft.as_ref().map_or(true, |draft| *draft == active),
        );
    }
}

fn review_snapshot(world: &World, page: Entity) -> Result<configuration::review::Snapshot, String> {
    let state = world.get::<Page>(page).ok_or("The gesture editor is unavailable.")?;
    configuration::review::Snapshot::new(&state.origin, &candidate(world, page)?, &settings(world))
}

fn review_apply(world: &mut World, page: Entity, active: serde_json::Value, draft: serde_json::Value) -> Result<(), String> {
    let origin: GestureSettings = serde_json::from_value(active).map_err(|error| error.to_string())?;
    let draft: GestureSettings = serde_json::from_value(draft).map_err(|error| error.to_string())?;
    draft.validate()?;
    if world.get::<Page>(page).is_none() { return Err("The gesture editor is unavailable.".into()); }
    load(world, page, draft);
    let mut state = world.get_mut::<Page>(page).unwrap();
    state.origin = origin;
    state.validation_error = None;
    let status = state.status;
    text(world, status, "Review applied to the draft. Save desktop gestures to activate it.");
    Ok(())
}

#[cfg(test)]
mod tests;
