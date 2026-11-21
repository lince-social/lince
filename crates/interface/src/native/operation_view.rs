// non-lean-yet
// Scope: const REPORT_LIMIT, struct Reports, fn create, fn usage.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   24.

use bevy::prelude::*;

const REPORT_LIMIT: usize = 16;

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
    let _presentation = nucleus::performance::Span::new("operation-usage-presentation");
    let reports = &reports[reports.len().saturating_sub(REPORT_LIMIT)..];
    if world
        .get::<Reports>(entity)
        .is_some_and(|previous| previous.0 == reports)
    {
        return;
    }
    nucleus::performance::add("operation-usage-reports-prepared", reports.len() as u64);
    world
        .entity_mut(entity)
        .insert(Reports(reports.to_vec()))
        .despawn_children();
    crate::native::edit_mode::label(
        world,
        entity,
        "Reported usage · private to this account",
        14.0,
    );
    if reports.is_empty() {
        crate::native::edit_mode::label(world, entity, "Usage and cost unavailable", 13.0);
    }
    for report in reports.iter().rev() {
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
        crate::native::edit_mode::label(world, entity, &text, 13.0);
    }
}
