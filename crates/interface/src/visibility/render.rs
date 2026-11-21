mod preview;

use bevy::prelude::*;
use nucleus::visibility::Data;

use crate::{controls, theme::Typography};

use super::{
    Button, Intent, Panel,
    commands::apply,
    draft::{
        BoundEnd::{Lower, Upper},
        Side::{Exclude, Include},
        read_fields,
    },
};

fn label(world: &mut World, parent: Entity, caption: impl Into<String>) {
    let font = world.resource::<Typography>().text(15.0);
    world.spawn((
        ChildOf(parent),
        Text::new(caption),
        font,
        crate::style::text(crate::tokens::Token::Ink),
    ));
}

fn button(world: &mut World, parent: Entity, panel: Entity, caption: &str, intent: Intent) {
    let entity = world
        .spawn((
            ChildOf(parent),
            controls::button(0),
            Button { panel, intent },
            Node {
                min_height: px(36),
                padding: UiRect::all(px(6)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::style::border(crate::tokens::Token::Accent),
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(caption);
    }
    label(world, entity, caption);
    world.entity_mut(entity).observe(
        |activate: On<bevy::ui_widgets::Activate>,
         buttons: Query<&Button>,
         mut commands: Commands| {
            if let Ok(button) = buttons.get(activate.entity) {
                let button = button.clone();
                commands.queue(move |world: &mut World| apply(world, button.panel, button.intent));
            }
        },
    );
}

fn input(world: &mut World, parent: Entity, caption: &str, value: &str) -> Entity {
    label(world, parent, caption);
    let bundle = controls::single_line_editor(value, world.resource::<Typography>(), 0, 10);
    let mut accessibility = accesskit::Node::new(accesskit::Role::TextInput);
    accessibility.set_label(caption);
    accessibility.set_value(value);
    world
        .spawn((
            ChildOf(parent),
            bundle,
            controls::Editor {
                title: caption.into(),
                secret: false,
            },
            bevy::a11y::AccessibilityNode::from(accessibility),
        ))
        .id()
}

pub(super) fn draw(world: &mut World, entity: Entity) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    read_fields(world, &mut panel);
    controls::clear(world, entity);
    panel.fields.clear();
    panel.radius_fields.clear();
    label(world, entity, "Visibility");
    label(
        world,
        entity,
        "Any matching include allows access. Any matching exclusion denies it. Lower proximity means a closer Organ.",
    );
    if panel.draft.is_none() {
        button(world, entity, entity, "Choose Record", Intent::ChooseRecord);
        if let Some(context) = &panel.context {
            if let Some(record) = context
                .records
                .iter()
                .find(|record| record.uid == panel.record)
            {
                label(world, entity, format!("Record: {}", record.label));
            }
            if panel.choosing_record {
                for record in &context.records {
                    button(
                        world,
                        entity,
                        entity,
                        &record.label,
                        Intent::Record(record.uid.clone()),
                    );
                }
            }
        }
        let row = controls::row(world, entity);
        for (data, caption) in [
            (Data::Record, "Record data"),
            (Data::Place, "Saved place"),
            (Data::LiveLocation, "Live location"),
        ] {
            button(
                world,
                row,
                entity,
                &format!("{}{}", if data == panel.data { "✓ " } else { "" }, caption),
                Intent::Data(data),
            );
        }
    }
    label(
        world,
        entity,
        match panel.data {
            Data::Record => {
                "These rules bound Record access and future sharing. Existing roles, grants, and sync settings still apply."
            }
            Data::Place => {
                "Choose an exact point or an approximate area for each inclusion. Approximate access withholds exact Record replication and address text. Linked places also require their own permission."
            }
            Data::LiveLocation => {
                "Live coordinates expire. Choose an exact point or an approximate area for each inclusion. Person and Organ exclusions override named invitations. The controller retains exact access."
            }
        },
    );
    for (side, rules) in [(Include, &panel.include), (Exclude, &panel.exclude)] {
        if panel.draft.is_some_and(|draft| draft != side) {
            continue;
        }
        label(
            world,
            entity,
            if side == Exclude {
                "Exclude conditions"
            } else {
                "Include conditions"
            },
        );
        if rules.is_empty() {
            label(
                world,
                entity,
                if side == Exclude {
                    "No excluded Organs"
                } else {
                    "No additional Organ access"
                },
            );
        }
        for (index, rule) in rules.iter().enumerate() {
            let block = controls::column(world, entity);
            label(
                world,
                block,
                format!(
                    "{} {} · all selected criteria must match",
                    if side == Exclude {
                        "Exclude"
                    } else {
                        "Include"
                    },
                    index + 1
                ),
            );
            label(
                world,
                block,
                if rule.organs.is_empty() {
                    if rule.persons.is_empty() {
                        "Any known authenticated Organ"
                    } else {
                        "Selected Persons on their authenticated devices"
                    }
                } else {
                    "Selected Organs only"
                },
            );
            button(
                world,
                block,
                entity,
                "Choose Organs and Persons for this condition",
                Intent::ChooseOrgans(side, index),
            );
            if let Some(context) = &panel.context {
                for organ in &context.organs {
                    if panel.choosing_organs != Some((side, index)) {
                        if rule.organs.contains(&organ.uid) {
                            label(world, block, &organ.label);
                        }
                        continue;
                    }
                    button(
                        world,
                        block,
                        entity,
                        &format!(
                            "{}{}",
                            if rule.organs.contains(&organ.uid) {
                                "✓ "
                            } else {
                                ""
                            },
                            organ.label
                        ),
                        Intent::Organ(side, index, organ.uid.clone()),
                    );
                }
                label(
                    world,
                    block,
                    "Persons (empty = no Person restriction; selected Organ and proximity criteria still apply)",
                );
                for person in &context.persons {
                    if panel.choosing_organs != Some((side, index)) {
                        if rule.persons.contains(&person.uid) {
                            label(world, block, format!("Person: {}", person.label));
                        }
                        continue;
                    }
                    button(
                        world,
                        block,
                        entity,
                        &format!(
                            "{}Person: {}",
                            if rule.persons.contains(&person.uid) {
                                "✓ "
                            } else {
                                ""
                            },
                            person.label
                        ),
                        Intent::Person(side, index, person.uid.clone()),
                    );
                }
            }
            let lower = input(
                world,
                block,
                "Lower proximity bound (empty = no lower limit)",
                &rule.lower,
            );
            panel.fields.push((side, index, Lower, lower));
            button(
                world,
                block,
                entity,
                if rule.lower_inclusive {
                    "Lower: at least (≥)"
                } else {
                    "Lower: greater than (>)"
                },
                Intent::Inclusive(side, index, Lower),
            );
            let upper = input(
                world,
                block,
                "Upper proximity bound (empty = no upper limit)",
                &rule.upper,
            );
            panel.fields.push((side, index, Upper, upper));
            if side == Include && panel.data != Data::Record {
                let radius = input(
                    world,
                    block,
                    "Area radius in metres (empty = exact point; 100–100000)",
                    &rule.radius,
                );
                panel.radius_fields.push((index, radius));
                label(
                    world,
                    block,
                    "Matching inclusions grant the most precise allowed view. The area center is generalized before sharing.",
                );
            }
            button(
                world,
                block,
                entity,
                if rule.upper_inclusive {
                    "Upper: at most (≤)"
                } else {
                    "Upper: less than (<)"
                },
                Intent::Inclusive(side, index, Upper),
            );
            button(
                world,
                block,
                entity,
                "Remove condition",
                Intent::Remove(side, index),
            );
        }
        button(
            world,
            entity,
            entity,
            if side == Exclude {
                "Add exclusion"
            } else {
                "Add inclusion"
            },
            Intent::Add(side),
        );
    }
    let row = controls::row(world, entity);
    button(world, row, entity, "Preview rules", Intent::Preview);
    if panel.draft.is_none() {
        button(world, row, entity, "Save visibility", Intent::Save);
        button(world, row, entity, "Reload saved rules", Intent::Refresh);
    }
    label(
        world,
        entity,
        "Preview of these rules · existing roles and authentication also apply",
    );
    preview::draw(world, entity, &panel);
    label(world, entity, &panel.notice);
    button(
        world,
        entity,
        entity,
        "Credits and licenses",
        Intent::Credits,
    );
    if panel.show_credits {
        label(
            world,
            entity,
            "Accessibility · AccessKit contributors · MIT",
        );
        label(
            world,
            entity,
            include_str!("../../licenses/accesskit-MIT.txt"),
        );
    }
    world.entity_mut(entity).insert(panel);
}
