use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use nucleus::RecordKind;
use protein::Predicate;
use protein::authority::{
    self, AssertionState, AuthorityError, ConceptState, ExtensionProperty, GraphSnapshot,
    MutationTarget, Operation, RecordContent, RecordDecision, RecordState, RolePolicy,
    VisibilityCeiling,
};
use store::access_snapshot::{
    self, AccessMetadataSnapshot, AccessSnapshotLimits, LinguaVisibility, TargetRecordContent,
    VisibilityGrant, VisibilitySubject, VisibilityTarget,
};
use store::session_access::{self, DeviceAdmission};
use store::sqlx::{Sqlite, Transaction};

#[derive(Debug, Clone)]
pub struct AccessLimits {
    pub snapshot: AccessSnapshotLimits,
    pub authority: authority::Limits,
}

impl Default for AccessLimits {
    fn default() -> Self {
        Self {
            snapshot: Default::default(),
            authority: authority::Limits {
                grants: 8192,
                predicate_nodes: 65536,
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessError {
    InvalidSession,
    InvalidScope,
    MissingAuthority,
    InvalidAuthority,
    AuthorityChanged,
    InvalidMutation,
    Storage,
    Policy(AuthorityError),
}

impl fmt::Display for AccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSession => "access session is unavailable",
            Self::InvalidScope => "access scope is unavailable",
            Self::MissingAuthority => "access authority is missing",
            Self::InvalidAuthority => "access authority is invalid",
            Self::AuthorityChanged => "access authority changed during the operation",
            Self::InvalidMutation => "access mutation is invalid or incomplete",
            Self::Storage => "access storage is unavailable",
            Self::Policy(error) => return fmt::Display::fmt(error, formatter),
        })
    }
}

impl std::error::Error for AccessError {}

impl From<store::StoreError> for AccessError {
    fn from(_: store::StoreError) -> Self {
        Self::Storage
    }
}

impl From<AuthorityError> for AccessError {
    fn from(error: AuthorityError) -> Self {
        Self::Policy(error)
    }
}

#[derive(Clone, PartialEq, Eq, Default)]
pub struct ReadableSets {
    pub records: BTreeSet<String>,
    pub concepts: BTreeSet<String>,
    pub places: BTreeSet<String>,
    pub assertions: BTreeSet<String>,
    pub linguas: BTreeSet<String>,
    pub concept_parents: BTreeSet<(String, String)>,
    pub lingua_concepts: BTreeSet<(String, String)>,
}

pub struct BatchDecision {
    pub records: BTreeMap<String, RecordDecision>,
    pub readable: ReadableSets,
    target_content: BTreeMap<String, TargetRecordContent>,
    revisions: BTreeMap<String, i64>,
}

impl BatchDecision {
    pub fn target_content(&self, uid: &str) -> Option<&TargetRecordContent> {
        self.target_content.get(uid)
    }

    pub fn revision(&self, uid: &str) -> Option<i64> {
        self.revisions.get(uid).copied()
    }
}

struct AuthoritySource {
    person: store::auth::PersonAccess,
    policies: Vec<store::role_policies::RolePolicyRow>,
    permissions: BTreeSet<String>,
    unrestricted: BTreeSet<String>,
}

struct Snapshot {
    metadata: AccessMetadataSnapshot,
    content: BTreeMap<String, TargetRecordContent>,
    graph: GraphSnapshot,
    ceiling: VisibilityCeiling,
    readable: ReadableSets,
    revisions: BTreeMap<String, i64>,
}

pub struct Access<'operation, 'database> {
    transaction: &'operation mut Transaction<'database, Sqlite>,
    admission: DeviceAdmission,
    hosted_organ: String,
    targets: BTreeSet<String>,
    source: AuthoritySource,
    policy: RolePolicy,
    current: Snapshot,
    limits: AccessLimits,
}

struct DependencyCatalog {
    policies: Vec<store::role_policies::RolePolicyRow>,
    people: Vec<store::auth::RetainedPersonAccess>,
    selectors: Vec<Predicate>,
}

