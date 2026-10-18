use super::*;
use std::{sync::atomic::{AtomicBool, Ordering}, time::{Duration, Instant}};

struct Event {
    event: vk::Event,
    device: ash::Device,
    _owner: wgpu::Device,
}

impl Drop for Event {
    fn drop(&mut self) { unsafe { self.device.destroy_event(self.event, None); } }
}

struct Retained(Arc<AtomicBool>);

impl Drop for Retained {
    fn drop(&mut self) { self.0.store(true, Ordering::Release); }
}

pub fn probe() -> Result<()> {
    let renderer = VulkanRenderer::new()?;
    let warm = renderer.create_shared_image(64, 32)?;
    renderer.clear(warm, wgpu::Color::BLACK)?.wait_for_probe()?;
    let image = renderer.create_shared_image(64, 32)?;
    let event = Arc::new(Event {
        event: unsafe { renderer.raw_device.create_event(&vk::EventCreateInfo::default(), None) }
            .map_err(|error| ProbeFailure::at("synchronization_event", error))?,
        device: renderer.raw_device.clone(),
        _owner: renderer.device.clone(),
    });
    let fence = event.clone();
    let (release, gate) = std::sync::mpsc::sync_channel(1);
    let signal = std::thread::spawn(move || {
        let _ = gate.recv_timeout(Duration::from_secs(2));
        unsafe { fence.device.set_event(fence.event) }
    });
    let started = Instant::now();
    renderer.submit_raw("synchronization_delay", |command| unsafe {
        renderer.raw_device.cmd_wait_events(command, &[event.event], vk::PipelineStageFlags::HOST, vk::PipelineStageFlags::ALL_COMMANDS, &[], &[], &[]);
    })?;
    renderer.retain(event);
    let released = Arc::new(AtomicBool::new(false));
    renderer.retain(Retained(released.clone()));
    let output = renderer.clear(image, wgpu::Color::RED)?;
    let elapsed = started.elapsed();
    let premature = released.load(Ordering::Acquire) || output.ready()?;
    let _ = release.send(());
    signal.join().map_err(|_| ProbeFailure::at("synchronization_signal", "Signal thread panicked"))?
        .map_err(|error| ProbeFailure::at("synchronization_signal", error))?;
    output.wait_for_probe()?;
    if elapsed >= Duration::from_secs(1) {
        return Err(ProbeFailure::at("synchronization_nonblocking", "Rendering waited for the deliberately stalled GPU"));
    }
    if premature || !released.load(Ordering::Acquire) {
        return Err(ProbeFailure::at("synchronization_lifetime", "GPU-owned resources did not follow completion"));
    }
    println!("Delayed GPU submission returned in {} ms; completion and retained-resource release occurred after the gate opened.", elapsed.as_millis());
    Ok(())
}
