use super::*;
use anicca::{ProjectedFrequency, ProjectedRule};
use chrono::{DateTime, Utc};
use nucleus::karma::{Cadence, CadenceStep, Carry, Consequences, FrequencyAst, Gate};
use store::karma::frequencies;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Automation {
    pub frequencies: Vec<ProjectedFrequency>,
    pub rules: Vec<ProjectedRule>,
}

impl Automation {
    pub fn uids(&self) -> impl Iterator<Item = &String> {
        self.frequencies
            .iter()
            .map(|v| &v.uid)
            .chain(self.rules.iter().map(|v| &v.uid))
    }

    pub fn normalize(&mut self) -> Result<(), EngineError> {
        for frequency in &mut self.frequencies {
            let definition = frequency_definition(frequency)?;
            definition.compile(&Default::default()).map_err(invalid)?;
            *frequency = project_frequency(&frequency.uid, frequency.quantity, &definition)?;
        }
        for rule in &mut self.rules {
            switch(rule.quantity)?;
            let consequences: Consequences =
                serde_json::from_str(&rule.consequences_json).map_err(invalid)?;
            rule.consequences_json = serde_json::to_string(&consequences).map_err(invalid)?;
            rule_cadence(rule)?.validate().map_err(invalid)?;
            rule_condition(rule)?;
            if let Some(anchor) = &mut rule.anchor_at {
                *anchor = DateTime::from_timestamp_millis(instant(anchor)?.timestamp_millis())
                    .ok_or_else(|| invalid("Invalid Rule anchor"))?
                    .to_rfc3339();
            }
        }
        self.frequencies.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.rules.sort_by(|a, b| a.uid.cmp(&b.uid));
        Ok(())
    }

    pub fn render(&self) -> Result<String, EngineError> {
        let mut text = String::new();
        for frequency in &self.frequencies {
            text.push_str(&format!(
                "Frequency {} {{\n    title {}\n    quantity {}\n",
                frequency.slug,
                quote(&frequency.head),
                frequency.quantity
            ));
            if let Some(definition) = &frequency.definition {
                text.push_str(&format!("    definition {}\n", quote(definition)));
            } else {
                text.push_str(&format!(
                    "    every {} {}\n    timezone {}\n    next_at {}\n",
                    frequency.every.0,
                    frequency.every.1,
                    quote(&frequency.timezone),
                    frequency.next_at
                ));
            }
            text.push_str(&format!("}} ^{}\n\n", frequency.uid));
        }
        if !self.rules.is_empty() {
            text.push_str("Karma local {\n    Rules {\n");
            for rule in &self.rules {
                text.push_str(&format!(
                    "        Rule {} {{\n            quantity {}\n            record @{}\n",
                    rule.slug, rule.quantity, rule.record_slug
                ));
                if !rule.frequency_slug.is_empty() {
                    text.push_str(&format!("            frequency @{}\n", rule.frequency_slug));
                }
                if let Some(source) = &rule.condition {
                    text.push_str(&format!(
                        "            condition {}\n            gate {}\n            carry {}\n",
                        block(source)?,
                        rule.gate.as_deref().unwrap_or("!=0"),
                        rule.carry.as_deref().unwrap_or("value")
                    ));
                }
                text.push_str(&format!(
                    "            consequences {}\n",
                    block(&rule.consequences_json)?
                ));
                if let Some(cadence) = &rule.cadence {
                    text.push_str(&format!("            cadence {}\n", quote(cadence)));
                }
                if let Some(anchor) = &rule.anchor_at {
                    text.push_str(&format!("            anchor_at {anchor}\n"));
                }
                if let Some(note) = &rule.note {
                    text.push_str(&format!("            note {}\n", quote(note)));
                }
                text.push_str(&format!("        }} ^{}\n", rule.uid));
            }
            text.push_str("    }\n}\n");
        }
        Ok(text)
    }
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).expect("string")
}

fn block(value: &str) -> Result<String, EngineError> {
    if value.contains("\"\"\"") {
        return Err(invalid(
            "A Karma field contains a triple quote that Lingua cannot represent",
        ));
    }
    Ok(format!("\"\"\"{value}\"\"\""))
}

fn switch(value: i64) -> Result<(), EngineError> {
    if matches!(value, 0 | 1) {
        Ok(())
    } else {
        Err(invalid("Frequency and Rule quantities must be 0 or 1"))
    }
}

fn instant(value: &str) -> Result<DateTime<Utc>, EngineError> {
    DateTime::parse_from_rfc3339(value)
        .map(|v| v.with_timezone(&Utc))
        .map_err(invalid)
}

