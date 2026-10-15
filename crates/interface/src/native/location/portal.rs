use futures_util::StreamExt;
use std::{collections::HashMap, sync::Arc, time::Duration};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";

fn message(error: impl ToString) -> String {
    error.to_string()
}

async fn needed(engine: &engine::Engine) -> bool {
    engine
        .location_sources()
        .await
        .iter()
        .any(|source| source.settings.source_kind == nucleus::location::SourceKind::Device)
}

pub(super) async fn capture(engine: &Arc<engine::Engine>) -> Result<(), String> {
    let connection = Connection::session().await.map_err(message)?;
    let proxy = Proxy::new(
        &connection,
        DESTINATION,
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Location",
    )
    .await
    .map_err(message)?;
    let token = nucleus::new_uid("location").replace('-', "_");
    let options = HashMap::from([
        ("session_handle_token", Value::from(token.as_str())),
        ("time-threshold", Value::from(3_u32)),
        ("distance-threshold", Value::from(0_u32)),
        ("accuracy", Value::from(5_u32)),
    ]);
    let session: OwnedObjectPath = proxy
        .call("CreateSession", &(options,))
        .await
        .map_err(message)?;
    let result = run(engine, &connection, &proxy, &session, &token).await;
    if let Ok(proxy) = Proxy::new(
        &connection,
        DESTINATION,
        session.as_str(),
        "org.freedesktop.portal.Session",
    )
    .await
    {
        let _: Result<(), _> = proxy.call("Close", &()).await;
    }
    result
}

async fn run(
    engine: &Arc<engine::Engine>,
    connection: &Connection,
    proxy: &Proxy<'_>,
    session: &OwnedObjectPath,
    token: &str,
) -> Result<(), String> {
    let sender = connection
        .unique_name()
        .ok_or("Portal connection has no identity")?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let request_path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = Proxy::new(
        connection,
        DESTINATION,
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .await
    .map_err(message)?;
    let mut responses = request.receive_signal("Response").await.map_err(message)?;
    let mut updates = proxy
        .receive_signal("LocationUpdated")
        .await
        .map_err(message)?;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let options = HashMap::from([("handle_token", Value::from(token))]);
    let _: OwnedObjectPath = proxy
        .call("Start", &(session, "", options))
        .await
        .map_err(message)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        tokio::select! {
            response = responses.next() => {
                let response = response.ok_or("Location permission request closed")?;
                let (status, _): (u32, HashMap<String, OwnedValue>) = response.body().deserialize().map_err(message)?;
                if status != 0 { return Err("Location permission was denied".into()); }
                break;
            }
            _ = interval.tick() => { if !needed(engine).await { return Ok(()); } }
            _ = tokio::time::sleep_until(deadline) => return Err("Location permission request timed out".into()),
        }
    }
    let mut sequence = 0_u64;
    loop {
        tokio::select! {
            update = updates.next() => {
                let update = update.ok_or("Location portal closed")?;
                let (source, values): (OwnedObjectPath, HashMap<String, OwnedValue>) = update.body().deserialize().map_err(message)?;
                if source != *session { continue; }
                let latitude = values.get("Latitude").and_then(|value| f64::try_from(value).ok()).ok_or("Portal latitude is missing")?;
                let longitude = values.get("Longitude").and_then(|value| f64::try_from(value).ok()).ok_or("Portal longitude is missing")?;
                let accuracy = values.get("Accuracy").and_then(|value| f64::try_from(value).ok()).filter(|value| *value >= 0.0);
                let (seconds, micros) = values.get("Timestamp").ok_or("Portal capture time is missing")?.downcast_ref::<(u64,u64)>().map_err(message)?;
                let captured_at_ms = seconds.checked_mul(1000).and_then(|value| value.checked_add(micros / 1000)).and_then(|value| i64::try_from(value).ok()).ok_or("Invalid portal capture time")?;
                sequence = sequence.saturating_add(1);
                let _ = engine.publish_device_location(latitude, longitude, accuracy, captured_at_ms, sequence).await;
            }
            _ = interval.tick() => { if !needed(engine).await { return Ok(()); } }
        }
    }
}
