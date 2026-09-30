use bevy::{prelude::*, text::EditableText};
use nucleus::simulation::{
    CheckDefinition, CheckOptions, Comparison, Evaluation, FailureMode, Predicate, Quantity,
};
use serde::{Deserialize, Serialize};

use super::{SimulationCastle, View};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Form {
    selected: Option<usize>,
    name: String,
    cell: String,
    record: String,
    amount: String,
    unit: String,
    interval: String,
    from: String,
    until: String,
    pub set_name: String,
    comparison: Comparison,
    evaluation: Evaluation,
    quantity: nucleus::simulation::QuantityBasis,
    pub saved: Option<nucleus::simulation::CheckSet>,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            selected: None,
            name: "Quantity limit".into(),
            cell: "a".into(),
            record: String::new(),
            amount: "0".into(),
            unit: String::new(),
            interval: "1".into(),
            from: String::new(),
            until: String::new(),
            set_name: "My checks".into(),
            comparison: Comparison::AtLeast,
            evaluation: Evaluation::EveryChange,
            quantity: nucleus::simulation::QuantityBasis::Stored,
            saved: None,
        }
    }
}

impl Form {
    pub(super) fn available(mut self) -> Self {
        self.quantity = nucleus::simulation::QuantityBasis::Available;
        self
    }
    pub(super) fn target(cell: &str, record: &str) -> Self {
        Self {
            cell: cell.into(),
            record: record.into(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy)]
enum Field {
    Name,
    Cell,
    Record,
    Amount,
    Unit,
    Interval,
    From,
    Until,
    SetName,
}

#[derive(Component)]
struct Input {
    owner: Entity,
    field: Field,
}

#[derive(Clone)]
pub(super) enum Command {
    Select(usize),
    Toggle(usize),
    Remove(usize),
    New,
    Apply,
    Comparison,
    QuantityBasis,
    Evaluation,
    Failure,
    Builtin(u8),
    Save,
    Load,
    List,
    Search,
}

fn button(world: &mut World, parent: Entity, owner: Entity, label: &str, command: Command) {
    crate::castle_feed::button(world, parent, owner, label, super::Command::Checks(command));
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let panel = world.get::<View>(owner).unwrap().checks;
    if let Some(children) = world.get::<Children>(panel) {
        let children: Vec<_> = children.iter().collect();
        for child in children {
            world.entity_mut(child).despawn();
        }
    }
    let model = world.get::<SimulationCastle>(owner).unwrap().clone();
    let Ok(scenario) = serde_json::from_str::<simulation::scenario::Scenario>(&model.scenario)
    else {
        crate::edit_mode::label(
            world,
            panel,
            "Correct the scenario before editing checks.",
            13.0,
        );
        return;
    };
    crate::edit_mode::label(world, panel, "Checks · optional and private", 16.0);
    let line = super::row(world, panel);
    button(
        world,
        line,
        owner,
        &format!("Quantity: {:?}", model.check_form.quantity),
        Command::QuantityBasis,
    );
    button(
        world,
        line,
        owner,
        match scenario.checking.on_failure {
            FailureMode::Stop => "On failure: Stop",
            FailureMode::Continue => "On failure: Continue",
        },
        Command::Failure,
    );
    for (label, command) in [
        ("New quantity check", Command::New),
        ("Fact integrity", Command::Builtin(0)),
        ("No duplicates", Command::Builtin(1)),
        ("No unexpected refusals", Command::Builtin(2)),
        ("No feedback cycles", Command::Builtin(3)),
    ] {
        button(world, line, owner, label, command);
    }
    for (index, check) in scenario.checks.iter().enumerate() {
        let line = super::row(world, panel);
        button(
            world,
            line,
            owner,
            if check.options.enabled {
                "Enabled"
            } else {
                "Disabled"
            },
            Command::Toggle(index),
        );
        let name = if check.options.name.is_empty() {
            &check.id
        } else {
            &check.options.name
        };
        button(
            world,
            line,
            owner,
            &format!("{name} · {:?}", check.evaluation()),
            Command::Select(index),
        );
        button(world, line, owner, "Remove", Command::Remove(index));
    }
    if scenario.checks.iter().all(|check| !check.options.enabled) {
        crate::edit_mode::label(
            world,
            panel,
            "No checks enabled: this run will be unverified.",
            13.0,
        );
    }
    let form = &model.check_form;
    for fields in [
        vec![
            ("Name", Field::Name, &form.name),
            ("Cell", Field::Cell, &form.cell),
            ("Record", Field::Record, &form.record),
        ],
        vec![
            ("Amount", Field::Amount, &form.amount),
            ("Unit UID (blank: no unit)", Field::Unit, &form.unit),
            (
                "Time / events / milliseconds",
                Field::Interval,
                &form.interval,
            ),
        ],
        vec![
            ("From (blank: start)", Field::From, &form.from),
            ("Until (blank: end)", Field::Until, &form.until),
            ("Saved set", Field::SetName, &form.set_name),
        ],
    ] {
        let line = super::row(world, panel);
        for (label, field, value) in fields {
            crate::edit_mode::label(world, line, label, 12.0);
            let input = world
                .spawn(crate::sand::text_editor(
                    value,
                    world.resource::<crate::theme::Typography>(),
                    0,
                ))
                .insert((
                    ChildOf(line),
                    Input { owner, field },
                    Node {
                        width: px(150),
                        ..default()
                    },
                ))
                .id();
            let mut text = world.get_mut::<EditableText>(input).unwrap();
            text.allow_newlines = false;
            text.visible_lines = Some(1.0);
            text.max_characters = Some(200);
        }
    }
    let line = super::row(world, panel);
    button(
        world,
        line,
        owner,
        &format!("Compare: {:?}", form.comparison),
        Command::Comparison,
    );
    button(
        world,
        line,
        owner,
        &format!("Evaluate: {:?}", form.evaluation),
        Command::Evaluation,
    );
    for (label, command) in [
        ("Apply check", Command::Apply),
        ("Save set", Command::Save),
        ("Load set", Command::Load),
        ("List sets", Command::List),
        ("Search with these checks", Command::Search),
    ] {
        button(world, line, owner, label, command);
    }
    crate::edit_mode::label(
        world,
        panel,
        "Every change checks committed states. Event and time intervals are sampled. Times use UTC dates, timestamps or milliseconds.",
        12.0,
    );
}

pub(super) fn capture(world: &mut World) {
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .map(|(input, text)| (input.owner, input.field, text.value().to_string()))
        .collect();
    for (owner, field, value) in values {
        if let Some(mut model) = world.get_mut::<SimulationCastle>(owner) {
            let form = &mut model.check_form;
            *match field {
                Field::Name => &mut form.name,
                Field::Cell => &mut form.cell,
                Field::Record => &mut form.record,
                Field::Amount => &mut form.amount,
                Field::Unit => &mut form.unit,
                Field::Interval => &mut form.interval,
                Field::From => &mut form.from,
                Field::Until => &mut form.until,
                Field::SetName => &mut form.set_name,
            } = value;
        }
    }
}

fn time(value: &str) -> simulation::Result<Option<i64>> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        super::runner::through(value).map(Some)
    }
}

