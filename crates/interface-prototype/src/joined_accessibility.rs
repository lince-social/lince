use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId,
    Rect, Role, Tree, TreeId, TreeUpdate,
};
use accesskit_winit::Adapter;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use winit::{event::WindowEvent, event_loop::ActiveEventLoop, window::Window};

const ROOT: NodeId = NodeId(0);
const BOX: NodeId = NodeId(1);
const BUTTON: NodeId = NodeId(2);
const CASTLE: NodeId = NodeId(3);
const CASTLE_TITLE: NodeId = NodeId(4);
const CASTLE_OPEN: NodeId = NodeId(5);
const INSTALLED_WEB: NodeId = NodeId(6);
const WEBSITE_WEB: NodeId = NodeId(7);

#[derive(Clone)]
struct AccessibilityFacts {
    initial_tree_requests: Arc<AtomicU64>,
    actions: Arc<AtomicU64>,
    deactivations: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    browser_count: Arc<AtomicU64>,
}

struct InitialTreeHandler {
    facts: AccessibilityFacts,
}

impl ActivationHandler for InitialTreeHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.facts
            .initial_tree_requests
            .fetch_add(1, Ordering::Relaxed);
        self.facts.active.store(true, Ordering::Release);
        let browser_count = self.facts.browser_count.load(Ordering::Acquire);
        Some(tree_update(
            [1280, 760],
            (browser_count >= 1).then_some([690, 18, 550, 340]),
            (browser_count >= 2).then_some([620, 402, 550, 340]),
        ))
    }
}

struct JoinedActionHandler {
    facts: AccessibilityFacts,
}

impl ActionHandler for JoinedActionHandler {
    fn do_action(&mut self, _request: ActionRequest) {
        self.facts.actions.fetch_add(1, Ordering::Relaxed);
    }
}

struct JoinedDeactivationHandler {
    facts: AccessibilityFacts,
}

impl DeactivationHandler for JoinedDeactivationHandler {
    fn deactivate_accessibility(&mut self) {
        self.facts.deactivations.fetch_add(1, Ordering::Relaxed);
        self.facts.active.store(false, Ordering::Release);
    }
}

pub struct JoinedAccessibility {
    adapter: Adapter,
    facts: AccessibilityFacts,
    published_updates: u64,
}

impl JoinedAccessibility {
    pub fn new(event_loop: &ActiveEventLoop, window: &Window, browser_count: usize) -> Self {
        let facts = AccessibilityFacts {
            initial_tree_requests: Arc::new(AtomicU64::new(0)),
            actions: Arc::new(AtomicU64::new(0)),
            deactivations: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicBool::new(false)),
            browser_count: Arc::new(AtomicU64::new(browser_count as u64)),
        };
        let adapter = Adapter::with_direct_handlers(
            event_loop,
            window,
            InitialTreeHandler {
                facts: facts.clone(),
            },
            JoinedActionHandler {
                facts: facts.clone(),
            },
            JoinedDeactivationHandler {
                facts: facts.clone(),
            },
        );
        Self {
            adapter,
            facts,
            published_updates: 0,
        }
    }

    pub fn process_event(&mut self, window: &Window, event: &WindowEvent) {
        self.adapter.process_event(window, event);
    }

    pub fn publish(&mut self, window_size: [u32; 2], installed: [u32; 4], website: [u32; 4]) {
        let installed = (installed[2] > 0 && installed[3] > 0).then_some(installed);
        let website = (website[2] > 0 && website[3] > 0).then_some(website);
        self.adapter
            .update_if_active(|| tree_update(window_size, installed, website));
        self.published_updates += 1;
    }

    pub fn node_count(&self) -> usize {
        6 + self.facts.browser_count.load(Ordering::Acquire).min(2) as usize
    }

    pub fn initial_tree_requests(&self) -> u64 {
        self.facts.initial_tree_requests.load(Ordering::Relaxed)
    }

    pub fn actions(&self) -> u64 {
        self.facts.actions.load(Ordering::Relaxed)
    }

    pub fn record_keyboard_action(&self) {
        self.facts.actions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn deactivations(&self) -> u64 {
        self.facts.deactivations.load(Ordering::Relaxed)
    }

    pub fn published_updates(&self) -> u64 {
        self.published_updates
    }
}