fn frequency_definition(value: &ProjectedFrequency) -> Result<FrequencyAst, EngineError> {
    switch(value.quantity)?;
    if let Some(source) = &value.definition {
        let mut definition: FrequencyAst = serde_json::from_str(source).map_err(invalid)?;
        definition.slug = nucleus::karma::Slug::new(&value.slug).map_err(invalid)?;
        definition.purpose = value.head.clone();
        return Ok(definition);
    }
    if !matches!(value.timezone.as_str(), "UTC" | "Etc/UTC") {
        return Err(invalid(
            "Use a Frequency definition for a calendar with a pinned timezone",
        ));
    }
    let mut step = CadenceStep::default();
    let count = value.every.0;
    match value.every.1.trim_end_matches('s') {
        "year" => step.years = count,
        "month" => step.months = count,
        "week" => step.weeks = count,
        "day" => step.days = count,
        "hour" => step.hours = count,
        "minute" => step.minutes = count,
        "second" => step.seconds = count,
        "millisecond" => step.milliseconds = count,
        _ => return Err(invalid("Unknown Frequency interval unit")),
    }
    let cadence = Cadence::every(step);
    cadence.validate().map_err(invalid)?;
    nucleus::karma::simple_frequency::frequency_from_cadence(
        nucleus::karma::Slug::new(&value.slug).map_err(invalid)?,
        value.head.clone(),
        &cadence,
        nucleus::karma::TimestampMs::from_millis(instant(&value.next_at)?.timestamp_millis())
            .map_err(invalid)?,
    )
    .map_err(invalid)
}

fn project_frequency(
    uid: &str,
    quantity: i64,
    definition: &FrequencyAst,
) -> Result<ProjectedFrequency, EngineError> {
    let mut value = ProjectedFrequency {
        uid: uid.into(),
        slug: definition.slug.as_str().into(),
        head: definition.purpose.clone(),
        quantity,
        every: (0, String::new()),
        timezone: String::new(),
        next_at: String::new(),
        definition: Some(serde_json::to_string(definition).map_err(invalid)?),
    };
    if let nucleus::karma::FrequencyCadenceAst::Elapsed {
        interval: nucleus::karma::DurationBinding::Literal { value: interval },
        anchor,
    } = &definition.cadence
    {
        let milliseconds = interval.get();
        for (unit, divisor) in [
            ("day", 86_400_000),
            ("hour", 3_600_000),
            ("minute", 60_000),
            ("second", 1000),
            ("millisecond", 1),
        ] {
            if milliseconds % divisor == 0
                && let Ok(count) = u32::try_from(milliseconds / divisor)
            {
                let mut simple = value.clone();
                simple.definition = None;
                simple.every = (count, unit.into());
                simple.timezone = "UTC".into();
                simple.next_at = DateTime::from_timestamp_millis(anchor.as_millis())
                    .ok_or_else(|| invalid("Invalid Frequency anchor"))?
                    .to_rfc3339();
                if frequency_definition(&simple)? == *definition {
                    value = simple;
                }
                break;
            }
        }
    }
    Ok(value)
}

fn rule_cadence(rule: &ProjectedRule) -> Result<Cadence, EngineError> {
    rule.cadence
        .as_ref()
        .map(|value| serde_json::from_str(value).map_err(invalid))
        .unwrap_or_else(|| Ok(Cadence::once()))
}

fn rule_condition(
    rule: &ProjectedRule,
) -> Result<Option<store::recurrence::RuleCondition>, EngineError> {
    let Some(source) = &rule.condition else {
        if rule.gate.is_some() || rule.carry.is_some() {
            return Err(invalid("A gate or carry needs a condition"));
        }
        return Ok(None);
    };
    nucleus::karma::Condition::parse(source).map_err(invalid)?;
    Ok(Some(store::recurrence::RuleCondition {
        source: source.clone(),
        gate: Gate::parse(rule.gate.as_deref().unwrap_or("!=0")).map_err(invalid)?,
        carry: Carry::parse(rule.carry.as_deref().unwrap_or("value")).map_err(invalid)?,
    }))
}

