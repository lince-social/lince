// non-lean-yet
// Scope: mod tests, struct Settings, enum Kind, enum Edit.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 131, 208, 397.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (258, 410).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (272, 358, 400).

use super::{Action, ShaderSettings};
use bevy::{
    ecs::entity_disabling::Disabled,
    input_focus::{FocusLost, InputFocus},
    prelude::*,
    text::EditableText,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Settings {
    pub shadow_radius: f32,
    pub blur_radius: f32,
    pub appearance_fade_ms: u16,
}

impl Settings {
    pub(crate) fn valid(self) -> bool {
        self.shadow_radius.is_finite()
            && (0.0..=32.0).contains(&self.shadow_radius)
            && self.blur_radius.is_finite()
            && (0.0..=24.0).contains(&self.blur_radius)
            && self.appearance_fade_ms <= 200
    }

    fn value(self, kind: Kind) -> f32 {
        match kind {
            Kind::Shadow => self.shadow_radius,
            Kind::Blur => self.blur_radius,
            Kind::Fade => f32::from(self.appearance_fade_ms),
        }
    }

    fn set(&mut self, kind: Kind, value: f32) -> bool {
        let mut candidate = *self;
        if !value.is_finite() {
            return false;
        }
        match kind {
            Kind::Shadow => candidate.shadow_radius = value,
            Kind::Blur => candidate.blur_radius = value,
            Kind::Fade if (0.0..=200.0).contains(&value) && value.fract() == 0.0 => {
                candidate.appearance_fade_ms = value as u16
            }
            Kind::Fade => return false,
        }
        if !candidate.valid() {
            return false;
        }
        *self = candidate;
        true
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Shadow,
    Blur,
    Fade,
}

#[derive(Clone, Copy, PartialEq)]
enum Edit {
    Apply,
    Cancel,
}

#[derive(Component)]
struct Field {
    root: Entity,
    canvas: String,
    kind: Kind,
    origin: f32,
    status: Entity,
    pending: Option<Edit>,
    conflict: bool,
}

impl Action for Edit {
    fn apply(&self, world: &mut World, target: Entity) {
        if let Some(mut field) = world.get_mut::<Field>(target) {
            field.pending = Some(*self);
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
pub(crate) struct Opacity(pub f32);

#[derive(Resource)]
pub(super) struct PresentationActive(pub bool);

#[derive(Component, Clone, Copy, PartialEq)]
struct Fade {
    started: Option<Instant>,
    from: f32,
    focused: bool,
    finished: bool,
}

pub(super) fn install(app: &mut App) {
    app.add_systems(
        PostUpdate,
        fields
            .after(crate::native::actions::ApplyActions)
            .before(bevy::ui::UiSystems::Layout),
    )
    .add_systems(
        PostUpdate,
        animate
            .after(crate::native::layout::ResolveFollow)
            .before(crate::native::topology::presentation::synchronize),
    );
}

pub(super) fn controls(world: &mut World, root: Entity, panel: Entity) {
    use crate::native::{
        actions::{ActionSequence, KeyBinding, KeyBindings, Modifiers},
        edit_mode::label,
    };
    let Some(canvas) = world
        .get::<crate::native::workspace::Workspaces>(root)
        .map(|spaces| spaces.canvas_id.clone())
    else {
        return;
    };
    let settings = world
        .get_resource::<ShaderSettings>()
        .copied()
        .unwrap_or_default()
        .compositor;
    label(world, panel, "Window and canvas appearance", 18.0);
    label(
        world,
        panel,
        "These settings apply to Lince items and, in Park, application windows. Enter or leaving a field applies its value; Escape restores it. Save the workspace to keep changes.",
        14.0,
    );
    for (kind, caption, error) in [
        (
            Kind::Shadow,
            "Shadow radius (0–32 logical units)",
            "Enter a finite radius from 0 to 32.",
        ),
        (
            Kind::Blur,
            "Background blur radius (0–24 logical units)",
            "Enter a finite radius from 0 to 24.",
        ),
        (
            Kind::Fade,
            "Appearance fade (0–200 milliseconds)",
            "Enter a whole number of milliseconds from 0 to 200.",
        ),
    ] {
        label(world, panel, caption, 14.0);
        let value = settings.value(kind);
        let input = world
            .spawn(crate::controls::single_line_editor(
                &value.to_string(),
                world.resource::<crate::native::theme::Typography>(),
                0,
                24,
            ))
            .id();
        world
            .entity_mut(input)
            .insert((
                Node {
                    width: percent(100),
                    min_width: px(0),
                    height: px(36),
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(panel),
                KeyBindings(vec![
                    KeyBinding::new(
                        KeyCode::Enter,
                        Modifiers::default(),
                        ActionSequence::default().then(Edit::Apply),
                    ),
                    KeyBinding::new(
                        KeyCode::Escape,
                        Modifiers::default(),
                        ActionSequence::default().then(Edit::Cancel),
                    ),
                ]),
            ))
            .observe(|event: On<FocusLost>, mut commands: Commands| {
                let entity = event.entity;
                commands.queue(move |world: &mut World| Edit::Apply.apply(world, entity));
            });
        crate::native::accessibility::input(world, input, caption, false);
        let status = label(world, panel, "", 13.0);
        crate::native::accessibility::status(world, status);
        world.entity_mut(input).insert((
            Field {
                root,
                canvas: canvas.clone(),
                kind,
                origin: value,
                status,
                pending: None,
                conflict: false,
            },
            InvalidMessage(error),
        ));
    }
    let status = label(world, panel, "", 13.0);
    world.entity_mut(status).insert(RenderStatus);
    crate::native::accessibility::status(world, status);
}

#[derive(Component)]
struct InvalidMessage(&'static str);

#[derive(Component)]
struct RenderStatus;

fn write(world: &mut World, entity: Entity, value: f32) {
    if let Some(mut input) = world.get_mut::<EditableText>(entity) {
        if !input.pending_edits.is_empty() {
            input.pending_edits.clear();
        }
        if input.pending_paste.is_some() {
            input.pending_paste = None;
        }
        let value = value.to_string();
        if input.value() != value.as_str() {
            input.editor.set_text(&value);
            input.queue_edit(bevy::text::TextEdit::TextEnd(false));
        }
    }
}

fn message(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut text) = world.get_mut::<Text>(entity)
        && text.0 != value
    {
        text.0 = value.into();
    }
}

fn fields(world: &mut World) {
    let _maintenance = nucleus::performance::Span::new("appearance-field-maintenance");
    super::update_settings_controls(world);
    world
        .run_system_cached(maintain_fields)
        .expect("maintain current appearance controls");
}

fn maintain_fields(
    world: &mut World,
    fields: &mut QueryState<Entity, With<Field>>,
    labels: &mut QueryState<Entity, With<RenderStatus>>,
    mut entities: Local<Vec<Entity>>,
    mut rendered: Local<Option<(crate::native::topology::compositor::Status, String)>>,
) {
    entities.clear();
    entities.extend(fields.iter(world));
    let has_fields = !entities.is_empty();
    for entity in entities.drain(..) {
        let field = world.get::<Field>(entity).unwrap();
        let (kind, status, mut origin, mut conflict, mut pending) = (
            field.kind,
            field.status,
            field.origin,
            field.conflict,
            field.pending,
        );
        let before = (origin.to_bits(), conflict, pending);
        let current = world
            .get_resource::<ShaderSettings>()
            .copied()
            .unwrap_or_default()
            .compositor
            .value(kind);
        let eligible =
            crate::native::shortcuts::input_eligible(world, field.root, &field.canvas, entity)
                && !world
                    .get_resource::<crate::host::control::Status>()
                    .is_some_and(|status| status.available && (status.locked || !status.active));
        if let Some(edit) = pending.take() {
            if !eligible {
                message(
                    world,
                    status,
                    "The original appearance control is no longer active.",
                );
            } else if matches!(edit, Edit::Cancel) {
                write(world, entity, current);
                origin = current;
                conflict = false;
                message(world, status, "");
            } else if current != origin {
                conflict = true;
                message(
                    world,
                    status,
                    "This setting changed. Press Escape to load its current value.",
                );
            } else if let Some(input) = world.get::<EditableText>(entity) {
                if input.is_composing() {
                    message(
                        world,
                        status,
                        "Finish composing before applying this value.",
                    );
                } else if !input.pending_edits.is_empty() || input.pending_paste.is_some() {
                    pending = Some(Edit::Apply);
                    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                        wake.after(Duration::ZERO);
                    }
                } else {
                    let candidate = input.value().to_cow().trim().parse::<f32>().ok();
                    let mut settings = world
                        .get_resource::<ShaderSettings>()
                        .copied()
                        .unwrap_or_default()
                        .compositor;
                    if let Some(value) = candidate.filter(|value| settings.set(kind, *value)) {
                        world.resource_mut::<ShaderSettings>().compositor = settings;
                        origin = value;
                        conflict = false;
                        message(world, status, "");
                    } else {
                        let error = world
                            .get::<InvalidMessage>(entity)
                            .map_or("Invalid appearance value.", |error| error.0);
                        message(world, status, error);
                    }
                }
            }
        } else if eligible
            && !conflict
            && current != origin
            && world.get_resource::<InputFocus>().and_then(InputFocus::get) != Some(entity)
            && world.get::<EditableText>(entity).is_some_and(|input| {
                !input.is_composing()
                    && input.pending_edits.is_empty()
                    && input.pending_paste.is_none()
                    && input.value().to_cow().trim().parse::<f32>().ok() == Some(origin)
            })
        {
            write(world, entity, current);
            origin = current;
        }
        if before != (origin.to_bits(), conflict, pending) {
            let mut field = world.get_mut::<Field>(entity).unwrap();
            field.origin = origin;
            field.conflict = conflict;
            field.pending = pending;
            nucleus::performance::add("appearance-field-state-updates", 1);
        }
    }
    entities.extend(labels.iter(world));
    if entities.is_empty() {
        if !has_fields {
            *entities = Vec::new();
        }
        *rendered = None;
        return;
    }
    let empty = crate::native::topology::compositor::Status::default();
    let current = world
        .get_resource::<crate::native::topology::compositor::Status>()
        .unwrap_or(&empty);
    if rendered.as_ref().is_none_or(|(old, _)| old != current) {
        let mut messages = Vec::new();
        if current.shadow_limited > 0 {
            messages.push(format!(
                "Some shadows could not be rendered ({}).",
                current.shadow_limited
            ));
        }
        if current.blur_limited > 0 {
            messages.push(format!(
                "Some background blur could not be rendered ({}).",
                current.blur_limited
            ));
        }
        if current.blur_pending > 0 {
            messages.push("Preparing background blur.".to_owned());
        }
        for reason in current.shadow_limits.union(&current.blur_limits) {
            messages.push(reason.message().to_owned());
        }
        *rendered = Some((current.clone(), messages.join(" ")));
        nucleus::performance::add("appearance-status-builds", 1);
    }
    let status = &rendered.as_ref().unwrap().1;
    for label in entities.drain(..) {
        message(world, label, status);
    }
}

fn animate(world: &mut World) {
    let _maintenance = nucleus::performance::Span::new("appearance-fade-maintenance");
    world
        .run_system_cached(maintain_fades)
        .expect("maintain current appearance fades");
}

fn maintain_fades(
    world: &mut World,
    items: &mut QueryState<Entity, With<crate::native::canvas::CanvasItem>>,
    decorated: &mut QueryState<
        Entity,
        (
            With<crate::native::canvas::CanvasItem>,
            Or<(With<Fade>, With<Opacity>)>,
        ),
    >,
    mut entities: Local<Vec<(Entity, Option<Entity>)>>,
    mut focused: Local<HashMap<Entity, Option<Entity>>>,
) {
    let settings = world
        .get_resource::<ShaderSettings>()
        .copied()
        .unwrap_or_default();
    let protected = world
        .get_resource::<crate::host::control::Status>()
        .is_some_and(|status| status.available && (status.locked || !status.active));
    let duration = if settings.disabled
        || settings.reduced_motion
        || protected
        || !super::compositor_presentation_active(world)
        || !settings.compositor.valid()
    {
        0
    } else {
        settings.compositor.appearance_fade_ms
    };
    entities.clear();
    focused.clear();
    if duration == 0 {
        entities.extend(decorated.iter(world).map(|entity| (entity, None)));
        for (entity, _) in entities.drain(..) {
            world.entity_mut(entity).remove::<(Fade, Opacity)>();
        }
        *entities = Vec::new();
        *focused = HashMap::new();
        return;
    }
    entities.extend(
        items
            .iter(world)
            .map(|entity| (entity, crate::native::layout::desktop::root(world, entity))),
    );
    if entities.is_empty() {
        *entities = Vec::new();
        *focused = HashMap::new();
        return;
    }
    for (_, root) in entities.iter() {
        if let Some(root) = root {
            focused
                .entry(*root)
                .or_insert_with(|| crate::native::layout::desktop::focused_entity(world, *root));
        }
    }
    let now = Instant::now();
    let mut active = false;
    for (entity, root) in entities.drain(..) {
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
            || world.get::<Disabled>(entity).is_some()
        {
            if world.get::<Fade>(entity).is_some() || world.get::<Opacity>(entity).is_some() {
                world.entity_mut(entity).remove::<(Fade, Opacity)>();
            }
            continue;
        }
        let is_focused =
            root.and_then(|root| focused.get(&root).copied().flatten()) == Some(entity);
        let ready = world
            .get::<crate::native::topology::presentation::Surface>(entity)
            .is_some_and(|surface| surface.visible)
            && crate::native::topology::presentation::displayed(world, entity);
        let previous = world.get::<Fade>(entity).copied();
        let mut fade = previous.unwrap_or(Fade {
            started: None,
            from: 0.0,
            focused: is_focused,
            finished: false,
        });
        if !ready {
            fade.started = None;
            fade.from = 0.0;
            fade.finished = false;
        } else if is_focused && !fade.focused {
            fade.started = Some(now);
            fade.from = 0.85;
            fade.finished = false;
        } else if fade.started.is_none() && !fade.finished {
            fade.started = Some(now);
        }
        fade.focused = is_focused;
        let alpha = if fade.finished {
            1.0
        } else {
            fade.started.map_or(0.0, |started| {
                let fraction = now.saturating_duration_since(started).as_secs_f32()
                    / (f32::from(duration) / 1000.0);
                if fraction >= 1.0 {
                    fade.finished = true;
                    1.0
                } else {
                    active = true;
                    fade.from + (1.0 - fade.from) * fraction
                }
            })
        };
        if world.get::<Opacity>(entity) != Some(&Opacity(alpha)) {
            world.entity_mut(entity).insert(Opacity(alpha));
        }
        if previous != Some(fade) {
            if let Some(mut current) = world.get_mut::<Fade>(entity) {
                *current = fade;
            } else {
                world.entity_mut(entity).insert(fade);
            }
            nucleus::performance::add("appearance-fade-state-updates", 1);
        }
    }
    if active && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.after(Duration::from_millis(16));
    }
}
