use super::*;
use nucleus::{
    transfer::receipt_history::Provenance,
    trust_analysis::{
        IdentityContext, MAX_BYTES, MAX_DISCLOSURES, MAX_IDENTITIES,
        contacts::{self, CheckedDisclosure},
        evidence, paths,
    },
};
mod loading;
mod analysis;
#[cfg(test)]
mod tests;

struct Original {
    artifact: Arc<Artifact>,
    provenance: Option<Provenance>,
    signature: Verification,
}

#[derive(Default)]
struct Originals {
    items: Vec<Original>,
    bytes: usize,
    events: usize,
    identities: BTreeSet<Identity>,
    partial: bool,
    local_complete: bool,
    excluded: BTreeSet<Identity>,
    withheld_sources: BTreeSet<(String, CanonicalHash)>,
    custody_records:BTreeSet<String>,
}

impl Originals {
    fn push(
        &mut self,
        artifact: Arc<Artifact>,
        provenance: Option<Provenance>,
    ) -> Result<bool, EngineError> {
        if packet_profiles(&artifact)
            .iter()
            .any(|identity| self.excluded.contains(identity))
        {
            return Ok(true);
        }
        let bytes = artifact.bytes().map_err(invalid)?.len();
        let events = match artifact.as_ref() {
            Artifact::Receipt(document) => document
                .public_bundle()
                .map_or(1, |bundle| bundle.publication.events.len().max(1)),
            _ => 1,
        };
        let identities = self
            .identities
            .iter()
            .cloned()
            .chain(packet_profiles(&artifact))
            .collect::<BTreeSet<_>>();
        if self.bytes.saturating_add(bytes) > MAX_BYTES
            || self.events.saturating_add(events) > MAX_DISCLOSURES
            || identities.len() > MAX_IDENTITIES
        {
            self.partial = true;
            return Ok(false);
        }
        self.bytes += bytes;
        self.events += events;
        self.identities = identities;
        self.items.push(Original {
            artifact,
            provenance,
            signature: Verification::Invalid,
        });
        Ok(true)
    }
}

impl Engine {
    pub(super) async fn verify_remote_evidence(
        &self,
        id: &str,
        review: &Review,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        let _slot = self
            .trust_remote
            .slots
            .try_acquire()
            .map_err(|_| invalid("Evidence verification is at its concurrent work limit"))?;
        tokio::time::timeout(
            Duration::from_secs(15),
            self.verify_remote_evidence_inner(id, review, actor),
        )
        .await
        .map_err(|_| invalid("Evidence verification reached its work deadline; narrow the query"))?
    }

