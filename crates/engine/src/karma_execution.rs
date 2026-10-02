use serde::Serialize;

use crate::{Engine, EngineError};

#[derive(Serialize)]
pub struct DeviceExecution {
    pub permitted: bool,
    pub local_running: bool,
    pub executing: bool,
    pub reason: Option<String>,
    pub roster_version: Option<i64>,
    pub executors: Vec<String>,
}

impl Engine {
    pub async fn karma_device_execution(&self) -> Result<DeviceExecution, EngineError> {
        let local_running = store::cells::config(&self.store.pool, "lince.karma-runtime")
            .await?
            .and_then(|value| value["running"].as_bool())
            .unwrap_or(true);
        let Some(cell) = store::cells::local(&self.store.pool).await? else {
            return Ok(DeviceExecution {
                permitted: true,
                local_running,
                executing: local_running,
                reason: None,
                roster_version: None,
                executors: Vec::new(),
            });
        };
        let organ = store::organs::local(&self.store.pool).await?;
        let roster = match organ {
            Some(organ) => self.roster_of(&organ.uid).await?,
            None => None,
        };
        let mut reason = None;
        let mut executors = Vec::new();
        let mut version = None;
        let permitted =
            if let Some(roster) = roster {
                version = Some(roster.roster.version);
                executors = roster
                    .roster
                    .cells
                    .iter()
                    .filter(|cell| cell.may(crate::roster::CAP_KARMA))
                    .map(|cell| cell.cell_uid.clone())
                    .collect();
                if !crate::roster::roster_signature_is_valid(&roster)
                    || !self
                        .key_chains(&roster.roster.organ_uid, &roster.roster.root_key)
                        .await?
                {
                    reason = Some("The Organ roster is expired or no longer authorized".into());
                    false
                } else if !roster.roster.cells.iter().any(|member| {
                    member.cell_uid == cell.uid && member.may(crate::roster::CAP_KARMA)
                }) {
                    reason = Some(
                        "This Cell has no Karma execution permission in the signed Organ roster"
                            .into(),
                    );
                    false
                } else {
                    true
                }
            } else {
                reason = Some("Karma execution needs a signed Organ roster".into());
                false
            };
        if permitted && !local_running {
            reason = Some("Karma is stopped on this Cell".into());
        }
        Ok(DeviceExecution {
            permitted,
            local_running,
            executing: permitted && local_running,
            reason,
            roster_version: version,
            executors,
        })
    }

    pub(crate) async fn require_karma_execution(
        &self,
        target: Option<&str>,
    ) -> Result<(), EngineError> {
        let status = self.karma_device_execution().await?;
        if !status.executing {
            return Err(EngineError::Forbidden(
                status
                    .reason
                    .unwrap_or_else(|| "Karma execution is disabled".into()),
            ));
        }
        if let Some(target) = target
            && !store::executor::runs_here(&self.store.pool, target).await?
        {
            return Err(EngineError::Forbidden(
                "This Cell is not the designated executor".into(),
            ));
        }
        Ok(())
    }

    pub async fn choose_karma_executor(
        &self,
        root: &crate::trust::Signer,
        cell_uid: &str,
        enabled: bool,
        additional: bool,
        expected_version: i64,
    ) -> Result<crate::roster::SignedRoster, EngineError> {
        let _guard = self.roster_execution.lock().await;
        let held = self
            .roster_of(&root.actor_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("No Organ roster to change".into()))?;
        if held.roster.version != expected_version {
            return Err(EngineError::Conflict {
                code: "roster_changed",
                message: "The roster changed; refresh before choosing Karma executors".into(),
            });
        }
        if !held
            .roster
            .cells
            .iter()
            .any(|cell| cell.cell_uid == cell_uid)
        {
            return Err(EngineError::Consequence("Choose an enrolled Cell".into()));
        }
        if !enabled && additional {
            return Err(EngineError::Consequence(
                "Additional execution applies only when enabling a Cell".into(),
            ));
        }
        let mut cells = held.roster.cells;
        for cell in &mut cells {
            if cell.cell_uid == cell_uid || (enabled && !additional) {
                cell.capabilities
                    .retain(|capability| capability != crate::roster::CAP_KARMA);
            }
            if cell.cell_uid == cell_uid && enabled {
                cell.capabilities.push(crate::roster::CAP_KARMA.into());
            }
        }
        self.publish_roster(root, cells).await
    }
}
