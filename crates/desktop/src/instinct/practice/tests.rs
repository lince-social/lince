use super::*;
use crate::actions::{ActionSequence, PracticeIntent};
use lince_interface::practice::Progress;

fn fixture() -> (App, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins((
        MinimalPlugins,
        bevy::input::InputPlugin,
        crate::actions::ActionsPlugin,
        crate::edit_mode::EditModePlugin,
        crate::protein_area::ProteinAreaPlugin,
        crate::protein_castle::ProteinCastlePlugin,
        crate::area_mutation::AreaMutationPlugin,
        super::super::InstinctPlugin,
        crate::tutorial::TutorialPlugin,
    ))
    .init_resource::<Assets<Font>>()
    .init_resource::<crate::theme::Typography>()
    .init_resource::<crate::tokens::ThemeSettings>()
    .init_resource::<bevy::input_focus::InputFocus>()
    .insert_resource(crate::wake::WakeSignal::new(|| {}));
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            Workspaces::default(),
            crate::canvas::CanvasView::default(),
            ComputedNode {
                size: Vec2::new(1280.0, 720.0),
                ..default()
            },
            UiGlobalTransform::default(),
        ))
        .id();
    app.update();
    (app, root)
}

fn start(world: &mut World, root: Entity, slug: &str, mode: Mode) {
    StartPage {
        slug: slug.into(),
        mode,
    }
    .apply(world, root);
    input::refresh(world);
}

#[cfg_attr(test, test)]
fn every_page_starts_directly_and_escape_keeps_progress_distinct() {
    let (mut app, root) = fixture();
    for mode in [Mode::Free, Mode::Assisted] {
        for page in lince_interface::handbook::PAGES {
            start(app.world_mut(), root, page.slug, mode);
            assert!(app.world().get::<Practice>(root).is_some(), "{}", page.slug);
            Command::Skip.apply(app.world_mut(), root);
            Command::Close.apply(app.world_mut(), root);
            assert!(
                !app.world()
                    .get::<Practice>(root)
                    .unwrap()
                    .runner
                    .restricted()
            );
            Command::Discard.apply(app.world_mut(), root);
            assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
            assert!(app.world().get::<Practice>(root).is_none());
        }
    }
    assert!(
        !app.world()
            .resource::<Learned>()
            .0
            .0
            .values()
            .any(|progress| *progress == Progress::Practiced)
    );
    start(app.world_mut(), root, "castles", Mode::Assisted);
    let uid = app.world().resource::<Content>().0["sands"]
        .projection
        .uid
        .clone();
    let step = app.world().get::<Practice>(root).unwrap().runner.step;
    ActionSequence::default()
        .then(crate::description::Link {
            reference: uid,
            context: crate::description::Context {
                owner: root,
                source: crate::protein_area::Source::Organ(
                    app.world().get::<Practice>(root).unwrap().source.clone(),
                ),
            },
        })
        .run(app.world_mut(), root);
    let practice = app.world().get::<Practice>(root).unwrap();
    assert_eq!(practice.reference.as_deref(), Some("sands"));
    assert_eq!(practice.runner.step, step);
    assert_eq!(practice.runner.mode, Mode::Assisted);
    Recall(None).apply(app.world_mut(), root);
    assert!(
        app.world()
            .get::<Practice>(root)
            .unwrap()
            .reference
            .is_none()
    );
    Command::Close.apply(app.world_mut(), root);
    Command::Discard.apply(app.world_mut(), root);
}

#[cfg_attr(test, test)]
fn user_action_and_next_share_observation_without_duplicate_samples() {
    for (manual, mode) in [
        (false, Mode::Free),
        (true, Mode::Free),
        (false, Mode::Assisted),
        (true, Mode::Assisted),
    ] {
        let (mut app, root) = fixture();
        start(app.world_mut(), root, "sands", mode);
        Command::Skip.apply(app.world_mut(), root);
        if manual {
            crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
            for kind in [
                crate::sand_store::SandKind::Square,
                crate::sand_store::SandKind::Text,
            ] {
                ActionSequence::default()
                    .then(crate::edit_mode::EditAction::AddSand(kind))
                    .run(app.world_mut(), root);
            }
        }
        Command::Next.apply(app.world_mut(), root);
        app.update();
        assert_eq!(app.world().get::<Practice>(root).unwrap().runner.step, 2);
        let count = app.world_mut().query::<&Owned>().iter(app.world()).count();
        assert_eq!(count, 2);
        if manual {
            let square = find(app.world_mut(), root, Role::Square).unwrap();
            app.world_mut().get_mut::<CanvasItem>(square).unwrap().size = Vec2::new(280.0, 200.0);
            ActionSequence::default()
                .then(crate::sand_placement::PlacementAction::Pin)
                .run(app.world_mut(), square);
        }
        Command::Next.apply(app.world_mut(), root);
        app.update();
        assert_eq!(app.world().get::<Practice>(root).unwrap().runner.step, 3);
        Command::Next.apply(app.world_mut(), root);
        app.update();
        assert!(matches!(
            app.world().get::<Practice>(root).unwrap().runner.phase,
            Phase::Complete
        ));
        assert_eq!(
            app.world_mut().query::<&Owned>().iter(app.world()).count(),
            2
        );
        Command::Next.apply(app.world_mut(), root);
        assert_eq!(
            app.world_mut().query::<&Owned>().iter(app.world()).count(),
            2
        );
        Command::Discard.apply(app.world_mut(), root);
    }
}

