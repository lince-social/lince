use std::time::Duration;

use crate::CellRuntime;

const REPUBLISH_INTERVAL: Duration = Duration::from_secs(3600);

pub fn spawn_runner(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut bus = state.engine.subscribe();
        let mut queries = state.engine.watch_query_changes();
        let mut next_republish = tokio::time::Instant::now() + REPUBLISH_INTERVAL;
        loop {
            let wire = state.wire.read().await.clone();
            match match &wire {
                Some(wire) => wire.sync_once().await,
                None => Ok(0),
            } {
                Ok(moved) if moved > 0 => tracing::debug!(moved, "sync pass moved batches"),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "sync pass failed"),
            }
            if tokio::time::Instant::now() >= next_republish {
                next_republish = tokio::time::Instant::now() + REPUBLISH_INTERVAL;
                republish(&state, wire.as_deref()).await;
            }
            drop(wire);
            let sleep_secs = match shortest_interval(&state).await {
                Ok(seconds) => seconds,
                Err(error) => {
                    tracing::warn!(%error, "Cannot read sync settings. Lince will retry in 30 seconds");
                    30
                }
            };
            tokio::select! {
                changed = queries.changed() => {
                    if changed.is_err() { break; }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                _ = tokio::time::sleep(Duration::from_secs(sleep_secs)) => {}
                received = bus.recv() => {
                    if matches!(received, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; }
                    if received.is_ok() {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        while bus.try_recv().is_ok() {}
                    }
                }
            }
        }
    })
}

fn supervised<F, Fut>(state: CellRuntime, worker: engine::social::Worker, run: F) -> tokio::task::JoinHandle<()>
where
    F: Fn(CellRuntime) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    crate::social_host::supervise(state.engine.clone(), worker, move || tokio::spawn(run(state.clone())))
}

