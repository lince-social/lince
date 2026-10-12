use super::*;
use engine::{
    actions::Action,
    signal_endpoints::{Request, hex},
};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};

#[tokio::test]
async fn https_proxy_ingress_verifies_readings_and_enforces_body_and_origin_limits() {
    let state = crate::tests::fixture().await;
    let key = SigningKey::from_slice(&[1; 32]).unwrap();
    let public_key = hex(key.verifying_key().to_encoded_point(true).as_bytes());
    let data = state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::Save {
                    endpoint: None,
                    expected_revision: None,
                    name: "Sensor".into(),
                    public_key,
                    signal_slug: "sensor".into(),
                    enabled: true,
                    cooldown_seconds: 60,
                    minimum: "0".into(),
                    maximum: "100".into(),
                    expires_at: (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339(),
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(data["endpoints"][0]["enabled"], false);
    state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: data["endpoints"][0]["uid"].as_str().unwrap().into(),
                    expected_revision: 1,
                    enabled: true,
                },
            },
            None,
        )
        .await
        .unwrap();
    let mut reading = Reading {
        audience: data["audience"].as_str().unwrap().into(),
        endpoint: data["endpoints"][0]["uid"].as_str().unwrap().into(),
        signal: data["endpoints"][0]["signal_uid"].as_str().unwrap().into(),
        credential_epoch: 1,
        sequence: 1,
        issued_at: chrono::Utc::now().timestamp(),
        value: "12".into(),
        signature: String::new(),
    };
    let signature: Signature = key.sign(&reading.signing_payload().unwrap());
    reading.signature = hex(&signature.to_bytes());
    let facade =
        crate::Facade::start_public(state.cell.clone(), "127.0.0.1:0", "https://lince.test")
            .await
            .unwrap();
    let url = format!(
        "http://{}/signals/{}/readings",
        facade.address, reading.signal
    );
    let client = reqwest::Client::new();
    let post = || {
        client
            .post(&url)
            .header("host", "lince.test")
            .header("x-forwarded-proto", "https")
    };
    assert_eq!(
        client
            .post(&url)
            .json(&reading)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let first = post().json(&reading).send().await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let receipt = first.json::<serde_json::Value>().await.unwrap();
    assert_eq!(
        post()
            .json(&reading)
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap(),
        receipt
    );
    let mut tampered = reading.clone();
    tampered.value = "13".into();
    assert_eq!(
        post().json(&tampered).send().await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut next = reading.clone();
    next.sequence = 2;
    let signature: Signature = key.sign(&next.signing_payload().unwrap());
    next.signature = hex(&signature.to_bytes());
    assert_eq!(
        post().json(&next).send().await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        post()
            .header("content-type", "application/json")
            .body(" ".repeat(2049))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut unknown = serde_json::to_value(&reading).unwrap();
    unknown["command"] = json!("delete-everything");
    assert_eq!(
        post().json(&unknown).send().await.unwrap().status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let local = crate::Facade::start(state.cell, "127.0.0.1:0")
        .await
        .unwrap();
    assert_eq!(
        client
            .post(format!(
                "http://{}/signals/{}/readings",
                local.address, reading.signal
            ))
            .json(&reading)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn global_ingress_limits_apply_before_signature_work() {
    let ingress = Ingress::default();
    let mut permits = Vec::new();
    for _ in 0..8 {
        permits.push(ingress.admit().await.unwrap());
    }
    assert_eq!(
        ingress.admit().await.unwrap_err().0,
        StatusCode::TOO_MANY_REQUESTS
    );
    drop(permits);
    for _ in 9..4096 {
        drop(ingress.admit().await.unwrap());
    }
    assert_eq!(
        ingress.admit().await.unwrap_err().0,
        StatusCode::TOO_MANY_REQUESTS
    );
    *ingress.window.lock().await = (Instant::now() - Duration::from_secs(61), 4096);
    assert!(ingress.admit().await.is_ok());
}

#[tokio::test]
async fn bearer_clients_update_only_the_signal_in_their_endpoint_grant() {
    let state = crate::tests::fixture().await;
    let data = state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::Save {
                    endpoint: None,
                    expected_revision: None,
                    name: "Job".into(),
                    public_key: String::new(),
                    signal_slug: "job-count".into(),
                    enabled: false,
                    cooldown_seconds: 1,
                    minimum: "0".into(),
                    maximum: "100".into(),
                    expires_at: (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339(),
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let endpoint = data["endpoints"][0]["uid"].as_str().unwrap();
    let signal = data["endpoints"][0]["signal_uid"].as_str().unwrap();
    state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: endpoint.into(),
                    expected_revision: 1,
                    enabled: true,
                },
            },
            None,
        )
        .await
        .unwrap();
    let issued = state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::IssueToken {
                    endpoint: endpoint.into(),
                    expected_revision: 2,
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let token = issued["issued_token"]["token"].as_str().unwrap();
    let facade =
        crate::Facade::start_public(state.cell.clone(), "127.0.0.1:0", "https://lince.test")
            .await
            .unwrap();
    let client = reqwest::Client::new();
    let post = |signal: &str| {
        client
            .post(format!(
                "http://{}/signals/{signal}/readings",
                facade.address
            ))
            .header("host", "lince.test")
            .header("x-forwarded-proto", "https")
    };
    let body = json!({"sequence": 1, "issued_at": chrono::Utc::now().timestamp(), "value": "7"});
    assert_eq!(
        post(signal).json(&body).send().await.unwrap().status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        post(signal)
            .bearer_auth("00".repeat(32))
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&nucleus::new_uid("r"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let first = post(signal)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let receipt = first.json::<serde_json::Value>().await.unwrap();
    assert_eq!(
        post(signal)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap(),
        receipt
    );
    let mut extra = body;
    extra["signal"] = json!(nucleus::new_uid("r"));
    assert_eq!(
        post(signal)
            .bearer_auth(token)
            .json(&extra)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        post(signal)
            .bearer_auth(token)
            .header("authorization", format!("Bearer {token}"))
            .json(&extra)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}
