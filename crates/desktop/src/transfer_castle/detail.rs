use super::*;
use model::FieldKind;
use ui::{Command, button, row, stack};

fn section(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    key: &str,
    label: &str,
    open: bool,
) -> Option<Entity> {
    let block = stack(world, parent);
    world.get_mut::<Node>(block).unwrap().padding = UiRect::all(px(10));
    world
        .entity_mut(block)
        .insert(crate::token_style::background(
            crate::tokens::Token::CanvasBackground,
        ));
    let expanded = world.get::<View>(owner).unwrap().expanded.contains(key) != open;
    button(
        world,
        block,
        owner,
        &format!("{} {label}", if expanded { "−" } else { "+" }),
        Command::Toggle(key.into()),
    );
    expanded.then_some(block)
}

fn evidence(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    key: &str,
    label: &str,
    value: &Value,
) {
    if !value.is_null()
        && let Some(block) = section(world, owner, parent, key, label, false)
    {
        forms::review(world, block, value, 0);
    }
}

fn fact(world: &mut World, parent: Entity, label: &str, value: impl AsRef<str>) {
    let value = value.as_ref();
    if !value.is_empty() {
        crate::edit_mode::label(world, parent, &format!("{label}: {value}"), 13.0);
    }
}

fn blockers(world: &mut World, owner: Entity, parent: Entity, key: &str, subject: &Value) {
    evidence(
        world,
        owner,
        parent,
        &format!("blockers-{key}"),
        "Availability and blockers",
        &subject["blocking_reasons"],
    );
}

fn base(world: &World, owner: Entity, transfer: &Value, action: &str) -> Value {
    action_base(transfer, action, &person(world, owner))
}

pub(super) fn action_base(transfer: &Value, action: &str, person: &str) -> Value {
    let mut payload = json!({"action":action,"transfer":transfer["uid"],"expected_revision":transfer["revision"],"person":person});
    if matches!(
        action,
        "create-transfer-thread"
            | "create-transfer-message"
            | "reject-transfer-invitation"
            | "set-transfer-occurrence-claim"
            | "set-transfer-occurrence-dispute"
            | "set-transfer-occurrence-application-formula"
            | "settle-transfer-occurrence"
            | "compensate-transfer-occurrence-settlement"
    ) {
        payload.as_object_mut().unwrap().remove("expected_revision");
    }
    if matches!(
        action,
        "withdraw-transfer-invitation"
            | "reopen-transfer-invitation"
            | "set-transfer-occurrence-claim"
            | "set-transfer-occurrence-dispute"
            | "set-transfer-occurrence-application-formula"
            | "settle-transfer-occurrence"
            | "compensate-transfer-occurrence-settlement"
    ) {
        payload.as_object_mut().unwrap().remove("transfer");
    }
    if matches!(
        action,
        "withdraw-transfer-invitation" | "reopen-transfer-invitation"
    ) {
        payload.as_object_mut().unwrap().remove("person");
    }
    payload
}

fn offer(world: &mut World, owner: Entity, parent: Entity, subject: &Value, cap: &str, form: Form) {
    if capability(subject, cap) {
        let label = form.title.clone();
        button(world, parent, owner, &label, Command::Open(form));
    }
}

fn field(form: &mut Form, path: &str, label: &str, kind: FieldKind) {
    if form.data.pointer(path).is_none() {
        form.data[path.trim_start_matches('/')] = Value::Null;
    }
    form.field(path, label, kind);
}

fn choice(values: &[&str]) -> FieldKind {
    FieldKind::Choice(values.iter().map(|value| value.to_string()).collect())
}

