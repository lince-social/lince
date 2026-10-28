use lince_interface::host::{
    OutputId,
    display::DisplaySnapshot,
    magnification::{Head, Outcome, Reload, Request, Response, Snapshot, valid},
    mapping,
};
use std::{collections::BTreeMap, io};

#[derive(Default)]
pub(crate) struct State {
    pub snapshot: Snapshot,
    configured: BTreeMap<String, f64>,
}

impl State {
    pub fn configure(&mut self, settings: BTreeMap<String, f64>) -> bool {
        if self.configured == settings { return false; }
        self.configured = settings;
        for head in &mut self.snapshot.heads { head.requested = self.configured.get(&head.name).copied().unwrap_or(1.0); }
        self.advance();
        true
    }

    pub fn reconcile(&mut self, display: &DisplaySnapshot, available: bool) -> bool {
        let heads = display.heads.iter().filter(|head| display.outputs.iter().any(|output| output.id == head.id)).map(|head| {
            Head { output: head.id, name: head.name.clone(), requested: self.configured.get(&head.name).copied().unwrap_or(1.0),
                effective: self.snapshot.heads.iter().find(|previous| previous.output == head.id && previous.name == head.name).and_then(|previous| previous.effective) }
        }).collect::<Vec<_>>();
        if self.snapshot.heads == heads && self.snapshot.available == available { return false; }
        self.snapshot.heads = heads;
        self.snapshot.available = available;
        self.advance();
        true
    }

    pub fn request(&mut self, request: Request, unavailable: Option<&str>, save: impl FnOnce(&str, f64, f64) -> io::Result<Option<String>>) -> bool {
        if request.id <= self.snapshot.last_request { return false; }
        self.snapshot.last_request = request.id;
        let result = (|| {
            if let Some(reason) = unavailable { return Err(reason.to_owned()); }
            if !self.snapshot.available { return Err("Display magnification requires Park's local output configuration.".to_owned()); }
            if request.revision != self.snapshot.revision { return Err("Outputs or magnification changed; reload the preserved draft before saving.".to_owned()); }
            if !valid(request.factor) { return Err("Magnification must be a finite factor between 1× and 4×.".to_owned()); }
            let head = self.snapshot.head(request.output).ok_or("The original output is no longer available.")?;
            let revision = self.snapshot.revision.checked_add(1).ok_or("Magnification revisions are exhausted; restart Park.")?;
            let warning = save(&head.name, request.factor, head.requested).map_err(|error| error.to_string())?;
            Ok((head.name.clone(), revision, warning))
        })();
        let (outcome, changed) = match result {
            Ok((name, revision, warning)) => {
                self.configured.insert(name, request.factor);
                self.snapshot.heads.iter_mut().find(|head| head.output == request.output).unwrap().requested = request.factor;
                self.snapshot.revision = revision;
                (Outcome::Saved { warning }, true)
            }
            Err(reason) => (Outcome::Failed(reason), false),
        };
        self.snapshot.response = Some(Response { request: request.id, outcome });
        changed
    }

    pub fn reload(&mut self, request: Reload, unavailable: Option<&str>, read: impl FnOnce() -> io::Result<BTreeMap<String, f64>>) {
        if request.id <= self.snapshot.last_request { return; }
        self.snapshot.last_request = request.id;
        let result = match unavailable {
            Some(reason) => Err(reason.to_owned()),
            None => read().map_err(|error| error.to_string()),
        };
        let outcome = match result {
            Ok(settings) => { self.configure(settings); Outcome::Loaded }
            Err(reason) => Outcome::Failed(reason),
        };
        self.snapshot.response = Some(Response { request: request.id, outcome });
    }

    pub fn presented(&mut self, output: OutputId, mappings: &mapping::Snapshot) -> bool {
        let Some(mapping) = mappings.get(output) else { return false; };
        let Some(head) = self.snapshot.heads.iter_mut().find(|head| head.output == output) else { return false; };
        let factor = mapping.magnification();
        if head.effective == Some(factor) { return false; }
        head.effective = Some(factor);
        true
    }

    fn advance(&mut self) { self.snapshot.revision = self.snapshot.revision.checked_add(1).expect("magnification settings revision"); }
}

#[cfg(test)]
mod tests;
