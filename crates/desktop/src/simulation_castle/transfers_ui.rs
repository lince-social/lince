use super::*;
use simulation::assumptions::{Direction, TransferAssumption, TransferSource};
use simulation::scenario::{Event, Input as ScenarioInput, Scenario};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Form {
    pub loan: Option<simulation::loans::Timing>,
    pub loan_until: String,
    pub loan_returned: String,
    pub selected: Option<usize>,
    pub pending: bool,
    pub title: String,
    pub person: String,
    pub record: String,
    pub amount: String,
    pub hours: String,
    pub unit: String,
    pub incoming: bool,
    pub source: Option<TransferSource>,
    pub key: String,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            loan: None,
            loan_until: String::new(),
            loan_returned: String::new(),
            selected: None,
            pending: false,
            title: "Transfer assumption".into(),
            person: String::new(),
            record: String::new(),
            amount: "1".into(),
            hours: "24".into(),
            unit: String::new(),
            incoming: false,
            source: None,
            key: String::new(),
        }
    }
}

impl Form {
    pub(super) fn valid(&self) -> bool {
        [
            &self.loan_until,
            &self.loan_returned,
            &self.title,
            &self.person,
            &self.record,
            &self.amount,
            &self.hours,
            &self.unit,
            &self.key,
        ]
        .iter()
        .all(|field| field.len() <= 2_000)
            && self.source.as_ref().is_none_or(|source| {
                source.revision > 0
                    && [&source.transfer, &source.promise, &source.exchange]
                        .iter()
                        .all(|field| !field.is_empty() && field.len() <= 512)
                    && source
                        .occurrence
                        .as_ref()
                        .is_none_or(|field| !field.is_empty() && field.len() <= 512)
            })
    }
}

#[derive(Clone, Copy)]
pub(super) enum Command {
    Select(usize),
    Remove(usize),
    New,
    Direction,
    Apply,
}

#[derive(Clone, Copy)]
enum Field {
    LoanUntil,
    LoanReturned,
    Title,
    Person,
    Record,
    Amount,
    Hours,
    Unit,
}

#[derive(Component)]
struct Input {
    owner: Entity,
    field: Field,
}