pub(super) fn render(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let heading = row(world, parent);
    button(
        world,
        heading,
        owner,
        "Back",
        Command::Select(String::new()),
    );
    crate::edit_mode::label(world, heading, &title(transfer), 21.0);
    if let Some(cell) = transfer["writing_cell_name"].as_str().or(transfer["writing_cell"].as_str()) {
        crate::edit_mode::label(world, parent, &format!("Open Cell {cell} to change this Transfer. You can simulate it here."), 13.0);
    }

    if capability(&world.get::<View>(owner).unwrap().context, "create") {
        button(
            world,
            heading,
            owner,
            "Use this outcome",
            Command::Open(model::observer(&person(world, owner), transfer)),
        );
    }
    if capability(transfer, "edit_terms") || capability(transfer, "adopt_terms") {
        button(world, heading, owner, "Edit terms", Command::Edit(false));
    }
    if capability(transfer, "edit_terms") && transfer["visibility"] == "hidden" {
        let mut form = Form::action("Discard unused draft", base(world, owner, transfer, "discard-transfer-draft"));
        form.review = json!({"operation":"Discard this unpublished, unaddressed draft. Signed history is retained; eligibility and revision are checked again when applying."});
        button(world, heading, owner, "Discard unused draft", Command::Open(form));
    }
    if capability(transfer, "counteroffer") {
        button(world, heading, owner, "Counteroffer", Command::Edit(true));
    }
    if capability(transfer, "edit_terms") {
        forms::disclosure_preview(world, owner, parent, transfer, false);
    }
    fact(
        world,
        parent,
        "Status",
        model::human(&model::status(transfer)),
    );
    for (key, label) in [
        ("revision", "Revision"),
        ("visibility", "Visibility"),
        ("agreement_type", "Agreement"),
        ("settlement", "Settlement"),
        ("reserve_default", "Default reserve"),
    ] {
        fact(world, parent, label, model::human(&text(transfer, key)));
    }
    fact(
        world,
        parent,
        "Confirmation",
        if transfer["require_confirmation"] == true {
            "Delivery and receipt"
        } else {
            "Not required"
        },
    );
    if !array(transfer, "private_applications").is_empty()
        && let Some(block) = section(
            world,
            owner,
            parent,
            "private_applications",
            "Changes to my Records",
            true,
        )
    {
        for application in array(transfer, "private_applications") {
            fact(
                world,
                block,
                "Agreed amount",
                text(application, "agreed_amount"),
            );
            for effect in array(
                &application["action_payloads"]["set_private_policy"],
                "effects",
            ) {
                fact(world, block, "My Record or Need", text(effect, "record"));
                fact(world, block, "How it changes", text(effect, "mode"));
                fact(world, block, "Formula", text(effect, "formula"));
            }
            if array(application, "balances").len() > 1 {
                forms::review(world, block, &application["balances"], 0);
            }
            if application["stock_limit"]["current"].is_object() {
                fact(
                    world,
                    block,
                    "Hard stock limit",
                    "Reserved stock is protected on the writing Cell",
                );
                fact(
                    world,
                    block,
                    "Minimum surplus",
                    text(&application["stock_limit"]["current"]["minimum"], "value"),
                );
            }
            let balance = &application["balance"];
            if balance.is_object() {
                for (key, label) in [
                    ("actual", "Actual"),
                    ("available", "Available now"),
                    ("reserved", "Reserved"),
                    ("surplus", "Surplus"),
                    ("can_offer", "Can offer now"),
                ] {
                    fact(world, block, label, text(&balance[key], "value"));
                }
                for reason in array(balance, "incomplete") {
                    fact(
                        world,
                        block,
                        "Balance incomplete",
                        reason.as_str().unwrap_or_default(),
                    );
                }
                for commitment in array(balance, "commitments") {
                    button(
                        world,
                        block,
                        owner,
                        &format!(
                            "{} remaining · {}",
                            text(&commitment["canonical_remaining"], "value"),
                            text(commitment, "title")
                        ),
                        Command::Select(text(commitment, "transfer")),
                    );
                }
                let mut scenario = balance.clone();
                scenario["selected_transfer"] = transfer["uid"].clone();
                button(
                    world,
                    block,
                    owner,
                    "Simulate remaining changes and future dates",
                    Command::SimulateBalance(scenario),
                );
            }
            projected_actions(world, owner, block, application);
        }
    }
    if let Some(block) = section(world, owner, parent, "agreement", "Agreement", true) {
        forms::review(world, block, &transfer["agreement"], 0);
        for party in array(transfer, "parties") {
            fact(
                world,
                block,
                &text(party, "actor_head"),
                text(party, "level_label"),
            );
            let controls = row(world, block);
            let level = party["level"].as_u64().unwrap_or(0);
            for (cap, label, target) in [
                ("review", "Checked · ready to agree", 1),
                ("commit", "Agree", 2),
                ("agreement_back", "Step back", level.saturating_sub(1)),
            ] {
                let mut action = base(world, owner, transfer, "set-transfer-agreement-level");
                action["person"] = party["actor"].clone();
                action["level"] = json!(target);
                offer(
                    world,
                    owner,
                    controls,
                    party,
                    cap,
                    Form::action(label, action),
                );
            }
            for target in 0..=2 {
                let permitted = u64::from(target) == level
                    || (u64::from(target) < level && capability(party, "agreement_back"))
                    || (u64::from(target) > level && capability(party, if level == 0 { "review" } else { "commit" }));
                if permitted && let Some(action) = karma::target_action(world, owner, transfer, &text(party, "actor"), target) {
                    let label = format!("Set level {target}");
                    button(world, controls, owner, &label, Command::Open(Form::action(&label, action)));
                }
            }
            blockers(world, owner, block, &text(party, "uid"), party);
        }
        forms::review(world, block, &transfer["readiness"], 0);
        karma::render(world, owner, block, transfer);
    }
    invitations(world, owner, parent, transfer);
    promises(world, owner, parent, transfer);
    occurrences(world, owner, parent, transfer);
    if transfer["agreement_type"] == "dependency" {
        crate::edit_mode::label(
            world,
            parent,
            "Agreement follows the required outcomes. Your agreement level does not decide their result.",
            13.0,
        );
        if transfer["observed_fulfilment"].is_object() {
            crate::edit_mode::label(
                world,
                parent,
                if transfer["observed_fulfilment"]["fulfilled"] == true {
                    "Commitment fulfilled by the observed outcome"
                } else {
                    "Waiting for the observed outcome"
                },
                14.0,
            );
        }
        for dependency in array(transfer, "dependencies") {
            let required = text(dependency, "required_state");
            let state = text(dependency, "actual_state");
            crate::edit_mode::label(
                world,
                parent,
                &format!(
                    "Requires {required} · {}",
                    if state.is_empty() {
                        "Unavailable"
                    } else {
                        &state
                    }
                ),
                13.0,
            );
            let reason = text(dependency, "blocking_reason");
            if !reason.is_empty() {
                crate::edit_mode::label(world, parent, &model::human(&reason), 13.0);
            }
            if dependency["blocking_reason"] == "upstream_evidence_stale" {
                crate::edit_mode::label(
                    world,
                    parent,
                    "Refresh the source Transfer to check its current outcome",
                    13.0,
                );
            }
            if dependency["upstream_kind"] == "transfer" {
                let upstream = text(dependency, "upstream");
                if world
                    .get::<View>(owner)
                    .unwrap()
                    .rows
                    .iter()
                    .any(|row| row["uid"] == upstream)
                {
                    button(
                        world,
                        parent,
                        owner,
                        "Open required Transfer",
                        Command::Select(upstream),
                    );
                }
            }
        }
    }
    delivery(world, owner, parent, transfer);
    negotiation(world, owner, parent, transfer);
    hierarchy(world, owner, parent, transfer);
    for (key, label) in [
        ("dependencies", "Required outcomes and evidence"),
        ("observed_fulfilment", "Observed fulfilment"),
        ("balance_detail", "Balance by concept"),
        ("balance", "Balance"),
        ("settlement_progress", "Settlement progress"),
        ("timeline", "Timeline and proof"),
        ("revision_evidence", "Signed revisions and changed terms"),
        ("proof", "Current proof"),
        ("confirmations", "Confirmation evidence"),
        ("visibility_projection", "Recipients and disclosure"),
        ("correction_lineage", "Corrections and lineage"),
        ("first_completes_evidence", "Source group completion"),
    ] {
        evidence(world, owner, parent, key, label, &transfer[key]);
    }
    blockers(world, owner, parent, "transfer", transfer);
}

