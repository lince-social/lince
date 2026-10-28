use super::{array, facet, status, text};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Workflow {
    #[default]
    General,
    Transport,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    #[default]
    Summary,
    List,
    Details,
    Options,
    Map,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub workflow: Workflow,
    pub panel: Panel,
    pub selected: String,
    pub history: bool,
    pub page: usize,
    scope: String,
    general_selection: String,
}

impl Workspace {
    pub fn matches_scope(&self, scope: &str) -> bool {
        self.scope == scope
    }

    pub fn enter(&mut self, scope: &str, selection: &str) {
        self.scope(scope);
        if self.workflow == Workflow::General {
            self.general_selection = selection.into();
        }
        self.workflow = Workflow::Transport;
        self.panel = Panel::Summary;
        self.history = false;
        self.selected.clear();
        self.page = 0;
    }

    pub fn scope(&mut self, scope: &str) {
        if self.scope != scope {
            *self = Self {
                scope: scope.into(),
                ..Self::default()
            };
        }
    }

    pub fn back(&mut self) -> Option<String> {
        match self.panel {
            Panel::Summary => {
                self.workflow = Workflow::General;
                Some(self.general_selection.clone())
            }
            Panel::Details => {
                self.panel = Panel::List;
                None
            }
            _ => {
                self.panel = Panel::Summary;
                None
            }
        }
    }

    pub fn select(&mut self, uid: &str) {
        self.selected = uid.into();
        self.panel = Panel::Details;
    }

    pub fn rows<'a>(&self, rows: &'a [Value], person: &str) -> Vec<&'a Value> {
        rows.iter()
            .filter(|row| is_transport(row))
            .filter(|row| {
                self.history
                    || (!person.is_empty()
                        && (text(row, "creator") == person
                            || array(row, "parties")
                                .iter()
                                .any(|party| text(party, "actor") == person))
                        && (facet(row, "active")
                            || matches!(status(row).as_str(), "active" | "partially_settled"))
                        && row["journey"]["capacity_released"] != true
                        && row["journey"]["expired"] != true)
            })
            .collect()
    }

    pub fn records(row: &Value) -> Vec<String> {
        let mut records = vec![text(row, "uid")];
        for promise in array(row, "promises") {
            if promise["disclosed"]["location"] == false {
                continue;
            }
            for key in ["origin", "destination"] {
                if let Some(uid) = promise["transport"][key].as_str() {
                    records.push(uid.into());
                }
            }
            records.extend(
                array(&promise["transport"], "via")
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        records.retain(|uid| nucleus::valid_uid(uid, "r"));
        records.sort();
        records.dedup();
        records
    }

    pub fn valid(&self) -> bool {
        [&self.scope, &self.general_selection, &self.selected]
            .iter()
            .all(|value| value.len() <= 4096)
    }
}

pub fn is_transport(row: &Value) -> bool {
    array(row, "promises")
        .iter()
        .any(|promise| promise["transport"].is_object())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn active_first_respects_person_and_finished_journeys() {
        let rows = vec![
            json!({"uid":"mine","primary_status":"active","parties":[{"actor":"me"}],"promises":[{"transport":{}}]}),
            json!({"uid":"other","primary_status":"active","parties":[{"actor":"other"}],"promises":[{"transport":{}}]}),
            json!({"uid":"finished","primary_status":"active","parties":[{"actor":"me"}],"promises":[{"transport":{}}],"journey":{"capacity_released":true}}),
            json!({"uid":"ordinary","primary_status":"active","parties":[{"actor":"me"}],"promises":[]}),
        ];
        let mut workspace = Workspace::default();
        workspace.enter("organ/me", "previous");
        assert_eq!(workspace.rows(&rows, "me").len(), 1);
        assert_eq!(workspace.rows(&rows, "me")[0]["uid"], "mine");
        assert!(workspace.rows(&rows, "").is_empty());
        workspace.history = true;
        assert_eq!(workspace.rows(&rows, "me").len(), 3);
    }

    #[test]
    fn panel_navigation_preserves_selection_and_scope_changes_clear_private_state() {
        let mut workspace = Workspace::default();
        workspace.enter("organ/me", "general");
        workspace.select("trip");
        assert_eq!(workspace.back(), None);
        assert_eq!(workspace.panel, Panel::List);
        assert_eq!(workspace.selected, "trip");
        workspace.back();
        assert_eq!(workspace.back(), Some("general".into()));
        let encoded = serde_json::to_vec(&workspace).unwrap();
        let mut restored: Workspace = serde_json::from_slice(&encoded).unwrap();
        assert!(restored.valid());
        restored.enter("organ/me", "general");
        restored.select("private");
        restored.scope("organ/other");
        assert_eq!(restored.workflow, Workflow::General);
        assert!(restored.selected.is_empty());
    }

    #[test]
    fn map_records_skip_undisclosed_locations_and_deduplicate_stops() {
        let transfer = nucleus::new_uid("r");
        let public = nucleus::new_uid("r");
        let hidden = nucleus::new_uid("r");
        let row = json!({"uid":transfer,"promises":[
            {"disclosed":{"location":false},"transport":{"origin":hidden,"destination":hidden,"via":[hidden]}},
            {"transport":{"origin":public,"destination":public,"via":[public,"invalid"]}}
        ]});
        let records = Workspace::records(&row);
        assert_eq!(records.len(), 2);
        assert!(records.contains(&transfer));
        assert!(records.contains(&public));
        assert!(!records.contains(&hidden));
    }
}
