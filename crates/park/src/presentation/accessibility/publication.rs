use super::*;
use lince_interface::host::OutputId;
use std::collections::BTreeSet;

mod composition;

pub(super) struct Publication {
    pub epoch: u64,
    pub serial: u64,
    pub update: TreeUpdate,
    pub targets: BTreeMap<NodeId, Target>,
    pub spans: BTreeMap<NodeId, Span>,
    pub bounds: BTreeMap<OutputId, lince_interface::host::Rectangle>,
}

struct State {
    publication: Arc<Publication>,
    remaining: Mutex<BTreeSet<OutputId>>,
    shared: Arc<Shared>,
}

#[derive(Clone)]
pub(crate) struct Receipt {
    state: Arc<State>,
}

impl Receipt {
    pub(super) fn new(publication: Arc<Publication>, outputs: impl IntoIterator<Item = OutputId>, shared: Arc<Shared>) -> Self {
        let state = Arc::new(State { publication: publication.clone(), remaining: Mutex::new(outputs.into_iter().collect()), shared });
        Self { state }
    }

    pub(crate) fn presented(&self, output: OutputId) {
        let Ok(mut remaining) = self.state.remaining.lock() else { return; };
        if !remaining.remove(&output) { return; }
        drop(remaining);
        let shared = &self.state.shared;
        if !shared.requested.get() { return; }
        let Ok(mut catalog) = shared.catalog.lock() else { return; };
        let publication = &self.state.publication;
        if catalog.epoch != publication.epoch || catalog.serial.get(&output).is_some_and(|serial| *serial >= publication.serial) { return; }
        catalog.serial.insert(output, publication.serial);
        catalog.presented.insert(output, publication.clone());
        drop(catalog);
        shared.wake.ring();
    }

    fn complete(&self) -> bool { self.state.remaining.lock().is_ok_and(|remaining| remaining.is_empty()) }
}

pub(super) fn retained(world: &World, views: &[crate::native_input::projection::View], output: OutputId, publication: &Arc<Publication>) -> Option<Arc<Publication>> {
    let visible = |target: &Target| target.output != output || views.iter().any(|view| view.output == output && view.image == target.image)
        && native::accessibility::owner_context(world, target.entity, false).is_some_and(|current| target.context.same_owner(&current));
    if publication.targets.values().all(visible) { return None; }
    let mut targets = publication.targets.clone();
    targets.retain(|_, target| visible(target));
    Some(Arc::new(Publication {
        epoch: publication.epoch, serial: publication.serial, update: publication.update.clone(), targets,
        spans: publication.spans.clone(), bounds: publication.bounds.clone(),
    }))
}

impl Bridge {
    pub(in crate::presentation) fn receipt(&self) -> Option<Receipt> { self.pending.clone() }

    pub(super) fn flush(&mut self, world: &World, views: &[crate::native_input::projection::View]) {
        if !self.active || !self.shared.requested.get() { return; }
        let Ok(mut catalog) = self.shared.catalog.lock() else { return; };
        let epoch = catalog.epoch;
        let presented = std::mem::take(&mut catalog.presented);
        drop(catalog);
        let valid = |output: OutputId, publication: &Publication| {
            publication.epoch == epoch && views.iter().any(|view| view.output == output)
        };
        let previous_count = self.exports.len();
        self.exports.retain(|output, publication| valid(*output, publication));
        let mut changed = self.exports.len() != previous_count;
        if changed { self.shared.refresh.store(true, Ordering::Release); }
        for (output, publication) in presented {
            if valid(output, &publication) {
                changed |= self.exports.get(&output).is_none_or(|previous| !Arc::ptr_eq(previous, &publication));
                self.exports.insert(output, publication);
            } else {
                changed |= self.exports.remove(&output).is_some();
                self.shared.refresh.store(true, Ordering::Release);
            }
        }
        for (output, publication) in &mut self.exports {
            if let Some(current) = retained(world, views, *output, publication) {
                *publication = current;
                changed = true;
                self.shared.refresh.store(true, Ordering::Release);
            }
        }
        if self.pending.as_ref().is_some_and(Receipt::complete) { self.pending = None; }
        if !changed { return; }
        if self.exports.is_empty() && self.last.is_none() { return; }
        let (update, targets, spans) = composition::merge(&self.exports);
        #[cfg(test)]
        eprintln!("Accessibility presented focus {:?}, selections={:?}", update.focus,
            update.nodes.iter().filter_map(|(id, node)| node.text_selection().map(|selection| (*id, *selection))).collect::<Vec<_>>());
        #[cfg(test)]
        {
            let nodes = update.nodes.iter().map(|(id, node)| (*id, node)).collect::<BTreeMap<_, _>>();
            let mut pending = vec![ROOT];
            let mut reachable = BTreeSet::new();
            while let Some(id) = pending.pop() {
                if reachable.insert(id) && let Some(node) = nodes.get(&id) { pending.extend_from_slice(node.children()); }
            }
            assert_eq!(reachable, nodes.keys().copied().collect::<BTreeSet<_>>(), "Presented accessibility nodes must form one connected tree: {:?}",
                update.nodes.iter().take(32).map(|(id, node)| (*id, node.role(), node.children().to_vec())).collect::<Vec<_>>());
        }
        let Ok(mut catalog) = self.shared.catalog.lock() else { return; };
        if catalog.epoch != epoch { return; }
        catalog.targets = targets;
        catalog.spans = spans;
        drop(catalog);
        if self.needs_tree || self.last.as_ref() != Some(&update) {
            if let Some((_, root)) = update.nodes.iter().find(|(id, _)| *id == ROOT)
                && let Some(bounds) = root.bounds()
            {
                let screen = accesskit::Rect::new(0.0, 0.0, bounds.x1.max(0.0), bounds.y1.max(0.0));
                self.adapter.set_root_window_bounds(screen, screen);
            }
            self.adapter.update_if_active(|| update.clone());
            self.last = Some(update);
            self.needs_tree = false;
        }
    }
}
