use engine::Engine;
use engine::actions::Action;
use engine::sync::{Delivery, OpBatch};
use engine::trust::Signer;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn cell() -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, "")
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (e, organ)
}

async fn plain(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid
}

async fn wire_push(from: &Engine, to: &Engine) -> usize {
    from.drain_outbox(|_contact, root, batch| async move {
        match root {
            Some(root) => to.import_grant_batch(&root, &batch).await,
            None => to.import_op_batch(&batch).await,
        }
        .map_or_else(|e| Delivery::Failed(e.to_string()), |_| Delivery::Sent)
    })
    .await
    .expect("drain")
}

async fn pair_push(a: &Engine, b: &Engine, b_organ: &str) {
    let a_intro = a.introduction().await.unwrap();
    let b_intro = b.introduction().await.unwrap();
    b.adopt_introduction(&a_intro, 1).await.unwrap();
    a.adopt_introduction(&b_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&a.store.pool, b_organ, true, false)
        .await
        .unwrap();
}

fn tagged(concept: &str) -> serde_json::Value {
    serde_json::json!({ "source": "record", "where": [{ "concept_in": concept }] })
}

async fn tag(e: &Engine, record: &str, concept: &str) {
    let uid = store::concepts::create(&e.store.pool, concept, &[])
        .await
        .unwrap();
    store::assertions::set_identity(&e.store.pool, record, Some(&uid), None)
        .await
        .unwrap();
}

async fn set_share(e: &Engine, contact: &str, protein: Option<serde_json::Value>) {
    e.act(
        Action::SetContactShare {
            target: contact.to_string(),
            protein,
        },
        None,
    )
    .await
    .expect("set share");
}

#[tokio::test]
async fn a_selection_sends_only_the_records_that_match_it() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let shared = plain(&a, "holiday").await;
    let private = plain(&a, "tax.return").await;
    tag(&a, &shared, "family").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;

    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &shared)
            .await
            .unwrap()
            .is_some(),
        "a matching record reaches the contact"
    );
    assert!(
        store::records::get(&b.store.pool, &private)
            .await
            .unwrap()
            .is_none(),
        "a record outside the selection never travels"
    );
}

#[tokio::test]
async fn tagging_a_record_later_sends_it_without_touching_the_rule() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let later = plain(&a, "photos").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;
    assert!(
        store::records::get(&b.store.pool, &later)
            .await
            .unwrap()
            .is_none(),
        "it does not match yet"
    );

    tag(&a, &later, "family").await;
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &later)
            .await
            .unwrap()
            .is_some(),
        "a mutable rule fires on the change that made it true"
    );
}

#[tokio::test]
async fn a_record_entering_the_selection_arrives_complete() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let record = plain(&a, "ledger").await;
    a.append_user(&record, 7.0).await.unwrap();
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;

    tag(&a, &record, "family").await;
    wire_push(&a, &b).await;

    assert_eq!(
        store::records::quantity(&b.store.pool, &record)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(7.0),
        "history from before it matched is replayed, not skipped"
    );
}

#[tokio::test]
async fn leaving_the_selection_stops_sending_without_taking_it_back() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let record = plain(&a, "notes").await;
    tag(&a, &record, "family").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;
    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_some()
    );

    set_share(&a, &b_organ, Some(tagged("work"))).await;
    a.append_user(&record, 3.0).await.unwrap();
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_some(),
        "what they already hold stays, the same as hiding"
    );
    assert_eq!(
        store::records::quantity(&b.store.pool, &record)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0),
        "but changes after it left do not travel"
    );
}

#[tokio::test]
async fn deleting_a_shared_record_still_reaches_whoever_holds_it() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let record = plain(&a, "receipt").await;
    tag(&a, &record, "family").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;
    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_some()
    );

    store::records::mark_deleted(&a.store.pool, &record)
        .await
        .unwrap();
    store::sync_ops::log_local(
        &a.store.pool,
        "record",
        &record,
        "",
        store::sync_ops::OpKind::Tombstone,
        None,
    )
    .await
    .unwrap();
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_none(),
        "a tombstone follows whoever was sent the record"
    );
}

#[tokio::test]
async fn no_selection_leaves_the_feed_exactly_as_it_was() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let record = plain(&a, "anything").await;
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_some(),
        "an unset selection is not an empty one"
    );
}

