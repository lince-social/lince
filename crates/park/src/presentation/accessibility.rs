use super::*;
use accesskit::{Action, ActionData, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, NodeId, TreeId, TreeUpdate};
use native::accessibility::Context;
use std::{collections::BTreeMap, sync::{Mutex, atomic::{AtomicBool, AtomicU64, Ordering}, mpsc::{Receiver, SyncSender, sync_channel}}};

mod tree;
mod publication;
#[cfg(all(test, feature = "accessibility-gate"))]
pub(crate) mod platform;
pub(crate) use publication::Receipt;
use publication::Publication;

const ROOT: NodeId = NodeId(1);
const MAX_REQUESTS: usize = 128;
const MAX_NODES: usize = 262_144;
static NEXT_NODE: AtomicU64 = AtomicU64::new(2);

fn node_id() -> NodeId {
    NodeId(NEXT_NODE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value.checked_add(1)).expect("accessibility node identity"))
}

#[derive(Clone, PartialEq)]
struct Target {
    entity: Entity,
    context: Context,
    output: lince_interface::host::OutputId,
    image: bevy::asset::AssetId<Image>,
    actions: Vec<Action>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Span {
    entity: Entity,
    source: accesskit::TextPosition,
    length: usize,
}

#[derive(Default)]
struct Catalog {
    epoch: u64,
    targets: BTreeMap<NodeId, Target>,
    spans: BTreeMap<NodeId, Span>,
    presented: BTreeMap<lince_interface::host::OutputId, Arc<Publication>>,
    serial: BTreeMap<lince_interface::host::OutputId, u64>,
}

struct Request {
    epoch: u64,
    target: Target,
    action: ActionRequest,
    selection: Option<accesskit::TextSelection>,
}

impl Request {
    fn native_action(&self) -> ActionRequest {
        let mut action = self.action.clone();
        if let Some(selection) = self.selection { action.data = Some(ActionData::SetTextSelection(selection)); }
        action
    }
}

fn translate_selection(spans: &BTreeMap<NodeId, Span>, entity: Entity, selection: accesskit::TextSelection) -> Option<accesskit::TextSelection> {
    let position = |position: accesskit::TextPosition| {
        let span = spans.get(&position.node).filter(|span| span.entity == entity && position.character_index <= span.length)?;
        Some(accesskit::TextPosition { node: span.source.node, character_index: span.source.character_index.checked_add(position.character_index)? })
    };
    Some(accesskit::TextSelection { anchor: position(selection.anchor)?, focus: position(selection.focus)? })
}

struct Shared {
    catalog: Mutex<Catalog>,
    sender: SyncSender<Request>,
    requested: bevy::a11y::AccessibilityRequested,
    refresh: AtomicBool,
    wake: WakeSignal,
    progress: Option<crate::runtime::InputProgress>,
    pending_input: AtomicU64,
}

impl Shared {
    fn retire(&self) {
        if let Ok(mut catalog) = self.catalog.lock() {
            catalog.epoch = catalog.epoch.checked_add(1).expect("accessibility interaction epoch");
            catalog.targets.clear();
            catalog.spans.clear();
            catalog.presented.clear();
            catalog.serial.clear();
            self.pending_input.store(0, Ordering::Release);
        }
    }
}

#[derive(Clone)]
struct Handler(Arc<Shared>);

impl ActivationHandler for Handler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.requested.set(true);
        self.0.refresh.store(true, Ordering::Release);
        self.0.wake.ring();
        None
    }
}

impl DeactivationHandler for Handler {
    fn deactivate_accessibility(&mut self) {
        self.0.requested.set(false);
        self.0.retire();
        self.0.refresh.store(true, Ordering::Release);
        self.0.wake.ring();
    }
}

impl ActionHandler for Handler {
    fn do_action(&mut self, action: ActionRequest) {
        if action.target_tree != TreeId::ROOT || !self.0.requested.get()
            || matches!(&action.data, Some(ActionData::Value(value)) if value.len() > native::accessibility::MAX_ACTION_BYTES)
        { return; }
        let Ok(catalog) = self.0.catalog.lock() else { return; };
        let Some(target) = catalog.targets.get(&action.target_node) else { return; };
        if !target.actions.contains(&action.action) { return; }
        let selection = if let Some(ActionData::SetTextSelection(selection)) = &action.data {
            let Some(selection) = translate_selection(&catalog.spans, target.entity, *selection) else { return; };
            Some(selection)
        } else { None };
        let request = Request { epoch: catalog.epoch, target: target.clone(), action, selection };
        #[cfg(test)]
        eprintln!("Accessibility queued {:?} for {:?} on {:?}, selection={:?}", request.action.action, request.target.entity, request.target.output, request.selection);
        let fence = self.0.progress.as_ref().map_or(0, crate::runtime::InputProgress::watermark);
        let queued = self.0.sender.try_send(request).is_ok();
        if queued { self.0.pending_input.fetch_max(fence, Ordering::Release); }
        drop(catalog);
        if queued { self.0.wake.ring(); }
    }
}

