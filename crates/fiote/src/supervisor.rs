use {
    crate::{
        binary::PiBinary,
        cub::{Cub, CubSpec, CubStatus},
    },
    std::{
        collections::BTreeMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
        },
    },
};

pub struct Supervisor {
    binary: PiBinary,
    cubs: Mutex<BTreeMap<String, Arc<Cub>>>,
    next: AtomicU64,
}

impl Supervisor {
    pub fn new(binary: PiBinary) -> Self {
        Self {
            binary,
            cubs: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
        }
    }

    pub fn binary(&self) -> &PiBinary {
        &self.binary
    }

    pub fn spawn(&self, spec: CubSpec) -> Result<Arc<Cub>, String> {
        let id = format!("cub-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let cub = Cub::spawn(id.clone(), &self.binary.path, spec)?;
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
