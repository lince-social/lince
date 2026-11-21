//! Transfer query execution and output ordering.

mod actions;
mod audience;
mod evidence;
mod parties;
mod recipients;
mod state;

use self::{
    audience::Audience,
    state::{Status, Terms},
};

use super::{
    filter,
    hierarchy::{TransferOutput, attach_hierarchy_projection},
    history::{
        Timeline, parse_transfer_revision_evidence, transfer_fact_proof,
        transfer_revision_projection, transfer_timeline,
    },
    invitations, karma_state,
    occurrences::{self, Occurrences},
    order::SortKey,
    origin_delivery::origin_social_delivery_projection,
    projection,
    promises::{self, Promises},
    publication,
    query_context::{self, QueryContext},
    remote_delivery::append_remote_transfer_delivery_rows,
    roles,
    status::{agreement_required, correction_status},
    viewer::TransferViewer,
    visibility::{self, RelatedTransfers, Scope},
};
use crate::{Protein, ProteinError, ThreadsInclude, record_availability, record_threads};
use nucleus::performance::{Span, add};
use serde_json::{Value, from_str, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use store::{
    Store,
    action_intents::fact_has_committed_intent,
    concepts::canonical_names,
    facts, organs,
    records::{self, RecordSelection},
    transfer_agreement,
    transfers::{self, TransferListRow},
    transport,
};

pub(super) async fn execute_transfers(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
    viewer_override: Option<TransferViewer>,
) -> Result<Vec<Value>, ProteinError> {
    let filter_context = filter::Context::prepare(store, &protein.filter).await?;
    let order = SortKey::parse(&protein.order)?;
    let records = {
        let _read = Span::new("protein-transfer-records-read-await");
        records::read::<Option<String>>(&store.pool, RecordSelection::All, false).await?
    };
    let records_by_uid: projection::RecordIndex<'_> = {
        let _index = Span::new("protein-transfer-record-index");
        records
            .iter()
            .map(|record| (record.uid.as_str(), record))
            .collect()
    };
    let concept_names: HashMap<_, _> = canonical_names(&store.pool).await?.into_iter().collect();
    let viewer = match viewer_override {
        Some(viewer) => viewer,
        None => TransferViewer::resolve(store, subject).await?,
    };
    let QueryContext {
        row,
        create_person,
        need_outcomes,
    } = query_context::read(
        store,
        &viewer,
        installed_signer_actor,
        visible,
        &records_by_uid,
    )
    .await?;
    let query = RowQuery {
        protein,
        visible,
        installed_signer_actor,
        viewer: &viewer,
        records: &records_by_uid,
        concept_names: &concept_names,
        filter: &filter_context,
        create_person,
        need_outcomes: &need_outcomes,
    };
    let mut transfer_rows = Vec::new();
    for transfer in transfers::list_all(&store.pool).await? {
        if let Some(row) = query.project(store, transfer).await? {
            transfer_rows.push(row);
        }
    }
    append_remote_transfer_delivery_rows(
        store,
        protein,
        &viewer,
        installed_signer_actor,
        &mut transfer_rows,
        visible,
    )
    .await?;
    SortKey::sort(&mut transfer_rows, &order);
    if let Some(limit) = protein.limit {
        transfer_rows.truncate(limit);
    }
    publication::finish(
        store,
        &mut transfer_rows,
        &viewer,
        installed_signer_actor,
        visible,
    )
    .await?;
    attach_hierarchy_projection(&mut transfer_rows);
    let mut out = vec![row];
    out.extend(transfer_rows.into_iter().map(|row| row.value));
    Ok(out)
}

struct RowQuery<'a> {
    protein: &'a Protein,
    visible: Option<&'a HashSet<String>>,
    installed_signer_actor: Option<&'a str>,
    viewer: &'a TransferViewer,
    records: &'a projection::RecordIndex<'a>,
    concept_names: &'a HashMap<String, String>,
    filter: &'a filter::Context<'a>,
    create_person: Option<&'a str>,
    need_outcomes: &'a Value,
}

