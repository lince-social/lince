use super::*;
use crate::native::area_effects::{ForceMode, Immunity, Sorting};
use engine::area_transition::RecordChanges;

pub(crate) enum Edit<'a> {
    Name(&'a str),
    Rule(usize, &'a str),
    Center([f64; 2]),
    Size([f64; 2]),
    Radius(f64),
    Depth(f64),
    Target(AttractionTarget),
    Strength(f64),
    Sorting(Option<Sorting>),
    Scale(f32),
    Immunity(Immunity),
    ForceMode(ForceMode),
    Crossing { enter: bool, changes: RecordChanges },
}

impl Edit<'_> {
    pub(crate) fn valid(&self, area: &InfluenceArea) -> bool {
        nucleus::performance::add("area-edit-proposals", 1);
        valid(area, Some(self))
    }

    pub(crate) fn differs(&self, area: &InfluenceArea) -> bool {
        match self {
            Self::Name(value) => area.name != *value,
            Self::Rule(index, value) => area
                .rules
                .get(*index)
                .is_none_or(|rule| rule.value != *value),
            Self::Center(value) => area.center != *value,
            Self::Size(value) => area.size != *value,
            Self::Radius(value) => area.reach.radius != *value,
            Self::Depth(value) => area.depth != *value,
            Self::Target(value) => area.target != *value,
            Self::Strength(value) => area.strength != *value,
            Self::Sorting(value) => area.sorting != *value,
            Self::Scale(value) => area.scale != *value,
            Self::Immunity(value) => area.immunity != *value,
            Self::ForceMode(value) => area.force_mode != *value,
            Self::Crossing { enter, changes } => {
                (if *enter {
                    &area.changes.enter
                } else {
                    &area.changes.leave
                }) != changes
            }
        }
    }

    pub(crate) fn geometry(&self, area: &InfluenceArea) -> ([f64; 2], [f64; 2]) {
        match self {
            Self::Center(value) => (*value, area.size),
            Self::Size(value) => (area.center, *value),
            _ => (area.center, area.size),
        }
    }

    pub(crate) fn apply(self, area: &mut InfluenceArea) {
        match self {
            Self::Name(value) => value.clone_into(&mut area.name),
            Self::Rule(index, value) => value.clone_into(&mut area.rules[index].value),
            Self::Center(value) => area.center = value,
            Self::Size(value) => area.size = value,
            Self::Radius(value) => area.reach.radius = value,
            Self::Depth(value) => area.depth = value,
            Self::Target(value) => area.target = value,
            Self::Strength(value) => area.strength = value,
            Self::Sorting(value) => area.sorting = value,
            Self::Scale(value) => area.scale = value,
            Self::Immunity(value) => area.immunity = value,
            Self::ForceMode(value) => area.force_mode = value,
            Self::Crossing { enter, changes } => {
                let target = if enter {
                    &mut area.changes.enter
                } else {
                    &mut area.changes.leave
                };
                *target = changes;
            }
        }
        nucleus::performance::add("area-edits-published", 1);
    }
}

pub(super) fn valid(area: &InfluenceArea, edit: Option<&Edit<'_>>) -> bool {
    let (
        mut name,
        mut center,
        mut size,
        mut depth,
        mut target,
        mut strength,
        mut radius,
        mut sorting,
        mut scale,
        mut enter,
        mut leave,
    ) = (
        area.name.as_str(),
        area.center,
        area.size,
        area.depth,
        area.target,
        area.strength,
        area.reach.radius,
        &area.sorting,
        area.scale,
        &area.changes.enter,
        &area.changes.leave,
    );
    let mut rule_override = None;
    if let Some(edit) = edit {
        match edit {
            Edit::Name(value) => name = value,
            Edit::Rule(index, value) => {
                if *index >= area.rules.len() {
                    return false;
                }
                rule_override = Some((*index, *value));
            }
            Edit::Center(value) => center = *value,
            Edit::Size(value) => size = *value,
            Edit::Radius(value) => radius = *value,
            Edit::Depth(value) => depth = *value,
            Edit::Target(value) => target = *value,
            Edit::Strength(value) => strength = *value,
            Edit::Sorting(value) => sorting = value,
            Edit::Scale(value) => scale = *value,
            Edit::Crossing {
                enter: entering,
                changes,
            } => {
                if *entering {
                    enter = changes;
                } else {
                    leave = changes;
                }
            }
            Edit::Immunity(_) | Edit::ForceMode(_) => {}
        }
    }
    let _timing =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeAreaValidation);
    let size_vector = DVec2::from_array(size);
    area.records.len() <= 100_000
        && area
            .records
            .iter()
            .all(|(uid, state)| !uid.is_empty() && uid.len() <= 128 && state.valid())
        && area.id.len() == 32
        && area.id.bytes().all(|byte| byte.is_ascii_hexdigit())
        && !name.trim().is_empty()
        && name.chars().take(81).count() <= 80
        && area.opacity.is_finite()
        && (0.0..=1.0).contains(&area.opacity)
        && DVec2::from_array(center).is_finite()
        && size_vector.is_finite()
        && size_vector.min_element() >= 1.0
        && size_vector.max_element() <= 100_000.0
        && depth.is_finite()
        && (1.0..=100_000.0).contains(&depth)
        && match target {
            AttractionTarget::Center => true,
            AttractionTarget::Point(offset) => {
                DVec2::from_array(offset).is_finite()
                    && (DVec2::from_array(center) + DVec2::from_array(offset)).is_finite()
            }
        }
        && (0.0..=1_000_000.0).contains(&strength)
        && radius.is_finite()
        && (0.0..=100_000.0).contains(&radius)
        && area.rules.len() <= MAX_RULES
        && area.rules.iter().enumerate().all(|(index, rule)| {
            rule_override
                .filter(|(slot, _)| *slot == index)
                .map_or_else(
                    || rule.validate(),
                    |(_, value)| rule.property.valid_value(value),
                )
        })
        && area
            .sound
            .as_ref()
            .is_none_or(crate::native::sound_area::SoundArea::valid)
        && enter.validate()
        && leave.validate()
        && area.creation.valid()
        && area
            .change_filter
            .as_ref()
            .is_none_or(crate::native::protein_area::Config::valid)
        && scale.is_finite()
        && (0.05..=20.0).contains(&scale)
        && sorting.as_ref().is_none_or(|sort| {
            sort.strength.is_finite() && (0.0..=1_000_000.0).contains(&sort.strength)
        })
        && area
            .protein
            .as_ref()
            .is_none_or(crate::native::protein_area::Config::valid)
        && area
            .filter
            .as_ref()
            .is_none_or(crate::native::protein_area::Config::valid)
        && match &area.shape {
            AreaShape::Square | AreaShape::Circle => size[0] == size[1],
            AreaShape::Polygon(points) => valid_polygon(points),
        }
}
