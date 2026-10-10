#[derive(Clone, Copy, Debug)]
pub struct Cooling {
    remaining: f32,
    duration: f32,
}

impl Cooling {
    pub fn new(duration: f32) -> Self {
        assert!(duration.is_finite() && duration > 0.0);
        Self {
            remaining: 0.0,
            duration,
        }
    }

    pub fn wake(&mut self) {
        self.remaining = self.duration;
    }

    pub fn advance(&mut self, seconds: f32) {
        self.remaining = if seconds.is_finite() {
            (self.remaining - seconds.max(0.0)).max(0.0)
        } else {
            0.0
        };
    }

    pub fn heat(self) -> f32 {
        self.remaining / self.duration
    }
}

#[derive(Clone, Debug)]
pub struct Spring<const N: usize> {
    pub position: [f32; N],
    pub velocity: [f32; N],
}

impl<const N: usize> Spring<N> {
    pub fn new(position: [f32; N]) -> Self {
        Self {
            position,
            velocity: [0.0; N],
        }
    }

    pub fn advance(&mut self, target: [f32; N], seconds: f32) -> bool {
        self.advance_with_frequency(target, seconds, 18.0)
    }

    pub fn advance_with_frequency(&mut self, target: [f32; N], seconds: f32, omega: f32) -> bool {
        if !seconds.is_finite() || seconds > 1.0 {
            self.position = target;
            self.velocity = [0.0; N];
            return false;
        }
        let time = seconds.clamp(0.0, 0.1);
        let omega = omega.clamp(1.0, 30.0);
        let damping = 0.86_f32;
        let frequency = omega * (1.0 - damping * damping).sqrt();
        let decay = (-damping * omega * time).exp();
        let (sine, cosine) = (frequency * time).sin_cos();
        for (index, target) in target.iter().enumerate() {
            let offset = self.position[index] - target;
            let velocity = self.velocity[index];
            self.position[index] = target
                + decay
                    * (offset * cosine + (velocity + damping * omega * offset) / frequency * sine);
            self.velocity[index] = decay
                * (velocity * cosine
                    - (damping * omega * velocity + omega * omega * offset) / frequency * sine);
        }
        let active = self
            .position
            .iter()
            .zip(target)
            .any(|(position, target)| (position - target).abs() > 0.05)
            || self.velocity.iter().any(|velocity| velocity.abs() > 0.25);
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
