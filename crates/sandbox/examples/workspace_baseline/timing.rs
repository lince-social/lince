use super::*;

pub(super) const FIRST: u32 = 28;
pub(super) const FRAMES: u32 = 64 * 4;

#[derive(Resource, Default)]
pub(super) struct Timing {
    previous_frame: Option<Instant>,
    started: Option<Instant>,
    start_ticks: Option<u64>,
    end_ticks: Option<u64>,
    elapsed: Duration,
    pending: Option<(Instant, DVec2)>,
    frames_ms: Vec<f64>,
    responses_ms: Vec<f64>,
}

pub(super) fn drag(world: &mut World, frame: u32) {
    let index = frame - FIRST;
    let now = Instant::now();
    let mut timing = world.resource_mut::<Timing>();
    if let Some(previous) = timing.previous_frame.replace(now) {
        timing
            .frames_ms
            .push((now - previous).as_secs_f64() * 1000.0);
    } else {
        timing.started = Some(now);
        timing.start_ticks = cpu_ticks();
    }
    if index == FRAMES - 1 {
        timing.elapsed = now - timing.started.unwrap();
        timing.end_ticks = cpu_ticks();
    }
    if !index.is_multiple_of(4) {
        return;
    }
    assert!(
        timing.pending.is_none(),
        "Previous drag sample never reached presentation"
    );
    let state = world.resource::<Baseline>();
    let before = lince_sandbox::canvas::presented(world, state.sand.unwrap())
        .unwrap()
        .position;
    let offset = if index.is_multiple_of(8) {
        Vec2::new(64.0, 24.0)
    } else {
        Vec2::ZERO
    };
    let screen = state.drag_screen + offset;
    world.resource_mut::<Timing>().pending = Some((Instant::now(), before));
    pointer(world, screen);
}

pub(super) fn observe(world: &mut World) {
    let Some((start, before)) = world.resource::<Timing>().pending else {
        return;
    };
    let entity = world.resource::<Baseline>().sand.unwrap();
    if lince_sandbox::canvas::presented(world, entity)
        .is_some_and(|item| item.position.distance(before) > 1.0)
    {
        let mut timing = world.resource_mut::<Timing>();
        timing
            .responses_ms
            .push(start.elapsed().as_secs_f64() * 1000.0);
        timing.pending = None;
    }
}

impl Timing {
    pub(super) fn report(&self) -> Value {
        assert_eq!(self.responses_ms.len(), 64);
        json!({
            "workload":"64 alternating pointer moves during one held native drag, one every four updates",
            "input_boundary":"synthetic pointer injection to first changed presented canvas position in PostUpdate",
            "frame_boundary":"consecutive native replay updates during the held drag",
            "frames":timings(&self.frames_ms), "input":timings(&self.responses_ms),
            "frame_samples_ms":self.frames_ms, "input_samples_ms":self.responses_ms,
            "elapsed_ms":self.elapsed.as_secs_f64()*1000.0,
            "process_cpu_ticks":self.end_ticks.zip(self.start_ticks).map(|(end,start)| end-start)
        })
    }
}
