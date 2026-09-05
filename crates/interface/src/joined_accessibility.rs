use crate::{
    configuration::ConfigurationOperation,
    official_sands::{OFFICIAL_SAND_COUNT, OFFICIAL_SANDS},
    primitive_gallery::{PRIMITIVE_COUNT, PRIMITIVES},
    retained_ui::RetainedScene,
    sand::AccessibilityRole,
};
use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId,
    Rect, Role, Tree, TreeId, TreeUpdate,
};
use accesskit_winit::Adapter;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
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
const CONFIGURATION: NodeId = NodeId(10);
const OFFICIAL: NodeId = NodeId(11);
const OFFICIAL_RUNTIME: NodeId = NodeId(12);
const PRIMITIVE_START: u64 = 20;
const CONFIGURATION_START: u64 = 100;
const OFFICIAL_START: u64 = 200;
const OFFICIAL_RUNTIME_START: u64 = 10_000;

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
    requested_configuration: Arc<AtomicU64>,
    requested_official: Arc<AtomicU64>,
    requested_official_runtime: Arc<AtomicU64>,
    official_runtime_actions: Arc<Mutex<BTreeMap<u64, usize>>>,
    official_runtime_nodes: Arc<AtomicU64>,
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
            false,
            0,
            false,
            0,
            None,
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
        } else if (CONFIGURATION_START
            ..CONFIGURATION_START + ConfigurationOperation::ALL.len() as u64)
            .contains(&id)
        {
            self.facts
                .requested_configuration
                .store(id - CONFIGURATION_START, Ordering::Release);
        } else if (OFFICIAL_START..OFFICIAL_START + OFFICIAL_SAND_COUNT as u64).contains(&id) {
            self.facts
                .requested_official
                .store(id - OFFICIAL_START, Ordering::Release);
        } else if let Some(index) = self
            .facts
            .official_runtime_actions
            .lock()
            .ok()
            .and_then(|actions| actions.get(&id).copied())
        {
            self.facts
                .requested_official_runtime
                .store(index as u64, Ordering::Release);
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
            requested_configuration: Arc::new(AtomicU64::new(u64::MAX)),
            requested_official: Arc::new(AtomicU64::new(u64::MAX)),
            requested_official_runtime: Arc::new(AtomicU64::new(u64::MAX)),
            official_runtime_actions: Arc::new(Mutex::new(BTreeMap::new())),
            official_runtime_nodes: Arc::new(AtomicU64::new(0)),
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
        configuration_active: bool,
        configuration_focus: usize,
        official_active: bool,
        official_focus: usize,
        official_runtime: Option<&RetainedScene>,
    ) {
        let installed = (installed[2] > 0 && installed[3] > 0).then_some(installed);
        let website = (website[2] > 0 && website[3] > 0).then_some(website);
        if let Ok(mut actions) = self.facts.official_runtime_actions.lock() {
            actions.clear();
            if let Some(scene) = official_runtime {
                for (index, node) in scene
                    .nodes
                    .iter()
                    .filter(|node| node.interactive)
                    .enumerate()
                {
                    actions.insert(official_runtime_node_id(node.key.as_str()).0, index);
                }
            }
        }
        self.facts.official_runtime_nodes.store(
            official_runtime.map_or(0, |scene| scene.nodes.len() as u64 + 1),
            Ordering::Release,
        );
        self.adapter.update_if_active(|| {
            tree_update(
                window_size,
                installed,
                website,
                gallery_focus,
                configuration_active,
                configuration_focus,
                official_active,
                official_focus,
                official_runtime,
            )
        });
        self.facts
            .gallery_focus
            .store(gallery_focus as u64, Ordering::Release);
        self.published_updates += 1;
    }

    pub fn node_count(&self) -> usize {
        10 + PRIMITIVE_COUNT
            + ConfigurationOperation::ALL.len()
            + OFFICIAL_SAND_COUNT
            + self.facts.browser_count.load(Ordering::Acquire).min(2) as usize
            + self.facts.official_runtime_nodes.load(Ordering::Acquire) as usize
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

    pub fn take_requested_configuration(&self) -> Option<usize> {
        let requested = self
            .facts
            .requested_configuration
            .swap(u64::MAX, Ordering::AcqRel);
        (requested < ConfigurationOperation::ALL.len() as u64).then_some(requested as usize)
    }

    pub fn take_requested_official(&self) -> Option<usize> {
        let requested = self
            .facts
            .requested_official
            .swap(u64::MAX, Ordering::AcqRel);
        (requested < OFFICIAL_SAND_COUNT as u64).then_some(requested as usize)
    }

    pub fn take_requested_official_runtime(&self) -> Option<usize> {
        let requested = self
            .facts
            .requested_official_runtime
            .swap(u64::MAX, Ordering::AcqRel);
        (requested != u64::MAX).then_some(requested as usize)
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
    configuration_active: bool,
    configuration_focus: usize,
    official_active: bool,
    official_focus: usize,
    official_runtime: Option<&RetainedScene>,
) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label("Lince joined Interface laboratory");
    root.set_bounds(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: f64::from(window_size[0]),
        y1: f64::from(window_size[1]),
    });
    let mut root_children = vec![BOX, BUTTON, CASTLE, GALLERY, CONFIGURATION, OFFICIAL];
    if installed.is_some() {
        root_children.push(INSTALLED_WEB);
    }
    if website.is_some() {
        root_children.push(WEBSITE_WEB);
    }
    if official_runtime.is_some() {
        root_children.push(OFFICIAL_RUNTIME);
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
    let configuration_ids = (0..ConfigurationOperation::ALL.len())
        .map(|index| NodeId(CONFIGURATION_START + index as u64))
        .collect::<Vec<_>>();
    let mut configuration = Node::new(Role::Group);
    configuration.set_label("Configuration Sand");
    configuration.set_description("Open with F11; every accepted change previews and persists");
    configuration.set_bounds(Rect {
        x0: 16.0,
        y0: 72.0,
        x1: f64::from(window_size[0]) * 0.47,
        y1: 530.0,
    });
    configuration.set_children(configuration_ids.clone());
    nodes.push((CONFIGURATION, configuration));
    for (index, (operation, node_id)) in ConfigurationOperation::ALL
        .iter()
        .zip(configuration_ids)
        .enumerate()
    {
        let mut node = Node::new(Role::Button);
        node.set_label(operation.label());
        node.set_bounds(Rect {
            x0: 20.0,
            y0: 100.0 + index as f64 * 21.0,
            x1: f64::from(window_size[0]) * 0.47 - 8.0,
            y1: 119.0 + index as f64 * 21.0,
        });
        node.add_action(Action::Focus);
        node.add_action(Action::Click);
        nodes.push((node_id, node));
    }
    let official_ids = (0..OFFICIAL_SAND_COUNT)
        .map(|index| NodeId(OFFICIAL_START + index as u64))
        .collect::<Vec<_>>();
    let mut official = Node::new(Role::Group);
    official.set_label("Official Sand migration catalog");
    official
        .set_description("Open with F12; structure readiness is separate from Behavior completion");
    official.set_bounds(Rect {
        x0: 16.0,
        y0: 72.0,
        x1: f64::from(window_size[0]) * 0.47,
        y1: 650.0,
    });
    official.set_children(official_ids.clone());
    nodes.push((OFFICIAL, official));
    for (index, (spec, node_id)) in OFFICIAL_SANDS.iter().zip(official_ids).enumerate() {
        let mut node = Node::new(Role::Button);
        node.set_label(spec.display_name);
        node.set_description(spec.state.label());
        node.set_bounds(Rect {
            x0: 20.0,
            y0: 100.0 + index as f64 * 21.0,
            x1: f64::from(window_size[0]) * 0.47 - 8.0,
            y1: 119.0 + index as f64 * 21.0,
        });
        node.add_action(Action::Focus);
        node.add_action(Action::Click);
        nodes.push((node_id, node));
    }
    if let Some(scene) = official_runtime {
        let child_ids = scene
            .nodes
            .iter()
            .map(|node| official_runtime_node_id(node.key.as_str()))
            .collect::<Vec<_>>();
        let mut runtime = Node::new(Role::Group);
        runtime.set_label(format!("{} native retained runtime", scene.root_uid));
        runtime.set_description("Exact recursive Sand definitions projected by the native host");
        runtime.set_children(child_ids.clone());
        nodes.push((OFFICIAL_RUNTIME, runtime));
        for (semantic, node_id) in scene.nodes.iter().zip(child_ids) {
            let mut node = Node::new(accesskit_role(semantic.role));
            node.set_label(semantic.label.as_str());
            if let Some(description) = semantic.description.as_deref() {
                node.set_description(description);
            }
            node.set_bounds(Rect {
                x0: f64::from(semantic.rect.x),
                y0: f64::from(semantic.rect.y),
                x1: f64::from(semantic.rect.x + semantic.rect.width),
                y1: f64::from(semantic.rect.y + semantic.rect.height),
            });
            if semantic.interactive {
                node.add_action(Action::Focus);
                node.add_action(Action::Click);
            }
            nodes.push((node_id, node));
        }
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
        focus: if let Some(scene) = official_runtime {
            scene
                .focused()
                .map(|node| official_runtime_node_id(node.key.as_str()))
                .unwrap_or(OFFICIAL_RUNTIME)
        } else if configuration_active {
            NodeId(
                CONFIGURATION_START
                    + configuration_focus.min(ConfigurationOperation::ALL.len() - 1) as u64,
            )
        } else if official_active {
            NodeId(OFFICIAL_START + official_focus.min(OFFICIAL_SAND_COUNT - 1) as u64)
        } else {
            NodeId(PRIMITIVE_START + gallery_focus.min(PRIMITIVE_COUNT - 1) as u64)
        },
    }
}

