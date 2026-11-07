use super::*;
use std::{
    os::fd::{AsFd, AsRawFd, OwnedFd},
    sync::{atomic::{AtomicBool, Ordering}, mpsc::{SyncSender, sync_channel}},
    thread::JoinHandle,
    time::Duration,
};

struct Semaphore {
    handle: vk::Semaphore,
    device: ash::Device,
    _owner: wgpu::Device,
}

impl Drop for Semaphore {
    fn drop(&mut self) { unsafe { self.device.destroy_semaphore(self.handle, None); } }
}

pub(crate) struct Gate {
    release: SyncSender<()>,
    signal: Option<JoinHandle<()>>,
    expired: Arc<AtomicBool>,
    completion: Option<completion::Completion>,
}

impl Gate {
    pub(crate) fn release(mut self) {
        self.signal();
        self.completion.as_ref().unwrap().wait_for_probe().unwrap();
        assert!(!self.expired.load(Ordering::Acquire), "The private producer gate expired before the test released it");
    }

    fn signal(&mut self) {
        let _ = self.release.try_send(());
        if let Some(signal) = self.signal.take() { let _ = signal.join(); }
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        self.signal();
        if let Some(completion) = &self.completion { let _ = completion.wait_for_probe(); }
    }
}

pub(crate) fn ready(image: &Dmabuf) -> bool {
    image.handles().all(|fd| {
        let mut poll = [rustix::event::PollFd::new(&fd, rustix::event::PollFlags::IN)];
        rustix::event::poll(&mut poll, Some(&rustix::time::Timespec { tv_sec: 0, tv_nsec: 0 })).unwrap();
        poll[0].revents().contains(rustix::event::PollFlags::IN)
    })
}

pub(crate) fn pixels(renderer: &VulkanRenderer, color: wgpu::Color) -> (ExportedImage, Gate) {
    let image = renderer.clear(renderer.create_shared_image(320, 240).unwrap(), color).unwrap();
    image.wait_for_probe().unwrap();
    assert!(ready(&image.dmabuf));
    let event = Arc::new(synchronization::Event {
        event: unsafe { renderer.raw_device.create_event(&vk::EventCreateInfo::default(), None) }.unwrap(),
        device: renderer.raw_device.clone(),
        _owner: renderer.device.clone(),
    });
    let mut export = vk::ExportSemaphoreCreateInfo::default().handle_types(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
    let semaphore = Semaphore {
        handle: unsafe { renderer.raw_device.create_semaphore(&vk::SemaphoreCreateInfo::default().push_next(&mut export), None) }.unwrap(),
        device: renderer.raw_device.clone(),
        _owner: renderer.device.clone(),
    };
    let expired = Arc::new(AtomicBool::new(false));
    let (release, wait) = sync_channel(1);
    let timed_out = expired.clone();
    let held = event.clone();
    let signal = std::thread::spawn(move || {
        if wait.recv_timeout(Duration::from_secs(2)).is_err() { timed_out.store(true, Ordering::Release); }
        unsafe { held.device.set_event(held.event) }.unwrap();
    });
    let mut gate = Gate { release, signal: Some(signal), expired, completion: None };
    unsafe { renderer.queue.as_hal::<Vulkan>() }.unwrap().add_signal_semaphore(semaphore.handle, None);
    let submission = renderer.submit_raw("private_client_producer_gate", |command| unsafe {
        renderer.raw_device.cmd_wait_events(command, &[event.event], vk::PipelineStageFlags::HOST, vk::PipelineStageFlags::ALL_COMMANDS, &[], &[], &[]);
    }).unwrap();
    let semaphores = khr::external_semaphore_fd::Device::new(&renderer.raw_instance, &renderer.raw_device);
    let fd = unsafe { semaphores.get_semaphore_fd(&vk::SemaphoreGetFdInfoKHR::default().semaphore(semaphore.handle).handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)) };
    renderer.retain((event, semaphore));
    gate.completion = Some(renderer.completion.track(submission).unwrap());
    let fd = unsafe { OwnedFd::from_raw_fd(fd.unwrap()) };
    assert!(rustix::io::fcntl_getfd(&fd).unwrap().contains(rustix::io::FdFlags::CLOEXEC));
    #[repr(C)]
    struct Import { flags: u32, fd: i32 }
    const IMPORT: rustix::ioctl::Opcode = rustix::ioctl::opcode::write::<Import>(b'b', 3);
    for plane in image.dmabuf.handles() {
        unsafe { rustix::ioctl::ioctl(plane.as_fd(), rustix::ioctl::Setter::<IMPORT, _>::new(Import { flags: 2, fd: fd.as_raw_fd() })) }.unwrap();
    }
    assert!(!ready(&image.dmabuf), "The private DMA-BUF did not acquire the producer fence");
    (image, gate)
}
