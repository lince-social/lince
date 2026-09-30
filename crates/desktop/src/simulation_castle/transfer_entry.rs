use super::*;
use serde_json::Value;
use simulation::assumptions::TransferSource;

pub(super) fn prepare(
    transfer: &Value,
    promise: &Value,
    person: &str,
    shared: Option<(&nucleus::simulation::sharing::Shared, usize)>,
    loan_timing: bool,
) -> simulation::Result<SimulationCastle> {
    if person.is_empty() {
        return Err("Choose your Person first".into());
    }
    let field = |name: &str| promise[name].as_str().unwrap_or_default();
    let route = &promise["item"]["exchange"];
    let giver = promise["giver"].as_str().or(route["giver"].as_str());
    let receiver = promise["receiver"].as_str().or(route["receiver"].as_str());
    let incoming = if receiver == Some(person) {
        true
    } else if giver == Some(person) {
        false
    } else if giver.is_none() && receiver.is_none() && field("party") == person {
        promise["delta"]
            .as_f64()
            .ok_or("Quantity is not disclosed")?
            > 0.0
    } else {
        return Err("Choose an item where you give or receive".into());
    };
    let now = nucleus::execution::now().timestamp_millis();
    let mut scenario = simulation::fixtures::current_database(now);
    let mut source = transfer["uid"]
        .as_str()
        .zip(transfer["revision"].as_u64())
        .map(|(uid, revision)| {
            Ok::<_, simulation::Error>(TransferSource {
                transfer: uid.into(),
                revision,
                promise: field("uid").into(),
                exchange: promise["exchange"]
                    .as_str()
                    .or(route["uid"].as_str())
                    .ok_or("Choose an item with an exchange route")?
                    .into(),
                occurrence: None,
            })
        })
        .transpose()?;
    let mut amount = promise["delta"]
        .as_f64()
        .ok_or("Quantity is not disclosed")?
        .abs()
        .to_string();
    let mut hours = "24".to_string();
    if let Some((shared, index)) = shared {
        let assumption = shared
            .assumptions
            .get(index)
            .ok_or("Shared assumption unavailable")?;
        if source.as_ref().is_none_or(|source| {
            source.transfer != shared.transfer
                || source.revision != shared.revision
                || source.promise != assumption.source.promise
                || source.exchange != assumption.source.exchange
        }) {
            return Err(
                "Shared assumptions refer to different terms; review the current proposal".into(),
            );
        }
        source = Some(assumption.source.clone());
        amount = assumption.quantity.canonical();
        hours = (assumption.after_ms as f64 / 3_600_000.0).to_string();
        scenario.end_ms = now
            .checked_add(shared.duration_ms)
            .ok_or("Shared duration overflow")?;
    }
    let loan =
        if loan_timing && shared.is_none() && incoming && promise["accepted_loan"].is_object() {
            let accepted = &promise["accepted_loan"];
            let until = accepted["until_ms"]
                .as_i64()
                .ok_or("Loan deadline unavailable")?;
            scenario.end_ms = scenario.end_ms.max(until.saturating_add(86_400_000));
            hours = "0".into();
            Some(simulation::loans::Timing {
                source: nucleus::transfer::loans::Reference {
                    origin: accepted["origin"]
                        .as_str()
                        .ok_or("Loan origin unavailable")?
                        .into(),
                    transfer: transfer["uid"].as_str().ok_or("Loan unavailable")?.into(),
                    exchange: field("exchange").into(),
                },
                accepted_revision: accepted["revision"]
                    .as_u64()
                    .ok_or("Loan agreement unavailable")?,
                person: person.into(),
                until: None,
                returned: None,
            })
        } else {
            None
        };
    if let Some(until) = promise["loan"]["until"]
        .as_str()
        .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
    {
        scenario.end_ms = scenario
            .end_ms
            .max(until.timestamp_millis().saturating_add(86_400_000));
    }
    scenario.checks.clear();
    Ok(SimulationCastle {
        scenario: serde_json::to_string_pretty(&scenario)?,
        current_database: true,
        selected_cell: "current".into(),
        through: chrono::DateTime::from_timestamp_millis(scenario.end_ms)
            .ok_or("Simulation end time is invalid")?
            .to_rfc3339(),
        check_form: if promise["loan"].is_object() || loan.is_some() {
            checks_ui::Form::target("current", "").available()
        } else {
            checks_ui::Form::target("current", "")
        },
        transfer_form: transfers_ui::Form {
            loan,
            pending: true,
            title: promise["title"]
                .as_str()
                .or(promise["item"]["title"].as_str())
                .unwrap_or("Transfer assumption")
                .into(),
            person: person.into(),
            record: if let Some(record) = transfer["private_applications"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|application| {
                    application["person"] == person
                        && application["exchange"].as_str()
                            == source.as_ref().map(|source| source.exchange.as_str())
                })
                .and_then(|application| application["record"].as_str())
            {
                record.into()
            } else if field("party") == person {
                field("record").into()
            } else {
                String::new()
            },
            amount,
            hours,
            unit: field("unit").into(),
            incoming,
            source,
            ..Default::default()
        },
        ..Default::default()
    })
}

