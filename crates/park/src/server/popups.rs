use super::*;
use smithay::{
    desktop::{
        PopupGrab, PopupKeyboardGrab, PopupKind, PopupManager, PopupPointerGrab,
        PopupUngrabStrategy, find_popup_root_surface, get_popup_toplevel_coords,
    },
    input::pointer::Focus,
    wayland::shell::xdg::XdgPopupSurfaceData,
};

#[derive(Default)]
pub(super) struct Popups {
    pub manager: PopupManager,
    pub surfaces: HashMap<WlSurface, PopupSurface>,
    pub grab: Option<PopupGrab<State>>,
    pub root: Option<WlSurface>,
}

#[derive(Clone)]
pub struct PopupSnapshot {
    pub surface: WlSurface,
    pub origin: [i32; 2],
    pub nodes: Vec<SurfaceNode>,
}

impl PopupSnapshot {
    pub fn layer_mapping(&self, origin: [f64; 2]) -> Option<lince_interface::host::ContentMapping> {
        let mut nodes = self.nodes.iter();
        let rectangle = |node: &SurfaceNode| {
            Rectangle::new(
                (f64::from(node.origin[0]), f64::from(node.origin[1])).into(),
                (node.layout.size[0], node.layout.size[1]).into(),
            )
        };
        let first: Rectangle<f64, Logical> = rectangle(nodes.next()?);
        let bounds = nodes.fold(first, |bounds, node| bounds.merge(rectangle(node)));
        Some(lince_interface::host::ContentMapping { corners: None,
            canvas: lince_interface::host::Rectangle::new(
                [
                    origin[0] + f64::from(self.origin[0]) + bounds.loc.x,
                    origin[1] + f64::from(self.origin[1]) + bounds.loc.y,
                ],
                [bounds.size.w, bounds.size.h],
            )?,
            surface: lince_interface::host::Rectangle::new(
                [bounds.loc.x, bounds.loc.y],
                [bounds.size.w, bounds.size.h],
            )?,
        })
        .filter(valid_mapping)
    }
    pub fn mapping(
        &self,
        parent: &WindowSnapshot,
        item: &lince_interface::host::ItemGeometry,
    ) -> Option<lince_interface::host::ContentMapping> {
        let mut nodes = self.nodes.iter();
        let rectangle = |node: &SurfaceNode| {
            Rectangle::new(
                (f64::from(node.origin[0]), f64::from(node.origin[1])).into(),
                (node.layout.size[0], node.layout.size[1]).into(),
            )
        };
        let first: Rectangle<f64, Logical> = rectangle(nodes.next()?);
        let bounds = nodes.fold(first, |bounds, node| bounds.merge(rectangle(node)));
        let parent_bounds = parent.bounds()?;
        let parent_origin = parent
            .geometry
            .map_or(parent_bounds.loc, |geometry| geometry.loc.to_f64());
        let scale = std::array::from_fn::<_, 2, _>(|axis| {
            item.mapping.canvas.size()[axis] / item.mapping.surface.size()[axis]
        });
        let popup_origin = [
            f64::from(self.origin[0]) + bounds.loc.x + parent_origin.x - parent_bounds.loc.x,
            f64::from(self.origin[1]) + bounds.loc.y + parent_origin.y - parent_bounds.loc.y,
        ];
        let anchor = item.mapping.canvas_point(popup_origin)?;
        Some(lince_interface::host::ContentMapping { corners: None,
            canvas: lince_interface::host::Rectangle::new(
                anchor,
                [bounds.size.w * scale[0], bounds.size.h * scale[1]],
            )?,
            surface: lince_interface::host::Rectangle::new(
                [bounds.loc.x, bounds.loc.y],
                [bounds.size.w, bounds.size.h],
            )?,
        })
        .filter(valid_mapping)
    }

    pub fn hit_surface(&self, point: [f64; 2]) -> Option<(WlSurface, [f64; 2])> {
        self.nodes
            .iter()
            .find_map(|node| node.hit(point).map(|point| (node.surface.clone(), point)))
    }

    pub fn hit(&self, point: [f64; 2]) -> Option<(WlSurface, [f64; 2])> {
        let point = std::array::from_fn(|axis| point[axis] - f64::from(self.origin[axis]));
        self.nodes
            .iter()
            .find_map(|node| node.hit(point).map(|point| (node.surface.clone(), point)))
    }
}

impl State {
    pub(super) fn dismiss_children(&mut self, root: &WlSurface) {
        let children: Vec<_> = PopupManager::popups_for_surface(root)
            .filter_map(|(kind, _)| self.popups.surfaces.get(kind.wl_surface()).cloned())
            .collect();
        for child in children {
            if self.popups.surfaces.contains_key(child.wl_surface()) {
                self.reject_popup(&child);
            }
        }
    }

