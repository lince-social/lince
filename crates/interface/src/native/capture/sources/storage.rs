use super::*;

#[derive(Default)]
pub(crate) enum Scope {
    #[default]
    Empty,
    One(Entity),
    Many(BTreeSet<Entity>),
}

impl Scope {
    pub(super) fn insert(&mut self, entity: Entity) {
        match self {
            Self::Empty => *self = Self::One(entity),
            Self::One(first) if *first != entity => {
                *self = Self::Many(BTreeSet::from([*first, entity]));
            }
            Self::Many(entities) => {
                entities.insert(entity);
            }
            Self::One(_) => {}
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Entity> {
        let (one, many) = match self {
            Self::Empty => (None, None),
            Self::One(entity) => (Some(entity), None),
            Self::Many(entities) => (None, Some(entities)),
        };
        one.into_iter()
            .chain(many.into_iter().flat_map(BTreeSet::iter))
    }

    pub(super) fn into_btree(self) -> BTreeSet<Entity> {
        match self {
            Self::Empty => BTreeSet::new(),
            Self::One(entity) => BTreeSet::from([entity]),
            Self::Many(entities) => entities,
        }
    }
}

pub(super) struct Path {
    entities: [Entity; MAX_DEPTH],
    len: usize,
}

impl Default for Path {
    fn default() -> Self {
        Self {
            entities: [Entity::PLACEHOLDER; MAX_DEPTH],
            len: 0,
        }
    }
}

impl Path {
    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn insert(&mut self, entity: Entity) -> bool {
        if self.len == self.entities.len() || self.entities[..self.len].contains(&entity) {
            return false;
        }
        self.entities[self.len] = entity;
        self.len += 1;
        true
    }

    pub(super) fn remove(&mut self, entity: &Entity) {
        if let Some(index) = self.entities[..self.len]
            .iter()
            .position(|current| current == entity)
        {
            self.len -= 1;
            self.entities[index] = self.entities[self.len];
        }
    }
}
