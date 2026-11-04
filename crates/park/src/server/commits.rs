use super::*;

#[cfg(feature = "native-presentation")]
impl WindowSnapshot {
    pub(crate) fn same_semantics(&self, other: &Self) -> bool {
        self.id == other.id && self.surface == other.surface && self.mapped == other.mapped
            && self.title == other.title && self.app_id == other.app_id && self.parent == other.parent
            && self.geometry == other.geometry && self.minimum == other.minimum && self.maximum == other.maximum
            && same_nodes(&self.nodes, &other.nodes) && self.popups.len() == other.popups.len()
            && self.popups.iter().zip(&other.popups).all(|(left, right)| {
                left.surface == right.surface && left.origin == right.origin && same_nodes(&left.nodes, &right.nodes)
            })
    }
}

#[cfg(feature = "native-presentation")]
fn same_nodes(left: &[SurfaceNode], right: &[SurfaceNode]) -> bool {
    fn region(left: &Option<RegionAttributes>, right: &Option<RegionAttributes>) -> bool {
        match (left, right) {
            (None, None) => true,
            (Some(left), Some(right)) => left.rects.iter().map(|(kind, rect)| (matches!(kind, smithay::wayland::compositor::RectangleKind::Add), rect))
                .eq(right.rects.iter().map(|(kind, rect)| (matches!(kind, smithay::wayland::compositor::RectangleKind::Add), rect))),
            _ => false,
        }
    }
    left.len() == right.len() && left.iter().zip(right).all(|(left, right)| {
        left.surface == right.surface && left.origin == right.origin && left.layout == right.layout
            && left.scale == right.scale && left.transform == right.transform && left.color == right.color
            && left.content.size == right.content.size && region(&left.input, &right.input) && region(&left.opaque, &right.opaque)
    })
}

#[derive(PartialEq)]
pub(super) struct Visual {
    geometry: Option<Rectangle<i32, Logical>>,
    minimum: [i32; 2],
    maximum: [i32; 2],
    nodes: Vec<Node>,
}

#[derive(PartialEq)]
struct Node {
    surface: WlSurface,
    version: u64,
    origin: [i32; 2],
    layout: SurfaceLayout,
    input: Option<Vec<(bool, Rectangle<i32, Logical>)>>,
    opaque: Option<Vec<(bool, Rectangle<i32, Logical>)>>,
    color: lince_interface::host::color::Description,
}

impl Visual {
    pub(super) fn same_semantics(&self, other: &Self) -> bool {
        self.geometry == other.geometry && self.minimum == other.minimum && self.maximum == other.maximum
            && self.nodes.len() == other.nodes.len()
            && self.nodes.iter().zip(&other.nodes).all(|(left, right)| {
                left.surface == right.surface && left.origin == right.origin && left.layout == right.layout
                    && left.input == right.input && left.opaque == right.opaque && left.color == right.color
            })
    }

    fn new(
        geometry: Option<Rectangle<i32, Logical>>,
        minimum: [i32; 2],
        maximum: [i32; 2],
        nodes: &[SurfaceNode],
    ) -> Self {
        let region = |region: &Option<RegionAttributes>| {
            region.as_ref().map(|region| {
                region
                    .rects
                    .iter()
                    .map(|(kind, rect)| {
                        (
                            matches!(kind, smithay::wayland::compositor::RectangleKind::Add),
                            *rect,
                        )
                    })
                    .collect()
            })
        };
        Self {
            geometry,
            minimum,
            maximum,
            nodes: nodes
                .iter()
                .map(|node| Node {
                    surface: node.surface.clone(),
                    version: node.content.version,
                    origin: node.origin,
                    layout: node.layout,
                    input: region(&node.input),
                    opaque: region(&node.opaque),
                    color: node.color,
                })
                .collect(),
        }
    }
}

impl Server {
    #[cfg(any(feature = "nested", all(test, feature = "server-tests")))]
    pub(crate) fn callback_nodes(&self, window: &WindowSnapshot) -> Vec<SurfaceNode> {
        let visual = Visual::new(
            window.geometry,
            window.minimum,
            window.maximum,
            &window.nodes,
        );
        let Some(current) = self.state.visuals.get(&window.surface) else {
            return window.nodes.clone();
        };
        if current.geometry != visual.geometry
            || current.minimum != visual.minimum
            || current.maximum != visual.maximum
            || current.nodes.len() != visual.nodes.len()
            || !current
                .nodes
                .iter()
                .zip(&visual.nodes)
                .all(|(current, drawn)| {
                    current.surface == drawn.surface
                        && current.origin == drawn.origin
                        && current.layout == drawn.layout
                })
            || current != &visual
                && window
                    .nodes
                    .iter()
                    .any(|node| is_sync_subsurface(&node.surface))
        {
            return window.nodes.clone();
        }
        let latest = self.state.nodes(&window.surface);
        window
            .nodes
            .iter()
            .zip(&visual.nodes)
            .zip(&current.nodes)
            .map(|((drawn, visual), current)| {
                if current == visual {
                    latest
                        .iter()
                        .find(|node| node.surface == drawn.surface)
                        .unwrap_or(drawn)
                        .clone()
                } else {
                    drawn.clone()
                }
            })
            .collect()
    }
}

impl State {
    pub(super) fn visual(&self, root: &WlSurface) -> Visual {
        let state = with_states(root, |states| {
            *states
                .cached_state
                .get::<xdg::SurfaceCachedState>()
                .current()
        });
        Visual::new(
            state.geometry,
            [state.min_size.w, state.min_size.h],
            [state.max_size.w, state.max_size.h],
            &self.nodes(root),
        )
    }
}
