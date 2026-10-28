use super::*;

pub fn lifecycle_forms(transfer: &Value, person: &str) -> Vec<Form> {
    let mut forms = Vec::new();
    if capability(transfer, "edit_terms") && transfer["visibility"] == "hidden" {
        let mut form = Form::action(
            "Discard unused draft",
            action_base(transfer, "discard-transfer-draft", person),
        );
        form.review = json!({"Operation":"Discard this unpublished, unaddressed draft. Signed history is retained; eligibility and revision are checked again when applying."});
        forms.push(form);
    }
    for occurrence in array(transfer, "occurrences") {
        if capability(occurrence, "propose_cancellation")
            && occurrence["action_payloads"]["propose_cancellation"].is_object()
        {
            let mut form = Form::action(
                "Propose cancelling the remainder",
                occurrence["action_payloads"]["propose_cancellation"].clone(),
            );
            form.review = json!({"Occurrence":occurrence["uid"],"Operation":"Propose cancellation of the exact remaining amount. Other parties retain their agreement choices.","Remaining amount":form.data["expected_remaining_quantity"]});
            forms.push(form);
        }
        for cancellation in array(occurrence, "cancellations") {
            if capability(cancellation, "apply_cancellation")
                && cancellation["action_payloads"]["apply_cancellation"].is_object()
            {
                let mut form = Form::action(
                    "Complete agreed cancellation",
                    cancellation["action_payloads"]["apply_cancellation"].clone(),
                );
                form.review = json!({"Cancellation":cancellation["uid"],"Amount":cancellation["quantity"],"Operation":"Apply the agreed cancellation while retaining signed history."});
                forms.push(form);
            }
        }
    }
    for form in &mut forms {
        form.actor = Some(person.into());
        form.transfer = transfer["uid"].as_str().map(str::to_owned);
        form.revision = transfer["revision"].as_u64();
        form.data["person"] = json!(person);
    }
    forms
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn disposal_and_cancellation_keep_capabilities_revisions_and_exact_remainders() {
        let mut transfer = json!({"uid":"trip","revision":7,"visibility":"hidden",
        "capabilities":{"edit_terms":true},"occurrences":[{
            "uid":"delivery","capabilities":{"propose_cancellation":true},
            "action_payloads":{"propose_cancellation":{"action":"propose-transfer-cancellation","transfer":"trip","occurrence":"delivery","expected_revision":7,"expected_remaining_quantity":"0.000000000000000001"}},
            "cancellations":[]
        }]});
        let forms = lifecycle_forms(&transfer, "me");
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].data["action"], "discard-transfer-draft");
        let cancellation = forms[1].payload().unwrap();
        assert_eq!(
            cancellation["expected_remaining_quantity"],
            "0.000000000000000001"
        );
        assert_eq!(cancellation["expected_revision"], 7);
        assert_eq!(cancellation["person"], "me");
        assert_eq!(forms[1].revision, Some(7));
        transfer["visibility"] = json!("participants");
        transfer["occurrences"][0]["capabilities"]["propose_cancellation"] = json!(false);
        assert!(lifecycle_forms(&transfer, "me").is_empty());
    }
}

