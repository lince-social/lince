use super::serving::{all, approve, counts, fetch, peer, review};
use super::*;
use nucleus::{
    social::PublicRequest,
    trust_analysis::{
        Condition, Filter,
        discovery::{Page, Question, Request},
        serving::{self, Artifact, Chunk, SourceKind},
    },
};
use std::collections::BTreeSet;

pub(super) fn question(question: Question) -> Request {
    let now = nucleus::execution::now().timestamp();
    Request {
        id: nucleus::new_uid("ask"),
        question,
        issued_at: now,
        deadline: now + 15,
        after: None,
    }
}

pub(super) async fn discover(
    node: &Engine,
    endpoint: &str,
    request: &Request,
) -> Result<Page, engine::EngineError> {
    let value = node
        .social_public_request(
            endpoint,
            &iroh::SecretKey::from_bytes(&[240; 32]).public().to_string(),
            PublicRequest::DiscoverEvidence {
                document: Box::new(request.clone()),
            },
            nucleus::execution::now().timestamp(),
        )
        .await?;
    let page: Page = serde_json::from_value(value["page"].clone())?;
    page.validate(request, nucleus::execution::now().timestamp())
        .unwrap();
    Ok(page)
}

#[test]
fn discovery_pages_only_approved_evidence_bind_the_question_and_endpoint_and_stop_without_effects()
{
    support::run_async_test("evidence-discovery-pages", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let outsider = support::engine().await;
        let a = support::person(&node, "discovery.author").await;
        authorize(&node, &a).await;
        let (reader_organ, endpoint, _) = peer(&node, &reader, &a, 241).await;
        let (_, excluded, _) = peer(&node, &outsider, &a, 242).await;
        let audience = Filter {
            include: vec![Condition {
                identities: vec![Identity::Organ(reader_organ)],
                ..Default::default()
            }],
            exclude: Vec::new(),
        };
        let mut private_ids = Vec::new();
        let mut grants = std::collections::BTreeMap::new();
        for index in 0..10 {
            let b = support::person(&node, &format!("discovery.contact.{index}")).await;
            let document = preview(
                &node,
                &a,
                draft(&a, &b, 0, false),
                &format!("discovery:preview:{index}"),
            )
            .await;
            let record = publish(&node, &a, &document, &format!("discovery:contact:{index}")).await;
            private_ids.push(record.clone());
            if index == 9 {
                continue;
            }
            let reviewed = review(&node, &a, &record, SourceKind::Contact, audience.clone()).await;
            let grant = approve(&node, &a, &reviewed, &format!("discovery:grant:{index}"))
                .await
                .unwrap()
                .created
                .unwrap();
            private_ids.push(grant.clone());
            let hash: nucleus::karma::CanonicalHash =
                serde_json::from_value(reviewed["artifact_hash"].clone()).unwrap();
            grants.insert(hash, grant);
        }
        let mut request = question(Question::Contacts {
            subject: Identity::Person(a.uid.clone()),
        });
        let before = counts(&node).await;
        let first = discover(&node, &endpoint, &request).await.unwrap();
        assert_eq!(first.headers.len(), 8);
        assert!(first.partial && first.next.is_some());
        let encoded = serde_json::to_string(&first).unwrap();
        assert!(private_ids.iter().all(|uid| !encoded.contains(uid)));
        assert!(!encoded.contains(&a.uid));
        let mut next = request.clone();
        next.after = first.next.clone();
        let mut changed = next.clone();
        changed.id = nucleus::new_uid("ask");
        assert!(discover(&node, &endpoint, &changed).await.is_err());
        changed = next.clone();
        changed.question = Question::Contacts {
            subject: Identity::Organ(a.uid.clone()),
        };
        assert!(discover(&node, &endpoint, &changed).await.is_err());
        assert!(discover(&node, &excluded, &next).await.is_err());
        let last = discover(&node, &endpoint, &next).await.unwrap();
        assert_eq!(last.headers.len(), 1);
        assert!(!last.partial && last.next.is_none());
        let hashes = first
            .headers
            .iter()
            .chain(&last.headers)
            .map(|header| header.hash.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(hashes, grants.keys().cloned().collect());
        assert!(
            discover(&node, &excluded, &request)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        let unknown = iroh::SecretKey::from_bytes(&[243; 32]).public().to_string();
        assert!(discover(&node, &unknown, &request).await.is_err());
        let header = &first.headers[0];
        let chunk: Chunk = serde_json::from_value(
            fetch(&node, &endpoint, &header.hash).await.unwrap()["chunk"].clone(),
        )
        .unwrap();
        assert_eq!(chunk.total, header.bytes);
        let artifact = serving::assemble(
            &header.hash,
            &[chunk],
            nucleus::execution::now().timestamp(),
        )
        .unwrap();
        assert!(request.question.matches(&artifact));
        assert_eq!(counts(&node).await, before);
        node.act(
            Action::DeleteRecord {
                target: grants[&header.hash].clone(),
            },
            None,
        )
        .await
        .unwrap();
        assert!(fetch(&node, &endpoint, &header.hash).await.is_err());
        request.id = nucleus::new_uid("ask");
        let current = discover(&node, &endpoint, &request).await.unwrap();
        assert_eq!(current.headers.len(), 8);
        assert!(
            current
                .headers
                .iter()
                .all(|candidate| candidate.hash != header.hash)
        );
    });
}

#[test]
fn discovery_of_a_claim_series_preserves_changed_scope_withdrawals_and_explicit_serving_approval() {
    support::run_async_test("evidence-discovery-series", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let a = support::person(&node, "discovery.claim.author").await;
        authorize(&node, &a).await;
        let (_, endpoint, _) = peer(&node, &reader, &a, 244).await;
        let initial = claims::reviewed(
            &node,
            &a,
            claims::draft(&a, true),
            "discovery:claim-preview",
        )
        .await;
        let record = claims::publish(&node, &a, &initial, "discovery:claim-publish")
            .await
            .unwrap()
            .created
            .unwrap();
        let original =
            store::profile_claims::get_on(&mut *node.store.pool.acquire().await.unwrap(), &record)
                .await
                .unwrap()
                .document;
        let series = question(Question::ClaimSeries {
            issuer: Identity::Person(a.uid.clone()),
            claim: original.statement.claim.clone(),
        });
        assert!(
            discover(&node, &endpoint, &series)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        let reviewed = review(&node, &a, &record, SourceKind::Claim, all()).await;
        approve(&node, &a, &reviewed, "discovery:claim-grant")
            .await
            .unwrap();
        let first = discover(&node, &endpoint, &series).await.unwrap();
        assert_eq!(first.headers.len(), 1);
        let mut changed = claims::draft(&a, true);
        changed.claim = Some(original.statement.claim.clone());
        changed.expected_revision = 1;
        changed.descriptor.role = nucleus::transfer::outcome::Role::Receiver;
        changed.amount = "10".into();
        let reviewed =
            claims::reviewed(&node, &a, changed.clone(), "discovery:changed-preview").await;
        let record = claims::publish(&node, &a, &reviewed, "discovery:changed-publish")
            .await
            .unwrap()
            .created
            .unwrap();
        let series = question(series.question);
        assert!(
            discover(&node, &endpoint, &series)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        let reviewed = review(&node, &a, &record, SourceKind::Claim, all()).await;
        approve(&node, &a, &reviewed, "discovery:changed-grant")
            .await
            .unwrap();
        let second = discover(&node, &endpoint, &series).await.unwrap();
        assert_eq!(second.headers.len(), 1);
        assert_ne!(second.headers[0].hash, first.headers[0].hash);
        let activity = question(Question::Activity {
            subject: Identity::Person(a.uid.clone()),
            role: nucleus::transfer::outcome::Role::Giver,
            measurements: Vec::new(),
            from: None,
            until: None,
        });
        assert!(
            discover(&node, &endpoint, &activity)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        changed.expected_revision = 2;
        changed.withdrawn = true;
        changed.amount = "0".into();
        let withdrawn = claims::reviewed(&node, &a, changed, "discovery:withdrawal-preview").await;
        let record = claims::publish(&node, &a, &withdrawn, "discovery:withdrawal")
            .await
            .unwrap()
            .created
            .unwrap();
        let series = question(series.question);
        assert!(
            discover(&node, &endpoint, &series)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        let reviewed = review(&node, &a, &record, SourceKind::Claim, all()).await;
        approve(&node, &a, &reviewed, "discovery:withdrawal-grant")
            .await
            .unwrap();
        let before = counts(&node).await;
        let result = discover(&node, &endpoint, &series).await.unwrap();
        assert_eq!(result.headers.len(), 1);
        let header = &result.headers[0];
        let chunk: Chunk = serde_json::from_value(
            fetch(&node, &endpoint, &header.hash).await.unwrap()["chunk"].clone(),
        )
        .unwrap();
        let Artifact::Claim(document) = serving::assemble(
            &header.hash,
            &[chunk],
            nucleus::execution::now().timestamp(),
        )
        .unwrap() else {
            panic!("Expected the original claim")
        };
        assert!(document.statement.withdrawn);
        assert_eq!(document.statement.revision, 3);
        assert_eq!(counts(&node).await, before);
    });
}
