use super::{
    controls::Controls,
    data::*,
    input,
    render::{Scene, View},
    run::Run,
    wake::Pacing,
};
use bevy::{
    input_focus::{InputFocus, tab_navigation::TabIndex},
    prelude::*,
    ui::RelativeCursorPosition,
};
use std::time::Instant;

#[derive(Resource)]
struct Runtime;

#[derive(Component)]
pub(super) struct Screen(pub Entity);

#[derive(Component)]
#[component(on_remove = remove_scene)]
struct SceneLease(Entity);

fn remove_scene(
    mut world: bevy::ecs::world::DeferredWorld,
    context: bevy::ecs::lifecycle::HookContext,
) {
    let Some(scene) = world.get::<SceneLease>(context.entity).map(|lease| lease.0) else {
        return;
    };
    world.commands().entity(scene).try_despawn();
}

#[derive(Component)]
pub(super) struct Game {
    pub screen: Entity,
    pub run: Run,
    pub input: Controls,
    pub restart: bool,
    view: View,
    last_tick: Instant,
    was_active: bool,
    was_shown: bool,
    repaint: u8,
}

pub struct KamaliePlugin;

impl Plugin for KamaliePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Runtime)
            .init_resource::<InputFocus>()
            .init_resource::<Pacing>()
            .add_observer(input::press_elsewhere)
            .add_systems(Update, update);
    }
}

fn seed() -> u64 {
    let mut bytes = [0; 8];
    if getrandom::fill(&mut bytes).is_ok() {
        return u64::from_le_bytes(bytes);
    }
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    clock
        ^ SEQUENCE
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .wrapping_mul(0x9e3779b97f4a7c15)
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
    world.entity_mut(sand).insert(crate::native::castle::Castle);
    if !world.contains_resource::<Runtime>() {
        super::render::preview(world, sand);
        return sand;
    }
    let catalog = match Catalog::load() {
        Ok(catalog) => catalog,
        Err(error) => {
            crate::native::notifications::report(world, "Kamalie", error);
            return sand;
        }
    };
    let run = Run::new(seed(), catalog);
    let view = match View::new(world, sand, &run) {
        Ok(view) => view,
        Err(error) => {
            crate::native::notifications::report(world, "Kamalie", error);
            return sand;
        }
    };
    let body = world
        .spawn((
            ChildOf(sand),
            Node {
                width: percent(100),
                height: percent(100),
                min_width: px(0),
                min_height: px(0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(FLOOR),
        ))
        .id();
    let screen = world
        .spawn((
            ChildOf(body),
            Screen(sand),
            ImageNode::new(view.image.clone()),
            TabIndex(0),
            RelativeCursorPosition::default(),
            Node {
                width: percent(100),
                height: percent(100),
                min_width: px(0),
                min_height: px(0),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    input::install(world, screen);
    world.entity_mut(sand).insert(SceneLease(view.scene));
    world.entity_mut(sand).insert(Game {
        screen,
        run,
        input: Controls::default(),
        view,
        restart: false,
        last_tick: Instant::now(),
        was_active: false,
        was_shown: false,
        repaint: 2,
    });
    sand
}

fn visible(world: &World, owner: Entity) -> bool {
    let root = world
        .get::<crate::native::sand::InBox>(owner)
        .map(|root| root.0);
    if let Some(root) = root
        && let Some(spaces) = world.get::<crate::native::workspace::Workspaces>(root)
        && world
            .get::<crate::native::workspace::WorkspaceMember>(owner)
            .is_some_and(|member| member.0 != spaces.active)
    {
        return false;
    }
    let mut ancestor = Some(owner);
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = ancestor else { return true };
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
            || world
                .get::<Visibility>(entity)
                .is_some_and(|visible| *visible == Visibility::Hidden)
            || world
                .get::<InheritedVisibility>(entity)
                .is_some_and(|visible| !visible.get())
        {
            return false;
        }
        ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

fn update(world: &mut World) {
    let orphaned: Vec<_> = world
        .query::<(Entity, &Scene)>()
        .iter(world)
        .filter(|(_, scene)| world.get::<Game>(scene.owner).is_none())
        .map(|(entity, _)| entity)
        .collect();
    for scene in orphaned {
        world.despawn(scene);
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Game>>()
        .iter(world)
        .collect();
    let focus = world.resource::<InputFocus>().get();
    let mut windows = world.query::<&Window>();
    let application_active = world
        .get_resource::<crate::host::input::InputActivity>()
        .is_none_or(|activity| activity.active())
        && (windows.iter(world).next().is_none()
            || windows.iter(world).any(|window| window.focused));
    let mouse_down = world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|buttons| buttons.pressed(super::controls::ACTIVATE_TOWARD_MOUSE));
    let now = Instant::now();
    let mut animate = false;
    for owner in owners {
        let shown = visible(world, owner);
        let Some(mut game) = world.entity_mut(owner).take::<Game>() else {
            continue;
        };
        let active = shown && application_active && focus == Some(game.screen);
        if game.restart {
            game.run = Run::new(seed(), game.run.catalog);
            game.view.restart();
            game.restart = false;
            game.repaint = 2;
        }
        let elapsed = now.duration_since(game.last_tick).as_secs_f32();
        game.last_tick = now;
        fit_viewport(world, game.screen);
        let cursor = world.get::<RelativeCursorPosition>(game.screen);
        game.input.pointer = cursor.and_then(|cursor| cursor.normalized).map(|point| {
            (point + Vec2::splat(0.5)) * Vec2::new(WIDTH as f32 * TILE, HEIGHT as f32 * TILE)
        });
        if !mouse_down || cursor.is_none_or(|cursor| !cursor.cursor_over()) {
            game.input.activating = false;
        }
        if !active {
            game.input.clear();
            game.run.suspend();
            if focus == Some(game.screen) {
                world.resource_mut::<InputFocus>().clear();
            }
        } else {
            game.run
                .advance(if game.was_active { elapsed } else { 0.0 }, &mut game.input);
        }
        let moving = active && !game.run.paused;
        if moving || shown && !game.was_shown {
            game.repaint = 2;
        }
        game.view.active(world, shown);
        game.view.synchronize(world, &game.run);
        if shown {
            game.repaint = game.repaint.saturating_sub(1);
        }
        game.was_active = active;
        game.was_shown = shown;
        animate |= moving || shown && game.repaint > 0;
        world.entity_mut(owner).insert(game);
    }
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    world.resource_mut::<Pacing>().set(animate, wake);
}

fn fit_viewport(world: &mut World, screen: Entity) {
    let Some(parent) = world.get::<ChildOf>(screen).map(ChildOf::parent) else {
        return;
    };
    let Some(computed) = world.get::<ComputedNode>(parent) else {
        return;
    };
    let bounds = computed.size() * computed.inverse_scale_factor();
    if bounds.x <= 0.0 || bounds.y <= 0.0 {
        return;
    }
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let size = Vec2::new(
        bounds.x.min(bounds.y * aspect),
        bounds.y.min(bounds.x / aspect),
    );
    if let Some(mut node) = world.get_mut::<Node>(screen) {
        let width = px(size.x);
        let height = px(size.y);
        if node.width != width || node.height != height {
            node.width = width;
            node.height = height;
        }
    }
}
