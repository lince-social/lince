use nucleus::vocabulary::{DEFAULT_VOCABULARY_UID, TOPOS_UID};
use store::Store;

#[tokio::test]
async fn a_dictionary_completes_a_placeholder_and_keeps_its_source_owner() {
    let store = Store::open_memory().await.unwrap();
    let concept = nucleus::new_uid("c");
    let origin = nucleus::new_uid("o");
    store::sqlx::query("INSERT INTO concept(uid,canonical_name,created_at) VALUES(?,?,?)")
        .bind(&concept)
        .bind(format!("concept:{concept}"))
        .bind("now")
        .execute(&store.pool)
        .await
        .unwrap();
    let mut dictionary = nucleus::vocabulary::Dictionary {
        source: nucleus::vocabulary::Source {
            concept_uid: concept.clone(),
            vocabulary_uid: nucleus::new_uid("g"),
            original_name: "foreign-place".into(),
            original_language: "en".into(),
            definition: "An imported place meaning.".into(),
            derived_from: None,
        },
        terms: vec![nucleus::vocabulary::Term {
            uid: nucleus::new_uid("ct"),
            concept_uid: concept.clone(),
            language: "en".into(),
            name: "foreign-place".into(),
            owner_uid: None,
            preferred: true,
            retired: false,
            revision: 1,
        }],
    };
    store::vocabulary::apply_dictionary(&store.pool, &dictionary, &origin, false)
        .await
        .unwrap();
    let established: (String, String) =
        store::sqlx::query_as("SELECT canonical_name,origin_organ FROM concept WHERE uid=?")
            .bind(&concept)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(established, ("foreign-place".into(), origin.clone()));
    dictionary.terms[0].name = "physical-place".into();
    dictionary.terms[0].revision = 2;
    store::vocabulary::apply_dictionary(&store.pool, &dictionary, &origin, false)
        .await
        .unwrap();
    assert_eq!(
        store::vocabulary::resolve(&store.pool, "physical-place", None)
            .await
            .unwrap(),
        Some(concept.clone())
    );
    dictionary.terms[0].name = "another-origin".into();
    let stranger = nucleus::new_uid("o");
    store::vocabulary::apply_dictionary(&store.pool, &dictionary, &stranger, false)
        .await
        .unwrap();
    assert_eq!(
        store::vocabulary::resolve(&store.pool, "physical-place", None)
            .await
            .unwrap(),
        Some(concept)
    );
    assert!(
        store::vocabulary::resolve(&store.pool, "another-origin", None)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn dictionary_snapshots_retire_omitted_shared_terms_and_preserve_personal_terms() {
    let store = Store::open_memory().await.unwrap();
    let other = Store::open_memory().await.unwrap();
    let mut dictionary = nucleus::vocabulary::Dictionary {
        source: store::vocabulary::sources(&store.pool).await.unwrap()[TOPOS_UID].clone(),
        terms: store::vocabulary::terms(&store.pool, None)
            .await
            .unwrap()
            .into_iter()
            .filter(|term| term.concept_uid == TOPOS_UID)
            .collect(),
    };
    let person = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Owner",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let omitted = store::vocabulary::add_term_on(
        &mut tx,
        TOPOS_UID,
        "older-local-alternative",
        "en",
        None,
        true,
    )
    .await
    .unwrap();
    store::vocabulary::add_term_on(&mut tx, TOPOS_UID, "my-place", "", Some(&person), true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let origin = store::organs::local(&store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    for pool in [&store.pool, &other.pool] {
        store::vocabulary::apply_dictionary(pool, &dictionary, &origin, true)
            .await
            .unwrap();
    }
    assert!(
        store::vocabulary::resolve(&store.pool, "older-local-alternative", None)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::vocabulary::get_term(&store.pool, &omitted)
            .await
            .unwrap()
            .unwrap()
            .retired
    );
    assert_eq!(
        store::vocabulary::display_names(&store.pool, Some(&person))
            .await
            .unwrap()[TOPOS_UID],
        "my-place"
    );
    let mut active = Vec::new();
    for pool in [&store.pool, &other.pool] {
        let mut terms: Vec<_> = store::vocabulary::terms(pool, None)
            .await
            .unwrap()
            .into_iter()
            .filter(|term| term.concept_uid == TOPOS_UID)
            .map(|term| (term.uid, term.language, term.name, term.preferred))
            .collect();
        terms.sort();
        active.push(terms);
    }
    assert_eq!(active[0], active[1]);
    dictionary.terms = (0..512)
        .map(|index| nucleus::vocabulary::Term {
            uid: nucleus::new_uid("ct"),
            concept_uid: TOPOS_UID.into(),
            language: "en".into(),
            name: format!("foreign-term-{index}"),
            owner_uid: None,
            preferred: false,
            retired: false,
            revision: 1,
        })
        .collect();
    assert!(
        store::vocabulary::apply_dictionary(&store.pool, &dictionary, &origin, true)
            .await
            .is_err()
    );
    assert_eq!(
        store::vocabulary::display_names(&store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "location"
    );
}

#[tokio::test]
async fn seeded_source_and_every_translation_share_one_identity() {
    let store = Store::open_memory().await.unwrap();
    for name in [
        "tópos",
        "to\u{301}pos",
        "location",
        "address",
        "localização",
        "endereço",
        "τοποθεσία",
    ] {
        assert_eq!(
            store::concepts::resolve(&store.pool, name)
                .await
                .unwrap()
                .as_deref(),
            Some(TOPOS_UID),
            "{name}"
        );
    }
    assert!(
        store::concepts::resolve(&store.pool, "addres")
            .await
            .unwrap()
            .is_none()
    );
    let history: Vec<(i64, i64)> = store::sqlx::query_as(
        "SELECT revision,retired FROM concept_term_history WHERE term_uid=? ORDER BY revision",
    )
    .bind("ct_01JA0000000000000000000006")
    .fetch_all(&store.pool)
    .await
    .unwrap();
    assert_eq!(history, vec![(1, 0), (2, 1)]);
    let sources = store::vocabulary::sources(&store.pool).await.unwrap();
    let source = &sources[TOPOS_UID];
    assert_eq!(source.vocabulary_uid, DEFAULT_VOCABULARY_UID);
    assert_eq!(source.original_name, "tópos");
    assert!(!source.definition.is_empty());
    let (_, _, bindings) = store::vocabulary::location_bindings(&store.pool)
        .await
        .unwrap();
    assert!(bindings.contains(TOPOS_UID));
}

#[tokio::test]
async fn translations_change_presentation_and_retirement_preserves_source_and_history() {
    let store = Store::open_memory().await.unwrap();
    store::config::set_language(&store.pool, "el-Latn")
        .await
        .unwrap();
    assert_eq!(
        store::vocabulary::display_names(&store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "tópos"
    );
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::vocabulary::add_term_on(&mut tx, TOPOS_UID, "tópos", "EL-latn", None, true)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    store::config::set_language(&store.pool, "en")
        .await
        .unwrap();
    assert_eq!(
        store::vocabulary::display_names(&store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "location"
    );
    store::config::set_language(&store.pool, "pt-BR")
        .await
        .unwrap();
    assert_eq!(
        store::vocabulary::display_names(&store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "localização"
    );
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let uid = store::vocabulary::add_term_on(&mut tx, TOPOS_UID, "morada", "pt-BR", None, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        store::vocabulary::display_names(&store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "morada"
    );
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::edit_term_on(&mut tx, &uid, 1, "onde", "pt-BR", true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        store::concepts::resolve(&store.pool, "onde")
            .await
            .unwrap()
            .as_deref(),
        Some(TOPOS_UID)
    );
    assert!(
        store::concepts::resolve(&store.pool, "morada")
            .await
            .unwrap()
            .is_none()
    );
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::vocabulary::retire_term_on(&mut tx, &uid, 1)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::retire_term_on(&mut tx, &uid, 2)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        store::vocabulary::display_names(&store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "localização"
    );
    assert_eq!(
        store::vocabulary::sources(&store.pool).await.unwrap()[TOPOS_UID].original_name,
        "tópos"
    );
    let count: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM concept_term_history WHERE term_uid=?")
            .bind(&uid)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn homonyms_require_identity_including_canonical_and_alias_collisions() {
    let store = Store::open_memory().await.unwrap();
    let email = store::concepts::create(&store.pool, "address", &[])
        .await
        .unwrap();
    assert!(
        store::concepts::resolve(&store.pool, "address")
            .await
            .is_err()
    );
    assert_eq!(
        store::concepts::resolve(&store.pool, &email).await.unwrap(),
        Some(email.clone())
    );
    let other = store::concepts::create(&store.pool, "address", &[])
        .await
        .unwrap();
    assert_ne!(email, other);
    assert!(
        store::concepts::resolve(&store.pool, "address")
            .await
            .is_err()
    );
    let (_, _, bindings) = store::vocabulary::location_bindings(&store.pool)
        .await
        .unwrap();
    assert!(!bindings.contains(&email));
}

#[tokio::test]
async fn personal_alternatives_are_readable_only_in_their_person_context() {
    let store = Store::open_memory().await.unwrap();
    let person = nucleus::new_uid("r");
    let organ = store::organs::local(&store.pool).await.unwrap().unwrap();
    store::sqlx::query("INSERT INTO record(uid,kind,head,quantity_mantissa,quantity_scale,organ_uid,created_at,updated_at) VALUES(?,'person','Owner','0',0,?,'now','now')").bind(&person).bind(organ.uid).execute(&store.pool).await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::add_term_on(&mut tx, TOPOS_UID, "donkey", "", Some(&person), true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        store::concepts::resolve(&store.pool, "donkey")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store::vocabulary::resolve(&store.pool, "donkey", Some(&person))
            .await
            .unwrap()
            .as_deref(),
        Some(TOPOS_UID)
    );
    assert_eq!(
        store::vocabulary::display_names(&store.pool, Some(&person))
            .await
            .unwrap()[TOPOS_UID],
        "donkey"
    );
    assert_eq!(
        store::vocabulary::display_names(&store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "location"
    );
    let animal = store::concepts::create(&store.pool, "donkey", &[])
        .await
        .unwrap();
    assert!(
        store::vocabulary::resolve(&store.pool, "donkey", Some(&person))
            .await
            .is_err()
    );
    assert_eq!(
        store::concepts::resolve(&store.pool, "donkey")
            .await
            .unwrap(),
        Some(animal)
    );
}

#[tokio::test]
async fn dictionaries_preserve_sources_and_never_include_personal_terms() {
    let store = Store::open_memory().await.unwrap();
    let uid = store::concepts::create(&store.pool, "geographic-place", &[])
        .await
        .unwrap();
    let person = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Owner",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::add_term_on(&mut tx, &uid, "place", "en", None, true)
        .await
        .unwrap();
    store::vocabulary::add_term_on(&mut tx, &uid, "private-nickname", "", Some(&person), true)
        .await
        .unwrap();
    store::vocabulary::log_shared_on(&mut tx, &uid)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let raw: String = store::sqlx::query_scalar("SELECT value FROM sync_op WHERE tbl='concept' AND uid=? AND field='vocabulary' ORDER BY seq DESC LIMIT 1")
        .bind(&uid).fetch_one(&store.pool).await.unwrap();
    assert!(!raw.contains("private-nickname"));
    let dictionary: nucleus::vocabulary::Dictionary = serde_json::from_str(&raw).unwrap();
    let other = Store::open_memory().await.unwrap();
    let origin = store::organs::local(&store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::vocabulary::apply_dictionary(&other.pool, &dictionary, &origin, false)
        .await
        .unwrap();
    assert_eq!(
        store::vocabulary::sources(&other.pool).await.unwrap()[&uid],
        dictionary.source
    );
    assert_eq!(
        store::vocabulary::display_names(&other.pool, None)
            .await
            .unwrap()[&uid],
        "place"
    );
    assert!(
        store::vocabulary::terms(&other.pool, None)
            .await
            .unwrap()
            .iter()
            .any(|term| term.uid == dictionary.terms[0].uid)
    );
    let mut conflicting = dictionary.clone();
    conflicting.source.original_language = "pt".into();
    assert!(
        store::vocabulary::apply_dictionary(&other.pool, &conflicting, &origin, false)
            .await
            .is_err()
    );
    let mut seeded = nucleus::vocabulary::Dictionary {
        source: store::vocabulary::sources(&store.pool).await.unwrap()[TOPOS_UID].clone(),
        terms: store::vocabulary::terms(&store.pool, None)
            .await
            .unwrap()
            .into_iter()
            .filter(|term| term.concept_uid == TOPOS_UID)
            .collect(),
    };
    seeded.terms.iter_mut().for_each(|term| term.retired = true);
    store::vocabulary::apply_dictionary(&other.pool, &seeded, &origin, false)
        .await
        .unwrap();
    assert_eq!(
        store::concepts::resolve(&other.pool, "address")
            .await
            .unwrap()
            .as_deref(),
        Some(TOPOS_UID)
    );
}

#[tokio::test]
async fn source_and_term_identities_are_protected_by_database_invariants() {
    let store = Store::open_memory().await.unwrap();
    let term = store::vocabulary::terms(&store.pool, None)
        .await
        .unwrap()
        .into_iter()
        .find(|term| term.name == "location")
        .unwrap();
    let other = store::concepts::create(&store.pool, "Another meaning", &[])
        .await
        .unwrap();
    assert!(
        store::sqlx::query("UPDATE concept_term SET concept_uid=? WHERE uid=?")
            .bind(other)
            .bind(term.uid)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE concept_source SET original_name='changed' WHERE concept_uid=?")
            .bind(TOPOS_UID)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE concept SET canonical_name='changed' WHERE uid=?")
            .bind(TOPOS_UID)
            .execute(&store.pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn dictionary_ancestry_cannot_create_a_cycle() {
    let store = Store::open_memory().await.unwrap();
    let first = nucleus::new_uid("c");
    let second = nucleus::new_uid("c");
    let origin = nucleus::new_uid("r");
    let mut dictionary = nucleus::vocabulary::Dictionary {
        source: nucleus::vocabulary::Source {
            concept_uid: first.clone(),
            vocabulary_uid: store::linguas::LOCAL_UID.into(),
            original_name: "First".into(),
            original_language: "en".into(),
            definition: "A source".into(),
            derived_from: Some(second.clone()),
        },
        terms: Vec::new(),
    };
    store::vocabulary::apply_dictionary(&store.pool, &dictionary, &origin, false)
        .await
        .unwrap();
    dictionary.source.concept_uid = second.clone();
    dictionary.source.original_name = "Second".into();
    dictionary.source.derived_from = Some(first);
    assert!(
        store::vocabulary::apply_dictionary(&store.pool, &dictionary, &origin, false)
            .await
            .is_err()
    );
    assert!(
        !store::vocabulary::sources(&store.pool)
            .await
            .unwrap()
            .contains_key(&second)
    );
}
