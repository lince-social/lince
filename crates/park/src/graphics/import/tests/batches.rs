use super::*;
use std::time::{Duration, Instant};

fn process_cpu() -> Duration {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    Duration::new(value.tv_sec as u64, value.tv_nsec as u32)
}

fn interface_checks() {
    let results = lince_interface::native::laboratory::run_interface_checks().unwrap();
    assert!(results.first().is_some_and(|result| {
        result
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    }));
    assert!(
        results.iter().all(|result| result.error.is_none()),
        "{results:?}"
    );
}

fn target(renderer: &VulkanRenderer, size: [u32; 2], format: wgpu::TextureFormat) -> wgpu::Texture {
    renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("park_export_batch_test"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn write(renderer: &VulkanRenderer, source: &wgpu::Texture, pixel: &[u8]) -> Vec<u8> {
    let bytes = pixel.repeat((source.width() * source.height()) as usize);
    renderer.queue.write_texture(
        source.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source.width() * pixel.len() as u32),
            rows_per_image: Some(source.height()),
        },
        source.size(),
    );
    bytes
}

fn acquire(exported: &ExportedImage) -> Duration {
    let mut waited = Duration::ZERO;
    for fd in exported.dmabuf.handles() {
        let mut descriptor = [rustix::event::PollFd::new(
            &fd,
            rustix::event::PollFlags::IN,
        )];
        rustix::event::poll(
            &mut descriptor,
            Some(&rustix::time::Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
        )
        .unwrap();
        if descriptor[0]
            .revents()
            .contains(rustix::event::PollFlags::IN)
        {
            continue;
        }
        let started = Instant::now();
        rustix::event::poll(
            &mut descriptor,
            Some(&rustix::time::Timespec {
                tv_sec: 2,
                tv_nsec: 0,
            }),
        )
        .unwrap();
        assert!(
            descriptor[0]
                .revents()
                .contains(rustix::event::PollFlags::IN),
            "The exported buffer acquire fence did not become readable: {:?}",
            descriptor[0].revents()
        );
        waited += started.elapsed();
    }
    waited
}

fn read(
    renderer: &VulkanRenderer,
    exported: &ExportedImage,
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    exported.wait_for_probe().unwrap();
    assert!(exported.ready().unwrap());
    let waited = acquire(exported);
    if !waited.is_zero() {
        eprintln!(
            "Park retained export acquire wait_us={:.3} individual={}",
            waited.as_secs_f64() * 1e6,
            std::env::var_os("LINCE_PARK_EXPORT_INDIVIDUAL_CONTROL").is_some()
        );
    }
    let texture = renderer.import_dmabuf(&exported.dmabuf).unwrap();
    assert_eq!(texture.format(), format.remove_srgb_suffix());
    let pixel_size = if format == wgpu::TextureFormat::Rgba16Float {
        8
    } else {
        4
    };
    let row = texture.width() * pixel_size;
    let stride = row.next_multiple_of(256);
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("park_export_batch_readback"),
        size: u64::from(stride) * u64::from(texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    renderer.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    renderer
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    let mapped = slice.get_mapped_range();
    let bytes = mapped
        .chunks_exact(stride as usize)
        .flat_map(|row_bytes| row_bytes[..row as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    bytes
}

#[test]
#[ignore = "hardware Vulkan/EGL ordered native suite and export lifetime/precision/privacy regressions"]
fn batched_outputs_preserve_pixels_precision_initialization_and_lifetimes() {
    interface_checks();
    let renderer = VulkanRenderer::new().unwrap();
    assert!(renderer.export_render_targets(&[]).unwrap().is_empty());
    let formats = [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureFormat::Rgb10a2Unorm,
    ];
    let mut retained = Vec::new();
    for round in 0..3 {
        let sources = formats
            .into_iter()
            .enumerate()
            .map(|(index, format)| {
                target(
                    &renderer,
                    [33 + index as u32 * 4 + round * 2, 17 + index as u32 * 2],
                    format,
                )
            })
            .collect::<Vec<_>>();
        for change in 0..2 {
            let mut expected = Vec::new();
            for (index, source) in sources.iter().enumerate() {
                let mut pixel = match index {
                    0 => vec![17, 83, 149, 203],
                    1 => vec![211, 107, 47, 181],
                    2 => vec![0, 0x40, 0, 0x38, 0, 0x34, 0, 0x3c],
                    _ => ((2_u32 << 30) | (767 << 20) | (513 << 10) | 256)
                        .to_le_bytes()
                        .to_vec(),
                };
                pixel[0] = pixel[0].wrapping_add((round * 2 + change) as u8);
                expected.push(write(&renderer, source, &pixel));
            }
            let exports = if std::env::var_os("LINCE_PARK_EXPORT_INDIVIDUAL_CONTROL").is_some() {
                sources
                    .iter()
                    .map(|source| renderer.export_copy(source).unwrap())
                    .collect()
            } else {
                renderer.export_render_targets(&sources).unwrap()
            };
            assert_eq!(exports.len(), sources.len());
            retained.push((expected, exports));
        }
    }
    for (expected, exports) in retained {
        for ((expected, exported), format) in expected.into_iter().zip(exports).zip(formats) {
            assert_eq!(read(&renderer, &exported, format), expected);
            if format == wgpu::TextureFormat::Bgra8Unorm {
                let rgba = expected
                    .chunks_exact(4)
                    .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
                    .collect::<Vec<_>>();
                assert_eq!(
                    crate::presentation::probe_pixels(&exported.dmabuf).unwrap(),
                    rgba
                );
            }
        }
    }
    let clean = formats
        .into_iter()
        .map(|format| target(&renderer, [35, 19], format))
        .collect::<Vec<_>>();
    let invalid = target(&renderer, [1, 1], wgpu::TextureFormat::R8Unorm);
    let error = renderer
        .export_render_targets(&[clean[0].clone(), invalid])
        .err()
        .unwrap();
    assert_eq!(error.stage, "export_color_format");
    let invalid_usage = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("park_export_invalid_usage"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let error = renderer
        .export_render_targets(&[clean[0].clone(), invalid_usage])
        .err()
        .unwrap();
    assert_eq!(error.stage, "vulkan_export_copy");
    let exports = renderer.export_render_targets(&clean).unwrap();
    assert_eq!(exports.len(), clean.len());
    for (exported, format) in exports.into_iter().zip(formats) {
        let bytes = read(&renderer, &exported, format);
        assert!(bytes.iter().all(|byte| *byte == 0));
    }
    let expected = write(&renderer, &clean[0], &[23, 79, 139, 199]);
    let aliased = renderer
        .export_render_targets(&[clean[0].clone(), clean[0].clone()])
        .unwrap();
    assert_eq!(aliased.len(), 2);
    write(&renderer, &clean[0], &[0, 0, 0, 0]);
    for image in aliased {
        assert_eq!(read(&renderer, &image, formats[0]), expected);
    }
    let standalone = renderer.export_copy(&clean[0]).unwrap();
    assert!(
        read(&renderer, &standalone, formats[0])
            .iter()
            .all(|byte| *byte == 0)
    );
    lince_interface::host::performance::flush();
}

#[test]
#[ignore = "hardware Vulkan/EGL shared GPU context ownership regression; ordered interface suite first"]
fn worker_exports_survive_native_owner_and_gpu_context_shutdown() {
    interface_checks();
    let renderer = VulkanRenderer::new().unwrap();
    let sources = [
        target(&renderer, [67, 29], wgpu::TextureFormat::Bgra8Unorm),
        target(&renderer, [31, 53], wgpu::TextureFormat::Bgra8Unorm),
    ];
    let expected = [
        write(&renderer, &sources[0], &[19, 87, 239, 255]),
        write(&renderer, &sources[1], &[201, 56, 7, 255]),
    ];
    let gpu = renderer.gpu.clone();
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        released.recv().unwrap();
        let images = gpu.export_render_targets(&sources).unwrap();
        drop(sources);
        for image in &images {
            image.wait_for_probe().unwrap();
        }
        drop(gpu);
        images
    });
    drop(renderer);
    release.send(()).unwrap();
    let images = worker.join().unwrap();
    let reader = VulkanRenderer::new().unwrap();
    for (image, expected) in images.iter().zip(expected) {
        assert_eq!(read(&reader, image, wgpu::TextureFormat::Bgra8Unorm), expected);
    }
}

#[test]
#[cfg(feature = "nested")]
#[ignore = "hardware experimental direct-target precision/initialization/lifetime proof; ordered interface suite first"]
fn direct_targets_preserve_zero_initialization_float_and_packed_precision() {
    interface_checks();
    let renderer = VulkanRenderer::new().unwrap();
    let formats = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba16Float, wgpu::TextureFormat::Rgb10a2Unorm];
    let sources = formats.into_iter().enumerate().map(|(index, format)|
        target(&renderer, [33+index as u32*4, 19+index as u32*2], format)).collect::<Vec<_>>();
    let mut retained = Vec::new();
    for round in 0..3 {
        let prepared = crate::graphics::direct::Prepared::new(&renderer.gpu, &sources).unwrap();
        let textures = (0..sources.len()).map(|index| prepared.texture(index).clone()).collect::<Vec<_>>();
        let mut expected = Vec::new();
        for (index, texture) in textures.iter().enumerate() {
            let pixel: &[u8] = match index {
                0 => &[17,54,201,255],
                1 => &[201,54,17,255],
                2 => &[0,0x40,0,0x38,0,0x34,0,0x3c],
                _ => &[0x01,0x08,0x30,0xc0],
            };
            expected.push(if round==0 {vec![0; texture.width() as usize*texture.height() as usize*pixel.len()]}
                else {write(&renderer, texture, pixel)});
        }
        let images = prepared.export(&renderer.gpu, &textures).unwrap();
        drop(textures);
        for ((image, expected), format) in images.into_iter().zip(expected).zip(formats) {
            assert_eq!(read(&renderer, &image, format), expected);
            retained.push((image, expected, format));
        }
    }
    drop(sources);
    for (image, expected, format) in retained {
        assert_eq!(read(&renderer, &image, format), expected);
    }
    let srgb=target(&renderer,[8,8],wgpu::TextureFormat::Bgra8UnormSrgb);
    assert!(crate::graphics::direct::Prepared::new(&renderer.gpu,&[srgb]).is_err());
}

#[test]
#[ignore = "hardware export submission comparison; no concurrent compilation; ordered interface suite first"]
fn export_submission_comparison() {
    interface_checks();
    let renderer = VulkanRenderer::new().unwrap();
    let mut results = Vec::new();
    let mut retained_final = Vec::new();
    for count in [1, 2] {
        let sources = [[1920, 1080], [1366, 768]]
            .into_iter()
            .take(count)
            .map(|size| target(&renderer, size, wgpu::TextureFormat::Rgba16Float))
            .collect::<Vec<_>>();
        for source in &sources {
            write(&renderer, source, &[0, 0x40, 0, 0x38, 0, 0x34, 0, 0x3c]);
        }
        for _ in 0..4 {
            let warm = renderer.export_render_targets(&sources).unwrap();
            for image in warm {
                image.wait_for_probe().unwrap();
            }
        }
        for (block, batched) in [false, true, true, false, true, false, false, true]
            .into_iter()
            .enumerate()
        {
            for iteration in 0..16 {
                let cpu_started = process_cpu();
                let started = Instant::now();
                let exports = if batched {
                    renderer.export_render_targets(&sources).unwrap()
                } else {
                    sources
                        .iter()
                        .map(|source| renderer.export_copy(source).unwrap())
                        .collect()
                };
                let submitted = started.elapsed();
                let submitted_cpu = process_cpu() - cpu_started;
                for image in &exports {
                    image.wait_for_probe().unwrap();
                }
                let acquire_wait = exports.iter().map(acquire).sum::<Duration>();
                let complete = started.elapsed();
                let complete_cpu = process_cpu() - cpu_started;
                assert_eq!(exports.len(), count);
                results.push(serde_json::json!({"outputs":count,"batched":batched,"block":block,"iteration":iteration,
                    "submit_us":submitted.as_secs_f64()*1e6,"ready_us":complete.as_secs_f64()*1e6,"submit_cpu_us":submitted_cpu.as_secs_f64()*1e6,"ready_cpu_us":complete_cpu.as_secs_f64()*1e6,"acquire_wait_us":acquire_wait.as_secs_f64()*1e6}));
                if iteration == 0 {
                    for (image, source) in exports.iter().zip(&sources) {
                        let expected = [0, 0x40, 0, 0x38, 0, 0x34, 0, 0x3c]
                            .repeat((source.width() * source.height()) as usize);
                        assert_eq!(read(&renderer, image, source.format()), expected);
                    }
                }
                if block == 7 && iteration == 15 {
                    retained_final.extend(exports.into_iter().zip(sources.iter().map(|source| {
                        ((source.width() * source.height()) as usize, source.format())
                    })));
                }
            }
        }
    }
    println!(
        "Park export submission comparison {}",
        serde_json::to_string(&results).unwrap()
    );
    lince_interface::host::performance::flush();
    for (image, (pixels, format)) in retained_final {
        assert_eq!(
            read(&renderer, &image, format),
            [0, 0x40, 0, 0x38, 0, 0x34, 0, 0x3c].repeat(pixels)
        );
    }
}