pub fn spawn_social(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    supervised(state, engine::social::Worker::Publication, |state| async move {
        let mut changes = state.engine.watch_query_changes();
        loop {
            let online = state.wire.read().await.is_some();
            let mut succeeded = true;
            succeeded &= state.engine.social_refresh_reply_authorizations().await.is_ok();
            succeeded &= state.engine.social_reconcile_private_admissions().await.is_ok();
            succeeded &= state.engine.social_reconcile_revealed_contacts().await.is_ok();
            succeeded &= state.engine.social_prepare_messages_once().await.is_ok();
            succeeded &= state.engine.social_process_message_events_once().await.is_ok();
            if online {
                succeeded &= state.engine.social_publish_once().await.is_ok();
                succeeded &= state.engine.social_send_reports_once().await.is_ok();
            }
            state.engine.social_worker_completed(engine::social::Worker::Publication, succeeded);
            if !succeeded { tracing::debug!("Social publication or authorization remains queued"); }
            tokio::select! {
                result = changes.changed() => {
                    if result.is_err() { break; }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    })
}

pub fn spawn_private_send(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    supervised(state, engine::social::Worker::Send, |state| async move {
        loop {
            if state.wire.read().await.is_some() {
                let sent = state.engine.social_send_private_once().await.is_ok();
                let refreshed = state.engine.social_refresh_private_routes_once().await.is_ok();
                state.engine.social_worker_completed(engine::social::Worker::Send, sent && refreshed);
                if !sent || !refreshed { tracing::debug!("Private delivery or route refresh remains queued"); }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}

pub fn spawn_private_pickup(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    supervised(state, engine::social::Worker::Pickup, |state| async move {
        loop {
            if state.wire.read().await.is_some() {
                let succeeded = state.engine.social_collect_private_once().await.is_ok();
                state.engine.social_worker_completed(engine::social::Worker::Pickup, succeeded);
                if !succeeded { tracing::debug!("Private mailbox pickup remains queued"); }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}

pub fn spawn_gossip(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    supervised(state, engine::social::Worker::Gossip, |state| async move {
        let mut changes = state.engine.watch_query_changes();
        loop {
            if state.wire.read().await.is_some() {
                let succeeded = state.engine.social_gossip_once().await.is_ok();
                state.engine.social_worker_completed(engine::social::Worker::Gossip, succeeded);
                if !succeeded { tracing::debug!("Public announcement forwarding remains bounded and queued"); }
            }
            tokio::select! {
                result = changes.changed() => { if result.is_err() { break; } tokio::time::sleep(Duration::from_millis(250)).await; }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    })
}

pub fn spawn_ask(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    supervised(state, engine::social::Worker::Ask, |state| async move {
        let mut changes = state.engine.watch_query_changes();
        loop {
            if state.wire.read().await.is_some() {
                let succeeded = state.engine.social_ask_once().await.is_ok();
                state.engine.social_worker_completed(engine::social::Worker::Ask, succeeded);
                if !succeeded { tracing::debug!("Contact query work waits within its original bounded deadline"); }
            }
            tokio::select! {
                result = changes.changed() => { if result.is_err() { break; } tokio::time::sleep(Duration::from_millis(250)).await; }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    })
}

pub fn spawn_subscriptions(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    supervised(state,engine::social::Worker::Subscriptions,|state| async move {
        let mut changes=state.engine.watch_query_changes();
        loop {
            let online=state.wire.read().await.is_some();
            let result=state.engine.social_subscriptions_once(online).await;
            state.engine.social_worker_completed(engine::social::Worker::Subscriptions,result.is_ok());
            if let Ok(notices)=result {
                for id in notices {
                    utils::diagnostics::Diagnostics::global().report(&format!("social::saved-search::{id}"),"New public announcements match a saved search. Open Discovery's saved searches to review cached matches.");
                }
            } else {tracing::debug!("Saved-search work waits for participation, permission or bounded resources");}
            tokio::select! {
                result=changes.changed()=>{if result.is_err(){break;} tokio::time::sleep(Duration::from_millis(250)).await;}
                _=tokio::time::sleep(Duration::from_secs(5))=>{}
            }
        }
    })
}

pub fn spawn_mail_delivery(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut changes = state.engine.watch_query_changes();
        loop {
            let wire = state.wire.read().await.clone();
            if let Some(wire) = wire {
                if let Err(error) = wire.retry_saved_mail_once().await {
                    tracing::debug!(%error, "Saved mail delivery remains queued");
                }
                if let Err(error) = wire.collect_own_mail().await {
                    tracing::debug!(%error, "Mail collection will retry");
                }
            }
            tokio::select! {
                result = changes.changed() => {
                    if result.is_err() { break; }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    })
}

async fn republish(state: &CellRuntime, wire: Option<&engine::wire::Wire>) {
    if let Err(error) = state.engine.renew_local_roster().await {
        tracing::warn!(%error, "Could not renew the device roster");
    }
    if wire.map(engine::wire::Wire::reach) == Some(engine::wire::Reach::Local) || wire.is_none() {
        return;
    }
    let organ = match store::organs::local(&state.store.pool).await {
        Ok(Some(organ)) => organ,
        Ok(None) => {
            tracing::warn!("Cannot publish the directory record: the local Organ is missing");
            return;
        }
        Err(error) => {
            tracing::warn!(%error, "Cannot read the local Organ for the directory record");
            return;
        }
    };
    if let Err(error) = state.engine.republish_public_record(&organ.uid).await {
        tracing::warn!(%error, "could not republish the directory record");
    }
}

async fn shortest_interval(state: &CellRuntime) -> Result<u64, String> {
    let contacts = store::organs::contacts(&state.store.pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(contacts
        .iter()
        .filter(|c| c.sync_in && c.catchup_interval_secs > 0 && c.trust != "blocked")
        .map(|c| c.catchup_interval_secs.clamp(5, 300) as u64)
        .min()
        .unwrap_or(30))
}

pub fn spawn_presence(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut changes = state.engine.presence.changes();
        let mut active = false;
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                changed = changes.changed() => { if changed.is_err() { break } }
                _ = interval.tick(), if active => {}
            }
            let next = !state.engine.presence.records().is_empty();
            let wire = state.wire.read().await.clone();
            if (active || next)
                && let Some(wire) = wire
            {
                if let Err(error) = wire.sync_presence().await {
                    tracing::debug!(%error, "Cursor presence exchange failed");
                }
            }
            active = next;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    })
}
