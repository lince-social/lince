use super::*;
use std::{os::fd::AsFd, time::Duration};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_region,
        wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
    },
};
use wayland_protocols::xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1};
use wayland_protocols::xdg::decoration::zv1::client::{
    zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1,
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Default)]
struct Client {
    compositor: Option<wl_compositor::WlCompositor>,
    shell: Option<xdg_wm_base::XdgWmBase>,
    shm: Option<wl_shm::WlShm>,
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    layer_configures: Vec<[u32; 2]>,
    configures: Vec<[i32; 2]>,
    output: Option<wl_output::WlOutput>,
    outputs: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    output_name: String,
    output_size: [i32; 2],
    window_states: Vec<Vec<u32>>,
    serials: usize,
    releases: usize,
    closed: bool,
    ignore_configure: bool,
    decorations: Option<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1>,
    decoration_modes: Vec<zxdg_toplevel_decoration_v1::Mode>,
    keys: Vec<(u32, bool)>,
    seat: Option<wl_seat::WlSeat>,
    key_serial: u32,
    pointer_serial: u32,
    popup_configures: Vec<[i32; 4]>,
    popup_repositions: Vec<u32>,
    popup_done: usize,
    activation: Option<xdg_activation_v1::XdgActivationV1>,
    tokens: Vec<String>,
}

struct Harness {
    server: Server,
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
}

struct Window {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    role: xdg_toplevel::XdgToplevel,
}

impl Harness {
    fn new() -> Self {
        Self::connect(Server::new([640, 480]).unwrap())
    }

    fn connect(mut server: Server) -> Self {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        server.insert_client(server_stream).unwrap();
        let connection = Connection::from_socket(client_stream).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut harness = Self {
            server,
            connection,
            queue,
            client: Client::default(),
        };
        harness.sync();
        harness
    }

    fn sync(&mut self) {
        synchronize(
            &mut self.server,
            &self.connection,
            &mut self.queue,
            &mut self.client,
        )
        .unwrap();
    }

    fn window(&mut self) -> Window {
        let qh = self.queue.handle();
        let surface = self
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        let xdg = self
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
        surface.attach(None, 0, 0);
        surface.commit();
        self.sync();
        self.sync();
        Window { surface, xdg, role }
    }

    fn attach(&mut self, window: &Window, size: [i32; 2]) -> wl_buffer::WlBuffer {
        let buffer = self.buffer(size);
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, size[0], size[1]);
        window.surface.commit();
        self.sync();
        buffer
    }

    fn buffer(&self, size: [i32; 2]) -> wl_buffer::WlBuffer {
        let file = tempfile::tempfile().unwrap();
        file.set_len((size[0] * size[1] * 4) as u64).unwrap();
        let qh = self.queue.handle();
        let pool = self.client.shm.as_ref().unwrap().create_pool(
            file.as_fd(),
            size[0] * size[1] * 4,
            &qh,
            (),
        );
        let buffer = pool.create_buffer(
            0,
            size[0],
            size[1],
            size[0] * 4,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );
        pool.destroy();
        buffer
    }
}

