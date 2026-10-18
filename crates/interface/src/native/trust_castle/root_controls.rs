use super::*;
use nucleus::identity::{
    Command as Request,
    roots::{Proof, Review},
};

#[derive(Clone, Default)]
pub(super) struct Draft {
    pub subject: String,
    pub document: String,
    pub focused: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Subject,
    Document,
}

pub(super) fn capture(draft: &mut Draft, field: Field, value: String) {
    match field {
        Field::Subject => draft.subject = value,
        Field::Document => draft.document = value,
    }
}

#[derive(Clone)]
pub(super) enum UiCommand {
    Inspect,
    Export,
    Preview,
    Accept,
    Copy,
    ShowAnalysis,
}

#[derive(Clone)]
pub(super) struct Open(pub String);

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
                if let Some(owner) = open_view(world, root, Some(&Identity::Organ(self.0.clone())))
                {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.roots.subject = self.0.clone();
                    view.roots.focused = true;
                    super::render(world, owner);
                }
                return;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

pub(super) fn request(view: &View, command: &UiCommand) -> Result<Request, String> {
    let subject = view.roots.subject.trim();
    if !nucleus::valid_uid(subject, "r") {
        return Err("Choose the existing Organ's Record UID".into());
    }
    Ok(match command {
        UiCommand::Inspect => Request::OrganRoots {
            subject: subject.into(),
        },
        UiCommand::Export => Request::ExportOrganRoot {
            subject: subject.into(),
        },
        UiCommand::Preview => {
            let proof = Proof::parse(view.roots.document.as_bytes())?;
            if proof.organ != subject {
                return Err("This proof describes a different Organ".into());
            }
            Request::PreviewOrganRoot {
                subject: subject.into(),
                document: view.roots.document.clone(),
            }
        }
        UiCommand::Accept => {
            let value = &view.result["value"];
            if view.result["kind"] != "identity" || value["kind"] != "organ_root_preview" {
                return Err("Compare this Organ's exact fresh root review first".into());
            }
            let review: Review = serde_json::from_value(value["review"].clone())
                .map_err(|error| error.to_string())?;
            let proof = Proof::parse(view.roots.document.as_bytes())?;
            let hash = review.hash()?;
            if review.person != view.person
                || review.proof != proof
                || proof.organ != subject
                || value["review_hash"] != hash.as_str()
            {
                return Err("The reviewing Person, Organ or proof changed; preview again".into());
            }
            Request::AcceptOrganRoot {
                request_id: format!("organ-root:{}", hash.as_str()),
                review,
                expected_review: hash,
            }
        }
        UiCommand::Copy => return Err("Copy the exported proof without a network request".into()),
        UiCommand::ShowAnalysis => {
            return Err("Open local analysis without a network request".into());
        }
    })
}

