pub mod adapters;
mod edit;
mod effects;
mod records;

use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::canvas::{Component, Geometry};
use nucleus::component::ComponentState;
use protein::authority::RolePolicy;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use store::sqlx::{Sqlite, Transaction};

pub const PROTOCOL: u32 = 2;
pub const SCHEMA: u32 = 2;
pub const FEATURES: &[&str] = &[
    "role-union",
    "workspace-ceiling",
    "host-effects",
    "change-review",
    "transactional-controls",
    "record-editing",
];

#[derive(Default)]
pub struct Presence(std::sync::Mutex<BTreeMap<String, BTreeSet<String>>>);

impl Presence {
    pub fn join(&self, workspace: &str, participant: &str) -> Result<bool, EngineError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| invalid("Workspace presence unavailable"))?;
        let participants = state.entry(workspace.into()).or_default();
        if participants.len() >= 64 && !participants.contains(participant) {
            return Err(invalid(
                "This workspace already has 64 connected participants",
            ));
        }
        Ok(participants.insert(participant.into()))
    }

    pub fn leave(&self, workspace: Option<&str>, participant: &str) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        let mut changed = false;
        state.retain(|uid, participants| {
            if workspace.is_none_or(|workspace| workspace == uid) {
                changed |= participants.remove(participant);
            }
            !participants.is_empty()
        });
        changed
    }

    pub fn participants(&self, workspace: &str) -> Vec<String> {
        self.0
            .lock()
            .ok()
            .and_then(|state| {
                state
                    .get(workspace)
                    .map(|participants| participants.iter().cloned().collect())
            })
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Client {
    pub protocol: u32,
    pub schema: u32,
    pub lince_version: String,
    pub features: Vec<String>,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            protocol: PROTOCOL,
            schema: SCHEMA,
            lince_version: env!("CARGO_PKG_VERSION").into(),
            features: FEATURES.iter().map(|feature| (*feature).into()).collect(),
        }
    }
}