fn synchronize(
    server: &mut Server,
    connection: &Connection,
    queue: &mut EventQueue<Client>,
    client: &mut Client,
) -> Result<(), String> {
    let done = Arc::new(AtomicBool::new(false));
    connection.display().sync(&queue.handle(), done.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            return Err("the Wayland client timed out".into());
        }
        connection.flush().map_err(|error| error.to_string())?;
        server.dispatch().map_err(|error| error.to_string())?;
        queue
            .dispatch_pending(client)
            .map_err(|error| error.to_string())?;
        if let Some(read) = queue.prepare_read() {
            let fd = read.connection_fd();
            let mut poll = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN,
            )];
            let timeout = rustix::time::Timespec {
                tv_sec: 0,
                tv_nsec: 1_000_000,
            };
            rustix::event::poll(&mut poll, Some(&timeout)).map_err(|error| error.to_string())?;
            if poll[0].revents().contains(rustix::event::PollFlags::IN) {
                read.read().map_err(|error| error.to_string())?;
            }
        }
        queue
            .dispatch_pending(client)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "xdg_wm_base" => state.shell = Some(registry.bind(name, version.min(6), qh, ())),
                "xdg_activation_v1" => state.activation = Some(registry.bind(name, 1, qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "wl_seat" => {
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(5), qh, ());
                    seat.get_keyboard(qh, ());
                    seat.get_pointer(qh, ());
                    state.seat = Some(seat);
                }
                "wl_subcompositor" => state.subcompositor = Some(registry.bind(name, 1, qh, ())),
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "zxdg_decoration_manager_v1" => {
                    state.decorations = Some(registry.bind(name, 1, qh, ()))
                }
                "wl_output" => state.output = Some(registry.bind(name, version.min(4), qh, ())),
                "zxdg_output_manager_v1" => {
                    state.outputs = Some(registry.bind(name, version.min(3), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, Arc<AtomicBool>> for Client {
    fn event(
        _: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        done: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        done.store(true, Ordering::Release);
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
    fn event(
        _: &mut Self,
        proxy: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            proxy.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for Client {
    fn event(
        state: &mut Self,
        proxy: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            state.serials += 1;
            if !state.ignore_configure {
                proxy.ack_configure(serial);
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for Client {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                state.configures.push([width, height]);
                state.window_states.push(
                    states
                        .chunks_exact(4)
                        .map(|value| u32::from_ne_bytes(value.try_into().unwrap()))
                        .collect(),
                );
            }
            xdg_toplevel::Event::Close => state.closed = true,
            _ => {}
        }
    }
}

#[test]
fn client_window_state_requests_are_coalesced_until_map_and_confirmed_by_policy() {
    let mut harness = Harness::new();
    let window = harness.window();
    window.role.set_maximized();
    window.role.unset_maximized();
    window.role.set_maximized();
    window.role.set_fullscreen(None);
    window.role.unset_fullscreen();
    harness.sync();
    assert!(harness.server.requests().is_empty());
    let _buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    assert_eq!(
        harness.server.requests(),
        [(
            id,
            WindowRequest {
                maximized: Some(true),
                fullscreen: Some(false)
            }
        )]
    );
    assert!(harness.server.requests().is_empty());
    harness
        .server
        .configure_state(
            id,
            [620, 440],
            WindowState {
                maximized: true,
                fullscreen: false,
            },
        )
        .unwrap();
    harness.sync();
    assert_eq!(harness.client.configures.last(), Some(&[620, 440]));
    assert!(
        harness
            .client
            .window_states
            .last()
            .unwrap()
            .contains(&(xdg_toplevel::State::Maximized as u32))
    );
    assert!(
        !harness
            .client
            .window_states
            .last()
            .unwrap()
            .contains(&(xdg_toplevel::State::Fullscreen as u32))
    );
    let serials = harness.client.serials;
    harness
        .server
        .configure_state(
            id,
            [620, 440],
            WindowState {
                maximized: true,
                fullscreen: false,
            },
        )
        .unwrap();
    harness.sync();
    assert_eq!(harness.client.serials, serials);
    window.role.set_maximized();
    harness.sync();
    assert_eq!(harness.server.requests().len(), 1);
    harness
        .server
        .configure_state(
            id,
            [620, 440],
            WindowState {
                maximized: true,
                fullscreen: false,
            },
        )
        .unwrap();
    harness.sync();
    assert_eq!(harness.client.serials, serials + 1);
    window.role.set_fullscreen(None);
    harness.sync();
    assert_eq!(
        harness.server.requests(),
        [(
            id,
            WindowRequest {
                maximized: None,
                fullscreen: Some(true)
            }
        )]
    );
    harness
        .server
        .configure_state(
            id,
            [640, 480],
            WindowState {
                maximized: false,
                fullscreen: true,
            },
        )
        .unwrap();
    harness.sync();
    assert!(
        harness
            .client
            .window_states
            .last()
            .unwrap()
            .contains(&(xdg_toplevel::State::Fullscreen as u32))
    );
    window.role.unset_fullscreen();
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    assert!(harness.server.requests().is_empty());
}

impl Dispatch<wl_buffer::WlBuffer, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            state.releases += 1;
        }
    }
}

delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(Client: ignore wl_subsurface::WlSubsurface);
delegate_noop!(Client: ignore wl_seat::WlSeat);
delegate_noop!(Client: ignore wl_region::WlRegion);
delegate_noop!(Client: ignore xdg_positioner::XdgPositioner);
delegate_noop!(Client: ignore xdg_activation_v1::XdgActivationV1);
delegate_noop!(Client: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(Client: ignore wl_output::WlOutput);
delegate_noop!(Client: ignore zxdg_output_manager_v1::ZxdgOutputManagerV1);

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, ()> for Client {
    fn event(
        state: &mut Self,
        _: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zxdg_output_v1::Event::Name { name } => state.output_name = name,
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                state.output_size = [width, height]
            }
            _ => {}
        }
    }
}

#[test]
fn external_bars_receive_output_identity_and_logical_resize_information() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let _output = harness.client.outputs.as_ref().unwrap().get_xdg_output(
        harness.client.output.as_ref().unwrap(),
        &qh,
        (),
    );
    harness.sync();
    assert_eq!(harness.client.output_name, "Park");
    assert_eq!(harness.client.output_size, [640, 480]);
    harness.server.resize([800, 600]).unwrap();
    harness.sync();
    assert_eq!(harness.client.output_size, [800, 600]);
    assert_eq!(harness.client.output_name, "Park");
}

#[test]
fn bars_reserve_only_the_requested_output_and_close_on_disconnect() {
    let mut harness = Harness::new();
    let monitors = [
        Monitor {
            id: OutputId(1),
            name: "Park".into(),
            origin: [0, 0],
            size: [640, 480],
        },
        Monitor {
            id: OutputId(2),
            name: "DP-1".into(),
            origin: [640, 0],
            size: [800, 600],
        },
    ];
    harness.server.configure_outputs(&monitors).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let role = harness
        .client
        .layer_shell
        .as_ref()
        .unwrap()
        .get_layer_surface(
            &surface,
            harness.client.output.as_ref(),
            zwlr_layer_shell_v1::Layer::Top,
            "second-output-bar".into(),
            &qh,
            (),
        );
    role.set_size(0, 30);
    role.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    role.set_exclusive_zone(30);
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.layer_configures.last(), Some(&[800, 30]));
    let buffer = harness.buffer([800, 30]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    assert_eq!(
        harness.server.usable_output(OutputId(1)),
        Some(Rectangle::from_size((640, 480).into()))
    );
    assert_eq!(
        harness.server.usable_output(OutputId(2)),
        Some(Rectangle::new((0, 30).into(), (800, 570).into()))
    );
    let layers = harness.server.layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].output, OutputId(2));
    assert_eq!(layers[0].origin, [640, 0]);
    harness.server.configure_outputs(&monitors[..1]).unwrap();
    harness.sync();
    assert!(harness.server.layers().is_empty());
    assert_eq!(harness.server.output_viewports().len(), 1);
}

