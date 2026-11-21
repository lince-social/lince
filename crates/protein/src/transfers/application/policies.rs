//! Private application policy and stock controls for the local signing Person.

use super::policy_visible;
use crate::ProteinError;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use store::{
    Store, config::transfer_application_formula, exact::zero, sqlx::query_scalar,
    transfer_accounting::policy, transfer_balances::read, transfer_stock,
};

pub(in crate::transfers) async fn policies(
    store: &Store,
    transfer: &mut Value,
    signer: Option<&str>,
    visible: Option<&HashSet<String>>,
) -> Result<(), ProteinError> {
    let Some(transfer) = transfer.as_object_mut() else {
        return Ok(());
    };
    transfer.remove("private_applications");
    let Some(person) = signer else {
        return Ok(());
    };
    let local: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM record p JOIN record o ON o.slug = 'local-organ' AND o.kind = 'organ' WHERE p.uid = ? AND p.organ_uid = o.uid AND p.deleted_at IS NULL)")
        .bind(person).fetch_one(&store.pool).await?;
    if !local {
        return Ok(());
    }
    let Some(uid) = transfer.get("uid").and_then(Value::as_str) else {
        return Ok(());
    };
    let mut seen = BTreeSet::new();
    let mut applications = Vec::new();
    for promise in transfer
        .get("promises")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(exchange) = promise["exchange"].as_str() else {
            continue;
        };
        if (promise["giver"] != person && promise["receiver"] != person) || !seen.insert(exchange) {
            continue;
        }
        let policy = policy(&store.pool, uid, exchange, person).await?;
        if !policy_visible(policy.as_ref(), visible) {
            continue;
        }
        let fallback = match &policy {
            None if promise["giver"] == person => Some("-incoming()".into()),
            None => Some(transfer_application_formula(&store.pool).await?),
            Some(_) => None,
        };
        let record = policy
            .as_ref()
            .map(|value| value.record.as_str())
            .or_else(|| {
                if promise["party"] == person {
                    promise["record"].as_str()
                } else {
                    None
                }
            });
        let balance = match record {
            Some(record) if visible.is_none_or(|visible| visible.contains(record)) => {
                Some(read(&store.pool, record).await?)
            }
            Some(_) => continue,
            None => None,
        };
        let (stock, stock_version) = match record {
            Some(record) => (
                transfer_stock::get(&store.pool, record).await?,
                transfer_stock::version(&store.pool, record).await?,
            ),
            None => (None, 0),
        };
        let stock_limit = record.map(|_| json!({"current": stock, "version": stock_version}));
        let effects = policy.as_ref().map_or_else(
            || json!([{ "record": record, "formula": fallback, "mode": "quantity" }]),
            |policy| json!(policy.effects),
        );
        let effect_records = policy
            .iter()
            .flat_map(|policy| policy.effects.iter().map(|effect| effect.record.as_str()))
            .chain(record.filter(|_| policy.is_none()));
        let mut balances = Vec::new();
        for effect_record in effect_records {
            let extra;
            let balance = if Some(effect_record) == record
                && let Some(balance) = &balance
            {
                balance
            } else {
                extra = read(&store.pool, effect_record).await?;
                &extra
            };
            balances.push(json!({"record": effect_record, "balance": balance}));
        }
        applications.push(json!({
            "balance":balance,"balances":balances,"record":record,"stock_limit":stock_limit,
            "exchange":exchange,"person":person,"agreed_amount":promise["delta"].as_f64().map(f64::abs),
            "policy":policy,"capabilities":{"set_private_policy":true,"set_stock_limit":record.is_some(),"remove_stock_limit":stock.is_some()},
            "action_payloads":{
                "set_stock_limit":{"action":"set-record-stock-limit","record":record,"person":person,"minimum":zero(),"expected_version":stock_version,"request_id":null},
                "remove_stock_limit":{"action":"set-record-stock-limit","record":record,"person":person,"minimum":null,"expected_version":stock_version,"request_id":null},
                "set_private_policy":{
                "action":"set-transfer-private-application-policy","transfer":uid,"exchange":exchange,
                "person":person,"effects":effects,
                "expected_version":policy.as_ref().map_or(0, |value| value.version),"request_id":null
            }}
        }));
    }
    transfer.insert("private_applications".into(), Value::Array(applications));
    Ok(())
}
