use engine::{
    Engine,
    actions::Action,
    trust::Signer,
    wire::{ALPN_THREAD, Reach, Wire, WireRequest, WireResponse},
};
use iroh::{EndpointAddr, SecretKey};
use nucleus::sand_package::{Command, Kind, License, Query, Response};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

async fn node(seed: u8) -> (Arc<Engine>, Wire, String) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    engine
        .set_organ_signer(Signer::generate(&organ, "packages-key"))
        .await
        .unwrap();
    let wire = Wire::bind(
        engine.clone(),
        SecretKey::from_bytes(&[seed; 32]),
        Reach::Local,
    )
    .await
    .unwrap();
    (engine, wire, organ)
}

fn address(wire: &Wire) -> EndpointAddr {
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .next()
        .unwrap()
        .port();
    EndpointAddr::new(wire.node_id())
        .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
}

#[tokio::test]
async fn contacts_browse_and_fetch_public_packages_without_live_login_or_registry() {
    let (publisher, server, _) = node(71).await;
    let (_, client, client_organ) = node(72).await;
    store::organs::add_contact(&publisher.store.pool, &client_organ, None, "Reader", "", 0)
        .await
        .unwrap();
    store::organs::set_node_id(
        &publisher.store.pool,
        &client_organ,
        Some(&client.node_id().to_string()),
    )
    .await
    .unwrap();
    store::organs::set_trust(&publisher.store.pool, &client_organ, "known")
        .await
        .unwrap();
    let body = serde_json::json!({"format":nucleus::component::composition::FORMAT,"castle":{"name":"Shared","parts":[{}]}}).to_string();
    let record = publisher
        .act(
            Action::CreateCustomComponent {
                head: "Shared".into(),
                body,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let outcome = publisher
        .act(
            Action::SandPackage {
                request: Command::Save {
                    record,
                    kind: Kind::Castle,
                    licenses: vec![License {
                        name: "Example".into(),
                        text: "Retain original credit".into(),
                    }],
                    credits: vec!["Original Author".into()],
                },
            },
            None,
        )
        .await
        .unwrap();
    let Response::Saved { identity } = serde_json::from_value(outcome.data.unwrap()).unwrap()
    else {
        panic!("Expected identity");
    };
    let remote = address(&server);
    let serving = tokio::spawn(async move { server.serve().await });
    let request = WireRequest::SandPackages {
        query: Query::List { offset: 0 },
    };
    let response = client
        .request(remote.clone(), ALPN_THREAD, &request)
        .await
        .unwrap();
    let WireResponse::SandPackages {
        response: Response::Catalogue { entries, .. },
    } = response
    else {
        panic!("Expected catalogue");
    };
    assert!(entries.is_empty());
    let response = client
        .request(
            remote.clone(),
            ALPN_THREAD,
            &WireRequest::SandPackages {
                query: Query::Inspect {
                    identity: identity.clone(),
                },
            },
        )
        .await
        .unwrap();
    assert!(matches!(response, WireResponse::Error { .. }));
    publisher
        .act(
            Action::SandPackage {
                request: Command::SetPublic {
                    identity: identity.clone(),
                    public: true,
                },
            },
            None,
        )
        .await
        .unwrap();
    let WireResponse::SandPackages {
        response: Response::Catalogue { entries, .. },
    } = client
        .request(remote.clone(), ALPN_THREAD, &request)
        .await
        .unwrap()
    else {
        panic!("Expected catalogue");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].identity, identity);
    let response = client
        .request(
            remote.clone(),
            ALPN_THREAD,
            &WireRequest::SandPackages {
                query: Query::Inspect {
                    identity: identity.clone(),
                },
            },
        )
        .await
        .unwrap();
    let WireResponse::SandPackages {
        response: Response::Package {
            package, public, ..
        },
    } = response
    else {
        panic!("Expected package");
    };
    assert!(public);
    package.validate().unwrap();
    assert_eq!(package.manifest.identity, identity);
    assert_eq!(package.manifest.credits, ["Original Author"]);
    assert_eq!(package.manifest.licenses[0].text, "Retain original credit");
    let (_, stranger, _) = node(73).await;
    let response = stranger
        .request(remote.clone(), ALPN_THREAD, &request)
        .await;
    assert!(!matches!(response, Ok(WireResponse::SandPackages { .. })));
    publisher
        .act(
            Action::SandPackage {
                request: Command::SetPublic {
                    identity,
                    public: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    let WireResponse::SandPackages {
        response: Response::Catalogue { entries, .. },
    } = client.request(remote, ALPN_THREAD, &request).await.unwrap()
    else {
        panic!("Expected catalogue");
    };
    assert!(entries.is_empty());
    serving.abort();
    client.endpoint().close().await;
    stranger.endpoint().close().await;
}