#[test]
fn invalid_output_configurations_leave_the_active_session_unchanged() {
    let mut server = Server::new([640, 480]).unwrap();
    let before = server.output_viewports();
    let invalid = Monitor {
        id: OutputId(1),
        name: "Park".into(),
        origin: [i32::MAX, 0],
        size: [640, 480],
    };
    assert!(server.configure_outputs(&[invalid]).is_err());
    assert_eq!(server.output_viewports(), before);
}
delegate_noop!(Client: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);

impl Dispatch<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1, ()> for Client {
    fn event(
        state: &mut Self,
        _: &zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_toplevel_decoration_v1::Event::Configure {
            mode: wayland_client::WEnum::Value(mode),
        } = event
        {
            state.decoration_modes.push(mode);
        }
    }
}

#[test]
fn decoration_negotiation_preserves_client_controls_and_initial_configure_order() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let role = xdg.get_toplevel(&qh, ());
    let decoration = harness
        .client
        .decorations
        .as_ref()
        .unwrap()
        .get_toplevel_decoration(&role, &qh, ());
    decoration.set_mode(zxdg_toplevel_decoration_v1::Mode::ServerSide);
    harness.sync();
    assert_eq!(harness.client.serials, 0);
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.serials, 1);
    assert_eq!(
        harness.client.decoration_modes,
        [zxdg_toplevel_decoration_v1::Mode::ClientSide]
    );
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Client {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            proxy.ack_configure(serial);
            state.layer_configures.push([width, height]);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key {
            key,
            serial,
            state: key_state,
            ..
        } = event
        {
            state.key_serial = serial;
            state.keys.push((
                key,
                key_state == wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed),
            ));
        }
    }
}

impl Dispatch<xdg_popup::XdgPopup, ()> for Client {
    fn event(
        state: &mut Self,
        _: &xdg_popup::XdgPopup,
        event: xdg_popup::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_popup::Event::Configure {
                x,
                y,
                width,
                height,
            } => state.popup_configures.push([x, y, width, height]),
            xdg_popup::Event::Repositioned { token } => state.popup_repositions.push(token),
            xdg_popup::Event::PopupDone => state.popup_done += 1,
            _ => {}
        }
    }
}

#[test]
fn popup_configure_reposition_mapping_and_authorized_grab_follow_the_parent() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(160, 80);
    positioner.set_anchor_rect(620, 460, 1, 1);
    positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
    positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
    positioner.set_constraint_adjustment(
        xdg_positioner::ConstraintAdjustment::SlideX | xdg_positioner::ConstraintAdjustment::SlideY,
    );
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    popup.grab(
        harness.client.seat.as_ref().unwrap(),
        harness.client.key_serial,
    );
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.popup_done, 0);
    assert_eq!(
        harness.client.popup_configures.last(),
        Some(&[480, 400, 160, 80])
    );
    assert!(harness.server.popup_grab_active());
    let buffer = harness.buffer([160, 80]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    assert_eq!(harness.server.windows()[0].popups.len(), 1);
    positioner.set_anchor_rect(10, 20, 1, 1);
    popup.reposition(&positioner, 77);
    harness.sync();
    harness.sync();
    surface.commit();
    harness.sync();
    assert_eq!(harness.client.popup_repositions, [77]);
    assert_eq!(
        harness.client.popup_configures.last(),
        Some(&[11, 21, 160, 80])
    );
    assert_eq!(harness.server.windows()[0].popups[0].origin, [11, 21]);
    harness.server.dismiss_popup_grab();
    harness.sync();
    assert!(!harness.server.popup_grab_active());
    assert_eq!(harness.client.popup_done, 1);
}

#[test]
fn a_popup_cannot_grab_focus_with_an_unrelated_serial() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(100, 50);
    positioner.set_anchor_rect(0, 0, 1, 1);
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    popup.grab(harness.client.seat.as_ref().unwrap(), 0);
    surface.commit();
    harness.sync();
    assert_eq!(harness.client.popup_done, 1);
    assert!(!harness.server.popup_grab_active());
    assert_eq!(harness.server.windows().len(), 1);
    assert!(harness.client.popup_configures.is_empty());
    assert!(harness.server.state.popups.surfaces.is_empty());
}

#[test]
fn extreme_positioners_disconnect_only_the_malformed_client() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    for geometry in [[16385, 50, 0, 0], [100, 50, i32::MAX, i32::MIN]] {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        harness.server.insert_client(server_stream).unwrap();
        let connection = Connection::from_socket(client_stream).unwrap();
        let mut queue = connection.new_event_queue();
        let mut client = Client::default();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
        let positioner = client.shell.as_ref().unwrap().create_positioner(&qh, ());
        positioner.set_size(geometry[0], geometry[1]);
        positioner.set_anchor_rect(geometry[2], geometry[3], 1, 1);
        let surface = client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let xdg = client
            .shell
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &qh, ());
        let _popup = xdg.get_popup(None, &positioner, &qh, ());
        surface.commit();
        assert!(synchronize(&mut harness.server, &connection, &mut queue, &mut client).is_err());
        let _next = harness.attach(&window, [400, 300]);
        assert_eq!(
            harness.server.windows()[0].nodes[0].content.size,
            [400, 300]
        );
        assert_eq!(harness.server.clients.load(Ordering::Acquire), 1);
    }
}

