use super::InfluenceArea;
use crate::native::{
    actions::Action,
    assertion_editor,
    canvas::CanvasItem,
    protein_area::{RecordBinding, Source},
    sand_panel as panel, topology,
    workspace::WorkspaceMember,
};
use bevy::prelude::*;
use engine::record_creation::{Assertion, Draft};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub file_origin: Option<String>,
    pub assertions: Vec<Assertion>,
}

impl Defaults {
    pub(crate) fn valid(&self) -> bool {
        self.file_origin
            .as_deref()
            .is_none_or(nucleus::sync::valid_file_origin)
            && self.assertions.len() <= 128
            && self.assertions.iter().all(|item| {
                !item.predicate.trim().is_empty()
                    && item.predicate.len() <= 4096
                    && [&item.object, &item.quantity, &item.unit]
                        .into_iter()
                        .all(|value| value.as_ref().is_none_or(|text| text.len() <= 4096))
            })
    }

    fn is_empty(&self) -> bool {
        self.file_origin.is_none() && self.assertions.is_empty()
    }
}

#[derive(Clone)]
struct Save {
    origin: Entity,
    assertions: Entity,
    status: Entity,
}

impl Action for Save {
    fn apply(&self, world: &mut World, area: Entity) {
        let result = panel::value(world, self.origin).and_then(|value| {
            let value = value.trim();
            Ok(Defaults {
                file_origin: (!value.is_empty()).then(|| value.to_owned()),
                assertions: assertion_editor::draft(world, self.assertions)?,
            })
        });
        match result {
            Ok(defaults) if defaults.valid() => {
                if let Some(mut current) = world.get_mut::<InfluenceArea>(area)
                    && current.creation != defaults
                {
                    current.creation = defaults;
                }
                panel::status(world, self.status, "Creation defaults saved");
            }
            Ok(_) => panel::status(
                world,
                self.status,
                "Use a file origin from Sync Castle and valid Assertions",
            ),
            Err(error) => panel::status(world, self.status, error),
        }
    }
}

pub(crate) fn controls(world: &mut World, parent: Entity, area: Entity) {
    let Some(defaults) = world
        .get::<InfluenceArea>(area)
        .map(|area| area.creation.clone())
    else {
        return;
    };
    let origin = panel::field(
        world,
        parent,
        "File origin for new Records",
        defaults.file_origin.as_deref().unwrap_or(""),
    );
    world.entity_mut(origin).insert(crate::native::icons::Tooltip(
        "Enter the origin shown in Sync Castle. Use personal for the personal folder, or leave blank to inherit from another Area.".to_owned(),
    ));
    let assertions = panel::column(world, parent);
    let values: Vec<_> = defaults
        .assertions
        .iter()
        .map(|assertion| {
            json!({"uid": nucleus::new_uid("a"), "predicate": assertion.predicate,
            "object": assertion.object, "quantity": assertion.quantity, "unit": assertion.unit})
        })
        .collect();
    assertion_editor::spawn(
        world,
        assertions,
        RecordBinding {
            area,
            uid: String::new(),
            source: Source::Local,
        },
        &json!({"assertions": values}),
        true,
    );
    let status = crate::native::edit_mode::label(world, parent, "", 13.0);
    panel::button(
        world,
        parent,
        area,
        "Save creation defaults",
        Save {
            origin,
            assertions,
            status,
        },
    );
}

pub(crate) fn apply(world: &mut World, form: Entity, draft: &mut Draft) -> Result<(), String> {
    let Some((root, workspace, point)) = world
        .get::<ChildOf>(form)
        .zip(world.get::<WorkspaceMember>(form))
        .zip(topology::position(world, form))
        .map(|((parent, member), point)| (parent.parent(), member.0, point))
    else {
        return Ok(());
    };
    let mut areas = world.query::<(
        Entity,
        &InfluenceArea,
        &ChildOf,
        &WorkspaceMember,
        &CanvasItem,
    )>();
    let mut matching: Vec<_> = areas
        .iter(world)
        .filter(|(_, area, parent, member, _)| {
            parent.parent() == root
                && member.0 == workspace
                && area.enabled
                && !area.paused
                && !area.creation.is_empty()
        })
        .filter(|(entity, area, _, _, _)| {
            topology::influence::contains(area, topology::spatial(world, *entity), point)
        })
        .map(|(_, area, _, _, _)| area)
        .collect();
    matching.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    for area in matching {
        if let Some(origin) = &area.creation.file_origin {
            if draft
                .file_origin
                .as_ref()
                .is_some_and(|current| current != origin)
            {
                return Err("This Record is inside Areas with different file origins. Move it into one of those Areas to create it.".into());
            }
            draft.file_origin = Some(origin.clone());
        }
        for assertion in &area.creation.assertions {
            if !draft.assertions.contains(assertion) {
                draft.assertions.push(assertion.clone());
            }
        }
    }
    Ok(())
}