pub struct PreparedChanges<'operation, 'database> {
    access: Access<'operation, 'database>,
    dependencies: DependencyCatalog,
}

async fn dependency_catalog_on(
    transaction: &mut Transaction<'_, Sqlite>,
) -> Result<DependencyCatalog, AccessError> {
    let policies = store::role_policies::all_on(&mut **transaction).await?;
    let people = store::auth::retained_person_access_on(transaction).await?;
    let mut selectors = Vec::new();
    for row in &policies {
        let Some(policy) = &row.policy else {
            continue;
        };
        let policy: RolePolicy =
            serde_json::from_value(policy.clone()).map_err(|_| AccessError::InvalidAuthority)?;
        selectors.push(policy.read);
        selectors.extend(policy.grants.into_iter().map(|grant| grant.selector));
    }
    for person in &people {
        if let Some(filter) = &person.read_filter {
            selectors
                .push(serde_json::from_str(filter).map_err(|_| AccessError::InvalidAuthority)?);
        }
    }
    Ok(DependencyCatalog {
        policies,
        people,
        selectors,
    })
}

async fn authority_on(
    transaction: &mut Transaction<'_, Sqlite>,
    admission: &DeviceAdmission,
) -> Result<AuthoritySource, AccessError> {
    session_access::require_admission_on(transaction, admission)
        .await
        .map_err(|_| AccessError::InvalidSession)?;
    let person =
        store::auth::person_access_on(transaction, admission.authentication().person_uid())
            .await?
            .ok_or(AccessError::MissingAuthority)?;
    let mut policies = Vec::new();
    let mut permissions = BTreeSet::new();
    let mut unrestricted = BTreeSet::new();
    for role in store::person_roles::ids_on(transaction, &person.person_uid).await? {
        let keys = store::auth::role_permission_keys_by_id_on(transaction, role).await?;
        let row = store::role_policies::get_on(transaction, role).await?;
        if row.as_ref().is_none_or(|row| row.policy.is_none()) {
            unrestricted.extend(keys.iter().cloned());
        }
        permissions.extend(keys);
        if let Some(row) = row {
            policies.push(row);
        }
    }
    if !permissions.contains("record:read") {
        return Err(AuthorityError::Denied.into());
    }
    Ok(AuthoritySource {
        person,
        policies,
        permissions,
        unrestricted,
    })
}

pub async fn load_on<'operation, 'database>(
    transaction: &'operation mut Transaction<'database, Sqlite>,
    admission: &DeviceAdmission,
    hosted_organ: &str,
    target_uids: &[String],
    limits: AccessLimits,
) -> Result<Access<'operation, 'database>, AccessError> {
    if !nucleus::valid_uid(hosted_organ, "r") {
        return Err(AccessError::InvalidScope);
    }
    if target_uids.len() > limits.snapshot.targets {
        return Err(AuthorityError::LimitExceeded.into());
    }
    if target_uids.iter().any(|uid| !nucleus::valid_uid(uid, "r")) {
        return Err(AccessError::InvalidMutation);
    }
    let targets: BTreeSet<String> = target_uids.iter().cloned().collect();
    if targets.len() != target_uids.len() || targets.iter().any(|uid| !nucleus::valid_uid(uid, "r"))
    {
        return Err(AccessError::InvalidMutation);
    }
    let source = authority_on(transaction, admission).await?;
    let mut policy = RolePolicy {
        read: Predicate::Any(vec![]),
        grants: Vec::new(),
    };
    let mut reads = Vec::new();
    for row in &source.policies {
        let permissions =
            store::auth::role_permission_keys_by_id_on(transaction, row.role_id).await?;
        if let Some(raw) = &row.policy {
            let role: RolePolicy =
                serde_json::from_value(raw.clone()).map_err(|_| AccessError::InvalidAuthority)?;
            if permissions.iter().any(|key| key == "record:read") {
                reads.push(role.read);
            }
            policy
                .grants
                .extend(role.grants.into_iter().filter(|grant| {
                    permissions.iter().any(|key| {
                        key == match grant.operation {
                            Operation::Create => "record:create",
                            Operation::Update | Operation::Restore => "record:update",
                            Operation::Delete => "record:delete",
                        }
                    })
                }));
        }
    }
    policy.read = if source.unrestricted.contains("record:read") {
        Predicate::All(vec![])
    } else {
        Predicate::Any(reads)
    };
    if let Some(raw) = &source.person.read_filter {
        let filter: Predicate =
            serde_json::from_str(raw).map_err(|_| AccessError::InvalidAuthority)?;
        policy.read = Predicate::All(vec![policy.read, filter]);
    }
    let roles = store::person_roles::ids_on(transaction, &source.person.person_uid).await?;
    let current = snapshot_on(
        transaction,
        admission,
        hosted_organ,
        &roles,
        &policy,
        &targets,
        &limits,
    )
    .await?;
    Ok(Access {
        transaction,
        admission: admission.clone(),
        hosted_organ: hosted_organ.into(),
        targets,
        source,
        policy,
        current,
        limits,
    })
}