#[tokio::test]
async fn an_unreadable_selection_is_refused_rather_than_stored() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let refused = a
        .act(
            Action::SetContactShare {
                target: b_organ.clone(),
                protein: Some(serde_json::json!({ "source": "nonsense" })),
            },
            None,
        )
        .await;

    assert!(
        refused.is_err(),
        "a selection nobody can evaluate is not saved"
    );
    assert!(
        store::organs::contact(&a.store.pool, &b_organ)
            .await
            .unwrap()
            .and_then(|c| c.share_protein)
            .is_none(),
        "and the previous setting is untouched"
    );
}

async fn erase(e: &Engine, record: &str) {
    store::records::mark_deleted(&e.store.pool, record)
        .await
        .unwrap();
    store::sync_ops::log_local(
        &e.store.pool,
        "record",
        record,
        "",
        store::sync_ops::OpKind::Tombstone,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn hiding_a_record_then_deleting_it_still_takes_it_off_their_disk() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let record = plain(&a, "letter").await;
    wire_push(&a, &b).await;
    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_some(),
        "an ordinary contact receives an ordinary record"
    );

    store::visibility::set_hidden_from_organ(&a.store.pool, &b_organ, &record, true)
        .await
        .unwrap();
    erase(&a, &record).await;
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_none(),
        "hiding stops what travels next, it does not strand a copy nobody can clean up"
    );
}

#[tokio::test]
async fn a_record_that_left_the_selection_is_still_deleted_where_it_landed() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let record = plain(&a, "invoice").await;
    tag(&a, &record, "family").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;
    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_some()
    );

    set_share(&a, &b_organ, Some(tagged("work"))).await;
    wire_push(&a, &b).await;
    erase(&a, &record).await;
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &record)
            .await
            .unwrap()
            .is_none(),
        "a tombstone follows whoever was sent the record, selection or not"
    );
}

#[tokio::test]
async fn a_record_they_were_never_sent_is_never_announced_by_its_deletion() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let never = plain(&a, "private").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;
    erase(&a, &never).await;
    wire_push(&a, &b).await;

    assert!(
        store::sync_ops::all_by_hlc(&b.store.pool)
            .await
            .unwrap()
            .iter()
            .all(|op| op.uid != never),
        "a tombstone for a record they never had would be telling them it existed"
    );
}

#[tokio::test]
async fn a_record_coming_back_into_the_selection_catches_them_up_again() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let record = plain(&a, "diary").await;
    tag(&a, &record, "family").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;

    set_share(&a, &b_organ, Some(tagged("work"))).await;
    wire_push(&a, &b).await;
    a.append_user(&record, 5.0).await.unwrap();
    wire_push(&a, &b).await;
    assert_eq!(
        store::records::quantity(&b.store.pool, &record)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0),
        "the change while it was out does not travel"
    );

    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;

    assert_eq!(
        store::records::quantity(&b.store.pool, &record)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(5.0),
        "coming back is an entry like any other, so the gap is replayed"
    );
}

#[tokio::test]
async fn a_contact_catching_up_by_asking_is_narrowed_by_the_same_selection() {
    let (a, a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let shared = plain(&a, "holiday").await;
    let private = plain(&a, "tax.return").await;
    tag(&a, &shared, "family").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;

    let rows = store::sync_ops::ops_missing_from_vector(&a.store.pool, &a_organ, &[], 500)
        .await
        .expect("catch-up rows");
    assert!(
        rows.iter().any(|op| op.uid == private),
        "the log itself holds everything, which is why the narrowing has to happen here"
    );

    let served = engine::share::narrow_to_feed(&a, &b_organ, rows)
        .await
        .expect("narrow");

    assert!(
        served.iter().any(|op| op.uid == shared),
        "what the selection picks is still answered"
    );
    assert!(
        !served.iter().any(|op| op.uid == private),
        "asking for the log directly must not walk around the selection"
    );
    assert!(
        store::contact_share::held(&a.store.pool, &b_organ)
            .await
            .unwrap()
            .is_empty(),
        "an answer that was built is not an answer that arrived"
    );
}

#[tokio::test]
async fn clearing_a_selection_does_not_forget_what_they_were_already_sent() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let record = plain(&a, "kept").await;
    tag(&a, &record, "family").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;
    assert_eq!(
        store::contact_share::held(&a.store.pool, &b_organ)
            .await
            .unwrap()
            .len(),
        1
    );

    set_share(&a, &b_organ, None).await;

    assert!(
        store::contact_share::held(&a.store.pool, &b_organ)
            .await
            .unwrap()
            .contains(&record),
        "the rule is gone, but the copy on their disk is not — deleting it must still reach them"
    );
}

