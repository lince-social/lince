use crate::{
    primitive_gallery::{PRIMITIVE_COUNT, PRIMITIVES},
    sand::AccessibilityRole,
};
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
const GALLERY: NodeId = NodeId(8);
const CALL_FRAME: NodeId = NodeId(9);
const PRIMITIVE_START: u64 = 20;

#[derive(Clone)]
struct AccessibilityFacts {
    initial_tree_requests: Arc<AtomicU64>,
    actions: Arc<AtomicU64>,
    deactivations: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    browser_count: Arc<AtomicU64>,
    gallery_focus: Arc<AtomicU64>,
    requested_primitive: Arc<AtomicU64>,
    requested_composition: Arc<AtomicBool>,
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
        let gallery_focus = self.facts.gallery_focus.load(Ordering::Acquire) as usize;
        Some(tree_update(
            [1280, 760],
            (browser_count >= 1).then_some([690, 18, 550, 340]),
            (browser_count >= 2).then_some([620, 402, 550, 340]),
            gallery_focus,
        ))
    }
}

struct JoinedActionHandler {
    facts: AccessibilityFacts,
}

impl ActionHandler for JoinedActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        self.facts.actions.fetch_add(1, Ordering::Relaxed);
        let id = request.target_node.0;
        if (PRIMITIVE_START..PRIMITIVE_START + PRIMITIVE_COUNT as u64).contains(&id) {
            self.facts
                .requested_primitive
                .store(id - PRIMITIVE_START, Ordering::Release);
        } else if request.target_node == CASTLE_OPEN {
            self.facts
                .requested_composition
                .store(true, Ordering::Release);
        }
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
            gallery_focus: Arc::new(AtomicU64::new(4)),
            requested_primitive: Arc::new(AtomicU64::new(u64::MAX)),
            requested_composition: Arc::new(AtomicBool::new(false)),
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

    pub fn publish(
        &mut self,
        window_size: [u32; 2],
        installed: [u32; 4],
        website: [u32; 4],
        gallery_focus: usize,
    ) {
        let installed = (installed[2] > 0 && installed[3] > 0).then_some(installed);
        let website = (website[2] > 0 && website[3] > 0).then_some(website);
        self.adapter
            .update_if_active(|| tree_update(window_size, installed, website, gallery_focus));
        self.facts
            .gallery_focus
            .store(gallery_focus as u64, Ordering::Release);
        self.published_updates += 1;
    }

    pub fn node_count(&self) -> usize {
        8 + PRIMITIVE_COUNT + self.facts.browser_count.load(Ordering::Acquire).min(2) as usize
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

    pub fn take_requested_primitive(&self) -> Option<usize> {
        let requested = self
            .facts
            .requested_primitive
            .swap(u64::MAX, Ordering::AcqRel);
        (requested < PRIMITIVE_COUNT as u64).then_some(requested as usize)
    }

    pub fn take_requested_composition(&self) -> bool {
        self.facts
            .requested_composition
            .swap(false, Ordering::AcqRel)
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
    gallery_focus: usize,
) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label("Lince joined Interface laboratory");
    root.set_bounds(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: f64::from(window_size[0]),
        y1: f64::from(window_size[1]),
    });
    let mut root_children = vec![BOX, BUTTON, CASTLE, GALLERY];
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
    castle.set_label("Locked nested video-call Castle");
    castle.set_bounds(Rect {
        x0: 24.0,
        y0: 88.0,
        x1: 360.0,
        y1: 280.0,
    });
    castle.set_children(vec![CASTLE_TITLE]);

    let mut castle_title = Node::new(Role::Group);
    castle_title.set_label("Video-call compound Sand");
    castle_title.set_bounds(Rect {
        x0: 42.0,
        y0: 106.0,
        x1: 330.0,
        y1: 140.0,
    });
    castle_title.set_children(vec![CALL_FRAME, CASTLE_OPEN]);

    let mut call_frame = Node::new(Role::Group);
    call_frame.set_label("Video call retained surface");
    call_frame.set_bounds(Rect {
        x0: 42.0,
        y0: 146.0,
        x1: 330.0,
        y1: 214.0,
    });

    let mut castle_open = Node::new(Role::Button);
    castle_open.set_label("Open nested Record");
    castle_open.set_bounds(Rect {
        x0: 180.0,
        y0: 224.0,
        x1: 330.0,
        y1: 262.0,
    });
    castle_open.add_action(Action::Focus);
    castle_open.add_action(Action::Click);

    let primitive_ids = (0..PRIMITIVE_COUNT)
        .map(|index| NodeId(PRIMITIVE_START + index as u64))
        .collect::<Vec<_>>();
    let mut gallery = Node::new(Role::Group);
    gallery.set_label("Primitive Sand Gallery");
    gallery.set_bounds(Rect {
        x0: 16.0,
        y0: 90.0,
        x1: f64::from(window_size[0]) * 0.47,
        y1: 330.0,
    });
    gallery.set_children(primitive_ids.clone());

    let mut nodes = vec![
        (ROOT, root),
        (BOX, box_node),
        (BUTTON, button),
        (CASTLE, castle),
        (CASTLE_TITLE, castle_title),
        (CALL_FRAME, call_frame),
        (CASTLE_OPEN, castle_open),
        (GALLERY, gallery),
    ];
    for (index, (primitive, node_id)) in PRIMITIVES.iter().zip(primitive_ids).enumerate() {
        let mut node = Node::new(accesskit_role(primitive.role));
        node.set_label(primitive.label);
        let column = index % 2;
        let row = index / 2;
        let column_width = f64::from(window_size[0]) * 0.225;
        let left = 20.0 + column as f64 * column_width;
        let top = 100.0 + row as f64 * 21.0;
        node.set_bounds(Rect {
            x0: left,
            y0: top,
            x1: left + column_width - 8.0,
            y1: top + 19.0,
        });
        if primitive.interactive {
            node.add_action(Action::Focus);
            node.add_action(Action::Click);
        }
        nodes.push((node_id, node));
    }
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
        focus: NodeId(PRIMITIVE_START + gallery_focus.min(PRIMITIVE_COUNT - 1) as u64),
    }
}

fn accesskit_role(role: AccessibilityRole) -> Role {
    match role {
        AccessibilityRole::Text => Role::Paragraph,
        AccessibilityRole::Heading => Role::Heading,
        AccessibilityRole::Image => Role::Image,
        AccessibilityRole::Button => Role::Button,
        AccessibilityRole::Status => Role::Status,
        AccessibilityRole::TextInput => Role::TextInput,
        AccessibilityRole::MultilineTextInput => Role::MultilineTextInput,
        AccessibilityRole::Checkbox => Role::CheckBox,
        AccessibilityRole::Radio => Role::RadioButton,
        AccessibilityRole::Disclosure => Role::DisclosureTriangle,
        AccessibilityRole::Select => Role::ComboBox,
        AccessibilityRole::Tooltip => Role::Tooltip,
        AccessibilityRole::Alert => Role::Alert,
        AccessibilityRole::List => Role::List,
        AccessibilityRole::Row => Role::Row,
        AccessibilityRole::Article => Role::Article,
        AccessibilityRole::Group => Role::Group,
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
            4,
        );
        assert_eq!(update.nodes.len(), 8 + PRIMITIVE_COUNT + 2);
        assert_eq!(update.tree.map(|tree| tree.root), Some(ROOT));
        assert_eq!(update.focus, NodeId(PRIMITIVE_START + 4));
    }
}
