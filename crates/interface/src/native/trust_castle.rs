use crate::native::{
    actions::Action,
    cell_bridge::{CellMessage, ReceiveCell},
    sand_panel as panel,
};
use bevy::{math::DVec2, prelude::*, text::EditableText};
use nucleus::trust_analysis::{
    Identity,
    commands::{Command as Backend, ContactDraft, ContactReview},
    contacts::Audience,
    paths,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

mod analysis;
mod claims;
mod profile_claims;
mod recipe_controls;
mod root_controls;
mod serving_controls;
mod custody_controls;
mod remote_controls;
pub(crate) mod tests;
mod ui;
use ui::render;

pub struct TrustCastlePlugin;
impl Plugin for TrustCastlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (receive.after(ReceiveCell), capture, maintain, remote_controls::maintain).chain(),
        );
    }
}

#[derive(Clone, Default)]
struct Rule {
    scope: RuleScope,
    exclusion: bool,
    identities: String,
    known: i8,
    lower: String,
    upper: String,
    lower_inclusive: bool,
    upper_inclusive: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum RuleScope {
    #[default]
    Target,
    Intermediary,
    Issuer,
    Counterparty,
    Endpoint,
    Serving,
    CustodySender,
    CustodyReader,
    CustodyForwardPeer,
}

#[derive(Component)]
struct View {
    root: Entity,
    context: Value,
    person: String,
    contact: String,
    contact_organ: bool,
    issuer_organ: bool,
    public: bool,
    readers: String,
    term: String,
    revision: String,
    withdrawn: bool,
    as_organ: bool,
    import: String,
    anchor: String,
    anchor_organ: bool,
    depth: String,
    unilateral: bool,
    excluded: String,
    rules: Vec<Rule>,
    result: Value,
    review: Option<(ContactReview, nucleus::karma::CanonicalHash)>,
    pending: Option<(String, u64)>,
    generation: u64,
    bootstrap: bool,
    analyze_after_context: bool,
    notice: String,
    analysis: analysis::Draft,
    claims: claims::Draft,
    recipes: recipe_controls::Draft,
    roots: root_controls::Draft,
    serving: serving_controls::Draft,
    custody: custody_controls::Draft,
    remote: remote_controls::Draft,
}

fn clear_scope(view: &mut View) {
    view.analysis = analysis::Draft::default();
    view.claims = claims::Draft::default();
    view.recipes = recipe_controls::Draft::default();
    view.roots = root_controls::Draft::default();
    view.serving = serving_controls::Draft::default();
    view.custody = custody_controls::Draft::default();
    view.remote = remote_controls::Draft::default();
    view.analyze_after_context = false;
    view.contact.clear();
    view.anchor.clear();
    view.readers.clear();
    view.import.clear();
    view.excluded.clear();
    view.rules.clear();
    view.review = None;
    view.result = Value::Null;
    view.pending = None;
    view.as_organ = false;
    view.issuer_organ = false;
    view.public = false;
    view.withdrawn = false;
    view.revision = "0".into();
    view.term = "30".into();
    view.depth = "3".into();
    view.unilateral = false;
    view.contact_organ = false;
    view.anchor_organ = false;
    view.generation = view.generation.wrapping_add(1);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    Custody(custody_controls::Field),
    Serving(serving_controls::Field),
    OrganRoot(root_controls::Field),
    Recipe(recipe_controls::Field),
    Claim(claims::Field),
    Query(analysis::Field),
    Person,
    Contact,
    Readers,
    Term,
    Revision,
    Import,
    Anchor,
    Depth,
    Excluded,
    RuleIds(usize),
    RuleLower(usize),
    RuleUpper(usize),
}
#[derive(Component)]
struct Input {
    owner: Entity,
    key: Key,
    observed: String,
}

fn label(world: &mut World, parent: Entity, text: &str) {
    crate::native::edit_mode::label(world, parent, text, 13.0);
}
fn field(world: &mut World, parent: Entity, owner: Entity, key: Key, caption: &str, value: &str) {
    let entity = panel::field(world, parent, caption, value);
    if matches!(key, Key::Import | Key::Claim(claims::Field::Import) | Key::OrganRoot(root_controls::Field::Document))
        && let Some(mut input) = world.get_mut::<EditableText>(entity)
    {
        input.max_characters = Some(if matches!(key,Key::OrganRoot(_)) {16384} else {32768});
    }
    world.entity_mut(entity).insert(Input {
        owner,
        key,
        observed: value.into(),
    });
}

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    let body = panel::frame(world, sand, "Trust Castle");
    world.get_mut::<Node>(body).unwrap().overflow = Overflow::scroll_y();
    crate::native::scroll_sand::attach(world, body);
    world.entity_mut(body).insert(View {
        root,
        context: Value::Null,
        person: String::new(),
        contact: String::new(),
        contact_organ: false,
        issuer_organ: false,
        public: false,
        readers: String::new(),
        term: "30".into(),
        revision: "0".into(),
        withdrawn: false,
        as_organ: false,
        import: String::new(),
        anchor: String::new(),
        anchor_organ: false,
        depth: "3".into(),
        unilateral: false,
        excluded: String::new(),
        rules: Vec::new(),
        result: Value::Null,
        review: None,
        pending: None,
        generation: 0,
        bootstrap: false,
        analyze_after_context: false,
        notice: "Choose the Person whose evidence you want to manage.".into(),
        analysis: analysis::Draft::default(),
        claims: claims::Draft::default(),
        recipes: recipe_controls::Draft::default(),
        roots: root_controls::Draft::default(),
        serving: serving_controls::Draft::default(),
        custody: custody_controls::Draft::default(),
        remote: remote_controls::Draft::default(),
    });
    render(world, body);
    body
}

