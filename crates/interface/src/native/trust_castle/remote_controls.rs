use super::*;
use nucleus::trust_analysis::{
    remote::{self as model, Command as Request, Review},
    serving::Artifact,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(super) struct Draft {
    pub include_contacts: bool,
    pub receipt_sources: Vec<model::ReceiptSource>,
    pub contact_subjects: Vec<Identity>,
    pub review: Option<(Review, nucleus::karma::CanonicalHash)>,
    pub job: Option<String>,
    pub result: Value,
    pub document: Value,
    pub local: Value,
    pub verified: Value,
    pub verified_page: usize,
    pub page: usize,
    pub next_poll: Instant,
    pub pending: bool,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            include_contacts: false,
            receipt_sources: Vec::new(),
            contact_subjects: Vec::new(),
            review: None,
            job: None,
            result: Value::Null,
            document: Value::Null,
            local: Value::Null,
            verified: Value::Null,
            verified_page: 0,
            page: 0,
            next_poll: Instant::now(),
            pending: false,
        }
    }
}
#[derive(Clone)]
pub(super) enum UiCommand {
    Preview,
    Start,
    Poll,
    Verify,
    Context,
    VerifiedPage(usize),
    ReviewClaim(usize),
    Cancel,
    Forget,
    Contacts,
    Inspect(nucleus::karma::CanonicalHash),
    Page(usize),
    Copy,
    Retain,
}
struct Ticket {
    person: String,
    job: String,
    generation: u64,
    sender: tokio::sync::mpsc::Sender<cell::ClientMessage>,
}
#[derive(Resource, Default)]
pub(super) struct Tracked(BTreeMap<Entity, Ticket>);

pub(super) fn selection(view: &View) -> Result<model::Draft, String> {
    let linked_claims = view
        .claims
        .snapshot
        .as_ref()
        .map(|document| {
            vec![model::Series {
                issuer: document.statement.issuer.clone(),
                claim: document.statement.claim.clone(),
                snapshot: Some(document.clone()),
            }]
        })
        .unwrap_or_default();
    let draft = model::Draft {
        query: analysis::build(view)?,
        as_organ: view.as_organ,
        linked_claims,
        include_contacts: view.remote.include_contacts,
        receipt_sources: view.remote.receipt_sources.clone(),
        contact_subjects: view.remote.contact_subjects.clone(),
    };
    draft.questions()?;
    Ok(draft)
}

pub(super) fn request(view: &View, command: &UiCommand) -> Result<Request, String> {
    let job = || {
        view.remote
            .job
            .clone()
            .ok_or_else(|| "No remote query is active".to_owned())
    };
    Ok(match command {
        UiCommand::Preview => Request::Preview {
            draft: selection(view)?,
        },
        UiCommand::Start => {
            let (review, expected_review) = view
                .remote
                .review
                .clone()
                .ok_or("Review the actual questions and recipients first")?;
            if review.person != view.person
                || review.draft != selection(view)?
                || review.hash()? != expected_review
            {
                return Err("The questions, reader or policy changed; preview again".into());
            }
            Request::Start {
                review: Box::new(review),
                expected_review,
            }
        }
        UiCommand::Poll => Request::Poll { job: job()? },
        UiCommand::Verify => Request::Verify { job: job()? },
        UiCommand::Cancel => Request::Cancel { job: job()? },
        UiCommand::Forget => Request::Forget { job: job()? },
        UiCommand::Inspect(artifact) => Request::Inspect {
            job: job()?,
            artifact: artifact.clone(),
        },
        _ => return Err("This control stays in the view".into()),
    })
}

pub(super) fn receive(view: &mut View, result: &Value) -> bool {
    if !result["kind"]
        .as_str()
        .is_some_and(|kind| kind.starts_with("remote_"))
    {
        return false;
    }
    view.remote.pending = false;
    if result["kind"] == "remote_review" {
        view.remote.verified = Value::Null;
        view.remote.review = serde_json::from_value(result["review"].clone())
            .ok()
            .zip(serde_json::from_value(result["hash"].clone()).ok());
        view.remote.local =
            json!({"analysis":result["local"],"proof_records":result["proof_records"]});
        view.remote.page = 0;
    } else if result["kind"] == "remote_unavailable" {
        view.remote.review = None;
        view.remote.local =
            json!({"analysis":result["local"],"proof_records":result["proof_records"]});
    } else if result["kind"] == "remote_started" {
        view.remote.verified = Value::Null;
        view.remote.job = result["job"].as_str().map(str::to_owned);
        view.remote.review = None;
        view.remote.next_poll = Instant::now();
    } else if result["kind"] == "remote_cancelled" {
        view.remote.verified = Value::Null;
        if result["forgotten"] == true {
            view.remote.job = None;
        }
        view.remote.review = None;
        view.remote.document = Value::Null;
    } else if result["kind"] == "remote_verified" {
        view.remote.verified = result.clone();
        view.remote.verified_page = 0;
        return true;
    } else if result["kind"] == "remote_artifact" {
        view.remote.document = result.clone();
        view.remote.page = 0;
        return true;
    } else if result["kind"] == "remote_progress" && view.remote.document.is_object() {
        let hash = &view.remote.document["hash"];
        if !result["artifacts"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["hash"] == *hash))
        {
            view.remote.document = Value::Null;
        }
    }
    view.remote.result = result.clone();
    true
}

