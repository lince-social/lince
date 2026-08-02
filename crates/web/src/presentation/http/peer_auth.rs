//! The challenge gate on the organ↔organ HTTP boundary (Ontology §11
//! "Peers"): no op is served or accepted until the caller proves possession
//! of a key we stored at introduction. Stateless — the signature covers
//! method, path+query, a fresh timestamp, and the body hash, so a captured
//! request cannot be replayed elsewhere or later.

use axum::http::{HeaderMap, StatusCode};

pub(crate) const ORGAN_HEADER: &str = "x-lince-organ";
pub(crate) const TS_HEADER: &str = "x-lince-ts";
pub(crate) const SIG_HEADER: &str = "x-lince-sig";

fn header<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers.get(name)?.to_str().ok()
}

/// Verify a signed peer request; returns the proven organ uid. Everything
/// else — unknown organ, stale timestamp, bad signature — is a plain 403
/// with no detail served.
pub(crate) async fn verify_signed_request(
    engine: &engine::Engine,
    method: &str,
    path_and_query: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<String, (StatusCode, String)> {
    let forbidden = || (StatusCode::FORBIDDEN, "peer signature required".to_string());
    let organ = header(headers, ORGAN_HEADER).ok_or_else(forbidden)?;
    let ts = header(headers, TS_HEADER).ok_or_else(forbidden)?;
    let sig = header(headers, SIG_HEADER).ok_or_else(forbidden)?;
    if !engine::peers::timestamp_fresh(ts, chrono::Utc::now()) {
        return Err(forbidden());
    }
    let payload = engine::peers::request_signing_payload(method, path_and_query, ts, body);
    match engine::peers::verify_peer_signature(&engine.store, organ, &payload, sig).await {
        Ok(true) => Ok(organ.to_string()),
        Ok(false) => Err(forbidden()),
        Err(error) => Err((StatusCode::INTERNAL_SERVER_ERROR, error.to_string())),
    }
}

/// Sign a response body with the local Organ key and return the three peer
/// headers. `None` when no organ signer is installed (the client will refuse
/// the unproven answer — correct on both ends).
pub(crate) async fn response_headers(
    engine: &engine::Engine,
    body: &[u8],
) -> Option<[(&'static str, String); 3]> {
    let (organ, ts, sig) = engine.sign_peer_response(body).await?;
    Some([(ORGAN_HEADER, organ), (TS_HEADER, ts), (SIG_HEADER, sig)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use engine::Engine;
    use engine::trust::Signer;

    async fn cell() -> (Engine, String) {
        let e = Engine::open_memory().await.expect("engine");
        let organ = store::organs::ensure_local(&e.store.pool, "http://cell.test")
            .await
            .expect("local organ")
            .uid;
        let signer = Signer::generate(&organ, "ed25519:organ:v1");
        e.set_organ_signer(signer).await.expect("organ signer");
        (e, organ)
    }

    fn signed_headers(organ: &str, ts: &str, sig: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ORGAN_HEADER, HeaderValue::from_str(organ).unwrap());
        headers.insert(TS_HEADER, HeaderValue::from_str(ts).unwrap());
        headers.insert(SIG_HEADER, HeaderValue::from_str(sig).unwrap());
        headers
    }

    /// The four gate outcomes on the real verification path the handlers call:
    /// unsigned, correctly signed, wrong key, stale timestamp.
    #[tokio::test]
    async fn gate_accepts_only_fresh_correctly_signed_requests() {
        // The server Cell knows the caller through introduction-adopted keys.
        let (server, _) = cell().await;
        let caller = Signer::generate("o-caller", "ed25519:organ:v1");
        engine::trust::adopt_key(
            &server.store,
            "o-caller",
            &caller.key_id,
            &caller.public_key_b64(),
        )
        .await
        .expect("adopt caller key");

        let path = "/organ/ops?after=0";
        let body = b"";

        // Unsigned → 403.
        let unsigned = HeaderMap::new();
        let refused =
            verify_signed_request(&server, "GET", path, &unsigned, body).await;
        assert_eq!(refused.unwrap_err().0, StatusCode::FORBIDDEN);

        // Correctly signed → the proven organ uid.
        let ts = chrono::Utc::now().to_rfc3339();
        let payload = engine::peers::request_signing_payload("GET", path, &ts, body);
        let sig = caller.sign_bytes(&payload);
        let ok = verify_signed_request(
            &server,
            "GET",
            path,
            &signed_headers("o-caller", &ts, &sig),
            body,
        )
        .await
        .expect("verifies");
        assert_eq!(ok, "o-caller");

        // Signed by a key the server never adopted → 403.
        let stranger = Signer::generate("o-caller", "ed25519:organ:v1");
        let forged = stranger.sign_bytes(&payload);
        let refused = verify_signed_request(
            &server,
            "GET",
            path,
            &signed_headers("o-caller", &ts, &forged),
            body,
        )
        .await;
        assert_eq!(refused.unwrap_err().0, StatusCode::FORBIDDEN);

        // Fresh signature over a stale timestamp → 403 (replay bound).
        let stale_ts = (chrono::Utc::now()
            - chrono::TimeDelta::seconds(engine::peers::PEER_FRESHNESS_SECS + 5))
        .to_rfc3339();
        let stale_payload =
            engine::peers::request_signing_payload("GET", path, &stale_ts, body);
        let stale_sig = caller.sign_bytes(&stale_payload);
        let refused = verify_signed_request(
            &server,
            "GET",
            path,
            &signed_headers("o-caller", &stale_ts, &stale_sig),
            body,
        )
        .await;
        assert_eq!(refused.unwrap_err().0, StatusCode::FORBIDDEN);
    }

    /// Both directions: the response signature verifies against the server's
    /// stored key and fails for a stranger's answer.
    #[tokio::test]
    async fn response_signature_round_trips() {
        let (server, server_organ) = cell().await;
        let (client, _) = cell().await;
        // The client adopted the server's keys at introduction.
        for (key_id, public_key) in engine::trust::keys_of(&server.store, &server_organ)
            .await
            .expect("keys")
        {
            engine::trust::adopt_key(&client.store, &server_organ, &key_id, &public_key)
                .await
                .expect("adopt");
        }

        let body = br#"{"applied":3}"#;
        let headers = response_headers(&server, body).await.expect("signed");
        let (organ, ts, sig) = (
            headers[0].1.clone(),
            headers[1].1.clone(),
            headers[2].1.clone(),
        );
        assert!(
            client
                .verify_peer_response(&server_organ, &organ, &ts, &sig, body)
                .await
                .expect("verify")
        );
        // A stranger at the same address cannot fake it.
        assert!(
            !client
                .verify_peer_response(&server_organ, &organ, &ts, &sig, br#"{"applied":9}"#)
                .await
                .expect("verify tampered")
        );
    }
}