#[test]
fn extreme_window_and_subsurface_coordinates_do_not_escape_client_isolation() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    for subsurface in [false, true] {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        harness.server.insert_client(server_stream).unwrap();
        let connection = Connection::from_socket(client_stream).unwrap();
        let mut queue = connection.new_event_queue();
        let mut client = Client::default();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
        let surface = client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let xdg = client
            .shell
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &qh, ());
        let _role = xdg.get_toplevel(&qh, ());
        surface.commit();
        synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
        if subsurface {
            let child = client.compositor.as_ref().unwrap().create_surface(&qh, ());
            let role =
                client
                    .subcompositor
                    .as_ref()
                    .unwrap()
                    .get_subsurface(&child, &surface, &qh, ());
            role.set_position(i32::MAX, i32::MIN);
            let file = tempfile::tempfile().unwrap();
            file.set_len(16).unwrap();
            let pool = client
                .shm
                .as_ref()
                .unwrap()
                .create_pool(file.as_fd(), 16, &qh, ());
            let buffer = pool.create_buffer(0, 2, 2, 8, wl_shm::Format::Argb8888, &qh, ());
            child.attach(Some(&buffer), 0, 0);
            child.commit();
            surface.commit();
        } else {
            xdg.set_window_geometry(i32::MAX, i32::MIN, 100, 100);
            surface.commit();
        }
        assert!(synchronize(&mut harness.server, &connection, &mut queue, &mut client).is_err());
        let _next = harness.attach(&window, [400, 300]);
        assert_eq!(harness.server.windows().len(), 1);
        assert_eq!(
            harness.server.windows()[0].nodes[0].content.size,
            [400, 300]
        );
    }
}

#[test]
fn slow_popup_buffers_do_not_block_other_windows_and_parent_unmap_dismisses_them() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(100, 50);
    positioner.set_anchor_rect(20, 20, 1, 1);
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let _popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    surface.commit();
    harness.sync();
    assert_eq!(harness.client.popup_configures.len(), 1);
    let other = harness.window();
    let _other_buffer = harness.attach(&other, [400, 300]);
    assert_eq!(
        harness
            .server
            .windows()
            .iter()
            .filter(|window| window.mapped)
            .count(),
        2
    );
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    assert_eq!(harness.client.popup_done, 1);
    assert!(harness.server.state.popups.surfaces.is_empty());
    assert_eq!(
        harness
            .server
            .windows()
            .iter()
            .filter(|window| window.mapped)
            .count(),
        1
    );
}

#[test]
fn popup_parent_chains_are_bounded_before_manager_traversal() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(100, 50);
    positioner.set_anchor_rect(0, 0, 1, 1);
    let mut parent = window.xdg.clone();
    let mut popups = Vec::new();
    for _ in 0..35 {
        let surface = harness
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        let xdg = harness
            .client
            .shell
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &qh, ());
        let popup = xdg.get_popup(Some(&parent), &positioner, &qh, ());
        surface.commit();
        harness.sync();
        parent = xdg.clone();
        popups.push((surface, xdg, popup));
    }
    assert_eq!(harness.server.state.popups.surfaces.len(), 32);
    assert_eq!(harness.client.popup_done, 3);
    assert_eq!(harness.client.popup_configures.len(), 32);
    let _next = harness.attach(&window, [400, 300]);
    assert_eq!(
        harness.server.windows()[0].nodes[0].content.size,
        [400, 300]
    );
}

#[test]
fn another_client_cannot_use_a_window_owners_input_serial() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let stolen_serial = harness.client.key_serial;
    let (server_stream, client_stream) = UnixStream::pair().unwrap();
    harness.server.insert_client(server_stream).unwrap();
    let connection = Connection::from_socket(client_stream).unwrap();
    let mut queue = connection.new_event_queue();
    let mut client = Client::default();
    connection.display().get_registry(&queue.handle(), ());
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    let qh = queue.handle();
    let request = client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(stolen_serial, client.seat.as_ref().unwrap());
    request.commit();
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    assert_eq!(client.tokens.len(), 1);
    assert_eq!(harness.server.state.activation.tokens().count(), 0);
    assert!(harness.server.activations().is_empty());
    assert_eq!(harness.server.windows().len(), 1);
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for Client {
    fn event(
        state: &mut Self,
        _: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            state.tokens.push(token);
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Button { serial, .. } = event {
            state.pointer_serial = serial;
        }
    }
}

#[test]
fn layer_shell_popup_grabs_use_the_bar_click_without_keyboard_interactivity() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let layer = harness
        .client
        .layer_shell
        .as_ref()
        .unwrap()
        .get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            "popup-bar".into(),
            &qh,
            (),
        );
    layer.set_size(0, 30);
    layer.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    surface.commit();
    harness.sync();
    harness.sync();
    let buffer = harness.buffer([640, 30]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    let server_surface = harness.server.layers()[0].surface.clone();
    harness
        .server
        .pointer_motion(Some((server_surface, [12.0, 28.0])), 1)
        .unwrap();
    harness.server.pointer_button(272, true, 2);
    harness.sync();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(120, 80);
    positioner.set_anchor_rect(12, 28, 1, 1);
    positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
    let popup_surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&popup_surface, &qh, ());
    let popup = xdg.get_popup(None, &positioner, &qh, ());
    layer.get_popup(&popup);
    popup.grab(
        harness.client.seat.as_ref().unwrap(),
        harness.client.pointer_serial,
    );
    popup_surface.commit();
    harness.sync();
    harness.sync();
    assert!(harness.server.popup_grab_active());
    assert_eq!(harness.client.popup_done, 0);
    let buffer = harness.buffer([120, 80]);
    popup_surface.attach(Some(&buffer), 0, 0);
    popup_surface.commit();
    harness.sync();
    let layers = harness.server.layers();
    assert_eq!(layers[0].popups.len(), 1);
    let mapping = layers[0].popups[0].layer_mapping([0.0; 2]).unwrap();
    assert!(mapping.canvas.origin()[1] + mapping.canvas.size()[1] > 30.0);
}

