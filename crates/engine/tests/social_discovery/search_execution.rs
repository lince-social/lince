use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct SelectedServices {
    pages: std::collections::BTreeMap<String, Value>,
    failed: String,
    entered: tokio::sync::Semaphore,
    release: tokio::sync::Semaphore,
    active: AtomicUsize,
    peak: AtomicUsize,
    calls: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl Network for SelectedServices {
    async fn request(&self, service: &str, request: PublicRequest) -> Result<Value, EngineError> {
        let PublicRequest::Search { query, .. } = request else {
            panic!("Unexpected selected-service operation");
        };
        assert_eq!(query.text, "bicycle");
        self.calls.lock().unwrap().push(service.to_owned());
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        self.entered.add_permits(1);
        self.release.acquire().await.unwrap().forget();
        self.active.fetch_sub(1, Ordering::SeqCst);
        if service == self.failed {
            Err(EngineError::Consequence("Selected host offline".into()))
        } else {
            Ok(self.pages[service].clone())
        }
    }
}

async fn signed_row(author: &Engine, title: &str, endpoints: &[String]) -> Value {
    let draft = command(
        author,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: title.into(),
                destinations: endpoints.to_vec(),
                ..Default::default()
            },
        },
    )
    .await;
    let preview = command(
        author,
        Command::Preview {
            record: draft["record"].as_str().unwrap().into(),
            state: PostState::Active,
        },
    )
    .await;
    json!({"document":preview["document"],"hash":preview["preview_hash"]})
}

#[tokio::test]
async fn dispersed_selected_services_use_three_slots_and_merge_signed_copies_despite_failure() {
    let client = Arc::new(Engine::open_memory().await.unwrap());
    let author = Engine::open_memory().await.unwrap();
    let endpoints: Vec<_> = (200..204)
        .map(|seed| {
            iroh::SecretKey::from_bytes(&[seed; 32])
                .public()
                .to_string()
        })
        .collect();
    let shared = signed_row(&author, "Bicycle contribution", &endpoints).await;
    let mut pages = std::collections::BTreeMap::new();
    let mut expected =
        std::collections::BTreeSet::from([shared["document"]["id"].as_str().unwrap().to_owned()]);
    for (index, endpoint) in endpoints.iter().enumerate() {
        let row = signed_row(
            &author,
            &format!("Bicycle contribution from host {index}"),
            &endpoints,
        )
        .await;
        if index != 3 {
            expected.insert(row["document"]["id"].as_str().unwrap().to_owned());
        }
        pages.insert(
            endpoint.clone(),
            json!({"results":[shared,row],"updates":[]}),
        );
    }
    let network = Arc::new(SelectedServices {
        pages,
        failed: endpoints[3].clone(),
        entered: tokio::sync::Semaphore::new(0),
        release: tokio::sync::Semaphore::new(0),
        active: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        calls: Mutex::new(Vec::new()),
    });
    let transport: Arc<dyn Network> = network.clone();
    client.attach_social_network(transport);
    let mut services = endpoints.clone();
    services.push(endpoints[0].clone());
    let searching = client.clone();
    let search = tokio::spawn(async move {
        command(
            &searching,
            Command::Search {
                query: nucleus::social::Search {
                    text: "bicycle".into(),
                    ..Default::default()
                },
                services,
            },
        )
        .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        network.entered.acquire_many(3),
    )
    .await
    .unwrap()
    .unwrap()
    .forget();
    assert_eq!(network.calls.lock().unwrap().len(), 3);
    assert_eq!(network.peak.load(Ordering::SeqCst), 3);
    network.release.add_permits(4);
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), search)
        .await
        .unwrap()
        .unwrap();
    let calls: std::collections::BTreeSet<_> =
        network.calls.lock().unwrap().iter().cloned().collect();
    assert_eq!(calls, endpoints.iter().cloned().collect());
    assert_eq!(network.calls.lock().unwrap().len(), 4);
    assert_eq!(network.peak.load(Ordering::SeqCst), 3);
    assert_eq!(result["failures"][0]["service"], endpoints[3]);
    let rows = result["results"].as_array().unwrap();
    let found: std::collections::BTreeSet<_> = rows
        .iter()
        .map(|row| row["document"]["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(found, expected);
    let shared_row = rows
        .iter()
        .find(|row| row["document"]["id"] == shared["document"]["id"])
        .unwrap();
    assert_eq!(shared_row["sources"].as_array().unwrap().len(), 3);
    let before = network.calls.lock().unwrap().len();
    let local = command(
        &client,
        Command::Search {
            query: nucleus::social::Search {
                text: "bicycle".into(),
                ..Default::default()
            },
            services: vec![],
        },
    )
    .await;
    assert_eq!(local["results"].as_array().unwrap().len(), expected.len());
    assert_eq!(network.calls.lock().unwrap().len(), before);
    assert!(
        client
            .social_command(
                Command::Search {
                    query: Default::default(),
                    services: vec![endpoints[0].clone(), "invalid-endpoint".into()],
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    assert_eq!(network.calls.lock().unwrap().len(), before);
}

struct HangingService;

#[async_trait::async_trait]
impl Network for HangingService {
    async fn request(&self, _: &str, _: PublicRequest) -> Result<Value, EngineError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn an_unresponsive_selected_service_ends_at_its_deadline() {
    let client = Engine::open_memory().await.unwrap();
    let network: Arc<dyn Network> = Arc::new(HangingService);
    client.attach_social_network(network.clone());
    let endpoint = iroh::SecretKey::from_bytes(&[204; 32]).public().to_string();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        command(
            &client,
            Command::Search {
                query: Default::default(),
                services: vec![endpoint.clone()],
            },
        ),
    )
    .await
    .unwrap();
    assert!(result["results"].as_array().unwrap().is_empty());
    assert_eq!(result["failures"][0]["service"], endpoint);
}