fn invitations(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let Some(block) = section(
        world,
        owner,
        parent,
        "invitations",
        &format!("Invitations ({})", array(transfer, "invitations").len()),
        true,
    ) else {
        return;
    };
    for invitation in array(transfer, "invitations") {
        fact(
            world,
            block,
            &text(invitation, "addressed_person_head"),
            text(invitation, "status"),
        );
        fact(world, block, "Expires", text(invitation, "expires_at"));
        let controls = row(world, block);
        for (cap, label, verb) in [
            ("accept", "Accept", "accept-transfer-invitation"),
            ("reject", "Reject", "reject-transfer-invitation"),
            ("withdraw", "Withdraw", "withdraw-transfer-invitation"),
            ("reopen", "Reopen", "reopen-transfer-invitation"),
        ] {
            let mut action = base(world, owner, transfer, verb);
            action["invitation"] = invitation["uid"].clone();
            if matches!(cap, "accept" | "reject") {
                action["person"] = invitation["addressed_person"].clone();
            }
            let mut form = Form::action(label, action);
            if cap == "reopen" {
                field(
                    &mut form,
                    "/expires_at",
                    "New expiry (date, time, timezone)",
                    FieldKind::Optional,
                );
            }
            offer(world, owner, controls, invitation, cap, form);
        }
        evidence(
            world,
            owner,
            block,
            &format!("invitation-{}", text(invitation, "uid")),
            "Invitation history",
            &invitation["attempts"],
        );
        blockers(world, owner, block, &text(invitation, "uid"), invitation);
    }
    let mut form = Form::action(
        "Invite a Person",
        base(world, owner, transfer, "address-transfer-invitation"),
    );
    form.data["person"] = Value::Null;
    field(
        &mut form,
        "/person",
        "Invited Person",
        FieldKind::Reference("person".into()),
    );
    field(
        &mut form,
        "/expires_at",
        "Expiry (optional date, time, timezone)",
        FieldKind::Optional,
    );
    offer(world, owner, block, transfer, "address_invitation", form);
}

