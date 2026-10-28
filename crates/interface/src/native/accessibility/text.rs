use super::*;
use accesskit::{NodeId, TreeUpdate};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::text::{EditableText, FontCx, LayoutCx};

pub fn text_nodes(world: &mut World, entity: Entity, node: &mut accesskit::Node, update: &mut TreeUpdate, next_id: impl FnMut() -> NodeId) -> bool {
    if protected_text_input(world, entity) { node.clear_text_selection(); return false; }
    if !world.contains_resource::<FontCx>() || !world.contains_resource::<LayoutCx>() || world.get::<EditableText>(entity).is_none() { return false; }
    world.resource_scope(|world, mut fonts: Mut<FontCx>| {
        world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
            let mut input = world.get_mut::<EditableText>(entity).unwrap();
            input.bypass_change_detection().editor.driver(&mut fonts.context, &mut layout.0).accessibility(update, node, next_id, 0.0, 0.0, |_, _| {}).is_some()
        })
    })
}
