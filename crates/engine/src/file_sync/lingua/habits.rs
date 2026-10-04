use std::num::NonZeroU32;

use chrono::{DateTime, Utc};
use nucleus::karma::{
    Cadence, CadenceAst, CadenceStep, CadenceStepAst, CivilDateTime, FoldPolicy,
    FrequencyCadenceAst, GapPolicy, LocalTimeResolution, MissedPolicy, PositiveIntegerBinding,
    Slug, TimestampMs,
};
use sha2::{Digest, Sha256};
use store::sqlx::Row;

use super::*;
use crate::karma_habits::{Definition, Imported, Input, Kind, Object, Preview, TUTORIAL};

fn refused(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_habit_import",
        message: message.to_string(),
    }
}

fn hash(value: &impl serde::Serialize) -> Result<String, EngineError> {
    nucleus::karma::canonical_hash("lince.karma-habit.v1", value)
        .map(|hash| hash.as_str().into())
        .map_err(refused)
}

fn uid(organ: &str, kind: &str) -> String {
    let digest = Sha256::digest(format!("lince.karma-habit.v1\0{organ}\0{TUTORIAL}\0{kind}"));
    format!(
        "{}_{}",
        if kind == "rule" { "rule" } else { "r" },
        nucleus::ulid_from(0, u128::from_be_bytes(digest[..16].try_into().unwrap()))
    )
}

fn objects(organ: &str) -> Vec<Object> {
    [
        (Kind::Record, "Cleaning Room", "cleaning-room"),
        (
            Kind::Frequency,
            "Cleaning Room daily",
            "cleaning-room.daily",
        ),
        (
            Kind::Rule,
            "Cleaning Room daily reset",
            "cleaning-room.daily-reset",
        ),
    ]
    .into_iter()
    .map(|(kind, name, slug)| Object {
        uid: uid(organ, kind.as_str()),
        kind,
        name: name.into(),
        slug: slug.into(),
        exists: false,
    })
    .collect()
}

fn time(input: &Input) -> Result<(u32, u32), EngineError> {
    let bytes = input.time.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || ![bytes[0], bytes[1], bytes[3], bytes[4]]
            .into_iter()
            .all(|byte| byte.is_ascii_digit())
    {
        return Err(refused("Use local time as HH:MM"));
    }
    let hour = u32::from(bytes[0] - b'0') * 10 + u32::from(bytes[1] - b'0');
    let minute = u32::from(bytes[3] - b'0') * 10 + u32::from(bytes[4] - b'0');
    if hour >= 24 || minute >= 60 {
        return Err(refused("Use a valid local time from 00:00 to 23:59"));
    }
    if input.fold == FoldPolicy::Both {
        return Err(refused(
            "A daily habit uses the first or second occurrence of a repeated local time",
        ));
    }
    Ok((hour, minute))
}

