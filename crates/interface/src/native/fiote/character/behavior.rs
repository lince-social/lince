use bevy::prelude::*;

pub(super) const WALK_SPEED: f32 = 0.18 / (1.5 * 0.65);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Activity {
    Idle,
    LookAround,
    Walk,
    TurnLeft,
    TurnRight,
    SitDown,
    Sit,
    StandUp,
    LieDown,
    Rest,
    GetUp,
    FallAsleep,
    Sleep,
    WakeUp,
    GroomPaw,
    GroomFlank,
    Stretch,
}

impl Activity {
    pub(super) fn frame_period(self) -> std::time::Duration {
        std::time::Duration::from_millis(match self {
            Self::Sleep => 250,
            Self::Idle | Self::LookAround | Self::Sit | Self::Rest => 125,
            _ => 66,
        })
    }
    pub(super) const ALL: [Self; 17] = [
        Self::Idle,
        Self::LookAround,
        Self::Walk,
        Self::TurnLeft,
        Self::TurnRight,
        Self::SitDown,
        Self::Sit,
        Self::StandUp,
        Self::LieDown,
        Self::Rest,
        Self::GetUp,
        Self::FallAsleep,
        Self::Sleep,
        Self::WakeUp,
        Self::GroomPaw,
        Self::GroomFlank,
        Self::Stretch,
    ];

    pub(super) fn clip(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::LookAround => "look_around",
            Self::Walk => "walk",
            Self::TurnLeft => "turn_left",
            Self::TurnRight => "turn_right",
            Self::SitDown => "sit_down",
            Self::Sit => "sit_idle",
            Self::StandUp => "stand_up",
            Self::LieDown => "lie_down",
            Self::Rest => "rest",
            Self::GetUp => "get_up",
            Self::FallAsleep => "fall_asleep",
            Self::Sleep => "sleep",
            Self::WakeUp => "wake_up",
            Self::GroomPaw => "groom_paw",
            Self::GroomFlank => "groom_flank",
            Self::Stretch => "stretch",
        }
    }

    pub(super) fn loops(self) -> bool {
        matches!(
            self,
            Self::Idle
                | Self::Walk
                | Self::TurnLeft
                | Self::TurnRight
                | Self::Sit
                | Self::Rest
                | Self::Sleep
        )
    }

    fn next(self, choice: u32) -> Self {
        match self {
            Self::Idle => [
                Self::Walk,
                Self::SitDown,
                Self::LieDown,
                Self::LookAround,
                Self::Stretch,
            ][choice as usize % 5],
            Self::Walk => [Self::Idle, Self::TurnLeft, Self::TurnRight][choice as usize % 3],
            Self::TurnLeft | Self::TurnRight | Self::LookAround | Self::Stretch => Self::Idle,
            Self::SitDown | Self::GroomPaw => Self::Sit,
            Self::Sit => {
                if choice.is_multiple_of(3) {
                    Self::GroomPaw
                } else {
                    Self::StandUp
                }
            }
            Self::StandUp | Self::GetUp => Self::Idle,
            Self::LieDown | Self::GroomFlank | Self::WakeUp => Self::Rest,
            Self::Rest => [Self::FallAsleep, Self::GroomFlank, Self::GetUp][choice as usize % 3],
            Self::FallAsleep => Self::Sleep,
            Self::Sleep => Self::WakeUp,
        }
    }
}

#[derive(Clone)]
pub(super) struct Behavior {
    pub activity: Activity,
    pub position: Vec2,
    pub yaw: f32,
    pub walk_speed: f32,
    elapsed: f32,
    dwell: f32,
    destination: Vec2,
    random: u32,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            activity: Activity::Sit,
            position: Vec2::ZERO,
            yaw: 0.0,
            walk_speed: WALK_SPEED,
            elapsed: 0.0,
            dwell: 8.0,
            destination: Vec2::ZERO,
            random: 0x71f0_7e19,
        }
    }
}