    async fn verify_remote_evidence_inner(
        &self,
        id: &str,
        review: &Review,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        let now = nucleus::execution::now();
        let epoch = *self.watch_query_changes().borrow();
        let (found, partial) = {
            let mut jobs = self.trust_remote.jobs.lock().await;
            let job = jobs
                .get_mut(id)
                .ok_or_else(|| invalid("This transient query was cleared"))?;
            job.expire();
            if matches!(job.state, State::Queued | State::Running | State::Cancelled) {
                return Err(invalid(
                    "Wait for collection to finish before merging originals",
                ));
            }
            (
                job.artifacts.clone(),
                job.partial || job.state != State::Completed,
            )
        };
        let hashes = found.keys().cloned().collect::<BTreeSet<_>>();
        let mut originals = Originals {
            partial,
            local_complete: true,
            excluded: review
                .draft
                .query
                .exclude_everywhere
                .iter()
                .cloned()
                .collect(),
            ..Default::default()
        };
        for (hash, found) in &found {
            if found.expires_at <= now.timestamp()
                || self
                    .remote_packet_access(review, actor, &found.artifact)
                    .await
                    .is_err()
            {
                originals.partial = true;
                continue;
            }
            originals.push(
                found.artifact.clone(),
                Some(Provenance::Artifact { hash: hash.clone() }),
            )?;
        }
        self.merge_local_originals(review, actor, &mut originals)
            .await?;
        let before = super::super::serving::authority_stamp_many(
            &mut *self.store.pool.acquire().await?,
            originals.items.iter().map(|input| input.artifact.as_ref()),
        )
        .await?;
        for original in &mut originals.items {
            original.signature = self.remote_signature(&original.artifact, now).await;
        }
        let receipts = originals
            .items
            .iter()
            .filter_map(|input| {
                if input.signature != Verification::Accepted {
                    return None;
                }
                match (input.artifact.as_ref(), &input.provenance) {
                    (Artifact::Receipt(document), Some(provenance)) => Some(evidence::Receipt {
                        document,
                        provenance: provenance.clone(),
                    }),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        let mut groups = evidence::receipts(&receipts, now.timestamp()).map_err(invalid)?;
        for group in &mut groups {
            group.partial |= !originals.local_complete || originals.withheld_sources.contains(&(group.origin.clone(), group.occurrence.clone()));
        }
        let claims = originals
            .items
            .iter()
            .filter_map(|input| {
                if !matches!(
                    input.signature,
                    Verification::Accepted | Verification::WithheldAuthority
                ) {
                    return None;
                }
                match input.artifact.as_ref() {
                    Artifact::Claim(document) => Some(evidence::Claim {
                        document,
                        provenance: input.provenance.clone(),
                        authority_current: input.signature == Verification::Accepted,
                    }),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        let mut heads = evidence::claim_heads(&claims, now.timestamp()).map_err(invalid)?;
        if !originals.local_complete {
            for head in &mut heads {
                head.state = evidence::HeadState::Withheld;
                head.revision = None;
                head.originals.clear();
            }
        }
        let disclosures = originals
            .items
            .iter()
            .filter_map(|input| {
                if !matches!(
                    input.signature,
                    Verification::Accepted | Verification::WithheldAuthority
                ) {
                    return None;
                }
                match input.artifact.as_ref() {
                    Artifact::Contact(document) => Some(CheckedDisclosure {
                        document: document.clone(),
                        readable: input.provenance.is_some(),
                        authority_current: input.signature == Verification::Accepted,
                    }),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        let mut policy =
            review.draft.query.contacts.clone().unwrap_or_else(|| {
                paths::Policy::for_anchor(Identity::Person(review.person.clone()))
            });
        policy
            .exclude_everywhere
            .extend(review.draft.query.exclude_everywhere.clone());
        policy.exclude_everywhere.sort();
        policy.exclude_everywhere.dedup();
        let mut tx = self.store.pool.begin().await?;
        let reader = self
            .contact_reader_on(&mut tx, &review.person, actor, review.draft.as_organ)
            .await?;
        let mut contexts = BTreeMap::<Identity, IdentityContext>::new();
        for identity in originals
            .identities
            .iter()
            .chain(std::iter::once(&policy.anchor))
        {
            let mut context = self
                .contact_identity_context_on(&mut tx, identity, actor)
                .await?;
            context.known |= identity == &reader;
            contexts.insert(identity.clone(), context);
        }
        if contexts.len() > MAX_IDENTITIES {
            return Err(invalid("The merged evidence identity budget was reached"));
        }
        let original_contexts = contexts.clone();
        let edges = if originals.local_complete {
            contacts::permitted_edges(&disclosures, &contexts, &reader, now.timestamp())
                .map_err(invalid)?
        } else {
            Vec::new()
        };
        for edge in &edges {
            if edge.from == reader {
                if let Some(context) = contexts.get_mut(&edge.to) {
                    context.known = true;
                }
            }
        }
        let paths = paths::search(&edges, &contexts, &policy).map_err(invalid)?;
        let used = paths
            .paths
            .iter()
            .flat_map(|path| path.proofs.iter().flatten())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut proof_sources: BTreeMap<CanonicalHash, BTreeSet<Provenance>> = BTreeMap::new();
        for original in &originals.items {
            if original.signature != Verification::Accepted {
                continue;
            }
            if let (Artifact::Contact(document), Some(provenance)) =
                (original.artifact.as_ref(), &original.provenance)
            {
                let statement = document.statement.id().map_err(invalid)?;
                if used.contains(&statement) {
                    proof_sources
                        .entry(statement)
                        .or_default()
                        .insert(provenance.clone());
                }
            }
        }
        let receipt_sources = groups
            .iter()
            .filter(|group| {
                !review.draft.receipt_sources.iter().any(|source| {
                    source.origin == group.origin && source.occurrence == group.occurrence
                })
            })
            .take(32)
            .map(|group| model::ReceiptSource {
                origin: group.origin.clone(),
                occurrence: group.occurrence.clone(),
            })
            .collect::<Vec<_>>();
        let mut contact_subjects = BTreeSet::new();
        for original in &originals.items {
            if original.signature != Verification::Accepted
                || !matches!(&original.provenance, Some(Provenance::Artifact { .. }))
            {
                continue;
            }
            if let Artifact::Contact(document) = original.artifact.as_ref() {
                if document.statement.audience != contacts::Audience::Public {
                    continue;
                }
                for identity in [&document.statement.issuer, &document.statement.contact] {
                    if identity != &policy.anchor
                        && identity != &reader
                        && !(review.draft.include_contacts
                            && identity == &review.draft.query.subject)
                        && !review.draft.contact_subjects.contains(identity)
                        && !policy.exclude_everywhere.contains(identity)
                    {
                        if let Some(context) = contexts.get(identity) {
                            if context.readable
                                && !context.blocked
                                && (policy.targets.allows(identity, context)
                                    || policy.intermediaries.allows(identity, context))
                            {
                                contact_subjects.insert(identity.clone());
                            }
                        }
                    }
                }
            }
        }
        let statuses = originals
            .items
            .iter()
            .filter_map(|input| {
                input
                    .provenance
                    .as_ref()
                    .map(|provenance| json!({"original":provenance,"signature":input.signature}))
            })
            .collect::<Vec<_>>();
        let partial = originals.partial
            || groups.iter().any(|group| group.partial)
            || paths.identity_limit_reached
            || paths.depth_limit_reached
            || originals.items.iter().any(|input| {
                input.provenance.is_some() && input.signature != Verification::Accepted
            });
        let mut analysis = nucleus::trust_analysis::query::analyze(
            groups.iter().map(evidence::ReceiptGroup::source_evidence).collect(),
            &contexts,
            review.draft.query.contacts.as_ref().map(|_| &paths),
            &review.draft.query,
        ).map_err(invalid)?;
        analysis.partial |= partial;
        let claim_checks = analysis::claim_checks(review, &originals, &groups, &contexts, &paths, &heads, partial, now.timestamp())?;
        drop(tx);
        #[cfg(feature = "trust-test-hooks")]
        {
            let checkpoint = {
                let mut held = self.trust_remote.verification_checkpoint.lock().await;
                if held.as_ref().is_some_and(|(job, _)| job == id) { held.take().map(|(_, checkpoint)| checkpoint) } else { None }
            };
            if let Some(checkpoint) = checkpoint {
                checkpoint.entered.notify_one();
                checkpoint.resumed.notified().await;
            }
        }
        let finished_at = nucleus::execution::now();
        let mut tx = self.store.pool.begin().await?;
        self.remote_access_on(&mut tx, review, actor, None).await?;
        for (identity, previous) in &original_contexts {
            let mut context = self
                .contact_identity_context_on(&mut tx, identity, actor)
                .await?;
            context.known |= identity == &reader;
            if &context != previous {
                return Err(invalid(
                    "A path identity or proximity changed while merging; verify again",
                ));
            }
        }
        for input in &originals.items {
            if let Some(provenance) = &input.provenance {
                self.remote_packet_access_on(&mut tx, review, actor, &input.artifact)
                    .await?;
                if let Provenance::Record { record, .. } = provenance {
                    self.contact_record_readable_on(&mut tx, record, actor)
                        .await?;
                    if originals.custody_records.contains(record) {self.validate_local_custody_on(&mut tx,record,actor,finished_at.timestamp()).await?;}
                } else if let Provenance::Artifact { hash } = provenance {
                    if found
                        .get(hash)
                        .is_none_or(|found| found.expires_at <= finished_at.timestamp())
                    {
                        return Err(invalid(
                            "An original's serving term expired while merging; refresh",
                        ));
                    }
                }
                if let Artifact::Claim(_) | Artifact::Contact(_) = input.artifact.as_ref() {
                    let expires = input.artifact.expires_at().map_err(invalid)?;
                    if expires > now.timestamp() && expires <= finished_at.timestamp() {
                        return Err(invalid(
                            "A current statement expired while merging; verify again",
                        ));
                    }
                }
            }
        }
        let after = super::super::serving::authority_stamp_many(
            &mut tx,
            originals.items.iter().map(|input| input.artifact.as_ref()),
        )
        .await?;
        if before != after || *self.watch_query_changes().borrow() != epoch {
            return Err(invalid(
                "Evidence, identity or permissions changed while merging; verify again",
            ));
        }
        let jobs = self.trust_remote.jobs.lock().await;
        if !jobs.get(id).is_some_and(|job| {
            job.state != State::Cancelled
                && job.artifacts.keys().cloned().collect::<BTreeSet<_>>() == hashes
        }) {
            return Err(invalid("The query was cleared or changed while merging"));
        }
        let result = json!({"trust":{"kind":"remote_verified","job":id,"query":review.draft.query,"analysis":analysis,"claim_checks":claim_checks,"checked_at":finished_at.timestamp(),"partial":partial,"receipts":groups,"claims":heads,"paths":paths,"proof_sources":proof_sources,"signatures":statuses,"receipt_sources":receipt_sources,"contact_subjects":contact_subjects.into_iter().take(32).collect::<Vec<_>>(),"notice":"Permitted local and collected originals evaluated together under the reviewed reader policy. Raw quantities and weighted indices are separate from physical truth. No import, identity adoption or network request occurred."}});
        let mut size = super::worker::Counter {
            bytes: 0,
            limit: MAX_BYTES,
        };
        serde_json::to_writer(&mut size, &result).map_err(|_| {
            invalid("The merged result exceeded its byte budget; narrow this query")
        })?;
        Ok(ActionOutcome {
            data: Some(result),
            ..Default::default()
        })
    }
}
