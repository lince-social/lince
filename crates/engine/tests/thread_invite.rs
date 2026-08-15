//! Thread invites (Ontology §11): what an Organ can put in front of you
//! before you have agreed to anything, and the two ways out of it.
//!
//! The properties worth pinning are all about restraint — an invite must not
//! travel, must not multiply, and must not decide anything on the user's
//! behalf.

use std::sync::Arc;

use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;

async fn cell(base_url: &str) -> (Arc<Engine>, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (Arc::new(e), organ)
}

/// Register `them` as a known contact of `us` — enough for the outbox to
/// consider them a sync destination.
async fn know(us: &Engine, organ_uid: &str) {
    store::organs::add_contact(&us.store.pool, organ_uid, None, "peer", "", 0)
        .await
        .expect("contact");
    store::organs::set_trust(&us.store.pool, organ_uid, "known")
        .await
        .expect("trust");
    store::organs::set_sync_policy(&us.store.pool, organ_uid, true, true)
        .await
        .expect("policy");
}

/// The property that makes an invite safe to receive: it is a LOCAL note.
///
/// Written through `records::create` it would be logged and enqueued to every
/// known contact — "Bea is asking to talk to me" pushed to everyone you know.
#[tokio::test]
async fn an_invite_is_local_and_is_never_pushed_to_anyone() {
    let (us, _) = cell("http://us.test").await;
    know(&us, "o-bystander").await;

    let organ = store::organs::local(&us.store.pool)
        .await
        .expect("local")
        .expect("organ")
        .uid;
    let before = store::sync_ops::after(&us.store.pool, &organ, 0, 1000)
        .await
        .expect("ops")
        .len();

    let invite = store::invites::put(&us.store.pool, "o-bea", "r-root", "Coffee")
        .await
        .expect("put")
        .expect("a first invite is accepted");

    let after = store::sync_ops::after(&us.store.pool, &organ, 0, 1000)
        .await
        .expect("ops");
    assert_eq!(
        after.len(),
        before,
        "an invite must log NO ops — it is a local notification, and the offer \
         it reports already arrived over the wire"
    );
    let queued: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM sync_outbox WHERE uid = ?")
        .bind(&invite.record_uid)
        .fetch_one(&us.store.pool)
        .await
        .expect("outbox");
    assert_eq!(queued, 0, "and nothing may be queued for any contact");
}

/// One pending per Organ. Without it, a declined conversation is just a spam
/// channel with extra steps.
#[tokio::test]
async fn one_organ_gets_one_pending_invite() {
    let (us, _) = cell("http://us.test").await;

    assert!(
        store::invites::put(&us.store.pool, "o-bea", "r-1", "First")
            .await
            .expect("put")
            .is_some()
    );
    assert!(
        store::invites::put(&us.store.pool, "o-bea", "r-2", "Again")
            .await
            .expect("put")
            .is_none(),
        "a second offer from the same Organ is refused while one is pending"
    );
    // And the refused one leaves nothing behind to render.
    assert_eq!(
        store::invites::pending(&us.store.pool)
            .await
            .expect("list")
            .len(),
        1
    );

    // A DIFFERENT Organ is unaffected: the cap is per sender, not a global
    // one-at-a-time queue.
    assert!(
        store::invites::put(&us.store.pool, "o-carla", "r-3", "Hello")
            .await
            .expect("put")
            .is_some()
    );
    assert_eq!(
        store::invites::pending(&us.store.pool)
            .await
            .expect("list")
            .len(),
        2
    );
}

