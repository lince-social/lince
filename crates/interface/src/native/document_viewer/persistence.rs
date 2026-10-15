use super::*;
use crate::native::workspace::WorkspaceMember;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedDocumentViewer {
    pub workspace: u64,
    castle: DocumentViewer,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::native::sand_placement::Placement,
    tokens: crate::native::tokens::TokenOverrides,
}

impl SavedDocumentViewer {
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

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedDocumentViewer> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            &crate::native::canvas::CanvasItem,
            &DocumentViewer,
        )>()
        .iter(world)
        .filter(|(entity, parent, _, _, _)| {
            parent.parent() == root
                && world
                    .get::<crate::native::external_drop::Preview>(*entity)
                    .is_none()
        })
        .map(|(entity, _, member, item, castle)| SavedDocumentViewer {
            workspace: member.0,
            castle: castle.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::native::sand_placement::Placement::capture(world, entity),
            tokens: crate::native::token_style::overrides(world, entity),
        })
        .collect()
}
