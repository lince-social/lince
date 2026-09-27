use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedTransferCastle {
    pub workspace: u64,
    castle: TransferCastle,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
}

impl SavedTransferCastle {
    pub(crate) fn valid(&self) -> bool {
        self.castle.form.as_ref().is_none_or(Form::valid)
            && self.castle.search.len() <= 1024
            && model::FILTERS
                .iter()
                .any(|(key, _)| *key == self.castle.filter)
            && ["attention", "name", "status"].contains(&self.castle.sort.as_str())
            && self.castle.person.len() <= 1024
            && self.castle.selected.len() <= 1024
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && Vec2::from_array(self.size).max_element() <= 100_000.0
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

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedTransferCastle> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            &crate::canvas::CanvasItem,
            &TransferCastle,
        )>()
        .iter(world)
        .filter(|(_, parent, _, _, _)| parent.parent() == root)
        .map(|(entity, _, member, item, castle)| SavedTransferCastle {
            workspace: member.0,
            castle: {
                let mut saved = castle.clone();
                if saved.form.as_ref().is_some_and(|form| form.step.is_none()) {
                    saved.form = None;
                }
                saved
            },
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
        })
        .collect()
}