pub async fn prepare_changes_on<'operation, 'database>(
    transaction: &'operation mut Transaction<'database, Sqlite>,
    admission: &DeviceAdmission,
    hosted_organ: &str,
    target_uids: &[String],
    limits: AccessLimits,
) -> Result<PreparedChanges<'operation, 'database>, AccessError> {
    let access = load_on(transaction, admission, hosted_organ, target_uids, limits).await?;
    if access.targets.is_empty() {
        return Err(AccessError::InvalidMutation);
    }
    let dependencies = dependency_catalog_on(access.transaction).await?;
    Ok(PreparedChanges {
        access,
        dependencies,
    })
}

impl<'operation, 'database> Access<'operation, 'database> {
    pub fn person_uid(&self) -> &str {
        self.admission.authentication().person_uid()
    }

    pub fn hosted_organ(&self) -> &str {
        &self.hosted_organ
    }

    pub fn peer_organ(&self) -> Option<&str> {
        self.admission
            .peer_contact()
            .map(|peer| peer.organ_uid.as_str())
    }

    pub fn readable(&self) -> &ReadableSets {
        &self.current.readable
    }

    pub fn readable_target(&self, uid: &str) -> Option<&TargetRecordContent> {
        (self.targets.contains(uid) && self.current.readable.records.contains(uid))
            .then(|| self.current.content.get(uid))
            .flatten()
    }

    pub fn permits(&self, permission: &str) -> bool {
        self.source.permissions.contains(permission)
    }

    pub fn require_permission(&self, permission: &str) -> Result<(), AccessError> {
        if self.permits(permission) {
            Ok(())
        } else {
            Err(AuthorityError::Denied.into())
        }
    }

