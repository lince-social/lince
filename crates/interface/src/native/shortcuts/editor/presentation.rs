use super::*;
use crate::native::layout::desktop::CommandContext;
use bevy::ecs::query::QueryState;
use std::collections::HashMap;

mod prepared;
pub(super) use prepared::Prepared;

type SummaryView = (Entity, Shortcut, Entity, Entity, Option<CommandContext>);
type RowView = (Entity, Shortcut, Entity, Entity, Entity);

#[derive(Resource)]
struct State {
    modifiers: QueryState<Entity, With<ModifierSummary>>,
    summaries: QueryState<(Entity, &'static Summary)>,
    rows: QueryState<(Entity, &'static Row)>,
    modifier_views: Vec<Entity>,
    summary_views: Vec<SummaryView>,
    row_views: Vec<RowView>,
    contexts: HashMap<Entity, CommandContext>,
    focused: HashSet<Entity>,
}

impl FromWorld for State {
    fn from_world(world: &mut World) -> Self {
        Self {
            modifiers: world.query_filtered(),
            summaries: world.query(),
            rows: world.query(),
            modifier_views: Vec::new(),
            summary_views: Vec::new(),
            row_views: Vec::new(),
            contexts: HashMap::new(),
            focused: HashSet::new(),
        }
    }
}

pub(super) fn refresh(world: &mut World) {
    let mut state = world
        .remove_resource::<State>()
        .unwrap_or_else(|| State::from_world(world));
    render(world, &mut state);
    world.insert_resource(state);
}

fn set_label(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity)
        && node.label() != Some(value)
    {
        node.set_label(value);
        engine::performance::add("shortcut-editor-control-writes", 1);
    }
}

fn text_value(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut text) = world.get_mut::<Text>(entity)
        && text.0 != value
    {
        value.clone_into(&mut text.0);
        engine::performance::add("shortcut-editor-control-writes", 1);
    }
}

fn set_text(world: &mut World, entity: Entity, value: impl AsRef<str>) {
    let value = value.as_ref();
    text_value(world, entity, value);
    if let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent)
        && world.get::<ActionButton>(parent).is_some()
    {
        set_label(world, parent, value);
    }
}

fn caption(world: &mut World, entity: Entity, value: &str, control: Entity, label: &str) {
    text_value(world, entity, value);
    if let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent)
        && world.get::<ActionButton>(parent).is_some()
    {
        if parent == control {
            set_label(world, control, label);
            return;
        }
        set_label(world, parent, value);
    }
    set_label(world, control, label);
}

fn disabled(world: &mut World, entity: Entity, disabled: bool) {
    if world.get::<InteractionDisabled>(entity).is_some() != disabled {
        if disabled {
            world.entity_mut(entity).insert(InteractionDisabled);
        } else {
            world.entity_mut(entity).remove::<InteractionDisabled>();
        }
        engine::performance::add("shortcut-editor-control-writes", 1);
    }
}

fn display(world: &mut World, entity: Entity, visible: bool) {
    let value = if visible {
        Display::Flex
    } else {
        Display::None
    };
    let mut node = world.get_mut::<Node>(entity).unwrap();
    if node.display != value {
        node.display = value;
        engine::performance::add("shortcut-editor-control-writes", 1);
    }
}

