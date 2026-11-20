use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use bevy::{
    prelude::*,
    text::{EditableText, FontCx, LayoutCx},
};
use cell::{ClientMessage, ServerMessage};
#[cfg(test)]
use loro::ExportMode;
use loro::{Frontiers, LoroDoc, UndoManager, VersionVector, cursor::Cursor};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::host::performance::{self, Count, Stage};
use crate::native::protein_area::{RecordBinding, Source};

mod journal;
mod splice_checks;
mod cursors;
mod identity;
mod incoming;
mod dispatch;
mod preparation;
use identity::{Documents, key_parts, key_ref};

pub(crate) fn laboratory_cases(cases: &mut Vec<crate::native::laboratory::Case>) {
    splice_checks::laboratory_cases(cases);
    journal::laboratory_cases(cases);
}
mod recovery;
pub(crate) use recovery::retry;

#[cfg(test)]
mod performance_tests;

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (
        World,
        std::sync::Arc<engine::Engine>,
        String,
        Entity,
        Entity,
    ) {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Title".into(),
                    body: "Olá 👩‍💻 עולם".into(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let runtime = cell::CellRuntime {
            commands: Default::default(),
            store: engine.store.clone(),
            engine: engine.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            speech: None,
            information: None,
        };
        let mut world = World::new();
        world.insert_resource(crate::native::app::CellHandle(runtime.clone()));
        world.init_resource::<Bindings>();
        world.init_resource::<bevy::input_focus::InputFocus>();
        world.insert_non_send(crate::native::cell_bridge::connect(
            runtime,
            crate::native::wake::WakeSignal::new(|| {}),
        ));
        let first = world.spawn(EditableText::new("Olá 👩‍💻 עולם")).id();
        let second = world.spawn(EditableText::new("Olá 👩‍💻 עולם")).id();
        let binding = RecordBinding {
            area: first,
            uid: uid.clone(),
            source: Source::Local,
        };
        attach(&mut world, first, binding.clone(), "body", None);
        attach(&mut world, second, binding, "body", None);
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| document.joined)
                && !world.resource::<Bindings>().documents.is_empty()
        })
        .await;
        (world, engine, uid, first, second)
    }

    #[tokio::test]
    async fn scoped_laboratory_denies_real_bindings_and_keeps_ordinary_edits_live() {
        let (mut world, engine, uid, first, second) = fixture().await;
        let root = world.spawn_empty().id();
        world
            .entity_mut(first)
            .insert((ChildOf(root), crate::native::workspace::WorkspaceMember(1)));
        world
            .entity_mut(second)
            .insert((ChildOf(root), crate::native::workspace::WorkspaceMember(1)));
        let temporary = world
            .spawn((ChildOf(root), crate::native::workspace::WorkspaceMember(2)))
            .id();
        let denied = world
            .spawn((ChildOf(temporary), EditableText::new("Private benchmark")))
            .id();
        world.insert_resource(crate::native::laboratory::Laboratory::scoped_for_test(
            root, 2, 1,
        ));
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        attach(&mut world, denied, record, "body", None);
        assert!(world.get::<TextBinding>(denied).is_none());
        assert!(crate::native::practice_cells::sender(&world, denied).is_none());
        assert!(crate::native::practice_cells::runtime(&world, denied).is_none());
        let source = crate::native::practice_cells::source(&world, denied).unwrap();
        assert!(crate::native::practice_cells::engine(&world, &Source::Organ(source)).is_none());
        let injected = world.get::<TextBinding>(first).unwrap().clone();
        world.entity_mut(denied).insert(injected);
        assert!(!can_edit(&world, denied));
        assert!(can_edit(&world, first));
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Ordinary live change");
        update(&mut world);
        assert_eq!(
            world
                .get::<EditableText>(second)
                .unwrap()
                .value()
                .to_string(),
            "Ordinary live change"
        );
        assert_eq!(
            world
                .get::<EditableText>(denied)
                .unwrap()
                .value()
                .to_string(),
            "Private benchmark"
        );
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| document.pending.is_empty() && !document.saving)
        })
        .await;
        assert_eq!(
            engine.doc_text(&uid).await.unwrap().1,
            "Ordinary live change"
        );
        assert_eq!(
            world
                .get::<EditableText>(denied)
                .unwrap()
                .value()
                .to_string(),
            "Private benchmark"
        );
    }

    #[tokio::test]
    async fn fiote_native_edits_merge_with_typing_and_appear_in_both_views() {
        let (mut world, engine, uid, first, second) = fixture().await;
        let agent = engine
            .act(
                engine::actions::Action::CreateAgent {
                    head: "Fiote".into(),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut tools = cell::FioteTools::default();
        cell::Session::local(
            engine.clone(),
            std::sync::Arc::new(cell::LaneHub::new()),
            "fiote-view-test",
        )
        .into_native_tools(cell::FioteContext {
            agent,
            record: uid.clone(),
            thread: uid.clone(),
        })
        .register(&mut tools);
        let read = tools
            .run("lince_read_record", serde_json::json!({"record_uid":uid}))
            .await;
        assert_eq!(read["ok"], true, "{read}");
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Olá 👩‍💻 עולם! My edit.");
        update(&mut world);
        let result = tools
            .run(
                "lince_edit_text",
                serde_json::json!({
                    "read_id":read["result"]["read_id"], "request_id":"fiote-edit",
                    "edits":[{"field":"body","before":"Olá","after":"Fiote says olá"}]
                }),
            )
            .await;
        assert_eq!(result["ok"], true, "{result}");
        let expected = "Fiote says olá 👩‍💻 עולם! My edit.";
        pump(&mut world, |world| {
            [first, second].iter().all(|entity| {
                world
                    .get::<EditableText>(*entity)
                    .unwrap()
                    .value()
                    .to_string()
                    == expected
            }) && world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|doc| doc.pending.is_empty() && !doc.saving)
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, expected);
    }

    async fn pump(world: &mut World, done: impl Fn(&World) -> bool) {
        for _ in 0..500 {
            let mut messages = Vec::new();
            {
                let mut bridge = world.non_send_mut::<crate::native::cell_bridge::CellBridge>();
                while let Ok(message) = bridge.incoming.try_recv() {
                    messages.push(message);
                }
            }
            for message in messages {
                crate::native::description::receive_live(world, &Source::Local, &message);
                receive(world, Source::Local, message);
            }
            update(world);
            if done(world) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let states = world
            .resource::<Bindings>()
            .documents
            .values()
            .map(|document| {
                (
                    &document.status,
                    document.failed,
                    document.inflight,
                    document.pending.len(),
                    document.saved,
                    document.revision,
                    document.saving,
                    document.joined,
                    document.sequence.cancelled(),
                )
            })
            .collect::<Vec<_>>();
        panic!("Record bindings did not settle: {states:?}");
    }

    #[tokio::test]
    async fn durable_local_edit_reaches_canonical_storage_without_another_interface_update() {
        let (mut world, engine, uid, first, second) = fixture().await;
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| {
                    document.pending.is_empty()
                        && !document.saving
                        && document.saved == document.revision
                })
        })
        .await;
        let expected = "Immediate local edit 👩‍💻";
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text(expected);
        update(&mut world);
        assert_eq!(world.get::<EditableText>(second).unwrap().value(), expected);
        tokio::time::timeout(Duration::from_secs(2), async {
            while engine.doc_text(&uid).await.unwrap().1 != expected {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect(
            "A durable local edit waited for another interface update before canonical storage",
        );
        assert_eq!(
            store::records::get(&engine.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .body,
            expected,
        );
        assert!(
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| { !document.pending.is_empty() })
        );
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| {
                    document.pending.is_empty()
                        && !document.saving
                        && document.saved == document.revision
                })
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, expected);
    }

    async fn hold_replies(world: &mut World, uid: &str) {
        let outgoing = world
            .non_send::<crate::native::cell_bridge::CellBridge>()
            .outgoing
            .clone();
        for index in 0..65 {
            let ClientMessage::Subscribe { mut protein, .. } =
                crate::native::cell_bridge::records_subscription()
            else {
                unreachable!()
            };
            protein.filter = vec![protein::Predicate::UidEq(uid.into())];
            outgoing
                .send(ClientMessage::Subscribe {
                    id: format!("held-edit-reply-{index}"),
                    protein,
                })
                .await
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while world
                .non_send::<crate::native::cell_bridge::CellBridge>()
                .incoming
                .len()
                < 64
                || outgoing.capacity() < 64
            {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("The reply-pressure precondition was not established");
    }

    #[tokio::test]
    async fn durable_local_backlog_drains_after_reply_pressure_without_more_interface_updates() {
        local_backlog_after_reply_pressure(false).await;
    }

    #[tokio::test]
    async fn full_local_sender_keeps_the_unsent_suffix_durable_and_ordered() {
        local_backlog_after_reply_pressure(true).await;
    }

    async fn local_backlog_after_reply_pressure(fill_sender: bool) {
        let (mut world, engine, uid, first, second) = fixture().await;
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| {
                    document.pending.is_empty()
                        && !document.saving
                        && document.saved == document.revision
                })
        })
        .await;
        hold_replies(&mut world, &uid).await;
        if fill_sender {
            let outgoing = &world
                .non_send::<crate::native::cell_bridge::CellBridge>()
                .outgoing;
            assert_eq!(outgoing.capacity(), 64);
            for _ in 0..63 {
                outgoing
                    .try_send(ClientMessage::Unsubscribe {
                        id: "unused-subscription".into(),
                    })
                    .unwrap();
            }
        }
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&record);
        let values = ["Queued é", "Queued é新", "Queued é新 👩‍💻"];
        for (index, value) in values.iter().enumerate() {
            world
                .get_mut::<EditableText>(first)
                .unwrap()
                .editor
                .set_text(value);
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    update(&mut world);
                    let document = &world.resource::<Bindings>().documents[&key];
                    assert!(!document.failed, "{}", document.status);
                    if !document.saving && document.saved == document.revision {
                        assert_eq!(document.pending.len(), index + 1);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .expect("The queued edit did not become independently durable");
        }
        let recovered = engine
            .load_record_edit_draft(&storage_source(&key, None), &uid)
            .await
            .unwrap()
            .recovered
            .unwrap();
        assert_eq!(recovered.pending.len(), values.len());
        let durable = LoroDoc::new();
        durable.import(&recovered.snapshot).unwrap();
        assert_eq!(durable.get_text("body").to_string(), values[2]);
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Olá 👩‍💻 עולם");

        let mut replies = Vec::new();
        let drained = if fill_sender {
            assert_eq!(world.resource::<Bindings>().documents[&key].inflight, 1);
            pump(&mut world, |world| {
                world.resource::<Bindings>().documents[&key]
                    .pending
                    .is_empty()
            })
            .await;
            Ok(())
        } else {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let mut bridge = world.non_send_mut::<crate::native::cell_bridge::CellBridge>();
                    while let Ok(message) = bridge.incoming.try_recv() {
                        replies.push(message);
                    }
                    drop(bridge);
                    if engine.doc_text(&uid).await.unwrap().1 == values[2] {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
        };
        let canonical = engine.doc_text(&uid).await.unwrap().1;
        let committed = store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM record_change_receipt WHERE record_uid=?",
        )
        .bind(&uid)
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
        assert!(
            drained.is_ok(),
            "Durable queued edits still require a UI acknowledgement between saves: canonical={canonical:?}, committed={committed}",
        );
        for request in &recovered.pending {
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM record_change_receipt WHERE change_uid=?",
                )
                .bind(&request.id)
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
                1,
            );
        }
        for message in replies {
            receive(&mut world, Source::Local, message);
        }
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key];
            document.pending.is_empty() && !document.saving && document.saved == document.revision
        })
        .await;
        assert_eq!(
            world.get::<EditableText>(second).unwrap().value(),
            values[2]
        );
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, values[2]);
    }

    #[tokio::test]
    async fn failed_local_edit_stops_admitted_dependents_until_the_field_is_corrected() {
        let (mut world, engine, uid, first, _) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&record);
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key];
            document.pending.is_empty() && !document.saving && document.saved == document.revision
        })
        .await;
        hold_replies(&mut world, &uid).await;
        let [failed, dependent, corrected] = std::array::from_fn(|_| nucleus::new_uid("op"));
        let request = |id: &str, value: &str| engine::record_change::Request {
            id: id.into(),
            record_uid: uid.clone(),
            mutation: engine::record_change::Mutation::Quantity {
                value: value.into(),
            },
        };
        submit(&mut world, &record, request(&failed, "incomplete")).unwrap();
        submit(&mut world, &record, request(&dependent, "7")).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                update(&mut world);
                let document = &world.resource::<Bindings>().documents[&key];
                assert!(!document.failed, "{}", document.status);
                if !document.saving && document.saved == document.revision {
                    assert_eq!(document.inflight, 2);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        let ClientMessage::Subscribe { protein, .. } =
            crate::native::cell_bridge::records_subscription()
        else {
            unreachable!()
        };
        world
            .non_send::<crate::native::cell_bridge::CellBridge>()
            .outgoing
            .send(ClientMessage::Unsubscribe {
                id: "held-edit-reply-0".into(),
            })
            .await
            .unwrap();
        world
            .non_send::<crate::native::cell_bridge::CellBridge>()
            .outgoing
            .send(ClientMessage::Subscribe {
                id: "backlog-barrier".into(),
                protein,
            })
            .await
            .unwrap();
        let mut replies = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                while let Ok(message) = world.non_send_mut::<crate::native::cell_bridge::CellBridge>().incoming.try_recv() {
                    let barrier = matches!(&message, ServerMessage::Snapshot { id, .. } if id == "backlog-barrier");
                    replies.push(message);
                    if barrier { return; }
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }).await.unwrap();
        assert!(
            replies
                .iter()
                .any(|reply| matches!(reply, ServerMessage::Error { id, .. } if id == &failed))
        );
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM record_change_receipt WHERE change_uid IN (?,?)",
            )
            .bind(&failed)
            .bind(&dependent)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            0
        );
        for reply in replies {
            receive(&mut world, Source::Local, reply);
        }
        update(&mut world);
        assert!(world.resource::<Bindings>().documents[&key].failed);
        assert_eq!(
            world.resource::<Bindings>().documents[&key].pending.len(),
            2
        );
        submit(&mut world, &record, request(&corrected, "9")).unwrap();
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key];
            !document.failed
                && document.pending.is_empty()
                && !document.saving
                && document.saved == document.revision
        })
        .await;
        for id in [&dependent, &corrected] {
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM record_change_receipt WHERE change_uid=?",
                )
                .bind(id)
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
                1
            );
        }
        assert_eq!(
            store::sqlx::query_scalar::<_, String>(
                "SELECT quantity_mantissa FROM record WHERE uid=?",
            )
            .bind(&uid)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            "9"
        );
    }

    #[tokio::test]
    async fn disconnect_during_recovery_keeps_the_draft_for_the_next_connection() {
        recovery_after_disconnect(false, false).await;
    }

    #[tokio::test]
    async fn closing_all_views_during_disconnect_keeps_the_draft_for_the_next_connection() {
        recovery_after_disconnect(true, false).await;
    }

    #[tokio::test]
    async fn submitting_after_connection_replacement_preserves_the_old_pending_prefix() {
        recovery_after_disconnect(false, true).await;
    }

    async fn recovery_after_disconnect(close_views: bool, submit_on_replacement: bool) {
        let (mut world, engine, uid, first, second) = fixture().await;
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| {
                    document.pending.is_empty()
                        && !document.saving
                        && document.saved == document.revision
                })
        })
        .await;
        let expected = "Kept across disconnection é新";
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text(expected);
        update(&mut world);
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&record);
        let request = world.resource::<Bindings>().documents[&key].pending[0]
            .id
            .clone();
        drop(
            world
                .remove_non_send::<crate::native::cell_bridge::CellBridge>()
                .unwrap(),
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let loaded = engine
                    .load_record_edit_draft(&storage_source(&key, None), &uid)
                    .await
                    .unwrap();
                if loaded
                    .recovered
                    .is_some_and(|draft| draft.pending.iter().any(|edit| edit.id == request))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("Disconnect cancelled the independent recovery write");
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Olá 👩‍💻 עולם");
        if close_views {
            assert!(world.despawn(first));
            assert!(world.despawn(second));
        }
        let runtime = world.resource::<crate::native::app::CellHandle>().0.clone();
        world.insert_non_send(crate::native::cell_bridge::connect(
            runtime,
            crate::native::wake::WakeSignal::new(|| {}),
        ));
        let replacement_request = nucleus::new_uid("op");
        if submit_on_replacement {
            submit(
                &mut world,
                &record,
                engine::record_change::Request {
                    id: replacement_request.clone(),
                    record_uid: uid.clone(),
                    mutation: engine::record_change::Mutation::Quantity { value: "9".into() },
                },
            )
            .unwrap();
        }
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key];
            document.pending.is_empty() && !document.saving && document.saved == document.revision
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, expected);
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM record_change_receipt WHERE change_uid=?",
            )
            .bind(request)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            1,
        );
        if submit_on_replacement {
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM record_change_receipt WHERE change_uid=?",
                )
                .bind(&replacement_request)
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
                1
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, String>(
                    "SELECT quantity_mantissa FROM record WHERE uid=?",
                )
                .bind(&uid)
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
                "9"
            );
        }
    }

    #[tokio::test]
    async fn drawing_insertion_keeps_local_drafts_and_stores_only_a_reference_in_the_description() {
        let (mut world, engine, uid, first, second) = fixture().await;
        assert!(can_edit(&world, first));
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Local draft 👩‍💻");
        let panel = world.spawn_empty().id();
        let drawing = nucleus::drawing::Drawing {
            strokes: vec![nucleus::drawing::Stroke {
                points: vec![[10.0, 20.0], [30.0, 50.0]],
                color: [40, 100, 180, 255],
                width: 3.0,
            }],
            ..Default::default()
        };
        let bytes = serde_json::to_vec(&drawing).unwrap();
        crate::native::description::insert_asset(
            &mut world,
            panel,
            first,
            nucleus::description_asset::Kind::Drawing,
            bytes,
            None,
        )
        .unwrap();
        pump(&mut world, |world| {
            world
                .get::<EditableText>(second)
                .unwrap()
                .value()
                .to_string()
                .contains("asset:")
                && world
                    .resource::<Bindings>()
                    .documents
                    .values()
                    .all(|document| document.pending.is_empty() && !document.saving)
        })
        .await;
        let body = engine.doc_text(&uid).await.unwrap().1;
        assert!(body.starts_with("Local draft 👩‍💻"));
        assert!(body.contains("![Native drawing](asset:"));
        assert!(!body.contains("strokes"));
        let row: (String, Vec<u8>) = store::sqlx::query_as(
            "SELECT kind, bytes FROM description_assets WHERE record_uid = ?",
        )
        .bind(uid)
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
        assert_eq!(row.0, "drawing");
        assert_eq!(
            serde_json::from_slice::<nucleus::drawing::Drawing>(&row.1).unwrap(),
            drawing
        );
    }

    #[tokio::test]
    async fn two_views_share_unicode_edits_and_persist_before_transmission() {
        let (mut world, engine, uid, first, second) = fixture().await;
        assert!(
            !world
                .get::<TextBinding>(first)
                .unwrap()
                .unsaved("Olá 👩‍💻 עולם")
        );
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Olá 👩‍💻 שלום עולם!");
        update(&mut world);
        assert_eq!(
            world
                .get::<EditableText>(second)
                .unwrap()
                .value()
                .to_string(),
            "Olá 👩‍💻 שלום עולם!"
        );
        let document = world
            .resource::<Bindings>()
            .documents
            .values()
            .next()
            .unwrap();
        assert_eq!(document.pending.len(), 1);
        assert!(document.journal.metadata.as_ref().is_none_or(|metadata| {
            !metadata.pending.contains(&document.pending[0].id)
        }));
        for entity in [first, second] {
            assert!(
                world
                    .get::<TextBinding>(entity)
                    .unwrap()
                    .unsaved("Olá 👩‍💻 שלום עולם!")
            );
        }
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| document.pending.is_empty() && !document.saving)
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Olá 👩‍💻 שלום עולם!");
        for entity in [first, second] {
            assert!(
                !world
                    .get::<TextBinding>(entity)
                    .unwrap()
                    .unsaved("Olá 👩‍💻 שלום עולם!")
            );
        }
        assert!(
            engine
                .load_record_edit_draft(&serde_json::to_string(&Source::Local).unwrap(), &uid)
                .await
                .unwrap()
                .recovered
                .is_none()
        );
    }

    #[tokio::test]
    async fn unsaved_indicator_survives_failure_without_marking_unchanged_fields() {
        let (mut world, _, _, first, _) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let title = world.spawn(EditableText::new("Title")).id();
        attach(&mut world, title, record.clone(), "head", None);
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Draft");
        update(&mut world);
        world.init_resource::<crate::native::tokens::ThemeSettings>();
        crate::native::save_feedback::borders(&mut world);
        assert_eq!(
            world.get::<BorderColor>(first).unwrap().top,
            crate::native::tokens::Token::Accent
                .default_value(Default::default())
                .color()
        );
        let mut bindings = world.resource_mut::<Bindings>();
        fail(
            bindings.documents.get_mut(&key(&record)).unwrap(),
            "Storage unavailable",
        );
        update(&mut world);
        assert!(world.get::<TextBinding>(first).unwrap().unsaved("Draft"));
        assert!(!world.get::<TextBinding>(title).unwrap().unsaved("Title"));
        crate::native::save_feedback::borders(&mut world);
        assert_eq!(
            world.get::<BorderColor>(first).unwrap().top,
            crate::native::tokens::Token::Error
                .default_value(Default::default())
                .color()
        );
        assert_eq!(
            world.get::<BorderColor>(title).unwrap().top,
            crate::native::tokens::Token::Accent
                .default_value(Default::default())
                .color()
        );
    }

    #[tokio::test]
    async fn composition_defers_remote_projection_and_merges_after_commit() {
        let (mut world, engine, uid, first, second) = fixture().await;
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .pending_edits
            .push(bevy::text::TextEdit::Insert("!".into()));
        engine
            .act(
                engine::actions::Action::EditRecordText {
                    target: uid.clone(),
                    head: None,
                    body: Some("REMOTE Olá 👩‍💻 עולם".into()),
                },
                None,
            )
            .await
            .unwrap();
        for _ in 0..10 {
            let message = {
                world
                    .non_send_mut::<crate::native::cell_bridge::CellBridge>()
                    .incoming
                    .try_recv()
                    .ok()
            };
            if let Some(message) = message {
                receive(&mut world, Source::Local, message);
            }
            update(&mut world);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert_eq!(
            world
                .get::<EditableText>(first)
                .unwrap()
                .value()
                .to_string(),
            "Olá 👩‍💻 עולם"
        );
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .pending_edits
            .clear();
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Olá 👩‍💻 עולם!");
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| document.pending.is_empty() && !document.saving)
        })
        .await;
        assert_eq!(
            world
                .get::<EditableText>(second)
                .unwrap()
                .value()
                .to_string(),
            "REMOTE Olá 👩‍💻 עולם!"
        );
    }

    #[test]
    fn undo_keeps_another_peers_text_and_cursors_follow_unicode() {
        let mut document = Document::new(Source::Local, None);
        let remote = LoroDoc::new();
        remote.get_text("body").insert(0, "Olá 👩‍💻").unwrap();
        remote.commit();
        document
            .doc
            .import(&remote.export(ExportMode::Snapshot).unwrap())
            .unwrap();
        let anchor = document
            .doc
            .get_text("body")
            .get_cursor(4, Default::default())
            .unwrap();
        splice(&document.doc, "body", "Olá 👩‍💻", "Olá 👩‍💻!").unwrap();
        remote.get_text("body").insert(0, "X ").unwrap();
        remote.commit();
        document
            .doc
            .import(&remote.export(ExportMode::Snapshot).unwrap())
            .unwrap();
        assert_eq!(document.doc.get_cursor_pos(&anchor).unwrap().current.pos, 6);
        document.undo.undo().unwrap();
        assert_eq!(document.doc.get_text("body").to_string(), "X Olá 👩‍💻");
    }

    #[tokio::test]
    async fn pending_paste_and_active_ime_preserve_remote_changes_and_commit_unicode() {
        let mut fonts = FontCx::default();
        let font = Font::from_bytes(
            include_bytes!("../../../../institute/assets/fonts/DejaVuSans/DejaVuSansMono.ttf")
                .to_vec(),
        );
        fonts.collection.register_fonts(font.data, None);
        fonts.set_sans_serif_family("DejaVu Sans Mono").unwrap();
        let mut layout = LayoutCx::default();
        for composing in [false, true] {
            let (mut world, engine, uid, first, second) = fixture().await;
            let before = "Olá 👩‍💻 עולם";
            let inserted = "é新👩‍💻";
            {
                let mut text = world.get_mut::<EditableText>(first).unwrap();
                text.pending_edits.clear();
                text.editor
                    .driver(&mut fonts.context, &mut layout.0)
                    .select_byte_range(before.len(), before.len());
                if composing {
                    text.editor
                        .driver(&mut fonts.context, &mut layout.0)
                        .set_compose(inserted, Some((inserted.len(), inserted.len())));
                    assert!(text.is_composing());
                } else {
                    text.pending_paste = Some(bevy::clipboard::ClipboardRead::Ready(Ok(
                        inserted.to_string()
                    )));
                    assert!(crate::native::record_view::pending_text(&text));
                }
            }
            engine
                .write_record_text(&uid, None, Some(&format!("REMOTE {before}")))
                .await
                .unwrap();
            pump(&mut world, |world| {
                world
                    .resource::<Bindings>()
                    .documents
                    .values()
                    .any(|document| {
                        document
                            .incoming
                            .iter()
                            .any(|message| matches!(message, incoming::Message::Change(_)))
                    })
            })
            .await;
            for entity in [first, second] {
                assert_eq!(
                    world
                        .get::<EditableText>(entity)
                        .unwrap()
                        .value()
                        .to_string(),
                    before
                );
            }
            assert!(
                world
                    .resource::<Bindings>()
                    .documents
                    .values()
                    .all(|document| {
                        document.pending.is_empty() && document.revision == document.saved
                    })
            );
            {
                let mut text = world.get_mut::<EditableText>(first).unwrap();
                if composing {
                    text.editor
                        .driver(&mut fonts.context, &mut layout.0)
                        .finish_compose();
                } else {
                    let pasted = text
                        .pending_paste
                        .take()
                        .unwrap()
                        .poll_result()
                        .unwrap()
                        .unwrap();
                    text.editor
                        .driver(&mut fonts.context, &mut layout.0)
                        .insert_or_replace_selection(&pasted);
                }
            }
            let expected = format!("REMOTE {before}{inserted}");
            pump(&mut world, |world| {
                world
                    .resource::<Bindings>()
                    .documents
                    .values()
                    .all(|document| {
                        document.pending.is_empty()
                            && !document.saving
                            && document.doc.get_text("body").to_string() == expected
                    })
            })
            .await;
            assert_eq!(engine.doc_text(&uid).await.unwrap().1, expected);
            for entity in [first, second] {
                assert_eq!(
                    world
                        .get::<EditableText>(entity)
                        .unwrap()
                        .value()
                        .to_string(),
                    expected
                );
            }
        }
    }

    #[tokio::test]
    async fn reopening_restores_and_delivers_a_saved_unsent_edit() {
        let (mut world, engine, uid, first, _) = fixture().await;
        let binding = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&binding);
        let draft = {
            let mut bindings = world.resource_mut::<Bindings>();
            let document = bindings.documents.get_mut(&key).unwrap();
            let before = document.doc.oplog_vv();
            splice(&document.doc, "body", "Olá 👩‍💻 עולם", "Recovered 👩‍💻").unwrap();
            document.queue(&uid, &before).unwrap();
            document
                .journal
                .prepare(&document.doc, &document.pending)
                .unwrap()
                .0
        };
        engine
            .save_record_edit_draft(&storage_source(&key, None), &uid, draft)
            .await
            .unwrap();
        world.insert_resource(Bindings::default());
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .get(&key)
                .is_some_and(|document| {
                    document.joined && document.pending.is_empty() && !document.saving
                })
                && world
                    .get::<EditableText>(first)
                    .unwrap()
                    .value()
                    .to_string()
                    == "Recovered 👩‍💻"
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Recovered 👩‍💻");
        assert!(
            engine
                .load_record_edit_draft(&storage_source(&key, None), &uid)
                .await
                .unwrap()
                .recovered
                .is_none()
        );
    }

    #[tokio::test]
    async fn scalar_only_bindings_save_without_joining_the_text_document() {
        let (mut world, _, uid, first, second) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        world.entity_mut(first).remove::<TextBinding>();
        world.entity_mut(second).remove::<TextBinding>();
        world.insert_resource(Bindings::default());
        submit(
            &mut world,
            &record,
            engine::record_change::Request {
                id: nucleus::new_uid("op"),
                record_uid: uid,
                mutation: engine::record_change::Mutation::Quantity {
                    value: "3.75".into(),
                },
            },
        )
        .unwrap();
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .get(&key(&record))
                .is_some_and(|document| {
                    document.loaded && document.pending.is_empty() && !document.saving
                })
        })
        .await;
        let document = &world.resource::<Bindings>().documents[&key(&record)];
        assert!(!document.joined);
        assert!(document.joining.is_none());
        assert_eq!(document.status, "Saved");
    }

    #[tokio::test]
    async fn a_durable_prefix_survives_a_full_sender_while_the_next_edit_is_still_writing() {
        let (mut world, engine, uid, first, _) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&record);
        let mut commands = hold_canonical_requests(&mut world, first);
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Alpha");
        update(&mut world);
        let first_id = world.resource::<Bindings>().documents[&key].pending[0]
            .id
            .clone();
        assert_eq!(world.resource::<Bindings>().documents[&key].inflight, 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while world.resource::<Bindings>().incoming.is_empty() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let durable = engine
            .load_record_edit_draft(&storage_source(&key, None), &uid)
            .await
            .unwrap()
            .recovered
            .unwrap();
        assert_eq!(durable.pending[0].id, first_id);
        let held = engine
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .unwrap();
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("AlphaBeta");
        update(&mut world);
        assert_eq!(world.resource::<Bindings>().documents[&key].inflight, 0);
        assert_eq!(
            world.resource::<Bindings>().documents[&key].pending[0].id,
            first_id
        );
        assert!(matches!(
            commands.try_recv().unwrap(),
            ClientMessage::CollabLeave { .. }
        ));
        update(&mut world);
        {
            let bindings = world.resource::<Bindings>();
            let document = &bindings.documents[&key];
            assert!(document.saved < document.revision && document.saving);
            assert_eq!(document.inflight, 1);
            assert_eq!(document.pending.front().unwrap().id, first_id);
            assert_eq!(document.pending.len(), 2);
        }
        match commands.try_recv().unwrap() {
            ClientMessage::Act {
                action: engine::actions::Action::ChangeRecord { request },
                ..
            } => assert_eq!(request.id, first_id),
            _ => panic!("Expected the durable canonical request"),
        }
        held.rollback().await.unwrap();
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key];
            document.saved == document.revision && !document.saving
        })
        .await;
        let recovered = engine
            .load_record_edit_draft(&storage_source(&key, None), &uid)
            .await
            .unwrap()
            .recovered
            .unwrap();
        assert_eq!(recovered.pending.len(), 2);
        let document = LoroDoc::new();
        document.import(&recovered.snapshot).unwrap();
        assert_eq!(document.get_text("body").to_string(), "AlphaBeta");
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Olá 👩‍💻 עולם");
    }

    #[tokio::test]
    async fn late_storage_results_cannot_mark_a_replaced_binding_as_loaded_or_saved() {
        let (mut world, _, _, first, _) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&record);
        let (instance, cursor, generation) = {
            let mut bindings = world.resource_mut::<Bindings>();
            let document = bindings.documents.get_mut(&key).unwrap();
            let (write, cursor) = document
                .journal
                .prepare(&document.doc, &document.pending)
                .unwrap();
            (document.journal.instance.clone(), cursor, write.generation)
        };
        let metadata = store::record_edit_drafts::Metadata {
            stamp: store::record_edit_drafts::Stamp {
                generation,
                sequence: 1,
            },
            pending: vec![],
            update_count: 0,
            update_bytes: 0,
            request_bytes: 0,
            checkpoint_bytes: 0,
            write_hash: String::new(),
        };
        let mut bindings = world.resource_mut::<Bindings>();
        bindings
            .documents
            .insert(key.clone(), Document::new(Source::Local, None));
        let replacement = bindings.documents[&key].journal.instance.clone();
        bindings
            .sender
            .send(StorageEvent::Loaded(
                key.clone(),
                instance.clone(),
                Ok(engine::record_edit::Loaded {
                    metadata: Some(metadata.clone()),
                    recovered: None,
                }),
            ))
            .unwrap();
        bindings
            .sender
            .send(StorageEvent::Saved(
                key.clone(),
                instance,
                u64::MAX,
                cursor,
                Ok(metadata),
            ))
            .unwrap();
        drop(bindings);
        update(&mut world);
        let bindings = world.resource::<Bindings>();
        let document = &bindings.documents[&key];
        assert_eq!(document.journal.instance, replacement);
        assert!(!document.loaded && document.saved == 0 && document.journal.metadata.is_none());
    }

    #[tokio::test]
    async fn a_failed_journal_write_keeps_the_request_and_explicit_retry_saves_it_once() {
        let (mut world, engine, uid, first, _) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&record);
        store::sqlx::query("CREATE TRIGGER fail_native_recovery BEFORE INSERT ON record_edit_update BEGIN SELECT RAISE(ABORT,'held native recovery'); END").execute(&engine.store.pool).await.unwrap();
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Kept locally é新");
        update(&mut world);
        let request = world.resource::<Bindings>().documents[&key].pending[0]
            .id
            .clone();
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key];
            document.journal.failed && !document.saving
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Olá 👩‍💻 עולם");
        assert!(
            store::record_edit_drafts::metadata(
                &engine.store.pool,
                &storage_source(&key, None),
                &uid
            )
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(
            world.resource::<Bindings>().documents[&key].pending[0].id,
            request
        );
        assert_eq!(
            world
                .get::<EditableText>(first)
                .unwrap()
                .value()
                .to_string(),
            "Kept locally é新"
        );
        store::sqlx::query("DROP TRIGGER fail_native_recovery")
            .execute(&engine.store.pool)
            .await
            .unwrap();
        retry(&mut world, &record);
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key];
            !document.failed && document.joined && document.pending.is_empty() && !document.saving
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Kept locally é新");
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM record_change_receipt WHERE change_uid=?"
            )
            .bind(&request)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn retiring_the_last_editor_releases_only_its_empty_recovery_cache() {
        let (mut world, engine, uid, first, second) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&record);
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Saved before closing");
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key];
            document.pending.is_empty() && !document.saving
        })
        .await;
        assert!(
            store::record_edit_drafts::metadata(
                &engine.store.pool,
                &storage_source(&key, None),
                &uid
            )
            .await
            .unwrap()
            .is_some()
        );
        world.entity_mut(first).remove::<TextBinding>();
        world.entity_mut(second).remove::<TextBinding>();
        world
            .resource_mut::<Bindings>()
            .documents
            .get_mut(&key)
            .unwrap()
            .touched = Instant::now() - Duration::from_secs(31);
        update(&mut world);
        assert!(!world.resource::<Bindings>().documents.contains_key(&key));
        pump(&mut world, |world| {
            world.resource::<Bindings>().cleanup.is_none()
        })
        .await;
        assert!(
            store::record_edit_drafts::metadata(
                &engine.store.pool,
                &storage_source(&key, None),
                &uid
            )
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(
            engine.doc_text(&uid).await.unwrap().1,
            "Saved before closing"
        );
    }

    #[tokio::test]
    async fn a_refused_scalar_can_be_corrected_without_reopening_the_record() {
        let (mut world, _, uid, first, _) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let request = |value: &str| engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: uid.clone(),
            mutation: engine::record_change::Mutation::Quantity {
                value: value.into(),
            },
        };
        submit(&mut world, &record, request("incomplete")).unwrap();
        pump(&mut world, |world| {
            world.resource::<Bindings>().documents[&key(&record)].failed
        })
        .await;
        assert_eq!(
            world.resource::<Bindings>().documents[&key(&record)]
                .pending
                .len(),
            1
        );
        submit(&mut world, &record, request("3.75")).unwrap();
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key(&record)];
            document.pending.is_empty() && !document.saving
        })
        .await;
        assert!(!world.resource::<Bindings>().documents[&key(&record)].failed);
        assert_eq!(
            world
                .get::<EditableText>(first)
                .unwrap()
                .value()
                .to_string(),
            "Olá 👩‍💻 עולם"
        );
    }
}

