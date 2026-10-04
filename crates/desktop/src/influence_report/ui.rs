use super::*;
use crate::edit_mode::label;

#[derive(Component)]
struct View {
    root: Entity,
    area: Option<Entity>,
    text: Entity,
    controls: Entity,
    signature: String,
}

fn button(world: &mut World, parent: Entity, root: Entity, action: Control, title: &str) {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::actions::ActionButton::new(root, crate::actions![action]),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_label(title);
    label(world, entity, title, 14.0);
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity, area: Option<Entity>) {
    label(world, parent, "Rules affecting a Sand", 18.0);
    label(
        world,
        parent,
        "Click a Sand in edit mode. This explanation stays available while you adjust its Areas. Pauses stop Area effects, sounds and boundary changes. Submitted Record changes may still finish.",
        14.0,
    );
    let text = label(
        world,
        parent,
        "Select a Sand to inspect its evaluated rules.",
        14.0,
    );
    let controls = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world.entity_mut(controls).insert(View {
        root,
        area,
        text,
        controls,
        signature: String::new(),
    });
}

fn outcome(world: &World, result: &Outcome) -> String {
    match result {
        Outcome::Active => "eligible".into(),
        Outcome::Disabled => "Area disabled".into(),
        Outcome::Paused => "paused".into(),
        Outcome::Filter => "Record does not match the filter".into(),
        Outcome::Reach => "outside influence reach".into(),
        Outcome::Outside => "outside the Area boundary".into(),
        Outcome::Immune(area) => format!("blocked by immunity from {}", name(world, *area)),
        Outcome::Group => "Area and Sand belong to the same group".into(),
        Outcome::Pinned => "Sand pinned to the screen".into(),
    }
}

fn name(world: &World, entity: Entity) -> &str {
    world
        .get::<InfluenceArea>(entity)
        .map_or("removed Area", |area| area.name.as_str())
}

pub(crate) fn describe(
    world: &World,
    root: Entity,
    sand: Entity,
    filter: Option<Entity>,
) -> String {
    let Some(report) = world.get::<Report>(sand) else {
        return "Waiting for rule evaluation.".into();
    };
    let workspace = world
        .get::<WorkspaceMember>(sand)
        .map_or(1, |member| member.0);
    let physics = crate::workspace_config::enabled(world, root, workspace);
    let screen_pin = world.get::<crate::sand_placement::Pinned>(sand).is_some();
    let members = crate::canvas_selection::group_members(world, root, sand);
    let space_pin = crate::topology::spatial(world, sand).world_pinned;
    let group_pin = members.iter().any(|member| {
        *member != sand
            && (world
                .get::<crate::sand_placement::Pinned>(*member)
                .is_some()
                || crate::topology::spatial(world, *member).world_pinned)
    });
    let mut lines = vec![
        "Inspected Sand".into(),
        if screen_pin {
            "Pinned to the screen: forces, sorting, size and boundary changes are suppressed."
                .into()
        } else if space_pin {
            "Position pinned in space: forces cannot move this Sand; size rules still apply.".into()
        } else if group_pin {
            "Another member of this Sand's group is pinned; the group stays fixed.".into()
        } else if members
            .iter()
            .any(|member| crate::physics::held(world, *member))
        {
            "This Sand or its group is being edited or placed; ordinary movement is temporarily held.".into()
        } else if members
            .iter()
            .any(|member| crate::area_mutation::pending(world, *member))
        {
            "A Record change is pending; physics temporarily holds this Sand or its group.".into()
        } else if crate::workspace_config::rules(world, root, workspace).paused {
            "Workspace rules are paused: Area effects and boundary changes are stopped.".into()
        } else if !physics {
            "Workspace physics is off: evaluated forces and sorting do not move this Sand.".into()
        } else {
            "Workspace physics is on.".into()
        },
        format!(
            "Combined force: ({:.2}, {:.2}, {:.2}). Size multiplier: {:.2}.",
            report.total.x, report.total.y, report.total.z, report.scale
        ),
    ];
    let raw: DVec3 = report.entries.iter().map(|entry| entry.force).sum();
    let magnitudes: f64 = report
        .entries
        .iter()
        .map(|entry| entry.force.length())
        .sum();
    if magnitudes > 0.0 && raw.length() < magnitudes * 0.5 {
        lines.push("Some forces oppose one another. Pause an Area to isolate its effect.".into());
    }
    if raw.length() > 1_000_000.0 {
        lines.push("Combined force was limited to 1000000.".into());
    }
    lines.push("Size stays between 0.05 and 20 times its base size. Boundary-change requests and repeated cycles have the limits below.".into());
    for entry in report
        .entries
        .iter()
        .filter(|entry| filter.is_none_or(|area| area == entry.area))
    {
        let Some(area) = world.get::<InfluenceArea>(entry.area) else {
            continue;
        };
        lines.push(format!(
            "{}: {:?} / {:?}: {}. Force ({:.2}, {:.2}, {:.2}).",
            area.name,
            area.direction,
            area.force_mode,
            outcome(world, &entry.motion),
            entry.force.x,
            entry.force.y,
            entry.force.z
        ));
        if !area.attraction_enabled {
            lines.push("Attraction/repulsion is switched off; sorting can still act.".into());
        }
        if let Some(sort) = &area.sorting {
            lines.push(format!(
                "Sorting: {}{}, strength {:.2}, {}.",
                if sort.horizontal {
                    "horizontal"
                } else {
                    "vertical"
                },
                if sort.reverse { ", reversed" } else { "" },
                sort.strength,
                entry.slot.map_or_else(
                    || "no slot for this Sand".into(),
                    |slot| format!("slot at ({:.2}, {:.2}, {:.2})", slot.x, slot.y, slot.z),
                )
            ));
        }
        if area.scale != 1.0 {
            lines.push(format!(
                "Size rule: {}; multiplier {:.2}.",
                outcome(world, &entry.size),
                entry.scale
            ));
        }
        if area.immunity != crate::area_effects::Immunity::None {
            lines.push(format!("Immunity {:?}; boundary {} this Sand. Other rules above name this Area when it blocks them.",
                area.immunity, if entry.inside { "contains" } else { "does not contain" }));
        }
        if !area.changes.is_empty() {
            lines.push(crate::area_mutation::explanation(world, entry.area, sand));
        }
    }
    if report.entries.is_empty() {
        lines.push("No valid Areas are present in this workspace.".into());
    }
    if let Some(error) = world
        .get::<crate::workspace_config::WorkspaceSettings>(root)
        .and_then(|settings| settings.0.get(&workspace))
        .and_then(|s| s.error.as_ref())
    {
        lines.push(error.clone());
    }
    lines.join("\n")
}

