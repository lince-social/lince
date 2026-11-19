use super::*;
use crate::{
    layout::desktop::PlacementId,
    native::{
        canvas::CanvasItem,
        layout::desktop::{
            Managed,
            input_settings::{prepared::Prepared, text as publish},
        },
    },
};
use bevy::ecs::query::QueryState;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(super) struct Observation {
    values: Vec<String>,
    flags: Option<(ApplicationOpening, bool)>,
}

pub(super) fn observe(world: &World, modal: &mut Modal) -> bool {
    let mut previous = std::mem::take(&mut modal.observed);
    let mut changed = false;
    let mut count = 0;
    for (index, entity) in fields(modal).enumerate() {
        let input = world.get::<EditableText>(entity);
        if let Some(value) = previous.values.get_mut(index) {
            if input.map_or(!value.is_empty(), |input| input.value() != value.as_str()) {
                *value = input.map_or_else(String::new, |input| input.value().to_string());
                changed = true;
            }
        } else {
            previous
                .values
                .push(input.map_or_else(String::new, |input| input.value().to_string()));
            changed = true;
        }
        count += 1;
    }
    changed |= previous.values.len() != count;
    previous.values.truncate(count);
    let flags = Some((modal.opening, modal.workspace_back_and_forth));
    changed |= previous.flags != flags;
    previous.flags = flags;
    modal.observed = previous;
    changed
}

type DraftKey = (usize, ApplicationOpening, bool);

fn draft_key(modal: &Modal) -> DraftKey {
    (
        modal.entries.len(),
        modal.opening,
        modal.workspace_back_and_forth,
    )
}

struct Draft {
    owner: Entity,
    origin: LayoutSettings,
    fields: Prepared<DraftKey, LayoutSettings>,
}

impl Draft {
    fn matches(&self, modal: &Modal) -> bool {
        self.owner == modal.overlay && self.origin == modal.origin
    }
}

pub(super) fn candidate<'w>(
    world: &'w World,
    modal: &Modal,
) -> Option<&'w Result<LayoutSettings, String>> {
    let draft = world.get_resource::<Presentation>()?.draft.as_ref()?;
    draft.matches(modal).then_some(())?;
    draft.fields.get(world, fields(modal), &draft_key(modal))
}

struct RowLabels {
    number: String,
    actions: [String; 3],
}

type Focused = Option<(u32, Option<[u16; 2]>)>;

struct FocusSource {
    id: PlacementId,
    intent: Option<[u16; 2]>,
    width: Option<u32>,
}

struct SummaryValue {
    base: Arc<str>,
    focused: Focused,
    text: String,
}

#[derive(Resource)]
struct Presentation {
    summaries: QueryState<(Entity, &'static Summary)>,
    managed: QueryState<(&'static Managed, &'static CanvasItem, &'static ChildOf)>,
    views: Vec<(Entity, Entity)>,
    focus: HashMap<Entity, FocusSource>,
    summary: Option<(LayoutSettings, Arc<str>)>,
    values: HashMap<Entity, SummaryValue>,
    labels: [Vec<RowLabels>; 2],
    draft: Option<Draft>,
    fallback: LayoutSettings,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            summaries: world.query(),
            managed: world.query(),
            views: Vec::new(),
            focus: HashMap::new(),
            summary: None,
            values: HashMap::new(),
            labels: Default::default(),
            draft: None,
            fallback: Default::default(),
        }
    }
}

pub(in crate::native::shortcuts) fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("width-settings-presentation");
    let mut presentation = world
        .remove_resource::<Presentation>()
        .unwrap_or_else(|| Presentation::from_world(world));
    render_summaries(world, &mut presentation);
    render_modal(world, &mut presentation);
    world.insert_resource(presentation);
}