type Key = (Source, String);

const PRESENCE_INTERVAL: Duration = Duration::from_millis(150);
const PRESENCE_KEEPALIVE: Duration = Duration::from_secs(5);

struct Presence {
    entity: Entity,
    selection: [usize; 2],
    version: Frontiers,
    value: (String, String, String),
    sent_at: Instant,
}

#[derive(Component, Clone)]
#[require(crate::native::sand::Unsaved(false))]
pub struct TextBinding {
    field: Arc<TextField>,
    observed: Arc<str>,
    confirmed: Arc<str>,
}

#[derive(Clone)]
pub struct TextField {
    pub record: RecordBinding,
    pub property: String,
    pub status: Option<Entity>,
}

impl std::ops::Deref for TextBinding {
    type Target = TextField;

    fn deref(&self) -> &Self::Target {
        &self.field
    }
}

impl std::ops::DerefMut for TextBinding {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.field)
    }
}

impl TextBinding {
    fn new(
        record: RecordBinding,
        property: &str,
        status: Option<Entity>,
        observed: Arc<str>,
    ) -> Self {
        Self {
            field: Arc::new(TextField {
                record,
                property: property.into(),
                status,
            }),
            confirmed: observed.clone(),
            observed,
        }
    }

    pub(crate) fn unsaved(&self, value: &str) -> bool {
        value != self.confirmed.as_ref()
    }

