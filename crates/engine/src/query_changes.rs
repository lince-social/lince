// non-lean-yet
// Scope: struct QueryChanges, struct GeneralQueryChanges.
// BeautifulRust (baseline lines):
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 30.
// - [rustc probe: missing_debug_implementations] type does not implement `std::fmt::Debug`;
//   consider adding `#[derive(Debug)]` or a manual implementation. Sites 15.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   4, 5, 6, 10, 15, 30.

use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryChanges {
    pub general: u64,
    pub projection: u64,
}

impl QueryChanges {
    pub fn revision(&self) -> u64 {
        self.general.wrapping_add(self.projection)
    }
}

pub struct GeneralQueryChanges {
    changes: watch::Receiver<QueryChanges>,
    observed: Option<u64>,
}

impl GeneralQueryChanges {
    pub(crate) fn new(changes: watch::Receiver<QueryChanges>) -> Self {
        let current = changes.borrow().general;
        let observed = match changes.has_changed() {
            Ok(false) => Some(current),
            _ => None,
        };
        Self { changes, observed }
    }

    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        loop {
            self.changes.changed().await?;
            let current = self.changes.borrow_and_update().general;
            let previous = self.observed.replace(current);
            if previous != Some(current) {
                crate::performance::add("general-query-changes-delivered", 1);
                return Ok(());
            }
            crate::performance::add("general-query-projection-only-skips", 1);
        }
    }
}
