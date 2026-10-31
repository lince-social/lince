use super::*;
#[cfg(feature = "nested")]
use smithay::backend::allocator::{Buffer, dmabuf::Dmabuf};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{zwp_linux_buffer_params_v1 as params, zwp_linux_dmabuf_v1 as manager, zwp_linux_dmabuf_feedback_v1 as feedback};

#[derive(Default)]
pub(super) struct Feedback {
    device: Vec<u8>,
    formats: Vec<(u32, u64)>,
    done: bool,
}

#[derive(Default)]
pub(super) struct Capture {
    pub(super) manager: Option<manager::ZwpLinuxDmabufV1>,
    pub(super) modifiers: std::collections::BTreeSet<(u32, u64)>,
    feedback: std::collections::BTreeMap<u32, Feedback>,
}

pub(super) fn bind(client: &mut Client, registry: &wl_registry::WlRegistry, name: u32, version: u32, qh: &QueueHandle<Client>) {
    let manager: manager::ZwpLinuxDmabufV1 = registry.bind(name, version.min(4), qh, ());
    if manager.version() >= 4 { manager.get_default_feedback(qh, 0); }
    client.dmabuf.manager = Some(manager);
}

impl Dispatch<feedback::ZwpLinuxDmabufFeedbackV1, u32> for Client {
    fn event(client: &mut Self, _: &feedback::ZwpLinuxDmabufFeedbackV1, event: feedback::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            feedback::Event::MainDevice { device } => client.dmabuf.feedback.entry(*id).or_default().device = device,
            feedback::Event::FormatTable { fd, size } => {
                use std::os::unix::fs::FileExt;
                assert!(size <= 65536 && size % 16 == 0);
                let mut bytes = vec![0; size as usize];
                std::fs::File::from(fd).read_exact_at(&mut bytes, 0).unwrap();
                let formats = bytes.chunks_exact(16).map(|entry| (u32::from_ne_bytes(entry[..4].try_into().unwrap()), u64::from_ne_bytes(entry[8..].try_into().unwrap()))).collect::<Vec<_>>();
                client.dmabuf.modifiers.extend(formats.iter().copied());
                client.dmabuf.feedback.entry(*id).or_default().formats = formats;
            }
            feedback::Event::Done => client.dmabuf.feedback.entry(*id).or_default().done = true,
            _ => {}
        }
    }
}

#[test]
fn ordinary_clients_receive_render_device_and_filtered_formats_on_default_and_surface_feedback() {
    use smithay::backend::allocator::{Format, Fourcc, Modifier};
    let device = rustix::fs::makedev(226, 128);
    for selected in [None, Some(device)] {
        let mut harness = Harness::new();
        harness.server.enable_dmabuf_formats([
            Format { code: Fourcc::Argb8888, modifier: Modifier::Linear },
            Format { code: Fourcc::Nv12, modifier: Modifier::Linear },
            Format { code: Fourcc::Argb8888, modifier: Modifier::Invalid },
        ], || selected).unwrap();
        harness.sync();
        harness.sync();
        let manager = harness.client.dmabuf.manager.as_ref().unwrap();
        assert_eq!(manager.version(), if selected.is_some() { 4 } else { 3 });
        assert_eq!(harness.client.dmabuf.modifiers, [(Fourcc::Argb8888 as u32, u64::from(Modifier::Linear))].into_iter().collect());
        if selected.is_some() {
            let window = harness.window();
            harness.client.dmabuf.manager.as_ref().unwrap().get_surface_feedback(&window.surface, &harness.queue.handle(), 1);
            harness.sync();
            harness.sync();
            for id in [0, 1] {
                let feedback = &harness.client.dmabuf.feedback[&id];
                assert!(feedback.done);
                assert_eq!(feedback.device, device.to_ne_bytes());
                assert_eq!(feedback.formats, [(Fourcc::Argb8888 as u32, u64::from(Modifier::Linear))]);
            }
        } else { assert!(harness.client.dmabuf.feedback.is_empty()); }
    }
}

