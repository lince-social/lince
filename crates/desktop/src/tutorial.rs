pub(crate) mod highlight;

use bevy::prelude::*;

pub use highlight::TutorialHighlight;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TutorialField {
    Query(Entity, String),
    Quantity(Entity, bool),
}

pub struct TutorialPlugin;

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            highlight::position
                .after(bevy::ui::UiSystems::PostLayout)
                .run_if(highlight::active),
        );
    }
}

#[derive(Clone)]
pub struct Start;

impl crate::actions::Action for Start {
    fn practice_intent(&self) -> crate::actions::PracticeIntent {
        crate::actions::PracticeIntent::Recovery
    }

    fn apply(&self, world: &mut World, owner: Entity) {
        crate::instinct::practice::StartPage {
            slug: "area-record-actions".into(),
            mode: lince_interface::practice::Mode::Free,
        }
        .apply(world, owner);
    }
}
