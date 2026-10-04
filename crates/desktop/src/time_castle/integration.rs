use super::*;
use engine::actions::Action;
use lince_interface::sound::{Cue, Output, Queue};
use nucleus::projection::{Context, OccurrenceLink, Status, Window};
use serde_json::{Value, json};
use std::sync::Mutex;

#[derive(Default)]
struct Heard(Mutex<Vec<Cue>>);

impl Output for Heard {
    fn play(&self, cue: Cue) -> Result<(), String> {
        self.0.lock().unwrap().push(cue);
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        self.0.lock().unwrap().clear();
        Ok(())
    }

    fn cancel(&self, scope: u64) -> Result<(), String> {
        self.0.lock().unwrap().retain(|cue| cue.scope != scope);
        Ok(())
    }

    fn retain(&self, scope: u64, keys: Vec<(String, i64)>) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .retain(|cue| cue.scope != scope || keys.contains(&(cue.key.clone(), cue.at_ms)));
        Ok(())
    }
}

async fn task(engine: &engine::Engine, title: &str, quantity: f64, work: Value) -> String {
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: title.into(),
                body: String::new(),
                quantity,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::SetExtension {
                target: uid.clone(),
                namespace: "work".into(),
                fds: work,
            },
            None,
        )
        .await
        .unwrap();
    uid
}

