use crate::host::{Direction, OutputId, Rectangle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub workspace: u64,
    pub output: String,
    #[serde(default)]
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub id: OutputId,
    pub name: String,
    pub bounds: Rectangle,
    pub active: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Assignments {
    outputs: Vec<Output>,
    preferences: Vec<Preference>,
    current: BTreeMap<u64, OutputId>,
    focused: Option<OutputId>,
    pending: BTreeMap<OutputId, u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignmentError {
    InvalidWorkspace,
    InvalidOutput,
    NeedWorkspace,
    MissingOutput,
}

impl Assignments {
    pub fn restore(preferences: Vec<Preference>) -> Result<Self, AssignmentError> {
        let mut seen = BTreeSet::new();
        let mut active = BTreeSet::new();
        if preferences.len() > 4096 || preferences.iter().any(|preference| {
            preference.workspace == 0
                || !seen.insert(preference.workspace)
                || preference.output.len() > 512
                || preference.active && (preference.output.is_empty() || !active.insert(preference.output.clone()))
        }) {
            return Err(AssignmentError::InvalidWorkspace);
        }
        Ok(Self {
            preferences,
            ..Self::default()
        })
    }

    pub fn preferences(&self) -> &[Preference] {
        &self.preferences
    }

    pub fn retain_workspaces(&mut self, workspaces: &BTreeSet<u64>) {
        self.preferences
            .retain(|preference| workspaces.contains(&preference.workspace));
        self.current
            .retain(|workspace, _| workspaces.contains(workspace));
        self.pending.retain(|_, workspace| workspaces.contains(workspace));
        let mut visible: BTreeSet<_> = self
            .outputs
            .iter()
            .filter(|output| workspaces.contains(&output.active))
            .map(|output| output.active)
            .collect();
        for output in &mut self.outputs {
            if !workspaces.contains(&output.active) {
                if let Some(workspace) = workspaces
                    .iter()
                    .find(|workspace| !visible.contains(workspace))
                    .copied()
                {
                    output.active = workspace;
                    self.current.insert(workspace, output.id);
                    visible.insert(workspace);
                }
            }
        }
    }

    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    pub fn focused(&self) -> Option<OutputId> {
        self.focused
    }

    pub fn register_workspace(&mut self, workspace: u64) -> Result<(), AssignmentError> {
        if workspace == 0
            || self.preferences.len() >= 4096
            || self
                .preferences
                .iter()
                .any(|preference| preference.workspace == workspace)
        {
            return Err(AssignmentError::InvalidWorkspace);
        }
        let output = self
            .focused
            .and_then(|id| self.outputs.iter().find(|output| output.id == id));
        self.preferences.push(Preference {
            workspace,
            output: output.map_or(String::new(), |output| output.name.clone()),
            active: false,
        });
        if let Some(output) = output {
            self.current.insert(workspace, output.id);
        }
        Ok(())
    }

    pub fn connect(
        &mut self,
        id: OutputId,
        name: String,
        bounds: Rectangle,
    ) -> Result<(), AssignmentError> {
        if name.is_empty()
            || name.len() > 512
            || self
                .outputs
                .iter()
                .any(|output| output.id == id || output.name == name)
        {
            return Err(AssignmentError::InvalidOutput);
        }
        let preferred: Vec<_> = self
            .preferences
            .iter()
            .filter(|preference| preference.output == name)
            .map(|preference| preference.workspace)
            .collect();
        let desired = self.preferences.iter().find(|preference| preference.output == name && preference.active)
            .map(|preference| preference.workspace).or_else(|| preferred.first().copied());
        let protected = self.outputs.iter().find(|output| Some(output.id) == self.focused).map(|output| output.active);
        let active = desired.filter(|workspace| Some(*workspace) != protected)
            .or_else(|| preferred.iter().find(|workspace| Some(**workspace) != protected).copied())
            .or_else(|| {
                self.preferences
                    .iter()
                    .find(|preference| {
                        !self
                            .outputs
                            .iter()
                            .any(|output| output.active == preference.workspace)
                    })
                    .map(|preference| preference.workspace)
            })
            .ok_or(AssignmentError::NeedWorkspace)?;
        let mut trial = self.clone();
        trial.outputs.push(Output {
            id,
            name: name.clone(),
            bounds,
            active,
        });
        for workspace in preferred {
            if Some(workspace) != protected {
                trial.current.insert(workspace, id);
            }
        }
        trial.current.insert(active, id);
        for preference in &mut trial.preferences {
            if preference.workspace == active && preference.output.is_empty() {
                preference.output = name.clone();
            }
            trial.current.entry(preference.workspace).or_insert(id);
        }
        let mut occupied: BTreeSet<_> = trial.outputs.iter().map(|output| output.active).collect();
        for output in &mut trial.outputs {
            if output.id == id {
                continue;
            }
            if trial.current.get(&output.active) == Some(&id) {
                output.active = trial
                    .preferences
                    .iter()
                    .find(|preference| {
                        !occupied.contains(&preference.workspace)
                    })
                    .map(|preference| preference.workspace)
                    .ok_or(AssignmentError::NeedWorkspace)?;
                trial.current.insert(output.active, output.id);
                occupied.insert(output.active);
            }
        }
        trial.focused = trial.focused.or(Some(id));
        if let Some(desired) = desired.filter(|desired| *desired != active) {
            trial.pending.insert(id, desired);
        } else {
            trial.remember_active(id);
        }
        *self = trial;
        Ok(())
    }

    pub fn disconnect(&mut self, id: OutputId) -> bool {
        let Some(index) = self.outputs.iter().position(|output| output.id == id) else {
            return false;
        };
        let removed = self.outputs.remove(index);
        self.pending.remove(&id);
        let replacement = self
            .outputs
            .iter()
            .find(|output| Some(output.id) == self.focused)
            .or_else(|| self.outputs.first())
            .map(|output| output.id);
        if let Some(replacement) = replacement {
            for output in self.current.values_mut() {
                if *output == id {
                    *output = replacement;
                }
            }
            if self.focused == Some(id) {
                self.focused = Some(replacement);
                self.outputs
                    .iter_mut()
                    .find(|output| output.id == replacement)
                    .unwrap()
                    .active = removed.active;
            }
        } else {
            self.current.clear();
            self.focused = None;
        }
        true
    }

    pub fn resize(&mut self, id: OutputId, bounds: Rectangle) -> Result<(), AssignmentError> {
        self.outputs
            .iter_mut()
            .find(|output| output.id == id)
            .ok_or(AssignmentError::MissingOutput)?
            .bounds = bounds;
        Ok(())
    }

    pub fn select(
        &mut self,
        output: OutputId,
        workspace: u64,
    ) -> Result<OutputId, AssignmentError> {
        if !self
            .preferences
            .iter()
            .any(|preference| preference.workspace == workspace)
        {
            return Err(AssignmentError::InvalidWorkspace);
        }
        if !self.outputs.iter().any(|candidate| candidate.id == output) {
            return Err(AssignmentError::MissingOutput);
        }
        if let Some(visible) = self
            .outputs
            .iter()
            .find(|candidate| candidate.active == workspace)
        {
            let selected = visible.id;
            self.focused = Some(selected);
            self.pending.remove(&selected);
            self.remember_active(selected);
            self.restore_pending();
            return Ok(selected);
        }
        let selected = self
            .outputs
            .iter_mut()
            .find(|candidate| candidate.id == output)
            .unwrap();
        selected.active = workspace;
        self.current.insert(workspace, output);
        self.focused = Some(output);
        self.pending.remove(&output);
        self.remember_active(output);
        self.restore_pending();
        Ok(output)
    }

    pub fn move_workspace(
        &mut self,
        workspace: u64,
        target: OutputId,
    ) -> Result<(), AssignmentError> {
        let target_name = self
            .outputs
            .iter()
            .find(|output| output.id == target)
            .ok_or(AssignmentError::MissingOutput)?
            .name
            .clone();
        let preference = self
            .preferences
            .iter_mut()
            .find(|preference| preference.workspace == workspace)
            .ok_or(AssignmentError::InvalidWorkspace)?;
        preference.output = target_name;
        preference.active = false;
        self.pending.retain(|output, pending| *output != target && *pending != workspace);
        let previous = self
            .outputs
            .iter()
            .find(|output| output.active == workspace)
            .map(|output| output.id);
        let replaced = self
            .outputs
            .iter()
            .find(|output| output.id == target)
            .unwrap()
            .active;
        if let Some(previous) = previous.filter(|previous| *previous != target) {
            self.outputs
                .iter_mut()
                .find(|output| output.id == previous)
                .unwrap()
                .active = replaced;
            self.current.insert(replaced, previous);
        }
        self.outputs
            .iter_mut()
            .find(|output| output.id == target)
            .unwrap()
            .active = workspace;
        self.current.insert(workspace, target);
        self.focused = Some(target);
        self.remember_active(target);
        if let Some(previous) = previous.filter(|previous| *previous != target) {
            self.remember_active(previous);
        }
        Ok(())
    }

    fn remember_active(&mut self, output: OutputId) {
        let Some(head) = self.outputs.iter().find(|head| head.id == output) else { return; };
        if !self.preferences.iter().any(|preference| preference.workspace == head.active && preference.output == head.name) {
            return;
        }
        for preference in &mut self.preferences {
            if preference.output == head.name {
                preference.active = preference.workspace == head.active;
            }
        }
    }

    fn restore_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for (target, workspace) in pending {
            let Some(head) = self.outputs.iter().find(|head| head.id == target) else { continue; };
            if !self.preferences.iter().any(|preference| preference.workspace == workspace && preference.output == head.name) {
                continue;
            }
            if self.focused == Some(target) || self.outputs.iter().any(|output| Some(output.id) == self.focused && output.active == workspace) {
                self.pending.insert(target, workspace);
                continue;
            }
            let focused = self.focused;
            let preferences = self.preferences.clone();
            if self.move_workspace(workspace, target).is_ok() {
                self.focused = focused;
                self.preferences = preferences;
            }
        }
    }

    pub fn pending_restorations(&self) -> impl Iterator<Item = (OutputId, u64)> + '_ {
        self.pending.iter().map(|(output, workspace)| (*output, *workspace))
    }

    pub fn restore_initial(&mut self, workspace: u64) -> Result<(), AssignmentError> {
        if !self.preferences.iter().any(|preference| preference.workspace == workspace) {
            return Err(AssignmentError::InvalidWorkspace);
        }
        let mut trial = self.clone();
        let previous = trial.focused;
        trial.focused = None;
        trial.restore_pending();
        let output = trial.outputs.iter().find(|output| output.active == workspace)
            .map(|output| output.id)
            .or_else(|| trial.current.get(&workspace).copied())
            .or(previous)
            .or_else(|| trial.outputs.first().map(|output| output.id))
            .ok_or(AssignmentError::MissingOutput)?;
        trial.select(output, workspace)?;
        *self = trial;
        Ok(())
    }

    pub fn directional(&self, direction: Direction) -> Option<OutputId> {
        let current = self
            .outputs
            .iter()
            .find(|output| Some(output.id) == self.focused)?;
        let axis = match direction {
            Direction::Left | Direction::Right => 0,
            _ => 1,
        };
        let sign = match direction {
            Direction::Left | Direction::Up => -1.0,
            _ => 1.0,
        };
        let center = |output: &Output, axis: usize| {
            output.bounds.origin()[axis] + output.bounds.size()[axis] * 0.5
        };
        self.outputs
            .iter()
            .filter(|output| output.id != current.id)
            .filter_map(|output| {
                let primary = (center(output, axis) - center(current, axis)) * sign;
                if primary <= 0.0 {
                    return None;
                }
                let cross = (center(output, 1 - axis) - center(current, 1 - axis)).abs();
                Some((output.id, primary.hypot(cross) * (1.0 + cross / primary)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
            .map(|candidate| candidate.0)
    }

    pub fn focus(&mut self, output: OutputId) -> Result<(), AssignmentError> {
        if !self.outputs.iter().any(|candidate| candidate.id == output) {
            return Err(AssignmentError::MissingOutput);
        }
        self.focused = Some(output);
        self.pending.remove(&output);
        self.remember_active(output);
        self.restore_pending();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f64) -> Rectangle {
        Rectangle::new([x, 0.0], [1000.0, 700.0]).unwrap()
    }

    fn setup() -> Assignments {
        let mut assignments = Assignments::restore(vec![
            Preference {
                workspace: 1,
                output: "eDP-1".into(),
                active: false,
            },
            Preference {
                workspace: 2,
                output: "DP-1".into(),
                active: false,
            },
            Preference {
                workspace: 3,
                output: "DP-1".into(),
                active: false,
            },
        ])
        .unwrap();
        assignments
            .connect(OutputId(1), "eDP-1".into(), bounds(0.0))
            .unwrap();
        assignments
            .connect(OutputId(2), "DP-1".into(), bounds(1000.0))
            .unwrap();
        assignments
    }

    #[test]
    fn selecting_a_visible_workspace_focuses_its_monitor() {
        let mut assignments = setup();
        assert_eq!(assignments.select(OutputId(1), 2), Ok(OutputId(2)));
        assert_eq!(assignments.outputs[0].active, 1);
        assert_eq!(assignments.focused(), Some(OutputId(2)));
        assert_eq!(assignments.select(OutputId(2), 3), Ok(OutputId(2)));
        assert_eq!(assignments.outputs[1].active, 3);
        assert_eq!(assignments.directional(Direction::Left), Some(OutputId(1)));
        assert_eq!(assignments.directional(Direction::Right), None);
    }

    #[test]
    fn directional_navigation_uses_geometry_and_deterministic_ties_in_every_direction() {
        let output = |id, origin| Output {
            id: OutputId(id),
            name: format!("Display-{id}"),
            bounds: Rectangle::new(origin, [800.0, 600.0]).unwrap(),
            active: id,
        };
        let mut assignments = Assignments {
            outputs: vec![
                output(50, [0.0, 0.0]),
                output(9, [10.0, 10_000.0]),
                output(7, [900.0, 0.0]),
                output(3, [900.0, 0.0]),
                output(5, [-1000.0, 0.0]),
                output(6, [0.0, -700.0]),
                output(8, [0.0, 700.0]),
            ],
            focused: Some(OutputId(50)),
            ..Default::default()
        };
        for _ in 0..2 {
            for (direction, expected) in [
                (Direction::Left, 5),
                (Direction::Down, 8),
                (Direction::Up, 6),
                (Direction::Right, 3),
            ] {
                assert_eq!(assignments.directional(direction), Some(OutputId(expected)));
            }
            assignments.outputs.reverse();
        }
        assignments
            .outputs
            .retain(|output| output.id == OutputId(50));
        for direction in [
            Direction::Left,
            Direction::Down,
            Direction::Up,
            Direction::Right,
        ] {
            assert_eq!(assignments.directional(direction), None);
        }
    }

    #[test]
    fn unplug_and_reconnect_preserve_preferred_monitor_and_focus() {
        let mut assignments = setup();
        assignments.select(OutputId(2), 2).unwrap();
        assert!(assignments.disconnect(OutputId(2)));
        assert_eq!(assignments.outputs[0].active, 2);
        assert_eq!(assignments.current.get(&3), Some(&OutputId(1)));
        assignments
            .connect(OutputId(8), "DP-1".into(), bounds(1000.0))
            .unwrap();
        assert_eq!(assignments.outputs[0].active, 1);
        assert_eq!(assignments.outputs[1].active, 2);
        assert_eq!(assignments.current.get(&3), Some(&OutputId(8)));
        assert_eq!(assignments.preferences[1].output, "DP-1");
    }

    #[test]
    fn initial_workspace_homes_follow_their_assigned_monitor_instead_of_the_first_connection() {
        let mut assignments = Assignments::default();
        for workspace in 1..=3 {
            assignments.register_workspace(workspace).unwrap();
        }
        assignments
            .connect(OutputId(1), "Display-A".into(), bounds(0.0))
            .unwrap();
        assert_eq!(assignments.preferences[0].output, "Display-A");
        assert!(assignments.preferences[1].output.is_empty());
        assignments
            .connect(OutputId(2), "Display-B".into(), bounds(1000.0))
            .unwrap();
        assert_eq!(assignments.preferences[1].output, "Display-B");
        let preferences = assignments.preferences.clone();
        assignments.focus(OutputId(2)).unwrap();
        assignments.disconnect(OutputId(2));
        assert_eq!(assignments.outputs[0].active, 2);
        assignments
            .connect(OutputId(8), "Display-B".into(), bounds(1000.0))
            .unwrap();
        assert_eq!(assignments.outputs[0].active, 1);
        assert_eq!(assignments.outputs[1].active, 2);
        assert_eq!(assignments.preferences, preferences);
    }

    #[test]
    fn explicit_moves_swap_visible_workspaces_without_mirroring() {
        let mut assignments = setup();
        assignments.move_workspace(1, OutputId(2)).unwrap();
        assert_eq!(assignments.outputs[0].active, 2);
        assert_eq!(assignments.outputs[1].active, 1);
        assert_eq!(assignments.preferences[0].output, "DP-1");
        let visible: BTreeSet<_> = assignments
            .outputs
            .iter()
            .map(|output| output.active)
            .collect();
        assert_eq!(visible.len(), assignments.outputs.len());
    }

    #[test]
    fn insufficient_workspaces_do_not_partially_attach_an_output() {
        let mut assignments = Assignments::default();
        assignments.register_workspace(1).unwrap();
        assignments
            .connect(OutputId(1), "eDP-1".into(), bounds(0.0))
            .unwrap();
        assert_eq!(
            assignments.connect(OutputId(2), "DP-1".into(), bounds(1000.0)),
            Err(AssignmentError::NeedWorkspace)
        );
        assert_eq!(assignments.outputs.len(), 1);
        assert_eq!(assignments.focused(), Some(OutputId(1)));
        assert!(assignments.disconnect(OutputId(1)));
        assert_eq!(assignments.focused(), None);
        assignments
            .connect(OutputId(3), "eDP-1".into(), bounds(0.0))
            .unwrap();
        assert_eq!(assignments.outputs[0].active, 1);
    }

    #[test]
    fn reconnecting_preferred_output_keeps_a_workspace_on_other_monitors() {
        let mut assignments = Assignments::restore(vec![
            Preference {
                workspace: 1,
                output: "DP-1".into(),
                active: false,
            },
            Preference {
                workspace: 2,
                output: "DP-1".into(),
                active: false,
            },
        ])
        .unwrap();
        assignments
            .connect(OutputId(1), "eDP-1".into(), bounds(0.0))
            .unwrap();
        assignments
            .connect(OutputId(2), "DP-1".into(), bounds(1000.0))
            .unwrap();
        assert_eq!(assignments.outputs[0].active, 1);
        assert_eq!(assignments.outputs[1].active, 2);
        assert_eq!(assignments.current.get(&1), Some(&OutputId(1)));
        assert_eq!(assignments.pending_restorations().collect::<Vec<_>>(), vec![(OutputId(2), 1)]);
        assert_eq!(assignments.preferences[0].output, "DP-1");
    }

    #[test]
    fn returning_head_waits_for_the_input_workspace_then_restores_without_stealing_focus() {
        let mut assignments = setup();
        assignments.focus(OutputId(2)).unwrap();
        assignments.disconnect(OutputId(2));
        assignments.connect(OutputId(3), "DP-1".into(), bounds(1000.0)).unwrap();
        assert_eq!(assignments.focused(), Some(OutputId(1)));
        assert_eq!(assignments.outputs[0].active, 2);
        assert_eq!(assignments.outputs[1].active, 3);
        assert_eq!(assignments.pending_restorations().count(), 1);
        assignments.select(OutputId(1), 1).unwrap();
        assert_eq!(assignments.outputs[0].active, 1);
        assert_eq!(assignments.outputs[1].active, 2);
        assert_eq!(assignments.focused(), Some(OutputId(1)));
        assert_eq!(assignments.pending_restorations().count(), 0);
        assert_eq!(assignments.preferences.iter().find(|preference| preference.workspace == 2).unwrap().output, "DP-1");
    }

    #[test]
    fn an_explicit_home_move_while_the_old_head_is_absent_survives_its_return() {
        let mut assignments = setup();
        assignments.focus(OutputId(1)).unwrap();
        assignments.disconnect(OutputId(2));
        assignments.move_workspace(2, OutputId(1)).unwrap();
        assert_eq!(assignments.preferences.iter().find(|preference| preference.workspace == 2).unwrap().output, "eDP-1");
        assignments.connect(OutputId(3), "DP-1".into(), bounds(1000.0)).unwrap();
        assert_eq!(assignments.outputs.iter().find(|output| output.id == OutputId(1)).unwrap().active, 2);
        assert_ne!(assignments.outputs.iter().find(|output| output.id == OutputId(3)).unwrap().active, 2);
        assert_eq!(assignments.current.get(&2), Some(&OutputId(1)));
        assert_eq!(assignments.focused(), Some(OutputId(1)));
        assert_eq!(assignments.pending_restorations().count(), 0);
    }

    #[test]
    fn explicit_home_change_retires_an_older_return_request() {
        let mut assignments = setup();
        assignments.focus(OutputId(2)).unwrap();
        assignments.disconnect(OutputId(2));
        assignments.connect(OutputId(3), "DP-1".into(), bounds(1000.0)).unwrap();
        assert_eq!(assignments.pending_restorations().count(), 1);
        assignments.move_workspace(2, OutputId(1)).unwrap();
        assert_eq!(assignments.pending_restorations().count(), 0);
        assignments.disconnect(OutputId(3));
        assignments.connect(OutputId(4), "DP-1".into(), bounds(1000.0)).unwrap();
        assert_eq!(assignments.outputs[0].active, 2);
        assert_eq!(assignments.outputs[1].active, 3);
        assert_eq!(assignments.preferences.iter().find(|preference| preference.workspace == 2).unwrap().output, "eDP-1");
    }

    #[test]
    fn initial_restore_resolves_provisional_assignments_before_selecting_the_saved_workspace() {
        let preferences = vec![
            Preference { workspace: 1, output: "DP-1".into(), active: true },
            Preference { workspace: 2, output: "DP-1".into(), active: false },
        ];
        let mut assignments = Assignments::restore(preferences.clone()).unwrap();
        assignments.connect(OutputId(1), "eDP-1".into(), bounds(0.0)).unwrap();
        assignments.connect(OutputId(2), "DP-1".into(), bounds(1000.0)).unwrap();
        assignments.restore_initial(1).unwrap();
        assert_eq!(assignments.outputs[0].active, 2);
        assert_eq!(assignments.outputs[1].active, 1);
        assert_eq!(assignments.focused(), Some(OutputId(2)));
        assert_eq!(assignments.preferences(), preferences);
        assert_eq!(assignments.pending_restorations().count(), 0);
    }

    #[test]
    fn active_preferences_reject_ambiguous_or_unnamed_homes() {
        assert!(Assignments::restore(vec![Preference { workspace: 1, output: String::new(), active: true }]).is_err());
        assert!(Assignments::restore(vec![
            Preference { workspace: 1, output: "DP-1".into(), active: true },
            Preference { workspace: 2, output: "DP-1".into(), active: true },
        ]).is_err());
    }
}
