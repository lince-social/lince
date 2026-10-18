use super::*;

pub(super) async fn inspect(
    engine: &Engine,
    definition: &Definition,
    actor: Option<&str>,
) -> Result<
    (
        Preview,
        String,
        Option<String>,
        nucleus::transfer::karma::Snapshot,
    ),
    EngineError,
> {
    definition.validate().map_err(invalid)?;
    engine
        .stock_policy_actor(&definition.controller_uid, actor)
        .await?;
    engine
        .refuse_unreadable(actor, &[definition.stock_uid.clone()])
        .await?;
    let stock = store::records::get(&engine.store.pool, &definition.stock_uid)
        .await?
        .ok_or_else(|| invalid("Stock unavailable"))?;
    let local = store::organs::local(&engine.store.pool)
        .await?
        .ok_or_else(|| invalid("Local Organ unavailable"))?;
    let replica: bool =
        store::sqlx::query_scalar("SELECT replica_root IS NOT NULL FROM record WHERE uid=?")
            .bind(&stock.uid)
            .fetch_one(&engine.store.pool)
            .await?;
    if stock.kind != "plain"
        || stock.quantity.is_negative()
        || replica
        || stock.organ_uid.as_deref() != Some(local.uid.as_str())
    {
        return Err(invalid("Choose nonnegative local stock"));
    }
    let anchor = match &definition.recipe {
        Recipe::Offer { transfer_uid, .. } => transfer_uid,
        Recipe::Replenish {
            upstream_transfer_uid,
            ..
        } => upstream_transfer_uid,
    };
    let source = engine.transfer_karma_snapshot(anchor, actor).await?;
    let balance = store::transfer_balances::read(&engine.store.pool, &stock.uid).await?;
    if !balance.incomplete.is_empty() {
        return Err(invalid(
            "Review unavailable stock commitments before enabling planning",
        ));
    }
    let basis = nucleus::karma::canonical_hash(
        "lince.stock-policy-basis.v1",
        &json!({"balance":balance,"unit":stock.unit_uid}),
    )
    .map_err(invalid)?
    .as_str()
    .to_owned();
    let zero = store::exact::zero();
    let (offered, proposed, personal, own, frozen, reason) = match &definition.recipe {
        Recipe::Offer {
            transfer_uid,
            promise_uid,
            retain,
            mode,
        } => {
            engine
                .require_transfer_draft_creator(transfer_uid, Some(&definition.controller_uid))
                .await?;
            let promise = store::misc::get_promise(&engine.store.pool, promise_uid)
                .await?
                .ok_or_else(|| invalid("Offer unavailable"))?;
            if promise.transfer_uid.as_ref() != Some(transfer_uid)
                || promise.record_uid.as_ref() != Some(&stock.uid)
                || promise.party_uid.as_ref() != Some(&definition.controller_uid)
                || promise.delta >= 0.0
                || promise.unit_uid != stock.unit_uid
            {
                return Err(invalid("Choose your outgoing promise in this stock's unit"));
            }
            let offered = if matches!(
                promise.state,
                nucleus::PromiseState::Withdrawn | nucleus::PromiseState::Broken
            ) {
                zero
            } else {
                store::transfer_accounting::amount(promise.delta.abs())?
            };
            let matching = |commitment: &&store::transfer_balances::Commitment| {
                commitment.transfer == *transfer_uid
                    && commitment.promise == *promise_uid
                    && commitment.person == definition.controller_uid
            };
            let own = balance
                .commitments
                .iter()
                .filter(matching)
                .collect::<Vec<_>>();
            if own.iter().any(|commitment| {
                !commitment.outgoing
                    || store::exact::negate(commitment.private_remaining).map_or(true, |amount| {
                        amount.exact_numeric_cmp(commitment.canonical_remaining)
                            != std::cmp::Ordering::Equal
                    })
            }) {
                return Err(invalid(
                    "Review a stock-unit offer with an identity private quantity mapping",
                ));
            }
            let reserved =
                store::exact::sum_exact(own.iter().map(|commitment| commitment.reserved))?;
            let remaining = store::exact::sum_exact(
                own.iter().map(|commitment| commitment.canonical_remaining),
            )?;
            let others = store::transfer_balances::reserved_for(
                balance
                    .commitments
                    .iter()
                    .filter(|commitment| !matching(commitment)),
            )?;
            let available = store::exact::difference(balance.available, others)?;
            let frozen = source.active
                || source
                    .participants
                    .values()
                    .any(|party| party.guard.level > 0)
                || !store::transfers::occurrences_of(&engine.store.pool, transfer_uid)
                    .await?
                    .is_empty()
                || store::location::transfer_ended(&engine.store.pool, transfer_uid).await?;
            let proposed = if frozen || *mode == Mode::PreserveOffer {
                offered
            } else {
                max_zero(store::exact::difference(available, number(retain)?)?)
            };
            let planned = if frozen { remaining } else { proposed };
            let personal = store::exact::difference(available, planned)?;
            (offered,proposed,personal,reserved,frozen,if frozen {"Signed participation freezes this amount; review a revision or cancellation for shortages"} else {"Unaccepted amount follows the reviewed stock policy"}.to_owned())
        }
        Recipe::Replenish { below, target, .. } => {
            let proposed = if balance.surplus.exact_numeric_cmp(number(below)?).is_lt() {
                max_zero(store::exact::difference(number(target)?, balance.surplus)?)
            } else {
                zero
            };
            (
                zero,
                proposed,
                balance.surplus,
                zero,
                false,
                "Replenishment proposes a normal Need; review its private effects before agreement"
                    .into(),
            )
        }
    };
    let shortfall = if personal.is_negative() {
        store::exact::negate(personal)?
    } else {
        zero
    };
    Ok((
        Preview {
            actual: balance.actual.to_string(),
            available: balance.available.to_string(),
            reserved: balance.reserved.to_string(),
            reserved_offer: own.to_string(),
            offered: offered.to_string(),
            proposed: proposed.to_string(),
            personal: personal.to_string(),
            shortfall: shortfall.to_string(),
            frozen,
            reason,
        },
        basis,
        stock.unit_uid,
        source,
    ))
}