fn promises(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let Some(block) = section(
        world,
        owner,
        parent,
        "promises",
        &format!("Promises ({})", array(transfer, "promises").len()),
        true,
    ) else {
        return;
    };
    for promise in array(transfer, "promises") {
        if promise["giver"] == person(world, owner) || promise["receiver"] == person(world, owner) {
            button(
                world,
                block,
                owner,
                "Simulate this item",
                Command::Simulate(text(promise, "uid")),
            );
        }
        let uid = text(promise, "uid");
        let item = stack(world, block);
        if promise["loan"].is_object() {
            fact(world, item, "Loan", text(&promise["loan_status"], "state"));
            fact(
                world,
                item,
                "Proposed loan starts",
                text(&promise["loan"], "from"),
            );
            fact(
                world,
                item,
                "Proposed loan ends",
                text(&promise["loan"], "until"),
            );
            if let Some(until) = promise["accepted_loan"]["until_ms"].as_i64() {
                fact(
                    world,
                    item,
                    "Accepted deadline",
                    chrono::DateTime::from_timestamp_millis(until)
                        .map(|date| date.to_rfc3339())
                        .unwrap_or_default(),
                );
                crate::edit_mode::label(
                    world,
                    item,
                    "The Need is projected to reopen at the deadline. A return requires delivery, receipt and settlement.",
                    12.0,
                );
                let acting = person(world, owner);
                if promise["giver"] == acting || promise["receiver"] == acting {
                    button(
                        world,
                        item,
                        owner,
                        "Propose return",
                        Command::Open(model::loan_proposal(transfer, promise, &acting, false)),
                    );
                    if promise["receiver"] == acting {
                        button(
                            world,
                            item,
                            owner,
                            "Simulate loan dates or early return",
                            Command::SimulateLoan(text(promise, "uid")),
                        );
                        button(
                            world,
                            item,
                            owner,
                            "Publish future Need",
                            Command::Open(model::loan_proposal(transfer, promise, &acting, true)),
                        );
                    }
                    let mut extension = Form::action(
                        "Propose extension",
                        json!({"action":"propose-transfer-loan-extension","transfer":transfer["uid"],"exchange":promise["exchange"],"until":promise["loan"]["until"],"expected_revision":transfer["revision"],"person":acting,"request_id":null}),
                    );
                    extension.field(
                        "/until",
                        "New deadline (date, time, timezone)",
                        FieldKind::Text,
                    );
                    button(
                        world,
                        item,
                        owner,
                        "Propose extension",
                        Command::Open(extension),
                    );
                }
            }
        }
        let amount = promise["delta"]
            .as_f64()
            .map(|delta| {
                format!(
                    "{} {} {}",
                    if delta < 0.0 { "Gives" } else { "Receives" },
                    delta.abs(),
                    text(promise, "unit_name")
                )
            })
            .unwrap_or_else(|| "Quantity unavailable".into());
        crate::edit_mode::label(
            world,
            item,
            &format!("{} · {}", title(promise), amount),
            15.0,
        );
        if let Some(description) = promise["description"].as_str() {
            crate::edit_mode::label(world, item, description, 13.0);
        } else if promise.pointer("/disclosed/description") == Some(&Value::Bool(false)) {
            crate::edit_mode::label(world, item, "Description unavailable", 13.0);
        }
        fact(world, item, "State", text(promise, "state"));
        if !promise["exchange"].is_null() {
            fact(world, item, "Exchange", text(promise, "exchange"));
            fact(
                world,
                item,
                "From",
                promise["giver"].as_str().unwrap_or("Unavailable"),
            );
            fact(
                world,
                item,
                "To",
                promise["receiver"].as_str().unwrap_or("Unavailable"),
            );
        }
        fact(
            world,
            item,
            "Person",
            if promise["open"] == true {
                "OPEN".into()
            } else {
                text(promise, "party")
            },
        );
        fact(world, item, "Deadline", text(promise, "window_end"));
        fact(world, item, "Condition", text(promise, "condition"));
        let controls = row(world, item);
        if let Some(record) = promise["record"].as_str() {
            button(
                world,
                controls,
                owner,
                "Open record",
                Command::Record(record.into()),
            );
        }
        let mut action = base(world, owner, transfer, "activate-transfer-occurrence");
        action["promise"] = promise["uid"].clone();
        offer(
            world,
            owner,
            controls,
            promise,
            "activate",
            Form::action("Activate occurrence", action),
        );
        let mut action = base(world, owner, transfer, "claim-open-transfer-promise");
        action["promise"] = promise["uid"].clone();
        let mut terms = model::promise(&person(world, owner));
        terms["party"] = Value::Null;
        terms["delta"] = promise["delta"]
            .as_f64()
            .map(|delta| json!(-delta))
            .unwrap_or(Value::Null);
        terms["unit"] = promise["unit"].clone();
        terms["window_start"] = promise["window_start"].clone();
        terms["window_end"] = promise["window_end"].clone();
        terms["place"] = promise["place"].clone();
        terms["open"] = json!(false);
        terms["reuse_policy"] = promise["reuse_policy"].clone();
        action["terms"] = terms;
        let mut form = Form::action("Claim OPEN promise", action);
        form.field(
            "/person",
            "Claiming Person",
            FieldKind::Reference("person".into()),
        );
        form.field(
            "/terms/delta",
            "Your quantity (opposite direction)",
            FieldKind::Number,
        );
        form.field(
            "/terms/unit",
            "Your unit",
            FieldKind::Reference("unit".into()),
        );
        offer(world, owner, controls, promise, "claim", form);
        let mut action = base(world, owner, transfer, "reopen-transfer-promise");
        action["promise"] = promise["uid"].clone();
        action["open"] = json!(false);
        let mut form = Form::action("Reopen promise", action);
        field(
            &mut form,
            "/window_end",
            "New deadline (date, time, timezone)",
            FieldKind::Optional,
        );
        field(
            &mut form,
            "/open",
            "Reopen as OPEN",
            choice(&["false", "true"]),
        );
        offer(world, owner, controls, promise, "reopen", form);
        for (key, label) in [
            ("availability", "Availability"),
            ("place", "Place"),
            ("claim_pairs", "OPEN claim pairs"),
            ("predecessor", "Predecessor"),
            ("successor", "Successor"),
        ] {
            evidence(
                world,
                owner,
                item,
                &format!("{uid}-{key}"),
                label,
                &promise[key],
            );
        }
        blockers(world, owner, item, &uid, promise);
    }
}