    pub(crate) fn editor_unsaved(&self, text: &EditableText) -> bool {
        text.value() != self.confirmed.as_ref()
    }
}

#[cfg(feature = "behavior-checks")]
#[derive(Debug)]
pub struct EditObservation {
    pub revision: u64,
    pub durable_revision: u64,
    pub pending: Vec<String>,
    pub saving: bool,
    pub writable: bool,
    pub applied: bool,
    pub unsaved: bool,
    pub warning: bool,
    pub failed: bool,
}

#[cfg(feature = "behavior-checks")]
pub fn observe_edit(world: &World, entity: Entity) -> Option<EditObservation> {
    let binding = world.get::<TextBinding>(entity)?;
    let text = world.get::<EditableText>(entity)?;
    let document = world
        .get_resource::<Bindings>()?
        .documents
        .get(&key_ref(&binding.record))?;
    let value = text.value().to_string();
    Some(EditObservation {
        revision: document.revision,
        durable_revision: document.saved,
        pending: document
            .pending
            .iter()
            .map(|request| request.id.clone())
            .collect(),
        saving: document.saving,
        writable: document.joined && document.writable.contains(&binding.property),
        applied: binding.observed.as_ref() == value
            && document.doc.get_text(binding.property.as_str()).to_string() == value,
        unsaved: binding.unsaved(&value),
        warning: crate::native::save_feedback::warning(world, entity),
        failed: document.failed,
    })
}

