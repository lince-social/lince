use super::*;
use bevy::ecs::{entity_disabling::Disabled, query::Allow};

pub(super) fn input_changed(
    world: &World,
    owner: Entity,
    key: &InputKey,
    input: &EditableText,
) -> bool {
    let editor = world.get::<Editor>(owner).unwrap();
    let surface = match editor.target {
        Target::Surface(entity) => world.get::<Surface>(entity),
        Target::Record(_) => None,
    };
    let different = |value: &str| input.value() != value;
    match key {
        InputKey::GeometrySource | InputKey::DeformationSource => {
            let model::spatial::Mode::Mesh(settings) = &editor.effect.mode else {
                return false;
            };
            if matches!(key, InputKey::GeometrySource) {
                settings
                    .geometry
                    .as_ref()
                    .is_some_and(|stage| different(&stage.source))
            } else {
                settings
                    .deformation
                    .as_ref()
                    .is_some_and(|stage| different(&stage.source))
            }
        }
        InputKey::EffectLabel | InputKey::NewEffectId | InputKey::NewEffectLabel => {
            editor.catalog.input_changed(key, input)
        }
        InputKey::BindingLabel(_)
        | InputKey::BindingRecord(_)
        | InputKey::BindingOrgan(_)
        | InputKey::BindingNamespace(_)
        | InputKey::BindingPath(_) => numeric::input_changed(editor, key, input),
        InputKey::Source(name) => editor
            .effect
            .programs
            .get(name)
            .is_none_or(|value| different(value)),
        InputKey::Threshold(index) => editor
            .effect
            .rules
            .get(*index)
            .is_some_and(|rule| different(&rule.value)),
        InputKey::Range { task, end } => {
            let range = if *task {
                &editor.effect.inputs.task
            } else {
                &editor.effect.inputs.progress
            };
            range
                .as_ref()
                .is_some_and(|range| different(if *end { &range.end } else { &range.start }))
        }
        InputKey::Quantity => {
            if matches!(editor.target, Target::Record(_))
                && !editor.custom_quantity
                && different(editor.quantity.as_deref().unwrap_or(""))
            {
                return true;
            }
            if surface.is_some_and(|surface| different(&surface.quantity)) {
                return true;
            }
            let value = input.value().to_cow();
            model::quantity_text(&value) != editor.quantity.as_deref()
        }
        InputKey::ReferencedEffect | InputKey::Record | InputKey::Organ => {
            let Some(surface) = surface else {
                return false;
            };
            let value = input.value().to_cow();
            let value = value.trim();
            let valid = value.is_empty()
                || if matches!(key, InputKey::ReferencedEffect) {
                    model::valid_effect_id(value)
                } else {
                    nucleus::valid_uid(value, "r")
                };
            if !valid {
                return false;
            }
            let value = (!value.is_empty()).then_some(value);
            match key {
                InputKey::ReferencedEffect => surface.effect_id.as_deref() != value,
                InputKey::Record => surface.record.as_deref() != value,
                InputKey::Organ => match &surface.source {
                    Source::Local => value.is_some(),
                    Source::Organ(organ) => Some(organ.as_str()) != value,
                },
                _ => unreachable!(),
            }
        }
        InputKey::Strength | InputKey::Glow => {
            let Some(surface) = surface else {
                return false;
            };
            let value = input.value().to_cow();
            let Ok(value) = value.parse::<f32>() else {
                return false;
            };
            if !value.is_finite() {
                return false;
            }
            let (current, maximum) = if matches!(key, InputKey::Strength) {
                (surface.strength, 1.0)
            } else {
                (surface.glow, 24.0)
            };
            current.to_bits() != value.clamp(0.0, maximum).to_bits()
        }
    }
}

