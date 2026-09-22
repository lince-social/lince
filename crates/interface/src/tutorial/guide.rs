use super::*;
use crate::{
    area::{Direction, Property, ReachMode, ShapeKind},
    area_panel::AreaAction,
    edit_mode::EditAction,
};

#[derive(Clone, PartialEq)]
pub(super) enum Target {
    Edit(EditAction),
    Button(Entity, &'static str),
    Menu(Entity, &'static str, &'static str),
    Field(TutorialField),
    PanelField(&'static str),
    Control(&'static str),
    Canvas(Vec<Entity>),
    None,
}

#[derive(Clone, PartialEq)]
pub(super) struct Instruction {
    pub text: String,
    pub done: bool,
    pub target: Target,
}

#[derive(Component)]
pub(super) struct Guide {
    pub container: Entity,
    pub instructions: Vec<Instruction>,
    pub current: Option<Entity>,
}

fn add(list: &mut Vec<Instruction>, text: impl Into<String>, done: bool, target: Target) {
    list.push(Instruction {
        text: text.into(),
        done,
        target,
    });
}

fn contains(value: &serde_json::Value, key: &str, expected: Option<&str>) -> bool {
    match value {
        serde_json::Value::Object(values) => values.iter().any(|(name, value)| {
            (name == key && expected.is_none_or(|expected| value.as_str() == Some(expected)))
                || contains(value, key, expected)
        }),
        serde_json::Value::Array(values) => {
            values.iter().any(|value| contains(value, key, expected))
        }
        _ => false,
    }
}

fn has_condition(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(values) => values.iter().any(|(key, value)| {
            !matches!(key.as_str(), "all" | "any" | "not") || has_condition(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(has_condition),
        _ => false,
    }
}

fn navigation(target: &Target) -> bool {
    matches!(
        target,
        Target::Control(_)
            | Target::Edit(
                EditAction::Open
                    | EditAction::General
                    | EditAction::Areas
                    | EditAction::Area(AreaAction::Select(_))
            )
    )
}

pub(super) fn instructions(world: &mut World, root: Entity, verified: bool) -> Vec<Instruction> {
    let session = world.get::<Session>(root).unwrap();
    let (step, prefix, spawn, force, change, workspace, entered, completed) = (
        session.step,
        session.sample_prefix.clone(),
        session.spawn,
        session.force,
        session.change,
        session.workspace,
        session.entered,
        session.completed,
    );
    let mut list = Vec::new();
    if completed {
        add(
            &mut list,
            "All five lessons are complete. Your areas remain available in this workspace.",
            true,
            Target::None,
        );
        add(
            &mut list,
            "Close the tutorial.",
            false,
            Target::Button(root, "Close tutorial"),
        );
        return list;
    }
    add(
        &mut list,
        "Prepare two sample Records in the local Organ.",
        session.records.len() == 2,
        if session.error.is_some() {
            Target::Button(root, "Retry connection")
        } else {
            Target::None
        },
    );
    let mode = world.get::<crate::edit_mode::EditMode>(root);
    let editing = mode.is_some_and(|mode| mode.enabled);
    let areas_open = mode.is_some_and(|mode| mode.enabled && mode.areas);
    let toolbar_visible =
        !super::highlight::targets(world, root, &Target::Edit(EditAction::Open)).is_empty();
    add(
        &mut list,
        "Move the pointer to the bottom-right corner to show the controls, or focus Show controls with Tab.",
        editing || toolbar_visible,
        Target::Control("Show controls"),
    );
    add(
        &mut list,
        "Open Edit mode.",
        editing,
        Target::Edit(EditAction::Open),
    );
    if matches!(step, 1..=3) {
        let enabled = crate::workspace_config::enabled(world, root, workspace);
        let ready = enabled == (step < 3);
        add(
            &mut list,
            "Open General to change workspace physics.",
            ready
                || !super::highlight::targets(
                    world,
                    root,
                    &Target::Edit(EditAction::TogglePhysics),
                )
                .is_empty(),
            Target::Edit(EditAction::General),
        );
        add(
            &mut list,
            if step < 3 {
                "Turn Physics on."
            } else {
                "Turn Physics off so the cards stay where you drag them."
            },
            ready,
            Target::Edit(EditAction::TogglePhysics),
        );
    }
    add(
        &mut list,
        "Open Areas of influence.",
        areas_open,
        Target::Edit(EditAction::Areas),
    );
    let area_entity = match step {
        0 => spawn,
        1 | 2 => force,
        _ => change,
    };
    let area = owned(world, root, area_entity).cloned();
    add(
        &mut list,
        if step == 0 {
            "Add a square for Protein spawning."
        } else if step < 3 {
            "Add a separate circle for attraction and repulsion."
        } else {
            "Add a separate square for property changes."
        },
        area.is_some(),
        Target::Edit(EditAction::Area(AreaAction::Add(
            if matches!(step, 1 | 2) {
                ShapeKind::Circle
            } else {
                ShapeKind::Square
            },
        ))),
    );
    let selected = world
        .get::<crate::area_panel::AreaEditor>(root)
        .and_then(|editor| editor.selected);
    if let Some(entity) = area_entity.filter(|_| area.is_some()) {
        add(
            &mut list,
            format!("Select {} in the Areas panel.", area.as_ref().unwrap().name),
            selected == Some(entity),
            Target::Edit(EditAction::Area(AreaAction::Select(entity))),
        );
    }
    let entity = area_entity.unwrap_or(Entity::PLACEHOLDER);
    if step == 0 {
        let config = area.as_ref().and_then(|area| area.protein.as_ref());
        add(
            &mut list,
            "In Protein, click + to make this a Protein Area.",
            config.is_some(),
            Target::Button(entity, "Make this a Protein Area"),
        );
        add(
            &mut list,
            "Choose Local as the data source.",
            config.is_some_and(|config| config.source == crate::protein_area::Source::Local),
            Target::Menu(entity, "Data source", "Local"),
        );
        let editor = world
            .query::<(Entity, &crate::protein_area::QueryEditor)>()
            .iter(world)
            .find(|(_, link)| link.0 == entity)
            .map(|(editor, _)| editor);
        let query = config
            .map(|config| config.draft.query.clone())
            .unwrap_or_default();
        let filtered = contains(&query["where"], "text_contains", Some(&prefix));
        let editor_id = editor.unwrap_or(Entity::PLACEHOLDER);
        add(
            &mut list,
            "Click the Protein pencil to open its query Castle.",
            editor.is_some() || (filtered && config.is_some_and(|config| config.enabled)),
            Target::Button(
                entity,
                "Edit the query in a Protein Castle; changes return to this Area",
            ),
        );
        add(
            &mut list,
            "Under Filters, add a condition (+).",
            has_condition(&query["where"]),
            Target::Button(editor_id, "Add a condition"),
        );
        add(
            &mut list,
            "Choose Text contains for the condition.",
            contains(&query["where"], "text_contains", None),
            Target::Menu(editor_id, "Condition", "Text contains"),
        );
        let query_field = world.query::<&TutorialField>().iter(world).find(|field| matches!(field, TutorialField::Query(owner, path) if *owner == editor_id && path.ends_with("/text_contains"))).cloned();
        add(
            &mut list,
            format!(
                "Enter {prefix} in Text contains. Use Copy lesson text, then paste into the highlighted field."
            ),
            filtered,
            query_field.map_or(Target::None, Target::Field),
        );
        add(
            &mut list,
            "Click Run in the query Castle.",
            filtered && config.is_some_and(|config| config.enabled),
            Target::Button(editor_id, "Run this query and keep results live"),
        );
        for (property, title) in [
            ("head", "Title"),
            ("body", "Description"),
            ("quantity", "Quantity"),
        ] {
            add(
                &mut list,
                format!("In Row template, add {title} through Add property (+)."),
                config.is_some_and(|config| {
                    config
                        .bindings
                        .iter()
                        .any(|binding| binding.property == property)
                }),
                Target::Menu(entity, "Add property", title),
            );
        }
        add(
            &mut list,
            "Wait for both sample cards, with Title, Description and Quantity. If the query returns other Records, correct its filter.",
            verified,
            Target::Button(
                entity,
                "Edit the query in a Protein Castle; changes return to this Area",
            ),
        );
    } else {
        let matching = area
            .as_ref()
            .is_some_and(|area| observation::matching(world, root, area).is_ok());
        let rules = area
            .as_ref()
            .map(|area| area.rules.as_slice())
            .unwrap_or_default();
        add(
            &mut list,
            if rules.len() > 1 {
                "Remove extra Filter properties; keep only the sample 1 rule."
            } else {
                "Under Filter, add one property."
            },
            rules.len() == 1,
            Target::Edit(EditAction::Area(if rules.len() > 1 {
                AreaAction::RemoveRule(rules.len() - 1)
            } else {
                AreaAction::AddRule
            })),
        );
        add(
            &mut list,
            "Choose Title for the Filter property.",
            matching
                || rules
                    .first()
                    .is_some_and(|rule| rule.property == Property::Title),
            Target::Edit(EditAction::Area(AreaAction::Property(0, Property::Title))),
        );
        add(
            &mut list,
            format!("In Equals, enter {prefix} 1. Use Copy sample 1 title to paste it."),
            matching,
            Target::PanelField("Equals"),
        );
        if step < 3 {
            add(
                &mut list,
                "Enable Attraction.",
                area.as_ref().is_some_and(|area| area.attraction_enabled),
                Target::Edit(EditAction::Area(AreaAction::AttractionEnabled)),
            );
            add(
                &mut list,
                "Raise Strength above zero, for example to 100.",
                area.as_ref().is_some_and(|area| area.strength > 0.0),
                Target::PanelField("Area force strength"),
            );
            let direction = if step == 1 {
                Direction::Attract
            } else {
                Direction::Repel
            };
            add(
                &mut list,
                if step == 1 {
                    "Choose Attract."
                } else {
                    "Choose Repel."
                },
                area.as_ref()
                    .is_some_and(|area| area.direction == direction),
                Target::Edit(EditAction::Area(AreaAction::Direction(direction))),
            );
            add(
                &mut list,
                "Choose Unlimited reach.",
                area.as_ref()
                    .is_some_and(|area| area.reach.mode == ReachMode::Unlimited),
                Target::Edit(EditAction::Area(AreaAction::Reach(ReachMode::Unlimited))),
            );
        } else {
            add(
                &mut list,
                "Under On entry, set Quantity to 1.",
                area.as_ref()
                    .is_some_and(|area| area.changes.enter.quantity.as_deref() == Some("1")),
                Target::Field(TutorialField::Quantity(entity, true)),
            );
            add(
                &mut list,
                "Under On exit, set Quantity to 0.",
                area.as_ref()
                    .is_some_and(|area| area.changes.leave.quantity.as_deref() == Some("0")),
                Target::Field(TutorialField::Quantity(entity, false)),
            );
            let enabled = area.as_ref().is_some_and(|area| area.changes_enabled);
            add(
                &mut list,
                if enabled {
                    "Keep Change properties enabled. If changes paused after an error, switch it off, then on."
                } else {
                    "Switch Change properties on."
                },
                crate::area_mutation::armed(world, entity),
                Target::Edit(EditAction::Area(AreaAction::ChangesEnabled)),
            );
        }
        let sample_uid = world.get::<Session>(root).unwrap().records.first().cloned();
        let sample = rows(world, root)
            .into_iter()
            .find(|(_, uid, _, _)| Some(uid) == sample_uid.as_ref());
        let pending = sample
            .as_ref()
            .is_some_and(|(entity, _, _, _)| crate::area_mutation::pending(world, *entity));
        let inside = sample.as_ref().is_some_and(|(entity, _, _, _)| {
            area.as_ref().is_some_and(|area| {
                crate::topology::position(world, *entity).is_some_and(|point| {
                    crate::topology::influence::contains(
                        area,
                        crate::topology::spatial(world, area_entity.unwrap()),
                        point,
                    )
                })
            })
        });
        let text = if pending {
            "Wait for the Organ to save this crossing. Keep the card in place."
        } else if step < 3 {
            "Watch the highlighted sample and force area. If it is not moving, unpin the card and move it away from the area's center."
        } else if step == 3 && inside && !entered {
            "Drag sample 1 outside the highlighted square first. Wait for the save, then drag it back inside."
        } else if step == 3 {
            "Drag sample 1 by its edge into the highlighted square. Its center must cross the outline. Wait for Quantity 1."
        } else {
            "Drag sample 1 outside the highlighted square. Its center must cross the outline. Wait for Quantity 0."
        };
        let mut targets = Vec::new();
        if let Some((sample, _, _, _)) = sample {
            targets.push(sample);
        }
        if area.is_some() {
            targets.push(entity);
        }
        add(&mut list, text, verified, Target::Canvas(targets));
    }
    let configured = list.iter().all(|instruction| {
        instruction.done
            || navigation(&instruction.target)
            || matches!(instruction.target, Target::Canvas(_))
    });
    if verified || configured {
        for instruction in &mut list {
            if verified || navigation(&instruction.target) {
                instruction.done = true;
            }
        }
    }
    add(
        &mut list,
        if step == 4 {
            "Finish this tutorial."
        } else {
            "Continue to the next lesson."
        },
        false,
        Target::Button(root, if step == 4 { "Finish" } else { "Next" }),
    );
    list
}

pub(super) fn update(world: &mut World, root: Entity, verified: bool) {
    let instructions = instructions(world, root, verified);
    let current = instructions
        .iter()
        .position(|instruction| !instruction.done);
    let target = current
        .map(|index| instructions[index].target.clone())
        .unwrap_or(Target::None);
    let changed = world
        .get::<Guide>(root)
        .is_some_and(|guide| guide.instructions != instructions);
    if changed {
        super::view::checklist(world, root, &instructions, current);
        world.get_mut::<Guide>(root).unwrap().instructions = instructions;
    }
    super::highlight::update(world, root, &target);
    if changed {
        super::highlight::reveal_current(world, root);
    }
}