fn occurrences(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let Some(block) = section(
        world,
        owner,
        parent,
        "occurrences",
        &format!("Occurrences ({})", array(transfer, "occurrences").len()),
        true,
    ) else {
        return;
    };
    for occurrence in array(transfer, "occurrences") {
        let uid = text(occurrence, "uid");
        let item = stack(world, block);
        crate::edit_mode::label(
            world,
            item,
            &format!("{} · {}", text(occurrence, "path"), uid),
            15.0,
        );
        for (key, label) in [
            ("exchange", "Exchange"),
            ("quantity", "Quantity"),
            ("giver", "Giver"),
            ("receiver", "Receiver"),
            ("window_start", "Start"),
            ("window_end", "Deadline"),
        ] {
            fact(world, item, label, text(occurrence, key));
        }
        let progress = &occurrence["settlement_progress"];
        fact(
            world,
            item,
            "Settlement",
            format!(
                "{} settled · {} remaining",
                text(progress, "settled_quantity"),
                text(progress, "remaining_quantity")
            ),
        );
        if occurrence["system_disputed"] == true || occurrence["system_dispute"]["disputed"] == true
        {
            crate::edit_mode::label(world, item, "System safety hold", 14.0);
        }
        if occurrence["participant_disputed"] == true {
            crate::edit_mode::label(world, item, "Participant dispute", 14.0);
        }
        let controls = row(world, item);
        for role in ["delivery", "receipt"] {
            let claimed = occurrence[role]["claimed"] == true;
            let cap = format!("{}_{}", if claimed { "correct" } else { "confirm" }, role);
            let mut action = base(world, owner, transfer, "set-transfer-occurrence-claim");
            action["occurrence"] = json!(uid);
            action["role"] = json!(role);
            action["claimed"] = json!(!claimed);
            if world.get::<View>(owner).unwrap().context["viewer"]["local"] == true {
                action["person"] = occurrence[if role == "delivery" {
                    "giver"
                } else {
                    "receiver"
                }]
                .clone();
            }
            offer(
                world,
                owner,
                controls,
                occurrence,
                &cap,
                Form::action(
                    &format!("{} {role}", if claimed { "Retract" } else { "Confirm" }),
                    action,
                ),
            );
        }
        if capability(occurrence, "confirm_delivery") || capability(occurrence, "confirm_receipt") {
            let selected = world
                .get::<View>(owner)
                .unwrap()
                .selected_occurrences
                .contains(&uid);
            button(
                world,
                controls,
                owner,
                if selected {
                    "☑ Bulk selection"
                } else {
                    "☐ Bulk selection"
                },
                Command::SelectOccurrence(uid.clone()),
            );
        }
        for (cap, label, disputed) in [
            ("dispute", "Raise dispute", true),
            ("retract_dispute", "Retract dispute", false),
        ] {
            let mut action = base(world, owner, transfer, "set-transfer-occurrence-dispute");
            action["occurrence"] = json!(uid);
            action["disputed"] = json!(disputed);
            offer(
                world,
                owner,
                controls,
                occurrence,
                cap,
                Form::action(label, action),
            );
        }

        if capability(occurrence, "settle") && occurrence["settlement_preview"].is_object() {
            let mut action = base(world, owner, transfer, "settle-transfer-occurrence");
            action["occurrence"] = json!(uid);
            action["canonical_quantity"] = progress["remaining_quantity"].clone();
            let mut form = Form::action("Review settlement", action);
            field(
                &mut form,
                "/canonical_quantity",
                "Agreed amount to settle",
                FieldKind::Number,
            );
            button(
                world,
                controls,
                owner,
                "Review settlement",
                Command::Open(form),
            );
        }
        let remote = &occurrence["application_preparation"];
        if capability(remote, "begin") {
            let action = remote["action_payload"].clone();
            if action.is_object() {
                let mut form = Form::action("Prepare my Record changes", action);
                field(
                    &mut form,
                    "/canonical_quantity",
                    "Agreed amount",
                    FieldKind::Number,
                );
                button(
                    world,
                    controls,
                    owner,
                    "Prepare my Record changes",
                    Command::Open(form),
                );
            }
        }
        for (cap, label) in [
            ("propose_cancellation", "Propose cancelling the remainder"),
            ("create_remainder_draft", "Create remainder draft"),
            ("create_reversing_transfer", "Create reversing transfer"),
        ] {
            let action = occurrence["action_payloads"][cap].clone();
            if action.is_object() {
                let mut form = Form::action(label, action);
                if cap == "create_reversing_transfer" {
                    field(
                        &mut form,
                        "/canonical_quantity",
                        "Quantity to reverse",
                        FieldKind::Number,
                    );
                }
                offer(world, owner, controls, occurrence, cap, form);
            }
        }
        for slice in array(progress, "slices") {
            let slice_uid = text(slice, "uid");
            evidence(
                world,
                owner,
                item,
                &format!("slice-{slice_uid}"),
                &format!(
                    "Settlement {} · {}",
                    text(slice, "canonical_quantity"),
                    text(slice, "at")
                ),
                slice,
            );
            let mut action = base(
                world,
                owner,
                transfer,
                "compensate-transfer-occurrence-settlement",
            );
            action["settlement"] = slice["uid"].clone();
            offer(
                world,
                owner,
                item,
                slice,
                "compensate",
                Form::action("Compensate local application", action),
            );
        }
        fact(
            world,
            item,
            "Cancelled amount",
            text(progress, "cancelled_quantity_exact"),
        );
        for cancellation in array(occurrence, "cancellations") {
            fact(
                world,
                item,
                "Cancellation",
                format!(
                    "{} · {}",
                    text(&cancellation["quantity"], "value"),
                    text(cancellation, "status")
                ),
            );
            projected_actions(world, owner, item, cancellation);
        }
        for (key, label) in [
            ("availability", "Availability"),
            ("activation", "Activation evidence"),
            ("delivery", "Delivery history"),
            ("receipt", "Receipt history"),
            ("dispute", "Dispute history"),
            ("application", "Local application"),
            ("system_dispute", "System hold evidence"),
        ] {
            evidence(
                world,
                owner,
                item,
                &format!("{uid}-{key}"),
                label,
                &occurrence[key],
            );
        }
        blockers(world, owner, item, &uid, occurrence);
    }
    button(
        world,
        block,
        owner,
        "Review selected completion claims",
        Command::Bulk,
    );
}

