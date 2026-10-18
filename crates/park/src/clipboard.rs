use crate::{environment::environment_ready, server::Server};
use std::{io, time::Instant};

pub fn probe() -> io::Result<()> {
    if !environment_ready() {
        return Err(io::Error::other(
            "The clipboard probe needs its private display environment",
        ));
    }
    let mut server = Server::new([640, 480])?;
    let _socket = server.listen_named(&format!("wayland-park-{}", std::process::id()))?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let result = (|| {
            let mut source = bevy_clipboard::Clipboard::default();
            source
                .set_text("Park native clipboard: Olá 世界")
                .map_err(error)?;
            let mut target = bevy_clipboard::Clipboard::default();
            let text = target
                .fetch_text()
                .poll_result()
                .ok_or_else(|| io::Error::other("Native clipboard read remained pending"))?
                .map_err(error)?;
            if text != "Park native clipboard: Olá 世界" {
                return Err(io::Error::other(
                    "Native clipboard transfer changed the content",
                ));
            }
            if !std::process::Command::new(std::env::current_exe()?)
                .arg("--check-untrusted")
                .status()?
                .success()
            {
                return Err(io::Error::other(
                    "External clipboard access was not isolated",
                ));
            }
            Ok(())
        })();
        let _ = sender.send(result);
    });
    let started = Instant::now();
    loop {
        server.dispatch()?;
        match receiver.try_recv() {
            Ok(result) => {
                worker
                    .join()
                    .map_err(|_| io::Error::other("Native clipboard thread panicked"))?;
                result?;
                println!("Native clipboard text passed through Park's private Wayland display.");
                return Ok(());
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err(io::Error::other("Native clipboard thread stopped"));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        if started.elapsed() > std::time::Duration::from_secs(15) {
            return Err(io::Error::other("Native clipboard transfer timed out"));
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

pub fn check_external_client() -> io::Result<()> {
    use wayland_client::{
        Connection, Dispatch, QueueHandle,
        protocol::wl_registry::{self, WlRegistry},
    };
    #[derive(Default)]
    struct Registry(Vec<String>);
    impl Dispatch<WlRegistry, ()> for Registry {
        fn event(
            state: &mut Self,
            _: &WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global { interface, .. } = event {
                state.0.push(interface);
            }
        }
    }
    let connection = Connection::connect_to_env().map_err(error)?;
    let mut queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut state = Registry::default();
    queue.roundtrip(&mut state).map_err(error)?;
    if !state.0.iter().any(|interface| interface == "wl_compositor")
        || state
            .0
            .iter()
            .any(|interface| interface == "zwlr_data_control_manager_v1")
    {
        return Err(io::Error::other(
            "An external client received private clipboard-manager access",
        ));
    }
    Ok(())
}

fn error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}
