use super::super::input_settings::{button, disable, feedback, field, text, prepared::Prepared};
use bevy::ecs::query::QueryState;
use std::collections::HashMap;
use crate::{
    layout::desktop::OverviewSettings,
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

#[derive(Component)]
struct Page {
    body: Entity,
    toggle: (Entity, Entity),
    fields: [Entity; 2],
    summary: Entity,
    support: Entity,
    status: Entity,
    save: Entity,
    origin: OverviewSettings,
    error: Option<String>,
    expanded: bool,
}

#[derive(Clone, Copy)]
enum Change {
    Toggle,
    Save,
    Cancel,
    Reload,
    Rebase,
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        PostUpdate,
        refresh
            .after(crate::native::actions::ApplyActions)
            .before(bevy::ui::UiSystems::Layout),
    );
}

pub(super) fn current(world: &World) -> OverviewSettings {
    world
        .get_resource::<LayoutSettings>()
        .map_or_else(OverviewSettings::default, |layout| layout.overview)
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    let origin = current(world);
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
            Name::new("park.overview-settings"),
            ChildOf(parent),
        ))
        .id();
    label(world, page, "Overview transfers", 20.0);
    let summary = label(world, page, "", 14.0);
    let support = label(world, page, "", 14.0);
    world.entity_mut(support).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    {
        let mut node = world.get_mut::<Node>(support).unwrap();
        node.width = percent(100);
        node.min_width = px(0);
        node.flex_shrink = 1.0;
    }
    let toggle = button(world, page, page, "Edit overview transfers", Change::Toggle);
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
    let description = label(
        world,
        body,
        "Drag an item to rearrange it. Holding a real file drag over a workspace opens it after the hover delay.",
        14.0,
    );
    world.entity_mut(description).insert(TextLayout::linebreak(
        bevy::text::LineBreak::WordOrCharacter,
    ));
    world.get_mut::<Node>(description).unwrap().width = percent(100);
    let fields = [
        field(world, body, "File drag hover delay (100–2000 ms)", 4),
        field(world, body, "Edge scroll speed (0–100% of viewport/s)", 3),
    ];
    let status = feedback(world, body);
    let save = button(world, body, page, "Save overview transfers", Change::Save).0;
    button(world, body, page, "Cancel overview changes", Change::Cancel);
    button(world, body, page, "Reload active overview values", Change::Reload);
    button(world, body, page, "Review overview draft against file", Change::Rebase);
    world.entity_mut(page).insert(Page {
        body,
        toggle,
        fields,
        summary,
        support,
        status,
        save,
        origin,
        error: None,
        expanded: false,
    });
    load(world, page, origin);
    refresh(world);
}

fn load(world: &mut World, page: Entity, value: OverviewSettings) {
    let fields = world.get::<Page>(page).unwrap().fields;
    for (entity, value) in fields
        .into_iter()
        .zip([value.hover_delay_ms, value.edge_scroll_percent])
    {
        let mut input = world.get_mut::<EditableText>(entity).unwrap();
        let value = value.to_string();
        if input.value() != value.as_str() {
            input.editor.set_text(&value);
        }
    }
    world.get_mut::<Page>(page).unwrap().origin = value;
}

fn candidate(world: &World, page: Entity) -> Result<OverviewSettings, String> {
    let fields = world
        .get::<Page>(page)
        .ok_or("Overview settings are unavailable.")?
        .fields;
    if let Some(value) = world
        .get_resource::<Presentation>()
        .and_then(|presentation| presentation.drafts.get(&page))
        .and_then(|draft| draft.get(world, fields, &()))
    {
        return value.clone();
    }
    parse_fields(world, fields)
}

fn parse_fields(world: &World, fields: [Entity; 2]) -> Result<OverviewSettings, String> {
    let value = |entity| -> Result<u16, String> {
        let input = world
            .get::<EditableText>(entity)
            .ok_or("An overview field is unavailable.")?;
        if input.is_composing() || !input.pending_edits.is_empty() || input.pending_paste.is_some()
        {
            return Err("Finish typing before saving overview settings.".into());
        }
        input
            .value()
            .to_cow()
            .trim()
            .parse()
            .map_err(|_| "Use whole numbers for overview delay and speed.".into())
    };
    let value = OverviewSettings {
        hover_delay_ms: value(fields[0])?,
        edge_scroll_percent: value(fields[1])?,
    };
    value.validate()?;
    Ok(value)
}