    fn reject_popup(&mut self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        if self.popups.surfaces.contains_key(popup.wl_surface())
            && let Ok(root) = find_popup_root_surface(&kind)
        {
            let _ = PopupManager::dismiss_popup(&root, &kind);
        } else {
            popup.send_popup_done();
        }
        let removed: Vec<_> = self
            .popups
            .surfaces
            .keys()
            .filter(|surface| {
                let mut current = (*surface).clone();
                for _ in 0..32 {
                    if &current == popup.wl_surface() {
                        return true;
                    }
                    let Some(parent) = self
                        .popups
                        .surfaces
                        .get(&current)
                        .and_then(PopupSurface::get_parent_surface)
                    else {
                        return false;
                    };
                    current = parent;
                }
                true
            })
            .cloned()
            .collect();
        for surface in removed {
            self.popups.surfaces.remove(&surface);
        }
        self.changed();
    }

    pub(super) fn input_root(&self, surface: &WlSurface) -> Option<WlSurface> {
        if !surface.is_alive() {
            return None;
        }
        let mut root = surface.clone();
        for _ in 0..32 {
            if let Some(parent) = get_parent(&root) {
                root = parent;
                continue;
            }
            if let Some(popup) = self.popups.surfaces.get(&root) {
                root = popup.get_parent_surface()?;
                continue;
            }
            if let Some(popup) = self.ime.popups.iter().find(|popup| popup.wl_surface() == &root) {
                root = popup.get_parent()?.surface.clone();
                continue;
            }
            return (self.id(&root).is_some() || self.layers.surfaces.contains_key(&root))
                .then_some(root.clone()).or_else(|| (self.native_surface.as_ref() == Some(&root)).then_some(root));
        }
        None
    }

    pub(super) fn add_popup(&mut self, popup: PopupSurface, positioner: PositionerState) {
        if self.popups.surfaces.len() >= 256 || !valid_positioner(positioner) {
            popup.send_popup_done();
            return;
        }
        if let Some(parent) = popup.get_parent_surface() {
            if parent.client().map(|client| client.id())
                != popup.wl_surface().client().map(|client| client.id())
                || self.input_root(&parent).is_none()
            {
                popup.send_popup_done();
                return;
            }
        }
        if !self.position_popup(&popup, positioner) {
            return;
        }
        if self
            .popups
            .manager
            .track_popup(popup.clone().into())
            .is_ok()
        {
            self.popups
                .surfaces
                .insert(popup.wl_surface().clone(), popup);
        }
    }

    pub(super) fn position_popup(
        &mut self,
        popup: &PopupSurface,
        positioner: PositionerState,
    ) -> bool {
        if !valid_positioner(positioner) {
            self.reject_popup(popup);
            return false;
        }
        let kind = PopupKind::Xdg(popup.clone());
        let mut bounds = Rectangle::from_size((self.initial_size[0], self.initial_size[1]).into());
        if let Ok(root) = find_popup_root_surface(&kind) {
            let offset = get_popup_toplevel_coords(&kind);
            if let Some(id) = self.id(&root) {
                if let Some(geometry) = &self.presented_geometry {
                    if let Some(item) = geometry
                        .items
                        .iter()
                        .filter(|item| item.id == lince_interface::host::ItemId::Application(id))
                        .min_by_key(|item| self.presented_focus != Some(item.output))
                    {
                        if let Some(output) = geometry
                            .outputs
                            .iter()
                            .find(|output| output.id == item.output)
                        {
                            let area = if item.mapping.canvas == output.bounds {
                                output.bounds
                            } else {
                                output.usable.unwrap_or(output.bounds)
                            };
                            let scale = std::array::from_fn::<_, 2, _>(|axis| {
                                item.mapping.canvas.size()[axis] / item.mapping.surface.size()[axis]
                            });
                            let origin = std::array::from_fn::<_, 2, _>(|axis| {
                                (area.origin()[axis] - item.mapping.canvas.origin()[axis])
                                    / scale[axis]
                            });
                            let size = std::array::from_fn::<_, 2, _>(|axis| {
                                area.size()[axis] / scale[axis]
                            });
                            if origin
                                .iter()
                                .chain(size.iter())
                                .any(|value| !value.is_finite() || value.abs() > 1_000_000.0)
                            {
                                self.reject_popup(popup);
                                return false;
                            }
                            bounds = Rectangle::new(
                                (origin[0].floor() as i32, origin[1].floor() as i32).into(),
                                (
                                    size[0].ceil().max(1.0) as i32,
                                    size[1].ceil().max(1.0) as i32,
                                )
                                    .into(),
                            );
                        }
                    }
                }
            } else if let Some(output) = self
                .layers
                .outputs
                .get(&root)
                .and_then(|id| self.outputs.get(id))
            {
                let layer = self.layers.surfaces.get(&root).unwrap();
                let map = smithay::desktop::layer_map_for_output(&output.output);
                if let Some(geometry) = map.layer_geometry(layer) {
                    bounds = Rectangle::new(
                        (-geometry.loc.x, -geometry.loc.y).into(),
                        super::outputs::logical_size(&output.output).unwrap(),
                    );
                }
            }
            bounds.loc -= offset;
        }
        let geometry = positioner.get_unconstrained_geometry(bounds);
        popup.with_pending_state(|state| {
            state.positioner = positioner;
            state.geometry = geometry;
        });
        true
    }

