use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedCalendar {
    pub workspace: u64,
    calendar: Calendar,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
}

impl SavedCalendar {
    pub(crate) fn valid(&self) -> bool {
        self.calendar.valid()
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && self.placement.valid()
            && self.tokens.validate()
    }

    pub(crate) fn restore(self, world: &mut World, root: Entity) {
        let entity = spawn(
            world,
            root,
            self.workspace,
            DVec2::from_array(self.position),
            self.calendar,
        );
        let size = Vec2::from_array(self.size);
        world
            .get_mut::<crate::canvas::CanvasItem>(entity)
            .unwrap()
            .size = size;
        self.placement.restore(world, entity);
        world
            .entity_mut(entity)
            .insert((self.tokens, crate::token_style::AppliedSize(size)));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedCalendar> {
    world
        .query_filtered::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            &crate::canvas::CanvasItem,
            &CalendarSand,
        ), Without<Picker>>()
        .iter(world)
        .filter(|(_, parent, _, _, _)| parent.parent() == root)
        .map(|(entity, _, member, item, sand)| SavedCalendar {
            workspace: member.0,
            calendar: sand.0.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
        })
        .collect()
}
