use super::*;
use crate::native::workspace::WorkspaceMember;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedDemeterCastle {
    pub workspace: u64,
    castle: DemeterCastle,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::native::sand_placement::Placement,
    tokens: crate::native::tokens::TokenOverrides,
}

crate::native::sand_placement::scratch::saved!(SavedDemeterCastle);

impl SavedDemeterCastle {
    pub(crate) fn valid(&self) -> bool {
        self.castle.valid()
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
            self.castle,
        );
        let size = Vec2::from_array(self.size);
        world
            .get_mut::<crate::native::canvas::CanvasItem>(entity)
            .unwrap()
            .size = size;
        self.placement.restore(world, entity);
        world
            .entity_mut(entity)
            .insert((self.tokens, crate::native::token_style::AppliedSize(size)));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedDemeterCastle> {
    let entities: Vec<_> = world
        .query::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            &crate::native::canvas::CanvasItem,
            &DemeterCastle,
        )>()
        .iter(world)
        .filter(|(e, p, _, _, _)| {
            p.parent() == root
                && world
                    .get::<crate::native::canvas_item::Ephemeral>(*e)
                    .is_none()
        })
        .map(|(e, _, member, item, state)| (e, member.0, item.position, item.size, state.clone()))
        .collect();
    entities
        .into_iter()
        .map(|(e, workspace, position, size, mut castle)| {
            let _ = editor::capture(world, e);
            if let Some(current) = world.get::<DemeterCastle>(e) {
                castle = current.clone();
            }
            SavedDemeterCastle {
                workspace,
                castle,
                position: position.to_array(),
                size: size.to_array(),
                placement: crate::native::sand_placement::Placement::capture(world, e),
                tokens: crate::native::token_style::overrides(world, e),
            }
        })
        .collect()
}