pub(super) fn capture(world: &mut World) {
    let inputs: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .map(|(input, text)| (input.owner, input.field, text.value().to_string()))
        .collect();
    for (owner, field, value) in inputs {
        if let Some(mut model) = world.get_mut::<SimulationCastle>(owner) {
            let form = &mut model.transfer_form;
            *match field {
                Field::LoanUntil => &mut form.loan_until,
                Field::LoanReturned => &mut form.loan_returned,
                Field::Title => &mut form.title,
                Field::Person => &mut form.person,
                Field::Record => &mut form.record,
                Field::Amount => &mut form.amount,
                Field::Hours => &mut form.hours,
                Field::Unit => &mut form.unit,
            } = value;
        }
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let model = world.get::<SimulationCastle>(owner).unwrap().clone();
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let parent = view.transfers;
    world.entity_mut(parent).despawn_children();
    crate::edit_mode::label(
        world,
        parent,
        "Transfer assumptions · changes happen only in the copied database",
        14.0,
    );
    let Ok(scenario) = serde_json::from_str::<Scenario>(&model.scenario) else {
        return;
    };
    for (index, input) in scenario.inputs.iter().enumerate() {
        if let Event::AssumeLoan { timing } = &input.event {
            let line = row(world, parent);
            button(
                world,
                line,
                owner,
                &format!("Loan availability · {}", timing.source.exchange),
                Command::Select(index),
            );
            button(world, line, owner, "Remove", Command::Remove(index));
        }
        if let Event::AssumeTransfer { assumption } = &input.event {
            let line = row(world, parent);
            button(
                world,
                line,
                owner,
                &format!(
                    "{} · {} · {}",
                    assumption.title, assumption.quantity, assumption.record
                ),
                Command::Select(index),
            );
            button(world, line, owner, "Remove", Command::Remove(index));
        }
    }
    let form = &model.transfer_form;
    if form.loan.is_some() {
        crate::edit_mode::label(
            world,
            parent,
            "These assumptions change loan availability. To simulate the physical return too, use a return proposal.",
            12.0,
        );
    }
    if !model.pending_transfers.is_empty() {
        crate::edit_mode::label(
            world,
            parent,
            &format!(
                "{} more changes need your review and local bindings",
                model.pending_transfers.len()
            ),
            13.0,
        );
    }
    for (field, label, value) in [
        (
            Field::LoanUntil,
            "Assumed loan deadline (optional, with timezone)",
            &form.loan_until,
        ),
        (
            Field::LoanReturned,
            "Returned for Need availability (optional)",
            &form.loan_returned,
        ),
        (Field::Title, "Title", &form.title),
        (Field::Person, "My Person", &form.person),
        (Field::Record, "My Record", &form.record),
        (Field::Amount, "Amount", &form.amount),
        (Field::Hours, "Hours after the run starts", &form.hours),
        (Field::Unit, "Unit (when used)", &form.unit),
    ] {
        if matches!(field, Field::LoanUntil | Field::LoanReturned) != form.loan.is_some()
            && (matches!(field, Field::LoanUntil | Field::LoanReturned)
                || matches!(field, Field::Record | Field::Amount | Field::Unit))
        {
            continue;
        }
        let line = row(world, parent);
        crate::edit_mode::label(world, line, label, 12.0);
        let editor = world
            .spawn(crate::sand::text_editor(
                value,
                world.resource::<crate::theme::Typography>(),
                0,
            ))
            .insert((
                ChildOf(line),
                Input { owner, field },
                Node {
                    width: px(300),
                    flex_shrink: 0.0,
                    ..default()
                },
            ))
            .id();
        let mut text = world.get_mut::<EditableText>(editor).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(2_000);
    }
    let line = row(world, parent);
    if form.source.is_none() {
        button(
            world,
            line,
            owner,
            if form.incoming {
                "Incoming"
            } else {
                "Outgoing"
            },
            Command::Direction,
        );
    } else {
        crate::edit_mode::label(
            world,
            line,
            if form.incoming {
                "Assume I receive"
            } else {
                "Assume I give"
            },
            13.0,
        );
    }
    button(
        world,
        line,
        owner,
        if form.selected.is_some() {
            "Update assumption"
        } else {
            "Add assumption"
        },
        Command::Apply,
    );
    button(world, line, owner, "New assumption", Command::New);
}

fn button(world: &mut World, parent: Entity, owner: Entity, label: &str, command: Command) {
    crate::castle_feed::button(
        world,
        parent,
        owner,
        label,
        super::Command::Transfers(command),
    );
}

pub(super) fn apply(command: Command, world: &mut World, owner: Entity) -> simulation::Result<()> {
    let mut model = world.get::<SimulationCastle>(owner).unwrap().clone();
    let mut scenario: Scenario = serde_json::from_str(&model.scenario)?;
    match command {
        Command::New => model.transfer_form = Form::default(),
        Command::Direction => model.transfer_form.incoming = !model.transfer_form.incoming,
        Command::Select(index) => {
            let input = scenario
                .inputs
                .get(index)
                .ok_or("Assumption is no longer present")?;
            if let Event::AssumeLoan { timing } = &input.event {
                model.selected_cell = input.cell.clone();
                model.transfer_form = Form {
                    loan: Some(timing.clone()),
                    loan_until: timing.until.clone().unwrap_or_default(),
                    loan_returned: timing
                        .returned
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    person: timing.person.clone(),
                    hours: ((input.at_ms - scenario.start_ms) as f64 / 3_600_000.0).to_string(),
                    selected: Some(index),
                    ..Default::default()
                };
                world.entity_mut(owner).insert(model);
                render(world, owner);
                return Ok(());
            }
            let Event::AssumeTransfer { assumption } = &input.event else {
                return Err("Choose a Transfer assumption".into());
            };
            model.selected_cell = input.cell.clone();
            model.transfer_form = Form {
                selected: Some(index),
                pending: false,
                title: assumption.title.clone(),
                person: assumption.person.clone(),
                record: assumption.record.clone(),
                amount: assumption.quantity.canonical(),
                hours: ((input.at_ms - scenario.start_ms) as f64 / 3_600_000.0).to_string(),
                unit: assumption.unit.clone().unwrap_or_default(),
                incoming: assumption.direction == Direction::Incoming,
                source: assumption.source.clone(),
                key: assumption.key.clone(),
                ..Default::default()
            };
        }
        Command::Remove(index) => {
            if !scenario.inputs.get(index).is_some_and(|input| {
                matches!(
                    input.event,
                    Event::AssumeTransfer { .. } | Event::AssumeLoan { .. }
                )
            }) {
                return Err("Choose a Transfer assumption".into());
            }
            scenario.inputs.remove(index);
            model.transfer_form = Form::default();
        }
        Command::Apply => {
            let form = &model.transfer_form;
            if let Some(index) = form.selected
                && !scenario.inputs.get(index).is_some_and(|input| {
                    matches!(&input.event, Event::AssumeTransfer { assumption } if assumption.key == form.key) || matches!(&input.event,Event::AssumeLoan {..}) && form.loan.is_some()
                })
            {
                return Err("The selected assumption changed; select it again".into());
            }
            let hours: f64 = form.hours.trim().parse()?;
            if !hours.is_finite() || hours < 0.0 || hours > 24.0 * 365_000.0 {
                return Err("Choose valid hours after the run starts".into());
            }
            let at_ms = scenario
                .start_ms
                .checked_add((hours * 3_600_000.0).round() as i64)
                .ok_or("Assumption time overflow")?;
            let id = form
                .selected
                .and_then(|index| scenario.inputs.get(index))
                .map(|input| input.id.clone())
                .unwrap_or_else(|| nucleus::new_uid("assume"));
            let input = ScenarioInput {
                id: id.clone(),
                at_ms,
                cell: model.selected_cell.clone(),
                event: if let Some(mut timing) = form.loan.clone() {
                    timing.until = (!form.loan_until.trim().is_empty())
                        .then(|| form.loan_until.trim().to_owned());
                    timing.returned = (!form.loan_returned.trim().is_empty())
                        .then(|| nucleus::DecimalValue::parse_inferred(form.loan_returned.trim()))
                        .transpose()?;
                    timing.person = form.person.clone();
                    Event::AssumeLoan { timing }
                } else {
                    Event::AssumeTransfer {
                        assumption: TransferAssumption {
                            key: if form.key.is_empty() {
                                id
                            } else {
                                form.key.clone()
                            },
                            title: form.title.clone(),
                            person: form.person.trim().into(),
                            record: form.record.trim().into(),
                            quantity: nucleus::DecimalValue::parse_inferred(form.amount.trim())?,
                            unit: (!form.unit.trim().is_empty()).then(|| form.unit.trim().into()),
                            direction: if form.incoming {
                                Direction::Incoming
                            } else {
                                Direction::Outgoing
                            },
                            source: form.source.clone(),
                        },
                    }
                },
            };
            if let Some(index) = form.selected {
                *scenario
                    .inputs
                    .get_mut(index)
                    .ok_or("Assumption is no longer present")? = input;
            } else {
                model.transfer_form.selected = Some(scenario.inputs.len());
                scenario.inputs.push(input);
            }
            scenario.validate()?;
            if let Some(index) = model.transfer_form.selected
                && let Event::AssumeTransfer { assumption } = &scenario.inputs[index].event
            {
                model.transfer_form.key = assumption.key.clone();
            }
            model.transfer_form.pending = false;
            if !model.pending_transfers.is_empty() {
                model.transfer_form = model.pending_transfers.remove(0);
            }
        }
    }
    model.scenario = serde_json::to_string_pretty(&scenario)?;
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text(&model.scenario);
    update_input(world, owner, super::Field::Cell, &model.selected_cell);
    world.entity_mut(owner).insert(model);
    render(world, owner);
    sharing_ui::render(world, owner);
    Ok(())
}
