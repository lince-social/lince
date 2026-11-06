use super::*;
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

pub(in crate::graphics) struct ExportTimestamps {
    pool: vk::QueryPool,
    raw_device: ash::Device,
    _device: wgpu::Device,
    capacity: u32,
    next: AtomicU32,
    valid_bits: u32,
    period_ns: f64,
}

impl ExportTimestamps {
    fn new(gpu: &GpuContext, pairs: u32) -> Result<Self> {
        let families = unsafe {
            gpu.raw_instance
                .get_physical_device_queue_family_properties(gpu.physical_device)
        };
        let valid_bits = families[gpu.queue_family as usize].timestamp_valid_bits;
        let properties = unsafe {
            gpu.raw_instance
                .get_physical_device_properties(gpu.physical_device)
        };
        let period_ns = f64::from(properties.limits.timestamp_period);
        if !(1..=64).contains(&valid_bits) || !period_ns.is_finite() || period_ns <= 0.0 {
            return Err(ProbeFailure::at(
                "gpu_timestamps_unavailable",
                format!("queue valid bits={valid_bits}, period_ns={period_ns}"),
            ));
        }
        let capacity = pairs
            .checked_mul(2)
            .filter(|value| *value > 0)
            .ok_or_else(|| ProbeFailure::at("gpu_timestamps", "Invalid query capacity"))?;
        let pool = unsafe {
            gpu.raw_device.create_query_pool(
                &vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(capacity),
                None,
            )
        }
        .map_err(|error| ProbeFailure::at("gpu_timestamps_unavailable", error))?;
        Ok(Self {
            pool,
            raw_device: gpu.raw_device.clone(),
            _device: gpu.device.clone(),
            capacity,
            next: AtomicU32::new(0),
            valid_bits,
            period_ns,
        })
    }

    pub(in crate::graphics) fn begin(&self, command: vk::CommandBuffer) -> Result<u32> {
        let query = self
            .next
            .try_update(Ordering::AcqRel, Ordering::Acquire, |next| {
                next.checked_add(2).filter(|end| *end <= self.capacity)
            })
            .map_err(|_| ProbeFailure::at("gpu_timestamps", "Bounded query capacity exhausted"))?;
        unsafe {
            self.raw_device
                .cmd_reset_query_pool(command, self.pool, query, 2);
            self.raw_device.cmd_write_timestamp(
                command,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pool,
                query,
            );
        }
        Ok(query)
    }

    pub(in crate::graphics) fn end(&self, command: vk::CommandBuffer, query: u32) {
        unsafe {
            self.raw_device.cmd_write_timestamp(
                command,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                self.pool,
                query + 1,
            );
        }
    }

    fn read(&self, first: u32, outputs: usize) -> Result<serde_json::Value> {
        let mut values = vec![[0_u64; 2]; outputs * 2];
        unsafe {
            self.raw_device.get_query_pool_results(
                self.pool,
                first,
                &mut values,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WITH_AVAILABILITY,
            )
        }
        .map_err(|error| ProbeFailure::at("gpu_timestamp_results_unavailable", error))?;
        if values.iter().any(|value| value[1] == 0) {
            return Err(ProbeFailure::at(
                "gpu_timestamp_results_unavailable",
                "Incomplete queries",
            ));
        }
        let intervals = values
            .chunks_exact(2)
            .map(|pair| {
                elapsed_ticks(pair[0][0], pair[1][0], self.valid_bits) as f64 * self.period_ns
            })
            .collect::<Vec<_>>();
        let group_ns = elapsed_ticks(values[0][0], values.last().unwrap()[0], self.valid_bits)
            as f64
            * self.period_ns;
        Ok(serde_json::json!({
            "raw_ticks_and_availability": values,
            "per_output_interval_ns_not_additive": intervals,
            "first_start_to_last_end_ns": group_ns,
        }))
    }
}

impl Drop for ExportTimestamps {
    fn drop(&mut self) {
        unsafe {
            self.raw_device.destroy_query_pool(self.pool, None);
        }
    }
}

fn elapsed_ticks(start: u64, end: u64, valid_bits: u32) -> u64 {
    let mask = u64::MAX >> (64 - valid_bits);
    end.wrapping_sub(start) & mask
}