fn track(world: &mut World, owner: Entity) {
    let Some(sender) = world
        .get_non_send::<crate::native::cell_bridge::CellBridge>()
        .map(|bridge| bridge.outgoing.clone())
    else {
        return;
    };
    track_with_sender(world, owner, sender);
}

pub(super) fn track_with_sender(
    world: &mut World,
    owner: Entity,
    sender: tokio::sync::mpsc::Sender<cell::ClientMessage>,
) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let Some(job) = view.remote.job.clone() else {
        return;
    };
    let (person, generation) = (view.person.clone(), view.generation);
    world.init_resource::<Tracked>();
    world.resource_mut::<Tracked>().0.insert(
        owner,
        Ticket {
            person,
            job,
            generation,
            sender,
        },
    );
}

pub(super) fn maintain(world: &mut World) {
    world.init_resource::<Tracked>();
    let stale = world
        .resource::<Tracked>()
        .0
        .iter()
        .filter(|(owner, ticket)| {
            world.get::<View>(**owner).is_none_or(|view| {
                view.person != ticket.person
                    || view.generation != ticket.generation
                    || view.remote.job.as_deref() != Some(&ticket.job)
            })
        })
        .map(|(owner, _)| *owner)
        .collect::<Vec<_>>();
    for owner in stale {
        if let Some(ticket) = world.resource_mut::<Tracked>().0.remove(&owner) {
            let _ = ticket.sender.try_send(cell::ClientMessage::Act {
                id: nucleus::new_uid("trust-cancel"),
                action: engine::actions::Action::TrustAnalysis {
                    person: Some(ticket.person),
                    request_id: nucleus::new_uid("trust-cancel"),
                    request: Backend::Remote {
                        request: Request::Forget {
                            job: ticket.job.clone(),
                        },
                    },
                },
            });
            let mut rerender = false;
            if let Some(mut view) = world.get_mut::<View>(owner)
                && view.remote.job.as_deref() == Some(&ticket.job)
            {
                view.remote = Draft::default();
                rerender = true;
            }
            if rerender {
                super::render(world, owner);
            }
        }
    }
    let owners = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| view.remote.job.is_some())
        .map(|(owner, _)| owner)
        .collect::<Vec<_>>();
    for owner in owners {
        if crate::native::laboratory::suspended(world, owner) {
            continue;
        }
        if !world.resource::<Tracked>().0.contains_key(&owner) {
            track(world, owner);
        }
        let view = world.get::<View>(owner).unwrap();
        if view.pending.is_some() || Instant::now() < view.remote.next_poll {
            continue;
        }
        if view.remote.result["kind"] == "remote_progress"
            && !matches!(
                view.remote.result["state"].as_str(),
                Some("queued" | "running")
            )
        {
            continue;
        }
        if view.remote.result["kind"] == "remote_cancelled" {
            continue;
        }
        world.get_mut::<View>(owner).unwrap().remote.next_poll =
            Instant::now() + Duration::from_secs(1);
        apply(world, owner, &UiCommand::Poll);
    }
}

