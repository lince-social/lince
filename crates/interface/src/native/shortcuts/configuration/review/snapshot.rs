//! Typed, shared JSON snapshots of settings under review.

use super::{LayoutSettings, Settings};
use serde::Serialize;
use serde_json::Value;
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
        source.downcast_ref::<T>() == Some(&self.source)
    }
}

impl PartialEq for Part {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.encoded() == other.0.encoded()
    }
}

impl Part {
    fn matching(&self, source: &dyn Any) -> Option<&Self> {
        self.0.matches(source).then_some(self)
    }

    fn prepare<T: Source>(source: &T, cached: Option<&Self>) -> Result<Self, String> {
        if let Some(cached) = cached {
            return Ok(cached.clone());
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
        let cached = previous.map_or([None; 3], |snapshot| {
            [
                snapshot.0.base.matching(base),
                snapshot.0.local.matching(local),
                snapshot.0.active.matching(active),
            ]
        });
        if let Some(previous) = previous
            && cached.iter().all(Option::is_some)
        {
            return Ok(previous.clone());
        }
        let _preparation = engine::performance::Span::new("configuration-review-snapshot");
        let [cached_base, cached_local, cached_active] = cached;
        let base = Part::prepare(base, cached_base)?;
        let local = Part::prepare(local, cached_local)?;
        let active = Part::prepare(active, cached_active)?;
        engine::performance::add("configuration-review-snapshots-prepared", 1);
        Ok(Self(Arc::new(Values {
            base,
            local,
            active,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{host::input::FocusPolicy, layout::desktop::OverviewSettings};

    #[test]
    fn unchanged_snapshots_and_unchanged_parts_retain_their_owners() {
        let base = vec![String::from("base")];
        let local = vec![String::from("local")];
        let active = vec![String::from("active")];
        let previous = Snapshot::new(&base, &local, &active, None).expect("initial snapshot");
        let same = Snapshot::new(&base, &local, &active, Some(&previous)).expect("same values");
        assert!(previous.same(&same));

        let changed_local = vec![String::from("edited")];
        let changed = Snapshot::new(&base, &changed_local, &active, Some(&previous))
            .expect("changed local draft");
        assert!(!previous.same(&changed));
        assert!(std::ptr::eq(previous.base(), changed.base()));
        assert!(!std::ptr::eq(previous.local(), changed.local()));
        assert!(std::ptr::eq(previous.active(), changed.active()));
        assert_eq!(previous.local(), &serde_json::json!(["local"]));
        assert_eq!(changed.local(), &serde_json::json!(["edited"]));

        let separate = Snapshot::new(&base, &local, &active, None).expect("independent snapshot");
        assert!(previous == separate);
        assert!(!previous.same(&separate));
    }

    #[test]
    fn changing_the_settings_type_rebuilds_all_parts() {
        let overview = OverviewSettings::default();
        let previous = Snapshot::new(&overview, &overview, &overview, None).expect("overview");
        let focus = FocusPolicy::default();
        let updated = Snapshot::new(&focus, &focus, &focus, Some(&previous)).expect("focus");
        assert!(!previous.same(&updated));
        assert!(!std::ptr::eq(previous.base(), updated.base()));
        assert!(!std::ptr::eq(previous.local(), updated.local()));
        assert!(!std::ptr::eq(previous.active(), updated.active()));
        assert_eq!(
            updated.base(),
            &serde_json::to_value(focus).expect("focus JSON")
        );
    }
}
