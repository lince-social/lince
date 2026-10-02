use super::*;
use chrono::Timelike;
use nucleus::social::subscriptions::Subscription;
use store::sqlx::{Sqlite, Transaction};

const NAMESPACE: &str = "lince.social.saved-searches";
const DEVICE_NAMESPACE: &str = "lince.social.saved-search-device";
const MAX_FILTERS: usize = 16;
const MAX_ACTIVE: usize = 8;

mod worker;

pub(super) struct Guard {
    pub(super) filter: Subscription,
    pub(super) lease: String,
}

fn key(id: &str) -> String {
    format!("filter_{id}")
}

fn validate_filter(filter: &Subscription) -> Result<(), EngineError> {
    text(&filter.label, 80, false).map_err(invalid)?;
    validate_search(&filter.query)?;
    if !nucleus::valid_uid(&filter.id, "sub")
        || filter.query.after.is_some()
        || !(60..=10080).contains(&filter.interval_minutes)
        || filter.quiet_start_hour > 23
        || filter.quiet_end_hour > 23
        || filter.services.len() > 8
        || filter
            .services
            .iter()
            .any(|endpoint| endpoint.parse::<iroh::EndpointId>().is_err())
        || filter
            .services
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != filter.services.len()
        || filter
            .actor
            .as_deref()
            .is_some_and(|actor| !nucleus::valid_uid(actor, "r"))
    {
        return Err(invalid(
            "Choose a saved filter, at most eight distinct directory endpoints, a 60–10,080 minute interval and quiet hours from 0–23",
        ));
    }
    Ok(())
}

fn entries(map: &Value) -> Result<Vec<Subscription>, EngineError> {
    let fields = map
        .as_object()
        .ok_or_else(|| invalid("Invalid private saved-search settings"))?;
    let mut filters = Vec::new();
    for (field, value) in fields {
        let filter: Subscription = serde_json::from_value(value.clone())?;
        validate_filter(&filter)?;
        if *field != key(&filter.id) {
            return Err(invalid("Invalid saved-search field identity"));
        }
        filters.push(filter);
    }
    Ok(filters)
}

fn within_limits(map: &Value, filters: &[Subscription]) -> Result<bool, EngineError> {
    Ok(filters.len() <= MAX_FILTERS
        && filters.iter().filter(|filter| filter.enabled).count() <= MAX_ACTIVE
        && serde_json::to_vec(map)?.len() <= 64 * 1024)
}

fn quiet(filter: &Subscription, now: i64) -> bool {
    let hour = chrono::DateTime::from_timestamp(now, 0)
        .map(|time| time.with_timezone(&chrono::Local).hour())
        .unwrap_or(0);
    if filter.quiet_start_hour < filter.quiet_end_hour {
        (filter.quiet_start_hour..filter.quiet_end_hour).contains(&hour)
    } else if filter.quiet_start_hour > filter.quiet_end_hour {
        hour >= filter.quiet_start_hour || hour < filter.quiet_end_hour
    } else {
        false
    }
}