#[cfg_attr(test, tokio::test)]
async fn foundational_pages_confirm_native_effects_and_explain_direct_starts() {
    for mode in [Mode::Free, Mode::Assisted] {
        let (mut app, root) = fixture();
        app.add_plugins((
            crate::layout::LayoutPlugin,
            crate::record_binding::RecordBindingPlugin,
            crate::ontology::OntologyPlugin,
        ));
        app.add_systems(PostUpdate, crate::area_effects::update);
        for page in lince_interface::handbook::PAGES
            .iter()
            .take(24)
            .filter(|page| page.slug != "area-record-actions")
        {
            start(app.world_mut(), root, page.slug, mode);
            until(&mut app, |world| {
                world.get::<Practice>(root).unwrap().setup.is_none()
            })
            .await;
            let steps = app
                .world()
                .get::<Practice>(root)
                .unwrap()
                .runner
                .lesson
                .steps
                .len();
            for index in 0..steps {
                Command::Next.apply(app.world_mut(), root);
                until(&mut app, |world| {
                    let practice = world.get::<Practice>(root).unwrap();
                    practice.runner.step > index || practice.runner.phase == Phase::Complete
                })
                .await;
            }
            if page.slug == "learn-areas-of-influence" {
                let sand = find(app.world_mut(), root, Role::Square).unwrap();
                let area = find(app.world_mut(), root, Role::Area).unwrap();
                assert!(
                    app.world()
                        .get::<InfluenceArea>(area)
                        .unwrap()
                        .protein
                        .is_none()
                );
                let center =
                    DVec2::from_array(app.world().get::<InfluenceArea>(area).unwrap().center);
                let position = app.world().get::<CanvasItem>(sand).unwrap().position;
                assert!(app.world().get::<AreaForces>(sand).unwrap().0.iter().any(|force| force.area == area && force.force.dot(position - center) > 0.0));
            }
            Command::Close.apply(app.world_mut(), root);
            until(&mut app, |world| {
                world.get::<Practice>(root).unwrap().cleanup.ready()
            })
            .await;
            Command::Discard.apply(app.world_mut(), root);
        }
    }
}

#[cfg_attr(test, tokio::test)]
async fn protein_query_controls_and_saves_stay_in_the_sample_cell() {
    let (mut app, root) = fixture();
    start(app.world_mut(), root, "protein", Mode::Assisted);
    until(&mut app, |world| {
        world
            .get::<Practice>(root)
            .is_some_and(|practice| practice.records.len() == 2)
    })
    .await;
    Command::Next.apply(app.world_mut(), root);
    until(&mut app, |world| {
        world.get::<Practice>(root).unwrap().runner.step == 1 && sample_rows(world, root).len() == 1
    })
    .await;
    let area = find(app.world_mut(), root, Role::Spawn).unwrap();
    ActionSequence::default()
        .then(crate::protein_area::ProteinAction::Query)
        .run(app.world_mut(), area);
    let editor = app
        .world_mut()
        .query::<(Entity, &crate::protein_area::QueryEditor)>()
        .iter(app.world())
        .find(|(_, link)| link.0 == area)
        .unwrap()
        .0;
    assert!(permits_target(app.world(), editor));
    let source = app.world().get::<Practice>(root).unwrap().source.clone();
    assert_eq!(
        app.world()
            .get::<crate::practice_cells::PracticeSource>(editor)
            .unwrap()
            .0,
        source
    );
    let mut draft = app
        .world_mut()
        .get_mut::<crate::protein_castle::ProteinCastle>(editor)
        .unwrap();
    draft.draft.name = "Practice selection".into();
    draft.draft.slug = "practice-protein".into();
    drop(draft);
    ActionSequence::default()
        .then(crate::protein_castle::ProteinAction::Save)
        .run(app.world_mut(), editor);
    until(&mut app, |world| {
        world
            .resource::<crate::practice_cells::PracticeCells>()
            .records
            .get(&source)
            .map_or(0, |records| records.len())
            == 3
    })
    .await;
    let mut config = app
        .world()
        .get::<InfluenceArea>(area)
        .unwrap()
        .protein
        .clone()
        .unwrap();
    config.source = crate::protein_area::Source::Local;
    crate::protein_area::set_configuration(app.world_mut(), area, Some(config));
    assert_eq!(
        app.world()
            .get::<InfluenceArea>(area)
            .unwrap()
            .protein
            .as_ref()
            .unwrap()
            .source,
        crate::protein_area::Source::Organ(source)
    );
    for step in 1..3 {
        Command::Next.apply(app.world_mut(), root);
        until(&mut app, |world| {
            world.get::<Practice>(root).unwrap().runner.step > step
        })
        .await;
    }
    assert_eq!(sample_rows(app.world_mut(), root).len(), 2);
    Command::Discard.apply(app.world_mut(), root);
}

struct Change;
impl Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        world.entity_mut(target).insert(Name::new("changed"));
    }
}

