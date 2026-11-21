// non-lean-yet
// Scope: fn unrestricted_grants.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 92, 93, 94.
// - [Clippy probe: clippy::wildcard_imports] Use explicit imports while preserving framework and
//   test conventions. Sites 1.

use super::*;

pub(super) fn unrestricted_grants(
    policy: &mut RolePolicy,
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    targets: &[MutationTarget],
    unrestricted: &BTreeSet<String>,
) {
    let _preparation = crate::performance::Span::new("access-unrestricted-grant-preparation");
    let mut operations = [
        ("record:create", Operation::Create),
        ("record:update", Operation::Update),
        ("record:update", Operation::Restore),
        ("record:delete", Operation::Delete),
    ]
    .into_iter()
    .filter(|(key, _)| unrestricted.contains(*key))
    .map(|(_, operation)| operation)
    .peekable();
    if operations.peek().is_none() {
        return;
    }
    let mut properties = BTreeSet::from([
        authority::Property::Kind,
        authority::Property::Slug,
        authority::Property::Head,
        authority::Property::Body,
        authority::Property::Quantity,
        authority::Property::Unit,
        authority::Property::Place,
        authority::Property::Organ,
    ]);
    properties.extend(
        targets
            .iter()
            .flat_map(|target| target.touched_properties.iter().cloned()),
    );
    let extension_keys: BTreeSet<_> = current
        .records
        .iter()
        .chain(&proposed.records)
        .filter_map(|record| record.content.as_ref())
        .flat_map(|content| content.extensions.keys())
        .collect();
    properties.extend(
        extension_keys
            .into_iter()
            .cloned()
            .map(authority::Property::Extension),
    );
    let assertions = current
        .concepts
        .iter()
        .chain(&proposed.concepts)
        .map(|concept| concept.uid.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .flat_map(|predicate_uid| {
            [
                authority::AssertionGrant {
                    predicate_uid: predicate_uid.into(),
                    target: authority::AssertionTarget::Unary,
                    role: authority::AssertionRole::Ordinary,
                    properties: BTreeSet::from([
                        authority::AssertionProperty::Quantity,
                        authority::AssertionProperty::Unit,
                    ]),
                },
                authority::AssertionGrant {
                    predicate_uid: predicate_uid.into(),
                    target: authority::AssertionTarget::AnyReadableRecord,
                    role: authority::AssertionRole::Ordinary,
                    properties: BTreeSet::from([
                        authority::AssertionProperty::Quantity,
                        authority::AssertionProperty::Unit,
                    ]),
                },
                authority::AssertionGrant {
                    predicate_uid: predicate_uid.into(),
                    target: authority::AssertionTarget::Unary,
                    role: authority::AssertionRole::Identity,
                    properties: BTreeSet::new(),
                },
            ]
        })
        .collect::<Vec<_>>();
    for operation in operations {
        policy.grants.push(authority::MutationGrant {
            operation,
            selector: Predicate::All(vec![]),
            properties: properties.clone(),
            assertions_add: assertions.clone(),
            assertions_remove: assertions.clone(),
        });
        crate::performance::add("access-unrestricted-grants-built", 1);
    }
}
