//! Tracks running and completed Fiote child processes by ID.

use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
};

use crate::{
    binary::FioteBinary,
    credential::ProviderCredential,
    cub::{Cub, CubSpec, CubStatus},
};

/// Starts child processes and keeps their handles available until they are forgotten.
/// Registry access assumes that the internal mutex is healthy.
pub struct Supervisor {
    binary: FioteBinary,
    cubs: Mutex<BTreeMap<String, Arc<Cub>>>,
    next: AtomicU64,
}

impl fmt::Debug for Supervisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Supervisor")
            .field("binary", &self.binary)
            .finish_non_exhaustive()
    }
}

impl Supervisor {
    /// Creates a supervisor using the selected Fiote binary.
    pub fn new(binary: FioteBinary) -> Self {
        Self {
            binary,
            cubs: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
        }
    }

    /// The binary used for child processes.
    pub fn binary(&self) -> &FioteBinary {
        &self.binary
    }

    /// Starts and registers a child using the inherited environment.
    ///
    /// # Errors
    /// Returns a diagnostic if the child or its pipes cannot be opened.
    ///
    /// # Panics
    /// Requires a Tokio runtime and an unpoisoned registry.
    pub fn spawn(&self, spec: CubSpec) -> Result<Arc<Cub>, String> {
        self.spawn_with_credential(spec, None)
    }

    /// Starts a child with an optional credential scoped to its environment.
    ///
    /// # Errors
    /// Returns a diagnostic if the child or its pipes cannot be opened.
    ///
    /// # Panics
    /// Requires a Tokio runtime and an unpoisoned registry.
    pub fn spawn_with_credential(
        &self,
        spec: CubSpec,
        credential: Option<ProviderCredential>,
    ) -> Result<Arc<Cub>, String> {
        let id = format!("cub-{}", self.next.fetch_add(1, Relaxed));
        let cub = Cub::spawn(id.clone(), &self.binary.path, spec, credential)?;
        self.registry().insert(id, Arc::clone(&cub));
        Ok(cub)
    }

    /// Gets a registered child, including a finished child still in the registry.
    ///
    /// # Panics
    /// Panics if the registry mutex was poisoned.
    pub fn get(&self, id: &str) -> Option<Arc<Cub>> {
        self.registry().get(id).map(Arc::clone)
    }

    /// Reports all registered children in ID order.
    ///
    /// # Panics
    /// Panics if the registry mutex was poisoned.
    pub fn list(&self) -> Vec<CubStatus> {
        self.registry().values().map(|cub| cub.status()).collect()
    }

    /// Requests shutdown of a registered child.
    ///
    /// # Errors
    /// Returns a diagnostic if the child ID is absent.
    ///
    /// # Panics
    /// Panics if the registry or child's shutdown mutex was poisoned.
    pub fn stop(&self, id: &str) -> Result<(), String> {
        let cub = self.get(id).ok_or_else(|| format!("no cub named `{id}`"))?;
        cub.stop();
        Ok(())
    }

    /// Removes finished children and returns their IDs in registry order.
    ///
    /// # Panics
    /// Panics if the registry mutex was poisoned.
    pub fn forget_finished(&self) -> Vec<String> {
        self.registry()
            .extract_if(.., |_, cub| !cub.is_running())
            .map(|(id, _)| id)
            .collect()
    }

    fn registry(&self) -> MutexGuard<'_, BTreeMap<String, Arc<Cub>>> {
        self.cubs.lock().expect("Fiote child registry was poisoned")
    }
}