#[derive(Clone)]
pub(crate) struct Open {
    pub root: Entity,
    pub subject: Option<Identity>,
}

#[derive(Clone)]
struct OpenHere;
impl Action for OpenHere {
    fn apply(&self, world: &mut World, entity: Entity) {
        let mut cursor = Some(entity);
        while let Some(entity) = cursor {
            if world
                .get::<crate::native::workspace::Workspaces>(entity)
                .is_some()
            {
                Open {
                    root: entity,
                    subject: None,
                }
                .apply(world, entity);
                return;
            }
            if let Some(root) = world.get::<crate::native::sand::InBox>(entity) {
                Open {
                    root: root.0,
                    subject: None,
                }
                .apply(world, entity);
                return;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

pub(crate) fn entry_controls(world: &mut World, parent: Entity) {
    panel::button(world, parent, parent, "Trust and contact paths", OpenHere);
}
impl Action for Open {
    fn apply(&self, world: &mut World, _: Entity) {
        open_view(world, self.root, self.subject.as_ref());
    }
}

fn open_view(world: &mut World, root: Entity, subject: Option<&Identity>) -> Option<Entity> {
        let workspace = world
            .get::<crate::native::workspace::Workspaces>(root)
            .map_or(1, |value| value.active);
        let placement = world
            .get::<crate::native::canvas::CanvasView>(root)
            .map_or(DVec2::ZERO, |view| view.center);
        let sand = crate::native::sand_store::spawn_sand(
            world,
            root,
            workspace,
            crate::native::sand_store::SandKind::Trust,
            "Trust",
            placement,
        );
        if let Some(body) = world
            .get::<crate::native::sand_store::StoredSand>(sand)
            .and_then(|sand| sand.content)
        {
            if let Some(subject) = subject {
                let mut view = world.get_mut::<View>(body).unwrap();
                view.contact = subject.uid().into();
                view.contact_organ = matches!(subject, Identity::Organ(_));
                view.analysis.subject = subject.uid().into();
                view.analysis.organ = matches!(subject, Identity::Organ(_));
                view.analyze_after_context = true;
            }
            render(world, body);
            return Some(body);
        }
        None
}

pub(crate) fn claim_controls(world: &mut World, parent: Entity, document: &nucleus::trust_analysis::claims::Document) {
    panel::button(world, parent, parent, "Inspect and independently check this claim", profile_claims::OpenSnapshot(document.clone()));
}

pub(crate) fn organ_root_controls(world:&mut World,parent:Entity,subject:&str) {
    panel::button(world,parent,parent,"Compare or recover Organ owner identity",root_controls::Open(subject.into()));
}

pub(crate) fn evidence_serving_controls(world:&mut World,parent:Entity,record:&str,kind:nucleus::trust_analysis::serving::SourceKind) {
    panel::button(world,parent,parent,"Review minimal evidence sharing",serving_controls::Open {source:record.into(),kind,scope:nucleus::trust_analysis::serving::Scope::Fixed});
}

pub(crate) fn record_controls(
    world: &mut World,
    parent: Entity,
    binding: &crate::native::protein_area::RecordBinding,
    kind: &str,
) {
    let subject = match kind {
        "person" => Identity::Person(binding.uid.clone()),
        "organ" => Identity::Organ(binding.uid.clone()),
        _ => return,
    };
    subject_controls(world, parent, binding.area, subject);
}

pub(crate) fn subject_controls(world: &mut World, parent: Entity, context: Entity, subject: Identity) {
    let mut cursor = Some(context);
    while let Some(entity) = cursor {
        if world
            .get::<crate::native::workspace::Workspaces>(entity)
            .is_some()
        {
            panel::button(
                world,
                parent,
                parent,
                "Inspect permitted Transfer experience",
                Open {
                    root: entity,
                    subject: Some(subject),
                },
            );
            return;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
}

#[derive(Clone)]
enum Command {
    Custody(custody_controls::UiCommand),
    Remote(remote_controls::UiCommand),
    Serving(serving_controls::UiCommand),
    OrganRoot(root_controls::UiCommand),
    Recipe(recipe_controls::UiCommand),
    Claim(claims::UiCommand),
    Analyze,
    SaveQuery,
    LoadQuery(String),
    ListQueries(Option<String>),
    QueryToggle(analysis::Toggle),
    QueryMeasurement(Option<nucleus::transfer::receipt::Measurement>),
    ResolveMeasurement,
    QueryPage(usize),
    PreviewIdentity,
    AcceptIdentity,
    Context,
    Person(String),
    Contact(Identity),
    Toggle(&'static str),
    Preview,
    Confirm,
    Import,
    List(Option<String>),
    Inspect(String),
    Record(String),
    Export(String),
    Revise(Value, bool),
    Search,
    Copy,
    AddRule(RuleScope, bool),
    RemoveRule(usize),
    Known(usize),
    Bound(usize, bool),
    Clear,
}

fn identities(value: &str) -> Result<Vec<Identity>, String> {
    let mut result = BTreeSet::new();
    for token in value
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|token| !token.is_empty())
    {
        let (kind, uid) = token
            .split_once(':')
            .ok_or("Use person:r_… or organ:r_… identities")?;
        let identity = match kind {
            "person" => Identity::Person(uid.into()),
            "organ" => Identity::Organ(uid.into()),
            _ => return Err("Use person: or organ: before each identity".into()),
        };
        identity.validate()?;
        result.insert(identity);
    }
    Ok(result.into_iter().collect())
}

fn draft(view: &View) -> Result<ContactDraft, String> {
    let issuer = if view.issuer_organ {
        Identity::Organ(view.context["organ"].as_str().unwrap_or_default().into())
    } else {
        Identity::Person(view.person.clone())
    };
    let contact = if view.contact_organ {
        Identity::Organ(view.contact.clone())
    } else {
        Identity::Person(view.contact.clone())
    };
    let days: u32 = view.term.parse().map_err(|_| "Choose 1–366 days")?;
    if days == 0 || days > 366 {
        return Err("Choose 1–366 days".into());
    }
    let mut viewers = identities(&view.readers)?;
    if !view.public {
        viewers.push(Identity::Person(view.person.clone()));
        viewers.sort();
        viewers.dedup();
    }
    Ok(ContactDraft {
        issuer,
        contact,
        audience: if view.public {
            Audience::Public
        } else {
            Audience::Selected { viewers }
        },
        lifetime_seconds: days * 86400,
        expected_revision: view
            .revision
            .parse()
            .map_err(|_| "Choose the current nonnegative revision")?,
        withdrawn: view.withdrawn,
        key_id: String::new(),
        public_key: String::new(),
    })
}

fn path_policy(view: &View) -> Result<paths::Policy, String> {
    let anchor = if view.anchor_organ {
        Identity::Organ(view.anchor.clone())
    } else {
        Identity::Person(view.anchor.clone())
    };
    let mut policy = paths::Policy::for_anchor(anchor);
    policy.max_depth = view.depth.parse().map_err(|_| "Choose depth 1–6")?;
    policy.allow_unilateral = view.unilateral;
    policy.exclude_everywhere = identities(&view.excluded)?;
    for rule in &view.rules {
        if !matches!(rule.scope, RuleScope::Target | RuleScope::Intermediary) {
            continue;
        }
        let bound =
            |value: &str, inclusive| -> Result<Option<nucleus::visibility::Bound>, String> {
                if value.trim().is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(nucleus::visibility::Bound {
                        value: value
                            .parse()
                            .map_err(|_| "Proximity bounds must be nonnegative integers")?,
                        inclusive,
                    }))
                }
            };
        let condition = nucleus::trust_analysis::Condition {
            identities: identities(&rule.identities)?,
            known: match rule.known {
                1 => Some(true),
                2 => Some(false),
                _ => None,
            },
            lower: bound(&rule.lower, rule.lower_inclusive)?,
            upper: bound(&rule.upper, rule.upper_inclusive)?,
        };
        let filter = match rule.scope {
            RuleScope::Intermediary => &mut policy.intermediaries,
            RuleScope::Target => &mut policy.targets,
            _ => continue,
        };
        if rule.exclusion {
            filter.exclude.push(condition);
        } else {
            filter.include.push(condition);
        }
    }
    policy.validate()?;
    Ok(policy)
}

fn request(world: &mut World, owner: Entity, command: Backend) -> Result<(), String> {
    let view = world.get::<View>(owner).ok_or("Trust view closed")?;
    if view.pending.is_some() {
        return Err("Wait for the current request".into());
    }
    let id = nucleus::new_uid("trust-ui");
    let action = engine::actions::Action::TrustAnalysis {
        person: (!view.person.is_empty()).then(|| view.person.clone()),
        request_id: id.clone(),
        request: command,
    };
    send_action(world, owner, action)
}

fn send_action(
    world: &mut World,
    owner: Entity,
    action: engine::actions::Action,
) -> Result<(), String> {
    let view = world.get::<View>(owner).ok_or("Trust view closed")?;
    if view.pending.is_some() {
        return Err("Wait for the current request".into());
    }
    let generation = view.generation;
    let id = nucleus::new_uid("trust-ui");
    panel::send(
        world,
        owner,
        cell::ClientMessage::Act {
            id: id.clone(),
            action,
        },
    )?;
    world.get_mut::<View>(owner).unwrap().pending = Some((id, generation));
    Ok(())
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        capture(world);
        if let Self::Remote(command)=self {remote_controls::apply(world,owner,command);return;}
        if let Self::Serving(command)=self {serving_controls::apply(world,owner,command);render(world,owner);return;}
        if let Self::Custody(command)=self {custody_controls::apply(world,owner,command);render(world,owner);return;}
        if let Self::OrganRoot(command)=self {root_controls::apply(world,owner,command);render(world,owner);return;}
        if let Self::Recipe(command) = self {
            recipe_controls::apply(world, owner, command);
            return;
        }
        if let Self::Claim(command) = self {
            claims::apply(world, owner, command);
            return;
        }
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        if matches!(self, Self::ResolveMeasurement) {
            let action = engine::actions::Action::Vocabulary {
                request: nucleus::vocabulary::Command::InspectMeasurement {
                    concept: view.analysis.concept.clone(),
                    unit: (!view.analysis.unit.trim().is_empty())
                        .then(|| view.analysis.unit.clone()),
                },
            };
            if let Err(error) = send_action(world, owner, action) {
                world.get_mut::<View>(owner).unwrap().notice = error;
            }
            render(world, owner);
            return;
        }
        if matches!(self, Self::PreviewIdentity | Self::AcceptIdentity) {
            let request = (|| -> Result<nucleus::identity::Command, String> {
                let document =
                    nucleus::trust_analysis::contacts::Document::parse(view.import.as_bytes())?;
                let Identity::Person(subject) = document.statement.issuer else {
                    return Err(
                        "Use normal Organ identity controls to recognize an Organ root".into(),
                    );
                };
                let key = nucleus::identity::PersonKey {
                    subject,
                    key_id: document.key_id,
                    public_key: document.public_key,
                };
                if matches!(self, Self::PreviewIdentity) {
                    return Ok(nucleus::identity::Command::PreviewPersonKey { key });
                }
                let preview = &view.result["value"];
                if preview["kind"] != "person_key_preview"
                    || serde_json::to_value(&key).map_err(|error| error.to_string())?
                        != preview["key"]
                {
                    return Err(
                        "Compare a fresh identity preview for this exact imported key first".into(),
                    );
                }
                Ok(nucleus::identity::Command::AcceptPersonKey {
                    key,
                    expected_profile_revision: preview["profile_revision"]
                        .as_i64()
                        .ok_or("Missing profile revision")?,
                    expected_review: serde_json::from_value(preview["review_hash"].clone())
                        .map_err(|error| error.to_string())?,
                })
            })();
            let result = request.and_then(|request| {
                send_action(world, owner, engine::actions::Action::Identity { request })
            });
            if let Err(error) = result {
                world.get_mut::<View>(owner).unwrap().notice = error;
            }
            render(world, owner);
            return;
        }
        let backend = match self {
            Self::Analyze => analysis::build(view).map(|query| {
                Some(Backend::AnalyzeQuery {
                    query,
                    as_organ: view.as_organ,
                })
            }),
            Self::SaveQuery => analysis::build(view).map(|query| {
                Some(Backend::SaveQuery {
                    name: view.analysis.name.clone(),
                    query,
                })
            }),
            Self::LoadQuery(record) => Ok(Some(Backend::LoadQuery {
                record: record.clone(),
            })),
            Self::ListQueries(before) => Ok(Some(Backend::ListQueries {
                before: before.clone(),
            })),
            Self::Context => Ok(Some(Backend::Context)),
            Self::Preview => draft(view).map(|draft| Some(Backend::PreviewContact { draft })),
            Self::Confirm => view
                .review
                .clone()
                .ok_or("Preview the exact disclosure first".into())
                .map(|(review, expected_review)| {
                    Some(Backend::PublishContact {
                        review,
                        expected_review,
                    })
                }),
            Self::Import => Ok(Some(Backend::ImportContact {
                document: view.import.clone(),
                as_organ: view.as_organ,
            })),
            Self::List(before) => Ok(Some(Backend::ListContacts {
                before: before.clone(),
                as_organ: view.as_organ,
            })),
            Self::Inspect(record) => Ok(Some(Backend::InspectContact {
                record: record.clone(),
                as_organ: view.as_organ,
            })),
            Self::Export(record) => Ok(Some(Backend::ExportContact {
                record: record.clone(),
                as_organ: view.as_organ,
            })),
            Self::Search => path_policy(view).map(|policy| {
                Some(Backend::ContactPaths {
                    policy,
                    as_organ: view.as_organ,
                })
            }),
            _ => Ok(None),
        };
        match backend {
            Ok(Some(command)) => {
                let result = request(world, owner, command);
                if let Err(error) = result {
                    world.get_mut::<View>(owner).unwrap().notice = error;
                }
                render(world, owner);
                return;
            }
            Err(error) => {
                world.get_mut::<View>(owner).unwrap().notice = error;
                render(world, owner);
                return;
            }
            _ => {}
        }
        if matches!(self, Self::Copy) {
            let text = world.get::<View>(owner).unwrap().result["export"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let result = world
                .get_resource_mut::<bevy::clipboard::Clipboard>()
                .ok_or("Clipboard unavailable".to_owned())
                .and_then(|mut clipboard| {
                    clipboard.set_text(&text).map_err(|error| error.to_string())
                });
            world.get_mut::<View>(owner).unwrap().notice = result.map_or_else(
                |error| error,
                |()| "Disclosure copied. Its audience and term remain binding.".into(),
            );
            render(world, owner);
            return;
        }
        if let Self::Record(record) = self {
            let root = view.root;
            let _ = crate::native::full_record::open(
                world,
                root,
                record,
                crate::native::protein_area::Source::Local,
            );
            return;
        }
        let mut view = world.get_mut::<View>(owner).unwrap();
        if !matches!(self, Self::QueryPage(_)) {
            view.remote.review=None;
            view.review = None;
            view.claims.review = None;
            view.recipes.review = None;
            view.generation = view.generation.wrapping_add(1);
            view.result = Value::Null;
            view.analysis.page = 0;
        }
        match self {
            Self::QueryToggle(toggle) => {
                analysis::toggle(&mut view.analysis, *toggle);
                view.result = Value::Null;
            }
            Self::QueryMeasurement(measurement) => {
                view.analysis.measurements = measurement.iter().cloned().collect();
                view.result = Value::Null;
            }
            Self::QueryPage(page) => {
                view.analysis.page = *page;
            }
            Self::Person(person) => {
                if !view.person.is_empty() && view.person != *person {
                    clear_scope(&mut view);
                }
                view.person = person.clone();
                if view.analysis.subject.is_empty() {
                    view.analysis.subject = person.clone();
                    view.analysis.organ = false;
                }
                view.pending = None;
                view.result = Value::Null;
                view.import.clear();
                view.readers.clear();
                if view.anchor.is_empty() {
                    view.anchor = person.clone();
                    view.anchor_organ = false;
                }
            }
            Self::Contact(identity) => {
                view.contact = identity.uid().into();
                view.contact_organ = matches!(identity, Identity::Organ(_));
            }
            Self::Toggle(key) => match *key {
                "contact" => view.contact_organ = !view.contact_organ,
                "issuer" => view.issuer_organ = !view.issuer_organ,
                "public" => view.public = !view.public,
                "withdrawn" => view.withdrawn = !view.withdrawn,
                "as_organ" => {
                    view.as_organ = !view.as_organ;
                    view.result = Value::Null;
                    view.pending = None;
                }
                "anchor" => view.anchor_organ = !view.anchor_organ,
                "unilateral" => view.unilateral = !view.unilateral,
                _ => {}
            },
            Self::Revise(document, withdrawn) => {
                if let Ok(document) = serde_json::from_value::<
                    nucleus::trust_analysis::contacts::Document,
                >(document.clone())
                {
                    view.contact = document.statement.contact.uid().into();
                    view.contact_organ = matches!(document.statement.contact, Identity::Organ(_));
                    view.issuer_organ = matches!(document.statement.issuer, Identity::Organ(_));
                    view.revision = document.statement.revision.to_string();
                    view.withdrawn = *withdrawn;
                    view.public = matches!(document.statement.audience, Audience::Public);
                    view.readers = match document.statement.audience {
                        Audience::Public => String::new(),
                        Audience::Selected { viewers } => viewers
                            .iter()
                            .map(|identity| format!("{}:{}", identity.kind(), identity.uid()))
                            .collect::<Vec<_>>()
                            .join(", "),
                    };
                }
            }
            Self::AddRule(scope, exclusion) => {
                if view
                    .rules
                    .iter()
                    .filter(|rule| rule.scope == *scope)
                    .count()
                    < 64
                {
                    view.rules.push(Rule {
                        scope: *scope,
                        exclusion: *exclusion,
                        lower_inclusive: true,
                        upper_inclusive: true,
                        ..Default::default()
                    });
                }
            }
            Self::RemoveRule(index) => {
                if *index < view.rules.len() {
                    view.rules.remove(*index);
                }
            }
            Self::Known(index) => {
                if let Some(rule) = view.rules.get_mut(*index) {
                    rule.known = (rule.known + 1) % 3;
                }
            }
            Self::Bound(index, lower) => {
                if let Some(rule) = view.rules.get_mut(*index) {
                    if *lower {
                        rule.lower_inclusive = !rule.lower_inclusive;
                    } else {
                        rule.upper_inclusive = !rule.upper_inclusive;
                    }
                }
            }
            Self::Clear => {
                clear_scope(&mut view);
                view.notice = "Private results cleared".into();
            }
            _ => {}
        }
        drop(view);
        render(world, owner);
    }
}

fn capture(world: &mut World) {
    let changes = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, text)| input.observed != text.value().to_string())
        .map(|(input, text)| (input.owner, input.key, text.value().to_string()))
        .collect::<Vec<_>>();
    for (owner, key, value) in changes {
        let Some(mut view) = world.get_mut::<View>(owner) else {
            continue;
        };
        view.review = None;
        view.remote.review=None;
        view.claims.review = None;
        view.recipes.review = None;
        view.generation = view.generation.wrapping_add(1);
        match key {
            Key::Custody(field)=>{custody_controls::capture(&mut view.custody,field,value.clone());view.result=Value::Null;},
            Key::Serving(field)=>{serving_controls::capture(&mut view.serving,field,value.clone());view.result=Value::Null;},
            Key::OrganRoot(field) => {
                root_controls::capture(&mut view.roots,field,value.clone());
                view.result=Value::Null;
            }
            Key::Recipe(field) => {
                recipe_controls::capture(&mut view.recipes, field, value.clone());
                view.result = Value::Null;
            }
            Key::Claim(field) => {
                claims::capture(&mut view.claims, field, value.clone());
                view.result = Value::Null;
            }
            Key::Query(field) => {
                analysis::capture(&mut view.analysis, field, value.clone());
                view.result = Value::Null;
            }
            Key::Person => {
                clear_scope(&mut view);
                view.person = value.clone();
                view.pending = None;
                view.result = Value::Null;
                view.import.clear();
                view.contact.clear();
                view.anchor.clear();
                view.readers.clear();
                view.rules.clear();
            }
            Key::Contact => view.contact = value.clone(),
            Key::Readers => view.readers = value.clone(),
            Key::Term => view.term = value.clone(),
            Key::Revision => view.revision = value.clone(),
            Key::Import => view.import = value.clone(),
            Key::Anchor => view.anchor = value.clone(),
            Key::Depth => view.depth = value.clone(),
            Key::Excluded => view.excluded = value.clone(),
            Key::RuleIds(index) => {
                if let Some(rule) = view.rules.get_mut(index) {
                    rule.identities = value.clone();
                }
            }
            Key::RuleLower(index) => {
                if let Some(rule) = view.rules.get_mut(index) {
                    rule.lower = value.clone();
                }
            }
            Key::RuleUpper(index) => {
                if let Some(rule) = view.rules.get_mut(index) {
                    rule.upper = value.clone();
                }
            }
        }
        if key == Key::Import && view.result["kind"] == "identity" {
            view.result = Value::Null;
        }
        if matches!(
            key,
            Key::Anchor
                | Key::Depth
                | Key::Excluded
                | Key::RuleIds(_)
                | Key::RuleLower(_)
                | Key::RuleUpper(_)
        ) {
            view.result = Value::Null;
        }
        drop(view);
        let fields = world
            .query::<(Entity, &Input)>()
            .iter(world)
            .filter(|(_, input)| input.owner == owner && input.key == key)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        for field in fields {
            world.get_mut::<Input>(field).unwrap().observed = value.clone();
        }
        if key == Key::Person {
            render(world, owner);
        }
    }
}

fn maintain(world: &mut World) {
    let owners = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| !view.bootstrap)
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    for owner in owners {
        if crate::native::laboratory::suspended(world, owner) {
            continue;
        }
        world.get_mut::<View>(owner).unwrap().bootstrap = true;
        Command::Context.apply(world, owner);
    }
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    capture(world);
    let messages = world
        .get_resource::<Messages<CellMessage>>()
        .map(|messages| cursor.read(messages).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for message in messages {
        let owners = world
            .query_filtered::<Entity, With<View>>()
            .iter(world)
            .collect::<Vec<_>>();
        for owner in owners {
            let reset = matches!(
                &message.0,
                cell::ServerMessage::SessionAuthenticated { .. }
                    | cell::ServerMessage::LiveHello {
                        login_required: true
                    }
            ) || matches!(&message.0,cell::ServerMessage::Error {id,..} if id=="connection");
            if reset {
                let root = world.get::<View>(owner).unwrap().root;
                let mut view = world.get_mut::<View>(owner).unwrap();
                clear_scope(&mut view);
                view.person = match &message.0 {
                    cell::ServerMessage::SessionAuthenticated { person, .. } => person.clone(),
                    _ => String::new(),
                };
                view.context = Value::Null;
                view.contact.clear();
                view.anchor.clear();
                view.readers.clear();
                view.import.clear();
                view.rules.clear();
                view.excluded.clear();
                view.as_organ = false;
                view.issuer_organ = false;
                view.public = false;
                view.withdrawn = false;
                view.revision = "0".into();
                view.review = None;
                view.result = Value::Null;
                view.pending = None;
                view.bootstrap = false;
                view.root = root;
                view.notice = "The viewer changed. Private drafts and results were cleared.".into();
                drop(view);
                render(world, owner);
                continue;
            }
            let pending = world.get::<View>(owner).unwrap().pending.clone();
            let Some((pending, generation)) = pending else {
                continue;
            };
            if generation != world.get::<View>(owner).unwrap().generation {
                if matches!(&message.0,cell::ServerMessage::ActionOk {id,..}|cell::ServerMessage::Error {id,..} if *id==pending)
                {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.pending = None;
                    view.notice =
                        "Draft changed. Refresh retained disclosures to see completed requests."
                            .into();
                    drop(view);
                    render(world, owner);
                }
                continue;
            }
            match &message.0 {
                cell::ServerMessage::ActionOk { id, data, .. } if *id == pending => {
                    let result = data
                        .as_ref()
                        .map(|data| {
                            if data["identity"].is_object() {
                                json!({"kind":"identity","value":data["identity"]})
                            } else if data["measurement"].is_object() {
                                json!({"kind":"measurement","measurement":data["measurement"]})
                            } else {
                                data["trust"].clone()
                            }
                        })
                        .unwrap_or(Value::Null);
                    let context_result = result["kind"] == "contact_context";
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.pending = None;
                    if remote_controls::receive(&mut view,&result) {
                        view.notice="Reader-owned remote evidence updated".into();
                    } else if result["kind"] == "contact_context" {
                        if let Some(person) = result["person"].as_str() {
                            if !view.person.is_empty() && view.person != person {
                                clear_scope(&mut view);
                            }
                            view.person = person.into();
                            if view.analysis.subject.is_empty() {
                                view.analysis.subject = person.into();
                                view.analysis.organ = false;
                            }
                            if view.anchor.is_empty() {
                                view.anchor = person.into();
                                view.anchor_organ = false;
                            }
                        }
                        view.context = result;
                    } else if result["kind"] == "contact_review" {
                        view.claims.review = None;
                        view.recipes.review = None;
                        view.review = serde_json::from_value(result["review"].clone())
                            .ok()
                            .zip(serde_json::from_value(result["hash"].clone()).ok());
                        view.notice =
                            "Review these exact fields, audience and expiry before signing.".into();
                    } else if result["kind"] == "recipe_review" {
                        view.review = None;
                        view.claims.review = None;
                        view.recipes.review = None;
                        view.recipes.review = serde_json::from_value(result["review"].clone())
                            .ok()
                            .zip(serde_json::from_value(result["hash"].clone()).ok());
                        view.notice = "Review the exact output scope, signing Cell, audience and expiry before enabling automatic publication.".into();
                    } else if result["kind"] == "claim_review" {
                        view.recipes.review = None;
                        view.review = None;
                        view.claims.review = serde_json::from_value(result["review"].clone())
                            .ok()
                            .zip(serde_json::from_value(result["hash"].clone()).ok());
                        view.notice="Review the exact issuer, subject, amount, basis, audience and term before signing this claim.".into();
                    } else {
                        custody_controls::receive(&mut view,&result);
                        if result["kind"] == "measurement"
                            && let Ok(measurement) =
                                serde_json::from_value(result["measurement"].clone())
                        {
                            view.analysis.measurements = vec![measurement];
                            view.analysis.page = 0;
                        }
                        if result["kind"] == "saved_query" {
                            if let Some(name) = result["name"].as_str() {
                                view.analysis.name = name.into();
                            }
                            if let Ok(query) = serde_json::from_value(result["query"].clone()) {
                                analysis::load(&mut view, query);
                            }
                        }
                        if matches!(
                            result["kind"].as_str(),
                            Some("query_analysis" | "claim_check")
                        ) {
                            view.analysis.page = 0;
                        }
                        if result["kind"] == "claim_check" {
                            profile_claims::cache(&mut view, &result);
                        }
                        view.review = None;
                        view.claims.review = None;
                        view.recipes.review = None;
                        view.result = result;
                        view.notice = "Updated from the Cell".into();
                    }
                    let analyze = view.analyze_after_context && context_result && !view.person.is_empty();
                    if context_result { view.analyze_after_context = false; }
                    drop(view);
                    render(world, owner);
                    if analyze { Command::Analyze.apply(world, owner); }
                }
                cell::ServerMessage::Error { id, message, .. } if *id == pending => {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    if view.remote.pending {view.remote=remote_controls::Draft::default();}
                    view.pending = None;
                    view.result = Value::Null;
                    view.review = None;
                    view.claims.review = None;
                    view.recipes.review = None;
                    view.notice = message.clone();
                    drop(view);
                    render(world, owner);
                }
                _ => {}
            }
        }
    }
}