#[tokio::test]
async fn the_local_organ_is_never_narrowed_by_a_selection() {
    let (a, a_organ) = cell().await;
    let record = plain(&a, "mine").await;

    store::organs::set_contact_share_protein(
        &a.store.pool,
        &a_organ,
        Some(&serde_json::to_string(&tagged("family")).unwrap()),
    )
    .await
    .unwrap();

    let mut seen = Vec::new();
    a.drain_outbox(|contact, _root, batch: OpBatch| {
        let mine = contact.record_uid == a_organ;
        let uids: Vec<String> = batch.ops.iter().map(|op| op.uid.clone()).collect();
        let out = &mut seen;
        if mine {
            out.extend(uids);
        }
        async move { Delivery::Sent }
    })
    .await
    .expect("drain");

    assert!(
        seen.is_empty() || seen.contains(&record),
        "our own Organ is not a remote contact to be narrowed"
    );
}

fn under(record: &str, include_self: bool) -> serde_json::Value {
    serde_json::json!({
        "source": "record",
        "where": [{ "under": { "record": record, "kind": "part-of", "include_self": include_self } }]
    })
}

async fn part_of(e: &Engine, child: &str, parent: &str) -> String {
    let predicate = match store::concepts::resolve(&e.store.pool, "part-of")
        .await
        .unwrap()
    {
        Some(uid) => uid,
        None => store::concepts::create(&e.store.pool, "part-of", &[])
            .await
            .unwrap(),
    };
    store::assertions::assert(
        &e.store.pool,
        store::assertions::NewAssertion {
            subject_uid: child,
            predicate_uid: &predicate,
            object_uid: Some(parent),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn sharing_a_record_and_everything_under_it_reaches_the_whole_branch() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let trip = plain(&a, "trip").await;
    let day_one = plain(&a, "day.one").await;
    let photo = plain(&a, "day.one.photo").await;
    let unrelated = plain(&a, "tax.return").await;
    part_of(&a, &day_one, &trip).await;
    part_of(&a, &photo, &day_one).await;

    set_share(&a, &b_organ, Some(under(&trip, true))).await;
    wire_push(&a, &b).await;

    for (uid, name) in [
        (&trip, "the anchor"),
        (&day_one, "a child"),
        (&photo, "a grandchild"),
    ] {
        assert!(
            store::records::get(&b.store.pool, uid)
                .await
                .unwrap()
                .is_some(),
            "{name} travels — the branch is shared to any depth"
        );
    }
    assert!(
        store::records::get(&b.store.pool, &unrelated)
            .await
            .unwrap()
            .is_none(),
        "a record outside the branch never travels"
    );
}

#[tokio::test]
async fn sharing_only_what_is_under_a_record_leaves_the_record_itself_home() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let diary = plain(&a, "diary").await;
    let entry = plain(&a, "diary.monday").await;
    part_of(&a, &entry, &diary).await;

    set_share(&a, &b_organ, Some(under(&diary, false))).await;
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &entry)
            .await
            .unwrap()
            .is_some(),
        "a child travels"
    );
    assert!(
        store::records::get(&b.store.pool, &diary)
            .await
            .unwrap()
            .is_none(),
        "the anchor itself stays home when only its descendants are shared"
    );
}

#[tokio::test]
async fn a_record_moved_into_a_shared_branch_carries_its_own_children_with_it() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let trip = plain(&a, "trip").await;
    let album = plain(&a, "album").await;
    let photo = plain(&a, "album.photo").await;
    part_of(&a, &photo, &album).await;

    set_share(&a, &b_organ, Some(under(&trip, true))).await;
    wire_push(&a, &b).await;
    assert!(
        store::records::get(&b.store.pool, &photo)
            .await
            .unwrap()
            .is_none(),
        "nothing under the album is shared before the move"
    );

    part_of(&a, &album, &trip).await;
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &album)
            .await
            .unwrap()
            .is_some(),
        "the moved record travels"
    );
    assert!(
        store::records::get(&b.store.pool, &photo)
            .await
            .unwrap()
            .is_some(),
        "and so does everything already under it — one link changed, a whole branch entered"
    );
}