#[cfg_attr(test, test)]
fn inactive_idle_and_closed_practice_do_not_repeat_input_scans() {
    let (mut app, root) = fixture();
    app.init_resource::<input::Metrics>();
    for _ in 0..128 {
        crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            crate::sand_store::SandKind::Square,
            "",
            DVec2::ZERO,
        );
    }
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(app.world().resource::<input::Metrics>().refreshes, 0);
    let sample = |app: &mut App| {
        let mut durations = Vec::new();
        for _ in 0..51 {
            let started = Instant::now();
            app.update();
            durations.push(started.elapsed().as_micros());
        }
        durations.sort();
        durations[durations.len() / 2]
    };
    let inactive = sample(&mut app);
    start(app.world_mut(), root, "sands", Mode::Assisted);
    for _ in 0..5 {
        app.update();
    }
    let warmed = app.world().resource::<input::Metrics>().refreshes;
    let idle = sample(&mut app);
    assert_eq!(app.world().resource::<input::Metrics>().refreshes, warmed);
    Command::Close.apply(app.world_mut(), root);
    let closed_count = app.world().resource::<input::Metrics>().refreshes;
    let closed = sample(&mut app);
    assert_eq!(
        app.world().resource::<input::Metrics>().refreshes,
        closed_count
    );
    assert!(
        idle <= inactive * 5 / 4 + 100,
        "Idle practice budget exceeded: baseline {inactive} µs, idle {idle} µs"
    );
    assert!(
        closed <= inactive * 5 / 4 + 100,
        "Closed practice budget exceeded: baseline {inactive} µs, closed {closed} µs"
    );
    eprintln!(
        "Instinct median frame, 128 Sands: inactive {inactive} µs; active idle {idle} µs; closed {closed} µs"
    );
    Command::Discard.apply(app.world_mut(), root);
}

#[cfg_attr(test, test)]
fn close_and_component_removal_restore_interaction_and_owned_state() {
    let (mut app, root) = fixture();
    start(app.world_mut(), root, "sands", Mode::Assisted);
    Command::Next.apply(app.world_mut(), root);
    app.update();
    assert!(
        app.world()
            .get::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled
    );
    Command::Close.apply(app.world_mut(), root);
    assert!(
        !app.world()
            .get::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled
    );
    assert!(
        !app.world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .restricted()
    );
    Command::Discard.apply(app.world_mut(), root);
    start(app.world_mut(), root, "sands", Mode::Assisted);
    let workspace = app.world().get::<Practice>(root).unwrap().workspace;
    let shell = app.world().get::<Practice>(root).unwrap().shell;
    let unrelated = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        workspace,
        crate::sand_store::SandKind::EditableText,
        "Untouched",
        DVec2::ZERO,
    );
    input::refresh(app.world_mut());
    assert!(!permits_target(app.world(), unrelated));
    app.world_mut().entity_mut(root).remove::<Practice>();
    app.world_mut().flush();
    assert!(app.world().get_entity(shell).is_err());
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
    assert!(
        !app.world()
            .get::<Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.id == workspace)
    );
}

#[cfg_attr(test, test)]
fn assisted_scopes_actions_focus_and_recovery_and_fails_open_on_ambiguity() {
    let (mut app, root) = fixture();
    start(app.world_mut(), root, "sands", Mode::Assisted);
    let workspace = app.world().get::<Practice>(root).unwrap().workspace;
    let unrelated = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        workspace,
        crate::sand_store::SandKind::EditableText,
        "Untouched",
        DVec2::ZERO,
    );
    input::refresh(app.world_mut());
    assert!(
        app.world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .restricted()
    );
    assert!(!permits_action(app.world(), root, PracticeIntent::Target));
    assert!(!permits_target(app.world(), unrelated));
    ActionSequence::default()
        .then(Change)
        .run(app.world_mut(), unrelated);
    assert!(app.world().get::<Name>(unrelated).is_none());
    let other_root = app
        .world_mut()
        .spawn((crate::container::BoxRoot, Workspaces::default()))
        .id();
    assert!(permits_action(
        app.world(),
        other_root,
        PracticeIntent::Target
    ));
    let duplicate = app
        .world_mut()
        .spawn((
            crate::edit_mode::EditControl {
                root,
                action: crate::edit_mode::EditAction::Toggle,
            },
            ChildOf(root),
        ))
        .id();
    input::refresh(app.world_mut());
    assert!(
        !app.world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .restricted()
    );
    assert!(permits_target(app.world(), unrelated));
    app.world_mut().despawn(duplicate);
    input::refresh(app.world_mut());
    assert!(
        app.world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .restricted()
    );
    Command::Free.apply(app.world_mut(), root);
    assert!(permits_target(app.world(), unrelated));
    Command::Assisted.apply(app.world_mut(), root);
    Command::Close.apply(app.world_mut(), root);
    assert!(permits_target(app.world(), unrelated));
    Command::Discard.apply(app.world_mut(), root);
}