fn delivery(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let delivery = &transfer["social_delivery"];
    let Some(block) = section(world, owner, parent, "delivery", "Social delivery", false) else {
        return;
    };
    if delivery.is_null() {
        crate::edit_mode::label(
            world,
            block,
            "Delivery is not available for this transfer yet.",
            13.0,
        );
        return;
    }
    forms::review(world, block, &delivery["authority"], 0);
    forms::review(world, block, &delivery["executor"], 0);
    forms::review(world, block, &delivery["freshness"], 0);
    projected_actions(world, owner, block, delivery);
    for recipient in array(delivery, "recipients") {
        fact(
            world,
            block,
            &text(recipient, "person_head"),
            format!(
                "{} · {} · {}",
                text(recipient, "organ_head"),
                text(recipient, "mode"),
                text(&recipient["delivery"], "status")
            ),
        );
        fact(
            world,
            block,
            "Last error",
            text(&recipient["delivery"], "last_error"),
        );
        projected_actions(world, owner, block, recipient);
    }
    for handoff in array(delivery, "application_handoffs") {
        fact(
            world,
            block,
            "Agreed amount",
            text(handoff, "canonical_quantity"),
        );
        fact(
            world,
            block,
            "Changes to my Record",
            text(&handoff["private_preview"], "local_delta_exact"),
        );
        forms::review(world, block, handoff, 0);
        projected_actions(world, owner, block, handoff);
    }
    for (key, label) in [
        ("package_receipts", "Package receipts"),
        ("command_results", "Remote command receipts"),
        ("conflicts", "Conflicts"),
        ("replica_history", "Replica history"),
        ("eligible_recipients", "Eligible recipients"),
    ] {
        evidence(world, owner, block, key, label, &delivery[key]);
    }
    blockers(world, owner, block, "delivery", delivery);
}

