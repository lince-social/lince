//! Time-based cooling and damped spring motion for UI controls.

use std::iter::zip;

/// A cooling timer that restarts on interaction.
#[derive(Clone, Copy, Debug)]
pub struct Cooling {
    remaining: f32,
    duration: f32,
}

impl Cooling {
    /// Creates a cool timer with the given duration in seconds.
    ///
    /// # Panics
    ///
    /// Panics if the duration is non-finite or non-positive.
    pub fn new(duration: f32) -> Self {
        assert!(
            duration.is_finite() && duration > 0.0,
            "invalid duration: {duration}"
        );
        Self {
            remaining: 0.0,
            duration,
        }
    }

    /// Restarts the countdown.
    pub fn wake(&mut self) {
        self.remaining = self.duration;
    }

    /// Subtracts elapsed time, or cools immediately after a non-finite step.
    pub fn advance(&mut self, seconds: f32) {
        self.remaining = if seconds.is_finite() {
            (self.remaining - seconds.max(0.0)).max(0.0)
        } else {
            0.0
        };
    }

    /// Returns the fraction of the cooling duration still remaining.
    pub fn heat(self) -> f32 {
        self.remaining / self.duration
    }
}

const DEFAULT_FREQUENCY: f32 = 18.0;
const DAMPING: f32 = 0.86;
const MAX_STEP_SECONDS: f32 = 0.1;
const RESUME_WINDOW_SECONDS: f32 = 1.0;
const POSITION_TOLERANCE: f32 = 0.05;
const VELOCITY_TOLERANCE: f32 = 0.25;

/// An underdamped spring with independent coordinates.
#[derive(Clone, Debug)]
pub struct Spring<const N: usize> {
    pub position: [f32; N],
    pub velocity: [f32; N],
}

impl<const N: usize> Spring<N> {
    /// Creates a spring at rest at the given position.
    pub fn new(position: [f32; N]) -> Self {
        Self {
            position,
            velocity: [0.0; N],
        }
    }

    /// Advances toward the target at the default frequency and reports whether motion continues.
    pub fn advance(&mut self, target: [f32; N], seconds: f32) -> bool {
        self.advance_with_frequency(target, seconds, DEFAULT_FREQUENCY)
    }

    /// Advances with a custom angular frequency. Long or non-finite steps settle immediately.
    pub fn advance_with_frequency(&mut self, target: [f32; N], seconds: f32, omega: f32) -> bool {
        if !seconds.is_finite() || seconds > RESUME_WINDOW_SECONDS {
            self.position = target;
            self.velocity = [0.0; N];
            return false;
        }
        let time = seconds.clamp(0.0, MAX_STEP_SECONDS);
        let omega = omega.clamp(1.0, 30.0);
        let frequency = omega * (1.0 - DAMPING * DAMPING).sqrt();
        let decay = (-DAMPING * omega * time).exp();
        let (sine, cosine) = (frequency * time).sin_cos();
        for ((position, speed), target) in zip(zip(&mut self.position, &mut self.velocity), target)
        {
            let offset = *position - target;
            let velocity = *speed;
            *position = target
                + decay
                    * (offset * cosine + (velocity + DAMPING * omega * offset) / frequency * sine);
            *speed = decay
                * (velocity * cosine
                    - (DAMPING * omega * velocity + omega * omega * offset) / frequency * sine);
        }
        let active = self
            .position
            .iter()
            .zip(target)
            .any(|(position, target)| (position - target).abs() > POSITION_TOLERANCE)
            || self
                .velocity
                .iter()
                .any(|speed| speed.abs() > VELOCITY_TOLERANCE);
        if !active {
            self.position = target;
            self.velocity = [0.0; N];
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooling_uses_elapsed_time_and_only_interaction_restarts_it() {
        for step in [1.0_f32 / 30.0, 1.0 / 60.0, 1.0 / 144.0] {
            let mut cooling = Cooling::new(8.0);
            assert_eq!(cooling.heat(), 0.0);
            cooling.wake();
            for _ in 0..(4.0 / step).round() as usize {
                cooling.advance(step);
            }
            assert!((cooling.heat() - 0.5).abs() < 0.001);
            cooling.advance(20.0);
            assert_eq!(cooling.heat(), 0.0);
            cooling.wake();
            assert_eq!(cooling.heat(), 1.0);
        }
    }

    #[test]
    fn springs_follow_changed_targets_without_jumping_and_eventually_sleep() {
        let mut spring = Spring::new([80.0, -40.0]);
        assert!(spring.advance([0.0, 0.0], 1.0 / 60.0));
        assert!(spring.position[0] > 70.0);
        let velocity = spring.velocity;
        assert!(spring.advance([20.0, 10.0], 1.0 / 60.0));
        assert!(spring.velocity[0] < velocity[0]);
        for _ in 0..120 {
            spring.advance([20.0, 10.0], 1.0 / 60.0);
        }
        assert_eq!(spring.position, [20.0, 10.0]);
        assert!(!spring.advance([20.0, 10.0], 1.0 / 60.0));
    }

    #[test]
    fn resumed_motion_is_finite_and_does_not_replay_a_long_animation() {
        let mut spring = Spring::new([60.0]);
        assert!(!spring.advance([0.0], 10.0));
        assert_eq!(spring.position, [0.0]);
    }
}
