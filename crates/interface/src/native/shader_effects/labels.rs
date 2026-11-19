use super::*;
use model::{InputMappings, ResolvedInputs};

struct Mapping {
    inputs: InputMappings,
    quantity: Option<String>,
    values: ResolvedInputs,
    text: Arc<str>,
}

fn same_values(first: Option<ResolvedInputs>, second: Option<ResolvedInputs>) -> bool {
    match (first, second) {
        (None, None) => true,
        (Some(first), Some(second)) => {
            first.task == second.task
                && first.progress.map(f32::to_bits) == second.progress.map(f32::to_bits)
        }
        _ => false,
    }
}

impl Mapping {
    fn matches(&self, inputs: &InputMappings, quantity: Option<&str>) -> bool {
        &self.inputs == inputs && self.quantity.as_deref() == quantity
    }

    fn prepare(
        previous: Option<&Arc<Self>>,
        inputs: &InputMappings,
        quantity: Option<&str>,
    ) -> Arc<Self> {
        if let Some(previous) = previous.filter(|previous| previous.matches(inputs, quantity)) {
            return previous.clone();
        }
        let values = inputs.resolve(quantity);
        let text = previous
            .filter(|previous| same_values(Some(previous.values), Some(values)))
            .map_or_else(
                || Arc::from(values.label()),
                |previous| previous.text.clone(),
            );
        Arc::new(Self {
            inputs: inputs.clone(),
            quantity: quantity.map(str::to_owned),
            values,
            text,
        })
    }
}

struct Numeric {
    fields: Vec<(String, String)>,
    text: String,
}

fn numeric_fields(world: &World, preview: Option<Entity>) -> impl Iterator<Item = (&str, &str)> {
    preview
        .into_iter()
        .flat_map(move |preview| numeric::status_fields(world, preview))
}

impl Numeric {
    fn matches(&self, world: &World, preview: Option<Entity>) -> bool {
        numeric_fields(world, preview).eq(self
            .fields
            .iter()
            .map(|(label, value)| (label.as_str(), value.as_str())))
    }

    fn prepare(previous: Option<&Arc<Self>>, world: &World, preview: Option<Entity>) -> Arc<Self> {
        if let Some(previous) = previous.filter(|previous| previous.matches(world, preview)) {
            return previous.clone();
        }
        Arc::new(Self {
            fields: numeric_fields(world, preview)
                .map(|(label, value)| (label.to_owned(), value.to_owned()))
                .collect(),
            text: preview.map_or_else(String::new, |preview| numeric::status(world, preview)),
        })
    }
}

#[derive(Clone, Copy)]
struct Live<'a> {
    quantity: Option<&'a str>,
    values: Option<ResolvedInputs>,
    disabled: bool,
}

struct LiveText {
    quantity: Option<String>,
    values: Option<ResolvedInputs>,
    disabled: bool,
    mapped: Option<Arc<str>>,
    text: String,
}

impl LiveText {
    fn matches(&self, live: Live<'_>) -> bool {
        self.quantity.as_deref() == live.quantity
            && same_values(self.values, live.values)
            && self.disabled == live.disabled
    }

    fn prepare(previous: Option<&Arc<Self>>, live: Live<'_>) -> Arc<Self> {
        if let Some(previous) = previous.filter(|previous| previous.matches(live)) {
            return previous.clone();
        }
        let mapped = live.values.map(|values| {
            previous
                .filter(|previous| same_values(previous.values, live.values))
                .and_then(|previous| previous.mapped.clone())
                .unwrap_or_else(|| Arc::from(values.label()))
        });
        let mut text = String::from("\nLive Sand: quantity ");
        text.push_str(live.quantity.unwrap_or("unavailable"));
        text.push_str("; ");
        text.push_str(mapped.as_deref().unwrap_or("inactive"));
        if live.disabled {
            text.push_str(" · disabled or source unavailable");
        }
        Arc::new(Self {
            quantity: live.quantity.map(str::to_owned),
            values: live.values,
            disabled: live.disabled,
            mapped,
            text,
        })
    }
}

#[derive(Component)]
pub(super) struct EditorText {
    mapping: Arc<Mapping>,
    numeric: Arc<Numeric>,
    live: Option<Arc<LiveText>>,
    reference: &'static str,
    text: Arc<str>,
}

