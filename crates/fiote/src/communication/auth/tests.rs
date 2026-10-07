use super::*;
use chrono::{Duration as ChronoDuration, Utc};
use openidconnect::{
    Audience, PrivateSigningKey, StandardClaims, SubjectIdentifier,
    core::{CoreIdTokenClaims, CoreJwsSigningAlgorithm, CoreRsaPrivateSigningKey},
};
use serde_json::json;

fn key() -> CoreRsaPrivateSigningKey {
    CoreRsaPrivateSigningKey::from_pem(
        include_str!("../../../tests/fixtures/oidc-signing-key.pem"),
        None,
    )
    .unwrap()
}
fn jwt(issuer: &str, audience: &str, subject: &str, nonce: &str, expires: i64) -> String {
    CoreIdToken::new(
        CoreIdTokenClaims::new(
            IssuerUrl::new(issuer.into()).unwrap(),
            vec![Audience::new(audience.into())],
            Utc::now() + ChronoDuration::seconds(expires),
            Utc::now(),
            StandardClaims::new(SubjectIdentifier::new(subject.into())),
            Default::default(),
        )
        .set_nonce(Some(Nonce::new(nonce.into()))),
        &key(),
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        None,
        None,
    )
    .unwrap()
    .to_string()
}
struct Fixture {
    auth: Auth,
    nonce: Arc<std::sync::Mutex<String>>,
    scopes: Arc<std::sync::Mutex<String>>,
    requests: Arc<std::sync::Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn fixture() -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let nonce = Arc::new(std::sync::Mutex::new(String::new()));
    let scopes = Arc::new(std::sync::Mutex::new(SCOPES.to_string()));
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let n = nonce.clone();
    let s = scopes.clone();
    let r = requests.clone();
    let base = issuer.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header, body) = loop {
                let mut chunk = [0; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                if count == 0 {
                    break (String::new(), String::new());
                }
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(split) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let header = String::from_utf8(bytes[..split].to_vec()).unwrap();
                    let length = header
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|s| s.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= split + 4 + length {
                        break (
                            header,
                            String::from_utf8(bytes[split + 4..split + 4 + length].to_vec())
                                .unwrap(),
                        );
                    }
                }
                assert!(bytes.len() < 16384);
            };
            r.lock().unwrap().push(format!("{header}\n{body}"));
            let response = if header.starts_with("GET /.well-known/") {
                json!({"issuer":base,"jwks_uri":format!("{base}/jwks"),"revocation_endpoint":format!("{base}/revoke")})
            } else if header.starts_with("GET /jwks ") {
                serde_json::to_value(CoreJsonWebKeySet::new(vec![key().as_verification_key()]))
                    .unwrap()
            } else if header.starts_with("POST /api/accounts/oauth/token ") {
                let form: std::collections::HashMap<_, _> =
                    url::form_urlencoded::parse(body.as_bytes())
                        .into_owned()
                        .collect();
                assert_eq!(form.get("client_id").unwrap(), "oaiapp_fixture");
                assert_eq!(form.get("resource").unwrap(), RESOURCE);
                if form.get("grant_type").unwrap() == "refresh_token" {
                    assert!(!form.contains_key("scope"));
                    assert_eq!(form.get("refresh_token").unwrap(), "refresh-1");
                    json!({"access_token":"access-2","refresh_token":"refresh-2","expires_in":3600,"token_type":"Bearer"})
                } else {
                    assert_eq!(form.get("code").unwrap(), "fixture-code");
                    assert!(!form.get("code_verifier").unwrap().is_empty());
                    assert!(
                        form.get("redirect_uri")
                            .unwrap()
                            .starts_with("http://127.0.0.1:")
                    );
                    json!({"access_token":"access-1","refresh_token":"refresh-1","id_token":jwt(&base,"oaiapp_fixture","fixture-subject",&n.lock().unwrap(),3600),"scope":s.lock().unwrap().clone(),"expires_in":3600,"token_type":"Bearer"})
                }
            } else if header.starts_with("POST /revoke ") {
                json!({})
            } else {
                panic!("Unexpected fixture route: {header}");
            };
            let body = response.to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    Fixture {
        auth: Auth::at(&issuer).unwrap(),
        nonce,
        scopes,
        requests,
        task,
    }
}
async fn finish(fixture: &Fixture, existing: Option<Account>) -> Result<Secret, String> {
    let attempt = fixture
        .auth
        .begin("urn:uuid:fixture".into(), existing)
        .await
        .unwrap();
    let url = url::Url::parse(&attempt.url).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    *fixture.nonce.lock().unwrap() = query["nonce"].clone();
    let redirect = query["redirect_uri"].clone();
    let state = query["state"].clone();
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["ext_agent_host_id"], "urn:uuid:fixture");
    if query["client_id"] == "dynamic_agent_client" {
        assert_eq!(query["agent_name_hint"], "Lince");
    } else {
        assert!(!query.contains_key("agent_name_hint"));
        assert!(query.contains_key("id_token_hint"));
    }
    let task = tokio::spawn(attempt.finish());
    let mut redirect = url::Url::parse(&redirect).unwrap();
    redirect
        .query_pairs_mut()
        .append_pair("state", &state)
        .append_pair("code", "fixture-code")
        .append_pair("client_id", "oaiapp_fixture");
    let response = reqwest::Client::new().get(redirect).send().await.unwrap();
    assert!(response.status().is_success());
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn registration_and_returning_login_verify_identity_and_preserve_issued_client() {
    let fixture = fixture().await;
    let secret = finish(&fixture, None).await.unwrap();
    let account = Account::decode(&secret).unwrap();
    assert_eq!(account.client_id, "oaiapp_fixture");
    assert_eq!(account.subject, "fixture-subject");
    assert!(account.enabled());
    assert!(!format!("{account:?}").contains("access-1"));
    assert!(finish(&fixture, Some(account.clone())).await.is_ok());
    let mut changed = account;
    changed.subject = "other-user".into();
    assert!(
        finish(&fixture, Some(changed))
            .await
            .unwrap_err()
            .contains("another account")
    );
    *fixture.scopes.lock().unwrap() = "openid profile".into();
    assert!(
        !Account::decode(&finish(&fixture, None).await.unwrap())
            .unwrap()
            .enabled()
    );
}