fn cpu(clock: rustix::time::ClockId) -> Duration {
    let time = rustix::time::clock_gettime(clock);
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

#[test]
fn hardware_timestamp_deltas_mask_unused_bits_and_wrap_at_queue_precision() {
    assert_eq!(elapsed_ticks(251, 3, 8), 8);
    assert_eq!(elapsed_ticks(0xff00_00fb, 0xab00_0003, 8), 8);
    assert_eq!(elapsed_ticks(u64::MAX - 4, 2, 64), 7);
    assert_eq!(elapsed_ticks(0, 1_u64 << 63, 64), 1_u64 << 63);
}

#[test]
#[ignore = "hardware Vulkan timestamps, ordered native suite and exact exported pixels"]
fn export_hardware_intervals_separate_device_time_from_host_completion_age() {
    let checks = lince_interface::native::laboratory::run_interface_checks().unwrap();
    assert!(checks.first().is_some_and(|case| {
        case.name
            .ends_with("::protein_area_filter_visible_and_selectable")
    }));
    assert!(checks.iter().all(|case| case.error.is_none()), "{checks:?}");
    let mut renderer = VulkanRenderer::new().unwrap();
    let timestamps = Arc::new(
        ExportTimestamps::new(&renderer, 256)
            .expect("Hardware GPU execution measurement is unavailable"),
    );
    let mut samples = Vec::new();
    let sizes = [[1366, 768], [1920, 1080]];
    let format = wgpu::TextureFormat::Rgba16Float;
    let mut retained = Vec::new();
    for outputs in [1, 2] {
        let sources = sizes[..outputs]
            .iter()
            .map(|size| batches::target(&renderer, *size, format))
            .collect::<Vec<_>>();
        for (block, timed) in [false, true, true, false].into_iter().enumerate() {
            Arc::get_mut(&mut renderer.gpu).unwrap().export_timestamps =
                timed.then(|| timestamps.clone());
            let expected = sources
                .iter()
                .enumerate()
                .map(|(index, source)| {
                    batches::write(
                        &renderer,
                        source,
                        &[block as u8 + index as u8, 0x38, 0, 0x34, 0, 0x30, 0, 0x3c],
                    )
                })
                .collect::<Vec<_>>();
            for sample in 0..28 {
                let first = timestamps.next.load(Ordering::Acquire);
                let process_started = cpu(rustix::time::ClockId::ProcessCPUTime);
                let owner_started = cpu(rustix::time::ClockId::ThreadCPUTime);
                let started = Instant::now();
                let exports = renderer.export_render_targets(&sources).unwrap();
                let submit_wall = started.elapsed();
                let submit_owner = cpu(rustix::time::ClockId::ThreadCPUTime) - owner_started;
                let submit_process = cpu(rustix::time::ClockId::ProcessCPUTime) - process_started;
                let wait_started = Instant::now();
                exports[0].wait_for_probe().unwrap();
                let wait_wall = wait_started.elapsed();
                let completed_wall = started.elapsed();
                assert!(exports.iter().all(|image| image.ready().unwrap()));
                let query_count = timestamps.next.load(Ordering::Acquire) - first;
                assert_eq!(query_count, if timed { outputs as u32 * 2 } else { 0 });
                let read_started = Instant::now();
                let device = timed.then(|| timestamps.read(first, outputs).unwrap());
                let read_wall = read_started.elapsed();
                if sample >= 4 {
                    samples.push(serde_json::json!({
                        "outputs": outputs, "block": block, "sample": sample - 4,
                        "timed": timed, "host_export_return_ns": submit_wall.as_nanos(),
                        "host_export_owner_cpu_ns": submit_owner.as_nanos(),
                        "host_export_process_cpu_ns": submit_process.as_nanos(),
                        "completion_wait_ns": wait_wall.as_nanos(),
                        "host_export_to_completion_ns": completed_wall.as_nanos(),
                        "completed_query_read_ns": read_wall.as_nanos(),
                        "device": device,
                    }));
                }
                if sample == 27 {
                    for (image, bytes) in exports.iter().zip(&expected) {
                        assert_eq!(batches::read(&renderer, image, format), *bytes);
                    }
                    retained.push((exports, expected.clone()));
                }
            }
            for source in &sources {
                batches::write(&renderer, source, &[0; 8]);
            }
        }
    }
    Arc::get_mut(&mut renderer.gpu).unwrap().export_timestamps = None;
    for (exports, expected) in &retained {
        for (image, bytes) in exports.iter().zip(expected) {
            assert_eq!(batches::read(&renderer, image, format), *bytes);
        }
    }
    let report = serde_json::json!({
        "adapter": format!("{:?}", renderer.adapter.get_info()),
        "queue_family": renderer.queue_family, "timestamp_valid_bits": timestamps.valid_bits,
        "timestamp_period_ns": timestamps.period_ns,
        "query_count": timestamps.next.load(Ordering::Acquire),
        "interval": "TOP before export initialization/copy through BOTTOM after external release; includes dependencies, not a pure-copy busy-time counter",
        "gpu_clock_is_not_calibrated_to_host": true,
        "per_output_intervals_can_overlap": true,
        "source_sizes": sizes, "format": "RGBA16F", "warmups_per_block": 4,
        "ordered_common_cases": checks.len(), "retained_export_groups": retained.len(),
        "samples": samples,
    });
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("export-hardware-intervals.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    eprintln!(
        "Park hardware intervals: {} samples, {} queries, valid_bits={} period_ns={}",
        samples.len(),
        timestamps.next.load(Ordering::Acquire),
        timestamps.valid_bits,
        timestamps.period_ns
    );
    lince_interface::host::performance::flush();
}
