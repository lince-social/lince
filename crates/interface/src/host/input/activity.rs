use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug)]
pub struct InputActivity(Arc<AtomicU64>, Option<u64>);

impl Default for InputActivity {
    fn default() -> Self { Self(Arc::new(AtomicU64::new(1)), None) }
}

impl InputActivity {
    pub fn revision(&self) -> u64 { self.0.load(Ordering::Acquire) }

    pub fn active(&self) -> bool { self.revision() & 1 != 0 }

    pub fn begin_frame(&mut self) -> u64 {
        let revision = self.revision();
        self.1 = Some(revision);
        revision
    }

    pub fn frame_revision(&self) -> u64 { self.1.unwrap_or_else(|| self.revision()) }

    pub fn set_active(&self, active: bool) {
        let _ = self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
            if state == 0 || (state & 1 != 0) == active { return None; }
            Some(state.checked_add(2).map(|next| (next & !1) | u64::from(active)).unwrap_or(0))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inactive_or_returned_input_owner_cannot_reuse_an_earlier_activity_revision() {
        let mut activity = InputActivity::default();
        let original = activity.revision();
        assert_eq!(activity.begin_frame(), original);
        activity.set_active(false);
        assert!(!activity.active());
        let inactive = activity.revision();
        activity.set_active(false);
        assert_eq!(activity.revision(), inactive);
        activity.set_active(true);
        assert!(activity.active());
        assert_ne!(activity.revision(), original);
        assert_eq!(activity.frame_revision(), original);
        assert_eq!(activity.begin_frame(), activity.revision());
        let exhausted = InputActivity(Arc::new(AtomicU64::new(u64::MAX)), None);
        exhausted.set_active(false);
        exhausted.set_active(true);
        assert_eq!(exhausted.revision(), 0);
        assert!(!exhausted.active());
    }
}
