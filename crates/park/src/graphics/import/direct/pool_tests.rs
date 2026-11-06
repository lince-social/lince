use super::*;
use crate::graphics::import::tests::batches::{read, target, write};

fn interface_checks() {
    let checks = lince_interface::native::laboratory::run_interface_checks().unwrap();
    assert!(
        checks[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    assert!(
        checks.iter().all(|result| result.error.is_none()),
        "{checks:?}"
    );
}

fn textures(prepared: &Prepared, count: usize) -> Vec<wgpu::Texture> {
    (0..count)
        .map(|index| prepared.texture(index).clone())
        .collect()
}

#[test]
#[ignore = "private hardware pool lease/clear/retirement/budget proof; ordered interface suite first"]
fn pooled_targets_keep_output_and_capture_leases_clear_reuse_and_bound_replacement() {
    interface_checks();
    let renderer = VulkanRenderer::new().unwrap();
    let sources = [
        target(&renderer, [33, 19], wgpu::TextureFormat::Rgba16Float),
        target(&renderer, [21, 31], wgpu::TextureFormat::Rgb10a2Unorm),
    ];
    let mut pool = Pool::new(1, 1024 * 1024);
    let prepared = pool.acquire(&renderer.gpu, &sources).unwrap().unwrap();
    let ids = (0..2)
        .map(|index| prepared.dmabuf(index).clone())
        .collect::<Vec<_>>();
    let actual = textures(&prepared, 2);
    let expected = [
        write(&renderer, &actual[0], &[0, 0x40, 0, 0x38, 0, 0x34, 0, 0x3c]),
        write(&renderer, &actual[1], &[0x01, 0x08, 0x30, 0xc0]),
    ];
    assert!(pool.acquire(&renderer.gpu, &sources).unwrap().is_none());
    let mut images = prepared.export(&renderer.gpu, &actual).unwrap();
    drop(actual);
    for (index, image) in images.iter().enumerate() {
        assert_eq!(
            read(&renderer, image, sources[index].format()),
            expected[index]
        );
    }
    drop(images.remove(0));
    assert!(pool.acquire(&renderer.gpu, &sources).unwrap().is_none());
    let capture = images[0]._direct_lease.as_ref().unwrap().clone();
    drop(images);
    assert!(pool.acquire(&renderer.gpu, &sources).unwrap().is_none());
    drop(capture);
    for _ in 0..8 {
        let prepared = pool.acquire(&renderer.gpu, &sources).unwrap().unwrap();
        assert!((0..2).all(|index| prepared.dmabuf(index) == &ids[index]));
        let actual = textures(&prepared, 2);
        let images = prepared.export(&renderer.gpu, &actual).unwrap();
        drop(actual);
        for (index, image) in images.iter().enumerate() {
            let bytes = read(&renderer, image, sources[index].format());
            assert!(bytes.iter().all(|byte| *byte == 0));
        }
    }
    assert_eq!(pool.stats()[2], 1);
    assert_eq!(pool.stats()[3], 8);
    let prepared = pool.acquire(&renderer.gpu, &sources).unwrap().unwrap();
    let actual = textures(&prepared, 2);
    let retained = prepared.export(&renderer.gpu, &actual).unwrap();
    for image in &retained {
        image.wait_for_probe().unwrap();
    }
    drop(actual);
    let replacement = [target(&renderer, [51, 37], wgpu::TextureFormat::Bgra8Unorm)];
    assert!(pool.acquire(&renderer.gpu, &replacement).unwrap().is_none());
    drop(retained);
    let prepared = pool.acquire(&renderer.gpu, &replacement).unwrap().unwrap();
    assert_ne!(prepared.dmabuf(0), &ids[0]);
    let abandoned_id = prepared.dmabuf(0).clone();
    drop(prepared);
    let prepared = match pool.acquire(&renderer.gpu, &replacement).unwrap() {
        Some(prepared) => prepared,
        None => {
            pool.entries[0]
                .retirement
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .completion
                .wait_for_probe()
                .unwrap();
            pool.acquire(&renderer.gpu, &replacement).unwrap().unwrap()
        }
    };
    assert_ne!(prepared.dmabuf(0), &abandoned_id);
    assert!(pool.stats()[5] >= 2);
    let actual = textures(&prepared, 1);
    for image in prepared.export(&renderer.gpu, &actual).unwrap() {
        image.wait_for_probe().unwrap();
    }
    drop(actual);
    let mut tiny = Pool::new(1, 1);
    assert!(tiny.acquire(&renderer.gpu, &replacement).unwrap().is_none());
    assert_eq!(tiny.stats()[0], 0);
    assert_eq!(tiny.stats()[1], 0);
    assert!(
        pool.acquire(
            &renderer.gpu,
            &[target(
                &renderer,
                [8, 8],
                wgpu::TextureFormat::Rgba8UnormSrgb
            )]
        )
        .is_err()
    );
    let mut exact = Pool::new(2, pool.stats()[1]);
    let held = exact.acquire(&renderer.gpu, &replacement).unwrap().unwrap();
    assert!(
        exact
            .acquire(&renderer.gpu, &replacement)
            .unwrap()
            .is_none()
    );
    drop(held);
    renderer
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(5)),
        })
        .unwrap();
}