async fn until(app: &mut App, predicate: impl Fn(&mut World) -> bool) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        app.update();
        if predicate(app.world_mut()) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let text: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    let practices: Vec<_> = app
        .world_mut()
        .query::<&Practice>()
        .iter(app.world())
        .map(|practice| {
            (
                practice.runner.step,
                practice.runner.current().and_then(|step| step.operation),
                practice.results.clone(),
                practice.confirmed_entry,
                practice.confirmed_exit,
                practice.runner.phase.clone(),
                practice.setup.is_some(),
                practice.records.len(),
                practice.community.data.is_some(),
                practice.community.receiver.is_some(),
                practice.community.failed,
                practice
                    .tasks
                    .iter()
                    .map(tokio::task::JoinHandle::is_finished)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let records: Vec<_> = app
        .world_mut()
        .query::<(
            &CanvasItem,
            &RecordProperties,
            Option<&crate::protein_area::placement::Pending>,
        )>()
        .iter(app.world())
        .map(|(item, record, pending)| (item.position, record.0.clone(), pending.is_some()))
        .collect();
    let areas: Vec<_> = app
        .world_mut()
        .query::<(Entity, &InfluenceArea)>()
        .iter(app.world())
        .map(|(entity, area)| {
            (
                area.name.clone(),
                crate::area_mutation::armed(app.world(), entity),
                app.world()
                    .get::<crate::area_mutation::MutationStatus>(entity)
                    .map(|status| status.0.clone()),
            )
        })
        .collect();
    panic!("Practice did not confirm its result: {practices:?}, {records:?}, {areas:?}, {text:?}");
}

#[cfg_attr(test, tokio::test)]
async fn record_changes_use_an_isolated_cell_and_confirm_entry_and_exit() {
    let personal = Arc::new(engine::Engine::open_memory().await.unwrap());
    let record = personal
        .act(
            engine::actions::Action::CreateRecordDraft {
                draft: engine::record_creation::Draft {
                    head: "Instinct sample 1".into(),
                    quantity: "-12".into(),
                    ..default()
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: personal.store.clone(),
        engine: personal.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let (mut app, root) = fixture();
    let bridge = crate::cell_bridge::connect(runtime.clone(), crate::wake::WakeSignal::new(|| {}));
    app.insert_non_send(bridge);
    app.insert_resource(crate::app::CellHandle(runtime));
    app.add_plugins(crate::cell_bridge::CellBridgePlugin);
    app.update();
    start(app.world_mut(), root, "area-record-actions", Mode::Free);
    until(&mut app, |world| {
        world
            .get::<Practice>(root)
            .is_some_and(|practice| practice.records.len() == 2)
            && {
                let rows = sample_rows(world, root);
                rows.len() == 2
                    && rows.iter().all(|(entity, _)| {
                        world
                            .get::<crate::protein_area::placement::Pending>(*entity)
                            .is_none()
                            && world
                                .get::<crate::practice_cells::PracticeRecord>(*entity)
                                .is_some()
                    })
            }
    })
    .await;
    for step in 0..3 {
        Command::Next.apply(app.world_mut(), root);
        until(&mut app, |world| {
            world.get::<Practice>(root).unwrap().runner.step > step
        })
        .await;
    }
    let practice = app.world().get::<Practice>(root).unwrap();
    assert!(practice.confirmed_entry && practice.confirmed_exit);
    let source = practice.source.clone();
    let sample_uid = practice.records[0].clone();
    let sample_engine = app
        .world()
        .resource::<crate::practice_cells::PracticeCells>()
        .cells[&source]
        .engine
        .clone();
    assert_eq!(
        store::records::get(&personal.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "-12"
    );
    assert_eq!(
        store::records::get(&sample_engine.store.pool, &sample_uid)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "0"
    );
    Command::Discard.apply(app.world_mut(), root);
    assert!(
        app.world()
            .resource::<crate::practice_cells::PracticeCells>()
            .cells
            .is_empty()
    );
    assert!(
        app.world()
            .resource::<crate::practice_cells::PracticeCells>()
            .retired
            .contains(&source)
    );
}

#[cfg_attr(test, tokio::test)]
async fn work_controls_and_next_confirm_the_same_isolated_changes() {
    for (manual, mode) in [(false, Mode::Free), (true, Mode::Assisted)] {
        let (mut app, root) = fixture();
        app.add_plugins((
            crate::todo::TodoPlugin,
            crate::kanban::KanbanPlugin,
            crate::layout::LayoutPlugin,
            crate::calendar::CalendarPlugin,
        ));
        let personal = crate::practice_cells::persistence::runtime(
            engine::Engine::open_memory().await.unwrap(),
            &std::env::temp_dir().join(nucleus::new_uid("commands")),
        )
        .unwrap();
        let original = personal
            .engine
            .act(
                engine::actions::Action::CreateRecordDraft {
                    draft: engine::record_creation::Draft {
                        head: "Personal task".into(),
                        slug: Some("instinct-note".into()),
                        quantity: "-9".into(),
                        ..default()
                    },
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        app.insert_non_send(crate::cell_bridge::connect(
            personal.clone(),
            crate::wake::WakeSignal::new(|| {}),
        ));
        app.insert_resource(crate::app::CellHandle(personal.clone()));
        app.add_plugins(crate::cell_bridge::CellBridgePlugin);
        for slug in ["todo", "kanban", "calendar", "operation"] {
            start(app.world_mut(), root, slug, mode);
            until(&mut app, |world| find(world, root, Role::Feature).is_some()).await;
            let owner = find(app.world_mut(), root, Role::Feature).unwrap();
            let uid = app.world().get::<Practice>(root).unwrap().note.clone();
            if manual {
                match slug {
                    "todo" => {
                        until(&mut app, |world| crate::todo::contains(world, owner, &uid)).await;
                        crate::todo::Command::Complete(uid.clone()).apply(app.world_mut(), owner);
                    }
                    "kanban" => {
                        until(&mut app, |world| {
                            crate::kanban::column_entity(world, owner, 2).is_some_and(|area| {
                                crate::area_mutation::armed(world, area)
                                    && crate::area_mutation::tracks(world, area, &uid)
                            }) && sample_rows(world, root)
                                .iter()
                                .any(|(_, record)| record == &uid)
                        })
                        .await;
                        crate::kanban::move_card(app.world_mut(), owner, &uid, 2).unwrap();
                    }
                    "calendar" => {
                        until(&mut app, |world| {
                            sample_rows(world, root)
                                .iter()
                                .any(|(_, record)| record == &uid)
                        })
                        .await;
                        let source = app.world().get::<Practice>(root).unwrap().source.clone();
                        crate::calendar::Command::Record(crate::protein_area::RecordBinding {
                            uid: uid.clone(),
                            source: crate::protein_area::Source::Organ(source),
                            area: owner,
                        })
                        .apply(app.world_mut(), owner);
                    }
                    "operation" => {
                        until(&mut app, |world| {
                            crate::operation::ready(world, owner, "instinct-note")
                        })
                        .await;
                        crate::operation::input(app.world_mut(), owner, "@instinct-note");
                        crate::operation::OperationAction::Submit.apply(app.world_mut(), owner);
                    }
                    _ => unreachable!(),
                }
                until(&mut app, |world| {
                    let operation = world
                        .get::<Practice>(root)
                        .unwrap()
                        .runner
                        .current()
                        .unwrap()
                        .operation
                        .unwrap();
                    work::native_complete(world, root, operation)
                })
                .await;
            }
            Command::Next.apply(app.world_mut(), root);
            until(&mut app, |world| {
                world
                    .get::<Practice>(root)
                    .is_some_and(|practice| practice.runner.step >= 1)
            })
            .await;
            if slug == "todo" {
                if manual {
                    crate::todo::Command::Undo.apply(app.world_mut(), owner);
                    until(&mut app, |world| {
                        crate::todo::saved(world, owner, &uid, "-1")
                    })
                    .await;
                }
                Command::Next.apply(app.world_mut(), root);
                until(&mut app, |world| {
                    world.get::<Practice>(root).unwrap().runner.phase == Phase::Complete
                })
                .await;
            }
            let source = app.world().get::<Practice>(root).unwrap().source.clone();
            let sample = app
                .world()
                .resource::<crate::practice_cells::PracticeCells>()
                .cells[&source]
                .clone();
            let quantity = store::records::get(&sample.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .quantity
                .to_string();
            assert_eq!(
                quantity,
                match slug {
                    "todo" | "calendar" => "-1",
                    "kanban" => "-2",
                    _ => "0",
                },
                "{slug}"
            );
            Command::Close.apply(app.world_mut(), root);
            Command::Discard.apply(app.world_mut(), root);
            assert_eq!(
                store::records::get(&personal.store.pool, &original)
                    .await
                    .unwrap()
                    .unwrap()
                    .quantity
                    .to_string(),
                "-9"
            );
        }
    }
}

#[cfg_attr(test, tokio::test(flavor = "multi_thread", worker_threads = 2))]
async fn community_views_keep_identity_pairing_and_roles_in_prepared_cells() {
    let (mut app, root) = fixture();
    app.add_plugins((
        crate::organ_castle::OrganCastlePlugin,
        crate::configuration::ConfigurationPlugin,
        crate::sync_castle::SyncCastlePlugin,
        crate::access_control::AccessControlPlugin,
        crate::record_binding::RecordBindingPlugin,
        crate::thread_castle::ThreadCastlePlugin,
        crate::transfer_castle::TransferCastlePlugin,
        crate::karma_castle::KarmaCastlePlugin,
    ));
    for slug in [
        "learn-organ",
        "contacts",
        "access-control",
        "organ-sync",
        "devices",
        "mail",
        "discovery",
        "conversations",
        "calls",
        "learn-transfer",
        "transfer-automation",
        "instinct-import",
        "learn-sync",
        "blob-sync",
        "backup",
    ] {
        start(app.world_mut(), root, slug, Mode::Assisted);
        until(&mut app, |world| find(world, root, Role::Feature).is_some()).await;
        let steps = app
            .world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .lesson
            .steps
            .len();
        for step in 0..steps {
            Command::Next.apply(app.world_mut(), root);
            until(&mut app, |world| {
                world.get::<Practice>(root).unwrap().runner.step > step
            })
            .await;
        }
        if slug == "organ-sync" {
            let practice = app.world().get::<Practice>(root).unwrap();
            let data = practice.community.data.as_ref().unwrap();
            let peer = &app
                .world()
                .resource::<crate::practice_cells::PracticeCells>()
                .cells[data["source"].as_str().unwrap()];
            let arrived = store::records::get(&peer.store.pool, data["record"].as_str().unwrap())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(arrived.head, "A shared practice note");
        }
        Command::Close.apply(app.world_mut(), root);
        until(&mut app, |world| {
            world.get::<Practice>(root).unwrap().cleanup.ready()
        })
        .await;
        Command::Discard.apply(app.world_mut(), root);
        assert!(
            app.world()
                .resource::<crate::practice_cells::PracticeCells>()
                .cells
                .is_empty()
        );
    }
}

#[cfg_attr(test, test)]
fn discard_removes_moved_samples_and_restores_source_bindings() {
    let (mut app, root) = fixture();
    let personal = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        crate::sand_store::SandKind::Text,
        "Personal",
        DVec2::ZERO,
    );
    start(app.world_mut(), root, "sands", Mode::Free);
    pair(app.world_mut(), root);
    let sample = find(app.world_mut(), root, Role::Square).unwrap();
    app.world_mut()
        .get_mut::<WorkspaceMember>(sample)
        .unwrap()
        .0 = 1;
    let source = nucleus::new_uid("g");
    app.world_mut()
        .entity_mut(sample)
        .insert(crate::practice_cells::PracticeSource(source.clone()));
    let saved = crate::sand_placement::Placement::capture(app.world(), sample);
    assert!(saved.valid());
    let target = app.world_mut().spawn_empty().id();
    saved.restore(app.world_mut(), target);
    assert_eq!(
        crate::practice_cells::source(app.world(), target),
        Some(source)
    );
    Command::Discard.apply(app.world_mut(), root);
    assert!(app.world().get_entity(sample).is_err());
    assert!(app.world().get_entity(personal).is_ok());
}

#[cfg_attr(test, tokio::test(flavor = "multi_thread", worker_threads = 2))]
async fn automation_controls_confirm_saved_clocks_rules_and_habits() {
    let (mut app, root) = fixture();
    app.add_plugins((
        crate::frequency_castle::FrequencyCastlePlugin,
        crate::karma_castle::KarmaCastlePlugin,
        crate::record_binding::RecordBindingPlugin,
        crate::simulation_castle::SimulationCastlePlugin,
        crate::fiote::session::Plugin,
    ));
    for slug in [
        "frequency",
        "learn-karma",
        "habits",
        "commands",
        "simulation",
        "learn-fiote",
    ] {
        start(app.world_mut(), root, slug, Mode::Assisted);
        until(&mut app, |world| find(world, root, Role::Feature).is_some()).await;
        let steps = app
            .world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .lesson
            .steps
            .len();
        for index in 0..steps {
            Command::Next.apply(app.world_mut(), root);
            until(&mut app, |world| {
                world.get::<Practice>(root).unwrap().runner.step > index
                    || world.get::<Practice>(root).unwrap().runner.phase == Phase::Complete
            })
            .await;
        }
        assert_eq!(
            app.world().get::<Practice>(root).unwrap().runner.phase,
            Phase::Complete,
            "{slug}"
        );
        Command::Close.apply(app.world_mut(), root);
        until(&mut app, |world| {
            world.get::<Practice>(root).unwrap().cleanup.ready()
        })
        .await;
        let source = app.world().get::<Practice>(root).unwrap().source.clone();
        let sample = app
            .world()
            .resource::<crate::practice_cells::PracticeCells>()
            .cells[&source]
            .clone();
        assert!(
            !sample
                .engine
                .karma_device_execution()
                .await
                .unwrap()
                .executing,
            "{slug}"
        );
        assert!(
            store::recurrence::all(&sample.store.pool)
                .await
                .unwrap()
                .iter()
                .all(|rule| rule.is_paused()),
            "{slug}"
        );
        Command::Discard.apply(app.world_mut(), root);
    }
}

#[cfg_attr(test, tokio::test)]
async fn local_tools_edit_owned_files_navigate_and_cancel_choices() {
    let (mut app, root) = fixture();
    app.init_resource::<Assets<Image>>()
        .init_resource::<bevy::text::FontCx>()
        .init_resource::<bevy::text::LayoutCx>()
        .add_plugins((
            crate::file_explorer::FileExplorerPlugin,
            crate::ide::IdePlugin,
            crate::document_viewer::DocumentViewerPlugin,
            crate::terminal::TerminalPlugin,
            crate::external_drop::ExternalDropPlugin,
        ));
    for slug in [
        "ide",
        "language-tools",
        "documents",
        "terminal",
        "external-files",
    ] {
        start(app.world_mut(), root, slug, Mode::Assisted);
        until(&mut app, |world| find(world, root, Role::Feature).is_some()).await;
        let nodes: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<Node>>()
            .iter(app.world())
            .collect();
        for entity in nodes {
            if entity != root {
                if let Some(mut node) = app.world_mut().get_mut::<ComputedNode>(entity) {
                    node.size = Vec2::new(800.0, 500.0);
                }
            }
        }
        let steps = app
            .world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .lesson
            .steps
            .len();
        for step in 0..steps {
            Command::Next.apply(app.world_mut(), root);
            until(&mut app, |world| {
                for mut node in world.query::<&mut ComputedNode>().iter_mut(world) {
                    if node.size.x == 0.0 {
                        node.size = Vec2::new(800.0, 500.0);
                    }
                }
                world.get::<Practice>(root).unwrap().runner.step > step
            })
            .await;
        }
        if slug == "ide" {
            let path = &app
                .world()
                .get::<Practice>(root)
                .unwrap()
                .local
                .paths
                .as_ref()
                .unwrap()
                .note;
            assert_eq!(std::fs::read_to_string(path).unwrap(), local::NOTE);
        }
        Command::Close.apply(app.world_mut(), root);
        until(&mut app, |world| {
            world.get::<Practice>(root).unwrap().cleanup.ready()
        })
        .await;
        Command::Discard.apply(app.world_mut(), root);
    }
}

#[cfg_attr(test, tokio::test)]
async fn sound_lessons_use_owned_libraries_and_release_playback_workers() {
    let (mut app, root) = fixture();
    app.add_plugins((
        crate::sound::SoundPlugin,
        crate::recorder_castle::RecorderCastlePlugin,
        crate::sound_area::SoundAreaPlugin,
    ));
    for slug in ["recorder", "area-sound"] {
        start(app.world_mut(), root, slug, Mode::Assisted);
        until(&mut app, |world| find(world, root, Role::Feature).is_some()).await;
        let source = app.world().get::<Practice>(root).unwrap().source.clone();
        let path = app
            .world()
            .resource::<crate::practice_cells::PracticeCells>()
            .directories[&source]
            .join("recordings/instinct-demo.wav");
        assert!(path.is_file());
        assert!(app.world().get_resource::<crate::sound::Audio>().is_none());
        let steps = app
            .world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .lesson
            .steps
            .len();
        for step in 0..steps {
            Command::Next.apply(app.world_mut(), root);
            until(&mut app, |world| {
                world.get::<Practice>(root).unwrap().runner.step > step
            })
            .await;
        }
        Command::Close.apply(app.world_mut(), root);
        until(&mut app, |world| {
            world.get::<Practice>(root).unwrap().cleanup.ready()
        })
        .await;
        assert!(
            !app.world()
                .resource::<crate::practice_cells::PracticeCells>()
                .audio
                .contains_key(&source)
        );
        if slug == "recorder" {
            let owner = find(app.world_mut(), root, Role::Feature).unwrap();
            Command::Keep.apply(app.world_mut(), root);
            assert!(app.world().get::<Practice>(root).is_none());
            until(&mut app, |world| {
                world
                    .resource::<crate::practice_cells::PracticeCells>()
                    .audio
                    .get(&source)
                    .is_some_and(|audio| {
                        audio
                            .paths
                            .iter()
                            .any(|path| path == "recordings/instinct-demo.wav")
                    })
            })
            .await;
            assert_eq!(
                crate::practice_cells::source(app.world(), owner),
                Some(source.clone())
            );
            app.world_mut().despawn(owner);
            retire_source(app.world_mut(), &source);
        } else {
            Command::Discard.apply(app.world_mut(), root);
        }
    }
}

#[cfg_attr(test, tokio::test)]
async fn visual_lessons_save_shader_and_custom_castles_and_restore_views() {
    let (mut app, root) = fixture();
    app.init_resource::<Assets<Image>>()
        .init_resource::<bevy::text::FontCx>()
        .init_resource::<bevy::text::LayoutCx>()
        .init_resource::<bevy::picking::hover::HoverMap>()
        .add_plugins((
            crate::record_binding::RecordBindingPlugin,
            crate::description::DescriptionPlugin,
            crate::shader_castle::ShaderCastlePlugin,
            crate::castle_feed::FeedPlugin,
            crate::ide::IdePlugin,
            crate::information::InformationPlugin,
        ));
    app.add_systems(Update, crate::topology::ui::update);
    for slug in [
        "shaders",
        "topology",
        "custom-castles",
        "inspection",
        "information",
    ] {
        start(app.world_mut(), root, slug, Mode::Assisted);
        until(&mut app, |world| find(world, root, Role::Feature).is_some()).await;
        let steps = app
            .world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .lesson
            .steps
            .len();
        for step in 0..steps {
            Command::Next.apply(app.world_mut(), root);
            until(&mut app, |world| {
                if slug == "shaders" {
                    let field = world
                        .query::<(Entity, &crate::record_binding::TextBinding)>()
                        .iter(world)
                        .find(|(_, binding)| binding.property == "body")
                        .map(|(entity, _)| entity);
                    if let Some(field) = field {
                        world
                            .resource_mut::<bevy::input_focus::InputFocus>()
                            .set(field, bevy::input_focus::FocusCause::Navigated);
                    }
                }
                world.get::<Practice>(root).unwrap().runner.step > step
            })
            .await;
        }
        if slug == "shaders" {
            let practice = app.world().get::<Practice>(root).unwrap();
            let engine = &app
                .world()
                .resource::<crate::practice_cells::PracticeCells>()
                .cells[&practice.source]
                .engine;
            assert!(
                engine
                    .doc_text(&practice.note)
                    .await
                    .unwrap()
                    .1
                    .contains("fn shade(")
            );
            assert_eq!(
                practice.runner.progress.0["step-shader-example"],
                Progress::Visited
            );
        }
        if slug == "custom-castles" {
            let directory = sample_directory(app.world(), root).unwrap().join("castles");
            assert_eq!(std::fs::read_dir(directory).unwrap().count(), 1);
            assert!(crate::custom_castle::added_example(app.world(), root));
        }
        Command::Close.apply(app.world_mut(), root);
        until(&mut app, |world| {
            world.get::<Practice>(root).unwrap().cleanup.ready()
        })
        .await;
        if slug == "topology" {
            assert!(
                !app.world()
                    .get::<crate::topology::view::View>(root)
                    .is_some_and(|view| view.spatial)
            );
        }
        Command::Discard.apply(app.world_mut(), root);
    }
}

#[cfg_attr(test, test)]
fn laboratory_recovery_works_while_the_normal_tutorial_is_suspended() {
    for command in [
        Command::Next,
        Command::Skip,
        Command::Free,
        Command::Assisted,
        Command::Close,
    ] {
        let (mut app, root) = fixture();
        app.add_plugins(crate::laboratory::LaboratoryPlugin);
        start(app.world_mut(), root, "laboratory", Mode::Assisted);
        app.update();
        Command::Next.apply(app.world_mut(), root);
        let lab_root = app
            .world()
            .resource::<crate::laboratory::Laboratory>()
            .root
            .unwrap();
        assert!(crate::laboratory::suspended(app.world(), root));
        assert!(crate::laboratory::resources_visible(app.world()));
        assert!(
            !app.world()
                .resource::<crate::laboratory::Laboratory>()
                .resources
                .sands
                .is_empty()
        );
        laboratory_practice::recover(app.world_mut(), root, lab_root, command);
        assert!(!crate::laboratory::active(app.world()));
        assert!(!crate::laboratory::suspended(app.world(), root));
        if matches!(command, Command::Next) {
            assert_eq!(
                app.world().get::<Practice>(root).unwrap().runner.phase,
                Phase::Complete
            );
        }
        Command::Close.apply(app.world_mut(), root);
        Command::Discard.apply(app.world_mut(), root);
    }
    let (mut app, root) = fixture();
    app.add_plugins(crate::laboratory::LaboratoryPlugin);
    start(app.world_mut(), root, "laboratory", Mode::Assisted);
    app.update();
    Command::Next.apply(app.world_mut(), root);
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.press(KeyCode::ControlLeft);
    keys.press(KeyCode::ShiftLeft);
    keys.press(KeyCode::Escape);
    drop(keys);
    app.world_mut().run_system_cached(emergency).unwrap();
    app.world_mut().flush();
    assert!(!crate::laboratory::active(app.world()));
    assert!(app.world().get::<Practice>(root).is_none());
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
}

#[cfg_attr(test, tokio::test)]
async fn native_control_identity_survives_renaming_and_recreation_and_blocks_missing_actions() {
    let (mut app, root) = fixture();
    app.add_plugins((
        crate::sound::SoundPlugin,
        crate::recorder_castle::RecorderCastlePlugin,
    ));
    start(app.world_mut(), root, "recorder", Mode::Assisted);
    until(&mut app, |world| find(world, root, Role::Feature).is_some()).await;
    input::refresh(app.world_mut());
    let owner = find(app.world_mut(), root, Role::Feature).unwrap();
    let button = input::semantic(app.world_mut(), root).unwrap();
    let children = app
        .world()
        .get::<Children>(button)
        .unwrap()
        .iter()
        .collect::<Vec<_>>();
    for child in children {
        if let Some(mut text) = app.world_mut().get_mut::<Text>(child) {
            text.0 = "A new caption".into();
        }
    }
    app.world_mut().get_mut::<Node>(button).unwrap().margin.left = px(40);
    assert_eq!(input::semantic(app.world_mut(), root), Ok(button));
    app.world_mut().despawn(button);
    input::refresh(app.world_mut());
    Command::Next.apply(app.world_mut(), root);
    assert!(
        app.world()
            .get::<crate::sound::PlaybackPending>(owner)
            .is_none()
    );
    assert!(
        !app.world()
            .get::<Practice>(root)
            .unwrap()
            .runner
            .restricted()
    );
    let replacement = crate::castle_feed::button(
        app.world_mut(),
        owner,
        owner,
        "A recreated control",
        crate::recorder_castle::ui::Control::Play,
    );
    input::refresh(app.world_mut());
    assert_eq!(input::semantic(app.world_mut(), root), Ok(replacement));
    let duplicate = crate::castle_feed::button(
        app.world_mut(),
        owner,
        owner,
        "A second control",
        crate::recorder_castle::ui::Control::Play,
    );
    assert_eq!(
        input::semantic(app.world_mut(), root),
        Err(lince_interface::practice::Resolution::Ambiguous)
    );
    app.world_mut().despawn(duplicate);
    Command::Retry.apply(app.world_mut(), root);
    until(&mut app, |world| {
        world.get::<Practice>(root).unwrap().runner.step > 0
    })
    .await;
    Command::Close.apply(app.world_mut(), root);
    until(&mut app, |world| {
        world.get::<Practice>(root).unwrap().cleanup.ready()
    })
    .await;
    Command::Discard.apply(app.world_mut(), root);
}

crate::laboratory_cases! {
    every_page_starts_directly_and_escape_keeps_progress_distinct,
    user_action_and_next_share_observation_without_duplicate_samples,
    async foundational_pages_confirm_native_effects_and_explain_direct_starts timeout 120,
    assisted_scopes_actions_focus_and_recovery_and_fails_open_on_ambiguity,
    close_and_component_removal_restore_interaction_and_owned_state,
    inactive_idle_and_closed_practice_do_not_repeat_input_scans,
    async record_changes_use_an_isolated_cell_and_confirm_entry_and_exit,
    async protein_query_controls_and_saves_stay_in_the_sample_cell,
    async work_controls_and_next_confirm_the_same_isolated_changes timeout 90,
    discard_removes_moved_samples_and_restores_source_bindings,
    async automation_controls_confirm_saved_clocks_rules_and_habits timeout 90,
    async community_views_keep_identity_pairing_and_roles_in_prepared_cells timeout 120,
    async local_tools_edit_owned_files_navigate_and_cancel_choices timeout 90,
    async sound_lessons_use_owned_libraries_and_release_playback_workers timeout 90,
    async visual_lessons_save_shader_and_custom_castles_and_restore_views timeout 90,
    laboratory_recovery_works_while_the_normal_tutorial_is_suspended,
    async native_control_identity_survives_renaming_and_recreation_and_blocks_missing_actions,
}