    async fn finish_changes(
        self,
        targets: &[MutationTarget],
        dependencies: DependencyCatalog,
        assertion_intents: Option<&[authority::AssertionIntent]>,
    ) -> Result<BatchDecision, AccessError> {
        if targets.len() != self.targets.len()
            || targets
                .iter()
                .any(|target| !self.targets.contains(&target.record_uid))
        {
            return Err(AccessError::InvalidMutation);
        }
        let target_set: BTreeSet<String> = targets
            .iter()
            .map(|target| target.record_uid.clone())
            .collect();
        if target_set.is_empty() || targets.len() != target_set.len() || target_set != self.targets
        {
            return Err(AccessError::InvalidMutation);
        }
        let source = authority_on(self.transaction, &self.admission).await?;
        if source.person != self.source.person
            || source.policies != self.source.policies
            || source.permissions != self.source.permissions
            || source.unrestricted != self.source.unrestricted
        {
            return Err(AccessError::AuthorityChanged);
        }
        let current_dependencies = dependency_catalog_on(self.transaction).await?;
        if current_dependencies.policies != dependencies.policies
            || current_dependencies.people != dependencies.people
        {
            return Err(AccessError::AuthorityChanged);
        }
        let roles =
            store::person_roles::ids_on(self.transaction, &source.person.person_uid).await?;
        let proposed = snapshot_on(
            self.transaction,
            &self.admission,
            &self.hosted_organ,
            &roles,
            &self.policy,
            &self.targets,
            &self.limits,
        )
        .await?;
        validate_staged(
            &self.current,
            &proposed,
            targets,
            assertion_intents,
            &self.hosted_organ,
        )?;
        let mut policy = self.policy.clone();
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
        for record in self
            .current
            .graph
            .records
            .iter()
            .chain(&proposed.graph.records)
        {
            if let Some(content) = &record.content {
                properties.extend(
                    content
                        .extensions
                        .keys()
                        .cloned()
                        .map(authority::Property::Extension),
                );
            }
        }
        let assertions = self
            .current
            .graph
            .concepts
            .iter()
            .chain(&proposed.graph.concepts)
            .map(|concept| concept.uid.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .flat_map(|predicate_uid| {
                vec![
                    authority::AssertionGrant {
                        predicate_uid: predicate_uid.clone(),
                        target: authority::AssertionTarget::Unary,
                        role: authority::AssertionRole::Ordinary,
                        properties: BTreeSet::from([
                            authority::AssertionProperty::Quantity,
                            authority::AssertionProperty::Unit,
                        ]),
                    },
                    authority::AssertionGrant {
                        predicate_uid: predicate_uid.clone(),
                        target: authority::AssertionTarget::AnyReadableRecord,
                        role: authority::AssertionRole::Ordinary,
                        properties: BTreeSet::from([
                            authority::AssertionProperty::Quantity,
                            authority::AssertionProperty::Unit,
                        ]),
                    },
                    authority::AssertionGrant {
                        predicate_uid,
                        target: authority::AssertionTarget::Unary,
                        role: authority::AssertionRole::Identity,
                        properties: BTreeSet::new(),
                    },
                ]
            })
            .collect::<Vec<_>>();
        for (key, operation) in [
            ("record:create", Operation::Create),
            ("record:update", Operation::Update),
            ("record:update", Operation::Restore),
            ("record:delete", Operation::Delete),
        ] {
            if source.unrestricted.contains(key) {
                policy.grants.push(authority::MutationGrant {
                    operation,
                    selector: Predicate::All(vec![]),
                    properties: properties.clone(),
                    assertions_add: assertions.clone(),
                    assertions_remove: assertions.clone(),
                });
            }
        }
        let records = match assertion_intents {
            Some(intents) => authority::authorize_record_changes_with_assertion_intents(
                Some(&policy),
                &self.current.graph,
                &proposed.graph,
                &self.current.ceiling,
                &proposed.ceiling,
                targets,
                intents,
                &self.limits.authority,
            ),
            None => authority::authorize_record_changes(
                Some(&policy),
                &self.current.graph,
                &proposed.graph,
                &self.current.ceiling,
                &proposed.ceiling,
                targets,
                &self.limits.authority,
            ),
        }?;
        for decision in records.values() {
            let permission = match decision.operation {
                Operation::Create => "record:create",
                Operation::Update | Operation::Restore => "record:update",
                Operation::Delete => "record:delete",
            };
            if !self.source.permissions.contains(permission) {
                return Err(AuthorityError::Denied.into());
            }
        }
        let changed_dependencies = authority::selector_membership_changes(
            &dependencies.selectors,
            &self.current.graph,
            &proposed.graph,
            &self.limits.authority,
        )?;
        let semantic_writes = semantic_write_set(&self.current.graph, &proposed.graph, &records)?;
        if changed_dependencies
            .iter()
            .any(|uid| !semantic_writes.contains(uid))
            && !source.permissions.contains("permission:assign")
        {
            return Err(AuthorityError::Denied.into());
        }
        for draft in &proposed.metadata.message_drafts {
            let Some(decision) = records.get(&draft.record_uid) else {
                continue;
            };
            for field in ["author", "operator", "thread", "conversation"] {
                let property = authority::Property::Extension(ExtensionProperty {
                    namespace: "lince.message-draft".into(),
                    field: field.into(),
                });
                if decision.properties.contains(&property) {
                    match draft.metadata.get(field) {
                        None | Some(serde_json::Value::Null) => {}
                        Some(serde_json::Value::String(uid))
                            if proposed.readable.records.contains(uid) => {}
                        _ => return Err(AuthorityError::Denied.into()),
                    }
                }
            }
        }
        let revisions = proposed
            .revisions
            .into_iter()
            .filter(|(uid, _)| self.targets.contains(uid))
            .collect();
        Ok(BatchDecision {
            records,
            readable: proposed.readable,
            target_content: proposed
                .content
                .into_iter()
                .filter(|(uid, _)| self.targets.contains(uid))
                .collect(),
            revisions,
        })
    }
}

impl<'operation, 'database> PreparedChanges<'operation, 'database> {
    pub fn transaction_for_staging(&mut self) -> &mut Transaction<'database, Sqlite> {
        self.access.transaction
    }