impl Action for Change {
    fn apply(&self, world: &mut World, page: Entity) {
        let Some(state) = world.get::<Page>(page) else {
            return;
        };
        if crate::native::shortcuts::keyboard_owned(world) {
            return;
        }
        let (body, toggle, status, expanded) =
            (state.body, state.toggle, state.status, state.expanded);
        match self {
            Self::Toggle => {
                world.get_mut::<Page>(page).unwrap().expanded = !expanded;
                world.get_mut::<Node>(body).unwrap().display = if expanded {
                    Display::None
                } else {
                    Display::Flex
                };
                let caption = if expanded {
                    "Edit overview transfers"
                } else {
                    "Hide overview settings"
                };
                text(world, toggle.1, caption);
                world
                    .get_mut::<bevy::a11y::AccessibilityNode>(toggle.0)
                    .unwrap()
                    .set_label(caption);
                if expanded {
                    world
                        .resource_mut::<InputFocus>()
                        .set(toggle.0, FocusCause::Navigated);
                }
            }
            Self::Cancel | Self::Reload => {
                let fields = world.get::<Page>(page).unwrap().fields;
                if let Err(error) = configuration::review::settled(world, fields) { text(world, status, error); return; }
                let active = current(world);
                load(world, page, active);
                text(world, status, "Overview draft discarded.");
            }
            Self::Rebase => {
                if let Err(error) = configuration::review::open(world, page, "overview settings", review_snapshot, review_apply) { text(world, status, error); }
            }
            Self::Save => {
                let result = (|| {
                    let value = candidate(world, page)?;
                    if current(world) != world.get::<Page>(page).unwrap().origin {
                        return Err("Overview settings changed while you were editing. Reload active values or Review the draft against the file; your draft is kept.".to_owned());
                    }
                    let mut layout = world
                        .get_resource::<LayoutSettings>()
                        .cloned()
                        .unwrap_or_default();
                    layout.overview = value;
                    let shortcuts = world
                        .get_resource::<Settings>()
                        .cloned()
                        .unwrap_or_default();
                    let message = configuration::commit(world, shortcuts, layout)?;
                    world.get_mut::<Page>(page).unwrap().origin = value;
                    Ok::<String, String>(message)
                })();
                text(world, status, result.unwrap_or_else(|error| error));
            }
        }
        refresh(world);
    }
}

#[derive(Resource)]
struct Presentation {
    pages: QueryState<Entity, With<Page>>,
    pending: Vec<Entity>,
    drafts: HashMap<Entity, Prepared<(), OverviewSettings>>,
    summary: Option<(OverviewSettings, String)>,
    support: String,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            pages: world.query_filtered(),
            pending: Vec::new(),
            drafts: HashMap::new(),
            summary: None,
            support: String::new(),
        }
    }
}

fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("overview-settings-presentation");
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
        .drafts
        .retain(|page, _| world.get::<Page>(*page).is_some());
    if presentation.pending.is_empty() {
        return;
    }
    let active = current(world);
    if presentation
        .summary
        .as_ref()
        .is_none_or(|(previous, _)| *previous != active)
    {
        presentation.summary = Some((
            active,
            format!(
                "File hover: {} ms · edge scroll: {}% of viewport/s",
                active.hover_delay_ms, active.edge_scroll_percent
            ),
        ));
        engine::performance::add("settings-summaries-prepared", 1);
    }
    let support = world.get_resource::<crate::host::FileDragSupport>().map_or(
        "File hover requires file-drag events from the window host. Park provides this directly.",
        |support| if support.available { "File-hover activation is available in this host." }
            else { support.reason.as_deref().unwrap_or("This window host does not provide file-drag events. The saved settings can still be used in Park.") },
    );
    if presentation.support != support {
        support.clone_into(&mut presentation.support);
    }
    for page in presentation.pending.drain(..) {
        let state = world.get::<Page>(page).unwrap();
        let (fields, origin, summary, status, save, support_entity) = (
            state.fields,
            state.origin,
            state.summary,
            state.status,
            state.save,
            state.support,
        );
        let prepared = presentation.drafts.entry(page).or_default();
        let mut draft = prepared.prepare(world, fields, &(), || parse_fields(world, fields));
        if draft.as_ref().is_ok_and(|draft| *draft == origin) && origin != active {
            load(world, page, active);
            draft = prepared.prepare(world, fields, &(), || parse_fields(world, fields));
        }
        text(world, support_entity, &presentation.support);
        text(world, summary, &presentation.summary.as_ref().unwrap().1);
        let origin = world.get::<Page>(page).unwrap().origin;
        let error = draft.as_ref().err().map(String::as_str).or_else(|| (origin != active)
            .then_some("Overview settings changed. Reload active values or Review the draft against the file; your draft is kept."));
        disable(world, save, error.is_some());
        if world.get::<Page>(page).unwrap().error.as_deref() != error {
            text(
                world,
                status,
                error.unwrap_or("Ready to save overview settings."),
            );
            world.get_mut::<Page>(page).unwrap().error = error.map(str::to_owned);
        }
    }
}

fn review_snapshot(world: &World, page: Entity, previous: Option<&configuration::review::Snapshot>) -> Result<configuration::review::Snapshot, String> {
    let state = world.get::<Page>(page).ok_or("The overview editor is unavailable.")?;
    configuration::review::Snapshot::new(&state.origin, &candidate(world, page)?, &current(world), previous)
}

fn review_apply(world: &mut World, page: Entity, active: serde_json::Value, draft: serde_json::Value) -> Result<(), String> {
    let origin: OverviewSettings = serde_json::from_value(active).map_err(|error| error.to_string())?;
    let draft: OverviewSettings = serde_json::from_value(draft).map_err(|error| error.to_string())?;
    draft.validate()?;
    if world.get::<Page>(page).is_none() { return Err("The overview editor is unavailable.".into()); }
    load(world, page, draft);
    let mut state = world.get_mut::<Page>(page).unwrap();
    state.origin = origin;
    state.error = None;
    let status = state.status;
    text(world, status, "Review applied to the draft. Save overview settings to activate it.");
    Ok(())
}

#[cfg(test)]
mod tests;