/// Accepting opens the conversation and NOTHING else. Agreeing to read what
/// someone sends is not deciding who they are.
#[tokio::test]
async fn accepting_opens_the_conversation_without_deciding_anything_else() {
    let (us, _) = cell("http://us.test").await;
    store::organs::add_contact(&us.store.pool, "o-bea", None, "Bea", "", 1)
        .await
        .expect("contact");
    // `add_contact` defaults to `known`; start from `unknown` so this test
    // actually observes what accepting does rather than what setup did.
    store::organs::set_trust(&us.store.pool, "o-bea", "unknown")
        .await
        .expect("trust");
    store::replica::offer(&us.store.pool, "r-root", "o-bea")
        .await
        .expect("offer");
    let invite = store::invites::put(&us.store.pool, "o-bea", "r-root", "Coffee")
        .await
        .expect("put")
        .expect("invite");

    let root = us
        .act(
            Action::AcceptThreadInvite {
                invite: invite.record_uid.clone(),
            },
            None,
        )
        .await
        .expect("accept")
        .created
        .expect("the accepted root comes back");
    assert_eq!(root, "r-root");

    assert!(
        store::replica::is_accepted(&us.store.pool, "r-root", "o-bea")
            .await
            .expect("state"),
        "the grant is what actually lets their ops in"
    );
    assert!(
        store::invites::get(&us.store.pool, &invite.record_uid)
            .await
            .expect("get")
            .is_none(),
        "an answered invite is gone from the surface"
    );
    let contact = store::organs::contact(&us.store.pool, "o-bea")
        .await
        .expect("contact")
        .expect("row");
    assert_eq!(
        contact.trust, "unknown",
        "accepting must NOT promote them: deciding who someone is happens \
         inside the conversation, as its own deliberate act"
    );
    assert!(
        !contact.sync_out && !contact.sync_in,
        "and it enables no sync"
    );
}

/// Declining is an ANSWER, not a dismissal — it revokes the grant and frees
/// the sender to ask again later.
#[tokio::test]
async fn declining_revokes_the_grant_and_frees_the_slot() {
    let (us, _) = cell("http://us.test").await;
    store::replica::offer(&us.store.pool, "r-root", "o-bea")
        .await
        .expect("offer");
    let invite = store::invites::put(&us.store.pool, "o-bea", "r-root", "Coffee")
        .await
        .expect("put")
        .expect("invite");

    us.act(
        Action::DeclineThreadInvite {
            invite: invite.record_uid.clone(),
        },
        None,
    )
    .await
    .expect("decline");

    assert!(
        !store::replica::is_accepted(&us.store.pool, "r-root", "o-bea")
            .await
            .expect("state"),
        "a declined conversation must not stay reachable"
    );
    assert!(
        store::replica::state(&us.store.pool, "r-root", "o-bea")
            .await
            .expect("state")
            .is_none(),
        "the grant row is gone, so the sender is not left waiting on an answer \
         that never comes"
    );
    // The slot is free: they may ask once more. That is the ONE thing that
    // remains possible after a refusal.
    assert!(
        store::invites::put(&us.store.pool, "o-bea", "r-later", "Try again")
            .await
            .expect("put")
            .is_some()
    );
}

/// The invite Record is an ordinary Record, so it is visible to Protein
/// without a new source — which is how a surface renders the queue.
#[tokio::test]
async fn invites_are_visible_through_protein_like_any_other_record() {
    let (us, _) = cell("http://us.test").await;
    store::invites::put(&us.store.pool, "o-bea", "r-root", "Coffee")
        .await
        .expect("put")
        .expect("invite");

    let protein: protein::Protein = serde_json::from_value(serde_json::json!({
        "source": "record",
        "where": [{ "kind_eq": "thread_invite" }],
        "include": { "extension": { "namespace": store::invites::EXTENSION } },
    }))
    .expect("protein");
    let rows = protein::execute(&us.store, &protein)
        .await
        .expect("execute");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["head"], "Coffee");
    assert_eq!(
        rows[0]["extension"]["from_organ"], "o-bea",
        "the sender travels in the extension so a surface can say who is asking"
    );
    assert_eq!(rows[0]["extension"]["root"], "r-root");
}