pub(super) fn apply(world: &mut World, owner: Entity, command: &UiCommand) {
    if matches!(command, UiCommand::ShowAnalysis) {
        if let Some(mut view) = world.get_mut::<View>(owner) {
            view.roots.focused = false;
        }
        return;
    }
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    if matches!(command, UiCommand::Copy) {
        let text = view.result["value"]["document"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let result = world
            .get_resource_mut::<bevy::clipboard::Clipboard>()
            .ok_or("Clipboard unavailable".to_owned())
            .and_then(|mut clipboard| clipboard.set_text(&text).map_err(|error| error.to_string()));
        world.get_mut::<View>(owner).unwrap().notice = result.map_or_else(
            |error| error,
            |()| {
                "Public key-control proof copied. Compare the full fingerprint independently."
                    .into()
            },
        );
        return;
    }
    let result = request(view, command).and_then(|request| {
        send_action(world, owner, engine::actions::Action::Identity { request })
    });
    if let Err(error) = result {
        world.get_mut::<View>(owner).unwrap().notice = error;
    }
}

fn button(world: &mut World, owner: Entity, caption: &str, command: UiCommand) {
    panel::button(world, owner, owner, caption, Command::OrganRoot(command));
}

fn at(value: &Value) -> String {
    value
        .as_i64()
        .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| "unavailable".into())
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let draft = view.roots.clone();
    let result = view.result["value"].clone();
    let admin = view.context["organ_admin"] == true;
    let local = view.context["organ"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    label(world, owner, "Independently recognize an Organ owner key");
    if draft.focused {
        button(
            world,
            owner,
            "Open full trust analysis",
            UiCommand::ShowAnalysis,
        );
    }
    label(
        world,
        owner,
        "Use retained public key-control evidence and compare the full fingerprint through an independent channel. The owner's key can remain offline. Accepting an owner identity permits verification of its future signed statements and roster updates; it does not establish reliability or delivery. The local review lasts five minutes.",
    );
    field(
        world,
        owner,
        owner,
        Key::OrganRoot(Field::Subject),
        "Existing Organ Record UID",
        &draft.subject,
    );
    field(
        world,
        owner,
        owner,
        Key::OrganRoot(Field::Document),
        "Public key-control proof",
        &draft.document,
    );
    button(
        world,
        owner,
        "Inspect retained Organ root history",
        UiCommand::Inspect,
    );
    button(
        world,
        owner,
        "Preview exact owner-key recognition",
        UiCommand::Preview,
    );
    if admin && draft.subject == local && !local.is_empty() {
        button(
            world,
            owner,
            "Issue public key-control proof on this owner device",
            UiCommand::Export,
        );
    }
    if result["kind"] == "organ_root_proof" {
        label(
            world,
            owner,
            &format!(
                "Owner key: {} · fingerprint {} · key-control evidence issued at {}",
                result["proof"]["public_key"],
                result["fingerprint"],
                at(&result["proof"]["issued_at"])
            ),
        );
        button(
            world,
            owner,
            "Copy public key-control proof",
            UiCommand::Copy,
        );
    }
    if result["kind"] == "organ_roots" || result["kind"] == "organ_root_accepted" {
        let state = &result["state"];
        label(
            world,
            owner,
            &format!(
                "Original root: {} · revoked {}. Current root: {} · revoked {} · decision revision {} · audit available {}",
                state["original"]["public_key"],
                state["original"]["revoked"],
                state["current"]["public_key"],
                state["current"]["revoked"],
                state["revision"],
                state["available"]
            ),
        );
        for old in result["history"].as_array().into_iter().flatten() {
            label(
                world,
                owner,
                &format!(
                    "Retained revision {} · fingerprint {} · revoked {}",
                    old["revision"], old["fingerprint"], old["revoked"]
                ),
            );
        }
        if let Some(record) = result["record"].as_str() {
            panel::button(
                world,
                owner,
                owner,
                "Open private root-recognition audit Record",
                Command::Record(record.into()),
            );
        }
    }
    if result["kind"] == "organ_root_preview" {
        label(
            world,
            owner,
            &format!(
                "Review Organ {} · previous fingerprint {} · revoked {} · current decision {} · profile revision {}",
                result["review"]["proof"]["organ"],
                result["previous_fingerprint"],
                result["review"]["previous"]["current"]["revoked"],
                result["review"]["previous"]["revision"],
                result["review"]["profile_revision"]
            ),
        );
        label(
            world,
            owner,
            &format!(
                "Compare full proposed fingerprint: {} · key {} · key-control evidence issued at {}",
                result["new_fingerprint"],
                result["review"]["proof"]["public_key"],
                at(&result["review"]["proof"]["issued_at"])
            ),
        );
        label(
            world,
            owner,
            "Accept only after the independent comparison. This saves a private signed audit Record and retains original anchors and revocations. Deleting the current decision withholds it instead of restoring an older root.",
        );
        if admin {
            button(
                world,
                owner,
                "Accept independently compared Organ owner key",
                UiCommand::Accept,
            );
        }
    }
}
