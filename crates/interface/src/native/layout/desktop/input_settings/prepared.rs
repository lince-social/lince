use bevy::{prelude::*, text::EditableText};

struct Input {
    entity: Entity,
    value: Option<Value>,
}

struct Value {
    text: String,
    composing: bool,
    pending: bool,
}

impl Input {
    fn matches(&self, world: &World, entity: Entity) -> bool {
        self.entity == entity
            && match (self.value.as_ref(), world.get::<EditableText>(entity)) {
                (None, None) => true,
                (Some(previous), Some(current)) => {
                    current.value() == previous.text.as_str()
                        && current.is_composing() == previous.composing
                        && (!current.pending_edits.is_empty() || current.pending_paste.is_some())
                            == previous.pending
                }
                _ => false,
            }
    }

    fn capture(world: &World, entity: Entity) -> Self {
        let value = world.get::<EditableText>(entity).map(|text| {
            engine::performance::add("settings-field-values-prepared", 1);
            Value {
                text: text.value().to_string(),
                composing: text.is_composing(),
                pending: !text.pending_edits.is_empty() || text.pending_paste.is_some(),
            }
        });
        Self { entity, value }
    }

    fn refresh(&mut self, world: &World, entity: Entity) {
        if let (Some(previous), Some(current)) =
            (self.value.as_mut(), world.get::<EditableText>(entity))
        {
            if current.value() != previous.text.as_str() {
                previous.text = current.value().to_string();
                engine::performance::add("settings-field-values-prepared", 1);
            }
            previous.composing = current.is_composing();
            previous.pending = !current.pending_edits.is_empty() || current.pending_paste.is_some();
            self.entity = entity;
        } else {
            *self = Self::capture(world, entity);
        }
    }
}

pub(crate) struct Prepared<K, T> {
    source: Option<K>,
    inputs: Vec<Input>,
    result: Option<Result<T, String>>,
}

impl<K, T> Default for Prepared<K, T> {
    fn default() -> Self {
        Self {
            source: None,
            inputs: Vec::new(),
            result: None,
        }
    }
}

impl<K, T> Prepared<K, T> {
    pub(crate) fn get_matching(
        &self,
        world: &World,
        fields: impl IntoIterator<Item = Entity>,
        matches: impl FnOnce(&K) -> bool,
    ) -> Option<&Result<T, String>> {
        if !self.source.as_ref().is_some_and(matches) {
            return None;
        }
        let mut fields = fields.into_iter();
        if self.inputs.iter().all(|input| {
            fields
                .next()
                .is_some_and(|entity| input.matches(world, entity))
        }) && fields.next().is_none()
        {
            self.result.as_ref()
        } else {
            None
        }
    }

    pub(crate) fn prepare_matching(
        &mut self,
        world: &World,
        fields: impl IntoIterator<Item = Entity> + Clone,
        matches: impl FnOnce(&K) -> bool,
        source: impl FnOnce() -> K,
        build: impl FnOnce() -> Result<T, String>,
    ) -> &Result<T, String> {
        if self.get_matching(world, fields.clone(), matches).is_none() {
            let _preparation = engine::performance::Span::new("settings-field-preparation");
            let result = build();
            let mut count = 0;
            for (index, entity) in fields.into_iter().enumerate() {
                if let Some(input) = self.inputs.get_mut(index) {
                    if !input.matches(world, entity) {
                        input.refresh(world, entity);
                    }
                } else {
                    self.inputs.push(Input::capture(world, entity));
                }
                count += 1;
            }
            self.inputs.truncate(count);
            self.source = Some(source());
            self.result = Some(result);
            engine::performance::add("settings-candidates-prepared", 1);
        }
        self.result.as_ref().unwrap()
    }
}

impl<K: PartialEq + Clone, T> Prepared<K, T> {
    pub(crate) fn get(
        &self,
        world: &World,
        fields: impl IntoIterator<Item = Entity>,
        source: &K,
    ) -> Option<&Result<T, String>> {
        self.get_matching(world, fields, |previous| previous == source)
    }

    pub(crate) fn prepare(
        &mut self,
        world: &World,
        fields: impl IntoIterator<Item = Entity> + Clone,
        source: &K,
        build: impl FnOnce() -> Result<T, String>,
    ) -> &Result<T, String> {
        self.prepare_matching(
            world,
            fields,
            |previous| previous == source,
            || source.clone(),
            build,
        )
    }
}