    pub fn readable_target(&self, uid: &str) -> Option<&TargetRecordContent> {
        self.access.readable_target(uid)
    }

    pub fn current_target(&self, uid: &str) -> Option<&TargetRecordContent> {
        self.access
            .targets
            .contains(uid)
            .then(|| self.access.current.content.get(uid))
            .flatten()
    }

    pub fn current_assertion(&self, uid: &str) -> Option<&access_snapshot::AssertionMetadata> {
        self.access
            .current
            .metadata
            .assertions
            .iter()
            .find(|assertion| {
                assertion.uid == uid && self.access.targets.contains(&assertion.subject_uid)
            })
    }

    pub async fn finish_changes(
        self,
        targets: &[MutationTarget],
    ) -> Result<BatchDecision, AccessError> {
        self.access
            .finish_changes(targets, self.dependencies, None)
            .await
    }

    pub async fn finish_changes_with_assertion_intents(
        self,
        targets: &[MutationTarget],
        intents: &[authority::AssertionIntent],
    ) -> Result<BatchDecision, AccessError> {
        self.access
            .finish_changes(targets, self.dependencies, Some(intents))
            .await
    }
}

fn semantic_write_set(
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    decisions: &BTreeMap<String, RecordDecision>,
) -> Result<BTreeSet<String>, AccessError> {
    let current_records: BTreeMap<&str, _> = current
        .records
        .iter()
        .map(|record| (record.uid.as_str(), record))
        .collect();
    let proposed_records: BTreeMap<&str, _> = proposed
        .records
        .iter()
        .map(|record| (record.uid.as_str(), record))
        .collect();
    let mut semantic_writes = BTreeSet::new();
    for (uid, decision) in decisions {
        let semantic = match decision.operation {
            Operation::Create | Operation::Delete | Operation::Restore => true,
            Operation::Update => {
                let before = current_records
                    .get(uid.as_str())
                    .ok_or(AccessError::InvalidMutation)?;
                let after = proposed_records
                    .get(uid.as_str())
                    .ok_or(AccessError::InvalidMutation)?;
                *before != *after
                    || !decision.assertions_added.is_empty()
                    || !decision.assertions_removed.is_empty()
            }
        };
        if semantic {
            semantic_writes.insert(uid.clone());
        }
    }
    Ok(semantic_writes)
}

async fn snapshot_on(
    transaction: &mut Transaction<'_, Sqlite>,
    admission: &DeviceAdmission,
    hosted_organ: &str,
    roles: &[i64],
    policy: &RolePolicy,
    targets: &BTreeSet<String>,
    limits: &AccessLimits,
) -> Result<Snapshot, AccessError> {
    let metadata = access_snapshot::metadata_on(transaction, &limits.snapshot).await?;
    if !metadata.records.iter().any(|record| {
        record.uid == hosted_organ && record.kind == RecordKind::Organ && !record.deleted
    }) {
        return Err(AccessError::InvalidScope);
    }
    let existing_targets: Vec<String> = metadata
        .records
        .iter()
        .map(|record| record.uid.clone())
        .collect();
    let content_limits = AccessSnapshotLimits {
        targets: limits.authority.records,
        ..limits.snapshot.clone()
    };
    let content =
        access_snapshot::content_on(transaction, &existing_targets, &content_limits).await?;
    if content.len() != existing_targets.len() {
        return Err(AccessError::InvalidMutation);
    }
    let graph = graph(&metadata, &content)?;
    let (ceiling, linguas) = ceilings(&metadata, admission, hosted_organ, roles);
    let records = authority::readable_records(Some(policy), &graph, &ceiling, &limits.authority)?;
    let readable = readable_sets(&metadata, &ceiling, records, linguas);
    let mut revisions = BTreeMap::new();
    if !targets.is_empty() {
        for record in &metadata.records {
            let revision = store::record_revisions::get_on(transaction, &record.uid).await?;
            revisions.insert(revision.record_uid, revision.revision);
        }
    }
    Ok(Snapshot {
        metadata,
        content,
        graph,
        ceiling,
        readable,
        revisions,
    })
}