impl Engine {
    pub(super) async fn current_lingua_automation(
        &self,
        document: &Document,
    ) -> Result<Automation, EngineError> {
        let mut automation = Automation::default();
        let ids: HashSet<_> = document
            .records
            .iter()
            .map(|r| &r.uid)
            .chain(document.automation.frequencies.iter().map(|f| &f.uid))
            .collect();
        for uid in ids {
            if let Some(handle) = frequencies::get_handle(&self.store.pool, uid).await? {
                if store::records::get(&self.store.pool, uid).await?.is_none() {
                    continue;
                }
                let revision =
                    frequencies::get_revision(&self.store.pool, &handle.head_revision_hash)
                        .await?
                        .ok_or_else(|| invalid("Frequency revision missing"))?;
                if handle
                    .active_revision_hash
                    .as_ref()
                    .is_some_and(|hash| hash != &handle.head_revision_hash)
                {
                    return Err(invalid(
                        "Save the running Frequency revision before syncing its definition",
                    ));
                }
                if let Some(hash) = &handle.active_activation_hash {
                    let activation = frequencies::get_activation(&self.store.pool, hash)
                        .await?
                        .ok_or_else(|| invalid("Frequency activation missing"))?;
                    if activation.epoch.effective_parameters()
                        != &revision
                            .frequency
                            .compile(&Default::default())
                            .map_err(invalid)?
                            .effective_parameters
                    {
                        return Err(invalid(
                            "Frequency parameter overrides cannot be exported yet",
                        ));
                    }
                }
                let active = handle.active_activation_hash.is_some();
                automation.frequencies.push(project_frequency(
                    uid,
                    i64::from(active),
                    &revision.frequency,
                )?);
            }
        }
        let mut rules = HashMap::new();
        for record in &document.records {
            for rule in store::recurrence::for_record(&self.store.pool, &record.uid).await? {
                rules.insert(rule.uid.clone(), rule);
            }
        }
        for rule in &document.automation.rules {
            if let Some(current) = store::recurrence::get(&self.store.pool, &rule.uid).await? {
                rules.insert(current.uid.clone(), current);
            }
        }
        for rule in rules.into_values() {
            let Some(target) = store::records::get(&self.store.pool, &rule.record_uid).await?
            else {
                continue;
            };
            let slug = document
                .automation
                .rules
                .iter()
                .find(|r| r.uid == rule.uid)
                .map(|r| r.slug.clone())
                .unwrap_or_else(|| {
                    format!(
                        "rule-{}",
                        rule.uid.split_once('_').unwrap().1.to_ascii_lowercase()
                    )
                });
            automation.rules.push(ProjectedRule {
                uid: rule.uid.clone(),
                slug,
                quantity: i64::from(!rule.is_paused()),
                frequency_slug: String::new(),
                frequency_uid: None,
                record_slug: target.slug.unwrap_or(target.uid),
                record_uid: Some(rule.record_uid),
                condition: rule.condition.as_ref().map(|c| c.source.clone()),
                gate: rule.condition.as_ref().map(|c| c.gate.as_text()),
                carry: rule.condition.as_ref().map(|c| c.carry.as_text()),
                consequences_json: serde_json::to_string(&rule.consequences).map_err(invalid)?,
                cadence: Some(serde_json::to_string(&rule.cadence).map_err(invalid)?),
                anchor_at: Some(rule.anchor_at),
                note: rule.note,
            });
        }
        automation.normalize()?;
        Ok(automation)
    }

    pub(super) async fn apply_lingua_frequencies(
        &self,
        automation: &Automation,
        report: &mut FileSyncReport,
    ) -> Result<(), EngineError> {
        for frequency in &automation.frequencies {
            let definition = frequency_definition(frequency)?;
            let handle = frequencies::get_handle(&self.store.pool, &frequency.uid).await?;
            let before = handle.clone();
            let now = Utc::now();
            if let Some(handle) = handle {
                let revision =
                    frequencies::get_revision(&self.store.pool, &handle.head_revision_hash)
                        .await?
                        .ok_or_else(|| invalid("Frequency revision missing"))?;
                if revision.frequency != definition {
                    let action = if frequency.quantity == 0 {
                        Action::ReviseKarmaFrequency {
                            request_id: nucleus::new_uid("lingua"),
                            frequency_uid: frequency.uid.clone(),
                            expected_handle_revision: handle.handle_revision,
                            frequency: definition,
                        }
                    } else {
                        Action::SaveKarmaFrequency {
                            request_id: nucleus::new_uid("lingua"),
                            frequency_uid: Some(frequency.uid.clone()),
                            expected_handle_revision: Some(handle.handle_revision),
                            frequency: definition,
                            restart: true,
                        }
                    };
                    self.act(action, None).await?;
                }
            } else {
                let input = frequencies::CreateFrequencyInput {
                    request_id: format!("lingua-import:{}", frequency.uid),
                    frequency: definition.clone(),
                    owner_person_uid: None,
                    actor_person_uid: None,
                };
                let signer = self.signer.lock().await.clone();
                let commit = frequencies::create_identified(
                    &self.store.pool,
                    input,
                    Some(&frequency.uid),
                    None,
                    now,
                    |hash| signer.as_ref().map(|s| s.sign_hash(hash)),
                )
                .await?;
                self.finish_frequency_mutation(commit, now, true).await?;
            }
            let handle = frequencies::get_handle(&self.store.pool, &frequency.uid)
                .await?
                .ok_or_else(|| invalid("Frequency missing"))?;
            if frequency.quantity == 0 && handle.active_activation_hash.is_some() {
                self.act(
                    Action::PauseKarmaFrequency {
                        request_id: nucleus::new_uid("lingua"),
                        frequency_uid: frequency.uid.clone(),
                        expected_handle_revision: handle.handle_revision,
                    },
                    None,
                )
                .await?;
            } else if frequency.quantity == 1 && handle.active_activation_hash.is_none() {
                self.act(
                    Action::ActivateKarmaFrequency {
                        request_id: nucleus::new_uid("lingua"),
                        frequency_uid: frequency.uid.clone(),
                        expected_handle_revision: handle.handle_revision,
                        revision_hash: handle.head_revision_hash,
                        parameter_overrides: Default::default(),
                    },
                    None,
                )
                .await?;
            }
            let after = frequencies::get_handle(&self.store.pool, &frequency.uid).await?;
            if before.is_none() {
                report.created.push(frequency.uid.clone());
            } else if before != after {
                report.updated_from_disk.push(frequency.uid.clone());
            }
        }
        Ok(())
    }