fn render_summaries(world: &mut World, presentation: &mut Presentation) {
    presentation.views.clear();
    presentation.views.extend(
        presentation
            .summaries
            .iter(world)
            .filter(|(_, summary)| {
                !world
                    .get::<crate::native::edit_mode::EditMode>(summary.0)
                    .is_some_and(|mode| !mode.enabled)
            })
            .map(|(entity, summary)| (entity, summary.0)),
    );
    presentation
        .values
        .retain(|entity, _| world.get::<Summary>(*entity).is_some());
    presentation.focus.clear();
    if presentation.views.is_empty() {
        return;
    }
    let layout = world
        .get_resource::<LayoutSettings>()
        .unwrap_or(&presentation.fallback);
    if presentation
        .summary
        .as_ref()
        .is_none_or(|(source, _)| source != layout)
    {
        let value = format!(
            "Width cycle: {} · Row height cycle: {} · Resize step: {} px · Default width: {} · Gaps: {} · New applications: {} · Back-and-forth: {}",
            layout
                .width_presets
                .iter()
                .map(|ratio| text(*ratio))
                .collect::<Vec<_>>()
                .join(" → "),
            layout
                .height_presets
                .iter()
                .map(|ratio| text(*ratio))
                .collect::<Vec<_>>()
                .join(" → "),
            layout.resize_step,
            text(layout.default_width),
            layout.gaps,
            opening_text(layout.application_opening),
            if layout.workspace_back_and_forth {
                "On"
            } else {
                "Off"
            }
        );
        presentation.summary = Some((layout.clone(), value.into()));
        engine::performance::add("settings-summaries-prepared", 1);
    }
    for &(_, root) in &presentation.views {
        if presentation.focus.contains_key(&root) {
            continue;
        }
        let focused = world
            .get::<crate::native::workspace::Workspaces>(root)
            .and_then(|spaces| {
                spaces
                    .entries
                    .iter()
                    .find(|space| space.id == spaces.active)
            })
            .and_then(|space| {
                let id = space.desktop.focused()?;
                let item = space.desktop.items().iter().find(|item| item.id == id)?;
                let intent = if space.desktop.mode() == crate::host::WorkspaceLayout::Tiled
                    && !item.floating
                {
                    space
                        .desktop
                        .item_width(id)
                        .and_then(crate::layout::desktop::Width::ratio)
                } else {
                    item.free_width.map(|width| width.ratio)
                };
                Some(FocusSource {
                    id,
                    intent,
                    width: None,
                })
            });
        if let Some(focused) = focused {
            presentation.focus.insert(root, focused);
        }
    }
    let mut remaining = presentation.focus.len();
    if remaining != 0 {
        for (managed, item, parent) in presentation.managed.iter(world) {
            if let Some(focused) = presentation.focus.get_mut(&parent.parent())
                && focused.id == managed.id
                && focused.width.is_none()
            {
                focused.width = Some(item.size.x.to_bits());
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
            }
        }
    }
    let base = &presentation.summary.as_ref().unwrap().1;
    for &(entity, root) in &presentation.views {
        let focused = presentation
            .focus
            .get(&root)
            .and_then(|focused| focused.width.map(|width| (width, focused.intent)));
        let value = presentation
            .values
            .entry(entity)
            .or_insert_with(|| SummaryValue {
                base: Arc::from(""),
                focused: None,
                text: String::new(),
            });
        if !Arc::ptr_eq(&value.base, base) || value.focused != focused {
            value.base = Arc::clone(base);
            value.focused = focused;
            value.text.clear();
            value.text.push_str(base);
            if let Some((width, intent)) = focused {
                use std::fmt::Write;
                let intent = intent
                    .map(|ratio| format!("preset {}", text(ratio)))
                    .unwrap_or_else(|| "manual size".into());
                let _ = write!(
                    value.text,
                    "\nFocused width: {:.0} logical px · {}",
                    f32::from_bits(width),
                    intent
                );
            }
            engine::performance::add("width-settings-focus-summaries-prepared", 1);
        }
        publish(world, entity, &value.text);
    }
}

fn render_modal(world: &mut World, presentation: &mut Presentation) {
    let Some(mut modal) = world
        .get_resource_mut::<WidthEditor>()
        .and_then(|mut editor| editor.0.take())
    else {
        presentation.draft = None;
        return;
    };
    if world.get_entity(modal.overlay).is_err() {
        presentation.draft = None;
        super::super::release_keyboard(world, modal.overlay);
        focus(world, modal.previous);
        return;
    }
    if observe(world, &mut modal) {
        modal.message.clear();
    }
    if presentation
        .draft
        .as_ref()
        .is_none_or(|draft| !draft.matches(&modal))
    {
        presentation.draft = Some(Draft {
            owner: modal.overlay,
            origin: modal.origin.clone(),
            fields: Default::default(),
        });
    }
    let result = presentation.draft.as_mut().unwrap().fields.prepare(
        world,
        fields(&modal),
        &draft_key(&modal),
        || parse_candidate(world, &modal),
    );
    let stale = &modal.origin
        != world
            .get_resource::<LayoutSettings>()
            .unwrap_or(&presentation.fallback);
    let message = result.as_ref().err().map(String::as_str).unwrap_or_else(|| if stale {
        "Active layout settings changed. Reload active layout values or Review the draft against the file."
    } else { &modal.message });
    publish(world, modal.status, message);
    disabled(world, modal.save, result.is_err() || stale);
    disabled(world, modal.add, modal.entries.len() == 32);
    disabled(world, modal.add_height, modal.heights.len() == 32);
    for (kind_index, (kind, entries)) in [
        (Preset::Width, &modal.entries),
        (Preset::RowHeight, &modal.heights),
    ]
    .into_iter()
    .enumerate()
    {
        let labels = &mut presentation.labels[kind_index];
        for (index, entry) in entries.iter().enumerate() {
            if labels.len() <= index {
                labels.push(RowLabels {
                    number: format!("{} preset {}", kind.label(), index + 1),
                    actions: ["Move up", "Move down", "Remove"].map(|action| {
                        format!(
                            "{action} {} preset {}",
                            kind.label().to_lowercase(),
                            index + 1
                        )
                    }),
                });
                engine::performance::add("width-settings-row-labels-prepared", 1);
            }
            let labels = &labels[index];
            publish(world, entry.number, &labels.number);
            accessibility_label(world, entry.input, &labels.number);
            for (entity, caption) in [entry.up, entry.down, entry.remove]
                .into_iter()
                .zip(&labels.actions)
            {
                accessibility_label(world, entity, caption);
            }
            disabled(world, entry.up, index == 0);
            disabled(world, entry.down, index + 1 == entries.len());
            disabled(world, entry.remove, entries.len() == 1);
        }
    }
    world.resource_mut::<WidthEditor>().0 = Some(modal);
}