#[derive(Component)]
pub(crate) struct BindingStatus;

pub(crate) fn enabled(world: &World) -> bool {
    world.contains_resource::<Bindings>()
}

pub(crate) fn active(world: &World, entity: Entity) -> bool {
    world.get::<TextBinding>(entity).is_some_and(|binding| {
        world
            .get_resource::<Bindings>()
            .is_some_and(|bindings| bindings.documents.contains_key(&key_ref(&binding.record)))
    })
}

fn visible(world: &World, entity: Entity) -> bool {
    if world
        .get::<InheritedVisibility>(entity)
        .is_some_and(|visibility| !visibility.get())
    {
        return false;
    }
    let Some(node) = world.get::<ComputedNode>(entity) else {
        return true;
    };
    if node.size.x <= 0.0 || node.size.y <= 0.0 {
        return false;
    }
    let Some(target) = world.get::<ComputedUiRenderTargetInfo>(entity) else {
        return false;
    };
    let Some(transform) = world.get::<UiGlobalTransform>(entity) else {
        return false;
    };
    let center = transform.affine().translation;
    let bounds = Rect::from_center_size(center, node.size);
    bounds.max.x >= 0.0
        && bounds.max.y >= 0.0
        && bounds.min.x <= target.physical_size().x as f32
        && bounds.min.y <= target.physical_size().y as f32
}

