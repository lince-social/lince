use crate::host::{
    OutputId,
    display::{DisplayConfiguration, DisplaySnapshot, OutputSettings},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(super) struct Draft {
    pub origin: DisplaySnapshot,
    pub settings: BTreeMap<OutputId, OutputSettings>,
    pub enabled: BTreeSet<OutputId>,
}

impl Draft {
    pub fn new(origin: DisplaySnapshot) -> Self {
        let settings = origin
            .heads
            .iter()
            .filter_map(|head| {
                let mode = head
                    .current
                    .and_then(|index| head.modes.get(index))
                    .or_else(|| head.modes.iter().find(|mode| mode.preferred))
                    .or_else(|| head.modes.first())?;
                Some((
                    head.id,
                    OutputSettings { color: head.color, adaptive_sync: head.adaptive_sync, rotation: head.rotation,
                        mode: mode.clone(),
                        position: head.position,
                        scale: head.scale,
                    },
                ))
            })
            .collect();
        let enabled = origin
            .heads
            .iter()
            .filter(|head| head.current.is_some())
            .map(|head| head.id)
            .collect();
        Self {
            origin,
            settings,
            enabled,
        }
    }

    pub fn normalized(&self) -> Result<DisplayConfiguration, String> {
        if self.enabled.is_empty() {
            return Err("Keep at least one monitor enabled.".into());
        }
        let minimum: [i64; 2] = std::array::from_fn(|axis| {
            self.enabled
                .iter()
                .filter_map(|id| self.settings.get(id))
                .map(|settings| i64::from(settings.position[axis]))
                .min()
                .unwrap_or(0)
                .min(0)
        });
        self.origin
            .heads
            .iter()
            .map(|head| {
                let Some(settings) = self
                    .settings
                    .get(&head.id)
                    .filter(|_| self.enabled.contains(&head.id))
                else {
                    return Ok((head.id, None));
                };
                let mut settings = settings.clone();
                for axis in 0..2 {
                    settings.position[axis] =
                        i32::try_from(i64::from(settings.position[axis]) - minimum[axis]).map_err(
                            |_| "The monitor arrangement exceeds supported coordinates.".to_owned(),
                        )?;
                }
                Ok((head.id, Some(settings)))
            })
            .collect()
    }

    pub fn normalize(&mut self) -> Result<(), String> {
        for (id, settings) in self.normalized()? {
            if let Some(settings) = settings {
                self.settings.insert(id, settings);
            }
        }
        Ok(())
    }

    pub fn size(&self, id: OutputId) -> [f32; 2] {
        self.settings
            .get(&id)
            .map(|settings| {
                settings.logical_size().map(|size| size as f32)
            })
            .unwrap_or([1.0; 2])
    }

    pub fn snap(&mut self, id: OutputId, threshold: f32) {
        let size = self.size(id);
        let Some(settings) = self.settings.get(&id) else {
            return;
        };
        let position = settings.position;
        let mut next = position;
        for axis in 0..2 {
            let mut best = threshold;
            for other in self.enabled.iter().copied().filter(|other| *other != id) {
                let Some(settings) = self.settings.get(&other) else {
                    continue;
                };
                let start = settings.position[axis] as f32;
                let end = start + self.size(other)[axis];
                for candidate in [start, end, start - size[axis], end - size[axis]] {
                    let distance = (candidate - position[axis] as f32).abs();
                    if distance < best {
                        best = distance;
                        next[axis] = candidate.round() as i32;
                    }
                }
            }
        }
        if let Some(settings) = self.settings.get_mut(&id) {
            settings.position = next;
        }
    }

    pub fn align(&mut self, id: OutputId, reference: OutputId, direction: u8) {
        let size = self.size(id);
        let reference_size = self.size(reference);
        let Some(reference) = self.settings.get(&reference) else {
            return;
        };
        let mut position = reference.position;
        match direction {
            0 => position[0] -= size[0] as i32,
            1 => position[0] += reference_size[0] as i32,
            2 => position[1] -= size[1] as i32,
            _ => position[1] += reference_size[1] as i32,
        }
        if let Some(settings) = self.settings.get_mut(&id) {
            settings.position = position;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::display::{OutputHead, OutputMode};

    fn draft() -> Draft {
        Draft::new(DisplaySnapshot {
            revision: 7,
            heads: [1, 2]
                .map(|id| OutputHead { color: Default::default(), adaptive_sync: Default::default(), rotation: Default::default(),
                    id: OutputId(id),
                    name: format!("DP-{id}"),
                    modes: vec![OutputMode {
                        size: [1920, 1080],
                        refresh_mhz: 60000,
                        preferred: true,
                    }],
                    current: Some(0),
                    position: [(id as i32 - 1) * 1920, 0],
                    scale: 1.0,
                })
                .into(),
            ..Default::default()
        })
    }

    #[test]
    fn moving_a_monitor_left_or_above_translates_the_enabled_layout_without_changing_modes_or_identity()
     {
        let mut draft = draft();
        draft.settings.get_mut(&OutputId(2)).unwrap().position = [-1920, -1080];
        let normalized = draft.normalized().unwrap();
        assert_eq!(
            normalized[&OutputId(1)].as_ref().unwrap().position,
            [1920, 1080]
        );
        assert_eq!(normalized[&OutputId(2)].as_ref().unwrap().position, [0, 0]);
        assert_eq!(
            normalized[&OutputId(2)].as_ref().unwrap().mode,
            draft.origin.heads[1].modes[0]
        );
        assert_eq!(draft.settings[&OutputId(2)].position, [-1920, -1080]);
    }

    #[test]
    fn alignment_and_snap_use_logical_scaled_monitor_edges() {
        let mut draft = draft();
        draft.settings.get_mut(&OutputId(1)).unwrap().scale = 2.0;
        draft.align(OutputId(2), OutputId(1), 1);
        assert_eq!(draft.settings[&OutputId(2)].position, [960, 0]);
        draft.settings.get_mut(&OutputId(2)).unwrap().position = [966, 4];
        draft.snap(OutputId(2), 12.0);
        assert_eq!(draft.settings[&OutputId(2)].position, [960, 0]);
        draft.align(OutputId(2), OutputId(1), 2);
        assert_eq!(draft.settings[&OutputId(2)].position, [0, -1080]);
    }

    #[test]
    fn disabled_monitors_remain_in_the_catalogue_but_do_not_set_the_normalization_origin() {
        let mut draft = draft();
        draft.enabled.remove(&OutputId(2));
        draft.settings.get_mut(&OutputId(2)).unwrap().position = [-9999, -9999];
        let configuration = draft.normalized().unwrap();
        assert_eq!(configuration.len(), 2);
        assert!(configuration[&OutputId(2)].is_none());
        assert_eq!(
            configuration[&OutputId(1)].as_ref().unwrap().position,
            [0, 0]
        );
        draft.enabled.clear();
        assert!(draft.normalized().is_err());
    }
}