struct Event {
    event: vk::Event,
    device: ash::Device,
    _owner: wgpu::Device,
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_event(self.event, None);
        }
    }
}

#[test]
#[ignore = "private hardware pool GPU gate proof with bounded delayed submission; ordered interface suite first"]
fn dropped_image_leases_cannot_recycle_a_gpu_pending_target() {
    interface_checks();
    let renderer = VulkanRenderer::new().unwrap();
    let sources = [target(&renderer, [16, 16], wgpu::TextureFormat::Bgra8Unorm)];
    for exported in [true, false] {
        let mut pool = Pool::new(1, 1024 * 1024);
        let prepared = pool.acquire(&renderer.gpu, &sources).unwrap().unwrap();
        let actual = textures(&prepared, 1);
        for image in prepared.export(&renderer.gpu, &actual).unwrap() {
            image.wait_for_probe().unwrap();
        }
        drop(actual);
        let event = Arc::new(Event {
            event: unsafe {
                renderer
                    .raw_device
                    .create_event(&vk::EventCreateInfo::default(), None)
            }
            .unwrap(),
            device: renderer.raw_device.clone(),
            _owner: renderer.device.clone(),
        });
        let (release, gate) = std::sync::mpsc::sync_channel(1);
        let signal = {
            let event = event.clone();
            std::thread::spawn(move || {
                let _ = gate.recv_timeout(std::time::Duration::from_secs(1));
                unsafe { event.device.set_event(event.event) }.unwrap();
            })
        };
        renderer
            .submit_raw("pool_delayed_submission", |command| unsafe {
                renderer.raw_device.cmd_wait_events(
                    command,
                    &[event.event],
                    vk::PipelineStageFlags::HOST,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    &[],
                    &[],
                    &[],
                );
            })
            .unwrap();
        renderer.retain(event);
        let prepared = pool.acquire(&renderer.gpu, &sources).unwrap().unwrap();
        let actual = textures(&prepared, 1);
        let previous_id = prepared.dmabuf(0).clone();
        let completion = if exported {
            let images = prepared.export(&renderer.gpu, &actual).unwrap();
            let completion = images[0].completion.clone();
            drop(images);
            drop(actual);
            completion
        } else {
            drop(actual);
            drop(prepared);
            assert!(pool.acquire(&renderer.gpu, &sources).unwrap().is_none());
            pool.entries[0]
                .retirement
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .completion
                .clone()
        };
        let pending = !completion.ready().unwrap();
        let allocations = pool.stats()[2];
        let denied = (0..4).all(|_| pool.acquire(&renderer.gpu, &sources).unwrap().is_none());
        let bounded = pool.stats()[0] == 1 && pool.stats()[2] == allocations;
        release.send(()).unwrap();
        signal.join().unwrap();
        completion.wait_for_probe().unwrap();
        assert!(pending && denied && bounded);
        let prepared = pool.acquire(&renderer.gpu, &sources).unwrap().unwrap();
        assert_eq!(prepared.dmabuf(0) == &previous_id, exported);
        let actual = textures(&prepared, 1);
        for image in prepared.export(&renderer.gpu, &actual).unwrap() {
            image.wait_for_probe().unwrap();
        }
        drop(actual);
        renderer
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
    }
}