impl Client {
    pub fn validate(&self) -> Result<(), EngineError> {
        let version: Option<Vec<u32>> = self
            .lince_version
            .split('.')
            .map(|part| part.parse::<u32>().ok())
            .collect();
        if self.protocol != PROTOCOL
            || self.schema != SCHEMA
            || self.features.len() > 32
            || version
                .as_ref()
                .is_none_or(|parts| parts.len() != 3 || parts.as_slice() < [0, 7, 0].as_slice())
            || FEATURES
                .iter()
                .any(|required| !self.features.iter().any(|feature| feature == required))
        {
            return Err(EngineError::Conflict { code: "workspace_client_incompatible", message: "Workspace Sync requires Lince 0.7.0 or newer, protocol 2, schema 2 and all collaboration features. Update this client before joining.".into() });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub client: Client,
    pub command: Command,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    List,
    Capabilities,
    ValidateImport {
        name: String,
        policy: Value,
        layout: Value,
    },
    Publish {
        name: String,
        policy: Value,
        layout: Value,
    },
    SaveDraft {
        uid: String,
        host: Option<String>,
        workspace: String,
        base_revision: i64,
        change: Change,
    },
    Drafts,
    DiscardDraft {
        uid: String,
    },
    Preview {
        workspace: String,
        change: Change,
    },
    PreviewProposal {
        workspace: String,
        proposal: String,
    },
    PreviewPolicy {
        workspace: String,
        policy: Value,
    },
    Create {
        name: String,
        policy: Value,
    },
    Inspect {
        workspace: String,
        #[serde(default)]
        permitted_view: bool,
    },
    History {
        workspace: String,
        #[serde(default)]
        before: Option<String>,
    },
    Propose {
        workspace: String,
        request_id: String,
        base_revision: i64,
        change: Change,
    },
    Review {
        workspace: String,
        proposal: String,
        request_id: String,
        expected_revision: i64,
        approve: bool,
    },
    Delete {
        workspace: String,
        expected_revision: i64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Invoke {
        element: String,
        path: Vec<String>,
        event: Option<String>,
        action: Value,
    },
    EditRecord {
        record: String,
        edits: Vec<RecordEdit>,
    },
    RestoreRecord {
        record: String,
        slug: Option<String>,
        placement: String,
        geometry: Geometry,
    },
    Add {
        element: Element,
    },
    Configure {
        element: String,
        component: Component,
    },
    Move {
        element: String,
        position: [f64; 2],
    },
    Resize {
        element: String,
        size: [f64; 2],
    },
    Remove {
        element: String,
    },
    Rename {
        name: String,
    },
    Policy {
        policy: Value,
    },
    Area {
        element: String,
        changes: crate::area_transition::RecordChanges,
    },
    CreateRecord {
        draft: crate::record_creation::Draft,
        placement: String,
        geometry: Geometry,
    },
    ChangeRecord {
        record: String,
        changes: crate::area_transition::RecordChanges,
    },
    DeleteRecord {
        record: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordEdit {
    Text {
        head: Option<String>,
        body: Option<String>,
    },
    Slug {
        slug: Option<String>,
    },
    Unit {
        unit: Option<String>,
    },
    Extension {
        namespace: String,
        value: Value,
    },
    Assert {
        predicate: String,
        object: Option<String>,
        quantity: Option<String>,
        unit: Option<String>,
    },
    Retract {
        assertion: String,
    },
    AssertionValue {
        assertion: String,
        quantity: Option<String>,
        unit: Option<String>,
    },
    Refine {
        predicate: String,
        object: String,
    },
    Identity {
        predicate: Option<String>,
    },
}

fn change_permission(change: &Change) -> &'static str {
    if matches!(change, Change::Policy { .. }) {
        "workspace:access_control"
    } else {
        "workspace:update"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: String,
    pub component: Component,
    pub geometry: Geometry,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub elements: Vec<Element>,
    pub areas: BTreeMap<String, crate::area_transition::RecordChanges>,
    #[serde(default)]
    pub disabled_areas: BTreeSet<String>,
    #[serde(default)]
    pub disabled_controls: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub required_capabilities: BTreeSet<String>,
    pub ceiling: RolePolicy,
    #[serde(default)]
    pub viewers: Audience,
    #[serde(default)]
    pub editors: Audience,
    #[serde(default)]
    pub managers: Audience,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audience {
    pub everyone: bool,
    pub actors: BTreeSet<String>,
    pub roles: BTreeSet<i64>,
}

impl Default for Audience {
    fn default() -> Self {
        Self {
            everyone: true,
            actors: Default::default(),
            roles: Default::default(),
        }
    }
}

impl Audience {
    async fn admits_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
    ) -> Result<bool, EngineError> {
        let Some(actor) = actor else {
            return Ok(true);
        };
        if self.everyone || self.actors.contains(actor) {
            return Ok(true);
        }
        Ok(store::person_roles::ids_on(tx, actor)
            .await?
            .iter()
            .any(|role| self.roles.contains(role)))
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.actors.len() + self.roles.len() > 128
            || self
                .actors
                .iter()
                .any(|actor| !nucleus::valid_uid(actor, "r"))
            || self.roles.iter().any(|role| *role <= 0)
        {
            return Err(invalid(
                "Choose at most 128 existing Actors and Roles for each workspace audience",
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Hosted {
    uid: String,
    name: String,
    revision: i64,
    policy: Policy,
    layout: Layout,
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}
fn name_valid(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 80 && !name.chars().any(char::is_control)
}

fn component_valid(component: &Component) -> bool {
    adapters::valid(component)
}

impl Policy {
    fn validate(&self) -> Result<(), EngineError> {
        self.viewers.validate()?;
        self.editors.validate()?;
        self.managers.validate()?;
        if self.required_capabilities.len() > 128
            || self
                .required_capabilities
                .iter()
                .any(|key| !utils::auth::all_permission_keys().contains(key))
            || self.ceiling.grants.len() > 128
        {
            return Err(invalid(
                "Choose bounded, existing workspace capabilities and at most 128 grants",
            ));
        }
        for selector in std::iter::once(&self.ceiling.read)
            .chain(self.ceiling.grants.iter().map(|grant| &grant.selector))
        {
            protein::validate(&protein::Protein {
                source: protein::Source::Record,
                filter: vec![selector.clone()],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: vec![],
                limit: None,
            })?;
        }
        Ok(())
    }
}

impl Layout {
    fn validate(&self) -> Result<(), EngineError> {
        if self.elements.len() > 256
            || self
                .elements
                .iter()
                .map(|element| adapters::size(&element.component))
                .sum::<usize>()
                > 256
            || self.records().len() > 256
            || serde_json::to_vec(self)?.len() > 262144
        {
            return Err(invalid(
                "A shared workspace supports at most 256 elements, 256 exposed Records and 256 KiB",
            ));
        }
        let mut ids = BTreeSet::new();
        for element in &self.elements {
            if !nucleus::valid_uid(&element.id, "placement")
                || !ids.insert(&element.id)
                || !component_valid(&element.component)
            {
                return Err(invalid(
                    "Shared topology supports Text, Record, Area, declared controls and compositions. Other components require a host adapter.",
                ));
            }
            element.geometry.validate().map_err(invalid)?;
        }
        for (id, changes) in &self.areas {
            if !changes.validate()
                || !self.elements.iter().any(|element| {
                    &element.id == id
                        && matches!(
                            element.component,
                            Component::Builtin {
                                state: ComponentState::Area { .. }
                            }
                        )
                })
            {
                return Err(invalid(
                    "An Area recipe requires an existing Area and valid changes",
                ));
            }
        }
        if self
            .disabled_areas
            .iter()
            .any(|id| !self.areas.contains_key(id))
        {
            return Err(invalid("A suspended Area must have an existing recipe"));
        }
        if self.disabled_controls.iter().any(|id| {
            !self.elements.iter().any(|element| {
                &element.id == id && !adapters::controls(&element.component).is_empty()
            })
        }) {
            return Err(invalid("A suspended control must exist"));
        }
        Ok(())
    }

    fn records(&self) -> BTreeSet<String> {
        self.elements
            .iter()
            .flat_map(|element| adapters::component_records(&element.component).into_iter())
            .chain(
                self.areas
                    .values()
                    .flat_map(|changes| changes.assign.iter().chain(&changes.unassign).cloned()),
            )
            .collect()
    }
}

async fn load(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<Hosted, EngineError> {
    let row: Option<(String, i64, String, String)> = store::sqlx::query_as(
        "SELECT name, revision, policy, layout FROM shared_workspace WHERE uid = ?",
    )
    .bind(uid)
    .fetch_optional(&mut **tx)
    .await?;
    let (name, revision, policy, layout) =
        row.ok_or_else(|| invalid("Shared workspace unavailable"))?;
    Ok(Hosted {
        uid: uid.into(),
        name,
        revision,
        policy: serde_json::from_str(&policy)?,
        layout: serde_json::from_str(&layout)?,
    })
}

fn sensitive(hosted: &Hosted, change: &Change) -> bool {
    match change {
        Change::Policy { .. }
        | Change::Area { .. }
        | Change::CreateRecord { .. }
        | Change::ChangeRecord { .. }
        | Change::DeleteRecord { .. }
        | Change::EditRecord { .. }
        | Change::RestoreRecord { .. } => true,
        Change::Add { element } => !adapters::controls(&element.component).is_empty(),
        Change::Configure { element, component } => {
            hosted.layout.areas.contains_key(element)
                || !matches!(
                    (
                        hosted
                            .layout
                            .elements
                            .iter()
                            .find(|item| item.id == *element)
                            .map(|item| &item.component),
                        component
                    ),
                    (
                        Some(Component::Builtin {
                            state: ComponentState::Text { .. }
                        }),
                        Component::Builtin {
                            state: ComponentState::Text { .. }
                        }
                    )
                )
        }
        Change::Move { element, .. }
        | Change::Resize { element, .. }
        | Change::Remove { element } => hosted.layout.areas.contains_key(element),
        _ => false,
    }
}

fn change_valid(change: &Change) -> Result<(), EngineError> {
    let valid_element = |id: &str| nucleus::valid_uid(id, "placement");
    match change {
        Change::Invoke {
            element,
            path,
            event,
            action,
        } => {
            if !valid_element(element)
                || path.len() > 7
                || path.iter().any(|part| part.is_empty() || part.len() > 80)
                || event.as_ref().is_some_and(|event| event.len() > 80)
            {
                return Err(invalid("Invalid control address"));
            }
            adapters::action_change(action)?;
        }
        Change::EditRecord { record, edits } => {
            if !nucleus::valid_uid(record, "r") {
                return Err(invalid("Invalid Record identity"));
            }
            edit::validate(edits)?;
        }
        Change::RestoreRecord {
            record,
            slug,
            placement,
            geometry,
        } => {
            if !nucleus::valid_uid(record, "r")
                || !valid_element(placement)
                || slug.as_ref().is_some_and(|slug| !nucleus::valid_slug(slug))
            {
                return Err(invalid("Invalid restoration"));
            }
            geometry.validate().map_err(invalid)?;
        }
        Change::Add { element } => {
            if !valid_element(&element.id) || !component_valid(&element.component) {
                return Err(invalid("Unsupported shared element"));
            }
            element.geometry.validate().map_err(invalid)?;
        }
        Change::Configure { element, component } => {
            if !valid_element(element) || !component_valid(component) {
                return Err(invalid("Unsupported shared configuration"));
            }
        }
        Change::Move { element, position } => {
            if !valid_element(element) {
                return Err(invalid("Invalid placement identity"));
            }
            Geometry {
                position: *position,
                size: [1.0; 2],
            }
            .validate()
            .map_err(invalid)?;
        }
        Change::Resize { element, size } => {
            if !valid_element(element) {
                return Err(invalid("Invalid placement identity"));
            }
            Geometry {
                position: [0.0; 2],
                size: *size,
            }
            .validate()
            .map_err(invalid)?;
        }
        Change::Remove { element } => {
            if !valid_element(element) {
                return Err(invalid("Invalid placement identity"));
            }
        }
        Change::Rename { name } => {
            if !name_valid(name) {
                return Err(invalid("Use a workspace name of 1–80 characters"));
            }
        }
        Change::Policy { policy } => {
            serde_json::from_value::<Policy>(policy.clone())?.validate()?;
        }
        Change::Area { element, changes } => {
            if !valid_element(element) || !changes.validate() {
                return Err(invalid("Invalid Area recipe"));
            }
        }
        Change::CreateRecord {
            draft,
            placement,
            geometry,
        } => {
            draft.validate()?;
            if !valid_element(placement) || draft.work != json!({}) {
                return Err(invalid("Use a plain Record draft and valid placement"));
            }
            geometry.validate().map_err(invalid)?;
        }
        Change::ChangeRecord { record, changes } => {
            if !nucleus::valid_uid(record, "r") || !changes.validate() || changes.is_empty() {
                return Err(invalid("Invalid Record change"));
            }
        }
        Change::DeleteRecord { record } => {
            if !nucleus::valid_uid(record, "r") {
                return Err(invalid("Invalid Record identity"));
            }
        }
    }
    Ok(())
}

fn presentation_conflicts(a: &Change, b: &Change) -> bool {
    let element = |change: &Change| match change {
        Change::Add { element } => Some(element.id.clone()),
        Change::Configure { element, .. }
        | Change::Move { element, .. }
        | Change::Resize { element, .. }
        | Change::Remove { element }
        | Change::Area { element, .. } => Some(element.clone()),
        _ => None,
    };
    if matches!(
        a,
        Change::Policy { .. }
            | Change::CreateRecord { .. }
            | Change::ChangeRecord { .. }
            | Change::DeleteRecord { .. }
            | Change::EditRecord { .. }
            | Change::RestoreRecord { .. }
            | Change::Invoke { .. }
    ) || matches!(
        b,
        Change::Policy { .. }
            | Change::CreateRecord { .. }
            | Change::ChangeRecord { .. }
            | Change::DeleteRecord { .. }
            | Change::EditRecord { .. }
            | Change::RestoreRecord { .. }
            | Change::Invoke { .. }
    ) {
        return true;
    }
    if matches!((a, b), (Change::Rename { .. }, Change::Rename { .. })) {
        return true;
    }
    element(a).is_some_and(|id| element(b).as_ref() == Some(&id))
}

fn policy_records(policy: &Policy) -> BTreeSet<String> {
    fn visit(selector: &protein::Predicate, records: &mut BTreeSet<String>) {
        match selector {
            protein::Predicate::All(items) | protein::Predicate::Any(items) => {
                for item in items {
                    visit(item, records);
                }
            }
            protein::Predicate::Not(item) => visit(item, records),
            protein::Predicate::UidEq(uid)
            | protein::Predicate::OrganEq(uid)
            | protein::Predicate::Under { record: uid, .. } => {
                records.insert(uid.clone());
            }
            protein::Predicate::OrganIn(uids) => records.extend(uids.iter().cloned()),
            protein::Predicate::Relation {
                other: Some(uid), ..
            } => {
                records.insert(uid.clone());
            }
            _ => {}
        }
    }
    let mut records = BTreeSet::new();
    visit(&policy.ceiling.read, &mut records);
    for grant in &policy.ceiling.grants {
        visit(&grant.selector, &mut records);
        for rule in grant.assertions_add.iter().chain(&grant.assertions_remove) {
            if let protein::authority::AssertionTarget::Record(uid) = &rule.target {
                records.insert(uid.clone());
            }
        }
    }
    for audience in [&policy.viewers, &policy.editors, &policy.managers] {
        records.extend(audience.actors.iter().cloned());
    }
    records
}

fn change_records(change: &Change) -> BTreeSet<String> {
    match change {
        Change::Policy { policy } => serde_json::from_value::<Policy>(policy.clone())
            .map(|policy| policy_records(&policy))
            .unwrap_or_default(),
        Change::Invoke { action, .. } => adapters::action_change(action)
            .map(|change| change_records(&change))
            .unwrap_or_default(),
        Change::EditRecord { record, edits } => std::iter::once(record.clone())
            .chain(edit::references(edits))
            .collect(),
        Change::RestoreRecord { record, .. } => BTreeSet::from([record.clone()]),
        Change::Add { element } => adapters::component_records(&element.component),
        Change::Configure { component, .. } => adapters::component_records(component),
        Change::Area { changes, .. } => changes
            .assign
            .iter()
            .chain(&changes.unassign)
            .cloned()
            .collect(),
        Change::ChangeRecord { record, changes } => std::iter::once(record.clone())
            .chain(changes.assign.iter().chain(&changes.unassign).cloned())
            .collect(),
        Change::CreateRecord { draft, .. } => draft
            .assertions
            .iter()
            .filter_map(|assertion| assertion.object.clone())
            .collect(),
        Change::DeleteRecord { record } => BTreeSet::from([record.clone()]),
        _ => BTreeSet::new(),
    }
}

async fn independent_since(
    tx: &mut Transaction<'_, Sqlite>,
    hosted: &Hosted,
    base: i64,
    change: &Change,
) -> Result<bool, EngineError> {
    if base < 1 || base >= hosted.revision || hosted.revision - base > 256 {
        return Ok(false);
    }
    let changes: Vec<String> = store::sqlx::query_scalar("SELECT change FROM workspace_change WHERE workspace_uid=? AND status='applied' AND applied_revision>? ORDER BY applied_revision LIMIT 257").bind(&hosted.uid).bind(base).fetch_all(&mut **tx).await?;
    if changes.len() as i64 != hosted.revision - base {
        return Ok(false);
    }
    for other in changes {
        let other: Change = serde_json::from_str(&other)?;
        if presentation_conflicts(change, &other) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn apply(hosted: &mut Hosted, change: &Change) -> Result<(), EngineError> {
    let find = |layout: &mut Layout, id: &str| {
        layout
            .elements
            .iter()
            .position(|element| element.id == id)
            .ok_or_else(|| invalid("Element unavailable"))
    };
    match change {
        Change::Invoke { .. } => {
            let resolved = adapters::resolve(&hosted.layout, change)?;
            apply(hosted, &resolved)?;
        }
        Change::EditRecord { record, .. } => {
            if !hosted.layout.records().contains(record) {
                return Err(invalid("Choose an exposed Record"));
            }
        }
        Change::RestoreRecord {
            record,
            placement,
            geometry,
            ..
        } => hosted.layout.elements.push(Element {
            id: placement.clone(),
            geometry: geometry.clone(),
            component: Component::Builtin {
                state: ComponentState::Record {
                    record: record.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
            },
        }),
        Change::Add { element } => hosted.layout.elements.push(element.clone()),
        Change::Configure { element, component } => {
            let index = find(&mut hosted.layout, element)?;
            hosted.layout.elements[index].component = component.clone();
            hosted.layout.areas.remove(element);
            hosted.layout.disabled_areas.remove(element);
            hosted.layout.disabled_controls.remove(element);
        }
        Change::Move { element, position } => {
            let index = find(&mut hosted.layout, element)?;
            hosted.layout.elements[index].geometry.position = *position;
        }
        Change::Resize { element, size } => {
            let index = find(&mut hosted.layout, element)?;
            hosted.layout.elements[index].geometry.size = *size;
        }
        Change::Remove { element } => {
            let index = find(&mut hosted.layout, element)?;
            hosted.layout.elements.remove(index);
            hosted.layout.areas.remove(element);
            hosted.layout.disabled_areas.remove(element);
            hosted.layout.disabled_controls.remove(element);
        }
        Change::Rename { name } if name_valid(name) => hosted.name = name.trim().into(),
        Change::Rename { .. } => return Err(invalid("Use a workspace name of 1–80 characters")),
        Change::Policy { policy } => {
            hosted.policy = serde_json::from_value(policy.clone())?;
            hosted.policy.validate()?;
            hosted
                .layout
                .disabled_areas
                .extend(hosted.layout.areas.keys().cloned());
            hosted.layout.disabled_controls.extend(
                hosted
                    .layout
                    .elements
                    .iter()
                    .filter(|element| !adapters::controls(&element.component).is_empty())
                    .map(|element| element.id.clone()),
            );
        }
        Change::Area { element, changes } => {
            hosted.layout.areas.insert(element.clone(), changes.clone());
            hosted.layout.disabled_areas.remove(element);
            hosted.layout.disabled_controls.remove(element);
        }
        Change::CreateRecord {
            draft,
            placement,
            geometry,
        } => {
            draft.validate()?;
            if draft.work != json!({}) {
                return Err(invalid(
                    "Shared creation supports plain Records with assertions; configure work metadata through the Record editor",
                ));
            }
            hosted.layout.elements.push(Element {
                id: placement.clone(),
                component: Component::Builtin {
                    state: ComponentState::Record {
                        record: draft.uid.clone(),
                        mode: Default::default(),
                        start_call: None,
                    },
                },
                geometry: geometry.clone(),
            });
        }
        Change::ChangeRecord { record, changes } => {
            if !nucleus::valid_uid(record, "r")
                || !changes.validate()
                || changes.is_empty()
                || !hosted.layout.records().contains(record)
            {
                return Err(invalid("Choose an exposed Record and bounded changes"));
            }
        }
        Change::DeleteRecord { record } => {
            if !nucleus::valid_uid(record, "r") || !hosted.layout.records().contains(record) {
                return Err(invalid("Choose an exposed Record"));
            }
            hosted.layout.elements.retain(|element| {
                !adapters::component_records(&element.component).contains(record)
            });
            hosted.layout.disabled_controls.retain(|id| {
                hosted
                    .layout
                    .elements
                    .iter()
                    .any(|element| &element.id == id)
            });
            if hosted.layout.records().contains(record) {
                return Err(invalid(
                    "Remove Area references before deleting this Record",
                ));
            }
        }
    }
    hosted.layout.validate()
}

impl Engine {
    async fn workspace_permission_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        hosted: &Hosted,
        permission: &str,
    ) -> Result<(), EngineError> {
        self.require_permission_on(tx, actor, permission).await?;
        let audience = match permission {
            "workspace:read" => &hosted.policy.viewers,
            "workspace:access_control" | "workspace:delete" => &hosted.policy.managers,
            _ => &hosted.policy.editors,
        };
        if !audience.admits_on(tx, actor).await? {
            return Err(EngineError::Forbidden(format!(
                "Missing {permission} admission for this workspace"
            )));
        }
        Ok(())
    }

    async fn workspace_required_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        hosted: &Hosted,
    ) -> Result<(), EngineError> {
        self.workspace_permission_on(tx, actor, hosted, "workspace:read")
            .await?;
        for capability in &hosted.policy.required_capabilities {
            self.require_permission_on(tx, actor, capability).await?;
        }
        Ok(())
    }

    async fn workspace_readable_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        hosted: &Hosted,
        exact: bool,
    ) -> Result<BTreeSet<String>, EngineError> {
        self.workspace_required_on(tx, actor, hosted).await?;
        let targets = hosted.layout.records();
        if targets.is_empty() {
            return Ok(BTreeSet::new());
        }
        let graph =
            crate::access::policy_graph_on(tx, &targets.iter().cloned().collect::<Vec<_>>())
                .await
                .map_err(crate::record_policy::denied)?;
        let readable = self.readable_on(tx, actor, &graph).await?;
        let ceiling = crate::record_policy::ceiling(
            &graph,
            graph
                .records
                .iter()
                .filter(|record| !record.deleted)
                .map(|record| record.uid.clone())
                .collect(),
        );
        let permitted = protein::authority::readable_records(
            Some(&hosted.policy.ceiling),
            &graph,
            &ceiling,
            &Default::default(),
        )
        .map_err(crate::record_policy::denied)?;
        let readable = readable
            .intersection(&permitted)
            .cloned()
            .collect::<BTreeSet<_>>();
        if exact && !targets.is_subset(&readable) {
            return Err(EngineError::Conflict { code:"workspace_missing_read_access", message:"Exact Workspace Sync requires read access to every exposed Record and reference. Request a separate permitted view to omit unavailable elements.".into() });
        }
        Ok(targets.intersection(&readable).cloned().collect())
    }

    async fn workspace_admission_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        hosted: &Hosted,
    ) -> Result<(), EngineError> {
        self.workspace_readable_on(tx, actor, hosted, true).await?;
        Ok(())
    }

    async fn workspace_admission(
        &self,
        actor: Option<&str>,
        hosted: &Hosted,
        exact: bool,
    ) -> Result<BTreeSet<String>, EngineError> {
        let mut tx = self.store.pool.begin().await?;
        let readable = self
            .workspace_readable_on(&mut tx, actor, hosted, exact)
            .await?;
        tx.commit().await?;
        Ok(readable)
    }

    async fn workspace_layout_ceiling(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        hosted: &Hosted,
    ) -> Result<(), EngineError> {
        let targets = hosted.layout.records();
        let graph =
            crate::access::policy_graph_on(tx, &targets.iter().cloned().collect::<Vec<_>>())
                .await
                .map_err(crate::record_policy::denied)?;
        let ceiling = crate::record_policy::ceiling(
            &graph,
            graph
                .records
                .iter()
                .filter(|record| !record.deleted)
                .map(|record| record.uid.clone())
                .collect(),
        );
        let readable = protein::authority::readable_records(
            Some(&hosted.policy.ceiling),
            &graph,
            &ceiling,
            &Default::default(),
        )
        .map_err(crate::record_policy::denied)?;
        if !targets.is_subset(&readable) {
            return Err(invalid(
                "The layout exposes a Record outside the workspace read ceiling",
            ));
        }
        Ok(())
    }

    pub async fn workspace_view(
        &self,
        actor: Option<&str>,
        workspace: &str,
        client: &Client,
        permitted_view: bool,
    ) -> Result<Value, EngineError> {
        client.validate()?;
        let mut tx = self.store.pool.begin().await?;
        self.require_login_on(&mut tx).await?;
        let hosted = load(&mut tx, workspace).await?;
        let readable = self
            .workspace_readable_on(&mut tx, actor, &hosted, !permitted_view)
            .await?;
        let mut layout = hosted.layout.clone();
        if permitted_view {
            layout.elements.retain(|element| {
                adapters::component_records(&element.component).is_subset(&readable)
                    && !hosted.layout.areas.contains_key(&element.id)
            });
            layout.areas.clear();
            layout.disabled_areas.clear();
            layout.disabled_controls.clear();
        }
        let can_review = !permitted_view
            && self
                .workspace_permission_on(&mut tx, actor, &hosted, "workspace:access_control")
                .await
                .is_ok();
        let can_edit = !permitted_view
            && self
                .workspace_permission_on(&mut tx, actor, &hosted, "workspace:update")
                .await
                .is_ok();
        let may_read_policy = if can_review && policy_records(&hosted.policy).is_empty() {
            true
        } else if can_review {
            let graph = crate::access::policy_graph_on(&mut tx, &[])
                .await
                .map_err(crate::record_policy::denied)?;
            policy_records(&hosted.policy)
                .is_subset(&self.readable_on(&mut tx, actor, &graph).await?)
        } else {
            false
        };
        let host: String = store::sqlx::query_scalar("SELECT uid FROM record WHERE slug='local-organ' AND kind='organ' AND deleted_at IS NULL").fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(
            json!({ "uid": if permitted_view { format!("permitted:{}", hosted.uid) } else { hosted.uid.clone() }, "host":host, "participants":if permitted_view { vec![] } else { self.workspace_presence.participants(&hosted.uid) }, "hosted_workspace":hosted.uid, "name":hosted.name, "revision":hosted.revision, "layout":layout, "separate_view":permitted_view, "can_edit":can_edit, "can_review":can_review, "policy_unavailable":can_review && !may_read_policy, "policy":if may_read_policy { serde_json::to_value(hosted.policy)? } else { Value::Null }, "client":Client::default() }),
        )
    }

    pub async fn workspace_request(
        &self,
        mut request: Request,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        request.client.validate()?;
        if let Command::Propose {
            change: Change::Area { changes, .. } | Change::ChangeRecord { changes, .. },
            ..
        }
        | Command::Preview {
            change: Change::Area { changes, .. } | Change::ChangeRecord { changes, .. },
            ..
        } = &mut request.command
        {
            *changes = self.canonical_area_changes(changes.clone()).await?;
        }
        let payload = serde_json::to_string(&request)?;
        if payload.len() > 524288 {
            return Err(invalid("Workspace request exceeds 512 KiB"));
        }
        let mut outcome = ActionOutcome::default();
        let changed = !matches!(
            request.command,
            Command::List
                | Command::Capabilities
                | Command::ValidateImport { .. }
                | Command::Inspect { .. }
                | Command::History { .. }
                | Command::Drafts
                | Command::SaveDraft { .. }
                | Command::DiscardDraft { .. }
                | Command::Preview { .. }
                | Command::PreviewPolicy { .. }
                | Command::PreviewProposal { .. }
        );
        match request.command {
            Command::Preview { workspace, change } => {
                let mut tx = store::write_tx(&self.store.pool).await?;
                let hosted = load(&mut tx, &workspace).await?;
                self.workspace_admission_on(&mut tx, actor, &hosted).await?;
                self.workspace_permission_on(&mut tx, actor, &hosted, change_permission(&change))
                    .await?;
                outcome.data = Some(
                    self.workspace_preview_on(&mut tx, &hosted, &change, actor)
                        .await?,
                );
                tx.rollback().await?;
            }
            Command::PreviewProposal {
                workspace,
                proposal,
            } => {
                let mut tx = store::write_tx(&self.store.pool).await?;
                let hosted = load(&mut tx, &workspace).await?;
                self.workspace_admission_on(&mut tx, actor, &hosted).await?;
                self.workspace_permission_on(&mut tx, actor, &hosted, "workspace:access_control")
                    .await?;
                let row:Option<(String,String,Option<String>)> = store::sqlx::query_as("SELECT actor_uid,change,author_admission FROM workspace_change WHERE workspace_uid=? AND uid=? AND status='pending'").bind(&workspace).bind(&proposal).fetch_optional(&mut *tx).await?;
                let (author, raw, admission) =
                    row.ok_or_else(|| invalid("Pending proposal unavailable"))?;
                crate::login::require_workspace_origin_on(&mut tx, &author, admission.as_deref())
                    .await?;
                let change: Change = serde_json::from_str(&raw)?;
                let before = crate::access::policy_graph_on(&mut tx, &[])
                    .await
                    .map_err(crate::record_policy::denied)?;
                let readable_before = self.readable_on(&mut tx, actor, &before).await?;
                if !matches!(change, Change::RestoreRecord { .. })
                    && !change_records(&change).is_subset(&readable_before)
                {
                    return Err(invalid("Proposal references unavailable Records"));
                }
                let author = (!author.is_empty()).then_some(author.as_str());
                self.workspace_admission_on(&mut tx, author, &hosted)
                    .await?;
                self.workspace_permission_on(&mut tx, author, &hosted, change_permission(&change))
                    .await?;
                let mut preview = self
                    .workspace_preview_on(&mut tx, &hosted, &change, author)
                    .await?;
                let after = crate::access::policy_graph_on(&mut tx, &[])
                    .await
                    .map_err(crate::record_policy::denied)?;
                let readable_after = self.readable_on(&mut tx, actor, &after).await?;
                if preview["consequences"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|effect| {
                        effect["record"].as_str().is_none_or(|record| {
                            if effect["deleted"] == true {
                                !readable_before.contains(record)
                            } else {
                                !readable_after.contains(record)
                            }
                        })
                    })
                {
                    return Err(invalid("Proposal consequences include unavailable Records"));
                }
                if let Some(effects) = preview["consequences"].as_array_mut() {
                    for effect in effects {
                        redact_assertions(effect, &readable_before, &readable_after);
                    }
                }
                preview["proposal"] = json!(proposal);
                preview["original_actor"] = json!(author);
                tx.rollback().await?;
                outcome.data = Some(preview);
            }
            Command::PreviewPolicy { workspace, policy } => {
                let mut tx = self.store.pool.begin().await?;
                let hosted = load(&mut tx, &workspace).await?;
                self.workspace_admission_on(&mut tx, actor, &hosted).await?;
                self.workspace_permission_on(&mut tx, actor, &hosted, "workspace:access_control")
                    .await?;
                let mut proposed = hosted.clone();
                apply(&mut proposed, &Change::Policy { policy })?;
                let graph = crate::access::policy_graph_on(&mut tx, &[])
                    .await
                    .map_err(crate::record_policy::denied)?;
                if !policy_records(&proposed.policy).is_empty()
                    && !policy_records(&proposed.policy)
                        .is_subset(&self.readable_on(&mut tx, actor, &graph).await?)
                {
                    return Err(invalid("Workspace policy references unavailable Records"));
                }
                self.workspace_controls_ceiling_on(&mut tx, &proposed)
                    .await?;
                self.workspace_layout_ceiling(&mut tx, &proposed).await?;
                let may_list = self
                    .require_permission_on(&mut tx, actor, "user:read")
                    .await
                    .is_ok();
                let graph = crate::access::policy_graph_on(&mut tx, &[])
                    .await
                    .map_err(crate::record_policy::denied)?;
                let exposed = hosted.layout.records();
                let mut affected = Vec::new();
                if may_list {
                    let people: Vec<String> = store::sqlx::query_scalar("SELECT a.person_uid FROM person_access a JOIN record p ON p.uid=a.person_uid WHERE p.kind='person' AND p.deleted_at IS NULL ORDER BY a.person_uid LIMIT 128").fetch_all(&mut *tx).await?;
                    for person in people {
                        let reads = exposed.is_empty()
                            || self
                                .readable_on(&mut tx, Some(&person), &graph)
                                .await
                                .is_ok_and(|readable| exposed.is_subset(&readable));
                        let before = reads
                            && self
                                .workspace_required_on(&mut tx, Some(&person), &hosted)
                                .await
                                .is_ok();
                        let after = reads
                            && self
                                .workspace_required_on(&mut tx, Some(&person), &proposed)
                                .await
                                .is_ok();
                        let edit_before = before
                            && self
                                .workspace_permission_on(
                                    &mut tx,
                                    Some(&person),
                                    &hosted,
                                    "workspace:update",
                                )
                                .await
                                .is_ok();
                        let edit_after = after
                            && self
                                .workspace_permission_on(
                                    &mut tx,
                                    Some(&person),
                                    &proposed,
                                    "workspace:update",
                                )
                                .await
                                .is_ok();
                        if before != after || edit_before != edit_after {
                            affected.push(json!({"person":person,"view_before":before,"view_after":after,"edit_before":edit_before,"edit_after":edit_after}));
                        }
                    }
                }
                tx.commit().await?;
                outcome.data = Some(
                    json!({"base_revision":hosted.revision,"affected_actors":affected,"actor_catalogue_limit":128,"actor_catalogue_visible":may_list,"suspended_areas":proposed.layout.disabled_areas,"explanation":"Every existing Area is suspended until its behavior is reviewed under the new policy."}),
                );
            }
            Command::SaveDraft {
                uid,
                host,
                workspace,
                base_revision,
                change,
            } => {
                self.require_permission(actor, "workspace:update").await?;
                let raw = serde_json::to_string(&change)?;
                if !nucleus::valid_uid(&uid, "draft")
                    || !nucleus::valid_uid(&workspace, "workspace")
                    || host
                        .as_ref()
                        .is_some_and(|host| !nucleus::valid_uid(host, "r"))
                    || base_revision <= 0
                    || raw.len() > 65536
                {
                    return Err(invalid(
                        "Use a bounded draft with its original host, workspace and base revision",
                    ));
                }
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "workspace:update")
                    .await?;
                let owner = actor.unwrap_or("");
                let count: i64 = store::sqlx::query_scalar(
                    "SELECT COUNT(*) FROM workspace_draft WHERE actor_uid=? AND uid!=?",
                )
                .bind(owner)
                .bind(&uid)
                .fetch_one(&mut *tx)
                .await?;
                if count >= 128 {
                    return Err(invalid("The local draft limit is 128"));
                }
                let saved = store::sqlx::query("INSERT INTO workspace_draft(uid,actor_uid,host_uid,workspace_uid,base_revision,change,saved_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(uid) DO UPDATE SET change=excluded.change,saved_at=excluded.saved_at WHERE workspace_draft.actor_uid=excluded.actor_uid AND workspace_draft.host_uid IS excluded.host_uid AND workspace_draft.workspace_uid=excluded.workspace_uid AND workspace_draft.base_revision=excluded.base_revision").bind(&uid).bind(owner).bind(host).bind(workspace).bind(base_revision).bind(raw).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *tx).await?.rows_affected();
                if saved != 1 {
                    return Err(invalid("Draft identity already belongs to another context"));
                }
                tx.commit().await?;
                outcome.created = Some(uid);
                outcome.data = Some(json!({"state":"draft_saved","base_revision":base_revision}));
            }
            Command::DiscardDraft { uid } => {
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_login_on(&mut tx).await?;
                store::sqlx::query("DELETE FROM workspace_draft WHERE uid=? AND actor_uid=?")
                    .bind(uid)
                    .bind(actor.unwrap_or(""))
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                outcome.data = Some(json!({"state":"draft_discarded"}));
            }
            Command::Drafts => {
                let rows: Vec<(String,Option<String>,String,i64,String)> = store::sqlx::query_as("SELECT uid,host_uid,workspace_uid,base_revision,change FROM workspace_draft WHERE actor_uid=? ORDER BY saved_at DESC,uid DESC LIMIT 128").bind(actor.unwrap_or("")).fetch_all(&self.store.pool).await?;
                let mut drafts = Vec::new();
                for (uid, host, workspace, base_revision, change) in rows {
                    drafts.push(json!({"uid":uid,"host":host,"workspace":workspace,"base_revision":base_revision,"change":serde_json::from_str::<Value>(&change)?}));
                }
                outcome.data = Some(json!({"drafts":drafts}));
            }
            Command::List => {
                self.require_permission(actor, "workspace:read").await?;
                let rows: Vec<(String, String, i64)> = store::sqlx::query_as(
                    "SELECT uid, name, revision FROM shared_workspace ORDER BY name, uid LIMIT 257",
                )
                .fetch_all(&self.store.pool)
                .await?;
                if rows.len() > 256 {
                    return Err(invalid("Workspace catalogue exceeds 256 entries"));
                }
                let mut tx = self.store.pool.begin().await?;
                let mut available = Vec::new();
                for (uid, name, revision) in rows {
                    let hosted = load(&mut tx, &uid).await?;
                    if self
                        .workspace_permission_on(&mut tx, actor, &hosted, "workspace:read")
                        .await
                        .is_err()
                    {
                        continue;
                    }
                    let mut required = true;
                    for capability in &hosted.policy.required_capabilities {
                        required &= self
                            .require_permission_on(&mut tx, actor, capability)
                            .await
                            .is_ok();
                    }
                    if required {
                        available.push(json!({"uid":uid,"name":name,"revision":revision}));
                    }
                }
                tx.commit().await?;
                outcome.data = Some(json!({"workspaces":available}));
            }
            Command::Capabilities => {
                self.require_permission(actor, "workspace:read").await?;
                outcome.data = Some(
                    json!({"components":["text","record","area","button","composition"],"actions":["workspace-record","edit-record-text","set-slug","set-unit","set-extension","assert-record","refine-assertion","set-identity"],"limitations":["Native domain Sands, calls, background automation and external effects use their dedicated controls.","Nested Areas need a top-level placement and an explicitly reviewed recipe.","Reserved extensions and managed Record kinds use their domain workflows."],"client":Client::default()}),
                );
            }
            Command::ValidateImport {
                name,
                policy,
                layout,
            } => {
                let mut tx = store::write_tx(&self.store.pool).await?;
                let report = self
                    .workspace_import_on(&mut tx, actor, &name, policy, layout)
                    .await?;
                tx.rollback().await?;
                outcome.data = Some(report);
            }
            command @ (Command::Create { .. } | Command::Publish { .. }) => {
                let (name, policy, layout) = match command {
                    Command::Create { name, policy } => (name, policy, Layout::default()),
                    Command::Publish {
                        name,
                        policy,
                        layout,
                    } => (name, policy, serde_json::from_value::<Layout>(layout)?),
                    _ => unreachable!(),
                };
                let mut layout = layout;
                for element in &mut layout.elements {
                    element.component = adapters::portable(element.component.clone());
                }
                layout.validate()?;
                self.require_permission(actor, "workspace:create").await?;
                self.require_permission(actor, "workspace:access_control")
                    .await?;
                if !name_valid(&name) {
                    return Err(invalid("Use a workspace name of 1–80 characters"));
                }
                let policy: Policy = serde_json::from_value(policy)?;
                policy.validate()?;
                let uid = nucleus::new_uid("workspace");
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "workspace:create")
                    .await?;
                self.require_permission_on(&mut tx, actor, "workspace:access_control")
                    .await?;
                let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM shared_workspace")
                    .fetch_one(&mut *tx)
                    .await?;
                if count >= 256 {
                    return Err(invalid("This Organ already hosts 256 workspaces"));
                }
                let graph = crate::access::policy_graph_on(&mut tx, &[])
                    .await
                    .map_err(crate::record_policy::denied)?;
                if !policy_records(&policy).is_empty()
                    && !policy_records(&policy)
                        .is_subset(&self.readable_on(&mut tx, actor, &graph).await?)
                {
                    return Err(invalid("Workspace policy references unavailable Records"));
                }
                self.workspace_layout_ceiling(
                    &mut tx,
                    &Hosted {
                        uid: uid.clone(),
                        name: name.clone(),
                        revision: 1,
                        policy: policy.clone(),
                        layout: layout.clone(),
                    },
                )
                .await?;
                let hosted = Hosted {
                    uid: uid.clone(),
                    name: name.clone(),
                    revision: 1,
                    policy: policy.clone(),
                    layout: layout.clone(),
                };
                self.workspace_admission_on(&mut tx, actor, &hosted).await?;
                self.workspace_controls_ceiling_on(&mut tx, &hosted).await?;
                for changes in layout.areas.values() {
                    self.workspace_recipe_ceiling(&mut tx, &policy, changes)
                        .await?;
                }
                store::sqlx::query("INSERT INTO shared_workspace(uid,name,revision,policy,layout) VALUES(?,?,1,?,?)").bind(&uid).bind(name.trim()).bind(serde_json::to_string(&policy)?).bind(serde_json::to_string(&layout)?).execute(&mut *tx).await?;
                tx.commit().await?;
                outcome.created = Some(uid);
            }
            Command::Inspect {
                workspace,
                permitted_view,
            } => {
                outcome.data = Some(
                    self.workspace_view(actor, &workspace, &request.client, permitted_view)
                        .await?,
                )
            }
            Command::History { workspace, before } => {
                self.workspace_view(actor, &workspace, &request.client, false)
                    .await?;
                self.require_permission(actor, "workspace:access_control")
                    .await?;
                let mut tx = self.store.pool.begin().await?;
                let hosted = load(&mut tx, &workspace).await?;
                self.workspace_permission_on(&mut tx, actor, &hosted, "workspace:access_control")
                    .await?;
                let graph = crate::access::policy_graph_on(&mut tx, &[])
                    .await
                    .map_err(crate::record_policy::denied)?;
                let readable = self.readable_on(&mut tx, actor, &graph).await?;
                tx.commit().await?;
                let rows: Vec<(String,String,i64,Option<i64>,String,String,Option<String>,String)> = store::sqlx::query_as("SELECT uid,actor_uid,base_revision,applied_revision,change,status,reviewed_by,created_at FROM workspace_change WHERE workspace_uid = ? AND (? IS NULL OR uid < ?) ORDER BY uid DESC LIMIT 64").bind(workspace).bind(&before).bind(&before).fetch_all(&self.store.pool).await?;
                let mut next_cursor = (rows.len() == 64).then(|| rows.last().unwrap().0.clone());
                let mut last_scanned = None;
                let mut bytes = 0;
                let mut changes = Vec::new();
                for (uid, author, base, revision, change, state, reviewer, at) in rows {
                    if !changes.is_empty() && bytes + change.len() + 1024 > 1048576 {
                        next_cursor = last_scanned;
                        break;
                    }
                    last_scanned = Some(uid.clone());
                    let size = change.len() + 1024;
                    let parsed: Change = serde_json::from_str(&change)?;
                    if actor.is_some() && !change_records(&parsed).is_subset(&readable) {
                        continue;
                    }
                    if let Change::CreateRecord { draft, .. } = &parsed
                        && let Some(actor) = actor
                    {
                        if let Some(selector) =
                            protein::read_rules::effective_predicate(&self.store, actor).await?
                        {
                            let mut family = std::collections::HashSet::new();
                            for assertion in &draft.assertions {
                                family.extend(
                                    store::concepts::ancestors_including(
                                        &self.store.pool,
                                        &assertion.predicate,
                                    )
                                    .await?,
                                );
                            }
                            if draft.matches(&selector, &family, draft.validate()?) != Some(true) {
                                continue;
                            }
                        }
                    }
                    bytes += size;
                    changes.push(json!({"uid":uid,"actor":author,"base_revision":base,"revision":revision,"change":parsed,"status":state,"reviewed_by":reviewer,"created_at":at}));
                }
                outcome.data = Some(json!({"changes":changes,"next_cursor":next_cursor}));
            }
            Command::Delete {
                workspace,
                expected_revision,
            } => {
                self.require_permission(actor, "workspace:delete").await?;
                self.workspace_view(actor, &workspace, &request.client, false)
                    .await?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "workspace:delete")
                    .await?;
                let hosted = load(&mut tx, &workspace).await?;
                self.workspace_permission_on(&mut tx, actor, &hosted, "workspace:delete")
                    .await?;
                self.workspace_admission_on(&mut tx, actor, &hosted).await?;
                if store::sqlx::query("DELETE FROM shared_workspace WHERE uid = ? AND revision = ?")
                    .bind(workspace)
                    .bind(expected_revision)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected()
                    != 1
                {
                    return Err(stale());
                }
                tx.commit().await?;
            }
            command @ (Command::Propose { .. } | Command::Review { .. }) => {
                outcome = self.workspace_change(command, actor, &payload).await?
            }
        }
        if changed {
            self.notify_query_changed();
        }
        Ok(outcome)
    }

    async fn workspace_preview_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        hosted: &Hosted,
        change: &Change,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let before = crate::access::policy_graph_on(tx, &[])
            .await
            .map_err(crate::record_policy::denied)?;
        let mut proposed = hosted.clone();
        apply(&mut proposed, change)?;
        if matches!(change, Change::Policy { .. })
            && !policy_records(&proposed.policy).is_empty()
            && !policy_records(&proposed.policy)
                .is_subset(&self.readable_on(tx, actor, &before).await?)
        {
            return Err(invalid("Workspace policy references unavailable Records"));
        }
        if let Change::Area { changes, .. } = &change {
            self.workspace_recipe_ceiling(tx, &proposed.policy, changes)
                .await?;
        }
        let resolved = adapters::resolve(&hosted.layout, change)?;
        self.workspace_record_change(tx, &proposed, &resolved, actor, nucleus::execution::now())
            .await?;
        self.workspace_effects(
            tx,
            hosted,
            &proposed,
            change,
            actor,
            nucleus::execution::now(),
        )
        .await?;
        self.workspace_controls_ceiling_on(tx, &proposed).await?;
        self.workspace_layout_ceiling(tx, &proposed).await?;
        self.workspace_admission_on(tx, actor, &proposed).await?;
        let after = crate::access::policy_graph_on(tx, &[])
            .await
            .map_err(crate::record_policy::denied)?;
        let mut consequences = Vec::new();
        for record in &after.records {
            let old = before.records.iter().find(|old| old.uid == record.uid);
            let assertions_before = before
                .assertions
                .iter()
                .filter(|assertion| assertion.subject_uid == record.uid)
                .collect::<Vec<_>>();
            let assertions_after = after
                .assertions
                .iter()
                .filter(|assertion| assertion.subject_uid == record.uid)
                .collect::<Vec<_>>();
            if old != Some(record) || assertions_before != assertions_after {
                let content = |record: Option<&protein::authority::RecordState>| {
                    record.and_then(|record| record.content.as_ref()).map(|content| json!({"head":content.head,"body":content.body,"slug":content.slug,"quantity":content.quantity,"unit":content.unit_uid,"extensions":content.extensions.iter().map(|(property,value)| json!({"namespace":property.namespace,"field":property.field,"value":value})).collect::<Vec<_>>()}))
                };
                let assertions = |assertions: &[&protein::authority::AssertionState]| {
                    assertions.iter().map(|assertion| json!({"uid":assertion.uid,"predicate":assertion.predicate_uid,"object":assertion.object_uid,"role":assertion.role,"quantity":assertion.quantity,"unit":assertion.unit_uid})).collect::<Vec<_>>()
                };
                consequences.push(json!({"record":record.uid,"created":old.is_none(),"deleted":record.deleted,"before":content(old),"after":content(Some(record)),"quantity_before":old.and_then(|record| record.content.as_ref().map(|content| content.quantity)),"quantity_after":record.content.as_ref().map(|content| content.quantity),"assertions_before":assertions_before.len(),"assertions_after":assertions_after.len(),"assertion_details_before":assertions(&assertions_before),"assertion_details_after":assertions(&assertions_after)}));
            }
        }
        let readable_before = self.readable_on(tx, actor, &before).await?;
        let readable_after = self.readable_on(tx, actor, &after).await?;
        for effect in &mut consequences {
            redact_assertions(effect, &readable_before, &readable_after);
        }
        Ok(
            json!({"base_revision":hosted.revision,"requires_review":sensitive(hosted, change),"consequences":consequences,"layout_before":hosted.layout,"layout_after":proposed.layout,"explanation":"Preview only. The host rechecks the original Actor and current state when accepting the operation."}),
        )
    }

    async fn workspace_change(
        &self,
        command: Command,
        actor: Option<&str>,
        payload: &str,
    ) -> Result<ActionOutcome, EngineError> {
        let (workspace, request_id) = match &command {
            Command::Propose {
                workspace,
                request_id,
                ..
            }
            | Command::Review {
                workspace,
                request_id,
                ..
            } => (workspace, request_id),
            _ => unreachable!(),
        };
        if !nucleus::valid_uid(request_id, "request") {
            return Err(invalid("Use a durable workspace request UID"));
        }
        let owner = actor.unwrap_or("");
        let mut read_tx = self.store.pool.begin().await?;
        let current = load(&mut read_tx, workspace).await?;
        read_tx.commit().await?;
        self.workspace_admission(actor, &current, true).await?;
        let permission = match &command {
            Command::Review { .. } => "workspace:access_control",
            Command::Propose { change, .. } => change_permission(change),
            _ => unreachable!(),
        };
        self.require_permission(actor, permission).await?;
        let _import = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_login_on(&mut tx).await?;
        if let Some((original, result)) = store::sqlx::query_as::<_, (String, String)>("SELECT payload,result FROM workspace_receipt WHERE workspace_uid=? AND actor_uid=? AND request_id=?").bind(workspace).bind(owner).bind(request_id).fetch_optional(&mut *tx).await? {
            if original != payload { return Err(invalid("Workspace request identity was reused for different content")); }
            return Ok(ActionOutcome { data: Some(serde_json::from_str(&result)?), ..Default::default() });
        }
        let hosted = load(&mut tx, workspace).await?;
        if hosted.revision != current.revision {
            return Err(stale());
        }
        self.workspace_admission_on(&mut tx, actor, &hosted).await?;
        self.workspace_permission_on(&mut tx, actor, &hosted, permission)
            .await?;
        let now = nucleus::execution::now();
        let mut proposed = hosted.clone();
        let (change, author, proposal, approved, apply_now) = match &command {
            Command::Propose {
                change,
                base_revision,
                ..
            } => {
                if *base_revision < 1 || *base_revision > hosted.revision {
                    return Err(invalid("Choose an existing workspace base revision"));
                }
                change_valid(change)?;
                let pending = sensitive(&hosted, change)
                    || (*base_revision != hosted.revision
                        && !independent_since(&mut tx, &hosted, *base_revision, change).await?);
                if let Err(error) = apply(&mut proposed, change) {
                    if !pending || *base_revision == hosted.revision {
                        return Err(error);
                    }
                    proposed = hosted.clone();
                }
                let uid = nucleus::new_uid("change");
                let pending_count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM workspace_change WHERE workspace_uid=? AND status='pending'").bind(workspace).fetch_one(&mut *tx).await?;
                if pending_count >= 128 {
                    return Err(invalid("Review the pending changes before submitting more"));
                }
                let admission = crate::login::capture_workspace_origin(actor)?;
                store::sqlx::query("INSERT INTO workspace_change(uid,workspace_uid,actor_uid,base_revision,change,status,created_at,author_admission) VALUES(?,?,?,?,?,?,?,?)").bind(&uid).bind(workspace).bind(owner).bind(base_revision).bind(serde_json::to_string(change)?).bind(if pending { "pending" } else { "applied" }).bind(now.to_rfc3339()).bind(admission).execute(&mut *tx).await?;
                (change.clone(), owner.to_owned(), uid, false, !pending)
            }
            Command::Review {
                proposal,
                expected_revision,
                approve,
                ..
            } => {
                if *expected_revision != hosted.revision {
                    return Err(stale());
                }
                let row: Option<(String,String,String,Option<String>)> = store::sqlx::query_as("SELECT actor_uid,change,status,author_admission FROM workspace_change WHERE uid=? AND workspace_uid=?").bind(proposal).bind(workspace).fetch_optional(&mut *tx).await?;
                let (author, raw, state, admission) =
                    row.ok_or_else(|| invalid("Proposal unavailable"))?;
                if state != "pending" {
                    return Err(invalid("This proposal has already been reviewed"));
                }
                let change: Change = serde_json::from_str(&raw)?;
                if *approve {
                    crate::login::require_workspace_origin_on(
                        &mut tx,
                        &author,
                        admission.as_deref(),
                    )
                    .await?;
                    apply(&mut proposed, &change)?;
                }
                (change, author, proposal.clone(), true, *approve)
            }
            _ => unreachable!(),
        };
        let mut facts = Vec::new();
        if matches!(
            change,
            Change::Add { .. } | Change::Configure { .. } | Change::Area { .. }
        ) {
            self.workspace_admission_on(&mut tx, actor, &proposed)
                .await?;
        }
        if apply_now {
            if matches!(change, Change::Policy { .. }) {
                let graph = crate::access::policy_graph_on(&mut tx, &[])
                    .await
                    .map_err(crate::record_policy::denied)?;
                let author = (!author.is_empty()).then_some(author.as_str());
                if !policy_records(&proposed.policy).is_empty()
                    && !policy_records(&proposed.policy)
                        .is_subset(&self.readable_on(&mut tx, author, &graph).await?)
                {
                    return Err(invalid("Workspace policy references unavailable Records"));
                }
            }
            if let Change::Area { changes, .. } = &change {
                self.workspace_recipe_ceiling(&mut tx, &proposed.policy, changes)
                    .await?;
            }
            if !author.is_empty() {
                self.workspace_admission_on(&mut tx, Some(&author), &hosted)
                    .await?;
                self.workspace_permission_on(
                    &mut tx,
                    Some(&author),
                    &hosted,
                    change_permission(&change),
                )
                .await?;
            }
            let resolved = adapters::resolve(&hosted.layout, &change)?;
            facts = self
                .workspace_record_change(
                    &mut tx,
                    &proposed,
                    &resolved,
                    if author.is_empty() {
                        None
                    } else {
                        Some(author.as_str())
                    },
                    now,
                )
                .await?;
            if matches!(change, Change::Add { .. } | Change::Configure { .. }) {
                self.workspace_controls_ceiling_on(&mut tx, &proposed)
                    .await?;
            }
            self.workspace_layout_ceiling(&mut tx, &proposed).await?;
            self.workspace_admission_on(
                &mut tx,
                if author.is_empty() {
                    None
                } else {
                    Some(author.as_str())
                },
                &proposed,
            )
            .await?;
            facts.extend(
                self.workspace_effects(
                    &mut tx,
                    &hosted,
                    &proposed,
                    &change,
                    if author.is_empty() {
                        None
                    } else {
                        Some(author.as_str())
                    },
                    now,
                )
                .await?,
            );
            let revision = hosted
                .revision
                .checked_add(1)
                .ok_or_else(|| invalid("Workspace revision exhausted"))?;
            store::sqlx::query("UPDATE shared_workspace SET name=?,revision=?,policy=?,layout=? WHERE uid=? AND revision=?").bind(&proposed.name).bind(revision).bind(serde_json::to_string(&proposed.policy)?).bind(serde_json::to_string(&proposed.layout)?).bind(workspace).bind(hosted.revision).execute(&mut *tx).await?;
            store::sqlx::query("UPDATE workspace_change SET status='applied',applied_revision=?,reviewed_by=? WHERE uid=?").bind(revision).bind(if approved { Some(owner) } else { None }).bind(&proposal).execute(&mut *tx).await?;
        } else if approved {
            store::sqlx::query(
                "UPDATE workspace_change SET status='rejected',reviewed_by=? WHERE uid=?",
            )
            .bind(owner)
            .bind(&proposal)
            .execute(&mut *tx)
            .await?;
        }
        let data = json!({"proposal":proposal,"state":if apply_now { "applied" } else if approved { "rejected" } else { "pending" }, "revision":hosted.revision + i64::from(apply_now)});
        store::sqlx::query("INSERT INTO workspace_receipt(workspace_uid,actor_uid,request_id,payload,result) VALUES(?,?,?,?,?)").bind(workspace).bind(owner).bind(request_id).bind(payload).bind(serde_json::to_string(&data)?).execute(&mut *tx).await?;
        tx.commit().await?;
        let mut outcome = ActionOutcome {
            data: Some(data),
            ..Default::default()
        };
        for fact in facts {
            self.invalidate_record_doc(&fact.record_uid);
            outcome.facts.extend(self.publish_committed_fact(fact));
        }
        Ok(outcome)
    }
}

fn stale() -> EngineError {
    EngineError::Conflict {
        code: "workspace_changed",
        message: "The hosted workspace changed. Refresh and review the new revision.".into(),
    }
}

fn redact_assertions(effect: &mut Value, before: &BTreeSet<String>, after: &BTreeSet<String>) {
    for (key, readable) in [
        ("assertion_details_before", before),
        ("assertion_details_after", after),
    ] {
        for assertion in effect[key].as_array_mut().into_iter().flatten() {
            if assertion["object"]
                .as_str()
                .is_some_and(|object| !readable.contains(object))
            {
                assertion["object"] = json!("unavailable");
            }
        }
    }
}