    pub(super) async fn apply_lingua_rules(
        &self,
        automation: &Automation,
        report: &mut FileSyncReport,
    ) -> Result<(), EngineError> {
        for rule in &automation.rules {
            let target = rule
                .record_uid
                .as_ref()
                .ok_or_else(|| invalid("Rule target is unresolved"))?;
            self.reject_direct_transfer_record_mutation(target).await?;
            let mut cadence = rule_cadence(rule)?;
            let mut anchor = rule
                .anchor_at
                .as_deref()
                .map(instant)
                .transpose()?
                .unwrap_or_else(Utc::now);
            if !rule.frequency_slug.is_empty() {
                let frequency = store::frequency::resolve(&self.store.pool, &rule.frequency_slug)
                    .await?
                    .ok_or_else(|| invalid("Unknown Rule frequency"))?;
                cadence = frequency.cadence();
                anchor = frequency.anchor()?;
            }
            let consequences: Consequences =
                serde_json::from_str(&rule.consequences_json).map_err(invalid)?;
            let consequences = self
                .resolve_consequences(consequences.as_slice().to_vec())
                .await?;
            let mut condition = rule_condition(rule)?;
            if let Some(condition) = &mut condition {
                condition.source = self
                    .canonical_condition(Some(condition.source.clone()))
                    .await?
                    .ok_or_else(|| invalid("Empty condition"))?;
            }
            Box::pin(self.validate_automatic_rule(&consequences, condition.as_ref(), None)).await?;
            let current = store::recurrence::get(&self.store.pool, &rule.uid).await?;
            let before = current.clone();
            if let Some(current) = current {
                if current.record_uid != *target {
                    return Err(invalid("A Rule cannot change its target during file sync"));
                }
                if current.consequences != consequences
                    || current.condition != condition
                    || current.cadence != cadence
                    || instant(&current.anchor_at)?.timestamp_millis() != anchor.timestamp_millis()
                    || current.note != rule.note
                {
                    self.act(
                        Action::ReviseRecurrence {
                            recurrence: rule.uid.clone(),
                            expected_revision: current.revision,
                            request_id: nucleus::new_uid("lingua"),
                            consequences: consequences.as_slice().to_vec(),
                            condition: condition.as_ref().map(|c| c.source.clone()),
                            gate: condition.as_ref().map(|c| c.gate.as_text()),
                            carry: condition.as_ref().map(|c| c.carry.as_text()),
                            note: rule.note.clone(),
                            cadence,
                            anchor_at: Some(anchor.to_rfc3339()),
                        },
                        None,
                    )
                    .await?;
                }
            } else {
                let _guard = self.rule_execution.lock().await;
                store::recurrence::create_identified(
                    &self.store.pool,
                    store::recurrence::NewRecurrence {
                        record_uid: target,
                        consequences,
                        condition,
                        note: rule.note.as_deref(),
                        cadence,
                        anchor_at: anchor,
                        request_id: &format!("lingua-import:{}", rule.uid),
                        actor_uid: None,
                    },
                    Some(&rule.uid),
                    Utc::now(),
                )
                .await?;
            }
            self.notify_karma_deadline_change();
            self.query_changed
                .send_modify(|revision| *revision = revision.wrapping_add(1));
            let current = store::recurrence::get(&self.store.pool, &rule.uid)
                .await?
                .ok_or_else(|| invalid("Rule missing"))?;
            if current.is_paused() != (rule.quantity == 0) {
                self.act(
                    Action::SetRecurrencePaused {
                        recurrence: rule.uid.clone(),
                        expected_revision: current.revision,
                        request_id: nucleus::new_uid("lingua"),
                        paused: rule.quantity == 0,
                    },
                    None,
                )
                .await?;
            }
            let after = store::recurrence::get(&self.store.pool, &rule.uid).await?;
            if before.is_none() {
                report.created.push(rule.uid.clone());
            } else if before != after {
                report.updated_from_disk.push(rule.uid.clone());
            }
        }
        Ok(())
    }
}
