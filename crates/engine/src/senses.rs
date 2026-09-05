use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::error::EngineError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteOpen {
    pub promise_uid: String,
    pub organ: String,
    pub proximity: u32,
    pub concept: Option<String>,
    pub unit: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    #[serde(default = "half")]
    pub confidence: f64,
}

fn half() -> f64 {
    0.5
}

#[derive(Debug, Clone)]
pub struct MatchRule {
    pub watch_concept: Option<String>,
    pub max_proximity: u32,
    pub min_confidence: f64,
}

impl Default for MatchRule {
    fn default() -> Self {
        MatchRule {
            watch_concept: None,
            max_proximity: 1,
            min_confidence: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub local_promise: String,
    pub remote_promise: String,
    pub organ: String,
    pub score: f64,
}

impl From<store::senses::SenseRuleRow> for MatchRule {
    fn from(row: store::senses::SenseRuleRow) -> Self {
        MatchRule {
            watch_concept: row.watch_concept,
            max_proximity: row.max_proximity,
            min_confidence: row.min_confidence,
        }
    }
}

impl From<store::senses::RemoteOpenRow> for RemoteOpen {
    fn from(row: store::senses::RemoteOpenRow) -> Self {
        RemoteOpen {
            promise_uid: row.promise_uid,
            organ: row.organ,
            proximity: row.proximity,
            concept: row.concept,
            unit: row.unit,
            delta: row.delta,
            window_start: row.window_start,
            window_end: row.window_end,
            confidence: row.confidence,
        }
    }
}

impl Engine {
    pub async fn senses_pass(&self) -> Result<Vec<String>, EngineError> {
        let rules = store::senses::active_sense_rules(&self.store.pool).await?;
        if rules.is_empty() {
            return Ok(vec![]);
        }
        let cache: Vec<RemoteOpen> = store::senses::list_remote_open(&self.store.pool)
            .await?
            .into_iter()
            .map(RemoteOpen::from)
            .collect();
        if cache.is_empty() {
            return Ok(vec![]);
        }
        let mut asked = store::misc::open_decision_subjects(&self.store.pool).await?;
        let mut created = Vec::new();
        for rule_row in rules {
            let rule = MatchRule::from(rule_row);
            for draft in self.senses_match(&rule, &cache).await? {
                let subject = format!("{}|{}", draft.local_promise, draft.remote_promise);
                if !asked.insert((subject.clone(), "draft".to_string())) {
                    continue;
                }
                created.push(
                    store::misc::create_decision(
                        &self.store.pool,
                        &subject,
                        "draft",
                        &format!(
                            "Match: your promise {} meets {} from {} (score {:.2})",
                            draft.local_promise, draft.remote_promise, draft.organ, draft.score
                        ),
                        &serde_json::json!([
                            {
                                "label": "propose",
                                "remote": draft.remote_promise,
                                "organ": draft.organ,
                            },
                            { "label": "dismiss" },
                        ]),
                    )
                    .await?,
                );
            }
        }
        Ok(created)
    }

    pub async fn senses_match(
        &self,
        rule: &MatchRule,
        cache: &[RemoteOpen],
    ) -> Result<Vec<Draft>, EngineError> {
        let watch_family = match &rule.watch_concept {
            Some(name) => match store::concepts::resolve(&self.store.pool, name).await? {
                Some(uid) => Some(
                    store::concepts::descendants_including(&self.store.pool, &uid)
                        .await?
                        .into_iter()
                        .collect::<std::collections::HashSet<_>>(),
                ),
                None => Some(std::collections::HashSet::new()),
            },
            None => None,
        };

        let mut drafts = Vec::new();
        for local in store::misc::list_promises(&self.store.pool).await? {
            if local.state != nucleus::PromiseState::Open || local.party_uid.is_none() {
                continue;
            }
            let Some(local_record) = &local.record_uid else {
                continue;
            };
            let local_concept = self.record_concept(local_record).await?;
            if let (Some(family), Some(c)) = (&watch_family, &local_concept) {
                if !family.contains(c) {
                    continue;
                }
            }
            let local_family = self.concept_family(local_concept.as_deref()).await?;

            for remote in cache {
                if remote.proximity > rule.max_proximity {
                    continue;
                }
                if remote.confidence < rule.min_confidence {
                    continue;
                }
                if local.delta == 0.0
                    || remote.delta == 0.0
                    || local.delta.signum() == remote.delta.signum()
                {
                    continue;
                }
                if !concepts_align(
                    &local_family,
                    local_concept.as_deref(),
                    remote.concept.as_deref(),
                ) {
                    continue;
                }
                if !units_compatible(local.record_uid.as_deref(), &remote) {}
                if !windows_overlap(&local.window_end, &remote.window_start, &remote.window_end) {
                    continue;
                }
                let score = score(&local, remote);
                drafts.push(Draft {
                    local_promise: local.uid.clone(),
                    remote_promise: remote.promise_uid.clone(),
                    organ: remote.organ.clone(),
                    score,
                });
            }
        }
        drafts.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(drafts)
    }

    async fn record_concept(&self, record_uid: &str) -> Result<Option<String>, EngineError> {
        Ok(store::records::get(&self.store.pool, record_uid)
            .await?
            .and_then(|r| r.identity_predicate_uid))
    }

    async fn concept_family(
        &self,
        concept: Option<&str>,
    ) -> Result<std::collections::HashSet<String>, EngineError> {
        Ok(match concept {
            Some(c) => store::concepts::descendants_including(&self.store.pool, c)
                .await?
                .into_iter()
                .collect(),
            None => std::collections::HashSet::new(),
        })
    }
}

fn concepts_align(
    local_family: &std::collections::HashSet<String>,
    local: Option<&str>,
    remote: Option<&str>,
) -> bool {
    match (local, remote) {
        (None, _) | (_, None) => true,
        (Some(l), Some(r)) => l == r || local_family.contains(r),
    }
}

fn units_compatible(_local_record: Option<&str>, _remote: &RemoteOpen) -> bool {
    true
}

fn windows_overlap(
    local_end: &Option<String>,
    remote_start: &Option<String>,
    _remote_end: &Option<String>,
) -> bool {
    match (local_end, remote_start) {
        (Some(le), Some(rs)) => rs.as_str() <= le.as_str(),
        _ => true,
    }
}

fn score(local: &store::misc::PromiseRow, remote: &RemoteOpen) -> f64 {
    let proximity_term = 1.0 / (1.0 + remote.proximity as f64);
    let fit = {
        let want = local.delta.abs();
        let have = remote.delta.abs();
        if want == 0.0 || have == 0.0 {
            0.0
        } else {
            (want.min(have)) / (want.max(have))
        }
    };
    0.5 * remote.confidence + 0.3 * proximity_term + 0.2 * fit
}
