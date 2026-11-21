// non-lean-yet
// Scope: static NULL, enum Data, struct WindowValues, struct Projection.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 105, 108, 117,
//   222.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   117.

use super::*;
use crate::host::performance::{self, Count, Stage};
use crate::native::area::RecordProperties;
use serde_json::Value;

static NULL: Value = Value::Null;

#[derive(Clone, Copy)]
enum Data<'a> {
    Baseline(&'a Arc<Value>),
    Reply(&'a Value),
}

impl<'a> Data<'a> {
    fn value(self) -> &'a Value {
        match self {
            Self::Baseline(value) => value.as_ref(),
            Self::Reply(value) => value,
        }
    }
}

#[derive(Clone, Copy)]
struct WindowValues<'a> {
    title: &'a Value,
    app_id: &'a Value,
}

#[derive(Clone, Copy)]
struct Projection<'a> {
    data: Data<'a>,
    fields: Option<[(&'static str, &'a Value); 2]>,
}

impl<'a> Projection<'a> {
    fn new(data: Data<'a>) -> Self {
        Self { data, fields: None }
    }

    fn with_window(mut self, title_key: &'static str, values: WindowValues<'a>) -> Self {
        let data = self.data.value();
        assert!(
            data.is_object() || data.is_null(),
            "window metadata requires object or null Record properties"
        );
        let fields = [(title_key, values.title), ("app_id", values.app_id)];
        self.fields = (!fields
            .iter()
            .all(|(key, value)| data.get(*key) == Some(*value)))
        .then_some(fields);
        self
    }

    fn field(self, key: &str) -> Option<&'a Value> {
        if let Some(fields) = self.fields {
            for (field, value) in fields {
                if key == field {
                    return Some(value);
                }
            }
        }
        self.data.value().get(key)
    }

    fn window(self) -> WindowValues<'a> {
        WindowValues {
            title: self
                .field("window_title")
                .or_else(|| self.field("head"))
                .unwrap_or(&NULL),
            app_id: self.field("app_id").unwrap_or(&NULL),
        }
    }

    fn matches(self, current: &RecordProperties) -> bool {
        performance::count(Count::AttachmentPropertyComparisons);
        let Some(fields) = self.fields else {
            return matches!(self.data, Data::Baseline(data) if Arc::ptr_eq(data, &current.0))
                || self.data.value() == current.0.as_ref();
        };
        let Some(current) = current.0.as_object() else {
            return false;
        };
        let baseline = self.data.value().as_object();
        let mut expected_len = baseline.map_or(0, |data| data.len());
        for (key, value) in fields {
            if current.get(key) != Some(value) {
                return false;
            }
            if baseline.is_none_or(|data| !data.contains_key(key)) {
                expected_len += 1;
            }
        }
        current.len() == expected_len
            && baseline.is_none_or(|data| {
                data.iter().all(|(key, value)| {
                    fields.iter().any(|(field, _)| key.as_str() == *field)
                        || current.get(key) == Some(value)
                })
            })
    }

    fn materialize(self) -> RecordProperties {
        let mut data = match self.data {
            Data::Baseline(data) => data.clone(),
            Data::Reply(data) => {
                performance::count(Count::AttachmentPropertyCopies);
                Arc::new(data.clone())
            }
        };
        if let Some(fields) = self.fields {
            if matches!(self.data, Data::Baseline(_)) {
                performance::count(Count::AttachmentPropertyCopies);
            }
            let data = Arc::make_mut(&mut data);
            for (key, value) in fields {
                data[key] = value.clone();
            }
        }
        RecordProperties(data)
    }
}

enum Publication {
    Unchanged,
    Remove,
    Replace(RecordProperties),
}

impl Publication {
    fn prepare(current: Option<&RecordProperties>, desired: Option<Projection<'_>>) -> Self {
        match desired {
            Some(desired) if current.is_some_and(|current| desired.matches(current)) => {
                Self::Unchanged
            }
            Some(desired) => Self::Replace(desired.materialize()),
            None if current.is_some() => Self::Remove,
            None => Self::Unchanged,
        }
    }

    fn publish(self, entity: &mut bevy::ecs::world::EntityWorldMut<'_>) {
        match self {
            Self::Unchanged => {}
            Self::Remove => {
                entity.remove::<RecordProperties>();
                performance::count(Count::AttachmentPropertiesRemoved);
            }
            Self::Replace(data) => {
                entity.insert(data);
                performance::count(Count::AttachmentPropertiesWritten);
            }
        }
    }
}

fn baseline<'a>(world: &'a World, item: Entity, stream: &'a Stream) -> Option<Projection<'a>> {
    let baseline = &stream.baseline.as_ref()?.0;
    let mut desired = Projection::new(Data::Baseline(baseline));
    if world
        .get::<crate::native::layout::desktop::ApplicationWindow>(item)
        .is_some()
        && let Some(current) = world.get::<RecordProperties>(item)
    {
        desired = desired.with_window("head", Projection::new(Data::Baseline(&current.0)).window());
    }
    Some(desired)
}

fn restore_inner(world: &mut World, item: Entity, stream: &Stream) {
    if world.get_entity(item).is_err() {
        return;
    }
    let publication = Publication::prepare(
        world.get::<RecordProperties>(item),
        baseline(world, item, stream),
    );
    let bound = world.get::<RecordBinding>(item).is_some();
    let mut entity = world.entity_mut(item);
    if bound {
        entity.remove::<RecordBinding>();
        performance::count(Count::AttachmentBindingsChanged);
    }
    publication.publish(&mut entity);
}

pub(super) fn restore(world: &mut World, item: Entity, stream: &Stream) {
    let _properties = performance::Span::new(Stage::AttachmentProperties);
    restore_inner(world, item, stream);
}

pub(super) fn apply(world: &mut World, item: Entity, stream: &mut Stream, row: Option<&Value>) {
    let _properties = performance::Span::new(Stage::AttachmentProperties);
    let Some(row) = row.filter(|row| row.is_object() && row["uid"] == stream.uid) else {
        restore_inner(world, item, stream);
        if stream.message != "Record unavailable." {
            stream.message.clear();
            stream.message.push_str("Record unavailable.");
        }
        return;
    };
    let mut desired = Projection::new(Data::Reply(row));
    if world
        .get::<crate::native::layout::desktop::ApplicationWindow>(item)
        .is_some()
        && let Some(restored) = baseline(world, item, stream)
    {
        desired = desired.with_window("window_title", restored.window());
    }
    let publication = Publication::prepare(world.get::<RecordProperties>(item), Some(desired));
    let changed_binding = world.get::<RecordBinding>(item).is_none_or(|binding| {
        binding.area != item || binding.uid != stream.uid || binding.source != Source::Local
    });
    if !changed_binding && matches!(&publication, Publication::Unchanged) {
        performance::count(Count::AttachmentRepliesWithoutPropertyOrBindingWrites);
    }
    let mut entity = world.entity_mut(item);
    publication.publish(&mut entity);
    if changed_binding {
        entity.insert(RecordBinding {
            area: item,
            uid: stream.uid.clone(),
            source: Source::Local,
        });
        performance::count(Count::AttachmentBindingsChanged);
    }
    stream.message.clear();
}