/// The invite as it actually arrives: over the wire, from a real Organ, and
/// answered by the local user rather than by the protocol.
///
/// This is the seam the unit tests above cannot reach — the offer handler is
/// what turns a wire request into something a person sees.
#[tokio::test]
async fn an_offer_over_the_wire_becomes_an_invite_the_user_answers() {
    use engine::wire::{ALPN_SYNC, Reach, Wire, WireRequest, WireResponse};
    use iroh::{EndpointAddr, SecretKey};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn loopback(wire: &Wire) -> EndpointAddr {
        let port = wire
            .endpoint()
            .bound_sockets()
            .into_iter()
            .map(|addr| addr.port())
            .next()
            .expect("endpoint is bound");
        EndpointAddr::new(wire.node_id())
            .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }

    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), SecretKey::from_bytes(&[41; 32]), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), SecretKey::from_bytes(&[42; 32]), Reach::Local)
        .await
        .expect("b binds");

    // Each knows the other: the sync ALPN is where offers travel.
    know(&a, &b_organ).await;
    store::organs::set_node_id(&a.store.pool, &b_organ, Some(&b_wire.node_id().to_string()))
        .await
        .expect("node id");
    know(&b, &a_organ).await;
    store::organs::set_node_id(&b.store.pool, &a_organ, Some(&a_wire.node_id().to_string()))
        .await
        .expect("node id");

    let (conversation, _thread) = a
        .start_conversation(&b_organ, "Beach plans")
        .await
        .expect("conversation");

    let b_addr = loopback(&b_wire);
    let serving = {
        let b_wire = b_wire.clone();
        tokio::spawn(async move { b_wire.serve().await })
    };

    let response = a_wire
        .request(
            b_addr.clone(),
            ALPN_SYNC,
            &WireRequest::OfferGrant {
                root: conversation.clone(),
                title: "Beach plans".into(),
                intro: a.introduction().await.expect("A introduction"),
            },
        )
        .await
        .expect("offer is delivered");
    assert!(matches!(response, WireResponse::Applied { .. }));

    let pending = store::invites::pending(&b.store.pool)
        .await
        .expect("pending");
    assert_eq!(
        pending.len(),
        1,
        "the offer surfaced as something B can answer"
    );
    assert_eq!(
        pending[0].from_organ, a_organ,
        "the sender is the Organ the CONNECTION proved, never a name in the body"
    );
    assert_eq!(pending[0].root, conversation);

    // A second offer from the same Organ is absorbed silently — the sender is
    // told the same thing either way, so they cannot learn whether the first
    // was declined or merely unanswered.
    let again = a_wire
        .request(
            b_addr,
            ALPN_SYNC,
            &WireRequest::OfferGrant {
                root: conversation.clone(),
                title: "Beach plans".into(),
                intro: a.introduction().await.expect("A introduction"),
            },
        )
        .await
        .expect("second offer is delivered");
    assert!(matches!(again, WireResponse::Applied { .. }));
    assert_eq!(
        store::invites::pending(&b.store.pool)
            .await
            .expect("pending")
            .len(),
        1,
        "still one pending: an Organ gets one, however many times it asks"
    );

    // B answers it, and only now is the conversation theirs to receive.
    b.act(
        Action::AcceptThreadInvite {
            invite: pending[0].record_uid.clone(),
        },
        None,
    )
    .await
    .expect("accept");
    assert!(
        store::replica::is_accepted(&b.store.pool, &conversation, &a_organ)
            .await
            .expect("state")
    );

    serving.abort();
}

/// An invite must WAKE the board, not wait to be asked for.
///
/// Notifications are the one thing the board learns about that commits no
/// Fact — an invite is a local note in its own side table, so the `fact_bus`
/// cannot carry it. That gap used to be covered by a `fetch` on a two-second
/// interval running for as long as any board was open. This watch is what
/// replaced it, so the properties it has to hold are: it fires when an invite
/// lands, `notifications()` then describes it, and answering fires it again.
#[tokio::test]
async fn a_pending_invite_wakes_watchers_and_answering_wakes_them_again() {
    let (us, _organ) = cell("http://us.test").await;
    let (them, their_organ) = cell("http://them.test").await;
    know(&us, &their_organ).await;
    // The far side has to hold the conversation Record for `accept_invite` to
    // have something to accept.
    let (root, _thread) = them
        .start_conversation(&their_organ, "Coffee")
        .await
        .expect("their conversation");

    let mut watch = us.watch_notifications();
    assert!(
        us.notifications().await.expect("read").is_empty(),
        "nothing is pending before anyone asks"
    );

    store::invites::put(&us.store.pool, &their_organ, &root, "Coffee")
        .await
        .expect("invite lands");
    us.notify_notifications_changed();

    assert!(watch.has_changed().expect("watch alive"), "the board must be woken");
    watch.mark_unchanged();
    let pending = us.notifications().await.expect("read");
    assert_eq!(pending.len(), 1, "and told exactly what is waiting");
    assert_eq!(pending[0]["recordId"], root.as_str(), "pointing at the conversation");
    assert_eq!(pending[0]["organId"], their_organ.as_str(), "and at who is asking");
    let invite_uid = pending[0]["id"].as_str().expect("invite uid").to_string();

    us.decline_invite(&invite_uid).await.expect("decline");
    assert!(
        watch.has_changed().expect("watch alive"),
        "answering must wake them too, or the badge never clears"
    );
    assert!(
        us.notifications().await.expect("read").is_empty(),
        "and the list is empty again"
    );
}