pub(crate) async fn policy_graph_on(
    transaction: &mut Transaction<'_, Sqlite>,
    targets: &[String],
) -> Result<GraphSnapshot, AccessError> {
    let limits = AccessSnapshotLimits {
        targets: 4096,
        ..Default::default()
    };
    let metadata = access_snapshot::metadata_on(transaction, &limits).await?;
    if targets.len() > 256 {
        return Err(AccessError::InvalidMutation);
    }
    let records = metadata
        .records
        .iter()
        .map(|record| record.uid.clone())
        .collect::<Vec<_>>();
    let content = access_snapshot::content_on(transaction, &records, &limits).await?;
    graph(&metadata, &content)
}

fn graph(
    metadata: &AccessMetadataSnapshot,
    content: &BTreeMap<String, TargetRecordContent>,
) -> Result<GraphSnapshot, AccessError> {
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
            let body = content.get(&record.uid);
            if body.is_some_and(|body| {
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
                content: body.map(|body| RecordContent {
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
                }),
            })
        })
        .collect::<Result<_, AccessError>>()?;
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
                role: match assertion.role {
                    access_snapshot::AssertionRole::Ordinary => authority::AssertionRole::Ordinary,
                    access_snapshot::AssertionRole::Identity => authority::AssertionRole::Identity,
                },
                quantity: assertion.quantity,
                unit_uid: assertion.unit_uid.clone(),
            })
            .collect(),
        places: metadata.places.clone(),
    })
}

#[derive(Default)]
struct Restriction {
    positive: bool,
    matched: bool,
    denied: bool,
}

impl Restriction {
    fn admits(&self, default: bool) -> bool {
        !self.denied && (self.matched || default && !self.positive)
    }
}

fn ceilings(
    metadata: &AccessMetadataSnapshot,
    admission: &DeviceAdmission,
    hosted_organ: &str,
    roles: &[i64],
) -> (VisibilityCeiling, BTreeSet<String>) {
    let mut result = VisibilityCeiling::default();
    let mut linguas = BTreeSet::new();
    for role in roles {
        let (ceiling, visible) = role_ceilings(metadata, admission, hosted_organ, *role);
        result.records.extend(ceiling.records);
        result.concepts.extend(ceiling.concepts);
        result.places.extend(ceiling.places);
        linguas.extend(visible);
    }
    (result, linguas)
}