fn official_runtime_node_id(key: &str) -> NodeId {
    let hash = key
        .as_bytes()
        .iter()
        .fold(14_695_981_039_346_656_037_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211)
        });
    NodeId(OFFICIAL_RUNTIME_START + hash % (u64::MAX - OFFICIAL_RUNTIME_START))
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
    use crate::{
        official_sands::official_sand_package,
        retained_ui::{RetainedRect, RetainedScene},
    };

    #[test]
    fn joined_tree_has_stable_native_and_html_boundaries() {
        let update = tree_update(
            [1280, 760],
            Some([690, 18, 550, 340]),
            Some([620, 402, 550, 340]),
            4,
            false,
            0,
            false,
            0,
            None,
        );
        assert_eq!(
            update.nodes.len(),
            10 + PRIMITIVE_COUNT + ConfigurationOperation::ALL.len() + OFFICIAL_SAND_COUNT + 2
        );
        assert_eq!(update.tree.map(|tree| tree.root), Some(ROOT));
        assert_eq!(update.focus, NodeId(PRIMITIVE_START + 4));
    }

    #[test]
    fn retained_runtime_nodes_keep_semantic_identity_and_focus() {
        let scene = RetainedScene::from_package(
            &official_sand_package(),
            "record",
            BTreeMap::new(),
            RetainedRect {
                x: 640.0,
                y: 20.0,
                width: 600.0,
                height: 720.0,
            },
            2,
        )
        .unwrap();
        let focused = scene.focused().unwrap();
        let update = tree_update([1280, 760], None, None, 4, false, 0, false, 0, Some(&scene));
        assert_eq!(
            update.nodes.len(),
            10 + PRIMITIVE_COUNT
                + ConfigurationOperation::ALL.len()
                + OFFICIAL_SAND_COUNT
                + 1
                + scene.nodes.len()
        );
        assert_eq!(update.focus, official_runtime_node_id(focused.key.as_str()));
        assert_eq!(
            official_runtime_node_id("record/summary/open/control"),
            official_runtime_node_id("record/summary/open/control")
        );
        assert_ne!(
            official_runtime_node_id("record/summary/open/control"),
            official_runtime_node_id("record/thread/send/control")
        );
    }
}