pub(super) fn apply(world: &mut World, owner: Entity, command: &UiCommand) {
    if let UiCommand::ReviewClaim(index) = command {
        let Some(view) = world.get::<View>(owner) else { return };
        if view.pending.is_some() { return; }
        let Some(value) = view.remote.verified["claim_checks"].as_array().and_then(|checks| checks.get(*index)) else { return };
        if value["review_required"] != true { return; }
        let Ok(document) = serde_json::from_value::<nucleus::trust_analysis::claims::Document>(value["document"].clone()) else { return };
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.generation = view.generation.wrapping_add(1);
        view.remote = Draft::default();
        view.result = Value::Null;
        super::profile_claims::load_snapshot(&mut view, &document);
        view.notice = "Review the newly selected claim scope and reader policy, then explicitly review and start any remote questions.".into();
        drop(view);
        super::render(world, owner);
        return;
    }
    if matches!(command, UiCommand::Preview) {
        if world
            .get::<View>(owner)
            .is_some_and(|view| view.pending.is_some())
        {
            world.get_mut::<View>(owner).unwrap().notice = "Wait for the current request".into();
            return;
        }
        world.init_resource::<Tracked>();
        if let Some(ticket) = world.resource_mut::<Tracked>().0.remove(&owner) {
            let _ = ticket.sender.try_send(cell::ClientMessage::Act {
                id: nucleus::new_uid("trust-forget"),
                action: engine::actions::Action::TrustAnalysis {
                    person: Some(ticket.person),
                    request_id: nucleus::new_uid("trust-forget"),
                    request: Backend::Remote {
                        request: Request::Forget { job: ticket.job },
                    },
                },
            });
        }
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.remote.job = None;
        view.remote.result = Value::Null;
        view.remote.document = Value::Null;
        view.remote.review = None;
        view.remote.verified = Value::Null;
    }
    if matches!(command, UiCommand::Context) {
        let mut view = world.get_mut::<View>(owner).unwrap();
        if view.pending.is_some() {
            view.notice = "Wait for the current request".into();
            return;
        }
        let sources = serde_json::from_value::<Vec<model::ReceiptSource>>(
            view.remote.verified["receipt_sources"].clone(),
        );
        let contacts = serde_json::from_value::<Vec<Identity>>(
            view.remote.verified["contact_subjects"].clone(),
        );
        let (Ok(sources), Ok(contacts)) = (sources, contacts) else {
            view.notice = "Verify collected originals first".into();
            return;
        };
        for source in sources {
            if !view.remote.receipt_sources.contains(&source)
                && view.remote.receipt_sources.len() < 32
            {
                view.remote.receipt_sources.push(source);
            }
        }
        for identity in contacts {
            if !view.remote.contact_subjects.contains(&identity)
                && view.remote.contact_subjects.len() < 32
            {
                view.remote.contact_subjects.push(identity);
            }
        }
        view.generation = view.generation.wrapping_add(1);
        view.remote.review = None;
        drop(view);
        apply(world, owner, &UiCommand::Preview);
        return;
    }
    if matches!(
        command,
        UiCommand::Contacts | UiCommand::Page(_) | UiCommand::VerifiedPage(_)
    ) {
        let mut view = world.get_mut::<View>(owner).unwrap();
        match command {
            UiCommand::Contacts => {
                view.remote.include_contacts = !view.remote.include_contacts;
                view.remote.review = None;
                view.generation = view.generation.wrapping_add(1);
            }
            UiCommand::Page(page) => view.remote.page = *page,
            UiCommand::VerifiedPage(page) => view.remote.verified_page = *page,
            _ => {}
        }
        drop(view);
        super::render(world, owner);
        return;
    }
    if matches!(command, UiCommand::Copy | UiCommand::Retain) {
        let view = world.get::<View>(owner).unwrap();
        let artifact = serde_json::from_value::<Artifact>(view.remote.document["artifact"].clone());
        let result = artifact
            .map_err(|error| error.to_string())
            .and_then(|artifact| {
                if matches!(command, UiCommand::Copy) {
                    let bytes = artifact.bytes()?;
                    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                    world
                        .get_resource_mut::<bevy::clipboard::Clipboard>()
                        .ok_or("Clipboard unavailable")?
                        .set_text(&text)
                        .map_err(|e| e.to_string())
                } else {
                    let view = world.get::<View>(owner).unwrap();
                    let request_id = nucleus::new_uid("trust-retain");
                    let person = Some(view.person.clone());
                    let as_organ = view.as_organ;
                    let action = match artifact {
                        Artifact::Contact(document) => engine::actions::Action::TrustAnalysis {
                            person,
                            request_id,
                            request: Backend::ImportContact {
                                document: serde_json::to_string(&document)
                                    .map_err(|e| e.to_string())?,
                                as_organ,
                            },
                        },
                        Artifact::Claim(document) => engine::actions::Action::TrustAnalysis {
                            person,
                            request_id,
                            request: Backend::Claims {
                                request:
                                    nucleus::trust_analysis::claims::commands::Command::Import {
                                        document: serde_json::to_string(&document)
                                            .map_err(|e| e.to_string())?,
                                        as_organ,
                                    },
                            },
                        },
                        Artifact::Receipt(document) => engine::actions::Action::TransferReceipt {
                            person,
                            request_id,
                            request: nucleus::transfer::receipt::Command::Import {
                                document: serde_json::to_string(document.as_ref())
                                    .map_err(|e| e.to_string())?,
                            },
                        },
                    };
                    super::send_action(world, owner, action)
                }
            });
        if let Err(error) = result {
            world.get_mut::<View>(owner).unwrap().notice = error;
        }
        super::render(world, owner);
        return;
    }
    let request = request(world.get::<View>(owner).unwrap(), command);
    let starting = request.as_ref().ok().and_then(|request| {
        if let Request::Start { review, .. } = request {
            Some(review.nonce.clone())
        } else {
            None
        }
    });
    let result =
        request.and_then(|request| super::request(world, owner, Backend::Remote { request }));
    if let Err(error) = result {
        world.get_mut::<View>(owner).unwrap().notice = error;
    } else {
        world.get_mut::<View>(owner).unwrap().remote.pending = true;
        if let Some(id) = starting {
            world.get_mut::<View>(owner).unwrap().remote.job = Some(id);
            track(world, owner);
        }
    }
    super::render(world, owner);
}