fn projected_actions(world: &mut World, owner: Entity, parent: Entity, subject: &Value) {
    let Some(payloads) = subject["action_payloads"].as_object() else {
        return;
    };
    for (cap, action) in payloads {
        if !action.is_object() {
            continue;
        }
        let label = match cap.as_str() {
            "apply_cancellation" => "Complete agreed cancellation".to_string(),
            "set_stock_limit" => "Prevent spending reserved stock".to_string(),
            "remove_stock_limit" => "Remove stock limit".to_string(),
            "set_private_policy" => "Choose the Needs this outcome meets".to_string(),
            "compensate" => "Correct my Record changes".to_string(),
            "apply" => "Review my Record changes".to_string(),
            _ => model::human(cap),
        };
        let mut form = Form::action(&label, action.clone());
        if cap == "apply" {
            form.review = subject["private_preview"]["effects"].clone();
        }
        for (path, label, kind) in match cap.as_str() {
            "configure_recipient" => vec![
                (
                    "/recipient_person",
                    "Recipient Person",
                    FieldKind::Reference("person".into()),
                ),
                (
                    "/recipient_organ",
                    "Recipient Organ",
                    FieldKind::Reference("organ".into()),
                ),
                ("/mode", "Delivery mode", choice(&["hosted", "replicated"])),
            ],
            "set_mode" => vec![("/mode", "Delivery mode", choice(&["hosted", "replicated"]))],
            "designate_executor" => vec![(
                "/cell_uid",
                "Executor Cell",
                FieldKind::Reference("cell".into()),
            )],
            "apply" if !form.review.is_array() => vec![(
                "/local_record",
                "My Record to update",
                FieldKind::Reference("record".into()),
            )],
            _ => Vec::new(),
        } {
            field(&mut form, path, label, kind);
        }
        offer(world, owner, parent, subject, cap, form);
    }
}

