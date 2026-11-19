use std::{ops::Deref, sync::Arc};

#[derive(Clone)]
pub(super) struct DraftData<T> {
    original: Arc<T>,
    current: Arc<T>,
}

impl<T> DraftData<T> {
    pub(super) fn new(value: T) -> Self {
        let original = Arc::new(value);
        Self {
            current: original.clone(),
            original,
        }
    }

    pub(super) fn original(&self) -> &T {
        &self.original
    }

    pub(super) fn original_snapshot(&self) -> Arc<T> {
        self.original.clone()
    }

    pub(super) fn snapshot(&self) -> Arc<T> {
        self.current.clone()
    }
}

impl<T: Eq> DraftData<T> {
    pub(super) fn changed(&self) -> bool {
        !Arc::ptr_eq(&self.current, &self.original) && self.current != self.original
    }
}

impl<T: Clone> DraftData<T> {
    pub(super) fn edit(&mut self) -> &mut T {
        edit(&mut self.current)
    }
}

impl<T> Deref for DraftData<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.current
    }
}

pub(super) fn edit<T: Clone>(snapshot: &mut Arc<T>) -> &mut T {
    if Arc::strong_count(snapshot) > 1 {
        nucleus::performance::add("extension-draft-data-copies", 1);
    }
    Arc::make_mut(snapshot)
}