pub fn outcome_forms(transfer: &Value, person: &str) -> Vec<Form> {
    let mut forms = Vec::new();
    for promise in array(transfer, "promises") {
        if capability(promise, "activate") {
            let mut action = action_base(transfer, "activate-transfer-occurrence", person);
            action["promise"] = promise["uid"].clone();
            action["original_authorities"] = json!([]);
            let mut form = Form::action("Activate occurrence", action);
            form.review = json!({"Item":promise,"Original authority":"No optional Organ authority approvals are attached. Activation does not confirm physical delivery; later approval cannot rewrite its original evidence."});
            forms.push(form);
        }
    }
    for occurrence in array(transfer, "occurrences") {
        for role in ["delivery", "receipt"] {
            let claimed = occurrence[role]["claimed"] == true;
            if capability(
                occurrence,
                &format!("{}_{}", if claimed { "correct" } else { "confirm" }, role),
            ) {
                let mut action = action_base(transfer, "set-transfer-occurrence-claim", person);
                action["occurrence"] = occurrence["uid"].clone();
                action["role"] = json!(role);
                action["claimed"] = json!(!claimed);
                let mut form = Form::action(
                    &format!("{} {role}", if claimed { "Retract" } else { "Confirm" }),
                    action,
                );
                form.review = json!({"Occurrence":occurrence["uid"],"Role":role,"Claimed":!claimed,"Amount":occurrence["quantity"],"Meaning":"This is your signed report; it does not sign for the other party or settle the Transfer."});
                forms.push(form);
            }
        }
        for (capability_name, title, disputed) in [
            ("dispute", "Raise dispute", true),
            ("retract_dispute", "Retract dispute", false),
        ] {
            if capability(occurrence, capability_name) {
                let mut action = action_base(transfer, "set-transfer-occurrence-dispute", person);
                action["occurrence"] = occurrence["uid"].clone();
                action["disputed"] = json!(disputed);
                let mut form = Form::action(title, action);
                form.review = json!({"Occurrence":occurrence["uid"],"Disputed":disputed});
                forms.push(form);
            }
        }
        if capability(occurrence, "settle") {
            let mut action = action_base(transfer, "settle-transfer-occurrence", person);
            action["occurrence"] = occurrence["uid"].clone();
            action["canonical_quantity"] =
                occurrence["settlement_progress"]["remaining_quantity"].clone();
            let mut form = Form::action("Review settlement", action);
            form.mode = "needs-settlement-review".into();
            form.field(
                "/canonical_quantity",
                "Agreed amount to settle",
                FieldKind::Number,
            );
            forms.push(form);
        }
    }
    for form in &mut forms {
        form.actor = Some(person.into());
        form.transfer = transfer["uid"].as_str().map(str::to_owned);
        form.revision = transfer["revision"].as_u64();
    }
    forms
}

pub fn reviewed_payload(form: &Form, preview: &Value, actor: &str) -> Result<Value, String> {
    let mut action = form.data.clone();
    if text(preview, "person") != actor || actor.is_empty() {
        return Err("This review belongs to a different Person. Choose the Person whose signer is installed.".into());
    }
    if action["action"] == "complete-transfer-occurrence-claims-bulk" {
        let items: Vec<_> = array(preview, "items")
            .iter()
            .filter_map(|item| {
                item["action"]
                    .as_object()
                    .map(|action| Value::Object(action.clone()))
            })
            .collect();
        if preview["eligible"] != true
            || items.is_empty()
            || items.len() != array(preview, "items").len()
            || text(preview, "review_token").is_empty()
        {
            return Err("The selection contains blocked or missing claims".into());
        }
        action["review_token"] = preview["review_token"].clone();
        action["items"] = json!(items);
    } else {
        if !capability(preview, "settle")
            || preview["canonical_quantity"].as_f64() != action["canonical_quantity"].as_f64()
            || preview["occurrence"] != action["occurrence"]
        {
            return Err("The quantity or occurrence changed. Request a fresh review.".into());
        }
        for (expected, actual) in [
            ("expected_remaining_quantity", "remaining_quantity"),
            ("expected_local_delta", "local_delta"),
            (
                "expected_application_formula_hash",
                "application_formula_hash",
            ),
            (
                "expected_application_formula_version",
                "application_formula_version",
            ),
            ("expected_remainder_policy", "remainder_policy"),
        ] {
            if preview[expected].is_null() || preview[expected] != preview[actual] {
                return Err("The Cell returned an incomplete settlement review".into());
            }
            action[expected] = preview[expected].clone();
        }
        action["expected_effects_hash"] = preview["expected_effects_hash"].clone();
        if preview["effects"]
            .as_array()
            .is_some_and(|effects| !effects.is_empty())
            && preview["expected_effects_hash"]
                .as_str()
                .is_none_or(str::is_empty)
        {
            return Err("The Cell returned an incomplete group review".into());
        }
    }
    Ok(action)
}
pub fn action_base(transfer: &Value, action: &str, person: &str) -> Value {
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

pub fn agreement(
    snapshot: &nucleus::transfer::karma::Snapshot,
    person: &str,
    level: u8,
) -> Option<Value> {
    let participant = snapshot.participants.get(person)?;
    let mut guard = nucleus::transfer::karma::Guard::base(snapshot);
    guard
        .participants
        .insert(person.into(), participant.clone());
    Some(
        json!({"action":"assign-transfer-agreement-level","transfer":snapshot.transfer,"person":person,"expected_revision":snapshot.revision,"request_id":nucleus::new_uid("agreement-target"),"level":level,"expected":participant.guard,"expected_state":guard}),
    )
}