fn role_ceilings(
    metadata: &AccessMetadataSnapshot,
    admission: &DeviceAdmission,
    hosted_organ: &str,
    role: i64,
) -> (VisibilityCeiling, BTreeSet<String>) {
    let person = admission.authentication().person_uid();
    let peer = admission.peer_contact().map(|peer| peer.organ_uid.as_str());
    let mut restrictions: BTreeMap<&str, Restriction> = BTreeMap::new();
    for rule in &metadata.visibility_rules {
        let target = match &rule.target {
            VisibilityTarget::Record(uid)
            | VisibilityTarget::Concept(uid)
            | VisibilityTarget::Place(uid) => uid,
        };
        let restriction = restrictions.entry(target).or_default();
        let matches = match &rule.subject {
            VisibilitySubject::Public => true,
            VisibilitySubject::Actor(uid) => uid == person,
            VisibilitySubject::Organ(uid) => uid == hosted_organ || Some(uid.as_str()) == peer,
            VisibilitySubject::Role(id) => *id == role,
            VisibilitySubject::Unsupported { .. } => {
                restriction.denied = true;
                false
            }
        };
        if rule.field.is_some() {
            restriction.denied = true;
        }
        match rule.grant {
            VisibilityGrant::Visible => {
                restriction.positive = true;
                restriction.matched |= matches;
            }
            VisibilityGrant::Hidden => restriction.denied |= matches,
        }
    }
    let admitted = |uid: &str, default: bool| {
        restrictions
            .get(uid)
            .map_or(default, |restriction| restriction.admits(default))
    };
    let records: BTreeMap<&str, _> = metadata
        .records
        .iter()
        .map(|record| (record.uid.as_str(), record))
        .collect();
    let drafts: BTreeMap<&str, _> = metadata
        .message_drafts
        .iter()
        .map(|draft| (draft.record_uid.as_str(), draft))
        .collect();
    let draft_admitted = |uid: &str| {
        records.get(uid).is_some_and(|record| {
            record.kind != RecordKind::MessageDraft
                || drafts
                    .get(uid)
                    .is_some_and(|draft| draft.operator_uid == person)
        })
    };
    let mut ceiling = VisibilityCeiling::default();
    for record in &metadata.records {
        let scope_admitted = match &record.replica_root {
            None => admitted(
                &record.uid,
                record.organ_uid.as_deref() == Some(hosted_organ),
            ),
            Some(root) => records.get(root.as_str()).is_some_and(|root_record| {
                (!root_record.deleted || root == &record.uid)
                    && root_record
                        .replica_root
                        .as_ref()
                        .is_none_or(|parent| parent == root)
                    && admitted(root, false)
                    && draft_admitted(root)
                    && admitted(&record.uid, true)
            }),
        };
        if scope_admitted && draft_admitted(&record.uid) {
            ceiling.records.insert(record.uid.clone());
        }
    }
    let linguas: BTreeSet<String> = metadata
        .linguas
        .iter()
        .filter(|lingua| {
            lingua.visibility == LinguaVisibility::Public
                || lingua
                    .owner_organ
                    .as_deref()
                    .is_none_or(|owner| owner == hosted_organ)
        })
        .map(|lingua| lingua.uid.clone())
        .collect();
    let vocabulary: BTreeSet<&str> = metadata
        .lingua_concepts
        .iter()
        .filter(|member| linguas.contains(&member.lingua_uid))
        .map(|member| member.concept_uid.as_str())
        .collect();
    for concept in &metadata.concepts {
        if admitted(&concept.uid, vocabulary.contains(concept.uid.as_str())) {
            ceiling.concepts.insert(concept.uid.clone());
        }
    }
    for place in &metadata.places {
        if admitted(place, true) {
            ceiling.places.insert(place.clone());
        }
    }
    (ceiling, linguas)
}

fn readable_sets(
    metadata: &AccessMetadataSnapshot,
    ceiling: &VisibilityCeiling,
    records: BTreeSet<String>,
    linguas: BTreeSet<String>,
) -> ReadableSets {
    let assertions = metadata
        .assertions
        .iter()
        .filter(|assertion| {
            records.contains(&assertion.subject_uid)
                && assertion
                    .object_uid
                    .as_ref()
                    .is_none_or(|uid| records.contains(uid))
                && ceiling.concepts.contains(&assertion.predicate_uid)
                && assertion
                    .unit_uid
                    .as_ref()
                    .is_none_or(|uid| ceiling.concepts.contains(uid))
        })
        .map(|assertion| assertion.uid.clone())
        .collect();
    let concept_parents = metadata
        .concept_parents
        .iter()
        .filter(|parent| {
            ceiling.concepts.contains(&parent.concept_uid)
                && ceiling.concepts.contains(&parent.parent_uid)
        })
        .map(|parent| (parent.concept_uid.clone(), parent.parent_uid.clone()))
        .collect();
    let lingua_concepts = metadata
        .lingua_concepts
        .iter()
        .filter(|member| {
            linguas.contains(&member.lingua_uid) && ceiling.concepts.contains(&member.concept_uid)
        })
        .map(|member| (member.lingua_uid.clone(), member.concept_uid.clone()))
        .collect();
    ReadableSets {
        records,
        concepts: ceiling.concepts.clone(),
        places: ceiling.places.clone(),
        assertions,
        linguas,
        concept_parents,
        lingua_concepts,
    }
}