#[test]
fn activation_requires_recent_owned_input_and_tokens_are_single_use() {
    let mut harness = Harness::new();
    let source = harness.window();
    let _source_buffer = harness.attach(&source, [640, 480]);
    let target = harness.window();
    let _target_buffer = harness.attach(&target, [640, 480]);
    let windows = harness.server.windows();
    harness.server.focus(
        Some(windows[0].id),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(
        harness.client.key_serial,
        harness.client.seat.as_ref().unwrap(),
    );
    request.set_surface(&source.surface);
    request.commit();
    harness.sync();
    let token = harness.client.tokens.last().unwrap().clone();
    harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .activate(token.clone(), &target.surface);
    harness.sync();
    assert_eq!(harness.server.activations(), [windows[1].id]);
    harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .activate(token, &target.surface);
    harness.sync();
    assert!(harness.server.activations().is_empty());
    let invalid = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    invalid.set_serial(0, harness.client.seat.as_ref().unwrap());
    invalid.set_surface(&source.surface);
    invalid.commit();
    harness.sync();
    harness.client.activation.as_ref().unwrap().activate(
        harness.client.tokens.last().unwrap().clone(),
        &target.surface,
    );
    harness.sync();
    assert!(harness.server.activations().is_empty());
}

#[test]
fn newer_native_input_revokes_activation_and_surface_hint_is_optional() {
    let mut harness = Harness::new();
    let source = harness.window();
    let _source_buffer = harness.attach(&source, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(
        harness.client.key_serial,
        harness.client.seat.as_ref().unwrap(),
    );
    request.commit();
    harness.sync();
    harness.client.activation.as_ref().unwrap().activate(
        harness.client.tokens.last().unwrap().clone(),
        &source.surface,
    );
    harness.sync();
    assert_eq!(harness.server.activations(), [id]);
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(
        harness.client.key_serial,
        harness.client.seat.as_ref().unwrap(),
    );
    request.commit();
    harness.sync();
    harness.server.native_input();
    harness.client.activation.as_ref().unwrap().activate(
        harness.client.tokens.last().unwrap().clone(),
        &source.surface,
    );
    harness.sync();
    assert!(harness.server.activations().is_empty());
}

#[test]
fn activation_waits_for_map_and_expired_tokens_or_pending_requests_are_discarded() {
    let mut harness = Harness::new();
    let source = harness.window();
    let _buffer = harness.attach(&source, [640, 480]);
    let target = harness.window();
    let ids: Vec<_> = harness
        .server
        .windows()
        .iter()
        .map(|window| window.id)
        .collect();
    harness
        .server
        .focus(Some(ids[0]), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(
        harness.client.key_serial,
        harness.client.seat.as_ref().unwrap(),
    );
    request.commit();
    harness.sync();
    let (token, mut data) = harness
        .server
        .state
        .activation
        .tokens()
        .next()
        .map(|(token, data)| (token.clone(), data.clone()))
        .unwrap();
    harness.server.state.activation.remove_token(&token);
    data.timestamp = Instant::now() - Duration::from_secs(6);
    let expired = harness
        .server
        .state
        .activation
        .create_external_token(data)
        .0
        .to_string();
    harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .activate(expired, &target.surface);
    harness.sync();
    assert!(harness.server.activations().is_empty());
    assert!(harness.server.state.activations.is_empty());
    for expired in [true, false] {
        let request = harness
            .client
            .activation
            .as_ref()
            .unwrap()
            .get_activation_token(&qh, ());
        request.set_serial(
            harness.client.key_serial,
            harness.client.seat.as_ref().unwrap(),
        );
        request.commit();
        harness.sync();
        harness.client.activation.as_ref().unwrap().activate(
            harness.client.tokens.last().unwrap().clone(),
            &target.surface,
        );
        harness.sync();
        assert!(harness.server.activations().is_empty());
        assert_eq!(harness.server.state.activations.len(), 1);
        if expired {
            harness.server.state.activations.front_mut().unwrap().1 =
                Instant::now() - Duration::from_secs(6);
            assert!(harness.server.activations().is_empty());
            assert!(harness.server.state.activations.is_empty());
        }
    }
    let _target_buffer = harness.attach(&target, [400, 300]);
    assert_eq!(harness.server.activations(), [ids[1]]);
    assert!(harness.server.activations().is_empty());
}

#[test]
fn configure_commit_resize_and_buffer_lifetime() {
    let mut harness = Harness::new();
    let window = harness.window();
    assert_eq!(harness.client.configures, [[640, 480]]);
    assert_eq!(harness.client.serials, 1);
    let before = harness.server.windows().remove(0);
    assert!(!before.mapped);
    assert_eq!(before.title, "A real Wayland window");
    assert_eq!(before.app_id, "social.lince.park.test");
    assert_eq!(before.minimum, [120, 80]);
    assert_eq!(before.maximum, [1200, 900]);
    let _first = harness.attach(&window, [640, 480]);
    let snapshot = harness.server.windows().remove(0);
    assert!(snapshot.mapped);
    assert_eq!(snapshot.nodes[0].content.size, [640, 480]);
    assert_eq!(harness.client.releases, 0);
    harness.server.configure(before.id, [320, 240]).unwrap();
    harness.server.configure(before.id, [320, 240]).unwrap();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.configures, [[640, 480], [320, 240]]);
    let _second = harness.attach(&window, [320, 240]);
    assert_eq!(harness.client.releases, 0);
    snapshot.nodes[0].content.release();
    snapshot.nodes[0].content.release();
    drop(snapshot);
    harness.sync();
    assert_eq!(harness.client.releases, 1);
    let current = harness.server.windows().remove(0);
    current.nodes[0].content.release();
    harness.sync();
    assert_eq!(harness.client.releases, 2);
    harness.server.close(before.id);
    harness.sync();
    assert!(harness.client.closed);
}

#[test]
fn unmap_remap_keeps_role_identity_and_destruction_discards_it() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _first = harness.attach(&window, [640, 480]);
    let first = harness.server.windows().remove(0).id;
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    let unmapped = harness.server.windows().remove(0);
    assert_eq!(unmapped.id, first);
    assert!(!unmapped.mapped);
    assert!(unmapped.nodes.is_empty());
    window.surface.commit();
    harness.sync();
    harness.sync();
    let _second = harness.attach(&window, [640, 480]);
    let remapped = harness.server.windows().remove(0);
    assert_eq!(remapped.id, first);
    assert!(remapped.mapped);
    window.role.destroy();
    window.xdg.destroy();
    window.surface.destroy();
    harness.sync();
    assert!(harness.server.windows().is_empty());
    let _new = harness.window();
    assert_ne!(harness.server.windows().remove(0).id, first);
}

#[test]
fn disconnect_reclaims_registry_and_buffer_leases() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let held = harness.server.windows().remove(0);
    let Harness {
        mut server,
        connection,
        queue,
        client,
    } = harness;
    drop((window, connection, queue, client));
    for _ in 0..8 {
        server.dispatch().unwrap();
    }
    assert!(server.windows().is_empty());
    assert!(server.state.surfaces.is_empty());
    assert!(server.state.buffers.is_empty());
    assert_eq!(server.clients.load(Ordering::Acquire), 0);
    drop(held);
}