fn definition(
    form: &Form,
    scenario: &simulation::scenario::Scenario,
) -> simulation::Result<CheckDefinition> {
    let mut evaluation = form.evaluation.clone();
    match &mut evaluation {
        Evaluation::At { at_ms } => *at_ms = super::runner::through(&form.interval)?,
        Evaluation::EveryEvents { every } => *every = form.interval.trim().parse()?,
        Evaluation::EveryDuration { millis } => *millis = form.interval.trim().parse()?,
        _ => {}
    }
    let existing = form.selected.and_then(|index| scenario.checks.get(index));
    let id = existing.map_or_else(
        || {
            (1..)
                .map(|index| format!("check-{index}"))
                .find(|id| scenario.checks.iter().all(|check| &check.id != id))
                .unwrap()
        },
        |check| check.id.clone(),
    );
    let predicate = match existing.map(|check| &check.predicate) {
        Some(predicate)
            if !matches!(
                predicate,
                Predicate::Quantity { .. }
                    | Predicate::Nonnegative { .. }
                    | Predicate::QuantityEquals { .. }
            ) =>
        {
            predicate.clone()
        }
        _ => Predicate::Quantity {
            cell: form.cell.trim().into(),
            record: form.record.trim().into(),
            comparison: form.comparison,
            expected: Quantity {
                value: nucleus::DecimalValue::parse_inferred(form.amount.trim())?,
                unit: if form.unit.trim().is_empty() {
                    None
                } else {
                    Some(nucleus::karma::TypedUid::new(
                        nucleus::karma::ReferenceKind::Unit,
                        form.unit.trim(),
                    )?)
                },
            },
        },
    };
    Ok(CheckDefinition {
        id,
        predicate,
        options: CheckOptions {
            quantity: form.quantity,
            name: form.name.trim().into(),
            enabled: existing.is_none_or(|check| check.options.enabled),
            evaluation,
            window: nucleus::simulation::checks::CheckWindow {
                from_ms: time(&form.from)?,
                until_ms: time(&form.until)?,
            },
        },
    })
}