#[test]
#[ignore = "requires a render node and the selected external EGL client"]
fn external_egl_client_uses_hardware_when_the_render_device_is_advertised() {
    use smithay::backend::allocator::{Format, Fourcc, Modifier};
    use std::{os::unix::{fs::MetadataExt, process::CommandExt}, process::{Child, Stdio}, time::Instant};
    struct Process(Child);
    impl Drop for Process {
        fn drop(&mut self) {
            if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            let _ = self.0.wait();
        }
    }
    let provider = std::env::var("LINCE_PARK_TEST_PROVIDER").expect("Select the external EGL client");
    let node = std::env::var("LINCE_PARK_TEST_RENDER_NODE").expect("Select the compositor render node");
    let device = std::fs::metadata(node).unwrap().rdev();
    let temporary = tempfile::tempdir().unwrap();
    for enabled in [false, true] {
        let directory = temporary.path().join(if enabled { "feedback" } else { "no-feedback" });
        for folder in ["config/noctalia", "cache", "data", "state"] { std::fs::create_dir_all(directory.join(folder)).unwrap(); }
        std::fs::write(directory.join("config/noctalia/config.toml"), "[bar]\norder=[\"default\"]\n[bar.default]\nstart=[]\ncenter=[\"clock\"]\nend=[]\n[shell]\nsetup_wizard_enabled=false\noffline_mode=true\nlaunch_apps_as_systemd_services=false\n").unwrap();
        let mut server = Server::new([640, 480]).unwrap();
        server.enable_dmabuf_formats([
            Format { code: Fourcc::Argb8888, modifier: Modifier::Linear },
            Format { code: Fourcc::Abgr8888, modifier: Modifier::Linear },
        ], || enabled.then_some(device)).unwrap();
        let socket = server.listen().unwrap();
        let log = directory.join("client.log");
        let mut command = crate::launch::Application::new("dbus-run-session").unwrap().arg("--").arg("bash").arg("--noprofile").arg("--norc").arg("-c")
            .arg("export DBUS_SYSTEM_BUS_ADDRESS=\"$DBUS_SESSION_BUS_ADDRESS\"; exec \"$1\"").arg("park-egl-client").arg(&provider).command(&socket).unwrap();
        for (key, folder) in [("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_DATA_HOME", "data"), ("XDG_STATE_HOME", "state")] {
            command.env(key, directory.join(folder));
        }
        command.process_group(0).stdout(std::fs::File::create(&log).unwrap()).stderr(Stdio::from(std::fs::File::options().append(true).open(&log).unwrap()));
        let mut process = Process(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(20);
        let line = loop {
            server.dispatch().unwrap();
            let text = std::fs::read_to_string(&log).unwrap();
            if let Some(line) = text.lines().find(|line| line.contains("OpenGL ES vendor=") && line.contains("renderer=")) { break line.to_owned(); }
            assert!(Instant::now() < deadline && process.0.try_wait().unwrap().is_none(), "The external EGL client did not initialize: {text}");
            std::thread::sleep(Duration::from_millis(5));
        };
        eprintln!("DMA-BUF device feedback {enabled}: {line}");
        assert_eq!(line.contains("llvmpipe") || line.contains("softpipe"), !enabled, "The external client selected the wrong graphics path: {line}");
    }
}

impl Dispatch<manager::ZwpLinuxDmabufV1, ()> for Client {
    fn event(client: &mut Self, _: &manager::ZwpLinuxDmabufV1, event: manager::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let manager::Event::Modifier { format, modifier_hi, modifier_lo } = event {
            client.dmabuf.modifiers.insert((format, (u64::from(modifier_hi) << 32) | u64::from(modifier_lo)));
        }
    }
}

#[cfg(feature = "nested")]
pub(super) fn buffer(client: &Client, queue: &QueueHandle<Client>, image: &Dmabuf) -> wl_buffer::WlBuffer {
    let format = image.format();
    let modifier = u64::from(format.modifier);
    assert!(client.dmabuf.modifiers.contains(&(format.code as u32, modifier)), "The client cannot use an unadvertised DMA-BUF format/modifier");
    let parameters = client.dmabuf.manager.as_ref().unwrap().create_params(queue, ());
    assert_eq!(image.num_planes(), 1);
    parameters.add(image.handles().next().unwrap(), 0, image.offsets().next().unwrap(), image.strides().next().unwrap(),
        (modifier >> 32) as u32, modifier as u32);
    let buffer = parameters.create_immed(image.size().w, image.size().h, format.code as u32, params::Flags::empty(), queue, ());
    parameters.destroy();
    buffer
}

delegate_noop!(Client: ignore params::ZwpLinuxBufferParamsV1);
