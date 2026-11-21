// non-lean-yet
// Scope: struct Supervisor.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::missing_panics_doc] State the panic conditions for exported functions. Sites
//   39, 53, 61, 76.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 45, 81.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   48, 56, 64, 77.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 35, 39, 70.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 16.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 16, 23,
//   31, 35, 39, 53, 61, 70, 76.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites spawn: String
//   (35), spawn_with_credential: String (39), stop: String (70).

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::{
    binary::FioteBinary,
    credential::ProviderCredential,
    cub::{Cub, CubSpec, CubStatus},
};

pub struct Supervisor {
    binary: FioteBinary,
    cubs: Mutex<BTreeMap<String, Arc<Cub>>>,
    next: AtomicU64,
}

impl Supervisor {
    pub fn new(binary: FioteBinary) -> Self {
        Self {
            binary,
            cubs: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
        }
    }

    pub fn binary(&self) -> &FioteBinary {
        &self.binary
    }

    pub fn spawn(&self, spec: CubSpec) -> Result<Arc<Cub>, String> {
        self.spawn_with_credential(spec, None)
    }

    pub fn spawn_with_credential(
        &self,
        spec: CubSpec,
        credential: Option<ProviderCredential>,
    ) -> Result<Arc<Cub>, String> {
        let id = format!("cub-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let cub = Cub::spawn(id.clone(), &self.binary.path, spec, credential)?;
        self.cubs
            .lock()
            .expect("fiote supervisor mutex")
            .insert(id, Arc::clone(&cub));
        Ok(cub)
    }

    pub fn get(&self, id: &str) -> Option<Arc<Cub>> {
        self.cubs
            .lock()
            .expect("fiote supervisor mutex")
            .get(id)
            .cloned()
    }

    pub fn list(&self) -> Vec<CubStatus> {
        self.cubs
            .lock()
            .expect("fiote supervisor mutex")
            .values()
            .map(|cub| cub.status())
            .collect()
    }

    pub fn stop(&self, id: &str) -> Result<(), String> {
        let cub = self.get(id).ok_or_else(|| format!("no cub named `{id}`"))?;
        cub.stop();
        Ok(())
    }

    pub fn forget_finished(&self) -> Vec<String> {
        let mut cubs = self.cubs.lock().expect("fiote supervisor mutex");
        let finished: Vec<String> = cubs
            .iter()
            .filter(|(_, cub)| !cub.is_running())
            .map(|(id, _)| id.clone())
            .collect();
        for id in &finished {
            cubs.remove(id);
        }
        finished
    }
}
