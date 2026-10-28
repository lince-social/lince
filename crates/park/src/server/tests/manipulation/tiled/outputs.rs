use super::*;
use lince_interface::host::Direction;
use smithay::backend::winit::WinitEvent;

pub(super) const COLOR: [u8; 4] = [220, 180, 20, 255];

pub(super) struct Companion {
    pub(super) window: Window,
    pub(super) id: SurfaceId,
    pub(super) placement: lince_interface::layout::desktop::PlacementId,
    size: [u32; 2],
    geometry: Option<ItemGeometry>,
}

impl Companion {
    pub(super) fn verify(
        &mut self,
        frame: &crate::presentation::Frame,
        output: &OutputViewport,
        pixels: &[u8],
    ) {
        assert_eq!(output.id, OutputId(1));
        let item = frame
            .geometry
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.id == ItemId::Application(self.id))
            .unwrap();
        assert_eq!(item.output, output.id);
        if let Some(saved) = &self.geometry {
            assert_eq!(
                item, saved,
                "A target gesture changed the companion window's geometry"
            );
        } else {
            self.geometry = Some(item.clone());
        }
        let window = frame
            .windows
            .iter()
            .find(|window| window.surface == self.id)
            .unwrap();
        assert_eq!(window.size, self.size);
        assert!(!window.state.resizing);
        let point = output.raster.point(interior(item, output.bounds).unwrap());
        let offset = (point[1] as usize * output.physical_size[0] as usize + point[0] as usize) * 4;
        let expected = [COLOR[2], COLOR[1], COLOR[0]];
        assert!(
            (0..3).all(|axis| pixels[offset + axis].abs_diff(expected[axis]) < 20),
            "The companion window lost its pixels: {:?}",
            &pixels[offset..offset + 4]
        );
    }
}

pub(super) fn configure(fixture: &mut Fixture) {
    let identities = fixture.identities();
    fixture
        .desktop
        .server
        .configure_outputs(&[
            Monitor {
                id: OutputId(1),
                name: "Tiled companion".into(),
                origin: [-720, -64],
                size: [640, 480],
            },
            Monitor {
                id: OutputId(2),
                name: "Tiled destination".into(),
                origin: [96, 64],
                size: fixture.size.map(|value| value as i32),
            },
        ])
        .unwrap();
    fixture.desktop.invalidate_outputs().unwrap();
    fixture.desktop.dispatch().unwrap();
    fixture.flush();
    fixture.desktop.event(WinitEvent::Focus(true)).unwrap();
    fixture.flush();
    fixture.action(DesktopAction::MoveRowToOutput(Direction::Right));
    fixture.output = OutputId(2);
    fixture.configure();
    let current = fixture.desktop.current.as_ref().unwrap();
    assert_eq!(current.frame.output, Some(OutputId(2)));
    assert_eq!(current.frame.geometry.as_ref().unwrap().outputs.len(), 2);
    assert!(
        current
            .frame
            .outputs
            .iter()
            .any(|output| output.id == OutputId(1))
    );
    for (_, id) in &fixture.clients {
        let target = current
            .frame
            .input
            .pointer_targets
            .iter()
            .find(|target| target.item == ItemId::Application(*id))
            .unwrap();
        assert_eq!(target.output, OutputId(2));
    }
    assert_eq!(fixture.viewport().bounds.origin(), [96.0, 64.0]);
    assert_eq!(fixture.identities(), identities);
    let known: Vec<_> = fixture
        .desktop
        .server
        .windows()
        .iter()
        .map(|window| window.id)
        .collect();
    let qh = fixture.harness.queue.handle();
    let surface = fixture
        .harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = fixture
        .harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let role = xdg.get_toplevel(&qh, ());
    role.set_title("A real Wayland window".into());
    role.set_app_id("social.lince.park.test".into());
    role.set_min_size(120, 80);
    role.set_max_size(1200, 900);
    surface.commit();
    fixture.sync();
    fixture.sync();
    let window = Window { surface, xdg, role };
    let buffer = colored_buffer(&fixture.harness, [180, 160], COLOR);
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.commit();
    fixture.sync();
    fixture.desktop.dispatch().unwrap();
    fixture.flush();
    let id = fixture
        .desktop
        .server
        .windows()
        .iter()
        .find(|window| !known.contains(&window.id))
        .unwrap()
        .id;
    let serial = fixture.desktop.worker.input(Input::Activate(id)).unwrap();
    advance(&mut fixture.desktop, serial);
    fixture.action(DesktopAction::MoveToOutput(Direction::Left));
    requests::configure_client_color(
        &mut fixture.desktop,
        &mut fixture.harness,
        &window,
        id,
        COLOR,
    );
    let current = fixture.desktop.current.as_ref().unwrap();
    assert_eq!(current.frame.output, Some(OutputId(1)));
    let target = current
        .frame
        .input
        .pointer_targets
        .iter()
        .find(|target| target.item == ItemId::Application(id))
        .unwrap();
    assert_eq!(target.output, OutputId(1));
    let placement = target.placement;
    let size = current
        .frame
        .windows
        .iter()
        .find(|window| window.surface == id)
        .unwrap()
        .size;
    fixture.companion = Some(Companion {
        window,
        id,
        placement,
        size,
        geometry: None,
    });
    fixture.activate(0);
    fixture.configure();
    assert_eq!(
        fixture.desktop.current.as_ref().unwrap().frame.output,
        Some(OutputId(2))
    );
    let expanded = fixture.identities();
    assert_eq!(expanded.len(), 4);
    for (placement, uid) in identities {
        assert_eq!(expanded.get(&placement), Some(&uid));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn offset_secondary_output_preserves_partial_tile_claims_destination_pixels_and_input() {
    super::csd(false, true);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn offset_secondary_output_preserves_width_row_transactions_and_per_client_resize_state() {
    super::resize(false, true);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn perspective_secondary_output_preserves_partial_tile_claims_destination_pixels_and_input() {
    super::csd(true, true);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn perspective_secondary_output_preserves_width_row_transactions_and_per_client_resize_state() {
    super::resize(true, true);
}
