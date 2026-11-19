use super::*;

#[derive(Default)]
pub(super) struct Pass {
    pub(super) surfaces: Vec<(Entity, Entity, Entity, Entity)>,
    pub(super) living: HashMap<Entity, Entity>,
    pub(super) retired: Vec<Entity>,
    pub(super) text_owners: Vec<Entity>,
    pub(super) used_textures: HashSet<model::Texture>,
    pub(super) lights: Lights,
}

impl Pass {
    pub(super) fn finish(&mut self, dormant: bool) {
        if dormant {
            *self = Self::default();
            return;
        }
        self.surfaces.clear();
        self.living.clear();
        self.retired.clear();
        self.text_owners.clear();
        self.used_textures.clear();
        self.lights.candidates.clear();
        self.lights.live.clear();
    }
}

#[derive(Default)]
pub(super) struct Lights {
    pub(super) candidates: Vec<(Entity, Entity, bool)>,
    pub(super) live: HashSet<Entity>,
}
