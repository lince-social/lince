use super::*;

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
    #[cfg(any(feature = "nested", test))]
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
