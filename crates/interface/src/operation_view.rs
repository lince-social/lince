use bevy::prelude::*;

#[derive(Component, Default)]
struct Reports(Vec<nucleus::operation::Usage>);

pub(crate) fn create(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
            Reports::default(),
        ))
        .id()
}

pub(crate) fn usage(world: &mut World, entity: Entity, reports: &[nucleus::operation::Usage]) {
    if world
        .get::<Reports>(entity)
        .is_some_and(|previous| previous.0 == reports)
    {
        return;
    }
    world
        .entity_mut(entity)
        .insert(Reports(reports.to_vec()))
        .despawn_children();
    crate::edit_mode::label(
        world,
        entity,
        "Reported usage · private to this account",
        14.0,
    );
    if reports.is_empty() {
        crate::edit_mode::label(world, entity, "Usage and cost unavailable", 13.0);
    }
    for report in reports.iter().rev().take(16) {
        let number = |value: Option<u64>| {
            value
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".into())
        };
        let time =
            chrono::DateTime::from_timestamp_millis(report.updated_ms.min(i64::MAX as u64) as i64)
                .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_default();
        let mut text = format!("{} · {} · {time}", report.source, report.scope);
        if report.context_used.is_some() || report.context_capacity.is_some() {
            text.push_str(&format!(
                "\nContext: {} / {} tokens",
                number(report.context_used),
                number(report.context_capacity)
            ));
        }
        if report.scope == "request" {
            text.push_str(&format!(
                "\nInput: {} · output: {} · total: {} tokens",
                number(report.input_tokens),
                number(report.output_tokens),
                number(report.total_tokens)
            ));
        }
        text.push_str(&match &report.cost {
            Some(cost) => format!(
                "\n{} cost: {} {}",
                if cost.estimated {
                    "Estimated"
                } else {
                    "Reported"
                },
                cost.amount,
                cost.currency
            ),
            None => "\nCost unavailable".into(),
        });
        crate::edit_mode::label(world, entity, &text, 13.0);
    }
}