pub(crate) fn status<'a>(world: &'a World, record: &RecordBinding) -> Option<&'a str> {
    world
        .get_resource::<Bindings>()?
        .documents
        .get(&key_ref(record))
        .map(|document| document.status.as_str())
}

pub(crate) fn failed(world: &World, record: &RecordBinding) -> bool {
    failed_message(world, &record.source, &record.uid).is_some()
}

pub(crate) fn failed_message<'a>(world: &'a World, source: &Source, uid: &str) -> Option<&'a str> {
    world
        .get_resource::<Bindings>()
        .and_then(|bindings| bindings.documents.get(&key_parts(source, uid)))
        .filter(|document| document.failed)
        .map(|document| document.status.as_str())
}

pub(crate) fn pending(world: &World, record: &RecordBinding) -> bool {
    world
        .get_resource::<Bindings>()
        .and_then(|bindings| bindings.documents.get(&key_ref(record)))
        .is_some_and(|document| document.saving || !document.pending.is_empty())
}

struct Document {
    doc: LoroDoc,
    undo: UndoManager,
    text_version: Frontiers,
    text: HashMap<String, Arc<str>>,
    source: Source,
    principal: Option<String>,
    sender: Option<crate::native::cell_bridge::Sender>,
    pending: VecDeque<Arc<engine::record_change::Request>>,
    journal: journal::Journal,
    inflight: usize,
    dispatched: Option<Instant>,
    sequence: crate::native::cell_bridge::EditSequence,
    loaded: bool,
    joined: bool,
    joining: Option<Instant>,
    recovery_join: Option<String>,
    writable: Vec<String>,
    revision: u64,
    saved: u64,
    saving: bool,
    status: String,
    failed: bool,
    incoming: VecDeque<incoming::Message>,
    cursors: cursors::Cursors,
    presence: Option<Presence>,
    touched: Instant,
}

impl Document {
    fn new(source: Source, principal: Option<String>) -> Self {
        let doc = LoroDoc::new();
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(500);
        Self {
            text_version: doc.state_frontiers(),
            text: HashMap::new(),
            doc,
            undo,
            source,
            principal,
            sender: None,
            pending: VecDeque::new(),
            journal: journal::Journal::default(),
            inflight: 0,
            dispatched: None,
            sequence: Default::default(),
            loaded: false,
            joined: false,
            joining: None,
            recovery_join: None,
            writable: Vec::new(),
            revision: 0,
            saved: 0,
            saving: false,
            status: "Opening Record…".into(),
            failed: false,
            incoming: VecDeque::new(),
            cursors: default(),
            presence: None,
            touched: Instant::now(),
        }
    }

    fn refresh_text(&mut self) {
        let version = self.doc.state_frontiers();
        if self.text_version != version {
            self.text.clear();
            self.text_version = version;
        }
    }

    fn text(&mut self, property: &str) -> Arc<str> {
        if let Some(value) = self.text.get(property) {
            return value.clone();
        }
        let value: Arc<str> = self.doc.get_text(property).to_string().into();
        self.text.insert(property.into(), value.clone());
        value
    }

    fn reset_dispatch(&mut self) {
        if self.inflight != 0 || self.sequence.cancelled() {
            self.sequence.cancel();
            self.sequence = Default::default();
        }
        self.inflight = 0;
        self.dispatched = None;
    }

    fn replace_sender(&mut self, sender: Option<crate::native::cell_bridge::Sender>) {
        let same = match (&self.sender, &sender) {
            (Some(previous), Some(next)) => previous.same_channel(next),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.reset_dispatch();
            self.joined = false;
            self.joining = None;
        }
        self.sender = sender;
    }

    fn queue(&mut self, uid: &str, before: &VersionVector) -> Result<(), String> {
        if before == &self.doc.oplog_vv() {
            return Ok(());
        }
        let update_base64 = engine::collab::encode_update(&self.doc, before)?;
        self.pending
            .push_back(Arc::new(engine::record_change::Request {
                id: nucleus::new_uid("op"),
                record_uid: uid.into(),
                mutation: engine::record_change::Mutation::Text { update_base64 },
            }));
        nucleus::performance::Operation::new(&self.pending.back().unwrap().id)
            .mark("record-edit-queued");
        self.revision += 1;
        self.status = "Saving locally…".into();
        Ok(())
    }
}

enum StorageEvent {
    Loaded(Key, String, Result<engine::record_edit::Loaded, String>),
    Saved(
        Key,
        String,
        u64,
        journal::Cursor,
        Result<store::record_edit_drafts::Metadata, String>,
    ),
}

#[derive(Resource)]
struct Bindings {
    documents: Documents<Document>,
    sender: tokio::sync::mpsc::UnboundedSender<StorageEvent>,
    incoming: tokio::sync::mpsc::UnboundedReceiver<StorageEvent>,
    wake: crate::host::WakeDeadline,
    principals: HashMap<Source, String>,
    cleanup: Option<journal::Cleanup>,
}

impl Default for Bindings {
    fn default() -> Self {
        let (sender, incoming) = tokio::sync::mpsc::unbounded_channel();
        Self {
            documents: Documents::default(),
            sender,
            incoming,
            wake: Default::default(),
            principals: HashMap::new(),
            cleanup: None,
        }
    }
}

pub struct RecordBindingPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SyncBindings;

impl Plugin for RecordBindingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bindings>()
            .add_systems(PostUpdate, gate.before(bevy::text::EditableTextSystems))
            .add_systems(
                Update,
                local_messages
                    .after(crate::native::cell_bridge::ReceiveCell)
                    .run_if(crate::native::laboratory::normal),
            )
            .add_systems(
                PostUpdate,
                update
                    .in_set(SyncBindings)
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::native::actions::ApplyActions)
                    .run_if(crate::native::laboratory::normal),
            )
            .add_systems(
                PostUpdate,
                paint_cursors.after(bevy::ui::UiSystems::PostLayout),
            );
    }
}

pub fn can_edit(world: &World, entity: Entity) -> bool {
    if crate::native::laboratory::blocked(world, entity) {
        return false;
    }
    let Some(binding) = world.get::<TextBinding>(entity) else {
        return false;
    };
    world
        .get_resource::<Bindings>()
        .and_then(|bindings| bindings.documents.get(&key_ref(&binding.record)))
        .is_some_and(|document| {
            document.joined && document.writable.contains(&binding.property) && !document.failed
        })
}

