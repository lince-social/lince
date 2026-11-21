// non-lean-yet
// Scope: struct Page, enum Change, fn install, fn panel.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   270.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (59, 60, 65, 82, 86, 96, 102, 105, 115, 211, 213, 218, 272).

use super::{CursorWarpSupport, policy};
use crate::{
    host::input::FocusPolicy,
    native::{
        actions::Action,
        edit_mode::label,
        layout::desktop::input_settings::{button, caption, disable, display, feedback, text},
        shortcuts::{Settings, configuration::{self, LayoutSettings}},
    },
};
use bevy::{prelude::*, ecs::query::QueryState};
use std::collections::HashMap;

#[derive(Component)]
struct Page {
    body: Entity,
    toggle: (Entity, Entity),
    choices: [(Entity, Entity); 3],
    summary: Entity,
    support: Entity,
    status: Entity,
    save: Entity,
    origin: FocusPolicy,
    draft: FocusPolicy,
    expanded: bool,
    message: Option<String>,
}

#[derive(Clone, Copy)]
enum Change {
    Toggle,
    Choice(usize),
    Save,
    Cancel,
    Reload,
    Rebase,
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(PostUpdate, refresh.after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout));
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    let page = world.spawn((
        Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..default() },
        Name::new("park.focus-settings"),
        ChildOf(parent),
    )).id();
    label(world, page, "Window focus", 20.0);
    let summary = label(world, page, "", 14.0);
    let toggle = button(world, page, page, "Edit focus settings", Change::Toggle);
    let body = world.spawn((Node {
        display: Display::None, width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column,
        row_gap: px(6), flex_shrink: 0.0, ..default()
    }, ChildOf(page))).id();
    let help = label(world, body, "Click focus, focus following pointer movement and cursor warping are independent. Explicit Unfocus pauses pointer focus until you focus an item again. Cursor warping applies only to accepted keyboard or management focus changes.", 14.0);
    let support = label(world, body, "", 14.0);
    for entity in [help, support, summary] {
        world.get_mut::<Node>(entity).unwrap().width = percent(100);
        world.get_mut::<Node>(entity).unwrap().min_width = px(0);
        world.entity_mut(entity).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    }
    let choices = ["Click to focus", "Focus follows pointer", "Warp cursor after keyboard focus"].into_iter().enumerate()
        .map(|(index, caption)| button(world, body, page, caption, Change::Choice(index)))
        .collect::<Vec<_>>().try_into().unwrap();
    let status = feedback(world, body);
    let save = button(world, body, page, "Save focus settings", Change::Save).0;
    button(world, body, page, "Cancel focus changes", Change::Cancel);
    button(world, body, page, "Reload active focus values", Change::Reload);
    button(world, body, page, "Review focus draft against file", Change::Rebase);
    let origin = policy(world);
    world.entity_mut(page).insert(Page { body, toggle, choices, summary, support, status, save, origin, draft: origin, expanded: false, message: None });
    refresh(world);
}

impl Action for Change {
    fn apply(&self, world: &mut World, page: Entity) {
        if crate::native::shortcuts::keyboard_owned(world) || world.get::<Page>(page).is_none() { return; }
        let active = policy(world);
        match *self {
            Self::Toggle => {
                let mut state = world.get_mut::<Page>(page).unwrap();
                state.expanded = !state.expanded;
            }
            Self::Choice(index) => {
                let mut state = world.get_mut::<Page>(page).unwrap();
                match index {
                    0 => state.draft.click = !state.draft.click,
                    1 => state.draft.pointer_follow = !state.draft.pointer_follow,
                    2 => state.draft.warp = !state.draft.warp,
                    _ => return,
                }
                state.message = None;
            }
            Self::Cancel | Self::Reload => {
                let mut state = world.get_mut::<Page>(page).unwrap();
                state.origin = active;
                state.draft = active;
                state.message = None;
            }
            Self::Rebase => {
                if let Err(error) = configuration::review::open(world, page, "focus settings", review_snapshot, review_apply) { world.get_mut::<Page>(page).unwrap().message = Some(error); }
            }
            Self::Save => {
                let state = world.get::<Page>(page).unwrap();
                let draft = state.draft;
                let result = if state.origin != active {
                    Err("Focus settings changed while this draft was open. Cancel to load the saved values; your draft is kept.".to_owned())
                } else {
                    let mut layout = world.get_resource::<LayoutSettings>().cloned().unwrap_or_default();
                    layout.focus = draft;
                    let settings = world.get_resource::<Settings>().cloned().unwrap_or_default();
                    configuration::commit(world, settings, layout)
                };
                let mut state = world.get_mut::<Page>(page).unwrap();
                if result.is_ok() { state.origin = draft; }
                state.message = Some(result.unwrap_or_else(|error| error));
            }
        }
        refresh(world);
    }
}