pub(super) fn editor(world: &mut World, owner: Entity) -> Arc<str> {
    let _status = nucleus::performance::Span::new("shader-authoring-status-preparation");
    let editor = world.get::<Editor>(owner).unwrap();
    let target = match editor.target {
        Target::Surface(entity) => Some(entity),
        Target::Record(_) => None,
    };
    let reference = target
        .and_then(|entity| world.get::<Surface>(entity))
        .map(|surface| catalog::reference_status(world, surface))
        .unwrap_or_default();
    let live = target
        .and_then(|entity| world.get::<Background>(entity))
        .and_then(|background| {
            let state = world.get::<shader::EffectPreview>(background.preview)?;
            Some(Live {
                quantity: state.quantity.as_deref(),
                values: shader::lighting_inputs(world, background.preview),
                disabled: state.disabled || disabled(world),
            })
        });
    let previous = world.get::<EditorText>(owner);
    if let Some(previous) = previous.filter(|previous| {
        previous
            .mapping
            .matches(&editor.effect.inputs, editor.quantity.as_deref())
            && previous.numeric.matches(world, Some(editor.preview))
            && match (previous.live.as_ref(), live) {
                (None, None) => true,
                (Some(previous), Some(live)) => previous.matches(live),
                _ => false,
            }
            && previous.reference == reference
    }) {
        return previous.text.clone();
    }
    let mapping = Mapping::prepare(
        previous.map(|previous| &previous.mapping),
        &editor.effect.inputs,
        editor.quantity.as_deref(),
    );
    let numeric = Numeric::prepare(
        previous.map(|previous| &previous.numeric),
        world,
        Some(editor.preview),
    );
    let live = live
        .map(|live| LiveText::prepare(previous.and_then(|previous| previous.live.as_ref()), live));
    let mut text = mapping.text.to_string();
    if let Some(live) = &live {
        text.push_str(&live.text);
    }
    append(&mut text, &numeric.text);
    append(&mut text, reference);
    let text: Arc<str> = text.into();
    world.entity_mut(owner).insert(EditorText {
        mapping,
        numeric,
        live,
        reference,
        text: text.clone(),
    });
    nucleus::performance::add("shader-status-labels-built", 1);
    text
}

#[derive(Component)]
pub(super) struct FloatingText {
    mapping: Option<Arc<Mapping>>,
    numeric: Arc<Numeric>,
    unavailable: bool,
    frozen: bool,
    text: Arc<str>,
}

pub(super) fn floating(
    world: &World,
    owner: Entity,
    preview: Entity,
    selected: Option<&Effect>,
    quantity: Option<&str>,
    frozen: bool,
) -> (Arc<str>, Option<FloatingText>) {
    let _status = nucleus::performance::Span::new("shader-floating-status-preparation");
    let inputs = selected
        .and_then(|selected| shader::working(world, preview).or(Some(selected)))
        .map(|effect| &effect.inputs)
        .filter(|inputs| inputs.task.is_some() || inputs.progress.is_some());
    let numeric_preview = selected.map(|_| preview);
    let unavailable = selected.is_none();
    let previous = world.get::<FloatingText>(owner);
    if let Some(previous) = previous.filter(|previous| {
        previous.unavailable == unavailable
            && previous.frozen == frozen
            && match (previous.mapping.as_ref(), inputs) {
                (None, None) => true,
                (Some(previous), Some(inputs)) => previous.matches(inputs, quantity),
                _ => false,
            }
            && previous.numeric.matches(world, numeric_preview)
    }) {
        return (previous.text.clone(), None);
    }
    let mapping = inputs.map(|inputs| {
        Mapping::prepare(
            previous.and_then(|previous| previous.mapping.as_ref()),
            inputs,
            quantity,
        )
    });
    let numeric = Numeric::prepare(
        previous.map(|previous| &previous.numeric),
        world,
        numeric_preview,
    );
    let mut text = if unavailable {
        "Selected effect unavailable".into()
    } else {
        mapping
            .as_ref()
            .map_or_else(String::new, |mapping| mapping.text.to_string())
    };
    append(&mut text, &numeric.text);
    if frozen {
        append(&mut text, "Effect snapshot frozen");
    }
    let text: Arc<str> = text.into();
    let next = FloatingText {
        mapping,
        numeric,
        unavailable,
        frozen,
        text: text.clone(),
    };
    nucleus::performance::add("shader-status-labels-built", 1);
    (text, Some(next))
}

fn append(text: &mut String, suffix: &str) {
    if !suffix.is_empty() {
        if !text.is_empty() {
            text.push_str(" · ");
        }
        text.push_str(suffix);
    }
}
