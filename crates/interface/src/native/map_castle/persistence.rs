use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedMap {
    pub workspace: u64,
    camera: Camera,
    records: Vec<String>,
    person: String,
    area: Option<String>,
    #[serde(default)]
    authority_node_id: Option<String>,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::native::sand_placement::Placement,
    tokens: crate::native::tokens::TokenOverrides,
}

crate::native::sand_placement::scratch::saved!(SavedMap);

impl SavedMap {
    pub(crate) fn valid(&self) -> bool {
        self.camera.valid()
            && self.records.len() <= 10_000
            && self
                .records
                .iter()
                .all(|record| nucleus::valid_uid(record, "r"))
            && (self.person.is_empty() || nucleus::valid_uid(&self.person, "r"))
            && self.authority_node_id.as_ref().is_none_or(|node| {
                node.len() == 64 && node.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            && self
                .area
                .as_ref()
                .is_none_or(|area| !area.is_empty() && area.len() <= 256)
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && self.placement.valid()
            && self.tokens.validate()
    }

    pub(crate) fn restore(self, world: &mut World, root: Entity) {
        let auto_center = !self.records.is_empty() || self.area.is_some();
        let entity = spawn(
            world,
            root,
            self.workspace,
            DVec2::from_array(self.position),
            self.camera,
            self.records,
            self.person,
        );
        world.get_mut::<MapSand>(entity).unwrap().area = self.area;
        world.get_mut::<MapSand>(entity).unwrap().authority_node_id = self.authority_node_id;
        world.get_mut::<MapSand>(entity).unwrap().auto_center = auto_center;
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

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedMap> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
            &crate::native::canvas::CanvasItem,
            &MapSand,
        )>()
        .iter(world)
        .filter(|(_, parent, _, _, _)| parent.parent() == root)
        .map(|(entity, _, member, item, sand)| SavedMap {
            workspace: member.0,
            camera: if sand.records.is_empty()
                && sand.area.is_none()
                && !sand.device_centered
                && sand.public_markers.is_empty()
            {
                sand.camera
            } else {
                Camera::default()
            },
            records: if sand.invitation.is_some() {
                Vec::new()
            } else {
                sand.records.clone()
            },
            person: sand.person.clone(),
            area: sand.area.clone(),
            authority_node_id: if sand.invitation.is_some() {
                None
            } else {
                sand.authority_node_id.clone()
            },
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::native::sand_placement::Placement::capture(world, entity),
            tokens: crate::native::token_style::overrides(world, entity),
        })
        .collect()
}
