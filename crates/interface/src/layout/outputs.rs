use crate::host::{Direction, OutputId, Rectangle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub workspace: u64,
    pub output: String,
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
        if preferences.iter().any(|preference| {
            preference.workspace == 0
                || !seen.insert(preference.workspace)
                || preference.output.len() > 512
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
        let active = preferred
            .first()
            .copied()
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
            trial.current.insert(workspace, id);
        }
        trial.current.insert(active, id);
        for preference in &mut trial.preferences {
            if preference.output.is_empty() {
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
                let previous = output.active;
                output.active = trial
                    .preferences
                    .iter()
                    .find(|preference| {
                        (trial.current.get(&preference.workspace) == Some(&output.id)
                            || preference.workspace == previous
                            || !occupied.contains(&preference.workspace))
                            && preference.workspace != active
                    })
                    .map(|preference| preference.workspace)
                    .ok_or(AssignmentError::NeedWorkspace)?;
                trial.current.insert(output.active, output.id);
                occupied.insert(output.active);
            }
        }
        trial.focused = trial.focused.or(Some(id));
        *self = trial;
        Ok(())
    }

    pub fn disconnect(&mut self, id: OutputId) -> bool {
        let Some(index) = self.outputs.iter().position(|output| output.id == id) else {
            return false;
        };
        let removed = self.outputs.remove(index);
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
            self.focused = Some(visible.id);
            return Ok(visible.id);
        }
        let selected = self
            .outputs
            .iter_mut()
            .find(|candidate| candidate.id == output)
            .unwrap();
        selected.active = workspace;
        self.current.insert(workspace, output);
        self.focused = Some(output);
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
                (primary > 0.0).then_some((
                    output.id,
                    primary,
                    (center(output, 1 - axis) - center(current, 1 - axis)).abs(),
                ))
            })
            .min_by(|a, b| {
                a.1.total_cmp(&b.1)
                    .then_with(|| a.2.total_cmp(&b.2))
                    .then_with(|| a.0.cmp(&b.0))
            })
            .map(|candidate| candidate.0)
    }

    pub fn focus(&mut self, output: OutputId) -> Result<(), AssignmentError> {
        if !self.outputs.iter().any(|candidate| candidate.id == output) {
            return Err(AssignmentError::MissingOutput);
        }
        self.focused = Some(output);
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
            },
            Preference {
                workspace: 2,
                output: "DP-1".into(),
            },
            Preference {
                workspace: 3,
                output: "DP-1".into(),
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
            },
            Preference {
                workspace: 2,
                output: "DP-1".into(),
            },
        ])
        .unwrap();
        assignments
            .connect(OutputId(1), "eDP-1".into(), bounds(0.0))
            .unwrap();
        assignments
            .connect(OutputId(2), "DP-1".into(), bounds(1000.0))
            .unwrap();
        assert_eq!(assignments.outputs[0].active, 2);
        assert_eq!(assignments.outputs[1].active, 1);
        assert_eq!(assignments.current.get(&2), Some(&OutputId(1)));
        assert_eq!(assignments.preferences[0].output, "DP-1");
    }
}
