use super::contacts::{
    Audience, CheckedDisclosure, Document, RelationKind, Statement, permitted_edges,
};
use super::paths::{Policy, search};
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::collections::BTreeMap;

fn identity(seed: u128) -> Identity {
    Identity::Organ(format!("r_{}", crate::ulid_from(1, seed)))
}

fn context() -> IdentityContext {
    IdentityContext {
        readable: true,
        known: true,
        ..Default::default()
    }
}

fn disclosure(issuer: &Identity, contact: &Identity) -> CheckedDisclosure {
    CheckedDisclosure {
        document: Document {
            statement: Statement {
                issuer: issuer.clone(),
                contact: contact.clone(),
                revision: 1,
                issued_at: 100,
                expires_at: 1000,
                withdrawn: false,
                audience: Audience::Public,
            },
            key_id: "personally-compared-or-organ-root".into(),
            public_key: STANDARD.encode([0; 32]),
            signature: STANDARD.encode([0; 64]),
        },
        readable: true,
        authority_current: true,
    }
}

fn mutual(left: &Identity, right: &Identity) -> Vec<CheckedDisclosure> {
    vec![disclosure(left, right), disclosure(right, left)]
}

#[test]
fn contact_documents_bind_identity_kind_audience_revision_and_signing_purpose() {
    let a = identity(1);
    let b = identity(2);
    let document = disclosure(&a, &b).document;
    let bytes = document.signing_bytes().unwrap();
    let encoded = serde_json::to_vec(&document).unwrap();
    assert_eq!(Document::parse(&encoded).unwrap(), document);
    let mut changed = document.clone();
    changed.key_id = "another-key".into();
    assert_ne!(bytes, changed.signing_bytes().unwrap());
    changed = document.clone();
    changed.statement.audience = Audience::default();
    assert_ne!(bytes, changed.signing_bytes().unwrap());
    changed = document.clone();
    changed.statement.revision = 2;
    assert_ne!(bytes, changed.signing_bytes().unwrap());
    changed = document.clone();
    changed.statement.issuer = Identity::Person(a.uid().into());
    assert_ne!(bytes, changed.signing_bytes().unwrap());
    assert_ne!(
        document.statement.relation().unwrap(),
        changed.statement.relation().unwrap()
    );
    assert_eq!(
        document.statement.relation().unwrap(),
        disclosure(&b, &a).document.statement.relation().unwrap()
    );
    let mut value = serde_json::to_value(&document).unwrap();
    value["instruction"] = serde_json::json!({"action":"settle-transfer"});
    assert!(Document::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(Document::parse(&vec![b' '; contacts::MAX_DOCUMENT_BYTES + 1]).is_err());
    changed.signature = "x".repeat(88);
    assert!(changed.validate().is_err());
    changed.statement.contact = Identity::Organ(a.uid().into());
    assert!(changed.statement.validate().is_err());
}

#[test]
fn default_audience_is_private_and_person_and_organ_grants_are_distinct() {
    let person = Identity::Person(identity(1).uid().into());
    let organ = Identity::Organ(person.uid().into());
    let b = identity(2);
    let private = Audience::default();
    assert!(private.allows(&person, &person));
    assert!(!private.allows(&organ, &person));
    assert!(!private.allows(&b, &person));
    let selected = Audience::Selected {
        viewers: vec![person.clone()],
    };
    assert!(selected.allows(&person, &b));
    assert!(!selected.allows(&organ, &b));
    let repeated = Audience::Selected {
        viewers: vec![person.clone(), person],
    };
    assert!(repeated.validate().is_err());
}

#[test]
fn current_withdrawn_expired_hidden_or_revoked_proof_cannot_fall_back_to_an_older_revision() {
    let a = identity(1);
    let b = identity(2);
    let contexts = BTreeMap::from([(a.clone(), context()), (b.clone(), context())]);
    for reason in ["withdrawn", "expired", "hidden", "revoked", "restricted"] {
        let mut proofs = mutual(&a, &b);
        let mut current = proofs[0].clone();
        current.document.statement.revision = 2;
        match reason {
            "withdrawn" => current.document.statement.withdrawn = true,
            "expired" => current.document.statement.expires_at = 150,
            "hidden" => current.readable = false,
            "revoked" => current.authority_current = false,
            "restricted" => current.document.statement.audience = Audience::default(),
            _ => unreachable!(),
        }
        proofs.push(current);
        let edges = permitted_edges(&proofs, &contexts, &b, 200).unwrap();
        assert!(
            edges.iter().all(|edge| edge.kind != RelationKind::Mutual),
            "{reason}"
        );
        let found = search(&edges, &contexts, &Policy::for_anchor(a.clone())).unwrap();
        assert!(found.paths.is_empty(), "{reason}");
    }
}

#[test]
fn same_revision_conflicts_withhold_the_relation_and_replicas_add_no_paths() {
    let a = identity(1);
    let b = identity(2);
    let contexts = BTreeMap::from([(a.clone(), context()), (b.clone(), context())]);
    let mut proofs = mutual(&a, &b);
    proofs.extend(proofs.clone());
    let edges = permitted_edges(&proofs, &contexts, &a, 200).unwrap();
    assert_eq!(edges.len(), 2);
    let found = search(&edges, &contexts, &Policy::for_anchor(a.clone())).unwrap();
    assert_eq!(found.paths.len(), 1);
    let mut conflicting = proofs[0].clone();
    conflicting.document.statement.expires_at -= 1;
    proofs.push(conflicting);
    assert!(
        permitted_edges(&proofs, &contexts, &a, 200)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn four_identity_diamond_and_loop_have_one_deterministic_shortest_observed_path() {
    let [a, b, c, d] = [identity(1), identity(2), identity(3), identity(4)];
    let contexts = [a.clone(), b.clone(), c.clone(), d.clone()]
        .into_iter()
        .map(|identity| (identity, context()))
        .collect();
    let mut proofs = Vec::new();
    for (left, right) in [(&a, &c), (&a, &b), (&b, &d), (&c, &d), (&b, &c)] {
        proofs.extend(mutual(left, right));
    }
    let mut edges = permitted_edges(&proofs, &contexts, &a, 200).unwrap();
    let policy = Policy::for_anchor(a.clone());
    let first = search(&edges, &contexts, &policy).unwrap();
    edges.reverse();
    let second = search(&edges, &contexts, &policy).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.paths.len(), 3);
    let path = first
        .paths
        .iter()
        .find(|path| path.identities.last() == Some(&d))
        .unwrap();
    assert_eq!(path.identities, vec![a, b, d]);
    assert_eq!(path.distance(), 2);
    assert!(!path.unilateral);
    assert!(path.proofs.iter().all(|proofs| proofs.len() == 2));
    assert!(first.observed_only);
    assert!(!first.identity_limit_reached);
    assert!(!first.depth_limit_reached);
}

#[test]
fn unilateral_contacts_require_opt_in_and_remain_directional_and_labelled() {
    let a = identity(1);
    let b = identity(2);
    let contexts = BTreeMap::from([(a.clone(), context()), (b.clone(), context())]);
    let edges = permitted_edges(&[disclosure(&a, &b)], &contexts, &a, 200).unwrap();
    let mut policy = Policy::for_anchor(a.clone());
    assert!(search(&edges, &contexts, &policy).unwrap().paths.is_empty());
    policy.allow_unilateral = true;
    let found = search(&edges, &contexts, &policy).unwrap();
    assert_eq!(found.paths.len(), 1);
    assert!(found.paths[0].unilateral);
    policy.anchor = b;
    assert!(search(&edges, &contexts, &policy).unwrap().paths.is_empty());
}

#[test]
fn identity_and_depth_budgets_report_partial_observed_paths_without_route_multiplication() {
    let [a, b, c, d] = [identity(1), identity(2), identity(3), identity(4)];
    let contexts = [a.clone(), b.clone(), c.clone(), d.clone()]
        .into_iter()
        .map(|identity| (identity, context()))
        .collect();
    let proofs = [mutual(&a, &b), mutual(&b, &c), mutual(&c, &d)].concat();
    let edges = permitted_edges(&proofs, &contexts, &a, 200).unwrap();
    let mut policy = Policy::for_anchor(a);
    policy.max_depth = 1;
    let found = search(&edges, &contexts, &policy).unwrap();
    assert_eq!(found.paths.len(), 1);
    assert!(found.depth_limit_reached);
    policy.max_depth = 3;
    policy.max_identities = 2;
    let found = search(&edges, &contexts, &policy).unwrap();
    assert_eq!(found.paths.len(), 1);
    assert!(found.identity_limit_reached);
    policy.max_depth = 7;
    assert!(policy.validate().is_err());
    policy.max_depth = 3;
    policy.max_identities = 1025;
    assert!(policy.validate().is_err());
}

#[test]
fn target_filters_do_not_imply_intermediary_permission_and_exclusions_win() {
    let [a, b, c, d] = [identity(1), identity(2), identity(3), identity(4)];
    let mut contexts: BTreeMap<_, _> = [a.clone(), b.clone(), c.clone(), d.clone()]
        .into_iter()
        .map(|identity| (identity, context()))
        .collect();
    contexts.get_mut(&b).unwrap().proximity = Some(2);
    let proofs = [mutual(&a, &b), mutual(&b, &c), mutual(&c, &d)].concat();
    let edges = permitted_edges(&proofs, &contexts, &a, 200).unwrap();
    let mut policy = Policy::for_anchor(a);
    policy.targets.include = vec![Condition {
        identities: vec![d.clone()],
        ..Default::default()
    }];
    let found = search(&edges, &contexts, &policy).unwrap();
    assert_eq!(found.paths.len(), 1);
    assert_eq!(found.paths[0].identities.last(), Some(&d));
    policy.intermediaries.exclude = vec![Condition {
        identities: vec![b.clone()],
        known: Some(true),
        lower: Some(Bound {
            value: 1,
            inclusive: false,
        }),
        upper: Some(Bound {
            value: 2,
            inclusive: true,
        }),
    }];
    policy.intermediaries.include = vec![Condition::default()];
    assert!(search(&edges, &contexts, &policy).unwrap().paths.is_empty());
    policy.intermediaries.exclude[0].known = Some(false);
    assert_eq!(search(&edges, &contexts, &policy).unwrap().paths.len(), 1);
    policy.exclude_everywhere = vec![b];
    assert!(search(&edges, &contexts, &policy).unwrap().paths.is_empty());
}

#[test]
fn hidden_or_blocked_endpoint_never_appears_in_paths_or_affects_visible_budget() {
    let [a, b, c, d] = [identity(1), identity(2), identity(3), identity(4)];
    let mut contexts: BTreeMap<_, _> = [a.clone(), b.clone(), c.clone(), d.clone()]
        .into_iter()
        .map(|identity| (identity, context()))
        .collect();
    contexts.get_mut(&b).unwrap().readable = false;
    let public = mutual(&a, &c);
    let with_hidden = [public.clone(), mutual(&a, &b), mutual(&b, &d)].concat();
    let mut policy = Policy::for_anchor(a.clone());
    policy.max_identities = 2;
    let baseline = search(
        &permitted_edges(&public, &contexts, &a, 200).unwrap(),
        &contexts,
        &policy,
    )
    .unwrap();
    let actual = search(
        &permitted_edges(&with_hidden, &contexts, &a, 200).unwrap(),
        &contexts,
        &policy,
    )
    .unwrap();
    assert_eq!(baseline, actual);
    assert_eq!(actual.paths[0].identities, vec![a.clone(), c]);
    contexts.get_mut(&b).unwrap().readable = true;
    contexts.get_mut(&b).unwrap().blocked = true;
    assert_eq!(
        baseline,
        search(
            &permitted_edges(&with_hidden, &contexts, &a, 200).unwrap(),
            &contexts,
            &policy
        )
        .unwrap()
    );
}

#[test]
fn contact_processing_enforces_work_and_aggregate_byte_limits() {
    let a = identity(1);
    let b = identity(2);
    let contexts = BTreeMap::from([(a.clone(), context()), (b.clone(), context())]);
    let proof = disclosure(&a, &b);
    assert!(
        permitted_edges(
            &vec![proof.clone(); MAX_DISCLOSURES + 1],
            &contexts,
            &a,
            200
        )
        .is_err()
    );
    let mut large = proof;
    large.document.statement.audience = Audience::Selected {
        viewers: (10..74).map(identity).collect(),
    };
    let bytes = serde_json::to_vec(&large.document).unwrap().len();
    let count = MAX_BYTES / bytes + 1;
    assert!(count <= MAX_DISCLOSURES);
    assert!(
        permitted_edges(&vec![large; count], &contexts, &a, 200)
            .unwrap_err()
            .contains("8 MiB")
    );
}