#[tokio::test]
async fn callback_rejects_state_duplicate_fields_and_missing_or_changed_client() {
    let fixture = fixture().await;
    let attempt = fixture
        .auth
        .begin("urn:uuid:fixture".into(), None)
        .await
        .unwrap();
    assert!(
        attempt
            .callback("/auth/callback?state=wrong&code=x&client_id=oaiapp_fixture")
            .is_err()
    );
    assert!(
        attempt
            .callback(&format!("/auth/callback?state={}&code=x", attempt.state))
            .is_err()
    );
    assert!(
        attempt
            .callback(&format!(
                "/auth/callback?state={}&state={}&code=x&client_id=oaiapp_fixture",
                attempt.state, attempt.state
            ))
            .is_err()
    );
    assert!(
        attempt
            .callback(&format!(
                "/auth/callback?state={}&error=access_denied",
                attempt.state
            ))
            .is_err()
    );
    assert!(
        attempt
            .callback(&format!(
                "/auth/callback?state={}&code=x&client_id=dynamic_agent_client",
                attempt.state
            ))
            .is_err()
    );
    let mut account = Account::decode(&finish(&fixture, None).await.unwrap()).unwrap();
    account.host_id = "another-host".into();
    assert!(
        fixture
            .auth
            .begin("urn:uuid:fixture".into(), Some(account))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn identity_rejects_wrong_audience_issuer_nonce_expiry_and_tampered_signature() {
    let fixture = fixture().await;
    for (issuer, audience, nonce, expiry) in [
        (fixture.auth.issuer.as_str(), "other-app", "nonce", 3600),
        ("https://other.test", "oaiapp_fixture", "nonce", 3600),
        (
            fixture.auth.issuer.as_str(),
            "oaiapp_fixture",
            "other-nonce",
            3600,
        ),
        (fixture.auth.issuer.as_str(), "oaiapp_fixture", "nonce", -60),
    ] {
        assert!(
            fixture
                .auth
                .identity(
                    &Secret(jwt(issuer, audience, "fixture-subject", nonce, expiry)),
                    "oaiapp_fixture",
                    Some("nonce")
                )
                .await
                .is_err()
        );
    }
    let mut token = jwt(
        &fixture.auth.issuer,
        "oaiapp_fixture",
        "fixture-subject",
        "nonce",
        3600,
    )
    .into_bytes();
    let index = token.len() - 20;
    token[index] = if token[index] == b'A' { b'B' } else { b'A' };
    assert!(
        fixture
            .auth
            .identity(
                &Secret(String::from_utf8(token).unwrap()),
                "oaiapp_fixture",
                Some("nonce")
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rotating_refresh_is_serialized_and_logout_revoke_uses_the_current_session() {
    let fixture = fixture().await;
    let mut account = Account::decode(&finish(&fixture, None).await.unwrap()).unwrap();
    account.expires_at = 0;
    let store = Arc::new(MemoryCredential(Mutex::new(account.encode().unwrap())));
    let lock = Arc::new(Mutex::new(()));
    let one = Session {
        auth: fixture.auth.clone(),
        store: store.clone(),
        lock: lock.clone(),
    };
    let two = Session {
        auth: fixture.auth.clone(),
        store: store.clone(),
        lock,
    };
    let (one, two) = tokio::join!(one.token(), two.token());
    assert_eq!(one.unwrap().0, "access-2");
    assert_eq!(two.unwrap().0, "access-2");
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("grant_type=refresh_token"))
            .count(),
        1
    );
    let current = Account::decode(&store.read().await.unwrap()).unwrap();
    assert_eq!(current.refresh_token.0, "refresh-2");
    fixture.auth.revoke(&current).await.unwrap();
    assert!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.contains("token=refresh-2") && r.contains("client_id=oaiapp_fixture"))
    );
}

#[tokio::test]
async fn loopback_ports_are_dynamic_and_host_identifier_is_stable() {
    let fixture = fixture().await;
    let busy = TcpListener::bind("127.0.0.1:1455").await.ok();
    let one = fixture
        .auth
        .begin("urn:uuid:fixture".into(), None)
        .await
        .unwrap();
    let two = fixture
        .auth
        .begin("urn:uuid:fixture".into(), None)
        .await
        .unwrap();
    assert_ne!(one.redirect, two.redirect);
    drop(busy);
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(host_id(dir.path()).unwrap(), host_id(dir.path()).unwrap());
}