pub(crate) fn update(world: &mut World) {
    let views: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .map(|(entity, view)| {
            (
                entity,
                view.root,
                view.area,
                view.text,
                view.controls,
                view.signature.clone(),
            )
        })
        .collect();
    for (entity, root, area, text, controls, old) in views {
        let Some(workspace) = world.get::<Workspaces>(root).map(|spaces| spaces.active) else {
            continue;
        };
        let sand = world
            .get::<Inspected>(root)
            .map(|s| s.0)
            .filter(|sand| crate::canvas_selection::eligible(world, root, *sand));
        let value = sand.map_or_else(
            || "Select a Sand to inspect its evaluated rules.".into(),
            |sand| describe(world, root, sand, area),
        );
        if let Some(mut label) = world.get_mut::<Text>(text) {
            label.set_if_neq(Text(value));
        }
        let rules = crate::workspace_config::rules(world, root, workspace);
        let areas: Vec<_> =
            world
                .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
                .iter(world)
                .filter(|(entity, _, parent, member)| {
                    parent.parent() == root
                        && member.0 == workspace
                        && area.is_none_or(|area| area == *entity)
                        && crate::area_panel::owns(world, root, *entity)
                        && (area.is_some()
                            || sand.and_then(|sand| world.get::<Report>(sand)).is_some_and(
                                |report| report.entries.iter().any(|entry| entry.area == *entity),
                            ))
                })
                .map(|(entity, area, _, _)| (entity, area.name.clone(), area.paused))
                .collect();
        let signature = format!("{workspace}:{rules:?}:{areas:?}");
        if signature == old {
            continue;
        }
        world.entity_mut(controls).despawn_children();
        button(
            world,
            controls,
            root,
            Control::Workspace,
            if rules.paused {
                "Resume workspace rules"
            } else {
                "Pause workspace rules"
            },
        );
        label(
            world,
            controls,
            &format!("In-flight Record changes: {} maximum", rules.max_pending),
            14.0,
        );
        for limit in [8, 32, 64] {
            button(
                world,
                controls,
                root,
                Control::Pending(limit),
                &format!("Allow up to {limit} requests"),
            );
        }
        label(
            world,
            controls,
            &format!(
                "Cycle limit: {} identical crossings per Record in 10 seconds",
                rules.max_repeats
            ),
            14.0,
        );
        for limit in [2, 4, 8] {
            button(
                world,
                controls,
                root,
                Control::Repeats(limit),
                &format!("Pause after {limit} repetitions"),
            );
        }
        for (entity, title, paused) in areas {
            button(
                world,
                controls,
                root,
                Control::Area(entity),
                &format!("{} {title}", if paused { "Resume" } else { "Pause" }),
            );
        }
        world.get_mut::<View>(entity).unwrap().signature = signature;
    }
}
