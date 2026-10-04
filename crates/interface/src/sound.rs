use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Off,
    Blip,
    Title,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub mode: Mode,
    pub volume: u8,
    pub voice: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Off,
            volume: 60,
            voice: None,
        }
    }
}

impl Settings {
    pub fn valid(&self) -> bool {
        self.volume <= 100 && self.voice.as_ref().is_none_or(|voice| voice.len() <= 1024)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub scope: u64,
    pub key: String,
    pub at_ms: i64,
    pub title: String,
    pub projected: bool,
    pub settings: Settings,
}

pub trait Output: Send + Sync {
    fn play(&self, cue: Cue) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn cancel(&self, scope: u64) -> Result<(), String>;
    fn retain(&self, scope: u64, keys: Vec<(String, i64)>) -> Result<(), String>;
}

struct Scope {
    last_ms: i64,
    cues: Vec<Cue>,
}

#[derive(Default)]
pub struct Queue {
    scopes: BTreeMap<u64, Scope>,
    played: HashSet<String>,
}

impl Queue {
    pub fn refresh(&mut self, scope: u64, now: i64, mut cues: Vec<Cue>) {
        for cue in &mut cues {
            cue.scope = scope;
        }
        cues.sort_by(|a, b| a.at_ms.cmp(&b.at_ms).then_with(|| a.key.cmp(&b.key)));
        let last_ms = self.scopes.get(&scope).map_or(now, |scope| scope.last_ms);
        self.scopes.insert(scope, Scope { last_ms, cues });
    }

    pub fn retain(&mut self, mut enabled: impl FnMut(u64) -> bool) -> bool {
        let before = self.scopes.len();
        self.scopes.retain(|scope, _| enabled(*scope));
        before != self.scopes.len()
    }

    pub fn next_ms(&self, now: i64) -> Option<i64> {
        self.scopes
            .values()
            .filter_map(|scope| {
                let start = scope.cues.partition_point(|cue| cue.at_ms <= now);
                scope.cues[start..]
                    .iter()
                    .find(|cue| cue.settings.mode != Mode::Off && !self.played.contains(&cue.key))
                    .map(|cue| cue.at_ms)
            })
            .min()
    }

    pub fn due(&mut self, now: i64) -> Vec<Cue> {
        let mut due = BTreeMap::<(i64, String), Cue>::new();
        for scope in self.scopes.values_mut() {
            if now < scope.last_ms {
                scope.last_ms = now;
                continue;
            }
            let start = scope
                .cues
                .partition_point(|cue| cue.at_ms <= scope.last_ms.max(now.saturating_sub(5001)));
            let end = scope.cues.partition_point(|cue| cue.at_ms <= now);
            for cue in &scope.cues[start..end] {
                if cue.settings.mode == Mode::Off
                    || cue.at_ms <= scope.last_ms
                    || cue.at_ms > now
                    || now.saturating_sub(cue.at_ms) > 5000
                    || self.played.contains(&cue.key)
                {
                    continue;
                }
                due.entry((cue.at_ms, cue.key.clone()))
                    .and_modify(|previous| {
                        let volume = previous.settings.volume.max(cue.settings.volume);
                        let projected = previous.projected && cue.projected;
                        if cue.settings.mode == Mode::Title && previous.settings.mode != Mode::Title
                        {
                            *previous = cue.clone();
                        }
                        previous.settings.volume = volume;
                        previous.projected = projected;
                    })
                    .or_insert_with(|| cue.clone());
            }
            scope.last_ms = now;
        }
        let mut ready = Vec::new();
        let mut blipped_at = None;
        for ((at, key), cue) in due {
            if self.played.contains(&key) {
                continue;
            }
            if cue.settings.mode != Mode::Blip || blipped_at != Some(at) {
                if cue.settings.mode == Mode::Blip {
                    blipped_at = Some(at);
                }
                ready.push(cue);
            }
            self.played.insert(key);
        }
        ready
    }