impl Behavior {
    fn random(&mut self) -> u32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random
    }

    pub(super) fn tick(&mut self, delta: f32, clip_duration: f32) -> bool {
        if !delta.is_finite() || delta <= 0.0 {
            return false;
        }
        let delta = delta.min(0.25);
        self.elapsed += delta;
        if self.activity == Activity::Walk {
            let direction = self.destination - self.position;
            let distance = direction.length();
            if distance > 0.001 {
                self.position += direction / distance * (self.walk_speed * delta).min(distance);
                let desired = direction.x.atan2(direction.y);
                let difference = (desired - self.yaw + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                self.yaw += difference.clamp(-3.0 * delta, 3.0 * delta);
            }
        } else if matches!(self.activity, Activity::TurnLeft | Activity::TurnRight) {
            self.yaw += if self.activity == Activity::TurnLeft {
                0.55 * delta
            } else {
                -0.55 * delta
            };
        } else {
            self.yaw *= (-3.0 * delta).exp();
        }
        let duration = if self.activity.loops() {
            self.dwell
        } else {
            clip_duration.max(0.1)
        };
        let arrived =
            self.activity == Activity::Walk && self.position.distance(self.destination) < 0.001;
        if self.elapsed < duration && !arrived {
            return false;
        }
        let choice = self.random();
        self.activity = self.activity.next(choice);
        self.elapsed = 0.0;
        self.dwell = match self.activity {
            Activity::Sleep => 20.0 + (choice % 21) as f32,
            Activity::Rest | Activity::Sit => 8.0 + (choice % 9) as f32,
            Activity::Walk | Activity::TurnLeft | Activity::TurnRight => 3.0,
            _ => 4.0 + (choice % 5) as f32,
        };
        if self.activity == Activity::Walk {
            self.destination = Vec2::new(
                if self.position.x >= 0.0 { -0.24 } else { 0.24 },
                (choice % 101) as f32 / 500.0 - 0.1,
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posture_changes_complete_before_grooming_or_movement() {
        for choice in 0..100 {
            assert_eq!(Activity::LieDown.next(choice), Activity::Rest);
            assert_eq!(Activity::SitDown.next(choice), Activity::Sit);
            assert_eq!(Activity::GetUp.next(choice), Activity::Idle);
            assert_eq!(Activity::GroomPaw.next(choice), Activity::Sit);
            assert_eq!(Activity::GroomFlank.next(choice), Activity::Rest);
            assert_eq!(Activity::FallAsleep.next(choice), Activity::Sleep);
            assert_eq!(Activity::Sleep.next(choice), Activity::WakeUp);
        }
        let mut behavior = Behavior::default();
        behavior.activity = Activity::LieDown;
        for _ in 0..19 {
            assert!(!behavior.tick(0.1, 2.0));
            assert_eq!(behavior.activity, Activity::LieDown);
        }
        assert!(behavior.tick(0.1, 2.0));
        assert_eq!(behavior.activity, Activity::Rest);
    }

    #[test]
    fn long_running_behavior_stays_in_bounds_and_visits_every_posture() {
        let mut behavior = Behavior::default();
        let mut visited = std::collections::HashSet::new();
        for _ in 0..100_000 {
            behavior.tick(0.1, 2.0);
            visited.insert(behavior.activity.clip());
            assert!(behavior.position.x.abs() <= 0.241);
            assert!(behavior.position.y.abs() <= 0.101);
            assert!(behavior.yaw.is_finite());
        }
        for activity in Activity::ALL {
            assert!(visited.contains(activity.clip()), "{}", activity.clip());
        }
    }

    #[test]
    fn invalid_time_and_suspension_do_not_advance_behavior() {
        let mut behavior = Behavior::default();
        for delta in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(!behavior.tick(delta, 2.0));
        }
        assert_eq!(behavior.elapsed, 0.0);
        assert!(!behavior.tick(300.0, 2.0));
        assert_eq!(behavior.elapsed, 0.25);
    }
}
