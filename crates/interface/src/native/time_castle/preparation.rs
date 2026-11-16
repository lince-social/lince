use super::*;
use std::{borrow::Cow, sync::Arc};

// Derived values belong to this clock, not its persisted settings. Compare the
// actual inputs so direct edits, restoration and same-tick changes all work.
#[derive(Component)]
struct SettingsSnapshot {
    value: Arc<Settings>,
    valid: bool,
}

pub(super) fn settings(world: &mut World, owner: Entity) -> Result<(Arc<Settings>, bool), String> {
    let value = &world
        .get::<TimeSettings>(owner)
        .ok_or("Time Castle closed")?
        .0;
    if let Some(previous) = world.get::<SettingsSnapshot>(owner)
        && previous.value.as_ref() == value
        && previous
            .value
            .source
            .as_ref()
            .zip(value.source.as_ref())
            .is_none_or(|(previous, current)| {
                previous.draft.same_preparation_inputs(&current.draft)
            })
    {
        return Ok((previous.value.clone(), previous.valid));
    }
    let _prepare = nucleus::performance::Span::new("time-settings-preparation");
    let valid = value.valid();
    let value = Arc::new(value.clone());
    world.entity_mut(owner).insert(SettingsSnapshot {
        value: value.clone(),
        valid,
    });
    nucleus::performance::add("time-settings-prepared", 1);
    Ok((value, valid))
}

enum Definition<'a> {
    Local(Source),
    Draft(Cow<'a, model::ScheduleSource>),
    Area(Cow<'a, crate::native::protein_area::Config>),
}

impl Definition<'_> {
    fn matches(&self, other: &Definition<'_>) -> bool {
        match (self, other) {
            (Self::Local(left), Definition::Local(right)) => left == right,
            (Self::Draft(left), Definition::Draft(right)) => {
                left.cell == right.cell && left.draft.same_preparation_inputs(&right.draft)
            }
            (Self::Area(left), Definition::Area(right)) => left.same_query_inputs(right),
            _ => false,
        }
    }

    fn into_owned(self) -> Definition<'static> {
        match self {
            Self::Local(source) => Definition::Local(source),
            Self::Draft(source) => Definition::Draft(Cow::Owned(source.into_owned())),
            Self::Area(config) => Definition::Area(Cow::Owned(config.into_owned())),
        }
    }

    fn compile(&self) -> Result<Selection, String> {
        let (source, query) = match self {
            Self::Local(source) => {
                return Ok(Selection {
                    source: source.clone(),
                    filter: Vec::new(),
                    order: default_schedule_order(Vec::new()),
                });
            }
            Self::Draft(source) => (source.cell.clone(), source.query()?),
            Self::Area(config) => (config.source.clone(), config.query()?),
        };
        Ok(Selection {
            source,
            filter: query.filter,
            order: default_schedule_order(query.order),
        })
    }
}

pub(super) struct Selection {
    pub source: Source,
    pub filter: Vec<protein::Predicate>,
    pub order: Vec<protein::Order>,
}

#[derive(Component)]
struct SourceSnapshot {
    definition: Definition<'static>,
    selection: Arc<Selection>,
}

fn prepare(
    world: &World,
    owner: Entity,
    definition: Definition<'_>,
) -> (Result<Arc<Selection>, String>, Option<SourceSnapshot>) {
    if let Some(previous) = world.get::<SourceSnapshot>(owner)
        && previous.definition.matches(&definition)
    {
        return (Ok(previous.selection.clone()), None);
    }
    let _prepare = nucleus::performance::Span::new("time-source-preparation");
    let result = definition.compile().map(Arc::new);
    // Successful Record-query compilation is input-only. A rejected non-Record
    // draft can reach time-dependent rolling-window validation; keep its live
    // error path instead of retaining that diagnostic as an invariant.
    let replacement = result.as_ref().ok().map(|selection| SourceSnapshot {
        definition: definition.into_owned(),
        selection: selection.clone(),
    });
    nucleus::performance::add("time-sources-prepared", 1);
    (result, replacement)
}

pub(super) fn source(
    world: &mut World,
    owner: Entity,
    settings: &Settings,
) -> Result<Arc<Selection>, String> {
    // These are live boundaries, not part of the retained pure compilation.
    let practice = crate::native::practice_cells::source(world, owner);
    let (result, replacement) = if let Some(source) = &settings.source {
        let (result, replacement) = prepare(world, owner, Definition::Draft(Cow::Borrowed(source)));
        // Invalid authored sources precede the practice-source check, as before.
        let result = result.and_then(|selection| {
            if practice.is_some_and(|cell| source.cell != Source::Organ(cell)) {
                Err("Choose a Protein from this practice Cell.".into())
            } else {
                Ok(selection)
            }
        });
        (result, replacement)
    } else if let Some(id) = &settings.area {
        let root = world
            .get::<ChildOf>(owner)
            .ok_or("Workspace missing")?
            .parent();
        let workspace = world
            .get::<crate::native::workspace::WorkspaceMember>(owner)
            .map(|member| member.0);
        let mut areas = world.query::<(
            &crate::native::area::InfluenceArea,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
        )>();
        let config = areas
            .iter(world)
            .find(|(area, parent, member)| {
                area.id == *id && parent.parent() == root && Some(member.0) == workspace
            })
            .and_then(|(area, _, _)| area.protein.as_ref())
            .ok_or("Selected Protein Area is unavailable")?;
        if practice.is_some_and(|cell| config.source != Source::Organ(cell)) {
            return Err("Choose a Protein from this practice Cell.".into());
        }
        prepare(world, owner, Definition::Area(Cow::Borrowed(config)))
    } else {
        prepare(
            world,
            owner,
            Definition::Local(practice.map_or(Source::Local, Source::Organ)),
        )
    };
    if let Some(replacement) = replacement {
        world.entity_mut(owner).insert(replacement);
    }
    result
}
