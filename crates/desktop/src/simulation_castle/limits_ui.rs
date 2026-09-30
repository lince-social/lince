use bevy::prelude::*;
use bevy::text::EditableText;

use super::{SimulationCastle, View};

#[derive(Component)]
struct Input {
    owner: Entity,
    wall_time: bool,
}

#[derive(Clone, Copy)]
struct Apply;

impl crate::actions::Action for Apply {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        super::capture(world);
        let status = world.get::<View>(owner).unwrap().status;
        if let Err(error) = apply(world, owner) {
            world.get_mut::<Text>(status).unwrap().0 = error.to_string();
        }
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let panel = world.get::<View>(owner).unwrap().limits;
    if let Some(children) = world.get::<Children>(panel) {
        let children: Vec<_> = children.iter().collect();
        for child in children {
            world.entity_mut(child).despawn();
        }
    }
    let Ok(scenario) = serde_json::from_str::<simulation::scenario::Scenario>(
        &world.get::<SimulationCastle>(owner).unwrap().scenario,
    ) else {
        return;
    };
    let line = super::row(world, panel);
    for (label, wall_time, value) in [
        (
            "Rule evaluations",
            false,
            scenario.limits.rule_evaluations.to_string(),
        ),
        (
            "Real running milliseconds (empty = no time limit)",
            true,
            scenario
                .limits
                .wall_time_ms
                .map_or_else(String::new, |value| value.to_string()),
        ),
    ] {
        crate::edit_mode::label(world, line, label, 13.0);
        let input = world
            .spawn(crate::sand::text_editor(
                &value,
                world.resource::<crate::theme::Typography>(),
                0,
            ))
            .insert((
                ChildOf(line),
                Input { owner, wall_time },
                Node {
                    width: px(200),
                    flex_shrink: 0.0,
                    ..default()
                },
            ))
            .id();
        let mut text = world.get_mut::<EditableText>(input).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(20);
    }
    crate::castle_feed::button(world, line, owner, "Set limits", Apply);
    crate::edit_mode::label(
        world,
        panel,
        "Limits also stop feedback inside a Rule chain. A limit reports the covered time; it does not prove a cycle. Paused time is excluded.",
        13.0,
    );
}

fn apply(world: &mut World, owner: Entity) -> simulation::Result<()> {
    let view = world
        .get::<View>(owner)
        .ok_or("Simulation controls unavailable")?;
    if view.task.is_some() {
        return Err("Set limits before starting the run".into());
    }
    let editor = view.editor;
    let status = view.status;
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.wall_time, text.value().to_string()))
        .collect();
    let mut scenario: simulation::scenario::Scenario =
        serde_json::from_str(&world.get::<SimulationCastle>(owner).unwrap().scenario)?;
    for (wall_time, value) in values {
        if wall_time {
            scenario.limits.wall_time_ms = if value.trim().is_empty() {
                None
            } else {
                Some(value.trim().parse()?)
            };
        } else {
            scenario.limits.rule_evaluations = value.trim().parse()?;
        }
    }
    scenario.validate()?;
    let source = serde_json::to_string_pretty(&scenario)?;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text(&source);
    world.get_mut::<SimulationCastle>(owner).unwrap().scenario = source;
    world.get_mut::<Text>(status).unwrap().0 = "Limits saved in this setup".into();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;

    #[test]
    fn native_limits_edit_the_setup_and_reject_invalid_limits_without_starting_a_run() {
        let (mut app, owner) = super::super::tests::fixture(SimulationCastle::default());
        let inputs: Vec<_> = app
            .world_mut()
            .query::<(Entity, &Input)>()
            .iter(app.world())
            .map(|(entity, input)| (entity, input.wall_time))
            .collect();
        for (entity, wall_time) in &inputs {
            app.world_mut()
                .get_mut::<EditableText>(*entity)
                .unwrap()
                .editor
                .set_text(if *wall_time { "60000" } else { "500" });
        }
        Apply.apply(app.world_mut(), owner);
        let scenario: simulation::scenario::Scenario =
            serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                .unwrap();
        assert_eq!(scenario.limits.rule_evaluations, 500);
        assert_eq!(scenario.limits.wall_time_ms, Some(60000));
        assert!(app.world().get::<View>(owner).unwrap().task.is_none());
        for (entity, wall_time) in inputs {
            if !wall_time {
                app.world_mut()
                    .get_mut::<EditableText>(entity)
                    .unwrap()
                    .editor
                    .set_text("0");
            }
        }
        Apply.apply(app.world_mut(), owner);
        let unchanged: simulation::scenario::Scenario =
            serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                .unwrap();
        assert_eq!(unchanged.limits.rule_evaluations, 500);
        assert!(app.world().get::<View>(owner).unwrap().task.is_none());
    }
}
