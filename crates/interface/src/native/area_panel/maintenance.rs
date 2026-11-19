use super::*;
use crate::native::area::editing::Edit;
use bevy::ecs::query::QueryState;

type Pending = (Entity, Entity, Entity, Field, Entity, String);

pub(crate) struct Preparation {
    fields: QueryState<(Entity, &'static AreaField, &'static EditableText)>,
    sliders: QueryState<(Entity, &'static ForceSlider)>,
    edits: Vec<Pending>,
    updates: Vec<(Entity, String)>,
    values: Vec<(Entity, f32, &'static str)>,
}

impl FromWorld for Preparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            fields: world.query(),
            sliders: world.query(),
            edits: Vec::new(),
            updates: Vec::new(),
            values: Vec::new(),
        }
    }
}

fn candidate<'a>(field: Field, area: &InfluenceArea, value: &'a str) -> Option<Edit<'a>> {
    match field {
        Field::Name => Some(Edit::Name(value)),
        Field::Rule(index) => Some(Edit::Rule(index, value)),
        _ => {
            let number = value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())?;
            Some(match field {
                Field::Center(axis) => {
                    let mut center = area.center;
                    center[axis] = number;
                    Edit::Center(center)
                }
                Field::Size(axis) => {
                    let mut size = area.size;
                    size[axis] = number;
                    if area.shape.kind() != ShapeKind::Drawn {
                        size = [number; 2];
                    }
                    Edit::Size(size)
                }
                Field::Radius => Edit::Radius(number),
                Field::Depth => Edit::Depth(number),
                Field::Target(axis) => {
                    let mut offset =
                        (area.target_position() - DVec2::from_array(area.center)).to_array();
                    offset[axis] = number;
                    Edit::Target(AttractionTarget::Point(offset))
                }
                _ => unreachable!(),
            })
        }
    }
}

pub(super) fn refresh(world: &mut World, preparation: &mut Preparation) {
    let _refresh = nucleus::performance::Span::new("area-editor-maintenance");
    let Preparation {
        fields,
        sliders,
        edits,
        updates,
        values,
    } = preparation;
    edits.clear();
    let mut field_count = 0;
    for (entity, field, text) in fields.iter(world) {
        field_count += 1;
        if !text.is_composing()
            && text.pending_paste.is_none()
            && world
                .get::<EditMode>(field.root)
                .is_some_and(|mode| mode.enabled && mode.areas)
            && text.value() != field.observed.as_str()
        {
            edits.push((
                entity,
                field.root,
                field.area,
                field.field,
                field.status,
                text.value().to_string(),
            ));
        }
    }
    nucleus::performance::add("area-editor-inputs-examined", field_count);
    for (entity, root, target, field, status, value) in edits.drain(..) {
        if !owns(world, root, target) {
            continue;
        }
        crate::native::area_mutation::disarm(
            world,
            target,
            "Property changes inactive after an edit. Configured changes resume automatically.",
        );
        let area = world.get::<InfluenceArea>(target).unwrap();
        let edit = candidate(field, area, &value).filter(|edit| edit.valid(area));
        let valid = edit.is_some();
        if let Some(edit) = edit {
            let (center, size) = edit.geometry(area);
            let changed = edit.differs(area);
            if let Edit::Depth(depth) = &edit {
                let mut placement = crate::native::topology::spatial(world, target);
                if placement.depth != Some(*depth) {
                    placement.depth = Some(*depth);
                    world.entity_mut(target).insert(placement);
                }
            }
            if let Some(before) = world
                .get::<crate::native::canvas::CanvasItem>(target)
                .copied()
            {
                let after = crate::native::canvas::CanvasItem {
                    position: DVec2::from_array(center),
                    size: DVec2::from_array(size).as_vec2(),
                };
                if before.position != after.position || before.size != after.size {
                    crate::native::layout::edited(world, target, before, after);
                }
            }
            if changed {
                edit.apply(&mut world.get_mut::<InfluenceArea>(target).unwrap());
            }
        }
        world.get_mut::<AreaField>(entity).unwrap().observed = value;
        let message = if valid { "" } else { "Invalid value" };
        if world
            .get::<Text>(status)
            .is_some_and(|text| text.0 != message)
        {
            message.clone_into(&mut world.get_mut::<Text>(status).unwrap().0);
            nucleus::performance::add("area-editor-status-published", 1);
        }
    }
    updates.clear();
    let focus = (field_count > 0)
        .then(|| world.resource::<bevy::input_focus::InputFocus>().get())
        .flatten();
    for (entity, field, text) in fields.iter(world) {
        if focus == Some(entity)
            || crate::native::record_view::pending_text(text)
            || text.is_composing()
            || text.value() != field.observed.as_str()
            || world
                .get::<Text>(field.status)
                .is_none_or(|status| !status.0.is_empty())
        {
            continue;
        }
        let Some(area) = world.get::<InfluenceArea>(field.area) else {
            continue;
        };
        let value = match field.field {
            Field::Center(axis) => area.center[axis],
            Field::Size(axis) => area.size[axis],
            Field::Radius => area.reach.radius,
            Field::Depth => area.depth,
            Field::Target(axis) => (area.target_position() - DVec2::from_array(area.center))[axis],
            _ => continue,
        };
        if field.observed.parse::<f64>().ok() != Some(value) {
            updates.push((entity, value.to_string()));
            nucleus::performance::add("area-editor-values-formatted", 1);
        }
    }
    for (entity, value) in updates.drain(..) {
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(&value);
        world.get_mut::<AreaField>(entity).unwrap().observed = value;
    }
    values.clear();
    for (entity, slider) in sliders.iter(world) {
        if let Some(area) = world.get::<InfluenceArea>(slider.area) {
            values.push((entity, area.strength as f32, force_status(area)));
        }
    }
    let slider_count = values.len();
    nucleus::performance::add("area-editor-sliders-examined", slider_count as u64);
    for (entity, value, message) in values.drain(..) {
        crate::native::slider::set_value(world, entity, value);
        if let Some(mut tip) = world.get_mut::<crate::native::icons::Tooltip>(entity)
            && tip.0 != message
        {
            message.clone_into(&mut tip.0);
        }
    }
    if field_count == 0 {
        *edits = Vec::new();
        *updates = Vec::new();
    }
    if slider_count == 0 {
        *values = Vec::new();
    }
}