pub(super) fn apply(
    command: &Command,
    world: &mut World,
    owner: Entity,
) -> simulation::Result<bool> {
    if matches!(
        command,
        Command::Save | Command::Load | Command::List | Command::Search
    ) {
        return Ok(false);
    }
    let mut model = world.get::<SimulationCastle>(owner).unwrap().clone();
    let mut scenario: simulation::scenario::Scenario = serde_json::from_str(&model.scenario)?;
    let form = &mut model.check_form;
    match command {
        Command::QuantityBasis => {
            form.quantity = match form.quantity {
                nucleus::simulation::QuantityBasis::Stored => {
                    nucleus::simulation::QuantityBasis::Available
                }
                nucleus::simulation::QuantityBasis::Available => {
                    nucleus::simulation::QuantityBasis::Stored
                }
            }
        }
        Command::Select(index) => {
            let check = scenario
                .checks
                .get(*index)
                .ok_or("Check no longer exists")?;
            form.selected = Some(*index);
            form.name = check.options.name.clone();
            form.evaluation = check.evaluation();
            form.quantity = check.options.quantity;
            form.from = check
                .options
                .window
                .from_ms
                .map(|time| time.to_string())
                .unwrap_or_default();
            form.until = check
                .options
                .window
                .until_ms
                .map(|time| time.to_string())
                .unwrap_or_default();
            form.interval = match form.evaluation {
                Evaluation::At { at_ms } => at_ms.to_string(),
                Evaluation::EveryEvents { every } => every.to_string(),
                Evaluation::EveryDuration { millis } => millis.to_string(),
                _ => "1".into(),
            };
            match &check.predicate {
                Predicate::Quantity {
                    cell,
                    record,
                    expected,
                    ..
                }
                | Predicate::QuantityEquals {
                    cell,
                    record,
                    expected,
                    ..
                } => {
                    form.cell = cell.clone();
                    form.record = record.clone();
                    form.amount = expected.value.to_string();
                    form.unit = expected
                        .unit
                        .as_ref()
                        .map(|uid| uid.as_str().into())
                        .unwrap_or_default();
                    form.comparison =
                        if let Predicate::Quantity { comparison, .. } = check.predicate {
                            comparison
                        } else {
                            Comparison::Equal
                        };
                }
                Predicate::Nonnegative { cell, record } => {
                    form.cell = cell.clone();
                    form.record = record.clone();
                    form.amount = "0".into();
                    form.unit.clear();
                    form.comparison = Comparison::AtLeast;
                }
                _ => {}
            }
        }
        Command::Toggle(index) => {
            let check = scenario
                .checks
                .get_mut(*index)
                .ok_or("Check no longer exists")?;
            check.options.enabled = !check.options.enabled;
        }
        Command::Remove(index) => {
            if *index < scenario.checks.len() {
                scenario.checks.remove(*index);
                form.selected = None;
            }
        }
        Command::New => {
            form.selected = None;
            form.name = "Quantity limit".into();
            form.cell = model.selected_cell.clone();
            form.record.clear();
        }
        Command::Apply => {
            let check = definition(form, &scenario)?;
            if let Some(index) = form.selected {
                *scenario
                    .checks
                    .get_mut(index)
                    .ok_or("Check changed; select it again")? = check;
            } else {
                form.selected = Some(scenario.checks.len());
                scenario.checks.push(check);
            }
            scenario.validate()?;
        }
        Command::Comparison => {
            form.comparison = match form.comparison {
                Comparison::Less => Comparison::AtMost,
                Comparison::AtMost => Comparison::Equal,
                Comparison::Equal => Comparison::AtLeast,
                Comparison::AtLeast => Comparison::Greater,
                Comparison::Greater => Comparison::Less,
            }
        }
        Command::Evaluation => {
            form.evaluation = match form.evaluation {
                Evaluation::Default | Evaluation::End => Evaluation::At {
                    at_ms: scenario.end_ms,
                },
                Evaluation::At { .. } => Evaluation::EveryChange,
                Evaluation::EveryChange => Evaluation::EveryEvents { every: 1 },
                Evaluation::EveryEvents { .. } => Evaluation::EveryDuration { millis: 1000 },
                Evaluation::EveryDuration { .. } => Evaluation::End,
            };
            form.interval = match form.evaluation {
                Evaluation::At { at_ms } => at_ms.to_string(),
                Evaluation::EveryEvents { every } => every.to_string(),
                Evaluation::EveryDuration { millis } => millis.to_string(),
                _ => "1".into(),
            };
        }
        Command::Failure => {
            scenario.checking.on_failure = if scenario.checking.on_failure == FailureMode::Stop {
                FailureMode::Continue
            } else {
                FailureMode::Stop
            }
        }
        Command::Builtin(kind) => {
            form.selected = None;
            let predicate = match kind {
                0 => Predicate::FactChain {},
                1 => Predicate::OncePerOccurrence {},
                3 => Predicate::NoRuleCycles { include_timed_recurrence: false },
                _ => Predicate::NoUnexpectedRefusals {},
            };
            let id = (1..)
                .map(|index| format!("builtin-{index}"))
                .find(|id| scenario.checks.iter().all(|check| &check.id != id))
                .unwrap();
            scenario.checks.push(CheckDefinition {
                id,
                predicate,
                options: Default::default(),
            });
        }
        _ => unreachable!(),
    }
    model.scenario = serde_json::to_string_pretty(&scenario)?;
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text(&model.scenario);
    world.entity_mut(owner).insert(model);
    render(world, owner);
    Ok(true)
}