impl RowQuery<'_> {
    async fn project(
        &self,
        store: &Store,
        TransferListRow {
            transfer,
            slug,
            head,
        }: TransferListRow,
    ) -> Result<Option<TransferOutput>, ProteinError> {
        let Self {
            protein,
            visible,
            installed_signer_actor,
            viewer,
            records: records_by_uid,
            concept_names,
            filter: filter_context,
            create_person,
            need_outcomes,
        } = *self;
        let uid = transfer.record_uid.as_str();
        let revision = transfer.revision.cast_unsigned();
        let creator = facts::creator_uid(&store.pool, uid).await?;
        let creator_person = transfers::creator_party_actor(&store.pool, uid).await?;
        let invitations = transfers::invitations_for_transfer(&store.pool, uid).await?;
        if visible.is_some_and(|targets| !targets.contains(uid)) {
            let viewer_created = viewer
                .subject
                .as_deref()
                .is_some_and(|subject| creator.as_deref() == Some(subject));
            let viewer_participates = match viewer.person.as_deref() {
                Some(person) => transfers::party_for_actor(&store.pool, uid, person)
                    .await?
                    .is_some(),
                None => false,
            };
            let viewer_was_addressed = viewer.person.as_deref().is_some_and(|person| {
                invitations
                    .iter()
                    .any(|invitation| invitation.addressed_person_uid == person)
            });
            if !viewer_created && !viewer_participates && !viewer_was_addressed {
                return Ok(None);
            }
        }
        let promises = transfers::promises_of(&store.pool, uid).await?;
        let correction_lineage = transfers::correction_links_for_transfer(&store.pool, uid).await?;
        let promise_successors =
            transfers::promise_successors_for_transfer(&store.pool, uid).await?;
        let mut open_claim_pairs_by_source = HashMap::new();
        for promise in &promises {
            let pairs = transfers::open_claim_pairs_for_source(&store.pool, &promise.uid).await?;
            if !pairs.is_empty() {
                open_claim_pairs_by_source.insert(promise.uid.as_str(), pairs);
            }
        }
        let occurrences = transfers::occurrences_of(&store.pool, uid).await?;
        let mut settlement_progress_by_occurrence = HashMap::new();
        for occurrence in &occurrences {
            if let Some(progress) =
                transfers::occurrence_settlement_progress(&store.pool, &occurrence.uid).await?
            {
                settlement_progress_by_occurrence.insert(occurrence.uid.as_str(), progress);
            }
        }
        let occurred_promises = occurrences
            .iter()
            .map(|occurrence| occurrence.promise_uid.as_str())
            .collect::<HashSet<_>>();
        let available_records: Vec<_> = promises
            .iter()
            .filter_map(|promise| promise.record_uid.as_deref())
            .filter(|uid| records_by_uid.contains_key(*uid))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let availability_by_record = record_availability(store, &available_records).await?;
        let parties = transfers::party_levels(&store.pool, uid).await?;
        let readiness_input = transfers::agreement_readiness_input(&store.pool, uid).await?;
        let agreement_events = transfers::agreement_events(&store.pool, uid, None).await?;
        let (mut agreement_readiness, mut dependency_status) =
            transfer_agreement::read(&store.pool, uid).await?;
        let RelatedTransfers {
            children,
            disclosure,
        } = visibility::related_transfers(
            store,
            &transfer,
            &readiness_input.dependencies,
            &mut agreement_readiness,
            &mut dependency_status,
            &Scope {
                viewer,
                visible,
                records: records_by_uid,
            },
        )
        .await?;
        let levels: Vec<i64> = parties.iter().map(|(_, _, level)| *level).collect();
        let states: Vec<nucleus::PromiseState> = promises.iter().map(|p| p.state).collect();
        let source_group_state =
            transfers::source_group_state_for_transfer(&store.pool, uid).await?;
        let current_revision_fact = if transfer.revision > 0 {
            transfers::revision_fact(&store.pool, uid, revision).await?
        } else {
            None
        };
        let revision_signed = match current_revision_fact.as_ref() {
            Some(fact) => {
                fact.signature.is_some()
                    || fact_has_committed_intent(&store.pool, &fact.uid).await?
            }
            None => false,
        };
        let current_revision_evidence = current_revision_fact
            .as_ref()
            .and_then(parse_transfer_revision_evidence);
        let revision_promises = current_revision_evidence
            .as_ref()
            .map(|evidence| evidence.terms.promises.as_slice());
        let committed = levels.iter().filter(|level| **level >= 2).count();
        let reviewed = levels.iter().filter(|level| **level >= 1).count();
        let required = agreement_required(
            &transfer.agreement_type,
            transfer.agreement_pct,
            parties.len(),
        );
        let audience = Audience::new(
            viewer,
            installed_signer_actor,
            creator.as_deref(),
            creator_person.as_deref(),
            &parties,
            &invitations,
        );
        let viewer_party = audience.party;
        let viewer_person = audience.person;
        let is_creator = audience.is_creator;
        let is_participant = viewer_party.is_some();
        let is_invitee = audience.is_invitee;
        let recipient_details_authorized = audience.show_details();
        let viewer_roles = audience.roles();
        let terms = Terms {
            transfer: &transfer,
            promises: &promises,
            states: &states,
            occurrences: &occurrences,
            progress: &settlement_progress_by_occurrence,
            occurred: &occurred_promises,
            parties: &parties,
            invitations: &invitations,
            readiness: &agreement_readiness,
        };
        let lifecycle = Status::derive(&terms, &levels, source_group_state.as_ref(), &audience);
        let status = lifecycle.status;
        if !filter_context.matches(
            &filter::Row {
                uid,
                slug: slug.as_deref(),
                revision,
                status,
                viewer_roles: &viewer_roles,
                invitations: &invitations,
                parties: &parties,
                promises: &promises,
                revision_promises,
                records_by_uid,
                viewer_person: viewer.person.as_deref(),
                viewer_involved: is_participant || is_invitee,
                viewer_privileged: viewer.local || is_creator,
            },
            &protein.filter,
        ) {
            return Ok(None);
        }
        let occurrence_roles = roles::validity(&readiness_input);
        let coalition_members = readiness_input.coalition.as_ref().map(|coalition| {
            coalition
                .party_uids
                .iter()
                .map(String::as_str)
                .collect::<HashSet<_>>()
        });
        let agreement_options = projection::agreement_options(
            &readiness_input,
            viewer,
            revision_signed,
            coalition_members.as_ref(),
            installed_signer_actor,
        );
        let actions = actions::Actions::derive(
            &terms,
            &readiness_input,
            &occurrence_roles,
            &audience,
            installed_signer_actor,
            creator_person.as_deref(),
        );
        let revision_evidence = if disclosure.history_hidden {
            add("protein-transfer-hidden-evidence", 1);
            Value::Null
        } else {
            transfer_revision_projection(store, uid, revision, recipient_details_authorized).await?
        };
        let mut threads = record_threads::for_record(
            store,
            uid,
            &ThreadsInclude {
                messages_limit: 100,
                ..Default::default()
            },
        )
        .await?;
        for thread in &mut threads {
            if thread.is_object() {
                projection::attach_action_access(
                    thread,
                    &[("create_message", &actions.negotiation)],
                );
            }
        }
        let invitation_history = invitations::read_history(store, &invitations).await?;
        let mut state_counts: BTreeMap<&str, usize> = Default::default();
        for state in &states {
            *state_counts.entry(state.as_str()).or_default() += 1;
        }
        let confirmations = facts::for_record(&store.pool, uid, 10_000)
            .await?
            .into_iter()
            .filter_map(|fact| {
                let payload = from_str::<Value>(fact.payload.as_deref()?).ok()?;
                let kind = payload.get("confirmation")?.as_str()?;
                Some(json!({
                    "kind": kind,
                    "actor": fact.actor_uid,
                    "at": fact.at,
                    "fact": fact.uid,
                }))
            })
            .collect::<Vec<_>>();
        let viewer_has_private_transfer_data = viewer.local
            || viewer.person.as_deref().is_some_and(|person| {
                promises
                    .iter()
                    .any(|promise| promise.party_uid.as_deref() == Some(person))
            });
        let local_organ_uid = organs::local(&store.pool).await?.map(|organ| organ.uid);
        let occurrence_projection = occurrences::project(
            store,
            &Occurrences {
                uid,
                revision: transfer.revision,
                viewer,
                visible,
                installed_signer_actor,
                creator_person: creator_person.as_deref(),
                create_person,
                need_outcomes,
                promises: &promises,
                occurrences: &occurrences,
                records_by_uid,
                settlement_progress_by_occurrence: &settlement_progress_by_occurrence,
                availability_by_record: &availability_by_record,
                concept_names,
                local_organ_uid: local_organ_uid.as_deref(),
                parties: &parties,
            },
        )
        .await?;
        let timeline = if disclosure.history_hidden {
            Vec::new()
        } else {
            transfer_timeline(
                store,
                &Timeline {
                    transfer_uid: uid,
                    revision,
                    invitations: &invitations,
                    agreement_events: &agreement_events,
                    occurrences: &occurrences,
                    source_group: source_group_state.as_ref(),
                    correction_lineage: &correction_lineage,
                    promise_successors: &promise_successors,
                    recipient_details_authorized,
                    viewer_person,
                },
            )
            .await?
        };
        let current_proof = match current_revision_fact
            .as_ref()
            .filter(|_| !disclosure.history_hidden)
        {
            Some(fact) => Some(transfer_fact_proof(store, &fact.uid).await?),
            None => None,
        };
        let visibility_projection = recipients::project(
            store,
            &transfer,
            creator_person.as_deref(),
            &parties,
            &invitations,
            &audience,
        )
        .await?;
        let mut balance: BTreeMap<&str, f64> = Default::default();
        for p in &promises {
            let key = p
                .record_uid
                .as_deref()
                .and_then(|uid| records_by_uid.get(uid))
                .and_then(|record| record.identity_predicate_uid.as_deref())
                .unwrap_or("(none)");
            *balance.entry(key).or_insert(0.0) += p.delta;
        }
        let balanced = !balance.is_empty() && balance.values().all(|v| v.abs() < 1e-9);
        let balance_detail = balance
            .iter()
            .map(|(&concept, delta)| {
                let concept = (concept != "(none)").then_some(concept);
                json!({
                    "concept": concept,
                    "concept_name": concept.and_then(|name| concept_names.get(name)),
                    "delta": delta,
                })
            })
            .collect::<Vec<_>>();
        let record = |uid| records_by_uid.get(uid);
        let parent_record = transfer.parent_uid.as_deref().and_then(record);
        let source_record = transfer.source_uid.as_deref().and_then(record);
        let mut value = json!({
            "kind": "transfer",
            "uid": uid,
            "slug": slug,
            "head": head,
            "revision": transfer.revision,
            "status": status,
            "primary_status": lifecycle.primary,
            "operational_status": lifecycle.primary,
            "inbox_facets": null,
            "balance": balance,
            "balance_detail": balance_detail,
            "balanced": balanced,
            "active": transfer.active,
            "agreement_type": transfer.agreement_type,
            "agreement_pct": transfer.agreement_pct,
            "settlement": transfer.settlement,
            "visibility": transfer.visibility,
            "max_proximity": transfer.max_proximity,
            "satiation": transfer.satiation,
            "first_completes_evidence": source_group_state.as_ref().map(evidence::first_completes),
            "parent": transfer.parent_uid,
            "parent_head": parent_record.map(|record| record.head.as_str()),
            "parent_slug": parent_record.and_then(|record| record.slug.as_deref()),
            "source": transfer.source_uid,
            "source_head": source_record.map(|record| record.head.as_str()),
            "source_slug": source_record.and_then(|record| record.slug.as_deref()),
            "correction_lineage": evidence::corrections(&correction_lineage, uid, recipient_details_authorized),
            "reserve_default": transfer.reserve_default,
            "require_confirmation": transfer.require_confirmation,
            "default_place": transfer.default_place,
            "viewer_party": viewer_party,
            "viewer_roles": viewer_roles,
            "revision_evidence": null,
            "proof": current_proof,
            "timeline": null,
            "visibility_projection": null,
            "threads": null,
            "capabilities": null,
            "blocking_reasons": null,
            "agreement": {
                "derived_from_children": lifecycle.derived_parent,
                "reviewed": reviewed,
                "committed": committed,
                "required": required,
                "total": parties.len(),
                "policy_satisfied": agreement_readiness.ready,
                "ready": agreement_readiness.ready,
                "readiness": {
                    "ready": agreement_readiness.ready,
                    "blockers": agreement_readiness.blockers,
                },
                "coalition": readiness_input.coalition.as_ref().map(|coalition| json!({
                    "revision": coalition.revision,
                    "threshold_pct": coalition.threshold_pct,
                    "eligible_count": coalition.eligible_count,
                    "frozen_by_event": coalition.frozen_by_event_uid,
                    "frozen_at": coalition.frozen_at,
                    "party_uids": coalition.party_uids,
                })),
                "history": agreement_events.iter().filter(|event| audience.sees_person(&event.person_uid)).map(|event| json!({
                    "uid": event.uid,
                    "revision": event.revision,
                    "party": event.party_uid,
                    "person": event.person_uid,
                    "from_level": event.from_level,
                    "to_level": event.to_level,
                    "fact": event.fact_uid,
                    "request_id": event.idempotency_key,
                    "at": event.created_at,
                })).collect::<Vec<_>>(),
            },
            "readiness": {
                "ready": agreement_readiness.ready,
                "blockers": agreement_readiness.blockers,
            },
            "outcome": {"revision":agreement_readiness.revision, "agreed":agreement_readiness.ready, "settled":agreement_readiness.settled},
            "observed_fulfilment": agreement_readiness.observed.then(|| json!({
                "fulfilled": agreement_readiness.ready,
                "evidence": viewer.local.then_some(&dependency_status),
            })),
            "children": null,
            "dependencies": dependency_status,
            "progress": state_counts,
            "settlement_progress": {
                "occurrences": occurrences.len(),
                "partially_settled": lifecycle.partially_settled,
                "settled": occurrence_projection.all_settled,
                "occurrence_statuses": occurrence_projection.status_counts,
                "by_resource": null,
            },
            "correction_status": correction_status(
                lifecycle.system_disputed, lifecycle.disputed,
                occurrence_projection.private_slices, occurrence_projection.compensated_slices,
            ),
            "private_compensated_slices": viewer_has_private_transfer_data.then_some(occurrence_projection.compensated_slices),
            "private_settlement_slices": viewer_has_private_transfer_data.then_some(occurrence_projection.private_slices),
            "confirmations": null,
            "occurrences": null,
            "invitations": null,
            "parties": null,
            "promises": null,
        });
        value["invitations"] = Value::Array(invitations::project(
            &invitations,
            invitation_history,
            &invitations::Viewer {
                viewer,
                viewer_person,
                show_all: recipient_details_authorized,
                creator_person: creator_person.as_deref(),
                is_creator,
                signer: installed_signer_actor,
            },
            records_by_uid,
        ));
        value["parties"] = Value::Array(parties::project(
            &parties,
            &parties::Parties {
                input: &readiness_input,
                readiness: &agreement_readiness,
                coalition: coalition_members.as_ref(),
                audience: &audience,
                signer: installed_signer_actor,
                revision_signed,
                records: records_by_uid,
            },
        ));
        value["promises"] = Value::Array(promises::project(&Promises {
            transfer: &transfer,
            promises: &promises,
            revision_promises: revision_promises.unwrap_or_default(),
            occurrences: &occurrences,
            successor_links: &promise_successors,
            readiness: &agreement_readiness,
            occurrence_roles: &occurrence_roles,
            open_claim_pairs: &open_claim_pairs_by_source,
            settlement_progress: &settlement_progress_by_occurrence,
            availability: &availability_by_record,
            records: records_by_uid,
            concept_names,
            viewer,
            signer: installed_signer_actor,
            signer_is_party: actions.signer_is_party,
            claim_person: actions.claim_person,
            claim_access_blocker: actions.claim_access_blocker,
            show_lineage: recipient_details_authorized,
        }));
        value["revision_evidence"] = revision_evidence;
        value["timeline"] = Value::Array(timeline);
        value["visibility_projection"] = visibility_projection;
        value["threads"] = Value::Array(threads);
        value["confirmations"] = Value::Array(confirmations);
        value["inbox_facets"] = lifecycle.inbox();
        actions.attach(&mut value, &agreement_options);
        occurrence_projection.attach(&mut value);
        if recipient_details_authorized
            && local_organ_uid.as_deref()
                == records_by_uid
                    .get(uid)
                    .and_then(|record| record.organ_uid.as_deref())
        {
            let social_delivery = origin_social_delivery_projection(
                store,
                uid,
                revision,
                viewer_person,
                (is_creator || is_participant)
                    && viewer_person.is_some()
                    && installed_signer_actor == viewer_person,
                visible,
            )
            .await?;
            if let Some(object) = value.as_object_mut() {
                object.insert("social_delivery".into(), social_delivery);
            }
        }
        value["children"] =
            actions::attach_children(children, &transfer, &audience, installed_signer_actor);
        disclosure.redact(&mut value);
        projection::attach_loans(store, uid, &mut value).await?;
        if value["promises"].as_array().is_some_and(|promises| {
            promises
                .iter()
                .any(|promise| promise["item"]["transport"].is_object())
        }) {
            value["journey"] = json!(transport::journey(&store.pool, uid).await?);
            value["transport_capacity"] = json!(transport::capacity(&store.pool, uid).await?);
        }
        nucleus::transfer::disclosure::project_transfer(
            &mut value,
            viewer.person.as_deref(),
            viewer.local || is_creator,
        );
        if protein
            .fields
            .as_ref()
            .is_some_and(|fields| fields.iter().any(|field| field == "karma_state"))
        {
            karma_state::append(store, &mut value).await?;
        }
        Ok(Some(TransferOutput {
            uid: transfer.record_uid,
            slug,
            head,
            revision,
            status: status.to_owned(),
            value,
        }))
    }
}
