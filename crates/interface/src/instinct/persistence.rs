use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedInstinct {
    pub workspace: u64,
    instinct: Instinct,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
}

impl SavedInstinct {
    pub(crate) fn valid(&self) -> bool {
        self.instinct.valid()
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
            self.instinct,
        );
        let size = Vec2::from_array(self.size);
        world.get_mut::<CanvasItem>(entity).unwrap().size = size;
        self.placement.restore(world, entity);
        world
            .entity_mut(entity)
            .insert((self.tokens, crate::token_style::AppliedSize(size)));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedInstinct> {
    world
        .query::<(Entity, &ChildOf, &WorkspaceMember, &CanvasItem, &Instinct)>()
        .iter(world)
        .filter(|(_, parent, _, _, _)| parent.parent() == root)
        .map(|(entity, _, member, item, instinct)| SavedInstinct {
            workspace: member.0,
            instinct: instinct.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
        })
        .collect()
}