pub(super) struct Bridge {
    adapter: accesskit_unix::Adapter,
    shared: Arc<Shared>,
    receiver: Receiver<Request>,
    identities: BTreeMap<Entity, NodeId>,
    text_identities: BTreeMap<(NodeId, usize), NodeId>,
    last: Option<TreeUpdate>,
    pending: Option<Receipt>,
    source: Option<Arc<Publication>>,
    exports: BTreeMap<lince_interface::host::OutputId, Arc<Publication>>,
    needs_tree: bool,
    serial: u64,
    active: bool,
    focused: bool,
}

impl Bridge {
    pub(super) fn new(world: &mut World) -> Self {
        world.init_resource::<bevy::a11y::AccessibilityRequested>();
        let (sender, receiver) = sync_channel(MAX_REQUESTS);
        let shared = Arc::new(Shared { catalog: Mutex::new(Catalog::default()), sender,
            requested: world.resource::<bevy::a11y::AccessibilityRequested>().clone(), refresh: AtomicBool::new(true), wake: world.resource::<WakeSignal>().clone(),
            progress: world.get_resource::<crate::runtime::InputProgress>().cloned(), pending_input: AtomicU64::new(0) });
        let handler = Handler(shared.clone());
        Self { adapter: accesskit_unix::Adapter::new(handler.clone(), handler.clone(), handler), shared, receiver,
            identities: BTreeMap::new(), text_identities: BTreeMap::new(), last: None, pending: None, source: None, exports: BTreeMap::new(), needs_tree: true, serial: 0, active: true, focused: false }
    }

    pub(super) fn invalidate(&mut self) {
        self.shared.retire();
        self.pending = None;
        self.source = None;
        self.exports.clear();
        self.identities.clear();
        self.text_identities.clear();
        self.needs_tree = true;
        while self.receiver.try_recv().is_ok() {}
        self.adapter.update_if_active(|| {
            let mut root = accesskit::Node::new(accesskit::Role::Window);
            root.set_label("Lince Park");
            TreeUpdate { nodes: vec![(ROOT, root)], tree: Some(accesskit::Tree::new(ROOT)), tree_id: TreeId::ROOT, focus: ROOT }
        });
        self.last = None;
        self.shared.refresh.store(true, Ordering::Release);
    }

    pub(super) fn focus(&mut self, active: bool) {
        if self.focused != active { self.adapter.update_window_focus_state(active); self.focused = active; }
    }

    fn requests(&mut self, world: &World, views: &[crate::native_input::projection::View]) -> Vec<Request> {
        self.flush(world, views);
        let mut requests = Vec::new();
        let Ok(catalog) = self.shared.catalog.lock() else { return requests; };
        let fence = self.shared.pending_input.load(Ordering::Acquire);
        if self.shared.progress.as_ref().is_some_and(|progress| !progress.reached(fence)) { return requests; }
        self.shared.pending_input.store(0, Ordering::Release);
        for _ in 0..MAX_REQUESTS {
            let Ok(request) = self.receiver.try_recv() else { break; };
            let presented = views.iter().any(|view| view.output == request.target.output && view.image == request.target.image);
            let current = current(&catalog, &request);
            if !self.active || !self.shared.requested.get() || !presented || !current {
                #[cfg(test)]
                eprintln!("Accessibility discarded {:?} for {:?}: active={}, requested={}, presented={}, current={}",
                    request.action.action, request.target.entity, self.active, self.shared.requested.get(), presented, current);
                continue;
            }
            requests.push(request);
        }
        requests
    }

    pub(super) fn publish(&mut self, world: &mut World, views: &[crate::native_input::projection::View], geometry: Option<&lince_interface::host::GeometrySnapshot>) {
        self.flush(world, views);
        let force = self.shared.refresh.swap(false, Ordering::AcqRel);
        self.needs_tree |= force;
        if !self.shared.requested.get() {
            if force { self.invalidate(); self.identities.clear(); self.shared.refresh.store(false, Ordering::Release); }
            return;
        }
        let visible = self.active && !world.get_resource::<lince_interface::host::control::Status>().is_some_and(|status| status.available && (status.locked || !status.active));
        if !visible || views.is_empty() {
            if self.last.is_some() || self.pending.is_some() { self.invalidate(); }
            return;
        }
        let Ok(catalog) = self.shared.catalog.lock() else { return; };
        let epoch = catalog.epoch;
        drop(catalog);
        if self.source.as_ref().is_some_and(|source| source.epoch != epoch) {
            self.identities.clear();
            self.text_identities.clear();
            self.source = None;
            self.pending = None;
        }
        let (update, targets, spans) = tree::build_views(world, views, geometry, &mut self.identities, &mut self.text_identities);
        if !force {
            if self.source.as_ref().is_some_and(|source| source.update == update && source.targets == targets && source.spans == spans) { return; }
        }
        #[cfg(test)]
        eprintln!("Accessibility prepared focus {:?}, selections={:?}", update.focus,
            update.nodes.iter().filter_map(|(id, node)| node.text_selection().map(|selection| (*id, *selection))).collect::<Vec<_>>());
        self.serial = self.serial.checked_add(1).expect("accessibility publication serial");
        let bounds = views.iter().filter_map(|view| view.viewport.as_ref().map(|viewport| (view.output, viewport.bounds))).collect();
        let publication = Arc::new(Publication { epoch, serial: self.serial, update, targets, spans, bounds });
        self.source = Some(publication.clone());
        self.pending = Some(Receipt::new(publication, views.iter().map(|view| view.output), self.shared.clone()));
    }
}

