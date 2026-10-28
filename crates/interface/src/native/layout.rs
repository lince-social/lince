use bevy::prelude::*;
use serde::{Deserialize, Serialize};

mod content;
pub mod desktop;
mod engine;
pub mod panel;
pub(crate) mod records;
mod solver;
pub(crate) mod viewport;

pub(crate) use engine::edited;

#[cfg(test)]
pub(crate) fn resolve_for_test(world: &mut World) { engine::resolve(world); }

pub(crate) fn linked(world: &World, entity: Entity) -> bool {
    let Some(layout) = world.get::<LayoutBox>(entity) else {
        return false;
    };
    layout.parent.is_some()
        || world
            .get::<ChildOf>(entity)
            .and_then(|parent| world.get::<Children>(parent.parent()))
            .is_some_and(|children| {
                children.iter().any(|child| {
                    world
                        .get::<LayoutBox>(child)
                        .is_some_and(|child| child.parent == Some(layout.id))
                })
            })
}
pub use engine::{LayoutRuntime, attach, configure, detach};

pub(crate) fn membership(world: &World, entity: Entity) -> Option<bevy::math::DVec3> {
    world.get::<LayoutBox>(entity)?.parent?;
    let mut parent = world
        .get::<LayoutRuntime>(entity)
        .and_then(|runtime| runtime.parent);
    for _ in 0..MAX_DEPTH {
        let current = parent?;
        if world
            .get::<crate::native::area::InfluenceArea>(current)
            .is_some()
        {
            return crate::native::topology::position(world, current);
        }
        parent = world
            .get::<LayoutRuntime>(current)
            .and_then(|runtime| runtime.parent);
    }
    None
}

pub use crate::layout::{Arrangement, Axis, LIMIT, Overflow, Rules, Sizing};
pub const MAX_DEPTH: usize = 32;

#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutBox {
    pub id: [u8; 16],
    #[serde(default)]
    pub capture_excluded: bool,
    pub parent: Option<[u8; 16]>,
    pub offset: [f32; 2],
    pub order: i32,
    pub rules: Rules,
}

impl LayoutBox {
    pub fn new(size: Vec2) -> Self {
        let mut id = [0; 16];
        getrandom::fill(&mut id).expect("layout identity");
        Self {
            id,
            capture_excluded: false,
            parent: None,
            offset: [0.0; 2],
            order: 0,
            rules: Rules::fixed(size),
        }
    }

    pub fn valid(self) -> bool {
        self.id != [0; 16]
            && self.parent != Some(self.id)
            && self.parent != Some([0; 16])
            && self
                .offset
                .iter()
                .all(|n| n.is_finite() && (0.0..=LIMIT).contains(n))
            && self.rules.valid()
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolveLayout;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolveFollow;

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        crate::native::capture::install(app);
        desktop::windowed_fullscreen::install(app);
        desktop::compact::install(app);
        desktop::scratch::install(app);
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_systems(
                PreUpdate,
                desktop::manipulation::pointer::input
                    .after(crate::native::shortcuts::CaptureInput)
                    .before(crate::native::shortcuts::desktop_input)
                    .before(bevy::picking::PickingSystems::ProcessInput),
            )
            .add_systems(
                PostUpdate,
                engine::resolve
                    .in_set(ResolveLayout)
                    .after(crate::native::token_style::ApplyTokenStyles)
                    .after(crate::native::token_metrics::layout)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                (
                    engine::finish,
                    engine::finish_content.after(crate::native::protein_area::MeasureRecords),
                )
                    .after(bevy::ui::UiSystems::PostLayout),
            )
            .add_systems(
                PostUpdate,
                (desktop::focus::resolve, desktop::publish)
                    .chain()
                    .in_set(ResolveFollow)
                    .after(ResolveLayout)
                    .before(bevy::camera::CameraUpdateSystems),
            )
            .add_systems(
                PostUpdate,
                desktop::manipulation::update
                    .after(ResolveLayout)
                    .before(ResolveFollow)
                    .before(crate::native::topology::presentation::synchronize),
            )
            .add_systems(Update, (panel::cleanup, desktop::create_controls))
            .add_systems(
                PostUpdate,
                desktop::monitors::identify
                    .after(ResolveFollow)
                    .after(crate::native::topology::presentation::synchronize)
                    .before(bevy::camera::CameraUpdateSystems)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                desktop::monitors::update
                    .after(crate::native::actions::ApplyActions)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_systems(
                PostUpdate,
                desktop::overview::update
                    .after(ResolveFollow)
                    .before(crate::native::topology::assets::update)
                    .before(crate::native::topology::presentation::synchronize)
                    .before(bevy::camera::CameraUpdateSystems)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                desktop::label_controls
                    .after(crate::native::actions::ApplyActions)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_observer(desktop::clicked)
            .add_observer(desktop::focus::moved)
            .add_systems(
                PostUpdate,
                desktop::recent::update
                    .after(ResolveFollow)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_observer(engine::scroll);
    }
}

#[cfg(test)]
mod tests;