impl Engine {
    fn habit_definition(
        &self,
        organ: &str,
        input: Input,
        now: DateTime<Utc>,
    ) -> Result<Definition, EngineError> {
        let (hour, minute) = time(&input)?;
        let runtime = self
            .configured_karma_runtime()
            .or_else(|error| match error {
                EngineError::Conflict {
                    code: "karma_runtime_unconfigured",
                    ..
                } => crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                    "habit-import".into(),
                ),
                other => Err(other),
            })?;
        let mut candidates = Vec::new();
        for offset in -2..=3 {
            let date = now
                .date_naive()
                .checked_add_signed(chrono::Duration::days(offset))
                .ok_or_else(|| refused("The first habit date is out of range"))?;
            let civil = CivilDateTime::from_naive(date.and_hms_opt(hour, minute, 0).unwrap())
                .map_err(refused)?;
            for revision in runtime.provider_revisions() {
                let Some(provider) = runtime.provider(revision) else {
                    continue;
                };
                let Ok(resolved) = provider.resolve_local(&input.timezone, civil) else {
                    continue;
                };
                let (at, paused) = match resolved {
                    LocalTimeResolution::Unique { instant } => (instant, None),
                    LocalTimeResolution::Gap {
                        first_valid_after, ..
                    } if input.gap == GapPolicy::ShiftForward => (first_valid_after, None),
                    LocalTimeResolution::Gap {
                        first_valid_after, ..
                    } if input.gap == GapPolicy::Pause => (
                        first_valid_after,
                        Some(
                            "The first local time is in a clock gap. Choose shift forward or skip before importing",
                        ),
                    ),
                    LocalTimeResolution::Fold { first, .. } if input.fold == FoldPolicy::First => {
                        (first, None)
                    }
                    LocalTimeResolution::Fold { second, .. }
                        if input.fold == FoldPolicy::Second =>
                    {
                        (second, None)
                    }
                    LocalTimeResolution::Fold { first, second }
                        if input.fold == FoldPolicy::Pause =>
                    {
                        (
                            if first.as_millis() > now.timestamp_millis() {
                                first
                            } else {
                                second
                            },
                            Some(
                                "The first local time is repeated. Choose the first or second occurrence before importing",
                            ),
                        )
                    }
                    _ => continue,
                };
                if at.as_millis() > now.timestamp_millis() {
                    candidates.push((at, civil, revision.clone(), paused));
                }
            }
        }
        candidates.sort_by_key(|candidate| candidate.0);
        let (first, civil, revision, paused) = candidates.into_iter().next().ok_or_else(|| {
            refused("No installed timezone rules resolve an upcoming daily occurrence")
        })?;
        if let Some(message) = paused {
            return Err(refused(message));
        }
        let objects = objects(organ);
        let mut frequency = nucleus::karma::simple_frequency::frequency_from_cadence(
            Slug::new(&objects[1].slug).map_err(refused)?,
            objects[1].name.clone(),
            &Cadence::every(CadenceStep {
                days: 1,
                ..Default::default()
            }),
            TimestampMs::from_millis(first.as_millis()).map_err(refused)?,
        )
        .map_err(refused)?;
        frequency.cadence = FrequencyCadenceAst::Calendar {
            cadence: CadenceAst {
                every: CadenceStepAst {
                    days: Some(PositiveIntegerBinding::Literal {
                        value: NonZeroU32::new(1).unwrap(),
                    }),
                    ..Default::default()
                },
                land_on: None,
                invalid_day: Default::default(),
                bound: Default::default(),
            },
            anchor: civil,
            timezone: input.timezone.clone(),
            tzdb: revision,
            gap: input.gap,
            fold: input.fold,
        };
        frequency.missed = MissedPolicy::Coalesce;
        frequency.compile(&Default::default()).map_err(refused)?;
        let definition =
            serde_json::to_string(&serde_json::to_string(&frequency).map_err(EngineError::Json)?)
                .map_err(EngineError::Json)?;
        let source = format!(
            "Cleaning Room (@cleaning-room: 0) {{ {record}\nClean the room. Completing this task sets its quantity to zero.\n}} {record}\n\nFrequency cleaning-room.daily {{\n    title \"Cleaning Room daily\"\n    quantity 1\n    definition {definition}\n}} ^{frequency}\n\nKarma local {{\n    Rules {{\n        Rule cleaning-room.daily-reset {{\n            quantity 1\n            record @cleaning-room\n            condition \"\"\"-1 * freq(@cleaning-room.daily)\"\"\"\n            gate !=0\n            carry value\n            consequences \"\"\"[{{\"kind\":\"set-quantity\"}}]\"\"\"\n        }} ^{rule}\n    }}\n}}\n",
            record = objects[0].uid,
            frequency = objects[1].uid,
            rule = objects[2].uid
        );
        Ok(Definition {
            input,
            first_local: civil,
            first_at_ms: first.as_millis(),
            frequency,
            source,
            objects,
        })
    }

    async fn habit_organ(&self, actor: Option<&str>) -> Result<String, EngineError> {
        self.require_permission(actor, "record:read").await?;
        self.require_permission(actor, "frequency:read").await?;
        store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.uid)
            .ok_or_else(|| refused("A habit import requires a local Organ"))
    }

    async fn stored_habit(&self, organ: &str) -> Result<Option<(Definition, bool)>, EngineError> {
        let row = store::sqlx::query("SELECT definition, completed FROM karma_habit_import WHERE organ_uid = ? AND tutorial = ?")
            .bind(organ).bind(TUTORIAL).fetch_optional(&self.store.pool).await?;
        row.map(|row| {
            Ok((
                serde_json::from_str::<Definition>(&row.get::<String, _>("definition"))
                    .map_err(EngineError::Json)?,
                row.get::<i64, _>("completed") == 1,
            ))
        })
        .transpose()
    }

    async fn habit_preview_from(
        &self,
        organ: &str,
        definition: Definition,
        imported: bool,
        actor: Option<&str>,
    ) -> Result<Preview, EngineError> {
        let mut objects = definition.objects.clone();
        let mut conflicts = Vec::new();
        let mut quantity = None;
        for object in &mut objects {
            object.exists = match object.kind {
                Kind::Record => {
                    let record = store::records::get(&self.store.pool, &object.uid).await?;
                    if let Some(record) = record {
                        self.refuse_unreadable(actor, std::slice::from_ref(&record.uid))
                            .await?;
                        quantity = Some(record.quantity);
                        object.name = record.head;
                        object.slug = record.slug.unwrap_or_default();
                        true
                    } else {
                        false
                    }
                }
                Kind::Frequency => {
                    let handle =
                        store::karma::frequencies::get_handle(&self.store.pool, &object.uid)
                            .await?;
                    if let Some(handle) = handle {
                        self.refuse_unreadable(actor, std::slice::from_ref(&object.uid))
                            .await?;
                        object.slug = handle.slug;
                        true
                    } else {
                        false
                    }
                }
                Kind::Rule => {
                    let rule = store::recurrence::get(&self.store.pool, &object.uid).await?;
                    if let Some(rule) = rule {
                        self.refuse_unreadable(actor, &[rule.record_uid]).await?;
                        if let Some(identity) =
                            store::karma_fields::identity(&self.store.pool, &object.uid).await?
                        {
                            object.name = identity.name;
                            object.slug = identity.slug;
                        }
                        true
                    } else {
                        false
                    }
                }
            };
            if !object.exists {
                let created = store::sqlx::query_scalar::<_, i64>("SELECT created FROM karma_habit_object WHERE organ_uid = ? AND tutorial = ? AND uid = ?")
                    .bind(organ).bind(TUTORIAL).bind(&object.uid).fetch_optional(&self.store.pool).await?.unwrap_or(0);
                if created == 1 {
                    conflicts.push(format!(
                        "The imported {} was deleted; reimport preserves that decision",
                        object.kind.as_str()
                    ));
                }
                let occupied = match object.kind {
                    Kind::Rule => {
                        store::sqlx::query_scalar::<_, String>(
                            "SELECT uid FROM recurrence WHERE slug = ?",
                        )
                        .bind(&object.slug)
                        .fetch_optional(&self.store.pool)
                        .await?
                    }
                    _ => store::records::resolve(&self.store.pool, &object.slug)
                        .await?
                        .map(|record| record.uid),
                };
                if occupied.is_some() {
                    conflicts.push(format!("@{} is already in use", object.slug));
                }
            }
        }
        let mut preview = Preview {
            fingerprint: String::new(),
            input: definition.input,
            first_local: definition.first_local,
            first_at_ms: definition.first_at_ms,
            frequency: definition.frequency,
            objects,
            imported,
            current_quantity: quantity,
            conflicts,
        };
        preview.fingerprint = hash(&preview)?;
        Ok(preview)
    }

    pub(crate) async fn preview_karma_habit(
        &self,
        input: Input,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Preview, EngineError> {
        time(&input)?;
        let organ = self.habit_organ(actor).await?;
        let (definition, imported) = self.stored_habit(&organ).await?.map_or_else(
            || {
                self.habit_definition(&organ, input, now)
                    .map(|definition| (definition, false))
            },
            Ok,
        )?;
        self.read_lingua_document(&definition.source, None).await?;
        self.habit_preview_from(&organ, definition, imported, actor)
            .await
    }

    pub(crate) async fn import_karma_habit(
        &self,
        input: Input,
        expected: String,
        request: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Imported, EngineError> {
        if request.is_empty() || request.len() > 200 || request.chars().any(char::is_control) {
            return Err(refused(
                "Use a request ID of 1–200 bytes without control characters",
            ));
        }
        time(&input)?;
        let organ = self.habit_organ(actor).await?;
        let stored = self.stored_habit(&organ).await?;
        let (definition, completed) = stored.clone().map_or_else(
            || {
                self.habit_definition(&organ, input.clone(), now)
                    .map(|definition| (definition, false))
            },
            Ok,
        )?;
        let preview = self
            .habit_preview_from(&organ, definition.clone(), completed, actor)
            .await?;
        if !preview.conflicts.is_empty() {
            return Err(refused(preview.conflicts.join("\n")));
        }
        let fingerprint = hash(&(input, &expected))?;
        let receipt = store::sqlx::query("SELECT fingerprint, result FROM karma_habit_request WHERE organ_uid = ? AND request_id = ?")
            .bind(&organ).bind(&request).fetch_optional(&self.store.pool).await?;
        let pending = if let Some(receipt) = receipt {
            if receipt.get::<String, _>("fingerprint") != fingerprint {
                return Err(refused(
                    "This import request ID was used for different contents",
                ));
            }
            if let Some(result) = receipt.get::<Option<String>, _>("result") {
                return serde_json::from_str(&result).map_err(EngineError::Json);
            }
            true
        } else {
            false
        };
        if !pending && preview.fingerprint != expected {
            return Err(refused(
                "The import preview changed. Preview again before importing",
            ));
        }
        for permission in [
            "record:create",
            "record:update",
            "frequency:create",
            "frequency:update",
            "karma:update",
        ] {
            self.require_permission(actor, permission).await?;
        }
        self.configured_karma_runtime()?;
        let mut document = self.read_lingua_document(&definition.source, None).await?;
        if document.records.len() != 1
            || document.automation.frequencies.len() != 1
            || document.automation.rules.len() != 1
        {
            return Err(refused(
                "The habit declaration must contain its Record, Frequency and Rule",
            ));
        }
        if document.records[0].uid != definition.objects[0].uid
            || document.automation.frequencies[0].uid != definition.objects[1].uid
            || document.automation.rules[0].uid != definition.objects[2].uid
        {
            return Err(refused("The habit declaration identities changed"));
        }
        document.automation.rules[0].record_uid = Some(definition.objects[0].uid.clone());
        let mut tx = store::write_tx(&self.store.pool).await?;
        if stored.is_none() {
            store::sqlx::query(
                "INSERT INTO karma_habit_import(organ_uid, tutorial, definition) VALUES (?, ?, ?)",
            )
            .bind(&organ)
            .bind(TUTORIAL)
            .bind(serde_json::to_string(&definition).map_err(EngineError::Json)?)
            .execute(&mut *tx)
            .await?;
            for object in &preview.objects {
                store::sqlx::query("INSERT INTO karma_habit_object(organ_uid, tutorial, kind, uid, created) VALUES (?, ?, ?, ?, ?)")
                    .bind(&organ).bind(TUTORIAL).bind(object.kind.as_str()).bind(&object.uid).bind(i64::from(object.exists)).execute(&mut *tx).await?;
            }
        }
        if !pending {
            store::sqlx::query("INSERT INTO karma_habit_request(organ_uid, tutorial, request_id, fingerprint) VALUES (?, ?, ?, ?)")
                .bind(&organ).bind(TUTORIAL).bind(&request).bind(&fingerprint).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        if !preview.objects[0].exists {
            let record = &document.records[0];
            self.act(
                Action::CreateRecordDraft {
                    draft: crate::record_creation::Draft {
                        uid: record.uid.clone(),
                        head: record.head.clone(),
                        body: record.body.clone(),
                        slug: record.slug.clone(),
                        quantity: record.quantity.as_ref().unwrap().0.clone(),
                        ..Default::default()
                    },
                },
                actor.map(str::to_owned),
            )
            .await?;
        }
        let mut report = FileSyncReport::default();
        if !preview.objects[1].exists {
            let mut frequencies = document.automation.clone();
            frequencies.rules.clear();
            frequencies.frequencies[0].quantity = 0;
            Box::pin(self.apply_lingua_frequencies_as(&frequencies, &mut report, actor)).await?;
        }
        let frequency =
            store::karma::frequencies::get_handle(&self.store.pool, &definition.objects[1].uid)
                .await?
                .ok_or_else(|| refused("The imported Frequency is unavailable"))?;
        if !preview.objects[2].exists {
            let mut rules = document.automation.clone();
            rules.frequencies.clear();
            rules.rules[0].condition = Some(format!("-1 * freq(@{})", frequency.slug));
            Box::pin(self.apply_lingua_rules_as(&rules, &mut report, actor)).await?;
        }
        let rule = store::recurrence::get(&self.store.pool, &definition.objects[2].uid)
            .await?
            .ok_or_else(|| refused("The imported Rule is unavailable"))?;
        let identity = store::karma_fields::identity(&self.store.pool, &rule.uid)
            .await?
            .ok_or_else(|| refused("The imported Rule identity is unavailable"))?;
        if !completed
            && (stored.is_some() || !preview.objects[2].exists)
            && rule.revision == 1
            && identity.name.is_empty()
        {
            let fields = store::karma_fields::for_rule(&self.store.pool, &rule.uid).await?;
            let fields = nucleus::karma::rule_field::RuleFieldKind::ALL.map(|kind| {
                let field = fields.iter().find(|field| field.kind == kind).unwrap();
                nucleus::karma::rule_field::RuleFieldInput::Reference {
                    uid: field.uid.clone(),
                    revision: field.revision,
                }
            });
            self.act(
                Action::SaveKarmaRule {
                    identity: Some(nucleus::karma::rule_field::RuleIdentity {
                        name: definition.objects[2].name.clone(),
                        slug: definition.objects[2].slug.clone(),
                    }),
                    rule: Some(rule.uid),
                    expected_revision: Some(rule.revision),
                    fields,
                    request_id: format!("habit-name:{}", definition.objects[2].uid),
                },
                actor.map(str::to_owned),
            )
            .await?;
        }
        if !completed
            && (stored.is_some() || !preview.objects[1].exists)
            && frequency.handle_revision == 1
            && frequency.active_activation_hash.is_none()
        {
            self.act(
                Action::ActivateKarmaFrequency {
                    request_id: format!("habit-activate:{}", frequency.record_uid),
                    frequency_uid: frequency.record_uid,
                    expected_handle_revision: frequency.handle_revision,
                    revision_hash: frequency.head_revision_hash,
                    parameter_overrides: Default::default(),
                },
                actor.map(str::to_owned),
            )
            .await?;
        }
        let preview = self
            .habit_preview_from(&organ, definition, true, actor)
            .await?;
        if preview.objects.iter().any(|object| !object.exists) || !preview.conflicts.is_empty() {
            return Err(refused(
                "The import is incomplete. Review its objects before retrying",
            ));
        }
        let imported = Imported {
            tutorial: TUTORIAL.into(),
            objects: preview.objects,
            quantity_at_import: preview
                .current_quantity
                .ok_or_else(|| refused("The imported task is unavailable"))?,
        };
        let mut tx = store::write_tx(&self.store.pool).await?;
        store::sqlx::query(
            "UPDATE karma_habit_import SET completed = 1 WHERE organ_uid = ? AND tutorial = ?",
        )
        .bind(&organ)
        .bind(TUTORIAL)
        .execute(&mut *tx)
        .await?;
        store::sqlx::query(
            "UPDATE karma_habit_request SET result = ? WHERE organ_uid = ? AND request_id = ?",
        )
        .bind(serde_json::to_string(&imported).map_err(EngineError::Json)?)
        .bind(&organ)
        .bind(&request)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.notify_karma_deadline_change();
        Ok(imported)
    }
}