    pub fn advance(&mut self, now: i64, output: &dyn Output) -> Vec<String> {
        self.due(now)
            .into_iter()
            .filter_map(|cue| output.play(cue).err())
            .collect()
    }
}

pub fn blip(rate: u32) -> Vec<f32> {
    let count = (rate / 12).max(1);
    (0..count)
        .map(|index| {
            let time = index as f32 / rate.max(1) as f32;
            let envelope = (std::f32::consts::PI * index as f32 / count as f32)
                .sin()
                .powi(2);
            (std::f32::consts::TAU * 660.0 * time).sin() * envelope * 0.18
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake(Mutex<Vec<Cue>>);
    impl Output for Fake {
        fn play(&self, cue: Cue) -> Result<(), String> {
            self.0.lock().unwrap().push(cue);
            Ok(())
        }
        fn stop(&self) -> Result<(), String> {
            self.0.lock().unwrap().clear();
            Ok(())
        }
        fn cancel(&self, scope: u64) -> Result<(), String> {
            self.0.lock().unwrap().retain(|cue| cue.scope != scope);
            Ok(())
        }
        fn retain(&self, scope: u64, keys: Vec<(String, i64)>) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .retain(|cue| cue.scope != scope || keys.contains(&(cue.key.clone(), cue.at_ms)));
            Ok(())
        }
    }
    fn cue(key: &str, at_ms: i64, mode: Mode) -> Cue {
        Cue {
            scope: 0,
            key: key.into(),
            at_ms,
            title: key.into(),
            projected: false,
            settings: Settings {
                mode,
                ..Settings::default()
            },
        }
    }

    #[test]
    fn loading_is_silent_and_duplicate_clocks_and_refreshes_alert_once() {
        let output = Fake::default();
        let mut queue = Queue::default();
        queue.refresh(
            1,
            1000,
            vec![
                cue("past", 500, Mode::Title),
                cue("future", 2000, Mode::Title),
            ],
        );
        queue.refresh(2, 1000, vec![cue("future", 2001, Mode::Title)]);
        queue.advance(1000, &output);
        assert!(output.0.lock().unwrap().is_empty());
        queue.advance(2001, &output);
        queue.refresh(1, 2000, vec![cue("future", 2000, Mode::Title)]);
        queue.advance(2001, &output);
        assert_eq!(output.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn simultaneous_titles_are_stable_and_blips_are_combined() {
        for (mode, count) in [(Mode::Title, 3), (Mode::Blip, 1)] {
            let output = Fake::default();
            let mut queue = Queue::default();
            queue.refresh(
                1,
                1000,
                vec![
                    cue("c", 2000, mode),
                    cue("a", 2000, mode),
                    cue("b", 2000, mode),
                ],
            );
            queue.advance(2000, &output);
            let played = output.0.lock().unwrap();
            assert_eq!(played.len(), count);
            assert_eq!(played[0].key, "a");
        }
    }

    #[test]
    fn removal_rescheduling_and_resume_do_not_play_stale_cues() {
        let output = Fake::default();
        let mut queue = Queue::default();
        queue.refresh(1, 1000, vec![cue("removed", 2000, Mode::Title)]);
        queue.refresh(1, 1100, vec![cue("moved", 4000, Mode::Title)]);
        assert_eq!(queue.next_ms(1100), Some(4000));
        queue.advance(2000, &output);
        assert!(output.0.lock().unwrap().is_empty());
        queue.advance(10_000, &output);
        assert!(output.0.lock().unwrap().is_empty());
        assert!(queue.retain(|_| false));
        assert_eq!(queue.next_ms(10_000), None);
    }

    #[test]
    fn generated_blip_is_short_finite_and_fades_at_its_edges() {
        let samples = blip(48_000);
        assert_eq!(samples.len(), 4000);
        assert_eq!(samples[0], 0.0);
        assert!(samples.last().unwrap().abs() < 0.001);
        assert!(
            samples
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.18)
        );
    }

    #[test]
    fn long_running_queues_keep_confirmation_deduplication_and_check_quiet_deadlines_cheaply() {
        let output = Fake::default();
        let mut queue = Queue::default();
        queue.refresh(
            1,
            1000,
            vec![cue("canonical occurrence", 2000, Mode::Title)],
        );
        queue.advance(2000, &output);
        queue.refresh(
            1,
            90_000_000,
            vec![cue("canonical occurrence", 90_000_001, Mode::Title)],
        );
        queue.advance(90_000_001, &output);
        assert_eq!(output.0.lock().unwrap().len(), 1);
        queue.refresh(
            2,
            0,
            (0..50_000)
                .map(|index| cue(&format!("future-{index}"), 100_000 + index, Mode::Title))
                .collect(),
        );
        let started = std::time::Instant::now();
        for now in 0..2000 {
            assert!(queue.advance(now, &output).is_empty());
            assert_eq!(queue.next_ms(now), Some(100_000));
        }
        assert!(started.elapsed().as_millis() < 500);
    }

    #[test]
    fn confirmed_titles_win_over_projected_duplicates_without_lowering_volume() {
        let output = Fake::default();
        let mut queue = Queue::default();
        let mut projection = cue("same", 2000, Mode::Blip);
        projection.projected = true;
        projection.settings.volume = 90;
        let mut actual = cue("same", 2000, Mode::Title);
        actual.settings.volume = 30;
        queue.refresh(1, 1000, vec![projection]);
        queue.refresh(2, 1000, vec![actual]);
        queue.advance(2000, &output);
        let played = output.0.lock().unwrap();
        assert_eq!(played.len(), 1);
        assert_eq!(played[0].settings.mode, Mode::Title);
        assert_eq!(played[0].settings.volume, 90);
        assert!(!played[0].projected);
    }

    #[test]
    fn failed_outputs_report_once_and_pending_output_can_be_pruned_by_occurrence() {
        struct Broken;
        impl Output for Broken {
            fn play(&self, _: Cue) -> Result<(), String> {
                Err("No output device".into())
            }
            fn stop(&self) -> Result<(), String> {
                Ok(())
            }
            fn cancel(&self, _: u64) -> Result<(), String> {
                Ok(())
            }
            fn retain(&self, _: u64, _: Vec<(String, i64)>) -> Result<(), String> {
                Ok(())
            }
        }
        let mut queue = Queue::default();
        queue.refresh(1, 1000, vec![cue("due", 2000, Mode::Title)]);
        assert_eq!(queue.advance(2000, &Broken), ["No output device"]);
        assert!(queue.advance(2001, &Broken).is_empty());
        let output = Fake::default();
        for (scope, key) in [(1, "removed"), (1, "kept"), (2, "other")] {
            let mut item = cue(key, 2000, Mode::Title);
            item.scope = scope;
            output.play(item).unwrap();
        }
        output.retain(1, vec![("kept".into(), 2000)]).unwrap();
        assert_eq!(output.0.lock().unwrap().len(), 2);
        output.cancel(1).unwrap();
        assert_eq!(output.0.lock().unwrap()[0].key, "other");
    }
}
