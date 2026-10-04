use bevy::{
    diagnostic::FrameCount,
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::gpu_readback::{Readback, ReadbackComplete},
    text::{EditableText, TextEdit},
    winit::WinitSettings,
};
use lince_desktop::{
    actions::ActionButton,
    area::RecordProperties,
    canvas::{CanvasItem, CanvasView},
    castle::{Castle, PresentCastles, StartupStatus},
    container::BoxRoot,
    icons::Tooltip,
    instinct::{Instinct, SeedInstinct},
    karma_castle::KarmaCastle,
    protein_area::RecordBinding,
    topology::presentation::Surface,
};
use std::{
    collections::{HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::Arc,
};

#[derive(Component, Default)]
struct Samples(HashSet<u64>);

#[derive(Resource, Default)]
struct Trial {
    start: Option<u32>,
    card: Option<Entity>,
    karma: Option<Entity>,
    baseline: Vec2,
    reads: usize,
    watchers: Vec<(Entity, Entity)>,
}

fn descendant(world: &World, mut entity: Entity, owner: Entity) -> bool {
    while let Some(parent) = world.get::<ChildOf>(entity) {
        entity = parent.parent();
        if entity == owner {
            return true;
        }
    }
    false
}

fn record_input(world: &mut World, owner: Entity, property: &str) -> Entity {
    world
        .query::<(Entity, &EditableText, &RecordBinding)>()
        .iter(world)
        .find(|(entity, _, _)| {
            if !descendant(world, *entity, owner) {
                return false;
            }
            let mut ancestor = *entity;
            while let Some(parent) = world.get::<ChildOf>(ancestor) {
                ancestor = parent.parent();
                if world
                    .get::<Tooltip>(ancestor)
                    .is_some_and(|tip| tip.0.eq_ignore_ascii_case(property))
                {
                    return true;
                }
                if ancestor == owner {
                    break;
                }
            }
            false
        })
        .map(|(entity, _, _)| entity)
        .expect("Record field must be editable")
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame.is_multiple_of(100) {
        let pending = world
            .query::<(Entity, &StartupStatus)>()
            .iter(world)
            .filter(|(_, status)| !status.0.is_empty())
            .count();
        println!("Frame {frame}: {pending} castles preparing");
    }
    if frame == 20 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        lince_desktop::kanban::spawn(world, root, 1, DVec2::ZERO).unwrap();
        let karma = lince_desktop::karma_castle::spawn(
            world,
            root,
            1,
            DVec2::new(0.0, 1000.0),
            KarmaCastle {
                draft: Some(Default::default()),
                ..default()
            },
        );
        world.resource_mut::<Trial>().karma = Some(karma);
        let mut view = world.get_mut::<CanvasView>(root).unwrap();
        view.zoom = 0.3;
        view.center.y = 200.0;
    }
    if world.resource::<Trial>().start.is_none() {
        let card = world
            .query::<(Entity, &RecordProperties, &Surface)>()
            .iter(world)
            .next()
            .map(|(entity, _, _)| entity);
        if let Some(card) = card
            && frame >= 60
            && world
                .get::<InheritedVisibility>(card)
                .is_some_and(|visible| visible.get())
            && world
                .get::<StartupStatus>(card)
                .is_some_and(|status| status.0.is_empty())
        {
            let owners: Vec<_> = world
                .query_filtered::<Entity, Or<(With<Instinct>, With<KarmaCastle>)>>()
                .iter(world)
                .chain(std::iter::once(card))
                .collect();
            for owner in owners {
                let surface = world.get::<Surface>(owner).unwrap();
                let image = world
                    .resource::<Assets<StandardMaterial>>()
                    .get(&surface.material)
                    .unwrap()
                    .base_color_texture
                    .clone()
                    .unwrap();
                let watcher = world
                    .spawn((Readback::texture(image), Samples::default()))
                    .observe(
                        |event: On<ReadbackComplete>,
                         mut trial: ResMut<Trial>,
                         mut samples: Query<&mut Samples>| {
                            let colors: HashSet<_> = event
                                .data
                                .chunks_exact(4)
                                .step_by(16)
                                .filter(|pixel| pixel[3] != 0)
                                .take(100_000)
                                .collect();
                            assert!(
                                colors.len() > 8,
                                "A presented castle lost its rendered content"
                            );
                            let mut hash = DefaultHasher::new();
                            event.data.hash(&mut hash);
                            samples
                                .get_mut(event.entity)
                                .unwrap()
                                .0
                                .insert(hash.finish());
                            trial.reads += 1;
                        },
                    )
                    .id();
                world
                    .resource_mut::<Trial>()
                    .watchers
                    .push((owner, watcher));
            }
            let baseline = world.get::<CanvasItem>(card).unwrap().size;
            let mut trial = world.resource_mut::<Trial>();
            trial.start = Some(frame);
            trial.card = Some(card);
            trial.baseline = baseline;
        }
        assert!(frame < 3000, "Castles did not become ready");
        return;
    }
    let elapsed = frame - world.resource::<Trial>().start.unwrap();
    let card = world.resource::<Trial>().card.unwrap();
    if elapsed < 80 && elapsed.is_multiple_of(4) {
        let step = elapsed / 4;
        let input = record_input(world, card, if step < 10 { "slug" } else { "title" });
        world
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Navigated);
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .pending_edits
            .push(if step % 10 < 6 {
                TextEdit::Insert("a".into())
            } else {
                TextEdit::Backspace
            });
        let karma = world.resource::<Trial>().karma.unwrap();
        let input = world
            .query::<(Entity, &EditableText, &bevy::a11y::AccessibilityNode)>()
            .iter(world)
            .find(|(entity, _, node)| {
                node.label() == Some("Condition") && descendant(world, *entity, karma)
            })
            .map(|(entity, _, _)| entity)
            .unwrap();
        world
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Navigated);
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .pending_edits
            .push(TextEdit::Insert("a".into()));
        let page = if step % 2 == 0 {
            "Areas of Influence"
        } else {
            "Interface"
        };
        let action = world
            .query::<(&Tooltip, &ActionButton)>()
            .iter(world)
            .find(|(tip, action)| tip.0 == page && world.get::<Instinct>(action.target).is_some())
            .map(|(_, action)| (action.target, action.actions.clone()))
            .unwrap();
        action.1.run(world, action.0);
    }
    let watchers = world.resource::<Trial>().watchers.clone();
    for (owner, watcher) in watchers {
        let surface = world.get::<Surface>(owner).unwrap();
        let image = world
            .resource::<Assets<StandardMaterial>>()
            .get(&surface.material)
            .unwrap()
            .base_color_texture
            .clone()
            .unwrap();
        world.entity_mut(watcher).insert(Readback::texture(image));
    }
    if elapsed >= 160 {
        let data = &world.get::<RecordProperties>(card).unwrap().0;
        let slug = data["slug"].as_str().unwrap().to_owned();
        let head = data["head"].as_str().unwrap().to_owned();
        assert!(!slug.is_empty());
        assert_ne!(head, "Title", "Title typing must reach the backend");
        let slug_input = record_input(world, card, "slug");
        let head_input = record_input(world, card, "title");
        assert_eq!(
            world
                .get::<EditableText>(slug_input)
                .unwrap()
                .value()
                .to_string(),
            slug
        );
        assert_eq!(
            world
                .get::<EditableText>(head_input)
                .unwrap()
                .value()
                .to_string(),
            head
        );
        assert!(
            world.resource::<Trial>().reads > 100,
            "GPU frames must be checked throughout the transitions"
        );
        for (owner, watcher) in &world.resource::<Trial>().watchers {
            assert!(
                world.get::<Samples>(*watcher).unwrap().0.len() > 2,
                "Castle {owner} stopped updating its visible content"
            );
            assert!(
                world.get::<StartupStatus>(*owner).unwrap().0.is_empty(),
                "Castle {owner} never finished preparing its replacement content"
            );
        }
        println!(
            "Castle transitions and Kanban typing passed with {} GPU readbacks",
            world.resource::<Trial>().reads
        );
        world.write_message(AppExit::Success);
    }
}

