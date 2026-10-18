use super::*;
use nucleus::trust_analysis::serving::{
    self as model, Artifact, Command as Request, Scope, SourceKind,
};

#[derive(Clone)]
pub(super) struct Draft {
    pub source: String,
    pub kind: SourceKind,
    pub scope: Scope,
    pub term: String,
    pub page: usize,
    pub focused: bool,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            source: String::new(),
            kind: SourceKind::Receipt,
            scope: Scope::Fixed,
            term: "86400".into(),
            page: 0,
            focused: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Source,
    Term,
}

#[derive(Clone)]
pub(super) enum UiCommand {
    Preview,
    Publish,
    List(Option<String>),
    Inspect(String),
    Kind,
    Scope,
    Page(usize),
    Full,
    Copy,
}

#[derive(Clone)]
pub(crate) struct Open {
    pub source: String,
    pub kind: SourceKind,
    pub scope: Scope,
}

impl Action for Open {
    fn apply(&self, world: &mut World, entity: Entity) {
        let mut cursor = Some(entity);
        while let Some(entity) = cursor {
            let root = if world
                .get::<crate::native::workspace::Workspaces>(entity)
                .is_some()
            {
                Some(entity)
            } else {
                world
                    .get::<crate::native::sand::InBox>(entity)
                    .map(|root| root.0)
            };
            if let Some(root) = root {
                if let Some(owner) = open_view(world, root, None) {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.serving.source = self.source.clone();
                    view.serving.kind = self.kind;
                    view.serving.scope = self.scope;
                    view.generation = view.generation.wrapping_add(1);
                    view.result = Value::Null;
                    view.serving.focused = true;
                    super::render(world, owner);
                }
                return;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

pub(super) fn capture(draft: &mut Draft, field: Field, value: String) {
    match field {
        Field::Source => draft.source = value,
        Field::Term => draft.term = value,
    };
    draft.page = 0;
}

pub(super) fn selection(view: &View) -> Result<model::Request, String> {
    let mut audience = nucleus::trust_analysis::Filter::default();
    for rule in view
        .rules
        .iter()
        .filter(|rule| rule.scope == RuleScope::Serving)
    {
        let condition = analysis::condition(rule)?;
        if rule.exclusion {
            audience.exclude.push(condition)
        } else {
            audience.include.push(condition)
        }
    }
    let request = model::Request {
        scope: view.serving.scope,
        source: model::Source {
            record: view.serving.source.trim().into(),
            kind: view.serving.kind,
        },
        as_organ: view.as_organ,
        audience,
        lifetime_seconds: view
            .serving
            .term
            .parse()
            .map_err(|_| "Choose a positive serving term in seconds")?,
    };
    request.validate()?;
    Ok(request)
}

pub(super) fn request(view: &View, command: &UiCommand) -> Result<Request, String> {
    Ok(match command {
        UiCommand::Preview => Request::Preview {
            request: selection(view)?,
        },
        UiCommand::Publish => {
            if view.result["kind"] != "serving_preview" {
                return Err("Preview the exact document and serving conditions first".into());
            }
            let review: model::Review =
                serde_json::from_value(view.result["review"].clone()).map_err(|e| e.to_string())?;
            let expected_review = review.hash()?;
            if review.person != view.person
                || review.request != selection(view)?
                || view.result["hash"] != expected_review.as_str()
            {
                return Err(
                    "Serving source, audience or reviewing Person changed; preview again".into(),
                );
            }
            Request::Publish {
                review: Box::new(review),
                expected_review,
            }
        }
        UiCommand::List(before) => Request::List {
            before: before.clone(),
        },
        UiCommand::Inspect(record) => Request::Inspect {
            record: record.clone(),
        },
        _ => return Err("This control stays on the device".into()),
    })
}

pub(super) fn apply(world: &mut World, owner: Entity, command: &UiCommand) {
    if matches!(
        command,
        UiCommand::Kind | UiCommand::Scope | UiCommand::Page(_) | UiCommand::Full
    ) {
        let mut view = world.get_mut::<View>(owner).unwrap();
        match command {
            UiCommand::Kind => {
                view.serving.kind = match view.serving.kind {
                    SourceKind::Receipt => SourceKind::Contact,
                    SourceKind::Contact => SourceKind::Claim,
                    SourceKind::Claim => SourceKind::Receipt,
                };
                view.serving.scope = Scope::Fixed;
                view.generation = view.generation.wrapping_add(1);
                view.result = Value::Null;
                view.serving.page = 0;
            }
            UiCommand::Scope => {
                view.serving.scope = match view.serving.scope {
                    Scope::Fixed => Scope::Recipe,
                    Scope::Recipe => Scope::Fixed,
                };
                view.generation = view.generation.wrapping_add(1);
                view.result = Value::Null;
                view.serving.page = 0;
            }
            UiCommand::Page(page) => view.serving.page = *page,
            UiCommand::Full => view.serving.focused = false,
            _ => {}
        }
        return;
    }
    if matches!(command, UiCommand::Copy) {
        let document = world.get::<View>(owner).unwrap().result["review"]["artifact"].clone();
        let result = serde_json::from_value::<Artifact>(document)
            .map_err(|e| e.to_string())
            .and_then(|artifact| artifact.bytes())
            .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
            .and_then(|text| {
                world
                    .get_resource_mut::<bevy::clipboard::Clipboard>()
                    .ok_or("Clipboard unavailable".to_owned())?
                    .set_text(&text)
                    .map_err(|e| e.to_string())
            });
        world.get_mut::<View>(owner).unwrap().notice=result.map_or_else(|error|error,|()|"Original minimal document copied; the private approval and source reference are excluded.".into());
        return;
    }
    let result = request(world.get::<View>(owner).unwrap(), command)
        .and_then(|request| super::request(world, owner, Backend::Serving { request }));
    if let Err(error) = result {
        world.get_mut::<View>(owner).unwrap().notice = error;
    }
}

fn button(world: &mut World, owner: Entity, caption: &str, command: UiCommand) {
    panel::button(world, owner, owner, caption, Command::Serving(command));
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let draft = view.serving.clone();
    let result = view.result.clone();
    let rules = view.rules.clone();
    let as_organ = view.as_organ;
    let organ_admin = view.context["organ_admin"] == true;
    label(world, owner, "Share minimal signed evidence with queries");
    label(
        world,
        owner,
        "The approval stays private. Share one fixed signed document, or explicitly allow a recipe's changing outputs within its reviewed meaning, audience and permit term. No included Organ conditions means private. Device and contact Answer consent and a current accepted representative roster are required. This is sharing permission, not a retention promise or physical-delivery confirmation.",
    );
    if draft.focused {
        button(world, owner, "Open full trust analysis", UiCommand::Full);
        if organ_admin || as_organ {
            panel::button(
                world,
                owner,
                owner,
                if as_organ {
                    "Source reader: Organ grants (requires current administration)"
                } else {
                    "Source reader: Person grants"
                },
                Command::Toggle("as_organ"),
            );
        }
    }
    field(
        world,
        owner,
        owner,
        Key::Serving(Field::Source),
        if draft.scope == Scope::Recipe {
            "Owned claim recipe Record UID"
        } else {
            "Owned evidence Record UID"
        },
        &draft.source,
    );
    if draft.kind == SourceKind::Claim {
        button(
            world,
            owner,
            if draft.scope == Scope::Recipe {
                "Sharing: current and future outputs of this exact recipe"
            } else {
                "Sharing: fixed claim document only"
            },
            UiCommand::Scope,
        );
    }
    button(
        world,
        owner,
        &format!("Evidence type: {}", draft.kind.as_str()),
        UiCommand::Kind,
    );
    field(
        world,
        owner,
        owner,
        Key::Serving(Field::Term),
        if draft.scope == Scope::Recipe {
            "Serving term in seconds (limited by recipe permit)"
        } else {
            "Serving term in seconds (shortened to original expiry)"
        },
        &draft.term,
    );
    for (index, rule) in rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| rule.scope == RuleScope::Serving)
    {
        ui::rule_controls(world, owner, index, rule);
    }
    for (caption, exclude) in [
        ("Include query readers", false),
        ("Exclude query readers", true),
    ] {
        panel::button(
            world,
            owner,
            owner,
            caption,
            Command::AddRule(RuleScope::Serving, exclude),
        );
    }
    label(
        world,
        owner,
        "Choose organ:r_… identities and optional contact Proximity ranges. Inclusions use OR; criteria within each condition use AND; exclusions win. Person-only signed audiences never become Organ grants.",
    );
    button(
        world,
        owner,
        "Preview exact evidence sharing",
        UiCommand::Preview,
    );
    button(
        world,
        owner,
        "List my serving approvals",
        UiCommand::List(None),
    );
    if result["kind"] == "serving_preview" || result["kind"] == "serving_inspection" {
        if result["review"]["request"]["scope"] == "recipe" {
            label(
                world,
                owner,
                "This approves changing totals, counts, dates and availability within the exact displayed recipe permit. The displayed output is an example. Each future document keeps its own expiry. Pause stops recipe sharing; revocation can serve a signed withdrawal. A new recipe, meaning, audience or signing permit requires another review.",
            );
        }
        label(
            world,
            owner,
            &format!(
                "Original document hash {} · serving ends {}",
                result["artifact_hash"], result["review"]["expires_at"]
            ),
        );
        let document =
            serde_json::to_string_pretty(&result["review"]["artifact"]).unwrap_or_default();
        let total = document.chars().count().div_ceil(4096).max(1);
        let page = draft.page.min(total - 1);
        label(
            world,
            owner,
            &format!("Exact document · page {} of {}", page + 1, total),
        );
        label(
            world,
            owner,
            &document
                .chars()
                .skip(page * 4096)
                .take(4096)
                .collect::<String>(),
        );
        if page > 0 {
            button(
                world,
                owner,
                "Previous document page",
                UiCommand::Page(page - 1),
            );
        }
        if page + 1 < total {
            button(
                world,
                owner,
                "Next document page",
                UiCommand::Page(page + 1),
            );
        }
        button(
            world,
            owner,
            "Copy original minimal document",
            UiCommand::Copy,
        );
        if result["kind"] == "serving_preview" {
            button(
                world,
                owner,
                "Enable exactly reviewed evidence sharing",
                UiCommand::Publish,
            );
        }
    }
    let entries = if result["kind"] == "serving_list" {
        result["entries"].as_array().cloned().unwrap_or_default()
    } else if matches!(
        result["kind"].as_str(),
        Some("serving_permission" | "serving_inspection")
    ) {
        vec![result.clone()]
    } else {
        Vec::new()
    };
    for entry in entries {
        let Some(record) = entry["record"].as_str() else {
            continue;
        };
        label(
            world,
            owner,
            &format!(
                "Approval {record} · {} · current {} · reviewed document {} · expires {}",
                if entry["scope"] == "recipe" {
                    "changing recipe outputs"
                } else {
                    "fixed document"
                },
                entry["current"],
                entry["artifact_hash"],
                entry["expires_at"]
            ),
        );
        button(
            world,
            owner,
            "Inspect private serving approval",
            UiCommand::Inspect(record.into()),
        );
        panel::button(
            world,
            owner,
            owner,
            "Open private approval Record",
            Command::Record(record.into()),
        );
        panel::button(
            world,
            owner,
            owner,
            "Stop this serving approval",
            Stop(record.into()),
        );
    }
    if let Some(next) = result["next"].as_str() {
        button(
            world,
            owner,
            "Older serving approvals",
            UiCommand::List(Some(next.into())),
        );
    }
    label(
        world,
        owner,
        "Stopping deletes the approval Record and prevents new fetches. Earlier permissions do not reactivate; already received bytes cannot be recalled. Fixed sharing never includes future outputs. Recipe sharing stays bound to the exact reviewed permit and private recipe revision.",
    );
}

#[derive(Clone)]
struct Stop(String);
impl Action for Stop {
    fn apply(&self, world: &mut World, owner: Entity) {
        super::capture(world);
        if let Err(error) = send_action(
            world,
            owner,
            engine::actions::Action::DeleteRecord {
                target: self.0.clone(),
            },
        ) {
            world.get_mut::<View>(owner).unwrap().notice = error;
        }
        super::render(world, owner);
    }
}
