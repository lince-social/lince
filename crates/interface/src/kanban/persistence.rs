use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedKanban {
    pub workspace: u64,
    board: Kanban,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
}

impl SavedKanban {
    pub(crate) fn valid(&self) -> bool {
        self.board.valid()
            && DVec2::from_array(self.position).is_finite()
            && self
                .size
                .iter()
                .all(|size| size.is_finite() && (1.0..=100_000.0).contains(size))
            && self.placement.valid()
            && self.tokens.validate()
    }

    pub(crate) fn restore(self, world: &mut World, root: Entity) {
        let entity = super::restore(
            world,
            root,
            self.workspace,
            DVec2::from_array(self.position),
            self.board,
        );
        world.get_mut::<CanvasItem>(entity).unwrap().size = Vec2::from_array(self.size);
        self.placement.restore(world, entity);
        world.entity_mut(entity).insert((
            self.tokens,
            crate::token_style::AppliedSize(Vec2::from_array(self.size)),
        ));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedKanban> {
    world
        .query::<(Entity, &ChildOf, &WorkspaceMember, &CanvasItem, &Kanban)>()
        .iter(world)
        .filter(|(_, parent, _, _, _)| parent.parent() == root)
        .map(|(entity, _, member, item, board)| SavedKanban {
            workspace: member.0,
            board: board.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
        })
        .collect()
}