fn monitor(world: &mut World) {
    let trial = world.resource::<Trial>();
    let Some(card) = trial.card else { return };
    let baseline = trial.baseline;
    let size = world.get::<CanvasItem>(card).unwrap().size;
    assert!(
        (size - baseline).length() < 0.1,
        "Typing or saving changed the Kanban card size: {baseline:?} -> {size:?}"
    );
    for (owner, _) in &trial.watchers {
        assert!(
            world.get::<InheritedVisibility>(*owner).unwrap().get(),
            "A presented castle became hidden"
        );
        assert!(world.get::<Castle>(*owner).is_some());
    }
}

#[tokio::main]
async fn main() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    for name in ["task", "todo"] {
        engine
            .act(
                engine::actions::Action::CreateConcept {
                    lingua: "g_local".into(),
                    name: name.into(),
                    parents: Vec::new(),
                },
                None,
            )
            .await
            .unwrap();
    }
    engine
        .act(
            engine::actions::Action::CreateRecordWithTags {
                head: "Title".into(),
                body: "Body".into(),
                quantity: -1.0,
                tags: vec!["task".into(), "todo".into()],
            },
            None,
        )
        .await
        .unwrap();
    lince_desktop::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(lince_desktop::cell_bridge::CellBridgePlugin)
        .insert_resource(lince_desktop::wake::WakeSignal::new(|| {}))
        .insert_resource(lince_desktop::app::CellHandle(cell::CellRuntime {
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            speech: None,
            information: None,
        }))
        .init_resource::<Trial>()
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1600.0, 1000.0);
                commands.spawn((BoxRoot, SeedInstinct));
            },
        )
        .add_systems(Update, exercise)
        .add_systems(Last, monitor.after(PresentCastles))
        .run();
}
