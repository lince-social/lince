use super::*;
use crate::workspace::WorkspaceMember;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedExplorer {
    pub workspace: u64,
    castle: FileExplorer,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
}

impl SavedExplorer {
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
            .get_mut::<crate::canvas::CanvasItem>(entity)
            .unwrap()
            .size = size;
        self.placement.restore(world, entity);
        world
            .entity_mut(entity)
            .insert((self.tokens, crate::token_style::AppliedSize(size)));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedExplorer> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            &crate::canvas::CanvasItem,
            &FileExplorer,
        )>()
        .iter(world)
        .filter(|(entity, parent, _, _, _)| {
            parent.parent() == root
                && world
                    .get::<View>(*entity)
                    .is_some_and(|view| matches!(view.target, Target::Standalone))
        })
        .map(|(entity, _, member, item, castle)| SavedExplorer {
            workspace: member.0,
            castle: castle.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
        })
        .collect()
}