fn tree_update(
    window_size: [u32; 2],
    installed: Option<[u32; 4]>,
    website: Option<[u32; 4]>,
) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label("Lince joined Interface laboratory");
    root.set_bounds(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: f64::from(window_size[0]),
        y1: f64::from(window_size[1]),
    });
    let mut root_children = vec![BOX, BUTTON, CASTLE];
    if installed.is_some() {
        root_children.push(INSTALLED_WEB);
    }
    if website.is_some() {
        root_children.push(WEBSITE_WEB);
    }
    root.set_children(root_children);

    let mut box_node = Node::new(Role::Canvas);
    box_node.set_label("Box with ten thousand active spatial bodies and two hundred visible Sands");
    box_node.set_bounds(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: f64::from(window_size[0]) * 0.62,
        y1: f64::from(window_size[1]),
    });

    let mut button = Node::new(Role::Button);
    button.set_label("Standalone Record button Sand");
    button.set_bounds(Rect {
        x0: 24.0,
        y0: 24.0,
        x1: 280.0,
        y1: 68.0,
    });
    button.add_action(Action::Focus);
    button.add_action(Action::Click);

    let mut castle = Node::new(Role::Group);
    castle.set_label("Locked Record-card Castle");
    castle.set_bounds(Rect {
        x0: 24.0,
        y0: 88.0,
        x1: 360.0,
        y1: 280.0,
    });
    castle.set_children(vec![CASTLE_TITLE, CASTLE_OPEN]);

    let mut castle_title = Node::new(Role::Heading);
    castle_title.set_label("Build the Box");
    castle_title.set_bounds(Rect {
        x0: 42.0,
        y0: 106.0,
        x1: 330.0,
        y1: 140.0,
    });

    let mut castle_open = Node::new(Role::Button);
    castle_open.set_label("Open Record");
    castle_open.set_bounds(Rect {
        x0: 180.0,
        y0: 224.0,
        x1: 330.0,
        y1: 262.0,
    });
    castle_open.add_action(Action::Focus);
    castle_open.add_action(Action::Click);

    let mut nodes = vec![
        (ROOT, root),
        (BOX, box_node),
        (BUTTON, button),
        (CASTLE, castle),
        (CASTLE_TITLE, castle_title),
        (CASTLE_OPEN, castle_open),
    ];
    if let Some(installed) = installed {
        let mut installed_web = Node::new(Role::WebView);
        installed_web.set_label("Installed HTML Sand with declared Lince bridge");
        installed_web.set_bounds(array_rect(installed));
        installed_web.add_action(Action::Focus);
        nodes.push((INSTALLED_WEB, installed_web));
    }
    if let Some(website) = website {
        let mut website_web = Node::new(Role::WebView);
        website_web.set_label("Website Sand without Lince authority");
        website_web.set_bounds(array_rect(website));
        website_web.add_action(Action::Focus);
        nodes.push((WEBSITE_WEB, website_web));
    }
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(ROOT)),
        tree_id: TreeId::ROOT,
        focus: BUTTON,
    }
}

fn array_rect(value: [u32; 4]) -> Rect {
    Rect {
        x0: f64::from(value[0]),
        y0: f64::from(value[1]),
        x1: f64::from(value[0] + value[2]),
        y1: f64::from(value[1] + value[3]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_tree_has_stable_native_and_html_boundaries() {
        let update = tree_update(
            [1280, 760],
            Some([690, 18, 550, 340]),
            Some([620, 402, 550, 340]),
        );
        assert_eq!(update.nodes.len(), 8);
        assert_eq!(update.tree.map(|tree| tree.root), Some(ROOT));
        assert_eq!(update.focus, BUTTON);
    }
}