#[cfg(test)]
pub(crate) fn hold_canonical_requests(
    world: &mut World,
    entity: Entity,
) -> tokio::sync::mpsc::Receiver<ClientMessage> {
    let binding = &world.get::<TextBinding>(entity).unwrap().record;
    let key = key(binding);
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    sender
        .try_send(ClientMessage::CollabLeave {
            record_uid: key.1.clone(),
        })
        .unwrap();
    let mut bindings = world.resource_mut::<Bindings>();
    let document = bindings.documents.get_mut(&key).unwrap();
    assert!(document.joined && document.pending.is_empty());
    document.sender = Some(sender.into());
    receiver
}

fn gate(
    bindings: Res<Bindings>,
    access: crate::native::laboratory::OwnerAccess,
    mut fields: Query<(Entity, &TextBinding, &mut EditableText)>,
) {
    let _gate = performance::Span::new(Stage::RecordBindingGate);
    for (entity, binding, mut text) in &mut fields {
        let writable = !access.blocked(entity)
            && bindings
                .documents
                .get(&key_ref(&binding.record))
                .is_some_and(|document| {
                    document.joined
                        && document.writable.contains(&binding.property)
                        && !document.failed
                });
        if !writable {
            if text.pending_paste.is_some() {
                text.pending_paste = None;
                performance::count(Count::RecordBindingDeniedPastes);
            }
            if text.pending_edits.iter().any(changes_text) {
                let before = text.pending_edits.len();
                text.pending_edits.retain(|edit| !changes_text(edit));
                performance::add(
                    Count::RecordBindingDeniedEdits,
                    (before - text.pending_edits.len()) as u64,
                );
            }
        }
    }
}

fn changes_text(edit: &bevy::text::TextEdit) -> bool {
    matches!(
        edit,
        bevy::text::TextEdit::Cut
            | bevy::text::TextEdit::Paste
            | bevy::text::TextEdit::Insert(_)
            | bevy::text::TextEdit::Backspace
            | bevy::text::TextEdit::BackspaceWord
            | bevy::text::TextEdit::Delete
            | bevy::text::TextEdit::DeleteWord
            | bevy::text::TextEdit::ImeSetCompose { .. }
            | bevy::text::TextEdit::ImeCommit { .. }
    )
}

fn key(record: &RecordBinding) -> Key {
    (record.source.clone(), record.uid.clone())
}

fn storage_source(key: &Key, principal: Option<&str>) -> String {
    let mut source = serde_json::to_string(&key.0).expect("Record source");
    if let Some(principal) = principal {
        source.push('\n');
        source.push_str(principal);
    }
    source
}

pub fn attach(
    world: &mut World,
    entity: Entity,
    record: RecordBinding,
    property: &str,
    status: Option<Entity>,
) {
    if crate::native::laboratory::blocked(world, entity) {
        return;
    }
    if let Some(status) = status {
        world.entity_mut(status).insert(BindingStatus);
    }
    let observed: Arc<str> = world
        .get::<EditableText>(entity)
        .map(|text| text.value().to_string())
        .unwrap_or_default()
        .into();
    world.entity_mut(entity).insert((
        TextBinding::new(record, property, status, observed),
        crate::native::sand_store::SandCredits(crate::native::credits::ATTRIBUTIONS),
    ));
}

pub fn receive(
    world: &mut World,
    source: Source,
    message: impl std::borrow::Borrow<ServerMessage>,
) {
    incoming::receive(world, source, message.borrow());
}

fn local_messages(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
    mut selected: Local<Vec<Arc<ServerMessage>>>,
) {
    let selection = performance::Span::new(Stage::RecordBindingIncomingSelect);
    let Some(messages) = world.get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
    else {
        return;
    };
    selected.extend(
        cursor
            .read(messages)
            .filter(|message| incoming::relevant(message.0.as_ref()))
            .map(|message| message.0.clone()),
    );
    performance::add(Count::RecordBindingMessagesSelected, selected.len() as u64);
    drop(selection);
    for message in selected.drain(..) {
        receive(world, Source::Local, message.as_ref());
    }
}

fn fail(document: &mut Document, error: impl std::fmt::Display) {
    document.sequence.cancel();
    document.failed = true;
    document.status = format!("Not saved: {error}. Your draft is kept.");
}

fn load_draft(document: &mut Document, loaded: engine::record_edit::Loaded) -> Result<(), String> {
    let restored = loaded.recovered.is_some();
    if let Some(draft) = loaded.recovered {
        document
            .doc
            .import(&draft.snapshot)
            .map_err(|error| error.to_string())?;
        if !draft.pending_updates.is_empty() {
            document
                .doc
                .import_batch(&draft.pending_updates)
                .map_err(|error| error.to_string())?;
        }
        let entered = std::mem::take(&mut document.pending);
        document.pending = draft.pending.into();
        document.pending.extend(entered);
    }
    document
        .journal
        .loaded(loaded.metadata, restored, &document.doc);
    document.loaded = true;
    Ok(())
}

fn import_collaboration(
    document: &mut Document,
    encoded: String,
    state: bool,
) -> Result<(), String> {
    let empty_before = document.doc.len_ops() == 0;
    let bytes = B64.decode(&encoded).map_err(|error| error.to_string())?;
    let status = document
        .doc
        .import(&bytes)
        .map_err(|error| error.to_string())?;
    let initial = state && document.journal.base(encoded, empty_before, &document.doc);
    if !initial && status.pending.is_some_and(|pending| !pending.is_empty()) {
        document.journal.pending_import(bytes)?;
    }
    Ok(())
}

pub(crate) fn submit(
    world: &mut World,
    record: &RecordBinding,
    request: engine::record_change::Request,
) -> Result<(), String> {
    if crate::native::laboratory::blocked(world, record.area) {
        return Err("Workspace is suspended".into());
    }
    let engine = crate::native::practice_cells::engine(world, &record.source)
        .ok_or("Cell storage is unavailable")?;
    let channel = crate::native::protein_area::editor_sender(world, record);
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let mut bindings = world
        .get_resource_mut::<Bindings>()
        .ok_or("Record bindings are unavailable")?;
    let reference = key_ref(record);
    if !bindings.documents.contains_key(&reference) {
        if bindings.documents.len() >= 64 {
            return Err("Wait for pending editors to finish".into());
        }
        let key = key(record);
        let principal = bindings.principals.get(&key.0).cloned();
        let storage = storage_source(&key, principal.as_deref());
        bindings
            .documents
            .insert(key.clone(), Document::new(record.source.clone(), principal));
        let instance = bindings.documents[&key].journal.instance.clone();
        let sender = bindings.sender.clone();
        tokio::spawn(async move {
            let result = engine
                .load_record_edit_draft(&storage, &key.1)
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(StorageEvent::Loaded(key, instance, result));
            if let Some(wake) = wake {
                wake.ring();
            }
        });
    }
    let principal = bindings.principals.get(&record.source).cloned();
    let document = bindings.documents.get_mut(&reference).unwrap();
    if document.failed {
        let correcting = document.principal == principal
            && document.pending.front().is_some_and(|previous| {
                document.inflight != 0
                    && match (&previous.mutation, &request.mutation) {
                        (engine::record_change::Mutation::Text { .. }, _) => false,
                        (
                            engine::record_change::Mutation::Work { field: before, .. },
                            engine::record_change::Mutation::Work { field: after, .. },
                        ) => before.key() == after.key(),
                        (before, after) => {
                            std::mem::discriminant(before) == std::mem::discriminant(after)
                        }
                    }
            });
        if !correcting {
            return Err(document.status.clone());
        }
        document.pending.pop_front();
        document.reset_dispatch();
        document.failed = false;
    }
    if document.pending.len() >= 512 {
        return Err("Wait for pending edits to sync".into());
    }
    document.replace_sender(channel);
    nucleus::performance::Operation::new(&request.id).mark("record-edit-queued");
    document.pending.push_back(Arc::new(request));
    document.revision += 1;
    document.status = "Saving locally…".into();
    Ok(())
}

fn splice(doc: &LoroDoc, property: &str, before: &str, after: &str) -> Result<(), String> {
    let mut prefix = 0;
    let mut prefix_bytes = 0;
    for (old, new) in before.chars().zip(after.chars()) {
        if old != new {
            break;
        }
        prefix += 1;
        prefix_bytes += old.len_utf8();
    }
    let old = &before[prefix_bytes..];
    let new = &after[prefix_bytes..];
    let suffix_bytes: usize = old
        .chars()
        .rev()
        .zip(new.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(character, _)| character.len_utf8())
        .sum();
    let text = doc.get_text(property);
    let removed = old[..old.len() - suffix_bytes].chars().count();
    if removed != 0 {
        text.delete(prefix, removed)
            .map_err(|error| error.to_string())?;
    }
    let inserted = &new[..new.len() - suffix_bytes];
    if !inserted.is_empty() {
        text.insert(prefix, inserted)
            .map_err(|error| error.to_string())?;
    }
    doc.commit();
    Ok(())
}

type SelectionAnchors = HashMap<Entity, (Option<Cursor>, Option<Cursor>)>;

fn selection(
    document: &Document,
    binding: &TextBinding,
    text: &EditableText,
) -> (Option<Cursor>, Option<Cursor>) {
    let _capture = performance::Span::new(Stage::RecordSelectionCapture);
    let selected = text.editor.raw_selection();
    let value = text.value();
    let [anchor, focus] = lince_interface::text::scalar_positions(
        value.chars(),
        [selected.anchor().index(), selected.focus().index()],
    );
    let field = document.doc.get_text(binding.property.as_str());
    let first = field.get_cursor(anchor, Default::default());
    let second = if anchor == focus {
        first.clone()
    } else {
        field.get_cursor(focus, Default::default())
    };
    (first, second)
}