#[derive(Default)]
struct Feedback {
    stale: bool,
    message: Option<String>,
    value: String,
}

impl Feedback {
    fn prepare(&mut self, stale: bool, message: Option<&str>) -> &str {
        if self.stale != stale || self.message.as_deref() != message {
            self.stale = stale;
            self.message = message.map(str::to_owned);
            self.value.clear();
            if stale {
                self.value.push_str("Saved focus settings changed. Your draft is kept. Reload active values or Review the draft against the file.");
                if message.is_some() {
                    self.value.push('\n');
                }
            }
            if let Some(message) = message {
                self.value.push_str(message);
            }
            engine::performance::add("settings-feedback-prepared", 1);
        }
        &self.value
    }
}

#[derive(Resource)]
struct Presentation {
    pages: QueryState<Entity, With<Page>>,
    pending: Vec<Entity>,
    summary: Option<(FocusPolicy, String)>,
    feedback: HashMap<Entity, Feedback>,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            pages: world.query_filtered(),
            pending: Vec::new(),
            summary: None,
            feedback: HashMap::new(),
        }
    }
}

fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("focus-settings-presentation");
    let mut presentation = world
        .remove_resource::<Presentation>()
        .unwrap_or_else(|| Presentation::from_world(world));
    render(world, &mut presentation);
    world.insert_resource(presentation);
}

fn render(world: &mut World, presentation: &mut Presentation) {
    presentation.pending.clear();
    presentation.pending.extend(presentation.pages.iter(world));
    presentation
        .feedback
        .retain(|page, _| world.get::<Page>(*page).is_some());
    if presentation.pending.is_empty() {
        return;
    }
    let active = policy(world);
    let support = world
        .get_resource::<CursorWarpSupport>()
        .is_some_and(|support| support.0);
    if presentation
        .summary
        .as_ref()
        .is_none_or(|(source, _)| *source != active)
    {
        let enabled = |value| if value { "On" } else { "Off" };
        presentation.summary = Some((
            active,
            format!(
                "Click: {} · pointer focus: {} · cursor warp: {}",
                enabled(active.click),
                enabled(active.pointer_follow),
                enabled(active.warp)
            ),
        ));
        engine::performance::add("settings-summaries-prepared", 1);
    }
    for page in presentation.pending.drain(..) {
        let state = world.get::<Page>(page).unwrap();
        if state.draft == state.origin && state.origin != active {
            let mut state = world.get_mut::<Page>(page).unwrap();
            state.origin = active;
            state.draft = active;
            state.message = None;
        }
        let state = world.get::<Page>(page).unwrap();
        let (body, toggle, choices, summary, support_label, status, save, draft, origin, expanded) = (
            state.body,
            state.toggle,
            state.choices,
            state.summary,
            state.support,
            state.status,
            state.save,
            state.draft,
            state.origin,
            state.expanded,
        );
        let message = presentation
            .feedback
            .entry(page)
            .or_default()
            .prepare(origin != active, state.message.as_deref());
        display(
            world,
            body,
            if expanded {
                Display::Flex
            } else {
                Display::None
            },
        );
        caption(
            world,
            toggle,
            if expanded {
                "Hide focus settings"
            } else {
                "Edit focus settings"
            },
        );
        for (index, (value, off, on)) in [
            (draft.click, "Click to focus: Off", "Click to focus: On"),
            (
                draft.pointer_follow,
                "Focus follows pointer: Off",
                "Focus follows pointer: On",
            ),
            (
                draft.warp,
                "Warp cursor after keyboard focus: Off",
                "Warp cursor after keyboard focus: On",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            caption(world, choices[index], if value { on } else { off });
        }
        text(world, summary, &presentation.summary.as_ref().unwrap().1);
        text(
            world,
            support_label,
            if support {
                "Cursor warping is available in this Park session."
            } else {
                "This window host cannot warp the system cursor. You can save the preference for a physical Park session."
            },
        );
        text(world, status, message);
        disable(world, save, draft == active || origin != active);
    }
}

fn review_snapshot(world: &World, page: Entity, previous: Option<&configuration::review::Snapshot>) -> Result<configuration::review::Snapshot, String> {
    let state = world.get::<Page>(page).ok_or("The focus editor is unavailable.")?;
    configuration::review::Snapshot::new(&state.origin, &state.draft, &policy(world), previous)
}

fn review_apply(world: &mut World, page: Entity, active: serde_json::Value, draft: serde_json::Value) -> Result<(), String> {
    let origin: FocusPolicy = serde_json::from_value(active).map_err(|error| error.to_string())?;
    let draft: FocusPolicy = serde_json::from_value(draft).map_err(|error| error.to_string())?;
    let mut state = world.get_mut::<Page>(page).ok_or("The focus editor is unavailable.")?;
    state.origin = origin;
    state.draft = draft;
    state.message = Some("Review applied to the draft. Save focus settings to activate it.".into());
    Ok(())
}