fn negotiation(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let Some(block) = section(world, owner, parent, "negotiation", "Negotiation", false) else {
        return;
    };
    for thread in array(transfer, "threads") {
        crate::edit_mode::label(world, block, &title(thread), 16.0);
        for message in array(thread, "messages") {
            let body = text(message, "body");
            if let Some(shared) = nucleus::simulation::sharing::Shared::parse(&body) {
                for (index, assumption) in shared.assumptions.iter().enumerate() {
                    button(
                        world,
                        block,
                        owner,
                        &format!("Simulate assumed {} on my database", assumption.quantity),
                        Command::SimulateShared(body.clone(), index),
                    );
                }
            }
            fact(
                world,
                block,
                &text(message, "sender"),
                text(message, "body"),
            );
            fact(world, block, "Sent", text(message, "created_at"));
            for reference in array(message, "references") {
                button(
                    world,
                    block,
                    owner,
                    &title(reference),
                    Command::Record(text(reference, "uid")),
                );
            }
        }
        let mut action = base(world, owner, transfer, "create-transfer-message");
        action["thread"] = thread["uid"].clone();
        action["body"] = json!("");
        action["references"] = json!([]);
        if let Some(body) = world
            .get::<TransferCastle>(owner)
            .unwrap()
            .shared_simulation
            .clone()
        {
            let mut shared_action = action.clone();
            shared_action["body"] = json!(body);
            let form = Form::action("Share prepared simulation here", shared_action);
            offer(world, owner, block, transfer, "create_message", form);
        }
        let mut form = Form::action("Reply", action);
        form.field("/body", "Message", FieldKind::Text);
        form.field(
            "/references",
            "Referenced records (comma-separated)",
            FieldKind::People,
        );
        field(
            &mut form,
            "/parent",
            "Reply to message (optional)",
            FieldKind::Optional,
        );
        offer(world, owner, block, transfer, "create_message", form);
    }
    let mut action = base(world, owner, transfer, "create-transfer-thread");
    action["head"] = json!("");
    let mut form = Form::action("New discussion", action);
    form.field("/head", "Discussion title", FieldKind::Text);
    offer(world, owner, block, transfer, "create_thread", form);
}

fn hierarchy(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let Some(block) = section(
        world,
        owner,
        parent,
        "hierarchy",
        "Transfer tree and branch",
        false,
    ) else {
        return;
    };
    if !array(transfer, "children").is_empty() {
        button(
            world,
            block,
            owner,
            "Simulate my remaining work in required children",
            Command::SimulateChildren,
        );
    }
    if transfer["agreement"]["derived_from_children"] == true {
        crate::edit_mode::label(
            world,
            block,
            "Agreement follows the required children",
            13.0,
        );
    }
    let rows = world.get::<View>(owner).unwrap().rows.clone();
    for (key, label) in [("parent", "Parent"), ("source", "Source")] {
        let uid = text(transfer, key);
        if !uid.is_empty() {
            button(
                world,
                block,
                owner,
                &format!("{label}: {uid}"),
                if rows.iter().any(|row| text(row, "uid") == uid) {
                    Command::Select(uid)
                } else {
                    Command::Record(uid)
                },
            );
        }
    }
    if transfer["children_details_hidden"] == true {
        crate::edit_mode::label(
            world,
            block,
            "Some required parts have private details",
            13.0,
        );
    }
    for child in array(transfer, "children") {
        let uid = text(child, "uid");
        let required = if child["required"] == true {
            "Required"
        } else {
            "Optional"
        };
        let state = if child["settled"] == true {
            "Settled"
        } else if child["ready"] == true {
            "Agreed"
        } else {
            "Waiting on agreement"
        };
        button(
            world,
            block,
            owner,
            &format!("{} · {required} · {state}", title(child)),
            if rows.iter().any(|row| text(row, "uid") == uid) {
                Command::Select(uid.clone())
            } else {
                Command::Record(uid.clone())
            },
        );
        if capability(child, "set_requirement") && child["action"].is_object() {
            let title = if child["required"] == true {
                "Make optional and reset parent agreement"
            } else {
                "Require this child and reset parent agreement"
            };
            let form = Form::action(title, child["action"].clone());
            button(world, block, owner, title, Command::Open(form));
        }
        if let Some(row) = rows.iter().find(|row| text(row, "uid") == uid) {
            for occurrence in array(row, "occurrences") {
                if capability(occurrence, "confirm_delivery")
                    || capability(occurrence, "confirm_receipt")
                {
                    let uid = text(occurrence, "uid");
                    let selected = world
                        .get::<View>(owner)
                        .unwrap()
                        .selected_occurrences
                        .contains(&uid);
                    button(
                        world,
                        block,
                        owner,
                        &format!("{} {}", if selected { "☑" } else { "☐" }, uid),
                        Command::SelectOccurrence(uid),
                    );
                }
            }
        }
    }
    button(
        world,
        block,
        owner,
        "Review selected branch claims",
        Command::Bulk,
    );
}