#[test]
fn repeated_buffer_commits_release_only_after_every_gpu_use_finishes() {
    let mut harness = Harness::new();
    let window = harness.window();
    let buffer = harness.attach(&window, [640, 480]);
    let first = harness.server.windows().remove(0);
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.commit();
    harness.sync();
    let second = harness.server.windows().remove(0);
    first.nodes[0].content.release();
    harness.sync();
    assert_eq!(harness.client.releases, 0);
    second.nodes[0].content.release();
    harness.sync();
    assert_eq!(harness.client.releases, 1);
    drop((first, second));
    harness.sync();
    assert_eq!(harness.client.releases, 1);
}

#[test]
fn synchronized_subsurface_commits_wait_for_parent_and_keep_offsets() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _root_buffer = harness.attach(&window, [640, 480]);
    let qh = harness.queue.handle();
    let child = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let sub = harness
        .client
        .subcompositor
        .as_ref()
        .unwrap()
        .get_subsurface(&child, &window.surface, &qh, ());
    sub.set_position(18, -12);
    let buffer = harness.buffer([80, 60]);
    child.attach(Some(&buffer), 0, 0);
    child.commit();
    harness.sync();
    assert_eq!(harness.server.windows()[0].nodes.len(), 1);
    window.surface.commit();
    harness.sync();
    let snapshot = harness.server.windows().remove(0);
    assert_eq!(snapshot.nodes.len(), 2);
    assert_eq!(snapshot.nodes[0].origin, [18, -12]);
    assert_eq!(snapshot.nodes[0].content.size, [80, 60]);
    let frame = Arc::new(AtomicBool::new(false));
    child.frame(&qh, frame.clone());
    child.commit();
    window.surface.commit();
    harness.sync();
    assert!(!frame.load(Ordering::Acquire));
    harness.server.presented(&[snapshot.id]);
    harness.sync();
    assert!(frame.load(Ordering::Acquire));
}

#[test]
fn an_invalid_client_does_not_disconnect_an_unrelated_window() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let good = harness.server.windows().remove(0).id;
    let (server_stream, client_stream) = UnixStream::pair().unwrap();
    harness.server.insert_client(server_stream).unwrap();
    let connection = Connection::from_socket(client_stream).unwrap();
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    let mut client = Client {
        ignore_configure: true,
        ..Default::default()
    };
    connection.display().get_registry(&qh, ());
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    let surface = client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let _role = xdg.get_toplevel(&qh, ());
    surface.commit();
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    let file = tempfile::tempfile().unwrap();
    file.set_len(16).unwrap();
    let pool = client
        .shm
        .as_ref()
        .unwrap()
        .create_pool(file.as_fd(), 16, &qh, ());
    let buffer = pool.create_buffer(0, 2, 2, 8, wl_shm::Format::Argb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    assert!(synchronize(&mut harness.server, &connection, &mut queue, &mut client).is_err());
    harness.sync();
    let remaining = harness.server.windows();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, good);
    assert!(remaining[0].mapped);
    assert_eq!(harness.server.clients.load(Ordering::Acquire), 1);
}

