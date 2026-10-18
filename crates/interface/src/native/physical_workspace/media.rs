use super::*;
use base64::{Engine, prelude::BASE64_STANDARD};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct Frame {
    pub size: [u32; 2],
    pub rgb: Vec<u8>,
}

pub struct PreparedFrame(nucleus::message::MessagePart);

pub(super) struct Captured {
    pub content: nucleus::message::MessagePart,
}

struct Job {
    root: Entity,
    workspace: u64,
    surface: String,
    camera: String,
    index: u32,
    session: String,
    revision: u64,
    started: Instant,
    stop: Arc<AtomicBool>,
    result: tokio::sync::mpsc::Receiver<Result<PreparedFrame, String>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Resource, Default)]
pub(super) struct Jobs(Vec<Job>);

pub(super) fn pending(world: &World, root: Entity, workspace: u64) -> usize {
    world
        .get_resource::<Jobs>()
        .map(|jobs| {
            jobs.0
                .iter()
                .filter(|job| job.root == root && job.workspace == workspace)
                .count()
        })
        .unwrap_or(0)
}

pub fn submit_frame(
    world: &mut World,
    root: Entity,
    workspace: u64,
    evidence: model::Evidence,
    frame: Frame,
) -> Result<(), String> {
    permitted(world, root, workspace)?;
    submit_prepared_frame(world, root, workspace, evidence, prepare_frame(frame)?)
}

pub fn prepare_frame(frame: Frame) -> Result<PreparedFrame, String> {
    let pixels = u64::from(frame.size[0]) * u64::from(frame.size[1]);
    if frame.size.contains(&0)
        || frame.size.iter().any(|n| *n > 1920)
        || pixels > 2_073_600
        || frame.rgb.len() as u64 != pixels * 3
    {
        return Err("Use a finite RGB frame of at most 1920 × 1080 pixels.".into());
    }
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 85)
        .encode(
            &frame.rgb,
            frame.size[0],
            frame.size[1],
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    if encoded.len() > 2 * 1024 * 1024 {
        return Err("Encoded camera frame exceeds 2 MiB.".into());
    }
    let content = nucleus::message::MessagePart::Attachment {
        name: "Requested camera observation.jpg".into(),
        mime_type: "image/jpeg".into(),
        data: BASE64_STANDARD.encode(encoded),
    };
    Ok(PreparedFrame(content))
}

pub fn submit_prepared_frame(
    world: &mut World,
    root: Entity,
    workspace: u64,
    evidence: model::Evidence,
    frame: PreparedFrame,
) -> Result<(), String> {
    if evidence.media.as_deref() != Some(evidence.id.as_str()) {
        return Err("Frame reference must match its observation identity.".into());
    }
    let mut installation = session_mut(world, root, workspace)?;
    let session = installation.0.get_mut(&workspace).unwrap();
    if session.media.len() >= 2 {
        return Err(
            "Stop capture or wait for older frames to expire before taking another.".into(),
        );
    }
    let now = session.now_ms();
    session.runtime.observe(workspace, evidence.clone(), now)?;
    session
        .media
        .insert(evidence.id, Captured { content: frame.0 });
    session.status = "Camera frame available for five seconds. It is sent only by Request Fiote with latest frame.".into();
    Ok(())
}

fn frame(index: u32, resolution: [u32; 2], stop: &AtomicBool) -> Result<Frame, String> {
    use nokhwa::{
        Camera,
        pixel_format::RgbFormat,
        utils::{
            CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
            Resolution,
        },
    };
    if stop.load(Ordering::Relaxed) {
        return Err("Capture canceled.".into());
    }
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(CameraFormat::new(
            Resolution::new(resolution[0], resolution[1]),
            FrameFormat::YUYV,
            30,
        )));
    let mut camera =
        Camera::new(CameraIndex::Index(index), requested).map_err(|e| e.to_string())?;
    if [camera.resolution().width(), camera.resolution().height()] != resolution {
        return Err(
            "The camera resolution differs from the saved setup; update it before calibrating."
                .into(),
        );
    }
    if u64::from(camera.resolution().width()) * u64::from(camera.resolution().height()) > 2_073_600
    {
        return Err("Camera resolution exceeds the capture budget.".into());
    }
    if stop.load(Ordering::Relaxed) {
        return Err("Capture canceled.".into());
    }
    camera.open_stream().map_err(|e| e.to_string())?;
    let result = (|| {
        let image = camera
            .frame()
            .map_err(|e| e.to_string())?
            .decode_image::<RgbFormat>()
            .map_err(|e| e.to_string())?;
        if stop.load(Ordering::Relaxed) {
            return Err("Capture canceled.".into());
        }
        Ok(Frame {
            size: [image.width(), image.height()],
            rgb: image.into_raw(),
        })
    })();
    let closed = camera.stop_stream().map_err(|e| e.to_string());
    closed?;
    result
}

