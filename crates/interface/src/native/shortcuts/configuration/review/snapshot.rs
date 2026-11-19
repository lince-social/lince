use super::*;
use std::{any::Any, sync::Arc};

mod sealed {
    pub trait Source {}
    impl Source for super::Settings {}
    impl Source for super::LayoutSettings {}
    impl Source for crate::layout::desktop::OverviewSettings {}
    impl Source for crate::host::input::FocusPolicy {}
    impl Source for crate::host::input::GestureSettings {}
    impl Source for Vec<String> {}
}

pub(crate) trait Source:
    sealed::Source + Serialize + Clone + Eq + Send + Sync + 'static
{
}
impl<T> Source for T where T: sealed::Source + Serialize + Clone + Eq + Send + Sync + 'static {}

#[derive(Clone)]
pub(crate) struct Snapshot(Arc<Values>);

struct Values {
    base: Part,
    local: Part,
    active: Part,
}

#[derive(Clone)]
struct Part(Arc<dyn ValueSource>);

trait ValueSource: Send + Sync {
    fn encoded(&self) -> &Value;
    fn matches(&self, source: &dyn Any) -> bool;
}

struct TypedValue<T> {
    source: T,
    encoded: Value,
}

impl<T: Source> ValueSource for TypedValue<T> {
    fn encoded(&self) -> &Value {
        &self.encoded
    }
    fn matches(&self, source: &dyn Any) -> bool {
        source
            .downcast_ref::<T>()
            .is_some_and(|source| *source == self.source)
    }
}

impl PartialEq for Part {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.encoded() == other.0.encoded()
    }
}

impl Part {
    fn new<T: Source>(source: &T, previous: Option<&Self>) -> Result<Self, String> {
        if let Some(previous) = previous
            && previous.0.matches(source)
        {
            return Ok(previous.clone());
        }
        let encoded = serde_json::to_value(source).map_err(|error| error.to_string())?;
        engine::performance::add("configuration-review-values-prepared", 1);
        Ok(Self(Arc::new(TypedValue {
            source: source.clone(),
            encoded,
        })))
    }
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || self.0.base == other.0.base
                && self.0.local == other.0.local
                && self.0.active == other.0.active
    }
}

impl Snapshot {
    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub(super) fn base(&self) -> &Value {
        self.0.base.0.encoded()
    }
    pub(super) fn local(&self) -> &Value {
        self.0.local.0.encoded()
    }
    pub(super) fn active(&self) -> &Value {
        self.0.active.0.encoded()
    }

    pub(crate) fn new<T: Source>(
        base: &T,
        local: &T,
        active: &T,
        previous: Option<&Self>,
    ) -> Result<Self, String> {
        if let Some(previous) = previous
            && previous.0.base.0.matches(base)
            && previous.0.local.0.matches(local)
            && previous.0.active.0.matches(active)
        {
            return Ok(previous.clone());
        }
        let _preparation = engine::performance::Span::new("configuration-review-snapshot");
        let base = Part::new(base, previous.map(|snapshot| &snapshot.0.base))?;
        let local = Part::new(local, previous.map(|snapshot| &snapshot.0.local))?;
        let active = Part::new(active, previous.map(|snapshot| &snapshot.0.active))?;
        engine::performance::add("configuration-review-snapshots-prepared", 1);
        Ok(Self(Arc::new(Values {
            base,
            local,
            active,
        })))
    }
}
