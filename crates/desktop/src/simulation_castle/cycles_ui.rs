use nucleus::simulation::{CycleKind, RuleCycle};

fn time(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map_or_else(|| ms.to_string(), |time| time.to_rfc3339())
}

pub(super) fn describe(cycle: &RuleCycle) -> String {
    let kind = match cycle.kind {
        CycleKind::Feedback => "Feedback",
        CycleKind::SettledFeedback => "Feedback that settled",
        CycleKind::TimedRecurrence => "Recurring scheduled changes",
    };
    let mut lines = vec![format!(
        "{kind} · {} · {} repetitions\nRules: {}",
        cycle.id,
        cycle.repetitions,
        cycle.rules.join(", ")
    )];
    for step in &cycle.steps {
        lines.push(format!(
            "{} · Cell {} · Rule {} revision {}",
            time(step.virtual_ms),
            step.cell,
            step.occurrence.rule_uid,
            step.occurrence.revision
        ));
        if let Some(intended) = step.occurrence.intended_at_ms
            && intended != step.at_ms
        {
            lines.push(format!("  Scheduled for {}", time(intended)));
        }
        lines.push(format!("  Woken by {}", step.occurrence.event_id));
        for change in &step.changes {
            lines.push(format!(
                "  {}: {} → {}{} · Fact {}",
                change.record.as_str(),
                change.before.value,
                change.after.value,
                change
                    .after
                    .unit
                    .as_ref()
                    .map_or(String::new(), |unit| format!(" {}", unit.as_str())),
                String::from(change.fact.clone())
            ));
        }
        for change in &step.transfer_changes {
            let participants: Vec<_> = change.after.participants.iter().filter_map(|(person, after)| {
                change.before.participants.get(person).filter(|before| *before != after).map(|before| format!("{person}: {} → {}", before.guard.level, after.guard.level))
            }).collect();
            lines.push(format!("  Transfer {} · Cell {} · {}\n  revision {} → {} · active {} → {} · published {} → {} · ready {} → {}{}", change.after.transfer, change.cell, time(change.virtual_ms), change.before.revision, change.after.revision, change.before.active, change.after.active, change.before.published, change.after.published, change.before.ready, change.after.ready, if participants.is_empty() { String::new() } else { format!("\n  {}", participants.join(", ")) }));
        }
        if step.changes.is_empty() && step.transfer_changes.is_empty() {
            lines.push(format!("  Actions: {}", step.consequences.join(", ")));
        }
    }
    if cycle.truncated {
        lines.push("The chain is longer than this sample; the Rules list includes every contributing Rule.".into());
    }
    lines.join("\n")
}