fn button(world: &mut World, owner: Entity, text: &str, command: UiCommand) {
    panel::button(world, owner, owner, text, Command::Remote(command));
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let state = view.remote.clone();
    let analysis_page = view.analysis.page;
    label(world, owner, "Ask around for original evidence");
    label(
        world,
        owner,
        "Review the actual public questions and pinned contacts before sending. Your weights, source policy and reader identity stay local unless named in the reviewed question. Work lasts at most 15 seconds; closing or changing this view cancels it. No automatic import or Transfer effects.",
    );
    button(
        world,
        owner,
        if state.include_contacts {
            "Include disclosed contacts about subject: yes"
        } else {
            "Include disclosed contacts about subject: no"
        },
        UiCommand::Contacts,
    );
    button(
        world,
        owner,
        "Review questions and replace prior transient query",
        UiCommand::Preview,
    );
    if state.local["analysis"].is_object() {
        analysis::render_result(world, owner, &state.local, 0);
    }
    if let Some((review, _)) = &state.review {
        let public = json!({"questions":review.questions,"recipients":review.endpoints,"deadline_seconds":15,"concurrent_requests":4,"aggregate_bytes":nucleus::trust_analysis::MAX_BYTES});
        let text = serde_json::to_string_pretty(&public).unwrap_or_default();
        let page = text
            .chars()
            .skip(state.page * 4096)
            .take(4096)
            .collect::<String>();
        label(world, owner, &page);
        button(
            world,
            owner,
            "Start exactly reviewed questions",
            UiCommand::Start,
        );
        pages(world, owner, state.page, text.chars().count());
    }
    if state.job.is_some() {
        button(world, owner, "Refresh query progress", UiCommand::Poll);
        button(
            world,
            owner,
            "Cancel query and discard evidence",
            UiCommand::Cancel,
        );
        button(world, owner, "Forget transient query", UiCommand::Forget);
        if matches!(
            state.result["state"].as_str(),
            Some("completed" | "expired")
        ) {
            button(
                world,
                owner,
                "Verify and analyze permitted local and collected originals",
                UiCommand::Verify,
            );
        }
    }
    if state.verified["kind"] == "remote_verified" {
        label(
            world,
            owner,
            &format!(
                "Merged originals · checked {} · partial {}",
                state.verified["checked_at"], state.verified["partial"]
            ),
        );
        label(
            world,
            owner,
            "Local and collected originals are evaluated together under your reviewed source policy. Quantities and weighted indices remain separate. Refresh after permissions or evidence change.",
        );
        if state.verified["analysis"].is_object() {
            analysis::render_result(world, owner, &state.verified, analysis_page);
        }
        for (index, check) in state.verified["claim_checks"].as_array().into_iter().flatten().enumerate() {
            label(world, owner, &format!("Claim {} · {} · claimed {} · observed {}", check["claim"].as_str().unwrap_or_default(), check["state"].as_str().unwrap_or("unknown"), check["document"]["statement"]["amount"]["value"].as_str().unwrap_or("unavailable"), check["support"]["observed"]["value"].as_str().unwrap_or("unavailable")));
            if check["review_required"] == true {
                button(world, owner, "Review changed claim scope in a separate check", UiCommand::ReviewClaim(index));
            }
        }
        let text = serde_json::to_string_pretty(&state.verified).unwrap_or_default();
        let signatures = state.verified["signatures"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let pages = text
            .chars()
            .count()
            .div_ceil(4096)
            .max(signatures.len().div_ceil(8))
            .max(1);
        let page = state.verified_page.min(pages - 1);
        label(
            world,
            owner,
            &text
                .chars()
                .skip(page * 4096)
                .take(4096)
                .collect::<String>(),
        );
        for entry in signatures.iter().skip(page * 8).take(8) {
            label(
                world,
                owner,
                &format!("Original signature: {}", entry["signature"]),
            );
            let original = &entry["original"];
            if let Some(record) = original["record"].as_str() {
                panel::button(
                    world,
                    owner,
                    owner,
                    "Open permitted local proof Record",
                    Command::Record(record.into()),
                );
            } else if let Ok(hash) = serde_json::from_value(original["hash"].clone()) {
                button(
                    world,
                    owner,
                    "Open collected original proof",
                    UiCommand::Inspect(hash),
                );
            }
        }
        if page > 0 {
            button(
                world,
                owner,
                "Previous merged-proof page",
                UiCommand::VerifiedPage(page - 1),
            );
        }
        if page + 1 < pages {
            button(
                world,
                owner,
                "Next merged-proof page",
                UiCommand::VerifiedPage(page + 1),
            );
        }
        if state.verified["receipt_sources"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
            || state.verified["contact_subjects"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty())
        {
            button(
                world,
                owner,
                "Review additional public proof-source and contact questions",
                UiCommand::Context,
            );
            label(
                world,
                owner,
                "This prepares a new question review and discards the old transient job. Inspect the exact public sources, identities and recipients before Start. Nothing is sent automatically.",
            );
        }
    }
    let result = &state.result;
    if result["kind"] == "remote_progress" {
        label(
            world,
            owner,
            &format!(
                "{} · {} received bytes · {} sent bytes · {} documents/events · {} ms · partial {}",
                result["state"],
                result["bytes"],
                result["sent_bytes"],
                result["events"],
                result["elapsed_ms"],
                result["partial"]
            ),
        );
        if let Some(peers) = result["endpoints"].as_array() {
            for peer in peers {
                label(
                    world,
                    owner,
                    &format!("{} · {} · {}", peer["organ"], peer["state"], peer["error"]),
                );
            }
        }
        if let Some(headers) = result["artifacts"].as_array() {
            for header in headers.iter().skip(state.page * 16).take(16) {
                label(
                    world,
                    owner,
                    &format!(
                        "{} · signature snapshot {} · {}",
                        header["kind"], header["signature_snapshot"], header["checked_at"]
                    ),
                );
                if let Ok(hash) = serde_json::from_value(header["hash"].clone()) {
                    button(
                        world,
                        owner,
                        "Open original and recheck signature",
                        UiCommand::Inspect(hash),
                    );
                }
            }
            if state.page > 0 {
                button(
                    world,
                    owner,
                    "Previous evidence page",
                    UiCommand::Page(state.page - 1),
                );
            }
            if (state.page + 1) * 16 < headers.len() {
                button(
                    world,
                    owner,
                    "Next evidence page",
                    UiCommand::Page(state.page + 1),
                );
            }
        }
    }
    let result = &state.document;
    if result["kind"] == "remote_artifact" {
        label(
            world,
            owner,
            &format!(
                "Original signature check: {} · {}",
                result["signature_state"], result["checked_at"]
            ),
        );
        let text = serde_json::to_string_pretty(&result["artifact"]).unwrap_or_default();
        label(
            world,
            owner,
            &text
                .chars()
                .skip(state.page * 4096)
                .take(4096)
                .collect::<String>(),
        );
        pages(world, owner, state.page, text.chars().count());
        button(
            world,
            owner,
            "Copy minimal original artifact",
            UiCommand::Copy,
        );
        button(
            world,
            owner,
            "Explicitly retain an owned evidence copy",
            UiCommand::Retain,
        );
    }
    if let Some(notice) = state.result["notice"]
        .as_str()
        .or_else(|| state.result["message"].as_str())
    {
        label(world, owner, notice);
    }
}

fn pages(world: &mut World, owner: Entity, page: usize, characters: usize) {
    if page > 0 {
        button(
            world,
            owner,
            "Previous document page",
            UiCommand::Page(page - 1),
        );
    }
    if (page + 1) * 4096 < characters {
        button(
            world,
            owner,
            "Next document page",
            UiCommand::Page(page + 1),
        );
    }
}