fn current(catalog: &Catalog, request: &Request) -> bool {
    catalog.epoch == request.epoch && catalog.targets.get(&request.action.target_node) == Some(&request.target)
        && match &request.action.data {
            Some(ActionData::SetTextSelection(selection)) => translate_selection(&catalog.spans, request.target.entity, *selection)
                .is_some_and(|selection| Some(selection) == request.selection),
            _ => request.selection.is_none(),
        }
}

impl Drop for Bridge {
    fn drop(&mut self) { self.invalidate(); }
}

impl Presentation {
    pub(crate) fn accessibility_input_completed(&self) {
        if let Some(bridge) = &self.accessibility {
            let fence = bridge.shared.pending_input.load(Ordering::Acquire);
            if fence != 0 && bridge.shared.progress.as_ref().is_some_and(|progress| progress.reached(fence)) { bridge.shared.wake.ring(); }
        }
    }

    pub fn enable_accessibility(&mut self) {
        if self.accessibility.is_none() { self.accessibility = Some(Bridge::new(self.app.world_mut())); }
    }

    pub(crate) fn accessibility_availability(&mut self, active: bool) {
        if let Some(bridge) = &mut self.accessibility {
            if bridge.active != active { bridge.active = active; bridge.invalidate(); }
            if !active { bridge.focus(false); }
        }
    }

    pub(super) fn accessibility_views(&mut self, geometry: Option<&lince_interface::host::GeometrySnapshot>) -> Vec<crate::native_input::projection::View> {
        let Some(focused) = self.native_input_view(geometry) else { return Vec::new(); };
        let mut views = vec![focused.clone()];
        for (output, target) in &self.output_state.targets {
            if *output == focused.output { continue; }
            let viewport = geometry.and_then(|geometry| geometry.outputs.iter().find(|viewport| viewport.id == *output)).cloned()
                .or_else(|| self.output_state.outputs.iter().find(|candidate| candidate.viewport.id == *output).map(|candidate| {
                    let mut viewport = candidate.viewport.clone();
                    if let Some(assignment) = self.output_state.assignments.outputs().iter().find(|assignment| assignment.id == *output) { viewport.workspace = assignment.active; }
                    viewport
                }));
            if viewport.is_none() { continue; }
            views.push(crate::native_input::projection::View { root: focused.root, output: *output, image: target.image.id(), raster: target.raster, origin: target.origin, viewport });
        }
        views.sort_by_key(|view| view.output);
        views
    }

    pub(super) fn accessibility_requests(&mut self) {
        if self.accessibility.is_none() { return; }
        let views = self.accessibility_views(None);
        let requests = self.accessibility.as_mut().unwrap().requests(self.app.world(), &views);
        for request in requests {
            let context = native::accessibility::context(self.app.world(), request.target.entity, true);
            if context.as_ref() != Some(&request.target.context) {
                #[cfg(test)]
                eprintln!("Accessibility changed {:?} for {:?}: context={}, owner={}, describes={}", request.action.action, request.target.entity,
                    context.is_some(), context.as_ref().is_some_and(|current| request.target.context.same_owner(current)),
                    request.target.context.describes(self.app.world(), request.target.entity));
                continue;
            }
            if self.output_state.assignments.focused().is_some_and(|output| output != request.target.output)
                && let Err(error) = self.focus_output(request.target.output)
            {
                native::notifications::report(self.app.world(), "Accessibility", &error.detail);
                continue;
            }
            let result = native::accessibility::perform(self.app.world_mut(), request.target.entity, &request.target.context, &request.native_action());
            #[cfg(test)]
            eprintln!("Accessibility performed {:?} for {:?}: result={:?}, selection={:?}", request.action.action, request.target.entity,
                result, self.app.world().get::<bevy::text::EditableText>(request.target.entity).map(|input| input.editor.raw_selection().text_range()));
            if let Err(reason) = result {
                native::notifications::report(self.app.world(), "Accessibility", reason);
            }
        }
    }

    pub(super) fn publish_accessibility(&mut self, geometry: Option<&lince_interface::host::GeometrySnapshot>) {
        if self.accessibility.is_none() { return; }
        let views = self.accessibility_views(geometry);
        if let Some(bridge) = &mut self.accessibility { bridge.publish(self.app.world_mut(), &views, geometry); }
    }
}

#[cfg(test)]
mod tests;
