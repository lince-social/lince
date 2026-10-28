use super::*;

pub(super) struct Ime {
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
    pub(super) method: zwp_input_method_v2::ZwpInputMethodV2,
}

pub(super) struct Popup {
    surface: wl_surface::WlSurface,
    role: zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
    buffer: wl_buffer::WlBuffer,
}

impl Popup {
    pub(super) fn destroy(self, fixture: &mut Fixture, method: &mut Ime) {
        self.role.destroy(); self.surface.destroy(); self.buffer.destroy(); method.exchange(fixture);
    }
}

impl Ime {
    pub(super) fn connect(fixture: &mut Fixture) -> Self {
        let (server, client) = std::os::unix::net::UnixStream::pair().unwrap();
        fixture.desktop.server.insert_authorized_client(server, vec![crate::launch::Capability::InputMethod], false).unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let mut queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut client = Client::default();
        synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).unwrap();
        let method = client.input_method.as_ref().unwrap().get_input_method(client.seat.as_ref().unwrap(), &queue.handle(), ());
        let mut ime = Self { connection, queue, client, method };
        ime.exchange(fixture);
        ime
    }

    pub(super) fn exchange(&mut self, fixture: &mut Fixture) {
        synchronize(&mut fixture.desktop.server, &self.connection, &mut self.queue, &mut self.client).unwrap();
    }

    pub(super) fn tick(&mut self, fixture: &mut Fixture) {
        self.exchange(fixture);
        fixture.tick();
        self.exchange(fixture);
    }

    pub(super) fn until(&mut self, fixture: &mut Fixture, mut ready: impl FnMut(&Fixture, &Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.tick(fixture);
            if ready(fixture, self) { return; }
            assert!(Instant::now() < deadline, "The real input-method connection did not settle: active={}, serial={}", self.active(), self.serial());
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn active(&self) -> bool { self.client.input_active }

    pub(super) fn serial(&self) -> u32 { self.client.input_serial }

    pub(super) fn popup(&mut self, fixture: &mut Fixture, size: [u32; 2], color: [u8; 4]) -> Popup {
        let qh = self.queue.handle();
        let surface = self.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let role = self.method.get_input_popup_surface(&surface, &qh, ());
        let bytes = color.repeat(size[0] as usize * size[1] as usize);
        let mut file = tempfile::tempfile().unwrap(); file.write_all(&bytes).unwrap();
        let pool = self.client.shm.as_ref().unwrap().create_pool(file.as_fd(), bytes.len() as i32, &qh, ());
        let buffer = pool.create_buffer(0, size[0] as i32, size[1] as i32, (size[0] * 4) as i32, wl_shm::Format::Argb8888, &qh, ());
        pool.destroy(); surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32); surface.commit();
        self.exchange(fixture);
        Popup { surface, role, buffer }
    }

    pub(super) fn keyboard_grab(&self) -> zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2 {
        self.method.grab_keyboard(&self.queue.handle(), ())
    }

    pub(super) fn keys(&self) -> &[(u32, bool)] { &self.client.input_keys }

    pub(super) fn destroy(mut self, fixture: &mut Fixture) {
        self.method.destroy();
        self.exchange(fixture);
    }
}
