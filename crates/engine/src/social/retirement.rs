use super::*;
use nucleus::social::requests::DeviceAuthorizationRequest;
use serde::{Deserialize, Serialize};

pub(super) const NAMESPACE: &str = "lince.social.retirements";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Decision {
    pub(super) context: String,
    pub(super) owner_key: String,
    pub(super) request_floor: i64,
    pub(super) retired_at: i64,
    pub(super) signature: String,
}

pub(super) fn pending_requests(
    authority: &Value,
    context: &str,
    members: &std::collections::BTreeMap<String, String>,
    floor: i64,
    now: i64,
) -> bool {
    authority
        .as_object()
        .into_iter()
        .flatten()
        .any(|(field, value)| {
            let Some(cell) = field.strip_prefix("request_") else {
                return false;
            };
            let Ok(request) = serde_json::from_value::<DeviceAuthorizationRequest>(value.clone())
            else {
                return false;
            };
            if request.cell != cell
                || request.context != context
                || request.requested_at <= floor
                || members.get(cell) != Some(&request.operational_key)
                || request_auth::validate_device_request(&request, now).is_err()
            {
                return false;
            }
            let authorized = &authority[format!("authorized_{cell}")];
            authorized["route"] != serde_json::to_value(&request.route).unwrap_or(Value::Null)
                || authorized["certificate"]["issued_at"]
                    .as_i64()
                    .is_none_or(|at| at < request.requested_at)
        })
}

fn checked(value: &Value, context: &str, owner_key: &str) -> Result<Decision, EngineError> {
    let decision: Decision = serde_json::from_value(value.clone())?;
    if decision.context != context
        || decision.owner_key != owner_key
        || decision.request_floor <= 0
        || decision.retired_at < decision.request_floor
        || !crate::roster::verify_with(
            owner_key,
            &signing_bytes("private-key-retirement", &decision)?,
            &decision.signature,
        )
    {
        return Err(invalid(
            "The owner-signed key retirement decision is invalid",
        ));
    }
    Ok(decision)
}

pub(super) async fn on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    organ: &str,
    context: &str,
    owner_key: &str,
) -> Result<Option<Decision>, EngineError> {
    let map = owner::extension_on(tx, organ, NAMESPACE).await?;
    map.get(context)
        .filter(|value| !value.is_null())
        .map(|value| checked(value, context, owner_key))
        .transpose()
}

pub(super) async fn save_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    organ: &str,
    decision: &Decision,
) -> Result<(), EngineError> {
    let mut map = owner::extension_on(tx, organ, NAMESPACE).await?;
    let entries = map
        .as_object()
        .ok_or_else(|| invalid("Invalid retained key retirement ledger"))?;
    if entries.len() >= 4096 && !entries.contains_key(&decision.context) {
        return Err(invalid("The permanent key retirement ledger is full"));
    }
    if let Some(previous) = on(tx, organ, &decision.context, &decision.owner_key).await? {
        if previous.request_floor > decision.request_floor
            || previous.request_floor == decision.request_floor
                && serde_json::to_value(&previous)? != serde_json::to_value(decision)?
        {
            return Err(invalid(
                "A newer or conflicting key retirement decision is already retained",
            ));
        }
    }
    map[&decision.context] = serde_json::to_value(decision)?;
    store::records::set_extension_on(tx, organ, NAMESPACE, &map).await?;
    Ok(())
}

impl Engine {
    pub(super) async fn social_retired_request_floor(
        &self,
        organ: &str,
        context: &str,
        owner_key: &str,
    ) -> Result<i64, EngineError> {
        let mut tx = self.store.pool.begin().await?;
        let decision = on(&mut tx, organ, context, owner_key).await?;
        tx.commit().await?;
        Ok(decision.map_or(0, |decision| decision.request_floor))
    }
}
