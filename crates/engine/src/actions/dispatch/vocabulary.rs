// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    invalid_dispatch,
};

impl Engine {
    pub(super) async fn dispatch_vocabulary(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CreateLingua { name, visibility } => {
                outcome.created = Some(
                    store::linguas::create(
                        &self.store.pool,
                        &name,
                        store::organs::local(&self.store.pool)
                            .await?
                            .map(|organ| organ.uid)
                            .as_deref(),
                        &visibility,
                    )
                    .await?,
                );
            }
            Action::RenameLingua { lingua, name } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                let mut ownership = self.store.pool.acquire().await?;
                store::vocabulary::require_local_vocabulary_on(&mut ownership, &lingua_uid).await?;
                drop(ownership);
                store::linguas::rename(&self.store.pool, &lingua_uid, &name).await?;
            }
            Action::DeleteLingua { lingua } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                let mut ownership = self.store.pool.acquire().await?;
                store::vocabulary::require_local_vocabulary_on(&mut ownership, &lingua_uid).await?;
                drop(ownership);
                if lingua_uid == store::linguas::LOCAL_UID {
                    return Err(EngineError::Consequence(
                        "the local Lingua is the ontology's permanent private home".into(),
                    ));
                }
                if let Err(error) = store::linguas::delete(&self.store.pool, &lingua_uid).await {
                    if error
                        .as_database_error()
                        .is_some_and(|error| error.is_foreign_key_violation())
                    {
                        return Err(EngineError::Conflict { code: "lingua_in_use", message: "This Vocabulary is the source of retained Concepts. Remove adopted entries or retire their terms while preserving the source.".into() });
                    }
                    return Err(error.into());
                }
            }
            Action::CreateConcept {
                lingua,
                name,
                parents,
            } => {
                self.require_permission(actor.as_deref(), "permission:assign")
                    .await?;
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                let mut ownership = self.store.pool.acquire().await?;
                store::vocabulary::require_local_vocabulary_on(&mut ownership, &lingua_uid).await?;
                drop(ownership);
                let mut parent_uids = Vec::new();
                for p in &parents {
                    parent_uids.push(
                        store::concepts::resolve(&self.store.pool, p)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(p.clone()))?,
                    );
                }
                let refs: Vec<&str> = parent_uids.iter().map(String::as_str).collect();
                outcome.created = Some(
                    store::concepts::create_in(&self.store.pool, &lingua_uid, &name, &refs).await?,
                );
            }
            Action::RenameConcept { concept, name } => {
                self.require_permission(actor.as_deref(), "permission:assign")
                    .await?;
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                let mut ownership = self.store.pool.acquire().await?;
                store::vocabulary::require_local_concept_on(&mut ownership, &concept_uid).await?;
                drop(ownership);
                store::concepts::rename(&self.store.pool, &concept_uid, &name).await?;
            }
            Action::DeleteConcept { concept } => {
                self.require_permission(actor.as_deref(), "permission:assign")
                    .await?;
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                let mut ownership = self.store.pool.acquire().await?;
                store::vocabulary::require_local_concept_on(&mut ownership, &concept_uid).await?;
                drop(ownership);
                match store::concepts::delete(&self.store.pool, &concept_uid).await {
                    Err(error)
                        if error
                            .as_database_error()
                            .is_some_and(|error| error.is_foreign_key_violation()) =>
                    {
                        return Err(EngineError::Conflict {
                            code: "concept_in_use",
                            message: "This concept is still used by Records, assertion history or other saved data. Remove it from a Lingua instead of deleting it."
                                .into(),
                        });
                    }
                    result => {
                        result?;
                    }
                }
            }
            Action::AdoptConcept { lingua, concept } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                let mut ownership = self.store.pool.acquire().await?;
                store::vocabulary::require_local_vocabulary_on(&mut ownership, &lingua_uid).await?;
                drop(ownership);
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                store::linguas::adopt(&self.store.pool, &lingua_uid, &concept_uid).await?;
            }
            Action::RemoveConceptFromLingua { lingua, concept } => {
                let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                let mut ownership = self.store.pool.acquire().await?;
                store::vocabulary::require_local_vocabulary_on(&mut ownership, &lingua_uid).await?;
                drop(ownership);
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                store::linguas::remove_concept(&self.store.pool, &lingua_uid, &concept_uid).await?;
            }
            Action::AddConceptParent { concept, parent } => {
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                let parent_uid = store::concepts::resolve(&self.store.pool, &parent)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(parent))?;
                store::concepts::add_parent(&self.store.pool, &concept_uid, &parent_uid).await?;
            }
            Action::RemoveConceptParent { concept, parent } => {
                let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                let parent_uid = store::concepts::resolve(&self.store.pool, &parent)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(parent))?;
                store::concepts::remove_parent(&self.store.pool, &concept_uid, &parent_uid).await?;
            }
            Action::PreviewInstinct => {
                outcome.data = Some(serde_json::to_value(
                    self.preview_instinct(actor.as_deref()).await?,
                )?);
            }
            Action::ImportInstinct { fingerprint } => {
                outcome = self
                    .import_instinct(&fingerprint, actor.as_deref(), now)
                    .await?;
            }
            Action::CreateMatchRule {
                slug,
                head,
                watch_concept,
                max_proximity,
                min_confidence,
                auto,
            } => {
                if !matches!(auto.as_str(), "draft_only" | "ask" | "auto_propose") {
                    return Err(EngineError::Consequence(format!(
                        "auto must be draft_only | ask | auto_propose, not `{auto}`"
                    )));
                }
                outcome.created = Some(
                    store::senses::create_sense_rule(
                        &self.store.pool,
                        store::senses::NewSenseRule {
                            slug: &slug,
                            head: &head,
                            watch_concept: watch_concept.as_deref(),
                            max_proximity,
                            min_confidence,
                            auto: &auto,
                        },
                    )
                    .await?,
                );
            }
            Action::AdoptConcepts { concepts } => {
                for seed in &concepts {
                    store::concepts::adopt(
                        &self.store.pool,
                        &seed.uid,
                        &seed.name,
                        seed.origin.as_deref(),
                        &seed.parents,
                    )
                    .await?;
                }
            }
            Action::DeclareEquivalence { a, b } => {
                let a = store::concepts::resolve(&self.store.pool, &a)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(a))?;
                let b = store::concepts::resolve(&self.store.pool, &b)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(b))?;
                store::concepts::declare_equivalence(&self.store.pool, &a, &b, actor.as_deref())
                    .await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