    pub(super) fn popup_commit(&mut self, surface: &WlSurface) {
        let Some(popup) = self.popups.surfaces.get(surface).cloned() else {
            return;
        };
        if !popup.is_initial_configure_sent() {
            let positioner = popup.with_pending_state(|state| state.positioner);
            if self.position_popup(&popup, positioner) {
                let _ = popup.send_configure();
            }
        }
    }

    pub(super) fn reconstrain_popups(&mut self) {
        let popups: Vec<_> = self.popups.surfaces.values().cloned().collect();
        for popup in popups {
            let positioner = with_states(popup.wl_surface(), |states| {
                let data = states
                    .data_map
                    .get::<XdgPopupSurfaceData>()?
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                (data.current.positioner.reactive && data.pending_configures().len() < 64)
                    .then_some(data.current.positioner)
            });
            if let Some(positioner) = positioner
                && popup.is_initial_configure_sent()
                && self.position_popup(&popup, positioner)
                && popup.has_pending_changes()
            {
                let _ = popup.send_configure();
            }
        }
    }

    pub(super) fn grab_popup(&mut self, popup: PopupSurface, seat: WlSeat, serial: Serial) {
        let kind = PopupKind::Xdg(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            self.reject_popup(&popup);
            return;
        };
        if Seat::<State>::from_resource(&seat) != Some(self.seat.clone())
            || !self.authorized_input(serial, &root)
            || !self.popups.surfaces.contains_key(popup.wl_surface())
        {
            self.reject_popup(&popup);
            return;
        }
        let Ok(grab) =
            self.popups
                .manager
                .grab_popup::<State>(root.clone(), kind, &self.seat, serial)
        else {
            self.reject_popup(&popup);
            return;
        };
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = self.seat.get_pointer() {
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
        self.popups.root = Some(root);
        self.popups.grab = Some(grab);
    }

    pub(super) fn popup_snapshots(&self, root: &WlSurface) -> Vec<PopupSnapshot> {
        PopupManager::popups_for_surface(root)
            .filter_map(|(popup, offset)| {
                if !self.popups.surfaces.contains_key(popup.wl_surface())
                    || !self.buffers.contains_key(popup.wl_surface())
                {
                    return None;
                }
                let geometry = popup.geometry();
                Some(PopupSnapshot {
                    surface: popup.wl_surface().clone(),
                    origin: [
                        offset.x.saturating_sub(geometry.loc.x),
                        offset.y.saturating_sub(geometry.loc.y),
                    ],
                    nodes: self.nodes(popup.wl_surface()),
                })
            })
            .chain(self.ime_snapshots(root))
            .collect()
    }
}

impl Server {
    pub fn popup_grab_active(&self) -> bool {
        self.state
            .popups
            .grab
            .as_ref()
            .is_some_and(|grab| !grab.has_ended())
    }

    pub fn popup_target(&self, surface: &WlSurface) -> bool {
        let mut surface = surface.clone();
        for _ in 0..32 {
            if let Some(parent) = get_parent(&surface) {
                surface = parent;
            } else {
                break;
            }
        }
        self.state.popups.surfaces.contains_key(&surface)
    }

    pub fn dismiss_popup_grab(&mut self) {
        let root = self
            .state
            .popups
            .grab
            .take()
            .and_then(|mut grab| grab.ungrab(PopupUngrabStrategy::All));
        self.state.popups.root = None;
        if let Some(keyboard) = self.state.seat.get_keyboard() {
            keyboard.unset_grab(&mut self.state);
        }
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.unset_grab(&mut self.state, serial, 0);
        }
        if let Some(root) = root {
            self.focus_surface(Some(root), serial);
        }
    }
}

fn valid_positioner(positioner: PositionerState) -> bool {
    [
        positioner.rect_size.w,
        positioner.rect_size.h,
        positioner.anchor_rect.size.w,
        positioner.anchor_rect.size.h,
    ]
    .iter()
    .all(|value| (1..=16384).contains(value))
        && [
            positioner.anchor_rect.loc.x,
            positioner.anchor_rect.loc.y,
            positioner.offset.x,
            positioner.offset.y,
        ]
        .iter()
        .all(|value| (-32768..=32768).contains(value))
        && positioner
            .parent_size
            .is_none_or(|size| (1..=16384).contains(&size.w) && (1..=16384).contains(&size.h))
}

fn valid_mapping(mapping: &lince_interface::host::ContentMapping) -> bool {
    mapping
        .canvas
        .origin()
        .iter()
        .chain(mapping.canvas.size().iter())
        .all(|value| value.abs() <= 1_048_576.0)
}
