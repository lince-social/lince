//! Editable display settings and logical monitor positions.

use crate::host::{
    Direction, OutputId,
    display::{DisplayConfiguration, DisplaySnapshot, OutputSettings},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display},
};

pub(crate) struct Draft {
    pub(crate) origin: DisplaySnapshot,
    pub(crate) settings: BTreeMap<OutputId, OutputSettings>,
    pub(crate) enabled: BTreeSet<OutputId>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ArrangementError {
    NoEnabledMonitor,
    CoordinateOverflow,
}

impl Display for ArrangementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoEnabledMonitor => "Keep at least one monitor enabled.",
            Self::CoordinateOverflow => "The monitor arrangement exceeds supported coordinates.",
        })
    }
}

impl std::error::Error for ArrangementError {}

impl Draft {
    pub(crate) fn new(origin: DisplaySnapshot) -> Self {
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
                    OutputSettings {
                        mode: mode.clone(),
                        position: head.position,
                        scale: head.scale,
                        rotation: head.rotation,
                        adaptive_sync: head.adaptive_sync,
                        color: head.color,
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

    fn minimum(&self) -> Result<[i64; 2], ArrangementError> {
        if self.enabled.is_empty() {
            return Err(ArrangementError::NoEnabledMonitor);
        }
        Ok(self
            .enabled
            .iter()
            .filter_map(|id| self.settings.get(id))
            .fold([0, 0], |[min_x, min_y], settings| {
                let [x, y] = settings.position.map(i64::from);
                [min_x.min(x), min_y.min(y)]
            }))
    }

    fn active_settings(&self, id: OutputId) -> Option<&OutputSettings> {
        self.settings
            .get(&id)
            .filter(|_| self.enabled.contains(&id))
    }

    /// Checks that the active positions can be translated without constructing a configuration.
    pub(crate) fn validate(&self) -> Result<(), ArrangementError> {
        let minimum = self.minimum()?;
        for head in &self.origin.heads {
            if let Some(settings) = self.active_settings(head.id) {
                translated(settings.position, minimum)?;
            }
        }
        Ok(())
    }

    fn normalized(&self) -> Result<DisplayConfiguration, ArrangementError> {
        let minimum = self.minimum()?;
        self.origin
            .heads
            .iter()
            .map(|head| {
                let settings = self.active_settings(head.id).map(|settings| {
                    translated(settings.position, minimum).map(|position| OutputSettings {
                        position,
                        ..settings.clone()
                    })
                });
                Ok((head.id, settings.transpose()?))
            })
            .collect()
    }

    /// Updates the draft positions and returns the corresponding backend configuration.
    pub(crate) fn normalize(&mut self) -> Result<DisplayConfiguration, ArrangementError> {
        let configuration = self.normalized()?;
        for (id, normalized) in &configuration {
            if let Some(normalized) = normalized
                && let Some(settings) = self.settings.get_mut(id)
            {
                settings.position = normalized.position;
            }
        }
        Ok(configuration)
    }

    pub(crate) fn size(&self, id: OutputId) -> [f32; 2] {
        self.settings.get(&id).map_or([1.0; 2], diagram_size)
    }

    pub(crate) fn snap(&mut self, id: OutputId, threshold: f32) {
        let Some(settings) = self.settings.get(&id) else {
            return;
        };
        let size = diagram_size(settings);
        let position = settings.position;
        let mut next = position;
        let mut distances = [threshold; 2];
        for other in self.enabled.iter().copied().filter(|other| *other != id) {
            let Some(settings) = self.settings.get(&other) else {
                continue;
            };
            let other_size = diagram_size(settings);
            for axis in 0..2 {
                let start = settings.position[axis] as f32;
                let end = start + other_size[axis];
                for candidate in [start, end, start - size[axis], end - size[axis]] {
                    let distance = (candidate - position[axis] as f32).abs();
                    if distance < distances[axis] {
                        distances[axis] = distance;
                        next[axis] = candidate.round() as i32;
                    }
                }
            }
        }
        if let Some(settings) = self.settings.get_mut(&id) {
            settings.position = next;
        }
    }

    pub(crate) fn align(
        &mut self,
        id: OutputId,
        reference: OutputId,
        direction: Direction,
    ) -> Result<(), ArrangementError> {
        let Some(reference) = self.settings.get(&reference) else {
            return Ok(());
        };
        let [x, y] = reference.position;
        let [reference_width, reference_height] = reference.logical_size();
        let Some(settings) = self.settings.get_mut(&id) else {
            return Ok(());
        };
        let [width, height] = settings.logical_size();
        let position = match direction {
            Direction::Left => x.checked_sub(width).map(|x| [x, y]),
            Direction::Right => x.checked_add(reference_width).map(|x| [x, y]),
            Direction::Up => y.checked_sub(height).map(|y| [x, y]),
            Direction::Down => y.checked_add(reference_height).map(|y| [x, y]),
        };
        settings.position = position.ok_or(ArrangementError::CoordinateOverflow)?;
        Ok(())
    }
}

fn translated([x, y]: [i32; 2], [min_x, min_y]: [i64; 2]) -> Result<[i32; 2], ArrangementError> {
    let coordinate = |position, minimum| {
        i32::try_from(i64::from(position) - minimum)
            .map_err(|_| ArrangementError::CoordinateOverflow)
    };
    Ok([coordinate(x, min_x)?, coordinate(y, min_y)?])
}

fn diagram_size(settings: &OutputSettings) -> [f32; 2] {
    settings.logical_size().map(|size| size as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::display::{OutputHead, OutputMode};

    fn draft() -> Draft {
        Draft::new(DisplaySnapshot {
            revision: 7,
            heads: [1, 2]
                .map(|id| OutputHead {
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
                    rotation: Default::default(),
                    adaptive_sync: false,
                    color: Default::default(),
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
        assert_eq!(draft.normalize().unwrap(), normalized);
        assert_eq!(draft.settings[&OutputId(2)].position, [0, 0]);
    }

    #[test]
    fn alignment_and_snap_use_logical_scaled_monitor_edges() {
        let mut draft = draft();
        draft.settings.get_mut(&OutputId(1)).unwrap().scale = 2.0;
        assert_eq!(draft.size(OutputId(1)), [960.0, 540.0]);
        draft
            .align(OutputId(2), OutputId(1), Direction::Right)
            .unwrap();
        assert_eq!(draft.settings[&OutputId(2)].position, [960, 0]);
        draft.settings.get_mut(&OutputId(2)).unwrap().position = [966, 4];
        draft.snap(OutputId(2), 12.0);
        assert_eq!(draft.settings[&OutputId(2)].position, [960, 0]);
        draft
            .align(OutputId(2), OutputId(1), Direction::Up)
            .unwrap();
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
        assert_eq!(draft.validate(), Err(ArrangementError::NoEnabledMonitor));
    }

    #[test]
    fn out_of_range_layout_edits_leave_the_draft_unchanged() {
        let mut draft = draft();
        let reference = OutputId(1);
        let moved = OutputId(2);
        draft.settings.get_mut(&reference).unwrap().position = [i32::MAX, 0];
        let previous = draft.settings[&moved].position;
        assert_eq!(
            draft.align(moved, reference, Direction::Right),
            Err(ArrangementError::CoordinateOverflow)
        );
        assert_eq!(draft.settings[&moved].position, previous);

        draft.settings.get_mut(&reference).unwrap().position = [i32::MIN, 0];
        assert_eq!(draft.validate(), Err(ArrangementError::CoordinateOverflow));
        assert_eq!(draft.normalize(), Err(ArrangementError::CoordinateOverflow));
        assert_eq!(draft.settings[&reference].position, [i32::MIN, 0]);
        assert_eq!(draft.settings[&moved].position, previous);
    }
}
