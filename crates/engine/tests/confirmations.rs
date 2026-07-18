//! Part VIII completion: delivery/receipt confirmations gate settlement, the
//! advisory per-concept balance, and transfer chat via the generic threads.

use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity,
        },
    )
    .await
    .expect("record")
    .uid
}

async fn person(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Person,
            head: slug,
            body: "",
            quantity: 1.0,
        },
    )
    .await
    .expect("person")
    .uid
}

/// Build an agreed, activated single-promise transfer with Maria as the party.
async fn agreed_transfer(e: &Engine, require_confirmation: bool) -> (String, String) {
    plain(e, "ana.apples", 10.0).await;
    person(e, "maria").await;
    let transfer = e
        .act(
            Action::CreateTransfer {
                slug: Some("xfer.apples".into()),
                head: "Apples".into(),
                agreement: "individual".into(),
                agreement_pct: None,
                satiation: None,
                source: None,
                reserve_default: None,
                require_confirmation,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let party = e
        .act(
            Action::AddParty {
                transfer: transfer.clone(),
                actor: "maria".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    e.act(
        Action::AddPromiseToTransfer {
            transfer: transfer.clone(),
            record: "ana.apples".into(),
            delta: -5.0,
            party: "maria".into(),
            window_end: None,
            condition: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AgreeTransfer {
            transfer: transfer.clone(),
            party,
            level: 2,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::ActivateTransfer {
            transfer: transfer.clone(),
        },
        None,
    )
    .await
    .unwrap();
    let maria = store::records::resolve(&e.store.pool, "maria")
        .await
        .unwrap()
        .unwrap()
        .uid;
    (transfer, maria)
}

#[tokio::test]
async fn confirmations_gate_settlement_when_demanded() {
    let e = engine().await;
    let (transfer, maria) = agreed_transfer(&e, true).await;

    let settle = e
        .act(
            Action::SettleTransfer {
                transfer: transfer.clone(),
                actor: maria.clone(),
            },
            None,
        )
        .await;
    assert!(settle.is_err(), "no confirmations yet: settlement refused");

    e.act(
        Action::ConfirmTransfer {
            transfer: transfer.clone(),
            confirmation: "delivery".into(),
        },
        None,
    )
    .await
    .unwrap();
    let settle = e
        .act(
            Action::SettleTransfer {
                transfer: transfer.clone(),
                actor: maria.clone(),
            },
            None,
        )
        .await;
    assert!(settle.is_err(), "one of two confirmations is not enough");

    e.act(
        Action::ConfirmTransfer {
            transfer: transfer.clone(),
            confirmation: "receipt".into(),
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SettleTransfer {
            transfer: transfer.clone(),
            actor: maria,
        },
        None,
    )
    .await
    .expect("both confirmations in: settlement proceeds");
    assert_eq!(
        store::records::quantity(
            &e.store.pool,
            &store::records::resolve(&e.store.pool, "ana.apples")
                .await
                .unwrap()
                .unwrap()
                .uid
        )
        .await
        .unwrap(),
        Some(5.0)
    );

    // bad confirmation kinds are rejected
    assert!(
        e.act(
            Action::ConfirmTransfer {
                transfer,
                confirmation: "vibes".into(),
            },
            None,
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn settlement_without_the_demand_needs_no_confirmation() {
    let e = engine().await;
    let (transfer, maria) = agreed_transfer(&e, false).await;
    e.act(
        Action::SettleTransfer {
            transfer,
            actor: maria,
        },
        None,
    )
    .await
    .expect("no confirmation demanded: settles directly");
}

#[tokio::test]
async fn balance_is_advisory_per_concept_and_chat_rides_threads() {
    let e = engine().await;
    let money = store::concepts::create(&e.store.pool, "money", &[])
        .await
        .unwrap();
    let ana_money = plain(&e, "ana.money", 300.0).await;
    let carlos_money = plain(&e, "carlos.money", 300.0).await;
    store::records::set_concept(&e.store.pool, &ana_money, Some(&money))
        .await
        .unwrap();
    store::records::set_concept(&e.store.pool, &carlos_money, Some(&money))
        .await
        .unwrap();
    person(&e, "ana").await;
    person(&e, "carlos").await;

    let transfer = e
        .act(
            Action::CreateTransfer {
                slug: Some("xfer.sale".into()),
                head: "Sale".into(),
                agreement: "full".into(),
                agreement_pct: None,
                satiation: None,
                source: None,
                reserve_default: None,
                require_confirmation: false,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    // money moves: -300 from Carlos, +300 to Ana -> balanced per @money
    for (record, delta, party) in [("carlos.money", -300.0, "carlos"), ("ana.money", 300.0, "ana")]
    {
        e.act(
            Action::AddPromiseToTransfer {
                transfer: transfer.clone(),
                record: record.into(),
                delta,
                party: party.into(),
                window_end: None,
                condition: None,
            },
            None,
        )
        .await
        .unwrap();
    }

    let q = protein::Protein {
        source: protein::Source::Transfer,
        filter: vec![protein::Predicate::UidEq(transfer.clone())],
        include: protein::Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["balanced"], true, "a trade sums to zero per concept");
    assert_eq!(rows[0]["balance"][&money], 0.0);

    // transfer chat is just threads on the transfer record (Window case 6)
    let thread = e
        .act(
            Action::CreateThread {
                target: transfer.clone(),
                head: "Negotiation".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    e.act(
        Action::CreateMessage {
            thread,
            body: "can we do saturday?".into(),
            parent: None,
        },
        None,
    )
    .await
    .unwrap();
    let q = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::UidEq(transfer)],
        include: protein::Include {
            threads: Some(protein::ThreadsInclude {
                messages_limit: 10,
            }),
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["threads"][0]["messages"][0]["body"], "can we do saturday?");
}