fn set_text(world: &mut World, entity: Entity, value: &str, positions: Option<(usize, usize)>) {
    if !world.contains_resource::<FontCx>() || !world.contains_resource::<LayoutCx>() {
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(value);
        return;
    }
    world.resource_scope(|world, mut fonts: Mut<FontCx>| {
        world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
            let mut text = world.get_mut::<EditableText>(entity).unwrap();
            text.editor.set_text(value);
            if let Some((anchor, focus)) = positions {
                let _restore = performance::Span::new(Stage::RecordSelectionRestore);
                let [anchor, focus] =
                    lince_interface::text::byte_positions(value.chars(), [anchor, focus]);
                text.editor
                    .driver(&mut fonts.context, &mut layout.0)
                    .select_byte_range(anchor, focus);
            }
        });
    });
}

fn publish_presence(
    world: &World,
    document: &mut Document,
    entity: Entity,
    binding: &TextBinding,
) -> Option<Instant> {
    let text = world.get::<EditableText>(entity)?;
    let selected = text.editor.raw_selection();
    let indices = [selected.anchor().index(), selected.focus().index()];
    let now = Instant::now();
    if let Some(previous) = &document.presence {
        if previous.entity == entity
            && previous.selection == indices
            && previous.version == document.text_version
            && previous.value.0 == binding.property
            && now < previous.sent_at + PRESENCE_KEEPALIVE
        {
            return Some(previous.sent_at + PRESENCE_KEEPALIVE);
        }
        if now < previous.sent_at + PRESENCE_INTERVAL {
            return Some(previous.sent_at + PRESENCE_INTERVAL);
        }
    }
    let (Some(anchor), Some(focus)) = selection(document, binding, text) else {
        return Some(now + PRESENCE_INTERVAL);
    };
    let anchor = B64.encode(anchor.encode());
    let focus = if indices[0] == indices[1] {
        anchor.clone()
    } else {
        B64.encode(focus.encode())
    };
    let value = (binding.property.clone(), anchor, focus);
    let publish = document.presence.as_ref().is_none_or(|previous| {
        previous.value != value || now >= previous.sent_at + PRESENCE_KEEPALIVE
    });
    if publish
        && !document.sender.as_ref().is_some_and(|sender| {
            sender
                .try_send(ClientMessage::CollabPresence {
                    record_uid: binding.record.uid.clone(),
                    property: value.0.clone(),
                    anchor: value.1.clone(),
                    focus: value.2.clone(),
                })
                .is_ok()
        })
    {
        return Some(now + PRESENCE_INTERVAL);
    }
    let sent_at = if publish {
        now
    } else {
        document.presence.as_ref()?.sent_at
    };
    document.presence = Some(Presence {
        entity,
        selection: indices,
        version: document.text_version.clone(),
        value,
        sent_at,
    });
    Some(sent_at + PRESENCE_KEEPALIVE)
}

fn update(world: &mut World) {
    world
        .run_system_cached(update_bindings)
        .expect("update Record bindings");
}