impl Engine {
    async fn social_subscription_device_enabled(&self) -> Result<bool, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let choice = store::records::get_extension(&self.store.pool, &cell.uid, DEVICE_NAMESPACE)
            .await?
            .unwrap_or_else(|| json!({"enabled":false}));
        choice["enabled"]
            .as_bool()
            .ok_or_else(|| invalid("Invalid saved-search device participation"))
    }

    async fn social_subscription_config_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<(String, Value, Vec<Subscription>), EngineError> {
        let organ: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind=? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(store::organs::LOCAL_ORGAN_SLUG)
        .bind(nucleus::RecordKind::Organ.as_str())
        .fetch_one(&mut **tx)
        .await?;
        let map = owner::extension_on(tx, &organ, NAMESPACE).await?;
        let filters = entries(&map)?;
        Ok((organ, map, filters))
    }

    pub(super) async fn social_save_subscription(
        &self,
        mut filter: Subscription,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        if filter.id.is_empty() {
            filter.id = nucleus::new_uid("sub");
        }
        filter.actor = actor.map(str::to_owned);
        validate_filter(&filter)?;
        let mut tx = self.social_write_tx().await?;
        let (organ, mut map, previous) = self.social_subscription_config_on(&mut tx).await?;
        let previous_bytes = serde_json::to_vec(&map)?.len();
        map[key(&filter.id)] = serde_json::to_value(&filter)?;
        let updated = entries(&map)?;
        if !within_limits(&map, &updated)? {
            let smaller = updated.len() <= previous.len()
                && updated.iter().filter(|entry| entry.enabled).count()
                    <= previous.iter().filter(|entry| entry.enabled).count()
                && serde_json::to_vec(&map)?.len() <= previous_bytes
                && (updated.len() < previous.len()
                    || updated.iter().filter(|entry| entry.enabled).count()
                        < previous.iter().filter(|entry| entry.enabled).count()
                    || serde_json::to_vec(&map)?.len() < previous_bytes);
            if !smaller {
                return Err(invalid(
                    "Resolve saved searches within sixteen filters, eight active filters and 64 KiB before adding more work",
                ));
            }
        }
        store::records::set_extension_on(&mut tx, &organ, NAMESPACE, &map).await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_subscriptions(None).await
    }

    pub(super) async fn social_remove_subscription(&self, id: &str) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(id, "sub") {
            return Err(invalid("Choose a saved filter"));
        }
        let mut tx = self.social_write_tx().await?;
        let (organ, mut map, _) = self.social_subscription_config_on(&mut tx).await?;
        map.as_object_mut()
            .ok_or_else(|| invalid("Invalid private saved-search settings"))?
            .remove(&key(id))
            .ok_or_else(|| invalid("This filter was already removed"))?;
        store::records::set_extension_on(&mut tx, &organ, NAMESPACE, &map).await?;
        store::sqlx::query("DELETE FROM social_subscription_job WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_subscriptions(None).await
    }

    pub(super) async fn social_configure_subscriptions(
        &self,
        enabled: bool,
    ) -> Result<Value, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let mut tx = self.social_write_tx().await?;
        store::records::set_extension_on(
            &mut tx,
            &cell.uid,
            DEVICE_NAMESPACE,
            &json!({"enabled":enabled}),
        )
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_subscriptions(None).await
    }

    pub(super) async fn social_subscriptions(
        &self,
        after: Option<&str>,
    ) -> Result<Value, EngineError> {
        if after.is_some_and(|id| !nucleus::valid_uid(id, "sub")) {
            return Err(invalid("Invalid saved-filter page"));
        }
        let mut tx = self.social_write_tx().await?;
        let (_, map, mut filters) = self.social_subscription_config_on(&mut tx).await?;
        let over_limit = !within_limits(&map, &filters)?;
        let total = filters.len();
        filters.sort_by(|a, b| a.id.cmp(&b.id));
        let page: Vec<_> = filters
            .into_iter()
            .filter(|filter| filter.id.as_str() > after.unwrap_or(""))
            .take(8)
            .collect();
        let mut result = Vec::new();
        for filter in page {
            let state:Option<(i64,Option<i64>,Option<String>,String)>=store::sqlx::query_as("SELECT next_attempt,last_completed,error,source FROM social_subscription_job WHERE id=?")
                .bind(&filter.id).fetch_optional(&mut *tx).await?;
            result.push(json!({"filter":filter,"runtime":state.map(|(next,done,error,source)|json!({"next_attempt":next,"last_completed":done,"error":error,"source":source}))}));
        }
        tx.commit().await?;
        let next = result.last().and_then(|row| row["filter"]["id"].as_str());
        Ok(
            json!({"saved_searches":result,"total":total,"over_limit":over_limit,"next_after":next,"device_enabled":self.social_subscription_device_enabled().await?,"can_manage_services":self.social_require_local_write().await.is_ok(),"status":if over_limit {"Merged settings exceed sixteen filters, eight active filters or 64 KiB. Periodic work is paused; disable or remove filters to resolve the limit"} else {"Saved filters sync through your owned devices. This device checks them only after its own opt-in. Notifications use local quiet hours; retained matches and search results are public cache evidence"}}),
        )
    }
}