pub(crate) fn open(
    world: &mut World,
    owner: Entity,
    transfer: &Value,
    promise: &Value,
    person: &str,
    shared: Option<(&nucleus::simulation::sharing::Shared, usize)>,
    loan_timing: bool,
) -> simulation::Result<()> {
    let mut model = prepare(transfer, promise, person, shared, loan_timing)?;
    if let Some((shared, selected)) = shared {
        for (index, assumption) in shared.assumptions.iter().enumerate() {
            if index == selected {
                continue;
            }
            let promise = transfer["promises"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|row| row["uid"] == assumption.source.promise)
                .ok_or("A shared item is no longer available")?;
            if promise["giver"] == person || promise["receiver"] == person {
                model.pending_transfers.push(
                    prepare(transfer, promise, person, Some((shared, index)), false)?.transfer_form,
                );
            }
        }
    }
    let root = world
        .get::<ChildOf>(owner)
        .ok_or("Workspace unavailable")?
        .parent();
    let workspace = world
        .get::<WorkspaceMember>(owner)
        .ok_or("Workspace unavailable")?
        .0;
    let position = world
        .get::<crate::canvas::CanvasItem>(owner)
        .ok_or("Canvas unavailable")?
        .position
        + DVec2::new(1180.0, 0.0);
    spawn(world, root, workspace, position, model);
    Ok(())
}

pub(crate) fn open_balance(
    world: &mut World,
    owner: Entity,
    balance: &Value,
) -> simulation::Result<()> {
    let model = prepare_balance(balance, nucleus::execution::now().timestamp_millis())?;
    let root = world
        .get::<ChildOf>(owner)
        .ok_or("Workspace unavailable")?
        .parent();
    let workspace = world
        .get::<WorkspaceMember>(owner)
        .ok_or("Workspace unavailable")?
        .0;
    let position = world
        .get::<crate::canvas::CanvasItem>(owner)
        .ok_or("Canvas unavailable")?
        .position
        + DVec2::new(1180.0, 0.0);
    spawn(world, root, workspace, position, model);
    Ok(())
}

