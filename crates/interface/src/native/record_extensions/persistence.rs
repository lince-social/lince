use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedExtensions {
    pub workspace: u64,
    settings: Settings,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::native::sand_placement::Placement,
    tokens: crate::native::tokens::TokenOverrides,
}

crate::native::sand_placement::scratch::saved!(SavedExtensions);

impl SavedExtensions {
    pub(crate) fn valid(&self) -> bool {
        self.settings.valid()
            && self.settings.mode != Mode::Column
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && self.placement.valid()
            && self.tokens.validate()
    }
    pub(crate) fn restore(self, world: &mut World, root: Entity) {
        let owner = spawn(
            world,
            root,
            self.workspace,
            DVec2::from_array(self.position),
            self.settings,
        );
        let size = Vec2::from_array(self.size);
        world
            .get_mut::<crate::native::canvas::CanvasItem>(owner)
            .unwrap()
            .size = size;
        self.placement.restore(world, owner);
        world
            .entity_mut(owner)
            .insert((self.tokens, crate::native::token_style::AppliedSize(size)));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedExtensions> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
            &crate::native::canvas::CanvasItem,
            &Settings,
        )>()
        .iter(world)
        .filter(|(_, parent, _, _, settings)| {
            parent.parent() == root && settings.mode != Mode::Column
        })
        .map(|(entity, _, workspace, item, settings)| SavedExtensions {
            workspace: workspace.0,
            settings: settings.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::native::sand_placement::Placement::capture(world, entity),
            tokens: crate::native::token_style::overrides(world, entity),
        })
        .collect()
}