#[test]
fn surface_mapping_handles_rotation_scale_and_viewport_crop() {
    let turns = [
        (Transform::Normal, [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]),
        (Transform::_90, [1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]),
        (Transform::_180, [1.0, 1.0], [-1.0, 0.0], [0.0, -1.0]),
        (Transform::_270, [0.0, 1.0], [0.0, -1.0], [1.0, 0.0]),
        (Transform::Flipped, [1.0, 0.0], [-1.0, 0.0], [0.0, 1.0]),
        (Transform::Flipped90, [0.0, 0.0], [0.0, 1.0], [1.0, 0.0]),
        (Transform::Flipped180, [0.0, 1.0], [1.0, 0.0], [0.0, -1.0]),
        (Transform::Flipped270, [1.0, 1.0], [0.0, -1.0], [-1.0, 0.0]),
    ];
    for (transform, origin, x, y) in turns {
        let layout = SurfaceLayout::new([240, 160], 2, transform, Default::default()).unwrap();
        assert_eq!(layout.uv_origin, origin);
        assert_eq!(layout.uv_x, x);
        assert_eq!(layout.uv_y, y);
        assert!(layout.contains([0.0, 0.0]));
        assert!(!layout.contains(layout.size));
        let size =
            transform.transform_size(smithay::utils::Size::<_, Logical>::from((120.0, 80.0)));
        assert_eq!(layout.size, [size.w, size.h]);
    }
    let crop = ViewportCachedState {
        src: Some(Rectangle::new((30.0, 10.0).into(), (60.0, 40.0).into())),
        dst: Some((180, 120).into()),
    };
    let layout = SurfaceLayout::new([240, 160], 2, Transform::Normal, crop).unwrap();
    assert_eq!(layout.size, [180.0, 120.0]);
    assert_eq!(layout.uv_origin, [0.25, 0.125]);
    assert_eq!(layout.uv_x, [0.5, 0.0]);
    assert_eq!(layout.uv_y, [0.0, 0.5]);
    assert!(SurfaceLayout::new([60, 40], 2, Transform::Normal, crop).is_none());
}

#[test]
fn layer_shell_bars_own_their_space_and_unmap_restores_it() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let role = harness
        .client
        .layer_shell
        .as_ref()
        .unwrap()
        .get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            "park-test-bar".into(),
            &qh,
            (),
        );
    role.set_size(0, 30);
    role.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    role.set_exclusive_zone(30);
    surface.attach(None, 0, 0);
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.layer_configures, [[640, 30]]);
    let buffer = harness.buffer([640, 30]);
    let canvas_revision = harness.server.window_revision();
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    assert_eq!(harness.server.window_revision(), canvas_revision);
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 30).into(), (640, 450).into())
    );
    assert_eq!(harness.server.layers()[0].origin, [0, 0]);
    role.set_exclusive_zone(42);
    role.set_size(0, 42);
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 42).into(), (640, 438).into())
    );
    harness.server.resize([800, 600]).unwrap();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.layer_configures.last(), Some(&[800, 42]));
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 42).into(), (800, 558).into())
    );
    surface.attach(None, 0, 0);
    surface.commit();
    harness.sync();
    assert!(harness.server.layers().is_empty());
    assert_eq!(
        harness.server.usable(),
        Rectangle::from_size((800, 600).into())
    );
    role.set_size(0, 28);
    role.set_exclusive_zone(28);
    surface.commit();
    harness.sync();
    harness.sync();
    let buffer = harness.buffer([800, 28]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 28).into(), (800, 572).into())
    );
    role.destroy();
    surface.destroy();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::from_size((800, 600).into())
    );
}

#[test]
fn changing_scale_without_attaching_a_buffer_validates_current_dimensions() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [641, 480]);
    window.surface.set_buffer_scale(2);
    window.surface.commit();
    assert!(
        synchronize(
            &mut harness.server,
            &harness.connection,
            &mut harness.queue,
            &mut harness.client
        )
        .is_err()
    );
    harness.server.dispatch().unwrap();
    assert!(harness.server.windows().is_empty());
}

#[test]
fn consumed_desktop_keys_do_not_reach_the_client_or_leave_stuck_modifiers() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.sync();
    harness.server.keyboard(125, true, 1, |_| false).unwrap();
    let event = harness
        .server
        .keyboard(35, true, 2, |event| event.modifiers.logo)
        .unwrap();
    assert!(event.modifiers.logo);
    assert_eq!(event.text.as_deref(), Some("h"));
    harness.server.keyboard(35, false, 3, |_| true).unwrap();
    let event = harness.server.keyboard(125, false, 4, |_| false).unwrap();
    assert!(!event.modifiers.logo);
    harness.server.keyboard(30, true, 5, |_| false).unwrap();
    harness.server.keyboard(30, false, 6, |_| false).unwrap();
    harness.sync();
    assert_eq!(
        harness.client.keys,
        [(125, true), (125, false), (30, true), (30, false)]
    );
    assert!(harness.server.keyboard(768, true, 7, |_| false).is_err());
    harness.server.clear_input(8);
    harness.server.keyboard(48, true, 9, |_| false).unwrap();
    harness.sync();
    assert_eq!(harness.client.keys.len(), 4);
}

#[test]
fn hit_testing_obeys_surface_geometry_subsurface_stacking_and_input_regions() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _root = harness.attach(&window, [640, 480]);
    let qh = harness.queue.handle();
    let compositor = harness.client.compositor.as_ref().unwrap();
    let child = compositor.create_surface(&qh, ());
    let sub = harness
        .client
        .subcompositor
        .as_ref()
        .unwrap()
        .get_subsurface(&child, &window.surface, &qh, ());
    sub.set_position(20, 30);
    let empty = compositor.create_region(&qh, ());
    child.set_input_region(Some(&empty));
    let buffer = harness.buffer([80, 60]);
    child.attach(Some(&buffer), 0, 0);
    child.commit();
    window.xdg.set_window_geometry(10, 20, 600, 400);
    window.surface.commit();
    harness.sync();
    let snapshot = harness.server.windows().remove(0);
    assert_eq!(
        snapshot.bounds(),
        Some(Rectangle::new((10.0, 20.0).into(), (600.0, 400.0).into()))
    );
    assert_eq!(snapshot.hit_content([15.0, 15.0]).unwrap().1, [25.0, 35.0]);
    assert_eq!(
        snapshot.hit_content([15.0, 15.0]).unwrap().0,
        snapshot.surface
    );
    assert!(snapshot.hit_content([600.0, 0.0]).is_none());
    child.set_input_region(None);
    child.commit();
    window.surface.commit();
    harness.sync();
    let snapshot = harness.server.windows().remove(0);
    let (target, point) = snapshot.hit_content([15.0, 15.0]).unwrap();
    assert_ne!(target, snapshot.surface);
    assert_eq!(point, [5.0, 5.0]);
}