fn render(world: &mut World, state: &mut State) {
    let _presentation = engine::performance::Span::new("shortcut-editor-presentation");
    let settings_changed = world.is_resource_changed::<Settings>();
    let keyboard_changed = world.is_resource_changed::<crate::host::input::KeyboardSnapshot>();
    if settings_changed {
        let modifier = world.resource::<Settings>().modifier();
        state.modifier_views.clear();
        state.modifier_views.extend(state.modifiers.iter(world));
        let value = (!state.modifier_views.is_empty()).then(|| {
            format!(
                "Mod is {}. Bindings are shared across workspaces.",
                modifier.label()
            )
        });
        for summary in state.modifier_views.drain(..) {
            set_text(world, summary, value.as_deref().unwrap());
        }
    }
    state.summary_views.clear();
    state
        .summary_views
        .extend(state.summaries.iter(world).map(|(entity, summary)| {
            (
                entity,
                summary.shortcut,
                summary.root,
                summary.label,
                summary.context,
            )
        }));
    state.contexts.clear();
    for (entity, shortcut, root, label, previous) in state.summary_views.drain(..) {
        let context = *state
            .contexts
            .entry(root)
            .or_insert_with(|| crate::native::layout::desktop::CommandContext::new(world, root));
        if !settings_changed && !keyboard_changed && previous == Some(context) {
            continue;
        }
        let settings = world.resource::<Settings>();
        let keyboard = world.get_resource::<crate::host::input::KeyboardSnapshot>();
        let value = shortcut.caption(settings, keyboard);
        let help = shortcut.help(settings, context, keyboard);
        caption(world, label, &value, entity, &help);
        if world
            .get::<crate::native::icons::Tooltip>(entity)
            .is_none_or(|tooltip| tooltip.0 != help)
        {
            world
                .entity_mut(entity)
                .insert(crate::native::icons::Tooltip(help));
        }
        if previous != Some(context) {
            world.get_mut::<Summary>(entity).unwrap().context = Some(context);
        }
    }
    let Some(mut modal) = world.resource_mut::<Editor>().modal.take() else {
        return;
    };
    if world.get_entity(modal.overlay).is_err() {
        super::super::release_keyboard(world, modal.overlay);
        return;
    }
    let filter = world.get::<EditableText>(modal.filter);
    let unchanged = modal.filter_source.as_deref().is_some_and(|previous| {
        filter.map_or_else(|| previous.is_empty(), |text| text.value() == previous)
    });
    let filter_changed = if !unchanged {
        let source = filter
            .map(|text| text.value().to_string())
            .unwrap_or_default();
        let normalized = source.to_lowercase();
        let changed = normalized != modal.observed;
        modal.observed = normalized;
        modal.filter_source = Some(source);
        changed
    } else {
        false
    };
    let focus = world.resource::<InputFocus>().get();
    if !modal.dirty
        && !keyboard_changed
        && !settings_changed
        && !filter_changed
        && focus == modal.observed_focus
    {
        world.resource_mut::<Editor>().modal = Some(modal);
        return;
    }
    modal.observed_focus = focus;
    modal.dirty = false;
    let prepared = modal.prepared.observe(
        &modal.draft,
        &modal.origin,
        world.get_resource::<crate::host::input::KeyboardSnapshot>(),
    );
    modal.review.extend(prepared.review.iter().copied());
    let changed = prepared.changed;
    let count = prepared.count;
    let stale = world.resource::<Settings>() != &modal.origin;
    let validation = &prepared.validation;
    let warning = if stale {
        "Active bindings changed. Reload active bindings or Review the draft against the file.\n"
            .to_owned()
    } else {
        validation
            .as_ref()
            .map(|error| format!("{error}\n"))
            .unwrap_or_default()
    };
    set_text(
        world,
        modal.status,
        format!(
            "{warning}{}\n{changed} changed commands · {count} commands with conflicting bindings{}",
            modal.message,
            if modal.conflicts_only {
                " · showing affected commands"
            } else {
                ""
            }
        ),
    );
    set_text(
        world,
        modal.modifier_text,
        format!("Mod: {}", modal.draft.modifier().label()),
    );
    display(world, modal.capture_panel, modal.capture.is_some());
    disabled(
        world,
        modal.validate,
        modal
            .capture
            .as_ref()
            .is_none_or(|capture| capture.value.is_none()),
    );
    disabled(
        world,
        modal.save,
        stale || validation.is_some() || prepared.conflicting || modal.capture.is_some(),
    );
    if let Some(capture) = &modal.capture {
        set_text(
            world,
            modal.capture_text,
            format!(
                "{}: {}",
                capture.shortcut.label(),
                capture.value.as_deref().unwrap_or(
                    if capture.shortcut.pointer_action().is_some() {
                        "hold a modifier and press a mouse button…"
                    } else {
                        "press a key combination…"
                    }
                )
            ),
        );
    }
    state.focused.clear();
    let mut ancestor = focus;
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = ancestor else {
            break;
        };
        if !state.focused.insert(entity) {
            break;
        }
        ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    state.row_views.clear();
    state.row_views.extend(
        state
            .rows
            .iter(world)
            .map(|(entity, row)| (entity, row.shortcut, row.binding, row.text, row.status)),
    );
    for (entity, shortcut, binding, text, status) in state.row_views.drain(..) {
        let row = prepared.rows.get(&shortcut).unwrap();
        let visible = state.focused.contains(&entity)
            || (!modal.conflicts_only || row.conflicting || modal.review.contains(&shortcut))
                && row.search.contains(&modal.observed);
        display(world, entity, visible);
        caption(world, text, &row.display, binding, &row.accessibility);
        set_text(world, status, &row.message);
    }
    world.resource_mut::<Editor>().modal = Some(modal);
}
