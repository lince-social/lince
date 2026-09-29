use engine::actions::Action;
use nucleus::simulation::Cause;

use crate::scenario::Invocation;
use crate::world::World;

impl World {
    pub(crate) async fn accept_invitation(
        &mut self,
        cell: &str,
        id: &str,
        transfer: &str,
        person: &str,
        cause: Cause,
    ) -> crate::Result<()> {
        let transfer = self.resolve_reference(transfer);
        let person = self.resolve_reference(person);
        let node = &self.nodes[cell];
        node.set_time(self.now_ms)?;
        let invitation =
            store::transfers::invitations_for_transfer(&node.engine().store.pool, &transfer)
                .await?
                .into_iter()
                .find(|invitation| invitation.addressed_person_uid == person)
                .ok_or("no invitation addressed to Person")?;
        let revision = store::transfers::get(&node.engine().store.pool, &transfer)
            .await?
            .ok_or("Transfer missing")?
            .revision
            .try_into()?;
        self.invoke(
            cell,
            &Invocation {
                id: id.into(),
                actor: None,
                action: Action::AcceptTransferInvitation {
                    invitation: invitation.uid,
                    expected_revision: revision,
                    request_id: id.into(),
                    transfer: Some(transfer),
                    person: Some(person),
                },
            },
            cause,
        )
        .await
    }

    pub(crate) async fn settle_reviewed(
        &mut self,
        cell: &str,
        id: &str,
        occurrence: &str,
        person: &str,
        quantity: nucleus::DecimalValue,
        cause: Cause,
    ) -> crate::Result<()> {
        let occurrence = self.resolve_reference(occurrence);
        let person = self.resolve_reference(person);
        let node = &self.nodes[cell];
        node.set_time(self.now_ms)?;
        let query = protein::Protein {
            source: protein::Source::TransferSettlementPreview,
            filter: vec![
                protein::Predicate::UidEq(occurrence.clone()),
                protein::Predicate::QuantityEq(quantity),
            ],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: None,
        };
        let signer = node
            .person_key
            .as_ref()
            .map(|signer| signer.actor_uid.as_str());
        let rows = node
            .execution
            .scope(protein::execute_for_with_signer(
                &node.engine().store,
                &query,
                None,
                signer,
            ))
            .await?;
        let preview = rows.first().ok_or("settlement preview unavailable")?;
        let number = |field: &str| {
            preview[field]
                .as_f64()
                .ok_or_else(|| format!("settlement preview missing {field}"))
        };
        let action = Action::SettleTransferOccurrence {
            occurrence,
            request_id: id.into(),
            person: Some(person),
            canonical_quantity: number("canonical_quantity")?,
            expected_remaining_quantity: number("expected_remaining_quantity")?,
            expected_local_delta: number("expected_local_delta")?,
            expected_application_formula_hash: preview["expected_application_formula_hash"]
                .as_str()
                .ok_or("settlement formula hash missing")?
                .into(),
            expected_application_formula_version: preview["expected_application_formula_version"]
                .as_u64()
                .ok_or("settlement formula version missing")?,
            expected_remainder_policy: match preview["expected_remainder_policy"].as_str() {
                Some("local_draft") => nucleus::transfer::TransferRemainderPolicy::LocalDraft,
                Some("visible") => nucleus::transfer::TransferRemainderPolicy::Visible,
                _ => return Err("settlement remainder policy missing".into()),
            },
        };
        self.invoke(
            cell,
            &Invocation {
                id: id.into(),
                actor: None,
                action,
            },
            cause,
        )
        .await
    }
}
