use serde::Serialize;

const MAX_WAITING_FRAMES: u32 = 30;
const PHYSICAL_TOLERANCE: u32 = 2;
const LOGICAL_STEPS: [[u32; 2]; 5] = [[960, 680], [800, 600], [1200, 700], [900, 680], [960, 680]];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeRequest {
    pub logical_width: u32,
    pub logical_height: u32,
    pub scale_factor: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct WindowStressFacts {
    pub requested_steps: u64,
    pub observed_steps: u64,
    pub converged_steps: u64,
    pub timed_out_steps: u64,
    pub complete: bool,
    pub results: Vec<ResizeProbeResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResizeProbeResult {
    pub requested_logical_width: u32,
    pub requested_logical_height: u32,
    pub requested_scale_factor: f64,
    pub expected_physical_width: u32,
    pub expected_physical_height: u32,
    pub observed_physical_width: u32,
    pub observed_physical_height: u32,
    pub observed_scale_factor: f64,
    pub frames_waited: u32,
    pub outcome: String,
}

pub struct WindowStress {
    next_step: usize,
    active: Option<ActiveResize>,
    results: Vec<ResizeProbeResult>,
}

struct ActiveResize {
    request: ResizeRequest,
    frames_waited: u32,
}

impl Default for WindowStress {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowStress {
    pub fn new() -> Self {
        Self {
            next_step: 0,
            active: None,
            results: Vec::new(),
        }
    }

    pub fn request_next(&mut self, scale_factor: f64) -> Option<ResizeRequest> {
        if self.active.is_some() {
            return None;
        }
        let [logical_width, logical_height] = *LOGICAL_STEPS.get(self.next_step)?;
        self.next_step += 1;
        let request = ResizeRequest {
            logical_width,
            logical_height,
            scale_factor,
        };
        self.active = Some(ActiveResize {
            request,
            frames_waited: 0,
        });
        Some(request)
    }

    pub fn observe(
        &mut self,
        physical_width: u32,
        physical_height: u32,
        scale_factor: f64,
    ) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        active.frames_waited += 1;
        let expected_width = physical_extent(active.request.logical_width, scale_factor);
        let expected_height = physical_extent(active.request.logical_height, scale_factor);
        let converged = close(physical_width, expected_width)
            && close(physical_height, expected_height)
            && (scale_factor - active.request.scale_factor).abs() <= f64::EPSILON;
        if !converged && active.frames_waited < MAX_WAITING_FRAMES {
            return false;
        }
        let active = self.active.take().expect("active resize probe");
        self.results.push(ResizeProbeResult {
            requested_logical_width: active.request.logical_width,
            requested_logical_height: active.request.logical_height,
            requested_scale_factor: active.request.scale_factor,
            expected_physical_width: expected_width,
            expected_physical_height: expected_height,
            observed_physical_width: physical_width,
            observed_physical_height: physical_height,
            observed_scale_factor: scale_factor,
            frames_waited: active.frames_waited,
            outcome: if converged { "converged" } else { "timed_out" }.into(),
        });
        true
    }

    pub fn is_complete(&self) -> bool {
        self.next_step == LOGICAL_STEPS.len() && self.active.is_none()
    }

    pub fn facts(&self) -> WindowStressFacts {
        WindowStressFacts {
            requested_steps: self.next_step as u64,
            observed_steps: self.results.len() as u64,
            converged_steps: self
                .results
                .iter()
                .filter(|result| result.outcome == "converged")
                .count() as u64,
            timed_out_steps: self
                .results
                .iter()
                .filter(|result| result.outcome == "timed_out")
                .count() as u64,
            complete: self.is_complete(),
            results: self.results.clone(),
        }
    }

    pub fn status(&self) -> String {
        let facts = self.facts();
        if facts.complete {
            format!(
                "COMPLETE: {} CONVERGED, {} TIMED OUT",
                facts.converged_steps, facts.timed_out_steps
            )
        } else {
            format!(
                "RUNNING: {} OF {} RESIZE STEPS OBSERVED",
                facts.observed_steps,
                LOGICAL_STEPS.len()
            )
        }
    }
}

fn physical_extent(logical: u32, scale_factor: f64) -> u32 {
    (f64::from(logical) * scale_factor)
        .round()
        .clamp(1.0, f64::from(u32::MAX)) as u32
}

fn close(observed: u32, expected: u32) -> bool {
    observed.abs_diff(expected) <= PHYSICAL_TOLERANCE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stress_advances_after_each_converged_observation() {
        let mut stress = WindowStress::new();

        while let Some(request) = stress.request_next(1.25) {
            assert!(stress.observe(
                physical_extent(request.logical_width, 1.25),
                physical_extent(request.logical_height, 1.25),
                1.25,
            ));
        }

        let facts = stress.facts();
        assert!(facts.complete);
        assert_eq!(facts.requested_steps, 5);
        assert_eq!(facts.observed_steps, 5);
        assert_eq!(facts.converged_steps, 5);
        assert_eq!(facts.timed_out_steps, 0);
    }

    #[test]
    fn compositor_refusal_becomes_a_bounded_timeout() {
        let mut stress = WindowStress::new();
        stress.request_next(1.0).expect("first request");

        for _ in 0..MAX_WAITING_FRAMES - 1 {
            assert!(!stress.observe(1366, 740, 1.0));
        }
        assert!(stress.observe(1366, 740, 1.0));

        let facts = stress.facts();
        assert_eq!(facts.observed_steps, 1);
        assert_eq!(facts.converged_steps, 0);
        assert_eq!(facts.timed_out_steps, 1);
        assert!(!facts.complete);
    }
}
