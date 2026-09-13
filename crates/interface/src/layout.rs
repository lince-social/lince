use bevy::prelude::*;
use serde::{Deserialize, Serialize};

mod content;
mod engine;
pub mod panel;
pub(crate) mod records;
mod solver;
pub(crate) mod viewport;

pub(crate) use engine::edited;

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
        if world.get::<crate::area::InfluenceArea>(current).is_some() {
            return crate::topology::position(world, current);
        }
        parent = world
            .get::<LayoutRuntime>(current)
            .and_then(|runtime| runtime.parent);
    }
    None
}

pub const LIMIT: f32 = 100_000.0;
pub const MAX_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sizing {
    Fixed,
    Fit,
    Fill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    Clip,
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Axis {
    pub sizing: Sizing,
    pub size: f32,
    pub min: f32,
    pub max: f32,
    pub overflow: Overflow,
}

impl Axis {
    pub fn fixed(size: f32) -> Self {
        Self {
            sizing: Sizing::Fixed,
            size,
            min: 1.0,
            max: LIMIT,
            overflow: Overflow::Clip,
        }
    }

    pub fn valid(self) -> bool {
        [self.size, self.min, self.max]
            .iter()
            .all(|n| n.is_finite())
            && self.min >= 1.0
            && self.max <= LIMIT
            && self.min <= self.max
            && (self.min..=self.max).contains(&self.size)
    }

    pub fn resolve(self, content: f32, available: f32) -> f32 {
        match self.sizing {
            Sizing::Fixed => self.size,
            Sizing::Fit => content,
            Sizing::Fill => available,
        }
        .clamp(self.min, self.max)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arrangement {
    Free,
    Row,
    Column,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rules {
    pub axes: [Axis; 2],
    pub arrangement: Arrangement,
    pub padding: f32,
    pub gap: f32,
    pub columns: u16,
    pub wrap: bool,
}

impl Rules {
    pub fn fixed(size: Vec2) -> Self {
        Self {
            axes: [Axis::fixed(size.x), Axis::fixed(size.y)],
            arrangement: Arrangement::Free,
            padding: 0.0,
            gap: 8.0,
            columns: 2,
            wrap: true,
        }
    }

    pub fn valid(self) -> bool {
        self.axes.iter().all(|axis| axis.valid())
            && self.padding.is_finite()
            && (0.0..=10_000.0).contains(&self.padding)
            && self.gap.is_finite()
            && (0.0..=10_000.0).contains(&self.gap)
            && (1..=128).contains(&self.columns)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutBox {
    pub id: [u8; 16],
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

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_systems(
                PostUpdate,
                engine::resolve
                    .in_set(ResolveLayout)
                    .after(crate::token_style::ApplyTokenStyles)
                    .after(crate::token_metrics::layout)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                (engine::finish, engine::finish_content).after(bevy::ui::UiSystems::PostLayout),
            )
            .add_systems(Update, panel::cleanup)
            .add_observer(engine::scroll);
    }
}

#[cfg(test)]
mod tests;