pub(super) fn publish_preview(
    In(owner): In<Entity>,
    mut editors: Query<&mut Editor, Allow<Disabled>>,
    mut previews: Query<&mut shader::EffectPreview, Allow<Disabled>>,
    mut surfaces: Query<&mut Surface, Allow<Disabled>>,
    contexts: Query<&numeric::Context, Allow<Disabled>>,
) -> Option<(Entity, numeric::Context)> {
    let _publication = nucleus::performance::Span::new("shader-authoring-preview-publication");
    let mut editor = editors.get_mut(owner).unwrap();
    let entity = editor.preview;
    let mut preview = previews.get_mut(entity).ok();
    let working = preview.as_ref().and_then(|preview| preview.working());
    let bootstrapping = editor
        .bootstrap
        .as_ref()
        .is_some_and(|bootstrap| working != Some(bootstrap));
    if matches!(editor.target, Target::Record(_)) {
        let capture = editor.catalog.needs_capture(&editor.effect);
        let capture_working = !bootstrapping
            && editor
                .catalog
                .needs_working_capture(&editor.effect, working);
        if capture || capture_working {
            let Editor {
                catalog, effect, ..
            } = &mut *editor;
            if capture {
                catalog.capture(effect);
            }
            if capture_working {
                catalog.capture_working(effect, working);
            }
        }
    }
    if !bootstrapping && editor.bootstrap.is_some() {
        editor.bootstrap = None;
    }
    if let Some(preview) = preview.as_mut() {
        let effect = if bootstrapping {
            editor.bootstrap.as_ref().unwrap()
        } else {
            &editor.effect
        };
        if !preview.effect.same_authored(effect) {
            preview.effect.clone_from(effect);
            nucleus::performance::add("shader-authoring-effect-copies", 1);
        }
        if preview.quantity != editor.quantity {
            preview.quantity.clone_from(&editor.quantity);
        }
    }
    let Target::Surface(target) = editor.target else {
        return None;
    };
    let Ok(mut surface) = surfaces.get_mut(target) else {
        return None;
    };
    if let Some(preview) = preview.as_mut() {
        let strength = surface.strength.min(0.65);
        if preview.strength.to_bits() != strength.to_bits() {
            preview.strength = strength;
        }
        if preview.motion != surface.motion {
            preview.motion = surface.motion;
        }
        if preview.frozen != surface.frozen {
            preview.frozen = surface.frozen;
        }
        if preview.disabled != surface.disabled {
            preview.disabled = surface.disabled;
        }
    }
    let context = contexts
        .get(entity)
        .ok()
        .is_none_or(|context| context.record != surface.record || context.source != surface.source)
        .then(|| {
            (
                entity,
                numeric::Context {
                    record: surface.record.clone(),
                    source: surface.source.clone(),
                },
            )
        });
    if surface
        .draft
        .as_ref()
        .is_none_or(|draft| !draft.same_authored(&editor.effect))
    {
        surface.draft = Some(editor.effect.clone());
        nucleus::performance::add("shader-authoring-draft-copies", 1);
    }
    context
}

pub(super) fn set_record_quantity(
    In((owner, quantity)): In<(Entity, Option<String>)>,
    mut editors: Query<&mut Editor, Allow<Disabled>>,
    inputs: Query<(Entity, &Input)>,
    mut texts: Query<&mut EditableText, Allow<Disabled>>,
    mut previews: Query<&mut shader::EffectPreview, Allow<Disabled>>,
) {
    let mut editor = editors.get_mut(owner).unwrap();
    let preview = editor.preview;
    if editor.quantity != quantity {
        editor.quantity.clone_from(&quantity);
    }
    let value = quantity.as_deref().unwrap_or("");
    for (entity, input) in &inputs {
        if input.editor != owner || !matches!(input.key, InputKey::Quantity) {
            continue;
        }
        let mut text = texts.get_mut(entity).unwrap();
        if text.value() != value {
            text.editor.set_text(value);
        }
    }
    let mut preview = previews.get_mut(preview).unwrap();
    if preview.quantity != quantity {
        preview.quantity = quantity;
    }
}
