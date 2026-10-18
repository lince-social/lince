use std::io::Error;

use engine::{Engine, actions::Action, signal_endpoints::Request};
use serde_json::Value;
use store::signal_endpoints::Endpoint;

use super::{oops, open};

const USAGE: &str = "Usage:
  lince ailuros inspect
  lince ailuros enroll <public-key> <name> <signal-slug> <expires-at-RFC3339>
  lince ailuros <enable|disable> <device-uid>";

async fn request(engine: &Engine, request: Request) -> Result<Value, Error> {
    engine
        .act(Action::SignalEndpoints { request }, None)
        .await
        .map_err(Error::other)?
        .data
        .ok_or_else(|| oops("Lince did not return Ailuros setup data"))
}

pub(super) async fn dispatch(verbs: &[&str]) -> Result<(), Error> {
    if !matches!(
        verbs,
        [] | ["inspect"] | ["enroll", _, _, _, _] | ["enable" | "disable", _]
    ) {
        return Err(oops(USAGE));
    }
    let directory = utils::config::lince_data_dir()
        .ok_or_else(|| oops("Cannot find the Lince data directory"))?;
    let _instance = lince_interface::activation::claim_headless(&directory)?;
    let store = open().await?;
    let engine = Engine::new(store).await.map_err(Error::other)?;
    let data = match verbs {
        [] | ["inspect"] => request(&engine, Request::Inspect).await?,
        ["enroll", key, name, slug, expiry] => {
            let inspected = request(&engine, Request::Inspect).await?;
            let endpoints: Vec<Endpoint> =
                serde_json::from_value(inspected["endpoints"].clone()).map_err(Error::other)?;
            if endpoints.iter().any(|row| row.public_key == *key) {
                return Err(oops(
                    "This public key is already enrolled; inspect its existing Signal",
                ));
            }
            let enrolled = request(
                &engine,
                Request::Save {
                    endpoint: None,
                    expected_revision: None,
                    name: (*name).into(),
                    public_key: (*key).into(),
                    signal_slug: (*slug).into(),
                    enabled: false,
                    cooldown_seconds: 60,
                    minimum: "0".into(),
                    maximum: "100".into(),
                    expires_at: (*expiry).into(),
                },
            )
            .await?;
            let rows: Vec<Endpoint> =
                serde_json::from_value(enrolled["endpoints"].clone()).map_err(Error::other)?;
            let endpoint = rows
                .iter()
                .find(|row| enrolled["saved"].as_str() == Some(row.uid.as_str()))
                .ok_or_else(|| oops("Enrolled device is missing from Lince's response"))?;
            request(
                &engine,
                Request::ConfigureStream {
                    signal: endpoint.signal_uid.clone(),
                    value_type: Some("jpeg".into()),
                    source_url: String::new(),
                    expected_revision: None,
                },
            )
            .await?;
            enrolled
        }
        [verb @ ("enable" | "disable"), uid] => {
            let inspected = request(&engine, Request::Inspect).await?;
            let rows: Vec<Endpoint> =
                serde_json::from_value(inspected["endpoints"].clone()).map_err(Error::other)?;
            let endpoint = rows
                .iter()
                .find(|row| row.uid == *uid)
                .ok_or_else(|| oops("Unknown Ailuros device; inspect this Cell first"))?;
            request(
                &engine,
                Request::SetEnabled {
                    endpoint: endpoint.uid.clone(),
                    expected_revision: endpoint.revision,
                    enabled: *verb == "enable",
                },
            )
            .await?
        }
        _ => return Err(oops(USAGE)),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&data).map_err(Error::other)?
    );
    Ok(())
}
