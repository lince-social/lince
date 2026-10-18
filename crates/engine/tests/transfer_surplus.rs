mod support;

use engine::{
    Engine,
    actions::{Action, TransferReservePoint},
};
use nucleus::transfer::{
    AgreementType,
    disclosure::TransferItem,
    exchange::ExchangeRoute,
    surplus::{Command, Definition, Mode, Preview, Recipe, Review, Saved},
};
use serde_json::Value;
use support::{DraftOptions, Person, TransferFixture};

const START: i64 = 1_893_456_000_000;
const DAY: i64 = 86_400_000;

struct Fixture {
    engine: Engine,
    owner: Person,
    other: Person,
    stock: String,
    transfer: TransferFixture,
    definition: Definition,
}

async fn fixture(amount: f64, reserve: TransferReservePoint, mode: Mode) -> Fixture {
    let engine = support::karma::engine().await;
    let owner = support::person(&engine, "stock-owner").await;
    let other = support::person(&engine, "stock-receiver").await;
    let role = store::auth::ensure_role(&engine.store.pool, "stock-controller")
        .await
        .unwrap();
    for (resource, actions) in [
        ("record", vec!["read", "create", "update"]),
        ("transfer", vec!["read", "create", "update"]),
        ("frequency", vec!["create", "update"]),
    ] {
        for action in actions {
            let permission = store::auth::ensure_permission(&engine.store.pool, resource, action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
    }
    store::auth::create_credential(&engine.store.pool, &owner.uid, "stock-owner", "hash", role)
        .await
        .unwrap();
    let stock = support::plain(&engine, "apples", amount).await;
    let mut promise = support::promise("apples-offer", &stock, &owner, -10.0);
    promise.item = Some(TransferItem {
        title: "Apples".into(),
        exchange: Some(ExchangeRoute {
            uid: "apples-route".into(),
            giver: owner.uid.clone(),
            receiver: other.uid.clone(),
        }),
        ..Default::default()
    });
    let transfer = support::create_transfer(
        &engine,
        &owner,
        std::slice::from_ref(&other),
        vec![promise],
        DraftOptions {
            slug: "apples-offer",
            agreement: AgreementType::Full,
            reserve_default: reserve,
            require_confirmation: true,
        },
    )
    .await;
    engine.set_signer(owner.signer.clone()).await.unwrap();
    let definition = Definition {
        uid: nucleus::new_uid("r"),
        controller_uid: owner.uid.clone(),
        stock_uid: stock.clone(),
        recipe: Recipe::Offer {
            transfer_uid: transfer.transfer.clone(),
            promise_uid: "apples-offer".into(),
            retain: "20".into(),
            mode,
        },
    };
    Fixture {
        engine,
        owner,
        other,
        stock,
        transfer,
        definition,
    }
}

async fn command(f: &Fixture, request: Command) -> Value {
    f.engine.set_signer(f.owner.signer.clone()).await.unwrap();
    f.engine
        .act(Action::Surplus { request }, Some(f.owner.uid.clone()))
        .await
        .unwrap()
        .data
        .unwrap()["stock"]
        .clone()
}

async fn review(f: &Fixture) -> (Review, String) {
    let data = command(
        f,
        Command::Review {
            definition: f.definition.clone(),
        },
    )
    .await;
    (
        serde_json::from_value(data["review"].clone()).unwrap(),
        data["commitment"].as_str().unwrap().into(),
    )
}

async fn enable(f: &Fixture, reviewed: &(Review, String)) -> Saved {
    serde_json::from_value(
        command(
            f,
            Command::Enable {
                review: Box::new(reviewed.0.clone()),
                commitment: reviewed.1.clone(),
            },
        )
        .await["saved"]
            .clone(),
    )
    .unwrap()
}

async fn inspect(f: &Fixture) -> (Preview, Option<Saved>) {
    let data = command(
        f,
        Command::Inspect {
            definition: f.definition.clone(),
        },
    )
    .await;
    (
        serde_json::from_value(data["preview"].clone()).unwrap(),
        serde_json::from_value(data["saved"].clone()).unwrap(),
    )
}

fn exact(actual: &str, expected: &str) {
    assert_eq!(
        nucleus::DecimalValue::parse_inferred(actual)
            .unwrap()
            .exact_numeric_cmp(nucleus::DecimalValue::parse_inferred(expected).unwrap()),
        std::cmp::Ordering::Equal,
        "{actual} != {expected}"
    );
}

async fn preview(f: &Fixture, expected: [&str; 3]) -> Preview {
    let (value, _) = inspect(f).await;
    for (actual, expected) in [&value.actual, &value.offered, &value.personal]
        .into_iter()
        .zip(expected)
    {
        exact(actual, expected);
    }
    value
}

async fn balance(f: &Fixture, expected: [&str; 3]) {
    let value = store::transfer_balances::read(&f.engine.store.pool, &f.stock)
        .await
        .unwrap();
    assert!(value.incomplete.is_empty(), "{:?}", value.incomplete);
    for (actual, expected) in [value.actual, value.reserved, value.surplus]
        .into_iter()
        .zip(expected)
    {
        exact(&actual.to_string(), expected);
    }
}

async fn consume(f: &Fixture, id: &str) {
    f.engine.set_signer(f.owner.signer.clone()).await.unwrap();
    let action = Action::CaptureEntry {
        target: f.stock.clone(),
        amount: "-1".into(),
        concept: None,
        note: Some("One apple consumed".into()),
        at: None,
        request_id: Some(id.into()),
    };
    let first = f
        .engine
        .act(action.clone(), Some(f.owner.uid.clone()))
        .await
        .unwrap();
    let after_first = store::records::get(&f.engine.store.pool, &f.stock)
        .await
        .unwrap()
        .unwrap()
        .quantity;
    let replay = f
        .engine
        .act(action, Some(f.owner.uid.clone()))
        .await
        .unwrap();
    assert!(replay.facts.is_empty());
    let entry = store::entries::replayed(&f.engine.store.pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.created.as_deref(), Some(entry.uid.as_str()));
    assert_eq!(
        store::records::get(&f.engine.store.pool, &f.stock)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        after_first
    );
}

async fn drain(f: &Fixture) {
    f.engine.set_signer(f.owner.signer.clone()).await.unwrap();
    for _ in 0..8 {
        let result = f.engine.run_database_effects().await.unwrap();
        assert!(
            result.outcomes.iter().all(|outcome| outcome.ok),
            "{:?}",
            result.outcomes
        );
        if !result.pending {
            return;
        }
        assert!(
            !result.unsupported,
            "Stock planning requires only ordinary database effects"
        );
    }
    panic!("Stock planning must quiesce without a reactive loop");
}

async fn no_delivery(f: &Fixture) {
    assert!(
        store::transfers::occurrences_of(&f.engine.store.pool, &f.transfer.transfer)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.engine.location_sources().await.is_empty());
    let row = store::records::get(&f.engine.store.pool, &f.stock)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.kind, "plain");
}

async fn rule_count(f: &Fixture) -> i64 {
    store::sqlx::query_scalar("SELECT COUNT(*) FROM recurrence WHERE record_uid=? AND json_extract(consequences_json,'$[0].kind')='refresh-stock-policy'").bind(&f.stock).fetch_one(&f.engine.store.pool).await.unwrap()
}

fn run(name: &str, test: impl std::future::Future<Output = ()> + Send + 'static) {
    support::run_async_test(name, || async move {
        nucleus::execution::Execution::new([91; 32], START)
            .unwrap()
            .scope(test)
            .await;
    });
}

#[test]
fn recompute_revises_only_unaccepted_surplus_and_freezes_the_signed_amount() {
    run("stock-recompute", async {
        let mut f = fixture(30.0, TransferReservePoint::Agreed, Mode::RecomputeOffer).await;
        let reviewed = review(&f).await;
        let saved = enable(&f, &reviewed).await;
        assert_eq!(enable(&f, &reviewed).await.rule_uid, saved.rule_uid);
        assert_eq!(rule_count(&f).await, 1);
        consume(&f, "consume-before-agreement").await;
        drain(&f).await;
        let unaccepted = preview(&f, ["29", "9", "20"]).await;
        exact(&unaccepted.proposed, "9");
        assert!(!unaccepted.frozen);
        balance(&f, ["29", "0", "29"]).await;
        no_delivery(&f).await;
        let revised = support::current_revision(&f.engine, &f.transfer.transfer).await;
        assert_eq!(revised, f.transfer.revision + 1);
        let fact =
            store::transfers::revision_fact(&f.engine.store.pool, &f.transfer.transfer, revised)
                .await
                .unwrap()
                .unwrap();
        assert!(
            engine::trust::verify_fact(&f.engine.store, &fact)
                .await
                .unwrap()
        );
        drain(&f).await;
        assert_eq!(
            support::current_revision(&f.engine, &f.transfer.transfer).await,
            revised
        );
        f.transfer.revision = revised;
        for person in [&f.owner, &f.other] {
            support::agree(
                &f.engine,
                &f.transfer,
                person,
                &format!("agree:{}", person.uid),
            )
            .await;
        }
        consume(&f, "consume-after-agreement").await;
        drain(&f).await;
        assert!(preview(&f, ["28", "9", "19"]).await.frozen);
        balance(&f, ["28", "9", "19"]).await;
        assert_eq!(
            support::current_revision(&f.engine, &f.transfer.transfer).await,
            revised
        );
        no_delivery(&f).await;
        let state = inspect(&f).await.1.unwrap();
        command(
            &f,
            Command::Pause {
                policy_uid: state.review.definition.uid,
                controller_uid: f.owner.uid.clone(),
                expected_revision: state.revision,
            },
        )
        .await;
        consume(&f, "consume-while-paused").await;
        drain(&f).await;
        assert!(!inspect(&f).await.1.unwrap().enabled);
        assert_eq!(
            support::current_revision(&f.engine, &f.transfer.transfer).await,
            revised
        );
    });
}

#[test]
fn stale_stock_reviews_and_atomic_offer_guards_cannot_change_current_terms() {
    run("stock-review-guards", async {
        for change in ["stock", "approval", "tamper", "actor"] {
            let f = fixture(30.0, TransferReservePoint::Agreed, Mode::RecomputeOffer).await;
            let (mut reviewed, commitment) = review(&f).await;
            match change {
                "stock" => consume(&f, "change-stock-after-review").await,
                "approval" => {
                    f.engine.set_signer(f.other.signer.clone()).await.unwrap();
                    f.engine
                        .act(
                            Action::SetTransferAgreementLevel {
                                transfer: f.transfer.transfer.clone(),
                                expected_revision: f.transfer.revision,
                                request_id: "review-by-receiver".into(),
                                person: Some(f.other.uid.clone()),
                                level: 1,
                            },
                            None,
                        )
                        .await
                        .unwrap();
                }
                "tamper" => reviewed.preview.proposed = "1000".into(),
                "actor" => {}
                _ => unreachable!(),
            }
            f.engine.set_signer(f.owner.signer.clone()).await.unwrap();
            let actor = if change == "actor" {
                &f.other.uid
            } else {
                &f.owner.uid
            };
            assert!(
                f.engine
                    .act(
                        Action::Surplus {
                            request: Command::Enable {
                                review: Box::new(reviewed.clone()),
                                commitment
                            }
                        },
                        Some(actor.clone())
                    )
                    .await
                    .is_err()
            );
            assert_eq!(rule_count(&f).await, 0);
            assert_eq!(
                support::current_revision(&f.engine, &f.transfer.transfer).await,
                f.transfer.revision
            );
            if matches!(change, "stock" | "approval") {
                let mut input = store::transfers::publication_input(
                    &f.engine.store.pool,
                    &f.transfer.transfer,
                    f.transfer.revision,
                    &f.owner.uid,
                    "stale-guarded-revision",
                )
                .await
                .unwrap();
                input.promises[0].delta = -9.0;
                let guard = nucleus::transfer::karma::Guard::exact(&reviewed.source);
                assert!(
                    store::transfers::revise_whole_draft_guarded(
                        &f.engine.store.pool,
                        input,
                        &guard,
                        (&f.stock, &reviewed.basis),
                        nucleus::execution::now(),
                        Some(f.owner.uid.clone()),
                        |hash| Some(f.owner.signer.sign_hash(hash))
                    )
                    .await
                    .is_err()
                );
                assert_eq!(
                    support::current_revision(&f.engine, &f.transfer.transfer).await,
                    f.transfer.revision
                );
            }
            no_delivery(&f).await;
        }
    });
}

#[test]
fn preserve_offer_counts_its_reservation_once_and_normal_cancellation_releases_once() {
    run("stock-preserve", async {
        for point in [TransferReservePoint::Proposed, TransferReservePoint::Agreed] {
            let mut f = fixture(30.0, point, Mode::PreserveOffer).await;
            let reviewed = review(&f).await;
            let saved = enable(&f, &reviewed).await;
            consume(&f, "preserve-consumption").await;
            drain(&f).await;
            let before = preview(&f, ["29", "10", "19"]).await;
            exact(&before.proposed, "10");
            let reserved = if point == TransferReservePoint::Proposed {
                "10"
            } else {
                "0"
            };
            balance(
                &f,
                ["29", reserved, if reserved == "10" { "19" } else { "29" }],
            )
            .await;
            assert_eq!(
                inspect(&f).await.1.unwrap().review.definition.recipe,
                f.definition.recipe
            );
            assert_eq!(enable(&f, &reviewed).await.rule_uid, saved.rule_uid);
            assert_eq!(rule_count(&f).await, 1);
            assert_eq!(
                support::current_revision(&f.engine, &f.transfer.transfer).await,
                f.transfer.revision
            );
            no_delivery(&f).await;
            for person in [&f.owner, &f.other] {
                support::agree(
                    &f.engine,
                    &f.transfer,
                    person,
                    &format!("preserve-agree:{}", person.uid),
                )
                .await;
            }
            let agreed = preview(&f, ["29", "10", "19"]).await;
            exact(&agreed.reserved_offer, "10");
            balance(&f, ["29", "10", "19"]).await;
            let occurrence = support::activate(
                &f.engine,
                &f.transfer,
                &f.owner,
                "apples-offer",
                "preserve-activate",
            )
            .await;
            let proposal = Action::ProposeTransferCancellation {
                transfer: f.transfer.transfer.clone(),
                occurrence,
                expected_revision: f.transfer.revision,
                expected_remaining_quantity: store::exact::integer(10),
                person: Some(f.owner.uid.clone()),
                request_id: "preserve-cancel-proposal".into(),
            };
            let cancellation = f
                .engine
                .act(proposal.clone(), None)
                .await
                .unwrap()
                .created
                .unwrap();
            assert_eq!(
                f.engine
                    .act(proposal, None)
                    .await
                    .unwrap()
                    .created
                    .as_deref(),
                Some(cancellation.as_str())
            );
            f.transfer.revision = support::current_revision(&f.engine, &f.transfer.transfer).await;
            for person in [&f.owner, &f.other] {
                support::agree(
                    &f.engine,
                    &f.transfer,
                    person,
                    &format!("cancel-agree:{}", person.uid),
                )
                .await;
            }
            f.engine.set_signer(f.owner.signer.clone()).await.unwrap();
            let release = Action::ApplyTransferCancellation {
                transfer: f.transfer.transfer.clone(),
                cancellation,
                expected_revision: f.transfer.revision,
                person: Some(f.owner.uid.clone()),
                request_id: "preserve-release".into(),
            };
            f.engine.act(release.clone(), None).await.unwrap();
            f.engine.act(release, None).await.unwrap();
            balance(&f, ["29", "0", "29"]).await;
            let cancelled: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence_cancellation")
                    .fetch_one(&f.engine.store.pool)
                    .await
                    .unwrap();
            assert_eq!(cancelled, 1);
        }
    });
}

#[test]
fn apples_daily_consumption_proposes_one_need_with_upstream_dependency_without_delivery() {
    support::run_async_test("stock-replenish", || async {
        let clock = nucleus::execution::Execution::new([92; 32], START).unwrap();
        clock.scope(async {
            let mut f=fixture(8.0,TransferReservePoint::None,Mode::PreserveOffer).await;
            let farmer_stock=support::plain(&f.engine,"farmer-stock",100.0).await;
            let mut promise=support::promise("farmer-apples",&farmer_stock,&f.other,-14.0);
            promise.item=Some(TransferItem {title:"Farmer apples".into(),exchange:Some(ExchangeRoute {uid:"farmer-route".into(),giver:f.other.uid.clone(),receiver:f.owner.uid.clone()}),..Default::default()});
            let upstream=support::create_transfer(&f.engine,&f.other,std::slice::from_ref(&f.owner),vec![promise],DraftOptions {slug:"farmer-offer",agreement:AgreementType::Full,reserve_default:TransferReservePoint::None,require_confirmation:true}).await;
            f.definition.recipe=Recipe::Replenish {below:"7".into(),target:"14".into(),upstream_transfer_uid:upstream.transfer.clone()};
            let reviewed=review(&f).await;
            let initial=enable(&f,&reviewed).await;
            assert!(initial.pending_need.is_none());
            let mut pending=None;
            for (day,expected,proposal) in [(0,"8","0"),(1,"7","0"),(2,"6","8")] {
                clock.set_time(START+day*DAY).unwrap();
                if day>0 {consume(&f,&format!("apple-day-{day}")).await;}
                let tick=Action::ApplyRecurrenceOccurrence {recurrence:initial.rule_uid.clone().unwrap(),due_at:clock.now().to_rfc3339(),amount:None,note:None};
                f.engine.act(tick.clone(),None).await.unwrap();
                drain(&f).await;
                let (preview,state)=inspect(&f).await;
                exact(&preview.actual,expected);
                exact(&preview.proposed,proposal);
                let state=state.unwrap();
                if day<2 {assert!(state.pending_need.is_none());} else {pending=state.pending_need;assert!(pending.is_some());}
                f.engine.act(tick,None).await.unwrap();
                drain(&f).await;
                assert_eq!(inspect(&f).await.1.unwrap().pending_need,pending);
                balance(&f,[expected,"0",expected]).await;
            }
            let need_transfer=pending.unwrap();
            let transfer=store::transfers::get(&f.engine.store.pool,&need_transfer).await.unwrap().unwrap();
            let need=store::records::get(&f.engine.store.pool,transfer.source_uid.as_deref().unwrap()).await.unwrap().unwrap();
            exact(&need.quantity.to_string(),"-8");
            let promises=store::transfers::promises_of(&f.engine.store.pool,&need_transfer).await.unwrap();
            assert_eq!(promises.len(),1);
            assert_eq!(promises[0].delta,-8.0);
            let dependencies=store::transfers::dependencies_of(&f.engine.store.pool,&need_transfer).await.unwrap();
            assert_eq!(dependencies.len(),1);
            assert_eq!(dependencies[0].upstream_uid,upstream.transfer);
            let farmer=protein::execute_for_with_signer(&f.engine.store,&protein::Protein {source:protein::Source::Transfer,filter:vec![protein::Predicate::UidEq(need_transfer.clone())],fields:None,include:Default::default(),aggregate:None,order:vec![],limit:None},None,Some(&f.other.uid)).await.unwrap();
            assert!(farmer.iter().any(|row|row["uid"]==need_transfer));
            let count:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE head='Stock replenishment need' AND deleted_at IS NULL").fetch_one(&f.engine.store.pool).await.unwrap();
            assert_eq!(count,1);
            assert_eq!(rule_count(&f).await,1);
            assert!(store::transfers::occurrences_of(&f.engine.store.pool,&need_transfer).await.unwrap().is_empty());
            assert!(store::transfers::party_levels(&f.engine.store.pool,&need_transfer).await.unwrap().iter().all(|party|party.2==0));
            no_delivery(&f).await;
            balance(&f,["6","0","6"]).await;
        }).await;
    });
}
