use super::*;
use std::borrow::Cow;

pub(super) enum Content<'a> {
    Borrowed(&'a BTreeMap<String, TargetRecordContent>),
    Owned(BTreeMap<String, TargetRecordContent>),
}

impl Content<'_> {
    fn take(&mut self, uid: &str) -> Option<Cow<'_, TargetRecordContent>> {
        match self {
            Self::Borrowed(content) => content.get(uid).map(Cow::Borrowed),
            Self::Owned(content) => content.remove(uid).map(Cow::Owned),
        }
    }
}

fn project_content(body: Cow<'_, TargetRecordContent>) -> RecordContent {
    match body {
        Cow::Borrowed(body) => RecordContent {
            slug: body.slug.clone(),
            head: body.head.clone(),
            body: body.body.clone(),
            quantity: body.quantity,
            unit_uid: body.unit_uid.clone(),
            place_uid: body.place_uid.clone(),
            extensions: body
                .extensions
                .iter()
                .flat_map(|(namespace, extension)| {
                    extension.fields.iter().map(move |(field, value)| {
                        (
                            ExtensionProperty {
                                namespace: namespace.clone(),
                                field: field.clone(),
                            },
                            value.clone(),
                        )
                    })
                })
                .collect(),
        },
        Cow::Owned(body) => RecordContent {
            slug: body.slug,
            head: body.head,
            body: body.body,
            quantity: body.quantity,
            unit_uid: body.unit_uid,
            place_uid: body.place_uid,
            extensions: body
                .extensions
                .into_iter()
                .flat_map(|(namespace, extension)| {
                    extension.fields.into_iter().map(move |(field, value)| {
                        (
                            ExtensionProperty {
                                namespace: namespace.clone(),
                                field,
                            },
                            value,
                        )
                    })
                })
                .collect(),
        },
    }
}

pub(super) fn project(
    metadata: &AccessMetadataSnapshot,
    mut content: Content<'_>,
) -> Result<GraphSnapshot, AccessError> {
    let _projection = crate::performance::Span::new("access-graph-projection");
    let mut moved = 0;
    let mut parents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for parent in &metadata.concept_parents {
        parents
            .entry(parent.concept_uid.clone())
            .or_default()
            .insert(parent.parent_uid.clone());
    }
    let records = metadata
        .records
        .iter()
        .map(|record| {
            let body = content.take(&record.uid);
            if body.as_ref().is_some_and(|body| {
                body.uid != record.uid
                    || body.kind != record.kind
                    || body.organ_uid != record.organ_uid
                    || body.deleted != record.deleted
                    || body.replica_root != record.replica_root
            }) {
                return Err(AccessError::InvalidMutation);
            }
            Ok(RecordState {
                uid: record.uid.clone(),
                kind: record.kind,
                organ_uid: record.organ_uid.clone(),
                deleted: record.deleted,
                content: body.map(|body| {
                    if matches!(&body, Cow::Owned(_)) {
                        moved += 1;
                    }
                    project_content(body)
                }),
            })
        })
        .collect::<Result<_, AccessError>>()?;
    crate::performance::add("access-record-content-moved", moved);
    Ok(GraphSnapshot {
        records,
        concepts: metadata
            .concepts
            .iter()
            .map(|concept| ConceptState {
                uid: concept.uid.clone(),
                name: concept.name.clone(),
                parents: parents.remove(&concept.uid).unwrap_or_default(),
            })
            .collect(),
        assertions: metadata
            .assertions
            .iter()
            .map(|assertion| AssertionState {
                uid: assertion.uid.clone(),
                subject_uid: assertion.subject_uid.clone(),
                predicate_uid: assertion.predicate_uid.clone(),
                object_uid: assertion.object_uid.clone(),
                object_uri: assertion.object_uri.clone(),
                role: match assertion.role {
                    access_snapshot::AssertionRole::Ordinary => authority::AssertionRole::Ordinary,
                    access_snapshot::AssertionRole::Identity => authority::AssertionRole::Identity,
                },
                quantity: assertion.quantity,
                unit_uid: assertion.unit_uid.clone(),
            })
            .collect(),
        places: metadata.places.clone(),
        resolved_links: metadata.resolved_links.clone(),
    })
}
