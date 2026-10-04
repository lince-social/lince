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
        .spawn((crate::container::BoxRoot, Workspaces::default()))
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
}

#[cfg_attr(test, test)]
fn user_action_and_next_share_observation_without_duplicate_samples() {
    for manual in [false, true] {
        let (mut app, root) = fixture();
        start(app.world_mut(), root, "sands", Mode::Free);
        Command::Skip.apply(app.world_mut(), root);
        if manual {
            Perform(Operation::PlaceSand).apply(app.world_mut(), root);
        }
        Command::Next.apply(app.world_mut(), root);
        app.update();
        assert_eq!(app.world().get::<Practice>(root).unwrap().runner.step, 2);
        let count = app.world_mut().query::<&Owned>().iter(app.world()).count();
        assert_eq!(count, 2);
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

struct Change;
impl Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        world.entity_mut(target).insert(Name::new("changed"));
    }
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
    for _ in 0..500 {
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
    panic!("Practice did not confirm its result: {text:?}");
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
            .contains(&sample_uid)
    );
}

crate::laboratory_cases! {
    every_page_starts_directly_and_escape_keeps_progress_distinct,
    user_action_and_next_share_observation_without_duplicate_samples,
    assisted_scopes_actions_focus_and_recovery_and_fails_open_on_ambiguity,
    async record_changes_use_an_isolated_cell_and_confirm_entry_and_exit,
}