fn validate_staged(
    current: &Snapshot,
    proposed: &Snapshot,
    targets: &[MutationTarget],
    assertion_intents: Option<&[authority::AssertionIntent]>,
    hosted_organ: &str,
) -> Result<(), AccessError> {
    let before = &current.metadata;
    let after = &proposed.metadata;
    if before.visibility_rules != after.visibility_rules
        || before.linguas != after.linguas
        || before.lingua_concepts != after.lingua_concepts
        || before.role_ids != after.role_ids
        || before.concepts != after.concepts
        || before.concept_parents != after.concept_parents
        || before.places != after.places
    {
        return Err(AccessError::AuthorityChanged);
    }
    let targets: BTreeMap<&str, _> = targets
        .iter()
        .map(|target| (target.record_uid.as_str(), target))
        .collect();
    let attempted_assertion_subjects: BTreeSet<&str> = assertion_intents
        .into_iter()
        .flatten()
        .filter_map(|intent| intent.before.as_ref().or(intent.after.as_ref()))
        .map(|assertion| assertion.subject_uid.as_str())
        .collect();
    let mut old_assertions: BTreeMap<&str, Vec<_>> = BTreeMap::new();
    let mut new_assertions: BTreeMap<&str, Vec<_>> = BTreeMap::new();
    for assertion in &before.assertions {
        old_assertions
            .entry(&assertion.subject_uid)
            .or_default()
            .push(assertion);
    }
    for assertion in &after.assertions {
        new_assertions
            .entry(&assertion.subject_uid)
            .or_default()
            .push(assertion);
    }
    for (uid, revision) in &current.revisions {
        if proposed.revisions.get(uid) != Some(revision) && !targets.contains_key(uid.as_str()) {
            return Err(AccessError::InvalidMutation);
        }
    }
    for record in &after.records {
        if !current.revisions.contains_key(&record.uid)
            && (!targets.contains_key(record.uid.as_str())
                || record.organ_uid.as_deref() != Some(hosted_organ))
        {
            return Err(AccessError::InvalidMutation);
        }
    }
    for (uid, content) in &proposed.content {
        if !current.content.contains_key(uid)
            && content
                .extensions
                .values()
                .any(|extension| extension.fields.is_empty())
        {
            return Err(AccessError::InvalidMutation);
        }
    }
    for (uid, original) in &current.content {
        let changed = proposed
            .content
            .get(uid)
            .ok_or(AccessError::InvalidMutation)?;
        if original.replica_root != changed.replica_root {
            return Err(AccessError::AuthorityChanged);
        }
        for (namespace, extension) in &original.extensions {
            match changed.extensions.get(namespace) {
                None if extension.fields.is_empty() => return Err(AccessError::InvalidMutation),
                Some(next)
                    if extension.fields == next.fields && extension.version != next.version =>
                {
                    return Err(AccessError::InvalidMutation);
                }
                _ => {}
            }
        }
        for (namespace, extension) in &changed.extensions {
            if !original.extensions.contains_key(namespace) && extension.fields.is_empty() {
                return Err(AccessError::InvalidMutation);
            }
        }
        if original == changed
            && current.revisions.get(uid) != proposed.revisions.get(uid)
            && targets
                .get(uid.as_str())
                .is_none_or(|target| target.touched_properties.is_empty())
            && old_assertions.get(uid.as_str()) == new_assertions.get(uid.as_str())
            && !attempted_assertion_subjects.contains(uid.as_str())
        {
            return Err(AccessError::InvalidMutation);
        }
    }
    Ok(())
}