fn update_bindings(
    world: &mut World,
    views: &mut QueryState<(Entity, &'static TextBinding)>,
    mut prepared: Local<preparation::Views>,
    mut anchors: Local<SelectionAnchors>,
) {
    let _timing = performance::Span::new(Stage::RecordBindingUpdate);
    let Some(mut bindings) = world.remove_resource::<Bindings>() else {
        prepared.clear();
        *anchors = SelectionAnchors::new();
        return;
    };
    if bindings
        .cleanup
        .as_ref()
        .is_some_and(journal::Cleanup::finished)
    {
        bindings.cleanup = None;
    }
    let mut retired = Vec::new();
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let mut deadline: Option<Instant> = None;
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    prepared.collect(world, views, focus);
    for (entity, binding) in prepared.ordered() {
        let reference = key_ref(&binding.record);
        let Some(engine) = crate::native::practice_cells::engine(world, &binding.record.source)
        else {
            continue;
        };
        if focus != Some(*entity) && !visible(world, *entity) {
            continue;
        }
        if !bindings.documents.contains_key(&reference) {
            if bindings.documents.len() >= 64 {
                if focus != Some(*entity) || bindings.cleanup.is_some() {
                    continue;
                }
                let oldest = bindings
                    .documents
                    .iter()
                    .filter(|(_, document)| document.pending.is_empty() && !document.saving)
                    .min_by_key(|(_, document)| document.touched)
                    .map(|(key, _)| key.clone());
                let Some(oldest) = oldest else {
                    continue;
                };
                if let Some(document) = bindings.documents.remove(&oldest) {
                    journal::retire(world, &oldest, &document, &mut retired);
                    if let Some(sender) = document.sender {
                        let _ = sender.try_send(ClientMessage::CollabLeave {
                            record_uid: oldest.1,
                        });
                    }
                }
            }
            let key = key(&binding.record);
            let principal = bindings.principals.get(&key.0).cloned();
            let storage = storage_source(&key, principal.as_deref());
            bindings.documents.insert(
                key.clone(),
                Document::new(binding.record.source.clone(), principal),
            );
            let instance = bindings.documents[&key].journal.instance.clone();
            let engine = engine.clone();
            let sender = bindings.sender.clone();
            let wake = wake.clone();
            tokio::spawn(async move {
                let result = engine
                    .load_record_edit_draft(&storage, &key.1)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(StorageEvent::Loaded(key, instance, result));
                if let Some(wake) = wake {
                    wake.ring();
                }
            });
        }
        let document = bindings.documents.get_mut(&reference).unwrap();
        document.touched = Instant::now();
        if document
            .sender
            .as_ref()
            .is_none_or(|sender| sender.is_closed())
        {
            document.sender = crate::native::protein_area::editor_sender(world, &binding.record);
            document.joined = false;
            document.joining = None;
            document.reset_dispatch();
        }
    }
    while let Ok(event) = bindings.incoming.try_recv() {
        match event {
            StorageEvent::Loaded(key, instance, result) => {
                if let Some(document) = bindings
                    .documents
                    .get_mut(&key)
                    .filter(|document| document.journal.instance == instance)
                {
                    match result.and_then(|raw| load_draft(document, raw)) {
                        Ok(()) => {}
                        Err(error) => fail(document, error),
                    }
                }
            }
            StorageEvent::Saved(key, instance, revision, cursor, result) => {
                if let Some(document) = bindings
                    .documents
                    .get_mut(&key)
                    .filter(|document| document.journal.instance == instance)
                {
                    document.saving = false;
                    match result {
                        Ok(metadata) => {
                            if performance::enabled() {
                                let previous = document
                                    .journal
                                    .metadata
                                    .as_ref()
                                    .map_or(&[][..], |previous| previous.pending.as_slice());
                                let mut known =
                                    dispatch::Ids::new(previous.iter().map(String::as_str));
                                for id in &metadata.pending {
                                    if !known.contains(id) {
                                        nucleus::performance::Operation::new(id)
                                            .mark("record-edit-draft-observed");
                                    }
                                }
                            }
                            document.journal.accept(metadata, cursor);
                            document.saved = revision;
                        }
                        Err(error) => {
                            document.journal.failed = true;
                            fail(document, error);
                        }
                    }
                }
            }
        }
    }
    let undo = world
        .get_resource::<ButtonInput<KeyCode>>()
        .map(|keys| {
            let modifier = keys.pressed(KeyCode::ControlLeft)
                || keys.pressed(KeyCode::ControlRight)
                || keys.pressed(KeyCode::SuperLeft)
                || keys.pressed(KeyCode::SuperRight);
            let redo = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            (modifier && keys.just_pressed(KeyCode::KeyZ), redo)
        })
        .unwrap_or_default();
    prepared.group();
    for (key, document) in &mut bindings.documents {
        let Some(engine) = crate::native::practice_cells::engine(world, &document.source) else {
            continue;
        };
        if document
            .sender
            .as_ref()
            .is_none_or(|sender| sender.is_closed())
        {
            document.sender = crate::native::protein_area::editor_sender(
                world,
                &RecordBinding {
                    area: Entity::PLACEHOLDER,
                    uid: key.1.clone(),
                    source: document.source.clone(),
                },
            );
            document.joined = false;
            document.joining = None;
            document.reset_dispatch();
        }
        let members = prepared.members(key);
        let needs_text = !members.is_empty()
            || document.pending.iter().any(|request| {
                matches!(
                    request.mutation,
                    engine::record_change::Mutation::Text { .. }
                )
            });
        let composing = members.iter().any(|(entity, _)| {
            world.get::<EditableText>(*entity).is_some_and(|text| {
                text.is_composing() || crate::native::record_view::pending_text(text)
            })
        });
        anchors.clear();
        if document.joined && !composing {
            let preserve_selection = document.text_version != document.doc.state_frontiers()
                || document.incoming.iter().any(|message| {
                    matches!(
                        message,
                        incoming::Message::State { .. } | incoming::Message::Change(_)
                    )
                })
                || (undo.0 && members.iter().any(|(entity, _)| focus == Some(*entity)))
                || members.iter().any(|(entity, binding)| {
                    world.get::<EditableText>(*entity).is_some_and(|text| {
                        let value = text.value();
                        value != binding.observed.as_ref()
                            || document
                                .text
                                .get(&binding.property)
                                .is_none_or(|published| value != published.as_ref())
                    })
                });
            for (entity, binding) in members {
                let Some(text) = world.get::<EditableText>(*entity) else {
                    continue;
                };
                let current = text.value();
                let changed = current != binding.observed.as_ref()
                    && !document.failed
                    && document.writable.contains(&binding.property);
                let value = changed.then(|| current.to_string());
                if let Some(value) = &value {
                    performance::count(Count::RecordEditorTextCopies);
                    performance::add(Count::RecordEditorTextCopiedBytes, value.len() as u64);
                    let _timing = performance::Span::new(Stage::RecordEditSplice);
                    let before = document.doc.oplog_vv();
                    if let Err(error) =
                        splice(&document.doc, &binding.property, &binding.observed, value)
                            .and_then(|()| document.queue(&key.1, &before))
                    {
                        fail(document, error);
                    }
                }
                if preserve_selection {
                    let text = world.get::<EditableText>(*entity).unwrap();
                    anchors.insert(*entity, selection(document, binding, text));
                }
                if let Some(value) = value {
                    world.get_mut::<TextBinding>(*entity).unwrap().observed = value.into();
                }
            }
            if undo.0
                && members.iter().any(|(entity, _)| focus == Some(*entity))
                && !document.failed
            {
                let before = document.doc.oplog_vv();
                let result = if undo.1 {
                    document.undo.redo()
                } else {
                    document.undo.undo()
                };
                if let Err(error) = result
                    .map_err(|error| error.to_string())
                    .and_then(|_| document.queue(&key.1, &before))
                {
                    fail(document, error);
                }
            }
        }
        if document.loaded && !composing {
            while let Some(message) = document.incoming.pop_front() {
                match message {
                    incoming::Message::State { snapshot, writable } => {
                        match import_collaboration(document, snapshot, true) {
                            Ok(()) => {
                                document.joined = true;
                                document.joining = None;
                                document.writable = writable;
                                document.status = if document.pending.is_empty() {
                                    "Saved".into()
                                } else {
                                    "Waiting to sync…".into()
                                };
                            }
                            Err(error) => fail(document, error),
                        }
                    }
                    incoming::Message::Change(update) => {
                        if let Err(error) = import_collaboration(document, update, false) {
                            fail(document, error);
                        }
                    }
                    incoming::Message::Cursors(cursors) => document.cursors.replace(cursors),
                    incoming::Message::Acknowledged(id) => {
                        if document
                            .pending
                            .front()
                            .is_some_and(|request| request.id == id)
                        {
                            nucleus::performance::Operation::new(&id)
                                .mark("record-edit-ack-observed");
                            document.pending.pop_front();
                            document.inflight = document.inflight.saturating_sub(1);
                            document.revision += 1;
                            document.status = if document.pending.is_empty() {
                                "Saved".into()
                            } else {
                                "Waiting to sync…".into()
                            };
                        }
                    }
                    incoming::Message::Error {
                        connection: true,
                        message,
                    } => {
                        document.joined = false;
                        document.joining = None;
                        document.reset_dispatch();
                        document.status = format!("Disconnected: {message}. Your draft is kept.");
                    }
                    incoming::Message::Error { message, .. } => fail(document, message),
                }
            }
        }
        if needs_text
            && document.loaded
            && !document.joined
            && !document.failed
            && document
                .joining
                .is_none_or(|at| at.elapsed().as_secs() >= 5)
        {
            if document.sender.as_ref().is_some_and(|sender| {
                sender
                    .try_send(ClientMessage::CollabJoin {
                        id: document
                            .recovery_join
                            .clone()
                            .unwrap_or_else(|| format!("binding-join-{}", key.1)),
                        record_uid: key.1.clone(),
                    })
                    .is_ok()
            }) {
                document.joining = Some(Instant::now());
            }
        }
        if needs_text && document.loaded && !document.joined && !document.failed {
            let now = Instant::now();
            let retry = document
                .joining
                .map(|started| started + Duration::from_secs(5))
                .filter(|at| *at > now)
                .unwrap_or(now + PRESENCE_INTERVAL);
            deadline = Some(deadline.map_or(retry, |previous| previous.min(retry)));
        }
        if document.loaded
            && document.revision > document.saved
            && !document.saving
            && !document.journal.failed
        {
            let timing = performance::Span::new(Stage::RecordDraftPrepare);
            let draft = document.journal.prepare(&document.doc, &document.pending);
            drop(timing);
            match draft {
                Ok((draft, cursor)) => {
                    let operations = if performance::enabled() {
                        let previous = draft
                            .expected
                            .as_ref()
                            .map_or(&[][..], |previous| previous.pending.as_slice());
                        let mut known = dispatch::Ids::new(previous.iter().map(String::as_str));
                        draft
                            .pending
                            .iter()
                            .filter(|request| !known.contains(&request.id))
                            .map(|request| nucleus::performance::Operation::new(&request.id))
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };
                    for operation in &operations {
                        operation.mark("record-edit-draft-queued");
                    }
                    let durable = document.dispatch(needs_text, Some(&draft));
                    let clearing = document.pending.is_empty();
                    document.saving = true;
                    let sender = bindings.sender.clone();
                    let engine = engine.clone();
                    let key = key.clone();
                    let storage = storage_source(&key, document.principal.as_deref());
                    let revision = document.revision;
                    let instance = document.journal.instance.clone();
                    let wake = wake.clone();
                    let queued = performance::start();
                    tokio::spawn(async move {
                        for operation in &operations {
                            operation.mark("record-edit-draft-started");
                        }
                        performance::record_since(Stage::RecordDraftQueueWait, queued);
                        let timing = performance::Span::new(Stage::RecordDraftWrite);
                        if clearing {
                            performance::count(Count::RecordDraftClears);
                        } else {
                            performance::count(Count::RecordDraftWrites);
                        }
                        let result = engine
                            .save_record_edit_draft(&storage, &key.1, draft)
                            .await
                            .map_err(|error| error.to_string());
                        if result.is_ok() {
                            for operation in &operations {
                                operation.mark("record-edit-draft-durable");
                            }
                        }
                        drop(timing);
                        let succeeded = result.is_ok();
                        let _ = sender
                            .send(StorageEvent::Saved(key, instance, revision, cursor, result));
                        for gate in durable {
                            let _ = gate.send(succeeded);
                        }
                        if let Some(wake) = wake {
                            wake.ring();
                        }
                    });
                }
                Err(error) => {
                    document.journal.failed = true;
                    fail(document, error);
                }
            }
        }
        document.dispatch(needs_text, None);
        document.refresh_text();
        let publication = performance::Span::new(Stage::RecordBindingPublish);
        for (entity, binding) in members {
            if document.joined && !composing && !document.failed {
                performance::count(Count::RecordBindingViews);
                let value = document.text(&binding.property);
                if world
                    .get::<EditableText>(*entity)
                    .is_some_and(|text| text.value() != value.as_ref())
                {
                    performance::count(Count::RecordBindingViewChanged);
                    let positions = anchors.get(entity).and_then(|(anchor, focus)| {
                        let anchor = anchor.as_ref()?;
                        let focus = focus.as_ref()?;
                        let first = document.doc.get_cursor_pos(anchor).ok()?.current.pos;
                        let second = if anchor == focus {
                            first
                        } else {
                            document.doc.get_cursor_pos(focus).ok()?.current.pos
                        };
                        Some((first, second))
                    });
                    set_text(world, *entity, &value, positions);
                }
                if world.get::<TextBinding>(*entity).unwrap().observed != value {
                    world.get_mut::<TextBinding>(*entity).unwrap().observed = value.clone();
                }
                if document.pending.is_empty()
                    && world.get::<TextBinding>(*entity).unwrap().confirmed != value
                {
                    world.get_mut::<TextBinding>(*entity).unwrap().confirmed = value;
                }
            }
            if let Some(status) = binding.status {
                if world
                    .get::<Text>(status)
                    .is_some_and(|label| label.0 != document.status)
                {
                    world.get_mut::<Text>(status).unwrap().0 = document.status.clone();
                }
            }
            if document.joined && document.pending.is_empty() && binding.property == "head" {
                let confirmed = document.text("head");
                if let Some(mut record) =
                    world.get_mut::<crate::native::record_view::RecordEditor>(*entity)
                {
                    if record.confirmed != confirmed.as_ref() {
                        record.confirmed = confirmed.to_string();
                    }
                }
            }
            if focus == Some(*entity)
                && document.joined
                && !composing
                && let Some(at) = publish_presence(world, document, *entity, binding)
            {
                deadline = Some(deadline.map_or(at, |previous| previous.min(at)));
            }
        }
        drop(publication);
    }
    bindings.documents.retain(|key, document| {
        let keep = document.touched.elapsed().as_secs() < 30
            || !document.pending.is_empty()
            || document.saving;
        if !keep {
            if bindings.cleanup.is_some() {
                return true;
            }
            journal::retire(world, key, document, &mut retired);
            if let Some(sender) = &document.sender {
                let _ = sender.try_send(ClientMessage::CollabLeave {
                    record_uid: key.1.clone(),
                });
            }
        }
        keep
    });
    if !retired.is_empty() {
        bindings.cleanup = Some(journal::Cleanup::start(retired, wake.clone()));
    }
    let retrying = bindings
        .documents
        .values()
        .any(|document| !document.failed && !document.pending.is_empty());
    let now = Instant::now();
    if retrying {
        let retry = now + PRESENCE_INTERVAL;
        deadline = Some(deadline.map_or(retry, |previous| previous.min(retry)));
    }
    if let Some((deadline, wake)) = deadline.zip(wake) {
        bindings.wake.at(&wake, deadline);
    } else {
        bindings.wake.cancel();
    }
    if prepared.has_groups() {
        anchors.clear();
    } else {
        *anchors = SelectionAnchors::new();
    }
    prepared.release();
    world.insert_resource(bindings);
}

fn paint_cursors(world: &mut World, mut presentation: Local<cursors::Presentation>) {
    presentation.paint(world);
}