impl Form {
    pub(super) fn valid(&self) -> bool {
        [
            &self.name,
            &self.cell,
            &self.record,
            &self.amount,
            &self.unit,
            &self.interval,
            &self.from,
            &self.until,
            &self.set_name,
        ]
        .iter()
        .all(|value| value.len() <= 200)
            && self
                .saved
                .as_ref()
                .is_none_or(|set| set.checks.len() <= 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantity_form_preserves_exact_amounts_and_check_windows() {
        let scenario = simulation::fixtures::daily();
        let form = Form {
            record: "stock".into(),
            amount: "0.125".into(),
            from: scenario.start_ms.to_string(),
            until: scenario.end_ms.to_string(),
            evaluation: Evaluation::EveryEvents { every: 1 },
            quantity: nucleus::simulation::QuantityBasis::Available,
            interval: "7".into(),
            ..Default::default()
        };
        let check = definition(&form, &scenario).unwrap();
        assert_eq!(
            check.options.quantity,
            nucleus::simulation::QuantityBasis::Available
        );
        assert_eq!(check.evaluation(), Evaluation::EveryEvents { every: 7 });
        assert_eq!(
            check.interval(scenario.start_ms, scenario.end_ms),
            (scenario.start_ms, scenario.end_ms)
        );
        assert!(
            matches!(check.predicate, Predicate::Quantity { expected, comparison: Comparison::AtLeast, .. } if expected.value.to_string() == "0.125")
        );
        let mut invalid = form;
        invalid.amount = "not a number".into();
        assert!(definition(&invalid, &scenario).is_err());
    }
}
