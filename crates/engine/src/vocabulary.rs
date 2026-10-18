use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::vocabulary::{Command, SAVED_LOCATION_ROLE_UID};
use serde_json::json;
mod inspector;

impl Engine {
    pub(crate) async fn vocabulary_command(
        &self,
        command: Command,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        use store::sqlx::Row;
        if !command.is_mutation() {
            self.require_permission(actor, "record:read").await?;
            if let Command::BindQuery { query } = &command {
                if query.len() > 131072 {
                    return Err(EngineError::Consequence(
                        "Use a query of at most 128 KiB".into(),
                    ));
                }
                let query: protein::Protein = serde_json::from_str(query)?;
                let bound = protein::bind_meanings(&self.store, &query, actor).await?;
                return Ok(ActionOutcome {
                    data: Some(json!({"query":bound})),
                    ..Default::default()
                });
            }
            let mut tx = store::write_tx(&self.store.pool).await?;
            self.require_permission_on(&mut tx, actor, "record:read")
                .await?;
            if matches!(
                command,
                Command::InspectFeature { .. } | Command::PreviewFeature { .. }
            ) {
                self.require_permission_on(&mut tx, actor, "permission:assign")
                    .await?;
            }
            let data = match command {
                Command::ExportTranslations { vocabulary } => {
                    self.require_permission_on(&mut tx, actor, "permission:assign")
                        .await?;
                    json!({"translations":store::vocabulary::export_translations_on(&mut tx,&vocabulary).await?})
                }
                Command::PreviewTranslations { package } => {
                    let package = store::vocabulary::read_translations(&package)?;
                    json!({"translations_preview":store::vocabulary::preview_translations_on(&mut tx,&package).await?})
                }
                Command::InspectMeaning { concept, offset } => {
                    json!({"meaning":self.inspect_meaning_on(&mut tx,&concept,actor,offset).await?})
                }
                Command::InspectMeasurement { concept, unit } => {
                    let meaning =
                        store::vocabulary::current_meaning_reference_on(&mut tx, &concept).await?;
                    let unit = if let Some(unit) = unit {
                        Some(store::vocabulary::current_meaning_reference_on(&mut tx, &unit).await?)
                    } else {
                        None
                    };
                    json!({"measurement":nucleus::transfer::receipt::Measurement { meaning, unit, object_commitment: None }})
                }
                Command::InspectFeature { feature } => {
                    json!({"feature_binding":store::vocabulary::feature_binding_on(&mut tx,&feature).await?})
                }
                Command::PreviewFeature {
                    feature,
                    concept,
                    enabled,
                    write,
                } => {
                    json!({"feature_preview":store::vocabulary::feature_preview_on(&mut tx,&feature,&concept,enabled,write).await?})
                }
                Command::InspectTerminology => {
                    json!({"terminology":store::vocabulary::terminology_state_on(&mut tx, actor).await?})
                }
                Command::InspectPublication { publication } => {
                    json!({"publication":store::vocabulary::publication_on(&mut tx, &publication, false).await?})
                }
                Command::PreviewPackage { package } => {
                    json!({"publication":store::vocabulary::read_package(&package)?})
                }
                Command::RecordHistory { record, offset } => {
                    drop(tx);
                    let record = self.resolve(&record).await?;
                    self.refuse_unreadable(actor, std::slice::from_ref(&record))
                        .await?;
                    let mut tx = store::write_tx(&self.store.pool).await?;
                    self.require_permission_on(&mut tx, actor, "record:read")
                        .await?;
                    let graph =
                        crate::access::policy_graph_on(&mut tx, std::slice::from_ref(&record))
                            .await
                            .map_err(|error| EngineError::Forbidden(error.to_string()))?;
                    if !self
                        .readable_on(&mut tx, actor, &graph)
                        .await?
                        .contains(&record)
                    {
                        return Err(EngineError::Forbidden("Record history unavailable".into()));
                    }
                    return Ok(ActionOutcome {
                        data: Some(
                            json!({"history":store::vocabulary::record_history_on(&mut tx,&record,offset).await?}),
                        ),
                        ..Default::default()
                    });
                }
                _ => unreachable!(),
            };
            return Ok(ActionOutcome {
                data: Some(data),
                ..Default::default()
            });
        }
        let preferences = matches!(
            command,
            Command::SetTerminologyContext { .. }
                | Command::PreferTerm { .. }
                | Command::UseWording { .. }
                | Command::HideTerm { .. }
                | Command::RestoreOriginal { .. }
                | Command::ResetTerminology { .. }
        );
        let personal = match &command {
            _ if preferences => true,
            Command::SetLanguage { .. } => actor.is_some(),
            Command::AddTerm { personal, .. } => *personal,
            Command::EditTerm { term, .. } | Command::RetireTerm { term, .. } => {
                let term = store::vocabulary::get_term(&self.store.pool, term)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("Term unavailable".into()))?;
                if term.owner_uid.is_some() && term.owner_uid.as_deref() != actor {
                    return Err(EngineError::Forbidden(
                        "This personal term belongs to another Person".into(),
                    ));
                }
                term.owner_uid.is_some()
            }
            _ => false,
        };
        let language_changed = matches!(command, Command::SetLanguage { .. });
        if personal && actor.is_none() && !preferences {
            return Err(EngineError::Consequence(
                "Sign in as a Person to create personal terms".into(),
            ));
        }
        if !personal {
            self.require_permission(actor, "permission:assign").await?;
            self.require_permission(
                actor,
                if matches!(
                    command,
                    Command::CreateConcept { .. }
                        | Command::CreateVocabulary { .. }
                        | Command::Adopt { .. }
                        | Command::AdoptPackage { .. }
                ) {
                    "record:create"
                } else {
                    "record:update"
                },
            )
            .await?;
        }
        let serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        if !personal {
            self.require_permission_on(&mut tx, actor, "permission:assign")
                .await?;
            self.require_permission_on(
                &mut tx,
                actor,
                if matches!(
                    command,
                    Command::CreateConcept { .. }
                        | Command::CreateVocabulary { .. }
                        | Command::Adopt { .. }
                        | Command::AdoptPackage { .. }
                ) {
                    "record:create"
                } else {
                    "record:update"
                },
            )
            .await?;
        }
        let mut created = None;
        let mut changed_concept = None;
        let mut data = None;
        if matches!(
            command,
            Command::CreateVocabulary { .. }
                | Command::Publish { .. }
                | Command::Withdraw { .. }
                | Command::Adopt { .. }
                | Command::AdoptPackage { .. }
        ) {
            self.require_permission_on(&mut tx, actor, "organ:update")
                .await?;
        }
        if !personal {
            let concept = match &command {
                Command::ReviseDefinition { concept, .. } | Command::AddTerm { concept, .. } => {
                    Some(concept.clone())
                }
                Command::EditTerm { term, .. } | Command::RetireTerm { term, .. } => {
                    store::sqlx::query_scalar("SELECT concept_uid FROM concept_term WHERE uid=?")
                        .bind(term)
                        .fetch_optional(&mut *tx)
                        .await?
                }
                _ => None,
            };
            if let Some(concept) = concept {
                store::vocabulary::require_local_concept_on(&mut tx, &concept).await?;
            }
        }
        match command {
            Command::ImportTranslations { package } => {
                let package = store::vocabulary::read_translations(&package)?;
                let preview = store::vocabulary::import_translations_on(&mut tx, &package).await?;
                for entry in preview["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|entry| entry["new_terms"].as_u64().unwrap_or(0) > 0)
                {
                    store::vocabulary::log_shared_on(
                        &mut tx,
                        entry["concept_uid"].as_str().unwrap(),
                    )
                    .await?;
                }
                data = Some(json!({"translations_preview":preview}));
            }
            Command::ExportTranslations { .. } | Command::PreviewTranslations { .. } => {
                unreachable!()
            }
            Command::BindFeature {
                feature,
                concept,
                enabled,
                write,
                expected_revision,
            } => {
                let preview = store::vocabulary::feature_preview_on(
                    &mut tx, &feature, &concept, enabled, write,
                )
                .await?;
                let binding: nucleus::vocabulary::FeatureBinding =
                    serde_json::from_value(preview["after"].clone())?;
                store::vocabulary::bind_feature_on(&mut tx, &binding, expected_revision).await?;
                let binding = store::vocabulary::feature_binding_on(&mut tx, &feature).await?;
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "concept",
                    &binding.write_concept_uid,
                    "feature_binding",
                    store::sync_ops::OpKind::Set,
                    Some(serde_json::to_string(&binding)?),
                )
                .await?;
                data = Some(json!({"feature_binding":binding}));
            }
            Command::InspectFeature { .. } | Command::PreviewFeature { .. } => unreachable!(),
            Command::SetTerminologyContext { context } => {
                store::vocabulary::set_terminology_context_on(
                    &mut tx,
                    store::vocabulary::terminology_owner(actor),
                    &context,
                )
                .await?;
            }
            Command::PreferTerm { concept, term } => {
                store::vocabulary::choose_term_on(&mut tx, actor, &concept, Some(&term)).await?;
            }
            Command::UseWording { concept, wording } => {
                store::vocabulary::use_wording_on(&mut tx, actor, &concept, &wording).await?;
            }
            Command::HideTerm { term, hidden } => {
                store::vocabulary::hide_term_on(&mut tx, actor, &term, hidden).await?;
            }
            Command::RestoreOriginal { concept } => {
                store::vocabulary::choose_term_on(&mut tx, actor, &concept, None).await?;
            }
            Command::ResetTerminology { concept } => {
                store::vocabulary::reset_terminology_on(&mut tx, actor, &concept).await?;
            }
            Command::InspectTerminology => unreachable!(),
            Command::InspectMeaning { .. } => unreachable!(),
            Command::InspectMeasurement { .. } => unreachable!(),
            Command::BindQuery { .. } => unreachable!(),
            Command::CreateVocabulary { name, organ } => {
                self.refuse_unreadable(actor, std::slice::from_ref(&organ))
                    .await?;
                created =
                    Some(store::vocabulary::create_vocabulary_on(&mut tx, &name, &organ).await?);
            }
            Command::ReviseDefinition {
                concept,
                expected_revision,
                definition,
            } => {
                let revision = store::vocabulary::revise_definition_on(
                    &mut tx,
                    &concept,
                    expected_revision,
                    &definition,
                )
                .await?;
                data = Some(json!({"definition_revision":revision}));
                changed_concept = Some(concept);
            }
            Command::Publish {
                vocabulary,
                expected_revision,
            } => {
                let package =
                    store::vocabulary::publish_on(&mut tx, &vocabulary, expected_revision).await?;
                created = Some(package.uid.clone());
                data = Some(json!({"publication":package}));
            }
            Command::Withdraw { publication } => {
                store::vocabulary::withdraw_on(&mut tx, &publication).await?;
            }
            Command::Adopt { publication, organ } => {
                self.refuse_unreadable(actor, std::slice::from_ref(&organ))
                    .await?;
                let package =
                    store::vocabulary::publication_on(&mut tx, &publication, true).await?;
                store::vocabulary::adopt_on(&mut tx, &package, &organ).await?;
                data = Some(json!({"publication":package}));
            }
            Command::AdoptPackage { package, organ } => {
                self.refuse_unreadable(actor, std::slice::from_ref(&organ))
                    .await?;
                let package = store::vocabulary::read_package(&package)?;
                store::vocabulary::adopt_on(&mut tx, &package, &organ).await?;
                created = Some(package.vocabulary_uid.clone());
                data = Some(json!({"publication":package}));
            }
            Command::InspectPublication { .. }
            | Command::PreviewPackage { .. }
            | Command::RecordHistory { .. } => unreachable!(),
            Command::SetLanguage { language } => {
                let language = store::vocabulary::language_tag(&language)?;
                if language.is_empty() {
                    return Err(EngineError::Consequence("Choose a display language".into()));
                }
                if let Some(actor) = actor {
                    store::sqlx::query("INSERT INTO person_terminology(person_uid,language) VALUES(?,?) ON CONFLICT(person_uid) DO UPDATE SET language=excluded.language")
                        .bind(actor).bind(language).execute(&mut *tx).await?;
                } else {
                    store::sqlx::query("INSERT OR IGNORE INTO configuration(id,created_at,updated_at) VALUES(1,?,?)")
                        .bind(nucleus::execution::now().to_rfc3339()).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *tx).await?;
                    store::sqlx::query("UPDATE configuration SET language=? WHERE id=1")
                        .bind(language)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            Command::CreateConcept {
                vocabulary,
                name,
                language,
                definition,
                derived_from,
            } => {
                store::vocabulary::require_local_vocabulary_on(&mut tx, &vocabulary).await?;
                let name = store::vocabulary::term_text(&name)?;
                let language = store::vocabulary::language_tag(&language)?;
                if definition.len() > 16384 || definition.trim().is_empty() {
                    return Err(EngineError::Consequence(
                        "Describe the source meaning in 1–16384 bytes".into(),
                    ));
                }
                let uid = nucleus::new_uid("c");
                store::vocabulary::validate_source_parent_on(
                    &mut tx,
                    &uid,
                    derived_from.as_deref(),
                )
                .await?;
                store::sqlx::query(
                    "INSERT INTO concept(uid,canonical_name,created_at) VALUES(?,?,?)",
                )
                .bind(&uid)
                .bind(&name)
                .bind(nucleus::execution::now().to_rfc3339())
                .execute(&mut *tx)
                .await?;
                store::sqlx::query("INSERT INTO concept_source(concept_uid,lingua_uid,original_name,original_language,definition,derived_from) VALUES(?,?,?,?,?,?)")
                    .bind(&uid).bind(&vocabulary).bind(&name).bind(&language).bind(definition.trim()).bind(derived_from).execute(&mut *tx).await?;
                store::sqlx::query(
                    "INSERT INTO lingua_concept(lingua_uid,concept_uid,adopted_at) VALUES(?,?,?)",
                )
                .bind(vocabulary)
                .bind(&uid)
                .bind(nucleus::execution::now().to_rfc3339())
                .execute(&mut *tx)
                .await?;
                store::vocabulary::add_term_on(&mut tx, &uid, &name, &language, None, true).await?;
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "concept",
                    &uid,
                    "canonical_name",
                    store::sync_ops::OpKind::Set,
                    Some(json!(name).to_string()),
                )
                .await?;
                created = Some(uid);
                changed_concept = created.clone();
            }
            Command::AddTerm {
                concept,
                name,
                language,
                preferred,
                ..
            } => {
                let exists: bool =
                    store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept WHERE uid=?)")
                        .bind(&concept)
                        .fetch_one(&mut *tx)
                        .await?;
                if !exists {
                    return Err(EngineError::Consequence(
                        "Choose a source Concept identity".into(),
                    ));
                }
                created = Some(
                    store::vocabulary::add_term_on(
                        &mut tx,
                        &concept,
                        &name,
                        &language,
                        if personal { actor } else { None },
                        preferred,
                    )
                    .await?,
                );
                if !personal {
                    changed_concept = Some(concept);
                }
            }
            Command::EditTerm {
                term,
                expected_revision,
                name,
                language,
                preferred,
            } => {
                store::vocabulary::edit_term_on(
                    &mut tx,
                    &term,
                    expected_revision,
                    &name,
                    &language,
                    preferred,
                )
                .await?;
                if !personal {
                    changed_concept = store::sqlx::query_scalar(
                        "SELECT concept_uid FROM concept_term WHERE uid=?",
                    )
                    .bind(term)
                    .fetch_optional(&mut *tx)
                    .await?;
                }
            }
            Command::RetireTerm {
                term,
                expected_revision,
            } => {
                store::vocabulary::retire_term_on(&mut tx, &term, expected_revision).await?;
                if !personal {
                    changed_concept = store::sqlx::query_scalar(
                        "SELECT concept_uid FROM concept_term WHERE uid=?",
                    )
                    .bind(term)
                    .fetch_optional(&mut *tx)
                    .await?;
                }
            }
            Command::BindLocationConcept {
                concept,
                enabled,
                write,
                expected_revision,
            } => {
                let row = store::sqlx::query(
                    "SELECT write_concept_uid,revision FROM vocabulary_role WHERE uid=?",
                )
                .bind(SAVED_LOCATION_ROLE_UID)
                .fetch_one(&mut *tx)
                .await?;
                if row.get::<i64, _>("revision") != expected_revision {
                    return Err(EngineError::Consequence(
                        "The location bindings changed; refresh before saving".into(),
                    ));
                }
                if !enabled && (write || row.get::<String, _>("write_concept_uid") == concept) {
                    return Err(EngineError::Consequence(
                        "Choose another location write Concept before removing this binding".into(),
                    ));
                }
                if enabled {
                    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM vocabulary_role_concept WHERE role_uid=? AND concept_uid!=?").bind(SAVED_LOCATION_ROLE_UID).bind(&concept).fetch_one(&mut *tx).await?;
                    if count >= 128 {
                        return Err(EngineError::Consequence(
                            "Accept at most 128 location Concepts".into(),
                        ));
                    }
                    let invalid: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_assertion WHERE predicate_uid=? AND retracted_at IS NULL AND (role!='ordinary' OR object_uid IS NULL OR subject_uid=object_uid OR quantity_mantissa IS NOT NULL OR unit_uid IS NOT NULL))")
                        .bind(&concept).fetch_one(&mut *tx).await?;
                    if invalid {
                        return Err(EngineError::Consequence("This Concept has Assertions incompatible with a saved geographic location".into()));
                    }
                    store::sqlx::query("INSERT INTO vocabulary_role_concept(role_uid,concept_uid) VALUES(?,?) ON CONFLICT DO NOTHING").bind(SAVED_LOCATION_ROLE_UID).bind(&concept).execute(&mut *tx).await?;
                } else {
                    store::sqlx::query(
                        "DELETE FROM vocabulary_role_concept WHERE role_uid=? AND concept_uid=?",
                    )
                    .bind(SAVED_LOCATION_ROLE_UID)
                    .bind(&concept)
                    .execute(&mut *tx)
                    .await?;
                }
                store::sqlx::query("UPDATE vocabulary_role SET revision=revision+1,write_concept_uid=CASE WHEN ? THEN ? ELSE write_concept_uid END WHERE uid=?")
                    .bind(write).bind(concept).bind(SAVED_LOCATION_ROLE_UID).execute(&mut *tx).await?;
                let (write_concept_uid, revision): (String, i64) = store::sqlx::query_as(
                    "SELECT write_concept_uid,revision FROM vocabulary_role WHERE uid=?",
                )
                .bind(SAVED_LOCATION_ROLE_UID)
                .fetch_one(&mut *tx)
                .await?;
                let concept_uids = store::sqlx::query_scalar("SELECT concept_uid FROM vocabulary_role_concept WHERE role_uid=? ORDER BY concept_uid")
                    .bind(SAVED_LOCATION_ROLE_UID).fetch_all(&mut *tx).await?;
                let binding = nucleus::vocabulary::LocationBinding {
                    write_concept_uid,
                    revision,
                    concept_uids,
                };
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "concept",
                    nucleus::vocabulary::TOPOS_UID,
                    "location_binding",
                    store::sync_ops::OpKind::Set,
                    Some(serde_json::to_string(&binding)?),
                )
                .await?;
            }
        }
        if let Some(concept) = changed_concept {
            store::vocabulary::log_shared_on(&mut tx, &concept).await?;
        }
        if preferences {
            data = Some(
                json!({"terminology":store::vocabulary::terminology_state_on(&mut tx, actor).await?}),
            );
        }
        tx.commit().await?;
        drop(serial);
        if language_changed {
            self.notify_config_changed();
        }
        self.notify_query_changed();
        Ok(ActionOutcome {
            created,
            data,
            ..Default::default()
        })
    }
}
