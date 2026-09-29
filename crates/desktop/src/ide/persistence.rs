use super::*;
use crate::workspace::WorkspaceMember;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedIde {
    pub workspace: u64,
    castle: Ide,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
    positions: BTreeMap<PathBuf, tabs::SavedPosition>,
}

impl SavedIde {
    pub(crate) fn valid(&self) -> bool {
        self.castle.valid()
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && self.placement.valid()
            && self.tokens.validate()
            && self.positions.len() <= 16
            && self.positions.iter().all(|(path, position)| {
                self.castle.paths.contains(path)
                    && position
                        .scroll
                        .iter()
                        .all(|value| value.is_finite() && *value >= 0.0)
            })
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
        world.get_mut::<View>(entity).unwrap().positions = self
            .positions
            .into_iter()
            .map(|(path, saved)| (path, tabs::Position { saved, ..default() }))
            .collect();
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedIde> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            &crate::canvas::CanvasItem,
            &Ide,
        )>()
        .iter(world)
        .filter(|(_, parent, _, _, _)| parent.parent() == root)
        .map(|(entity, _, member, item, castle)| SavedIde {
            workspace: member.0,
            castle: castle.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
            positions: tabs::saved(world, entity),
        })
        .collect()
}
