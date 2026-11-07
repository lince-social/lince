use std::sync::Arc;

#[cfg(feature = "handoff")]
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallbackIdentity {
    type_id: std::any::TypeId,
    pub type_name: &'static str,
    pub capture_size: usize,
}

#[cfg(feature = "handoff")]
pub(crate) trait HandoffCallback {
    fn handoff_identity(&self) -> CallbackIdentity;
}

#[cfg(feature = "handoff")]
impl<T: 'static> HandoffCallback for T {
    fn handoff_identity(&self) -> CallbackIdentity {
        CallbackIdentity {
            type_id: std::any::TypeId::of::<T>(),
            type_name: std::any::type_name::<T>(),
            capture_size: std::mem::size_of::<T>(),
        }
    }
}

crate::utils::ids::id_gen!(hooks_id);

/// Unique hook identifier used to unregister commit/descruction hooks
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HookId(Arc<InnerId>);

pub(crate) struct Hook<T: ?Sized> {
    pub id: HookId,
    pub cb: Arc<T>,
}

impl<T: ?Sized> std::fmt::Debug for Hook<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hook")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<T: ?Sized> Clone for Hook<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            cb: self.cb.clone(),
        }
    }
}

impl<T: ?Sized> Hook<T> {
    pub fn new(cb: Arc<T>) -> Self {
        Self {
            id: HookId(Arc::new(InnerId::new())),
            cb,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct InnerId(usize);

impl InnerId {
    fn new() -> Self {
        Self(hooks_id::next())
    }
}

impl Drop for InnerId {
    fn drop(&mut self) {
        hooks_id::remove(self.0);
    }
}