pub fn capture_once(
    world: &mut World,
    root: Entity,
    workspace: u64,
    surface: &str,
    index: u32,
) -> Result<(), String> {
    permitted(world, root, workspace)?;
    let handle = tokio::runtime::Handle::try_current()
        .map_err(|_| "Camera capture requires the application runtime.")?;
    if !world.contains_resource::<Jobs>() {
        return Err("Camera adapter is unavailable.".into());
    }
    if world.resource::<Jobs>().0.len() >= 2
        || world
            .resource::<Jobs>()
            .0
            .iter()
            .any(|job| job.index == index)
    {
        return Err("Wait for the current camera capture.".into());
    }
    let selected = world
        .get::<Installation>(root)
        .and_then(|i| i.0.get(&workspace))
        .ok_or("Configure a surface first.")?
        .runtime
        .surface(surface, workspace)?
        .clone();
    let camera = selected
        .camera
        .clone()
        .ok_or("Assign a camera to this surface.")?;
    let resolution = world.get::<Installation>(root).unwrap().0[&workspace]
        .runtime
        .configuration
        .devices
        .iter()
        .find(|device| device.id == camera)
        .unwrap()
        .resolution;
    blank(world, root);
    let session = nucleus::new_uid("capture");
    connect(world, root, workspace, &camera, &session)?;
    capture(world, root, workspace, &camera, true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let (sender, result) = tokio::sync::mpsc::channel(1);
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    handle.spawn_blocking(move || {
        let result = frame(index, resolution, &worker_stop).and_then(prepare_frame);
        let _ = sender.blocking_send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    world.resource_mut::<Jobs>().0.push(Job {
        root,
        workspace,
        surface: surface.into(),
        camera,
        index,
        session,
        revision: selected.calibration_revision,
        started: Instant::now(),
        stop,
        result,
    });
    Ok(())
}

pub(super) fn tick(world: &mut World) {
    let Some(mut jobs) = world.remove_resource::<Jobs>() else {
        return;
    };
    let mut retained = Vec::new();
    for mut job in jobs.0.drain(..) {
        let valid = permitted(world, job.root, job.workspace).is_ok()
            && world
                .get::<Installation>(job.root)
                .and_then(|i| i.0.get(&job.workspace))
                .is_some_and(|s| {
                    s.runtime
                        .connections
                        .get(&job.camera)
                        .is_some_and(|c| c.session == job.session && c.capture_enabled)
                        && s.runtime
                            .surface(&job.surface, job.workspace)
                            .is_ok_and(|surface| surface.calibration_revision == job.revision)
                });
        if !valid || job.started.elapsed().as_secs() >= 10 {
            job.stop.store(true, Ordering::Relaxed);
            if job.started.elapsed().as_secs() >= 10
                && let Some(mut installation) = world.get_mut::<Installation>(job.root)
                && let Some(session) = installation.0.get_mut(&job.workspace)
            {
                session.status = "Camera driver has not returned. Cancellation requested; capture may remain pending until the driver responds.".into();
            }
        }
        match job.result.try_recv() {
            Ok(result) if valid && !job.stop.load(Ordering::Relaxed) => {
                let now = world.get::<Installation>(job.root).unwrap().0[&job.workspace].now_ms();
                let id = nucleus::new_uid("frame");
                let result = result.and_then(|frame| {
                    submit_prepared_frame(
                        world,
                        job.root,
                        job.workspace,
                        model::Evidence {
                            id: id.clone(),
                            media: Some(id),
                            device: job.camera.clone(),
                            session: job.session.clone(),
                            sequence: 1,
                            captured_ms: now.saturating_sub(
                                job.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                            ),
                            surface: job.surface.clone(),
                            calibration_revision: job.revision,
                            confidence: 1.0,
                            projected_content: false,
                        },
                        frame,
                    )
                });
                if let Err(error) = result
                    && let Some(mut installation) = world.get_mut::<Installation>(job.root)
                    && let Some(session) = installation.0.get_mut(&job.workspace)
                {
                    session.status = error;
                    session.runtime.disconnect(&job.camera);
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => retained.push(job),
            _ => {}
        }
    }
    jobs.0 = retained;
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Installation>>()
        .iter(world)
        .collect();
    for root in roots {
        for session in world.get_mut::<Installation>(root).unwrap().0.values_mut() {
            session
                .media
                .retain(|id, _| session.runtime.observations.contains_key(id));
            for (id, route) in &mut session.routes {
                if session
                    .runtime
                    .requests
                    .get(id)
                    .is_none_or(|r| r.terminal())
                {
                    route.release_media();
                }
            }
        }
    }
    world.insert_resource(jobs);
}