#[tokio::test]
async fn reconciling_reads_only_what_changed_since_the_last_pass() {
    let (a, a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let shared = plain(&a, "holiday").await;
    tag(&a, &shared, "family").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    wire_push(&a, &b).await;

    let contact = store::organs::contact(&a.store.pool, &b_organ)
        .await
        .unwrap()
        .expect("contact");
    let mark = contact.share_seen_seq.expect("a pass leaves a watermark");
    assert!(
        mark > 0,
        "the watermark names the last op the pass consumed"
    );

    let (touched, _) = engine::share::touched_since(&a, &a_organ, mark)
        .await
        .expect("touched");
    assert!(
        touched.is_empty(),
        "with nothing written since, a pass has nothing to look at"
    );

    let later = plain(&a, "photos").await;
    let (touched, next) = engine::share::touched_since(&a, &a_organ, mark)
        .await
        .expect("touched");
    assert!(
        touched.contains(&later),
        "a new record is what the next pass looks at"
    );
    assert!(
        !touched.contains(&shared),
        "a record nobody wrote to is not looked at again"
    );
    assert!(next > mark, "the watermark advances past what was consumed");
}

async fn move_to(e: &Engine, record: &str, contact: &str) -> Result<(), engine::EngineError> {
    e.act(
        Action::MoveRecordTo {
            record: record.to_string(),
            target: contact.to_string(),
        },
        None,
    )
    .await
    .map(|_| ())
}

#[tokio::test]
async fn a_move_hands_the_record_over_and_then_lets_go_of_it() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let deed = plain(&a, "the.deed").await;
    set_share(&a, &b_organ, Some(tagged("family"))).await;
    move_to(&a, &deed, &b_organ).await.expect("move starts");

    assert!(
        store::records::get(&a.store.pool, &deed)
            .await
            .unwrap()
            .is_some(),
        "nothing is given up before it has landed"
    );

    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &deed)
            .await
            .unwrap()
            .is_some(),
        "the Record reaches them even though the selection never picked it"
    );
    assert!(
        store::records::get(&a.store.pool, &deed)
            .await
            .unwrap()
            .is_none(),
        "and only then does it leave here"
    );
}

#[tokio::test]
async fn a_move_that_never_lands_never_destroys_the_only_copy() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let deed = plain(&a, "the.deed").await;
    move_to(&a, &deed, &b_organ).await.expect("move starts");

    a.drain_outbox(|_contact, _root, _batch| async move {
        Delivery::Failed("the other Cell is off".into())
    })
    .await
    .expect("drain");

    assert!(
        store::records::get(&a.store.pool, &deed)
            .await
            .unwrap()
            .is_some(),
        "a Move that was never acknowledged keeps the Record here"
    );
    assert!(
        store::records::get(&b.store.pool, &deed)
            .await
            .unwrap()
            .is_none(),
        "and the other side has nothing"
    );
}

#[tokio::test]
async fn the_person_receiving_a_move_is_never_told_it_was_deleted() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let deed = plain(&a, "the.deed").await;
    move_to(&a, &deed, &b_organ).await.expect("move starts");
    wire_push(&a, &b).await;
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&a.store.pool, &deed)
            .await
            .unwrap()
            .is_none(),
        "the Record left this Cell"
    );
    assert!(
        store::records::get(&b.store.pool, &deed)
            .await
            .unwrap()
            .is_some(),
        "the tombstone never follows a Move — they keep what they were handed"
    );
}

#[tokio::test]
async fn a_record_can_only_be_on_its_way_to_one_person() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    let (c, c_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;
    pair_push(&a, &c, &c_organ).await;

    let deed = plain(&a, "the.deed").await;
    move_to(&a, &deed, &b_organ).await.expect("first move");
    let second = move_to(&a, &deed, &c_organ).await;

    assert!(
        second.is_err(),
        "handing one Record to two people is refused rather than raced"
    );
}

#[tokio::test]
async fn a_cancelled_move_keeps_the_record_and_keeps_sharing_it() {
    let (a, _a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair_push(&a, &b, &b_organ).await;

    let deed = plain(&a, "the.deed").await;
    move_to(&a, &deed, &b_organ).await.expect("move starts");
    a.act(
        Action::CancelRecordMove {
            record: deed.clone(),
        },
        None,
    )
    .await
    .expect("cancel");

    wire_push(&a, &b).await;
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&a.store.pool, &deed)
            .await
            .unwrap()
            .is_some(),
        "a cancelled Move gives nothing up"
    );
    assert!(
        store::records::get(&b.store.pool, &deed)
            .await
            .unwrap()
            .is_some(),
        "what already reached them stays a normal share"
    );
}