#[test]
fn destroying_the_buffer_proxy_keeps_committed_storage_available_until_import() {
    let mut harness = Harness::new();
    let window = harness.window();
    let buffer = harness.attach(&window, [640, 480]);
    buffer.destroy();
    harness.sync();
    let snapshot = harness.server.windows().remove(0);
    let node = &snapshot.nodes[0];
    assert!(!node.content.buffer().is_alive());
    let size = smithay::wayland::shm::with_buffer_contents(node.content.buffer(), |_, _, data| {
        [data.width, data.height]
    })
    .unwrap();
    assert_eq!(size, [640, 480]);
    node.content.release();
    harness.sync();
    assert_eq!(harness.client.releases, 0);
}

#[test]
fn presentation_acknowledges_only_callbacks_belonging_to_the_drawn_snapshot() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let first = Arc::new(AtomicBool::new(false));
    window.surface.frame(&harness.queue.handle(), first.clone());
    window.surface.commit();
    harness.sync();
    let drawn = harness.server.windows().remove(0);
    let second = Arc::new(AtomicBool::new(false));
    window
        .surface
        .frame(&harness.queue.handle(), second.clone());
    window.surface.commit();
    harness.sync();
    let pending = harness.server.windows().remove(0);
    harness.server.presented_nodes(&drawn.nodes);
    harness.sync();
    assert!(first.load(Ordering::Acquire));
    assert!(!second.load(Ordering::Acquire));
    harness.server.presented_nodes(&pending.nodes);
    harness.sync();
    assert!(second.load(Ordering::Acquire));
    assert!(harness.server.state.callbacks.is_empty());
}

#[test]
fn extreme_input_region_coordinates_do_not_overflow_during_hit_testing() {
    use smithay::wayland::compositor::RectangleKind;
    let mut region = RegionAttributes::default();
    region.rects.push((
        RectangleKind::Add,
        Rectangle::new((1, 1).into(), (i32::MAX, i32::MAX).into()),
    ));
    assert!(input_contains(&region, [2.0, 2.0]));
    region.rects.push((
        RectangleKind::Subtract,
        Rectangle::new((i32::MAX, i32::MAX).into(), (i32::MAX, i32::MAX).into()),
    ));
    assert!(input_contains(&region, [2.0, 2.0]));
    assert!(!input_contains(&region, [0.0, 0.0]));
}

#[test]
fn buffer_memory_limit_rejects_the_new_client_and_keeps_existing_windows() {
    let mut good = Harness::new();
    let mut windows = Vec::new();
    let mut buffers = Vec::new();
    for _ in 0..4 {
        let window = good.window();
        buffers.push(good.attach(&window, [16384, 4096]));
        windows.push(window);
    }
    let identities: Vec<_> = good
        .server
        .windows()
        .iter()
        .map(|window| window.id)
        .collect();
    let Harness {
        server,
        connection,
        queue,
        client,
    } = good;
    let mut bad = Harness::connect(server);
    let window = bad.window();
    let buffer = bad.buffer([16384, 4096]);
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.commit();
    assert!(
        synchronize(
            &mut bad.server,
            &bad.connection,
            &mut bad.queue,
            &mut bad.client
        )
        .is_err()
    );
    let remaining = bad.server.windows();
    assert_eq!(remaining.len(), 4);
    assert!(
        remaining
            .iter()
            .all(|window| window.mapped && identities.contains(&window.id))
    );
    assert_eq!(bad.server.clients.load(Ordering::Acquire), 1);
    drop((connection, queue, client, windows, buffers));
}

#[test]
fn top_and_bottom_bars_reserve_independent_space_through_resize_and_removal() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let mut bars = Vec::new();
    for (edge, layer, height) in [
        (
            zwlr_layer_surface_v1::Anchor::Top,
            zwlr_layer_shell_v1::Layer::Top,
            32,
        ),
        (
            zwlr_layer_surface_v1::Anchor::Bottom,
            zwlr_layer_shell_v1::Layer::Bottom,
            28,
        ),
    ] {
        let surface = harness
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        let role = harness
            .client
            .layer_shell
            .as_ref()
            .unwrap()
            .get_layer_surface(&surface, None, layer, "park-bar".into(), &qh, ());
        role.set_size(0, height);
        role.set_anchor(
            edge | zwlr_layer_surface_v1::Anchor::Left | zwlr_layer_surface_v1::Anchor::Right,
        );
        role.set_exclusive_zone(height as i32);
        surface.commit();
        harness.sync();
        harness.sync();
        let buffer = harness.buffer([640, height as i32]);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        harness.sync();
        bars.push((surface, role, buffer));
    }
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 32).into(), (640, 420).into())
    );
    harness.server.resize([900, 600]).unwrap();
    harness.sync();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 32).into(), (900, 540).into())
    );
    let (surface, role, _) = bars.pop().unwrap();
    role.destroy();
    surface.destroy();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 32).into(), (900, 568).into())
    );
}