async fn ready(engine: &std::sync::Arc<engine::Engine>, context: &Context, now: i64) {
    engine.request_projection(context.clone()).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if let Some(cache) = store::projection::read(&engine.store.pool, context, now)
                .await
                .unwrap()
            {
                assert!(
                    matches!(cache.status, Status::Ready { .. }),
                    "{:?}",
                    cache.status
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

fn clock(
    world: &mut World,
    root: Entity,
    query: &protein::Protein,
    window: &Window,
    index: u64,
) -> Entity {
    let mut settings = Settings::default();
    settings.set_aperture(4 * 86_400_000);
    settings.sound.mode = lince_interface::sound::Mode::Title;
    let owner = world
        .spawn((
            Node::default(),
            ChildOf(root),
            crate::workspace::WorkspaceMember(1),
            crate::canvas::CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size: Vec2::splat(420.0),
            },
            TimeSettings(settings),
        ))
        .id();
    populate(world, owner);
    world.resource_mut::<Feeds>().active.insert(
        owner,
        Feed {
            source: Source::Local,
            query: query.clone(),
            key: serde_json::to_string(query).unwrap(),
            id: format!("recurrence-clock-{index}"),
            sent: true,
            window: window.clone(),
            sender: None,
        },
    );
    owner
}

fn deliver(world: &mut World, messages: Vec<ServerMessage>) {
    for message in messages {
        assert!(
            !matches!(message, ServerMessage::Error { .. }),
            "{message:?}"
        );
        receive(world, &Source::Local, &message);
    }
    world.flush();
}

fn refresh_alerts(world: &World, clocks: &[Entity], queue: &mut Queue, now: i64) {
    for owner in clocks {
        let view = world.get::<View>(*owner).unwrap();
        let settings = &world.get::<TimeSettings>(*owner).unwrap().0.sound;
        queue.refresh(
            owner.to_bits(),
            now,
            audio::cues(view, owner.to_bits(), settings),
        );
    }
}

fn at(milliseconds: i64) -> String {
    chrono::DateTime::from_timestamp_millis(milliseconds)
        .unwrap()
        .to_rfc3339()
}

#[test]
fn time_castle_real_recurrence_and_scheduled_work_survive_admission_and_materialization() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(recurrence());
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn recurrence() {
    let directory = tempfile::tempdir().unwrap();
    let mut case = simulation::fixtures::daily();
    for invocation in &mut case.cells[0].seed {
        if let Action::CreateRecurrence { consequences, .. } = &mut invocation.action {
            *consequences = vec![nucleus::karma::Consequence::AddQuantity {
                delta: Some(nucleus::fact::zero_delta()),
            }];
        }
    }
    let base = case.start_ms;
    let day = 86_400_000;
    let simulation = simulation::world::World::open(case, &directory.path().join("world"))
        .await
        .unwrap();
    let node = &simulation.nodes["a"];
    node.execution
        .scope(async {
            let engine = node.cell.runtime().engine.clone();
            let organ = store::organs::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let cell = store::cells::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let root_signer = engine::trust::Signer::from_bytes(
                &organ.uid,
                engine::roster::ROOT_KEY_ID,
                [17; 32],
            );
            engine.publish_root_key(&root_signer).await.unwrap();
            engine
                .publish_roster(
                    &root_signer,
                    vec![engine::roster::CellEntry {
                        cell_uid: cell.uid,
                        node_id: "recurrence-clock-test".into(),
                        label: "Recurrence clock integration".into(),
                        operational_key: engine.local_organ_public_key().await.unwrap().unwrap(),
                        sealing_key: None,
                        front_door: false,
                        capabilities: engine::roster::full_capabilities(),
                    }],
                )
                .await
                .unwrap();
            assert!(engine.karma_device_execution().await.unwrap().executing);
            let stock = simulation.captured["stock"].clone();
            engine
                .act(
                    Action::SetExtension {
                        target: stock.clone(),
                        namespace: "work".into(),
                        fds: json!({"estimate_min":10}),
                    },
                    None,
                )
                .await
                .unwrap();
            let point = task(
                &engine,
                "Scheduled point without quantity debt",
                0.0,
                json!({"start":at(base + 600_000)}),
            )
            .await;
            let range = task(
                &engine,
                "Scheduled explicit range",
                -3.0,
                json!({"start":at(base + 660_000),"due":at(base + 1_260_000),"estimate_min":999}),
            )
            .await;
            let deadline = task(
                &engine,
                "Scheduled estimated deadline",
                2.0,
                json!({"due":at(base + 1_800_000),"estimate_min":5}),
            )
            .await;
            let context = Context {
                actor: None,
                window: Window {
                    from_ms: base,
                    until_ms: base + 4 * day,
                    timezone: "UTC".into(),
                },
            };
            engine
                .advance_karma_time(node.execution.now())
                .await
                .unwrap();
            let query = protein::schedule::query(context.window.clone(), Vec::new());
            let mut session = node.cell.runtime().local_session();
            let mut world = World::new();
            world.insert_resource(crate::theme::Typography(Handle::default()));
            world.init_resource::<bevy::input_focus::InputFocus>();
            world.init_resource::<Feeds>();
            world.add_observer(events::selected);
            let root = world.spawn(crate::workspace::Workspaces::default()).id();
            let clocks = [
                clock(&mut world, root, &query, &context.window, 0),
                clock(&mut world, root, &query, &context.window, 1),
            ];
            let mut receiver = crate::full_record::config(&point, Source::Local);
            receiver.listen_record_selection = true;
            let receiver = world
                .spawn((
                    crate::area::InfluenceArea {
                        protein: Some(receiver),
                        ..crate::area::InfluenceArea::new(
                            crate::area::AreaShape::Square,
                            bevy::math::DVec2::ZERO,
                            bevy::math::DVec2::splat(500.0),
                        )
                    },
                    ChildOf(root),
                    crate::workspace::WorkspaceMember(1),
                ))
                .id();
            events::listeners(&mut world);
            for (index, _) in clocks.iter().enumerate() {
                deliver(
                    &mut world,
                    session
                        .handle(ClientMessage::Subscribe {
                            id: format!("recurrence-clock-{index}"),
                            protein: query.clone(),
                        })
                        .await,
                );
            }
            assert!(
                world
                    .get::<View>(clocks[0])
                    .unwrap()
                    .entries
                    .iter()
                    .any(|entry| entry.record_uid == point)
            );
            ready(&engine, &context, base).await;
            deliver(&mut world, session.refresh().await);
            let snapshots = engine
                .projection
                .metrics
                .snapshots
                .load(std::sync::atomic::Ordering::Relaxed);
            for _ in 0..20 {
                engine.request_projection(context.clone()).await.unwrap();
                deliver(&mut world, session.refresh().await);
            }
            assert_eq!(
                engine
                    .projection
                    .metrics
                    .snapshots
                    .load(std::sync::atomic::Ordering::Relaxed),
                snapshots
            );
            let entries = world.get::<View>(clocks[0]).unwrap().entries.clone();
            let manual: Vec<_> = entries
                .iter()
                .filter(|entry| entry.origin["kind"] == "manual")
                .collect();
            assert_eq!(manual.len(), 3);
            assert!(manual.iter().all(|entry| !entry.preview));
            assert!(entries.iter().any(|entry| entry.record_uid == range
                && entry.time.as_ref().unwrap().until_ms == Some(base + 1_260_000)));
            assert!(entries.iter().any(|entry| entry.record_uid == deadline
                && entry.time.as_ref().unwrap().from_ms == base + 1_500_000));
            let recurring: Vec<_> = entries
                .iter()
                .filter(|entry| entry.record_uid == stock)
                .cloned()
                .collect();
            assert!(recurring.len() >= 3, "{recurring:?}");
            assert!(
                recurring
                    .iter()
                    .all(|entry| entry.time.as_ref().unwrap().until_ms
                        == Some(entry.time.as_ref().unwrap().from_ms + 600_000))
            );
            let future = recurring
                .iter()
                .find(|entry| entry.time.as_ref().unwrap().from_ms == base + 2 * day)
                .unwrap()
                .clone();
            let first_key = recurring
                .iter()
                .find(|entry| entry.time.as_ref().unwrap().from_ms == base + day)
                .unwrap()
                .cue_key("")
                .unwrap();
            let palette = palette::Palette::resolve(&world, clocks[0]);
            let labels = annotations::update(
                &mut world,
                clocks[0],
                base,
                Vec2::splat(420.0),
                true,
                true,
                &palette,
            );
            assert!(
                labels.iter().any(|label| label.title.replace('\n', " ")
                    == "Scheduled point without quantity debt")
            );
            assert!(
                labels
                    .iter()
                    .filter(|label| label.occurrence.index < entries.len())
                    .all(
                        |label| entries[label.occurrence.index].origin["kind"] != "manual"
                            || !label.time.contains("projected")
                    )
            );
            let scheduled_id = entries
                .iter()
                .find(|entry| entry.record_uid == point)
                .unwrap()
                .id
                .clone();
            crate::actions::Action::apply(
                &ui::Select(vec![scheduled_id.clone()]),
                &mut world,
                clocks[0],
            );
            world.flush();
            assert_eq!(
                world.get::<View>(clocks[0]).unwrap().selected,
                [scheduled_id]
            );
            assert!(!chrome::controls_open(&world, clocks[0]));
            assert_eq!(
                world
                    .get::<SelectedOccurrence>(receiver)
                    .unwrap()
                    .0
                    .record_uid,
                point
            );
            let heard = Heard::default();
            let mut queue = Queue::default();
            refresh_alerts(&world, &clocks, &mut queue, base);
            assert!(queue.advance(base, &heard).is_empty());
            queue.advance(base + 600_000, &heard);
            assert_eq!(heard.0.lock().unwrap().len(), 1);
            assert!(!heard.0.lock().unwrap()[0].projected);
            assert_eq!(
                heard.0.lock().unwrap()[0].title,
                "Scheduled point without quantity debt"
            );
            queue.advance(base + day, &heard);
            assert_eq!(
                heard
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|cue| cue.key == first_key)
                    .count(),
                1
            );
            assert!(
                heard
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|cue| cue.key == first_key)
                    .unwrap()
                    .projected
            );
            node.execution.set_time(base + day + 1).unwrap();
            engine
                .advance_karma_time(node.execution.now())
                .await
                .unwrap();
            let admitted: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM karma_rule_application WHERE status = 'applied'",
            )
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            assert!(admitted > 0);
            ready(&engine, &context, base + day + 1).await;
            deliver(&mut world, session.refresh().await);
            let current = world
                .get::<View>(clocks[0])
                .unwrap()
                .entries
                .iter()
                .find(|entry| entry.cue_key("").as_ref() == Some(&first_key));
            assert!(
                current.is_some(),
                "Admitted recurring work must stay visible until its estimated interval ends"
            );
            assert_eq!(current.unwrap().origin["kind"], "manual");
            refresh_alerts(&world, &clocks, &mut queue, base + day + 1);
            queue.advance(base + day + 2, &heard);
            assert_eq!(
                heard
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|cue| cue.key == first_key)
                    .count(),
                1
            );
            let cause: nucleus::simulation::Cause =
                serde_json::from_value(future.origin["cause"].clone()).unwrap();
            let nucleus::simulation::Cause::Rule {
                occurrence,
                consequence,
            } = cause
            else {
                unreachable!()
            };
            let link = OccurrenceLink {
                record: nucleus::karma::TypedUid::new(
                    nucleus::karma::ReferenceKind::Record,
                    &stock,
                )
                .unwrap(),
                occurrence,
                consequence,
            };
            crate::actions::Action::apply(
                &ui::Select(vec![future.id.clone()]),
                &mut world,
                clocks[0],
            );
            world.flush();
            let actual = task(
                &engine,
                "Materialized recurring work",
                -1.0,
                json!({"start":at(base + 2*day),"estimate_min":10,"projection_occurrence":link}),
            )
            .await;
            ready(&engine, &context, base + day + 1).await;
            deliver(&mut world, session.refresh().await);
            let after = &world.get::<View>(clocks[0]).unwrap().entries;
            let matches: Vec<_> = after
                .iter()
                .filter(|entry| entry.cue_key("") == future.cue_key(""))
                .collect();
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].record_uid, actual);
            assert_eq!(matches[0].origin["kind"], "manual");
            assert_eq!(
                world
                    .get::<SelectedOccurrence>(receiver)
                    .unwrap()
                    .0
                    .record_uid,
                actual
            );
            assert_eq!(
                world.get::<View>(clocks[0]).unwrap().selected,
                vec![matches[0].id.clone()]
            );
            refresh_alerts(&world, &clocks, &mut queue, base + day + 1);
            queue.advance(base + 2 * day, &heard);
            assert_eq!(
                heard
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|cue| Some(cue.key.clone()) == future.cue_key(""))
                    .count(),
                1
            );
            assert!(
                !heard
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|cue| Some(cue.key.clone()) == future.cue_key(""))
                    .unwrap()
                    .projected
            );
            node.execution.set_time(base + 2 * day + 1).unwrap();
            deliver(&mut world, session.refresh().await);
            refresh_alerts(&world, &clocks, &mut queue, base + 2 * day + 1);
            queue.advance(base + 2 * day + 2, &heard);
            assert_eq!(
                heard
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|cue| Some(cue.key.clone()) == future.cue_key(""))
                    .count(),
                1
            );
            let added = task(
                &engine,
                "New scheduled work",
                0.0,
                json!({"start":at(base + 2*day + 60_000)}),
            )
            .await;
            deliver(&mut world, session.refresh().await);
            let added_key = world
                .get::<View>(clocks[0])
                .unwrap()
                .entries
                .iter()
                .find(|entry| entry.record_uid == added)
                .unwrap()
                .cue_key("")
                .unwrap();
            refresh_alerts(&world, &clocks, &mut queue, base + 2 * day + 2);
            assert_eq!(
                queue.next_ms(base + 2 * day + 2),
                Some(base + 2 * day + 60_000)
            );
            engine
                .act(
                    Action::SetExtension {
                        target: added.clone(),
                        namespace: "work".into(),
                        fds: json!({"start":at(base + 2*day + 120_000)}),
                    },
                    None,
                )
                .await
                .unwrap();
            deliver(&mut world, session.refresh().await);
            refresh_alerts(&world, &clocks, &mut queue, base + 2 * day + 2);
            assert_eq!(
                queue.next_ms(base + 2 * day + 2),
                Some(base + 2 * day + 120_000)
            );
            queue.advance(base + 2 * day + 60_000, &heard);
            engine
                .act(
                    Action::SetExtension {
                        target: added.clone(),
                        namespace: "work".into(),
                        fds: json!({}),
                    },
                    None,
                )
                .await
                .unwrap();
            deliver(&mut world, session.refresh().await);
            assert!(
                world
                    .get::<View>(clocks[0])
                    .unwrap()
                    .entries
                    .iter()
                    .all(|entry| entry.record_uid != added)
            );
            refresh_alerts(&world, &clocks, &mut queue, base + 2 * day + 60_000);
            queue.advance(base + 2 * day + 120_000, &heard);
            assert!(
                heard
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|cue| cue.key != added_key)
            );
            let disabled_query = protein::schedule::query(
                Window {
                    from_ms: base + 2 * day,
                    until_ms: base + 3 * day,
                    timezone: "UTC".into(),
                },
                Vec::new(),
            );
            engine
                .act(
                    Action::SetExtension {
                        target: point.clone(),
                        namespace: "work".into(),
                        fds: json!({"start":at(base + 2*day + 60_000)}),
                    },
                    None,
                )
                .await
                .unwrap();
            let uncached = protein::execute(&engine.store, &disabled_query)
                .await
                .unwrap();
            assert!(
                uncached.iter().any(
                    |entry| entry["record_uid"] == point && entry["origin"]["kind"] == "manual"
                )
            );
            let mut private = Context {
                actor: None,
                window: context.window.clone(),
            };
            private.actor = engine
                .act(
                    Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Person,
                        head: "Unprivileged visitor".into(),
                        body: String::new(),
                        quantity: 1.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created;
            let filtered = protein::execute_for(&engine.store, &query, private.actor.as_deref())
                .await
                .unwrap();
            assert!(
                filtered
                    .iter()
                    .all(|entry| entry["kind"] != "schedule-entry")
            );
            engine.store.pool.close().await;
        })
        .await;
}
