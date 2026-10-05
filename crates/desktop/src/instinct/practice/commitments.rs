use super::*;
use serde_json::{Value, json};

pub(super) async fn fixture(
    runtime: &cell::CellRuntime,
    path: &std::path::Path,
    automation: bool,
) -> Result<Value, String> {
    let engine = &runtime.engine;
    let mut people = Vec::new();
    let mut signers = Vec::new();
    for (slug, name) in [
        ("practice-giver", "Practice giver"),
        ("practice-receiver", "Practice receiver"),
    ] {
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: Some(slug.into()),
                    kind: nucleus::RecordKind::Person,
                    head: name.into(),
                    body: String::new(),
                    quantity: 1.0,
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?
            .created
            .ok_or("The prepared Person was not confirmed.")?;
        let signer = engine::trust::Signer::load_or_create(
            &path.join(format!("{slug}.key")),
            &uid,
            "instinct-person",
        )
        .map_err(|error| error.to_string())?;
        people.push(uid);
        signers.push(signer);
    }
    let record = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("practice-stock".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Practice tokens".into(),
                body: "A promise is different from a confirmed change.".into(),
                quantity: 10.0,
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?
        .created
        .ok_or("The practice stock was not confirmed.")?;
    engine
        .set_signer(signers[0].clone())
        .await
        .map_err(|error| error.to_string())?;
    let promise = nucleus::new_uid("r");
    let payload = json!({"action":"create-transfer-draft","request_id":nucleus::new_uid("instinct-transfer"),"creator":people[0],"slug":"instinct-transfer","head":"Prepared token donation","agreement":"full","visibility":"hidden","reserve_default":"active","require_confirmation":true,"invitees":[people[1]],"promises":[{"uid":promise,"record":record,"party":people[0],"open":false,"delta":-1.0,"reuse_policy":"duplicate"}]});
    let action = serde_json::from_value(payload).map_err(|error| error.to_string())?;
    let transfer = engine
        .act(action, None)
        .await
        .map_err(|error| error.to_string())?
        .created
        .ok_or("The prepared proposal was not confirmed.")?;
    let invitation = store::transfers::invitations_for_transfer(&runtime.store.pool, &transfer)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|invitation| invitation.addressed_person_uid == people[1])
        .ok_or("The prepared invitation is missing.")?;
    engine
        .set_signer(signers[1].clone())
        .await
        .map_err(|error| error.to_string())?;
    let revision = store::transfers::get(&runtime.store.pool, &transfer)
        .await
        .map_err(|error| error.to_string())?
        .ok_or("The proposal is missing.")?
        .revision as u64;
    engine
        .act(
            engine::actions::Action::AcceptTransferInvitation {
                invitation: invitation.uid,
                expected_revision: revision,
                request_id: nucleus::new_uid("instinct-accept"),
                transfer: Some(transfer.clone()),
                person: Some(people[1].clone()),
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    let revision = store::transfers::get(&runtime.store.pool, &transfer)
        .await
        .map_err(|error| error.to_string())?
        .unwrap()
        .revision as u64;
    for level in [1, 2] {
        engine
            .act(
                engine::actions::Action::SetTransferAgreementLevel {
                    transfer: transfer.clone(),
                    expected_revision: revision,
                    request_id: nucleus::new_uid("instinct-agreement"),
                    person: Some(people[1].clone()),
                    level,
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    engine
        .set_signer(signers[0].clone())
        .await
        .map_err(|error| error.to_string())?;
    let mut data = json!({"transfer":transfer,"person":people[0],"promise":promise,"stock":record});
    if automation {
        let rule = engine
            .act(
                engine::actions::Action::SaveKarmaRule {
                    identity: Some(nucleus::karma::rule_field::RuleIdentity {
                        name: "Publish a prepared proposal".into(),
                        slug: "practice-transfer-rule".into(),
                    }),
                    rule: None,
                    expected_revision: None,
                    fields: [
                        "@practice-stock",
                        "< 0",
                        "@instinct-transfer: publish(@practice-giver)",
                    ]
                    .map(|text| {
                        nucleus::karma::rule_field::RuleFieldInput::Text {
                            source: text.into(),
                        }
                    }),
                    request_id: nucleus::new_uid("instinct-transfer-rule"),
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?
            .created
            .ok_or("The publication Rule was not confirmed.")?;
        data["rule"] = json!(rule);
    }
    Ok(data)
}

pub(super) fn view(world: &mut World, root: Entity, data: &Value) -> Entity {
    let practice = world.get::<Practice>(root).unwrap();
    let owner = crate::transfer_castle::spawn(
        world,
        root,
        practice.workspace,
        DVec2::new(780.0, 0.0),
        crate::transfer_castle::TransferCastle {
            selected: data["transfer"].as_str().unwrap().into(),
            person: data["person"].as_str().unwrap().into(),
            ..default()
        },
    );
    own(world, root, owner, Role::Feature);
    if data["rule"].is_string() {
        own(world, root, owner, Role::Auxiliary);
        let workspace = world.get::<Practice>(root).unwrap().workspace;
        let rule =
            crate::karma_castle::spawn(world, root, workspace, DVec2::new(1950.0, 0.0), default());
        own(world, root, rule, Role::Feature);
        rule
    } else {
        owner
    }
}

pub(super) fn execute(world: &mut World, owner: Entity, data: &Value, operation: Operation) {
    match operation {
        Operation::CheckTransfer => {
            crate::transfer_castle::set_agreement(world, owner, 1);
        }
        Operation::AgreeTransfer => {
            crate::transfer_castle::set_agreement(world, owner, 2);
        }
        Operation::ActivateTransfer => {
            crate::transfer_castle::activate_promise(
                world,
                owner,
                data["promise"].as_str().unwrap(),
            );
        }
        Operation::PauseTransferRule => {
            if let Some((uid, revision, state)) =
                crate::karma_castle::saved_rule(world, owner, "practice-transfer-rule")
                && state != "paused"
            {
                crate::karma_castle::RuleAction::Pause(uid, revision, true).apply(world, owner);
            }
        }
        _ => {}
    }
}

pub(super) fn complete(
    world: &mut World,
    owner: Entity,
    data: &Value,
    operation: Operation,
) -> bool {
    match operation {
        Operation::CheckTransfer => {
            crate::transfer_castle::agreement_level(world, owner).is_some_and(|level| level >= 1)
        }
        Operation::AgreeTransfer => {
            crate::transfer_castle::agreement_level(world, owner) == Some(2)
        }
        Operation::ActivateTransfer => {
            crate::transfer_castle::has_occurrence(world, owner, data["promise"].as_str().unwrap())
        }
        Operation::PauseTransferRule => {
            crate::karma_castle::saved_rule(world, owner, "practice-transfer-rule")
                .is_some_and(|(uid, _, state)| uid == data["rule"] && state == "paused")
        }
        _ => false,
    }
}