pub(crate) fn group_balance(rows: &[Value], root: &str, person: &str) -> simulation::Result<Value> {
    let mut pending = vec![root.to_string()];
    let mut selected = std::collections::BTreeSet::new();
    let mut commitments = std::collections::BTreeMap::new();
    let mut incomplete = Vec::new();
    while let Some(uid) = pending.pop() {
        if !selected.insert(uid.clone()) {
            continue;
        }
        let row = rows
            .iter()
            .find(|row| row["uid"] == uid)
            .ok_or("Open the required children before simulating this group")?;
        if row["children_details_hidden"] == true {
            return Err(
                "Some required parts are private; simulate the parts you can review individually"
                    .into(),
            );
        }
        for child in row["children"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|child| child["required"] == true)
        {
            pending.push(
                child["uid"]
                    .as_str()
                    .ok_or("Required child is unavailable")?
                    .into(),
            );
        }
        for application in row["private_applications"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|application| application["person"] == person)
        {
            let balance = &application["balance"];
            if !balance.is_object() {
                return Err("Choose your private Records before simulating the group".into());
            }
            incomplete.extend(
                balance["incomplete"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            for commitment in balance["commitments"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|commitment| {
                    commitment["transfer"] == uid && commitment["person"] == person
                })
            {
                let key = serde_json::to_string(&serde_json::json!([
                    commitment["transfer"],
                    commitment["exchange"],
                    commitment["occurrence"],
                    commitment["person"]
                ]))?;
                commitments.insert(key, commitment.clone());
            }
        }
    }
    let mut alternatives = std::collections::BTreeMap::new();
    for commitment in commitments
        .values()
        .filter(|commitment| !matches!(commitment["state"].as_str(), Some("active" | "kept")))
    {
        if let Some(group) = commitment["alternative_group"].as_str() {
            let transfer = commitment["transfer"].as_str().unwrap_or_default();
            if alternatives
                .insert(group, transfer)
                .is_some_and(|prior| prior != transfer)
            {
                return Err("Choose one alternative before simulating the group".into());
            }
        }
    }
    Ok(
        serde_json::json!({"selected_transfers": selected, "commitments": commitments.into_values().collect::<Vec<_>>(), "incomplete":incomplete}),
    )
}

fn prepare_balance(balance: &Value, now: i64) -> simulation::Result<SimulationCastle> {
    if balance["incomplete"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty())
    {
        return Err("Review the incomplete commitments before selecting a future scenario".into());
    }
    let mut forms = Vec::new();
    let mut horizon = 86_400_000;
    for commitment in balance["commitments"].as_array().into_iter().flatten() {
        let selected = commitment["transfer"] == balance["selected_transfer"]
            || balance["selected_transfers"]
                .as_array()
                .is_some_and(|rows| rows.contains(&commitment["transfer"]));
        if commitment["alternative_group"].is_string()
            && !selected
            && !matches!(commitment["state"].as_str(), Some("active" | "kept"))
        {
            continue;
        }
        if !selected
            && !matches!(
                commitment["state"].as_str(),
                Some("agreed" | "active" | "kept")
            )
        {
            continue;
        }
        let field = |name: &str| commitment[name].as_str().unwrap_or_default().to_string();
        let after = commitment["due"]
            .as_str()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .map_or(86_400_000, |at| {
                at.timestamp_millis().saturating_sub(now).max(0)
            });
        horizon = horizon.max(after);
        let amount: nucleus::DecimalValue =
            serde_json::from_value(commitment["canonical_remaining"].clone())?;
        forms.push(transfers_ui::Form {
            pending: true,
            title: field("title"),
            person: field("person"),
            record: commitment["application_record"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| field("record")),
            amount: amount.to_string(),
            hours: (after as f64 / 3_600_000.0).to_string(),
            unit: field("unit"),
            incoming: commitment["outgoing"] == false,
            source: Some(TransferSource {
                transfer: field("transfer"),
                revision: commitment["revision"].as_u64().unwrap_or(0),
                promise: field("promise"),
                exchange: field("exchange"),
                occurrence: commitment["occurrence"].as_str().map(str::to_string),
            }),
            ..Default::default()
        });
    }
    if forms.is_empty() {
        return Err("There are no remaining changes for this scenario".into());
    }
    let mut scenario = simulation::fixtures::current_database(now);
    scenario.end_ms = now
        .checked_add(horizon)
        .ok_or("Future date is out of range")?;
    scenario.checks.clear();
    let first = forms.remove(0);
    Ok(SimulationCastle {
        scenario: serde_json::to_string_pretty(&scenario)?,
        current_database: true,
        selected_cell: "current".into(),
        through: chrono::DateTime::from_timestamp_millis(scenario.end_ms)
            .ok_or("Future date is out of range")?
            .to_rfc3339(),
        transfer_form: first,
        pending_transfers: forms,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recipients_supply_private_bindings_and_their_own_checks() {
        let transfer = json!({"uid":"transfer", "revision":2});
        let promise = json!({"uid":"promise", "exchange":"exchange", "giver":"ana", "receiver":"beto", "party":"ana", "record":"anas-private-stock", "delta":-5});
        let model = prepare(&transfer, &promise, "beto", None, false).unwrap();
        assert!(model.current_database);
        assert!(model.transfer_form.pending);
        assert!(model.transfer_form.record.is_empty());
        assert!(model.transfer_form.incoming);
        let scenario: simulation::scenario::Scenario =
            serde_json::from_str(&model.scenario).unwrap();
        assert!(scenario.inputs.is_empty());
        assert!(scenario.checks.is_empty());
        assert_eq!(runner::through(&model.through).unwrap(), scenario.end_ms);
        assert!(prepare(&transfer, &promise, "dora", None, false).is_err());
        assert!(
            !serde_json::to_string(&model)
                .unwrap()
                .contains("anas-private-stock")
        );
    }
    #[test]
    fn balance_scenarios_select_remaining_work_and_one_alternative() {
        let item = |transfer: &str, alternative: Option<&str>, amount: &str, outgoing: bool| {
            json!({
                "transfer":transfer,"revision":2,"promise":format!("{transfer}-promise"),"exchange":format!("{transfer}-exchange"),
                "occurrence":format!("{transfer}-occurrence"),"person":"ana","record":"stock","state":"agreed",
                "outgoing":outgoing,"unit":null,"canonical_remaining":nucleus::DecimalValue::parse_inferred(amount).unwrap(),
                "private_remaining":nucleus::DecimalValue::parse_inferred(amount).unwrap(),"alternative_group":alternative,
                "due":"2026-09-30T12:00:00Z"
            })
        };
        let balance = json!({"selected_transfer":"apples","incomplete":[],"commitments":[
            item("apples",Some("offer"),"6",true),item("other-offer",Some("offer"),"10",true),item("income",None,"5",false)
        ]});
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-29T12:00:00Z")
            .unwrap()
            .timestamp_millis();
        let model = prepare_balance(&balance, now).unwrap();
        assert_eq!(model.transfer_form.amount, "6");
        assert!(!model.transfer_form.incoming);
        assert_eq!(
            model
                .transfer_form
                .source
                .as_ref()
                .unwrap()
                .occurrence
                .as_deref(),
            Some("apples-occurrence")
        );
        assert_eq!(model.pending_transfers.len(), 1);
        assert!(model.pending_transfers[0].incoming);
        assert_eq!(model.pending_transfers[0].hours, "24");
        let scenario: simulation::scenario::Scenario =
            serde_json::from_str(&model.scenario).unwrap();
        assert!(scenario.inputs.is_empty());
        assert!(scenario.checks.is_empty());
        let mut incomplete = balance;
        incomplete["incomplete"] = json!(["Missing unit"]);
        assert!(prepare_balance(&incomplete, now).is_err());
    }
    #[test]
    fn parent_scenarios_include_each_private_remainder_once() {
        let commitment = serde_json::json!({"transfer":"child","revision":3,"promise":"promise","exchange":"exchange",
            "occurrence":"occurrence","person":"ana","record":"stock","title":"Apples","state":"proposed",
            "outgoing":true,"canonical_remaining":nucleus::DecimalValue::parse_inferred("6").unwrap()});
        let balance = serde_json::json!({"commitments":[commitment],"incomplete":[]});
        let application = serde_json::json!({"person":"ana","balance":balance});
        let rows = vec![
            serde_json::json!({"uid":"parent","children":[{"uid":"child","required":true},{"uid":"optional","required":false}]}),
            serde_json::json!({"uid":"child","children":[],"private_applications":[application,application]}),
        ];
        let selected = group_balance(&rows, "parent", "ana").unwrap();
        assert_eq!(selected["commitments"].as_array().unwrap().len(), 1);
        let model = prepare_balance(&selected, 0).unwrap();
        assert_eq!(model.transfer_form.amount, "6");
        assert_eq!(
            model.transfer_form.source.unwrap().occurrence.as_deref(),
            Some("occurrence")
        );
        assert!(model.pending_transfers.is_empty());
        assert!(group_balance(&rows[..1], "parent", "ana").is_err());
        let mut hidden = rows;
        hidden[0]["children_details_hidden"] = serde_json::json!(true);
        assert!(group_balance(&hidden, "parent", "ana").is_err());
    }

    #[test]
    fn loan_timing_scenario_does_not_assume_another_receipt() {
        let until = nucleus::execution::now().timestamp_millis() + 3 * 86_400_000;
        let transfer = json!({"uid":"loan","revision":2});
        let promise = json!({"uid":"promise","title":"Bike","exchange":"bike","giver":"ana","receiver":"beto","delta":1,"unit":null,
            "accepted_loan":{"origin":"ana-organ","revision":2,"until_ms":until}});
        let model = prepare(&transfer, &promise, "beto", None, true).unwrap();
        let timing = model.transfer_form.loan.unwrap();
        assert_eq!(timing.source.transfer, "loan");
        assert_eq!(model.transfer_form.hours, "0");
        assert!(timing.returned.is_none());
        let scenario: simulation::scenario::Scenario =
            serde_json::from_str(&model.scenario).unwrap();
        assert!(scenario.inputs.is_empty());
        assert!(scenario.end_ms > until);
        assert!(
            prepare(&transfer, &promise, "beto", None, false)
                .unwrap()
                .transfer_form
                .loan
                .is_none()
        );
    }
}
