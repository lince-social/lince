use super::*;
use forms::{boolean, choices};
mod ask;
mod discovery_sources;
mod moderation;
mod operator;
mod reports;
mod resend;
mod subscriptions;

pub(super) fn delivery_label(stage: &str, destinations: &Value) -> String {
    match stage {
        "recipient-durable" => "The recipient saved this Message".into(),
        "recipient-refused" => "The recipient declined this Message".into(),
        "mailbox-stored" => {
            let hosts = destinations
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            let stored = hosts
                .iter()
                .filter(|host| host["stored"].as_i64().unwrap_or(0) > 0)
                .count();
            if stored == 1 {
                "One mailbox confirmed storage. The recipient has not confirmed receipt".into()
            } else if stored > 1 {
                format!(
                    "{stored} mailboxes confirmed storage. The recipient has not confirmed receipt"
                )
            } else {
                "Mailbox storage was confirmed. The recipient has not confirmed receipt".into()
            }
        }
        "queued" | "waiting" => "Saved on this device; waiting for keys or mailbox delivery".into(),
        "held" => "Delivery paused; review current device keys and permissions".into(),
        "expired" => "Delivery time ended; the retained Message is still available".into(),
        "cancelled" => "Delivery stopped; the retained Message is still available".into(),
        _ => stage.into(),
    }
}

pub(super) const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "image",
        author: "The image-rs developers",
        license: include_str!("../../licenses/image-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "rfd",
        author: "The rfd contributors",
        license: include_str!("../../licenses/document/rfd-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "base64",
        author: "Marshall Pierce and contributors",
        license: include_str!("../../licenses/base64-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "vodozemac",
        author: "The Matrix.org Foundation and contributors",
        license: include_str!("../../licenses/vodozemac-Apache-2.0.txt"),
    },
];

fn action(command: Value) -> Value {
    json!({"action":"social","request":command})
}

#[derive(Component, Default)]
struct ServerChoices(Vec<nucleus::social::ServerChoice>);

fn chosen_servers(world: &World, owner: Entity, role: &str) -> Value {
    json!(
        world
            .get::<ServerChoices>(owner)
            .into_iter()
            .flat_map(|choices| choices.0.iter())
            .filter(|choice| match role {
                "query" => choice.query,
                "mailbox" => choice.mailbox,
                _ => choice.publication,
            })
            .map(|choice| &choice.endpoint)
            .collect::<Vec<_>>()
    )
}

pub(super) fn populate(world: &mut World, owner: Entity, parent: Entity) {
    label(
        world,
        parent,
        "Publish small Needs and Contributions, anonymously or with your Organ profile. Public copies may remain after withdrawal.",
    );
    label(
        world,
        parent,
        "A signature proves control of a posting key; it does not verify a person's name, skills or claims. Anonymous posts still expose their text and network activity to selected services.",
    );
    form(
        world,
        owner,
        parent,
        "Refresh My posts and services",
        action(json!({"command":"overview"})),
        vec![],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Requests and private conversations",
        action(json!({"command":"requests"})),
        vec![],
        None,
    );
    compose(world, owner, parent, None, &json!({}));
    form(
        world,
        owner,
        parent,
        "Inspect a social server",
        action(json!({"command":"inspect-service","endpoint":""})),
        vec![Field(
            "/request/endpoint",
            "Pinned server endpoint ID",
            Kind::Text,
            json!(""),
        )],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Reset this device's live private messaging keys",
        action(json!({"command":"reset-private-sessions"})),
        vec![],
        Some(
            "Reset live private keys on this Cell? History and owner authority remain, but old encrypted queues are held and fresh messaging keys need owner authorization.",
        ),
    );
    form(
        world,
        owner,
        parent,
        "Prepare from an existing Need/Contribution",
        action(json!({"command":"prepare-from-record","source":""})),
        vec![Field(
            "/request/source",
            "Existing Record reference",
            Kind::Text,
            json!(""),
        )],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Search local announcements",
        action(
            json!({"command":"search","query":{"text":"","direction":null,"language":"","area":"","concept":"","unit":"","after":null},"services":[]}),
        ),
        vec![
            Field(
                "/request/query/text",
                "Words to find",
                Kind::Text,
                json!(""),
            ),
            Field(
                "/request/query/area",
                "Declared city/region (optional)",
                Kind::Text,
                json!(""),
            ),
            Field(
                "/request/query/language",
                "Language (optional)",
                Kind::Text,
                json!(""),
            ),
            Field(
                "/request/services",
                "Chosen directory endpoint IDs (these services see your query)",
                Kind::TextList,
                json!([]),
            ),
        ],
        None,
    );
}

fn compose(world: &mut World, owner: Entity, parent: Entity, record: Option<&str>, saved: &Value) {
    let draft = nucleus::social::PostDraft::default();
    let mut draft = serde_json::to_value(draft).unwrap();
    if record.is_none() && saved.get("destinations").is_none() {
        draft["destinations"] = chosen_servers(world, owner, "publication");
    }
    if let Some(fields) = saved.as_object() {
        for (key, value) in fields {
            if key != "source" {
                draft[key] = value.clone();
            }
        }
    }
    let fields = vec![
        Field(
            "/request/draft/title",
            "Title",
            Kind::Text,
            draft["title"].clone(),
        ),
        Field(
            "/request/draft/text",
            "Short announcement",
            Kind::Text,
            draft["text"].clone(),
        ),
        Field(
            "/request/draft/direction",
            "What are you posting?",
            choices(&["need", "contribution"]),
            draft["direction"].clone(),
        ),
        Field(
            "/request/draft/mode",
            "Public identity",
            choices(&["anonymous", "identified"]),
            draft["mode"].clone(),
        ),
        Field(
            "/request/draft/alias",
            "Anonymous alias (optional; reuse links your posts)",
            Kind::Text,
            draft["alias"].clone(),
        ),
        Field(
            "/request/draft/quantity",
            "Exact quantity (optional)",
            Kind::OptionalText,
            draft["quantity"].clone(),
        ),
        Field(
            "/request/draft/unit",
            "Public unit label (if quantity supplied)",
            Kind::OptionalText,
            draft["unit"].clone(),
        ),
        Field(
            "/request/draft/concept",
            "Public concept label (optional)",
            Kind::OptionalText,
            draft["concept"].clone(),
        ),
        Field(
            "/request/draft/language",
            "Language",
            Kind::Text,
            draft["language"].clone(),
        ),
        Field(
            "/request/draft/area",
            "Declared city/region (optional)",
            Kind::Text,
            draft["area"].clone(),
        ),
        Field(
            "/request/draft/availability",
            "Availability (optional)",
            Kind::Text,
            draft["availability"].clone(),
        ),
        Field(
            "/request/draft/redistribute",
            "Allow consenting contacts to spread this post",
            boolean(),
            draft["redistribute"].clone(),
        ),
        Field(
            "/request/draft/destinations",
            "Chosen service endpoint IDs, separated by commas",
            Kind::TextList,
            draft["destinations"].clone(),
        ),
        Field(
            "/request/draft/lifetime_days",
            "Announcement lifetime",
            Kind::Choice(vec![
                ("Seven days".into(), Value::Null),
                ("One day".into(), json!(1)),
                ("Three days".into(), json!(3)),
            ]),
            draft["lifetime_days"].clone(),
        ),
    ];
    form(
        world,
        owner,
        parent,
        "Save announcement draft",
        action(
            json!({"command":"save-draft","record":record,"source":saved.get("source").cloned().unwrap_or(Value::Null),"draft":draft}),
        ),
        fields,
        None,
    );
}

pub(super) fn profile(world: &mut World, owner: Entity, parent: Entity, saved: &Value) {
    if saved["editor"]["rotation_pending"] == true {
        label(
            world,
            parent,
            "Device membership changed. The owner device must replace the public editing authority; its publication worker will queue that change when available.",
        );
    }
    let can_edit = saved["editor"]["can_edit"] != false;
    label(
        world,
        parent,
        "One public profile for this Organ. Authorized saves sync to your devices and queue updates to the hosts you choose.",
    );
    let fields = saved
        .get("editor")
        .and_then(|doc| doc.get("fields"))
        .cloned()
        .or_else(|| {
            saved
                .get("published")
                .and_then(|doc| doc.get("fields"))
                .cloned()
        })
        .unwrap_or_else(|| {
            serde_json::to_value(nucleus::social::ProfileFields::default()).unwrap()
        });
    let import = form(
        world,
        owner,
        parent,
        "Prepare public profile image",
        action(json!({"command":"import-profile-image-data","encoded":""})),
        vec![],
        None,
    );
    panel::button(world, import, import, "Choose image…", PickImage);
    for draft in saved["editor"]["pending_drafts"]
        .as_array()
        .into_iter()
        .flatten()
    {
        label(
            world,
            parent,
            "Profile changes and selected prepared images sync between your devices, waiting for owner authorization. The owner rechecks this editor's current permission before publishing.",
        );
        if let Some(error) = draft["error"].as_str() {
            label(world, parent, error);
        }
    }
    disable_edit_form(world, import, can_edit);
    if !can_edit {
        label(
            world,
            parent,
            "Profile viewing only: this device or your login lacks editing authority. Refresh device membership or ask an authorized editor.",
        );
    }
    label(
        world,
        parent,
        "Images are resized to at most 128 KiB each and stripped of source metadata. Preparing an image does not publish it: add its resulting hash below, or clear a hash to remove that image. Selected prepared images travel with an offline draft through Own sync so the owner can publish them.",
    );
    let mut heads = Vec::new();
    if let Some(map) = saved.as_object() {
        let mut children = HashSet::new();
        for (key, doc) in map {
            if !key.starts_with("revision_") {
                continue;
            }
            for parent in doc["parents"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                children.insert(parent.to_owned());
            }
        }
        for key in map.keys().filter_map(|key| key.strip_prefix("revision_")) {
            if !children.contains(key) {
                heads.push(key.to_owned());
            }
        }
    }
    heads.sort();
    if heads.len() > 8 {
        label(
            world,
            parent,
            "More than eight profile branches remain. Each save resolves one group; refresh and save again until all branches converge.",
        );
        heads.truncate(8);
    }
    if heads.len() > 1 {
        label(
            world,
            parent,
            "Concurrent profile edits need resolution. Saving the selected fields resolves the listed branches.",
        );
        for conflict in saved["editor"]["conflicts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            label(
                world,
                parent,
                &format!(
                    "Choose {}: {}",
                    conflict["field"].as_str().unwrap_or("field"),
                    conflict["values"]
                ),
            );
        }
    }
    let save = form(
        world,
        owner,
        parent,
        "Save shared public profile",
        action(
            json!({"command":"save-profile","fields":fields,"parents":heads,"destinations":saved["destinations"].as_array().cloned().unwrap_or_default()}),
        ),
        vec![
            Field(
                "/request/fields/name",
                "Public display name",
                Kind::Text,
                fields["name"].clone(),
            ),
            Field(
                "/request/fields/description",
                "Public description",
                Kind::Text,
                fields["description"].clone(),
            ),
            Field(
                "/request/fields/area",
                "Declared city/region (optional)",
                Kind::Text,
                fields["area"].clone(),
            ),
            Field(
                "/request/fields/contact",
                "Public contact route (optional)",
                Kind::Text,
                fields["contact"].clone(),
            ),
            Field(
                "/request/fields/avatar",
                "Prepared avatar hash (optional)",
                Kind::OptionalText,
                fields["avatar"].clone(),
            ),
            Field(
                "/request/fields/banner",
                "Prepared banner hash (optional)",
                Kind::OptionalText,
                fields["banner"].clone(),
            ),
            Field(
                "/request/destinations",
                "Chosen profile host endpoint IDs, separated by commas",
                Kind::TextList,
                saved["destinations"]
                    .as_array()
                    .cloned()
                    .map(Value::Array)
                    .unwrap_or_else(|| json!([])),
            ),
        ],
        Some(
            "Review these public profile fields and selected hosts before saving the signed update.",
        ),
    );
    disable_edit_form(world, save, can_edit);
    form(
        world,
        owner,
        parent,
        "Reload public profile",
        action(json!({"command":"overview"})),
        vec![],
        None,
    );
    if !saved["published"].is_null() {
        let rotate = form(
            world,
            owner,
            parent,
            "Change public editing authority",
            action(json!({"command":"rotate-profile-authority"})),
            vec![],
            Some(
                "Change the public editing key on the owner device? Save your profile afterwards. Existing identified posts can be withdrawn and replaced. Hosts receive the revocation when online; earlier authority expires within seven days. Your anonymous posts keep their separate identities.",
            ),
        );
        disable_edit_form(
            world,
            rotate,
            can_edit && saved["editor"]["can_rotate"] != false,
        );
        label(
            world,
            parent,
            &format!(
                "Current profile state: {}",
                saved["published"]["state"].as_str().unwrap_or("saved")
            ),
        );
        let withdraw = form(
            world,
            owner,
            parent,
            "Withdraw public profile from its hosts",
            action(json!({"command":"withdraw-profile","parents":heads})),
            vec![],
            Some(
                "End hosting for this public profile? Previously copied profiles may remain elsewhere.",
            ),
        );
        disable_edit_form(world, withdraw, can_edit);
    }
}

fn disable_edit_form(world: &mut World, form: Entity, enabled: bool) {
    if enabled {
        return;
    }
    let buttons: Vec<_> = world
        .query::<(Entity, &crate::actions::ActionButton)>()
        .iter(world)
        .filter_map(|(entity, _)| {
            let mut parent = entity;
            while let Some(child) = world.get::<ChildOf>(parent) {
                parent = child.parent();
                if parent == form {
                    return Some(entity);
                }
            }
            None
        })
        .collect();
    for button in buttons {
        world
            .entity_mut(button)
            .insert(bevy::ui::InteractionDisabled);
    }
}

pub(super) fn profile_confirmation(world: &mut World, parent: Entity, request: &Value) {
    for key in ["name", "description", "area", "contact", "avatar", "banner"] {
        let value = &request["fields"][key];
        label(
            world,
            parent,
            &format!("{key}: {}", value.as_str().unwrap_or("none")),
        );
    }
    label(
        world,
        parent,
        &format!("Chosen hosts: {}", request["destinations"]),
    );
    label(
        world,
        parent,
        "Exact addresses or personal contact details entered above become public. Previous public copies can remain.",
    );
}

pub(super) fn result(world: &mut World, owner: Entity, parent: Entity, value: &Value) {
    if let Some(delivery) = value.get("private_delivery") {
        resend::view(world, owner, parent, delivery, value["can_edit"] == true);
        return;
    }
    if subscriptions::view(world, owner, parent, value) {
        return;
    }
    if reports::view(world, owner, parent, value) {
        return;
    }
    if moderation::view(world, owner, parent, value) {
        return;
    }
    if let Some(health) = value.get("service_health") {
        operator::view(world, parent, health);
        return;
    }
    if let Some(asks) = value.get("asks").filter(|asks| asks.is_object()) {
        ask::view(
            world,
            owner,
            parent,
            asks,
            value["can_manage_services"] == true,
        );
    }
    if let Some(gossip) = value.get("gossip") {
        gossip_view(
            world,
            owner,
            parent,
            gossip,
            value["can_manage_services"] == true,
        );
    }
    if let Some(servers) = value.get("servers") {
        let choices: Vec<nucleus::social::ServerChoice> =
            serde_json::from_value(servers.clone()).unwrap_or_default();
        world.entity_mut(owner).insert(ServerChoices(choices));
        servers_view(
            world,
            owner,
            parent,
            servers,
            value["can_manage_services"] == true,
        );
    }
    if let Some(descriptor) = value.get("descriptor") {
        label(
            world,
            parent,
            value["status"].as_str().unwrap_or("Selected server"),
        );
        let endpoint = descriptor["endpoint"].as_str().unwrap_or_default();
        label(world, parent, endpoint);
        qr::code(world, parent, endpoint);
        label(
            world,
            parent,
            &format!(
                "Roles: {} · statement valid until {}",
                descriptor["roles"], descriptor["expires_at"]
            ),
        );
        label(
            world,
            parent,
            &format!(
                "Configured storage: {} bytes · public documents: {} · incoming/outgoing: {}/{} bytes per minute",
                descriptor["settings"]["storage_bytes"],
                descriptor["settings"]["cache_entries"],
                descriptor["settings"]["incoming_bytes_per_minute"],
                descriptor["settings"]["outgoing_bytes_per_minute"]
            ),
        );
        label(
            world,
            parent,
            &format!(
                "Encrypted mail retention: up to {} days. Sender expiry may be sooner.",
                descriptor["mailbox_retention_days"]
            ),
        );
        for key in ["contact", "policy"] {
            if let Some(text) = descriptor["settings"][key]
                .as_str()
                .filter(|s| !s.is_empty())
            {
                label(world, parent, &format!("Operator {key}: {text}"));
            }
        }
        let choice = json!({"endpoint":descriptor["endpoint"],"label":"","operator":"","publication":false,"query":false,"mailbox":false});
        server_form(
            world,
            owner,
            parent,
            &choice,
            value["can_manage_services"] == true,
        );
    } else if let Some(requests) = value["requests"].as_array() {
        let can_edit = value["can_edit"] != false;
        label(
            world,
            parent,
            value["status"].as_str().unwrap_or("Private requests"),
        );
        if requests.is_empty() {
            label(
                world,
                parent,
                "No private requests yet. Find an announcement with private replies enabled and write an introduction.",
            );
        }
        for failure in value["receive_failures"].as_array().into_iter().flatten() {
            label(
                world,
                parent,
                &format!(
                    "Unreadable mailbox message at {}: {}",
                    failure["service"].as_str().unwrap_or_default(),
                    failure["error"].as_str().unwrap_or_default()
                ),
            );
            if failure["discard"] == true {
                label(
                    world,
                    parent,
                    "Discard saved; waiting for original mailbox authority and host confirmation.",
                );
            } else {
                let discard = form(
                    world,
                    owner,
                    parent,
                    "Discard unreadable ciphertext",
                    action(
                        json!({"command":"discard-private","context":failure["context"],"service":failure["service"],"envelope":failure["envelope"]}),
                    ),
                    vec![],
                    Some(
                        "Refuse this encrypted copy at the selected mailbox? It may still become readable after history or keys arrive. The sender receives a signed refusal; already retained history and other copies remain.",
                    ),
                );
                disable_edit_form(world, discard, can_edit);
            }
        }
        for request in requests {
            let root = request["record"].as_str().unwrap_or_default();
            label(
                world,
                parent,
                request["title"].as_str().unwrap_or("Private conversation"),
            );
            if request["draft"] == true {
                label(
                    world,
                    parent,
                    request["state"]["draft"]["text"]
                        .as_str()
                        .unwrap_or_default(),
                );
                notice(world,parent,request["state"]["error"].as_str().unwrap_or("Saved introduction waiting for owner authorization and selected mailbox registration"));
                let resume = form(
                    world,
                    owner,
                    parent,
                    "Resume this introduction on this device",
                    action(json!({"command":"resume-request","record":root})),
                    vec![],
                    None,
                );
                disable_edit_form(
                    world,
                    resume,
                    can_edit && request["state"]["expired"] != true,
                );
                let archive = form(
                    world,
                    owner,
                    parent,
                    "Discard saved introduction",
                    action(json!({"command":"archive-request","record":root})),
                    vec![],
                    Some(
                        "Discard this introduction on your devices? Previously retained copies on another person's devices cannot be erased by this action.",
                    ),
                );
                disable_edit_form(world, archive, can_edit);
                continue;
            }
            label(
                world,
                parent,
                request["state"]["state"].as_str().unwrap_or("pending"),
            );
            for message in request["messages"].as_array().into_iter().flatten().rev() {
                label(world, parent, message["body"].as_str().unwrap_or_default());
                if let Some(stage) = message["delivery"]["stage"].as_str() {
                    label(
                        world,
                        parent,
                        &delivery_label(stage, &message["destinations"]),
                    );
                }
                if let Some(error) = message["delivery"]["error"].as_str() {
                    label(world, parent, error);
                }
                for destination in message["destinations"].as_array().into_iter().flatten() {
                    if let Some(error) = destination["error"].as_str() {
                        label(
                            world,
                            parent,
                            &format!(
                                "Mailbox {}: {}",
                                destination["service"].as_str().unwrap_or_default(),
                                error
                            ),
                        );
                    }
                }
                if message["delivery"].is_object()
                    && message["delivery"]["stage"] != "recipient-durable"
                    && message["delivery"]["stage"] != "recipient-refused"
                {
                    form(
                        world,
                        owner,
                        parent,
                        "Review delivery or resume this saved message",
                        action(
                            json!({"command":"private-delivery-status","message":message["uid"]}),
                        ),
                        vec![],
                        None,
                    );
                }
            }
            if matches!(
                request["state"]["state"].as_str(),
                Some("pending" | "accepted")
            ) {
                panel::button(
                    world,
                    parent,
                    owner,
                    "Open conversation Thread",
                    super::Command::Open(root.to_owned(), false),
                );
                let reply = form(
                    world,
                    owner,
                    parent,
                    "Send private reply",
                    action(json!({"command":"send-private","conversation":root,"text":""})),
                    vec![Field(
                        "/request/text",
                        "Private message",
                        Kind::Text,
                        json!(""),
                    )],
                    None,
                );
                disable_edit_form(world, reply, can_edit);
                if request["state"]["state"] == "accepted" {
                    let reveal = form(
                        world,
                        owner,
                        parent,
                        "Reveal my Organ profile in this conversation",
                        action(json!({"command":"reveal-profile","conversation":root})),
                        vec![],
                        Some(
                            "Share your current reviewed Organ profile with this person? This deliberately links your private identity here to that profile. The message cannot be taken back from their saved history.",
                        ),
                    );
                    disable_edit_form(world, reveal, can_edit);
                    if let Some(profile) = request["reveal"]["peer"].get("profile") {
                        label(
                            world,
                            parent,
                            &format!(
                                "Revealed Organ: {} · profile valid until {}",
                                profile["fields"]["name"].as_str().unwrap_or_default(),
                                profile["expires_at"]
                            ),
                        );
                    }
                    if request["reveal"]["connected"] == true {
                        label(
                            world,
                            parent,
                            if request["verification"]["contact_present"] == true {
                                "Mutual consent and known contact are retained. Private replication remains separately controlled in contact settings."
                            } else {
                                "Mutual consent is retained; this device is waiting for a verified known contact."
                            },
                        );
                    } else if request["reveal"]["local_connect"] == true {
                        notice(
                            world,
                            parent,
                            "You chose Connect; waiting for the other person's consent.",
                        );
                    } else if request["reveal"]["local"].get("profile").is_some()
                        && request["reveal"]["peer"].get("profile").is_some()
                    {
                        let connect = form(
                            world,
                            owner,
                            parent,
                            "Connect as known contacts",
                            action(json!({"command":"connect-participant","conversation":root})),
                            vec![],
                            Some(
                                "Connect to this verified Organ? Both people must consent. This creates a contact with private-data replication disabled and keeps this conversation's history.",
                            ),
                        );
                        disable_edit_form(
                            world,
                            connect,
                            can_edit && request["verification"]["can_connect"] == true,
                        );
                    }
                    if let Some(recovery) = request["verification"]["recovery"].as_str() {
                        notice(world, parent, recovery);
                    }
                }
                for (name, decision) in [
                    ("Accept conversation", "accept"),
                    ("Decline request", "decline"),
                    ("Block participant", "block"),
                    ("Close conversation", "close"),
                ] {
                    if decision == "accept" && request["state"]["local_accepted"] == true
                        || decision == "decline" && request["state"]["state"] != "pending"
                    {
                        continue;
                    }
                    let control = form(
                        world,
                        owner,
                        parent,
                        name,
                        action(
                            json!({"command":"decide-request","conversation":root,"decision":decision}),
                        ),
                        vec![],
                        Some(
                            "This updates private sender admission at your selected mailboxes. Previously saved history remains on your devices and cannot erase the other person's copy.",
                        ),
                    );
                    disable_edit_form(world, control, can_edit);
                }
            } else {
                let archive = form(
                    world,
                    owner,
                    parent,
                    "Archive this private conversation",
                    action(json!({"command":"archive-request","record":root})),
                    vec![],
                    Some(
                        "Archive retained history on your devices after final delivery completes or expires? This cannot erase the other person's copy.",
                    ),
                );
                disable_edit_form(world, archive, can_edit);
            }
        }
        for block in value["blocks"].as_array().into_iter().flatten() {
            label(
                world,
                parent,
                &format!(
                    "Blocked private identity: {}",
                    block["peer"].as_str().unwrap_or_default()
                ),
            );
            let control = form(
                world,
                owner,
                parent,
                "Unblock this private identity",
                action(
                    json!({"command":"unblock-participant","context":block["context"],"peer":block["peer"]}),
                ),
                vec![],
                Some(
                    "Allow one fresh introduction from this private identity? Old conversations stay closed. Someone who replaces all their keys cannot be recognized by this block list.",
                ),
            );
            disable_edit_form(world, control, can_edit);
        }
        if let Some(after) = value["next_after"].as_str() {
            form(
                world,
                owner,
                parent,
                "Next Requests page",
                action(json!({"command":"requests","after":after})),
                vec![],
                None,
            );
        }
        return;
    }
    if value.get("source").is_some() && value.get("draft").is_some() {
        label(
            world,
            parent,
            value["status"]
                .as_str()
                .unwrap_or("Review the source fields"),
        );
        let mut draft = value["draft"].clone();
        draft["source"] = value["source"].clone();
        compose(world, owner, parent, None, &draft);
        return;
    }
    if let Some(posts) = value["posts"].as_array() {
        service_settings(
            world,
            owner,
            parent,
            &value["settings"],
            value["can_manage_services"] == true,
            value["services_managed"] == true,
        );
        for post in posts {
            let record = post["record"].as_str().unwrap_or_default();
            label(
                world,
                parent,
                post["draft"]["title"].as_str().unwrap_or("Announcement"),
            );
            let mut draft = post["draft"].clone();
            draft["source"] = post["source"].clone();
            compose(world, owner, parent, Some(record), &draft);
            if let Some(error) = post["source_error"].as_str() {
                label(world, parent, error);
            }
            if post["posting_authority_changed"] == true {
                label(
                    world,
                    parent,
                    "The owner replaced this post's editing key. Review and publish its current draft or withdrawal to update the selected hosts.",
                );
            }
            if post["draft"]["mode"] == "anonymous"
                && post["posting_authority"]["expires_at"]
                    .as_i64()
                    .is_none_or(|at| at <= chrono::Utc::now().timestamp())
            {
                label(
                    world,
                    parent,
                    "Anonymous editing waits for owner authorization. Your draft and retained history remain saved.",
                );
            }
            if let Some(candidate) = post.get("source_draft").filter(|v| !v.is_null()) {
                label(
                    world,
                    parent,
                    "The private source changed. Review this candidate before replacing your draft; publication requires a separate preview.",
                );
                let mut candidate = candidate.clone();
                candidate["source"] = post["source"].clone();
                compose(world, owner, parent, Some(record), &candidate);
            }
            if let Some(published) = post.get("published").filter(|v| !v.is_null()) {
                let state = if published["expires_at"]
                    .as_i64()
                    .is_some_and(|at| at <= chrono::Utc::now().timestamp())
                {
                    "expired"
                } else {
                    published["state"].as_str().unwrap_or("draft")
                };
                label(
                    world,
                    parent,
                    &format!(
                        "State: {state} · public revision {}",
                        published["revision"].as_str().unwrap_or_default()
                    ),
                );
            } else {
                label(world, parent, "State: draft saved on your devices");
            }
            form(
                world,
                owner,
                parent,
                "Private reply key status",
                action(json!({"command":"reply-key-status","record":record})),
                vec![],
                None,
            );
            form(
                world,
                owner,
                parent,
                "Prepare this device's private reply keys",
                action(
                    json!({"command":"prepare-reply-keys","record":record,"services":chosen_servers(world,owner,"mailbox")}),
                ),
                vec![Field(
                    "/request/services",
                    "Chosen mailbox host identities",
                    Kind::TextList,
                    chosen_servers(world, owner, "mailbox"),
                )],
                Some(
                    "Prepare separate keys for this device? The owner authorizes them through your own sync. Review and publish the next announcement preview to advertise private replies at your selected mailboxes.",
                ),
            );
            for (index, draft) in post["drafts"].as_array().into_iter().flatten().enumerate() {
                form(
                    world,
                    owner,
                    parent,
                    &format!(
                        "Use preserved draft {}: {}",
                        index + 1,
                        draft["title"].as_str().unwrap_or("announcement")
                    ),
                    action(
                        json!({"command":"save-draft","record":record,"source":post["source"],"draft":draft}),
                    ),
                    vec![],
                    None,
                );
            }
            for (caption, state) in [
                ("Preview publication/update", "active"),
                ("Preview pause", "paused"),
                ("Preview fulfilled", "fulfilled"),
                ("Preview withdrawal", "withdrawn"),
            ] {
                form(
                    world,
                    owner,
                    parent,
                    caption,
                    action(json!({"command":"preview","record":record,"state":state})),
                    vec![],
                    None,
                );
            }
            form(
                world,
                owner,
                parent,
                "Archive ended announcement",
                action(json!({"command":"archive-post","record":record})),
                vec![],
                Some(
                    "Archive this ended announcement locally? Its public ending must have reached the selected hosts or expired. Other people's copies remain.",
                ),
            );
        }
        if let Some(after) = value["next_posts_after"].as_str() {
            form(
                world,
                owner,
                parent,
                "Next My posts page",
                action(json!({"command":"post-page","after":after})),
                vec![],
                None,
            );
        }
        if !value["profile"].is_null() {
            profile(world, owner, parent, &value["profile"]);
        }
        for job in value["jobs"].as_array().into_iter().flatten() {
            label(
                world,
                parent,
                &format!(
                    "{}: {} · {}",
                    job["kind"].as_str().unwrap_or("Publication"),
                    job["state"].as_str().unwrap_or("pending"),
                    job["destination"].as_str().unwrap_or_default()
                ),
            );
            if let Some(error) = job["error"].as_str() {
                label(world, parent, error);
            }
            if matches!(
                job["state"].as_str(),
                Some("cancelled" | "failed" | "expired")
            ) && job["receipt"]["accepted"] == true
            {
                label(
                    world,
                    parent,
                    "The host acknowledged this earlier version; it is no longer the current publication.",
                );
            }
        }
        if posts.is_empty() {
            label(
                world,
                parent,
                "No saved announcements yet. Compose one above.",
            );
        }
    } else if let Some(draft) = value.get("profile_draft") {
        label(
            world,
            parent,
            value["status"].as_str().unwrap_or("Profile draft saved"),
        );
        profile(
            world,
            owner,
            parent,
            &json!({"editor":{"fields":draft["fields"],"can_edit":true},"destinations":draft["destinations"]}),
        );
    } else if value.get("preview_hash").is_some() {
        let doc = &value["document"];
        label(
            world,
            parent,
            "Public preview · review these fields before publishing",
        );
        if doc["mode"] == "identified" {
            label(
                world,
                parent,
                &format!(
                    "Public Organ identity: {}",
                    doc["profile"]["organ"].as_str().unwrap_or_default()
                ),
            );
        }
        if doc["resolves"]
            .as_array()
            .is_some_and(|heads| heads.len() > 1)
        {
            label(
                world,
                parent,
                "This update resolves concurrent publication branches using the fields displayed below. Check each field before publishing.",
            );
        }
        if doc["destinations"].as_array().is_some_and(Vec::is_empty) {
            label(
                world,
                parent,
                "No hosts selected. This publication stays in your local discovery cache.",
            );
        }
        label(
            world,
            parent,
            &format!(
                "{} · {} · {}",
                doc["mode"].as_str().unwrap_or_default(),
                doc["direction"].as_str().unwrap_or_default(),
                doc["state"].as_str().unwrap_or_default()
            ),
        );
        label(world, parent, doc["title"].as_str().unwrap_or_default());
        label(world, parent, doc["text"].as_str().unwrap_or_default());
        for (field, caption) in [
            ("alias", "Alias"),
            ("quantity", "Quantity"),
            ("unit", "Unit"),
            ("concept", "Concept"),
            ("language", "Language"),
            ("area", "Area"),
            ("availability", "Availability"),
        ] {
            if let Some(text) = doc[field].as_str().filter(|text| !text.is_empty()) {
                label(world, parent, &format!("{caption}: {text}"));
            }
        }
        label(
            world,
            parent,
            &format!(
                "Redistribution: {} · expires: {}",
                doc["redistribute"],
                chrono::DateTime::from_timestamp(doc["expires_at"].as_i64().unwrap_or(0), 0)
                    .map(|time| time.to_rfc3339())
                    .unwrap_or_default()
            ),
        );
        for destination in doc["destinations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            label(world, parent, &format!("Selected service: {destination}"));
        }
        form(
            world,
            owner,
            parent,
            "Publish this exact preview",
            action(
                json!({"command":"publish","record":value["record"],"preview_hash":value["preview_hash"],"document":doc}),
            ),
            vec![],
            Some("Publish the displayed fields to the selected destinations?"),
        );
    } else if let Some(record) = value["record"].as_str() {
        label(world, parent, value["status"].as_str().unwrap_or("Saved"));
        if let Some(status) = value["reply_keys"].as_str() {
            label(world, parent, &format!("Private reply authority: {status}"));
            if let Some(detail) = value["detail"].as_str() {
                label(world, parent, detail);
            }
            if let Some(time) = value["retire_after"].as_i64() {
                label(
                    world,
                    parent,
                    &format!(
                        "Earliest transport-key retirement: {}",
                        chrono::DateTime::from_timestamp(time, 0)
                            .map(|time| time.to_rfc3339())
                            .unwrap_or_default()
                    ),
                );
            }
            if let Some(expiry) = value["route"]["certificate"]["expires_at"].as_i64() {
                label(
                    world,
                    parent,
                    &format!(
                        "Authorized until {}",
                        chrono::DateTime::from_timestamp(expiry, 0)
                            .map(|time| time.to_rfc3339())
                            .unwrap_or_default()
                    ),
                );
            }
        }
        if value.get("draft").is_some() {
            form(
                world,
                owner,
                parent,
                "Preview saved announcement",
                action(json!({"command":"preview","record":record,"state":"active"})),
                vec![],
                None,
            );
        }
    } else if let Some(results) = value["results"].as_array() {
        discovery_sources::summary(world, owner, parent, value);
        if let Some(status) = value["status"].as_str() {
            label(world, parent, status);
        }
        if let Some(source) = value["source"].as_str() {
            label(world, parent, source);
        }
        if let Some(freshness) = value["freshness"].as_str() {
            label(world, parent, freshness);
        }
        if let Some(time) = value["checked_at"].as_i64() {
            label(
                world,
                parent,
                &format!(
                    "Last completed refresh: {}",
                    chrono::DateTime::from_timestamp(time, 0)
                        .map(|time| time.to_rfc3339())
                        .unwrap_or_default()
                ),
            );
        }
        if results.is_empty() {
            label(world, parent, "No matching visible cached announcements.");
        }
        for row in results {
            let doc = &row["document"];
            moderation::controls(
                world,
                owner,
                parent,
                doc,
                value["can_manage_services"] == true,
            );
            reports::controls(
                world,
                owner,
                parent,
                doc,
                value["can_manage_services"] == true,
            );
            label(
                world,
                parent,
                &format!(
                    "{} · {}",
                    doc["title"].as_str().unwrap_or_default(),
                    doc["mode"].as_str().unwrap_or_default()
                ),
            );
            label(world, parent, doc["text"].as_str().unwrap_or_default());
            if !doc["reply"].is_null() && doc["state"] == "active" {
                form(
                    world,
                    owner,
                    parent,
                    "Write a private introduction",
                    action(
                        json!({"command":"open-request","post":doc,"text":"","alias":"","services":chosen_servers(world,owner,"mailbox")}),
                    ),
                    vec![
                        Field(
                            "/request/text",
                            "Short private introduction (up to 2 KiB)",
                            Kind::Text,
                            json!(""),
                        ),
                        Field(
                            "/request/alias",
                            "Private alias (optional)",
                            Kind::Text,
                            json!(""),
                        ),
                        Field(
                            "/request/services",
                            "Your selected return mailbox endpoint IDs",
                            Kind::TextList,
                            chosen_servers(world, owner, "mailbox"),
                        ),
                    ],
                    None,
                );
            }
            if let Some(organ) = doc["profile"]["organ"].as_str() {
                form(
                    world,
                    owner,
                    parent,
                    "View public Organ profile",
                    action(
                        json!({"command":"fetch-profile","organ":organ,"services":doc["destinations"]}),
                    ),
                    vec![],
                    None,
                );
            }
            discovery_sources::row(world, parent, row);
        }
        for failure in value["failures"].as_array().into_iter().flatten() {
            label(
                world,
                parent,
                &format!(
                    "{}: {}",
                    failure["service"].as_str().unwrap_or_default(),
                    failure["error"].as_str().unwrap_or("Service unavailable")
                ),
            );
        }
        if let Some(after) = value["next_after"].as_str() {
            let mut query = value["query"].clone();
            query["after"] = json!(after);
            form(
                world,
                owner,
                parent,
                "Continue browsing announcements",
                action(json!({"command":"search","query":query,"services":value["services"]})),
                vec![],
                None,
            );
        }
    } else if let Some(hash) = value["asset_hash"].as_str() {
        label(
            world,
            parent,
            value["status"].as_str().unwrap_or("Image prepared"),
        );
        label(world, parent, hash);
        qr::code(world, parent, hash);
    } else if value["image_hash"].is_string() {
        label(
            world,
            parent,
            "Public image verified. Loading the selected image…",
        );
        load_image(world, parent, value);
    } else if let Some(profile) = value.get("profile") {
        if value["state"] == "revoked" {
            label(
                world,
                parent,
                "This copy uses revoked editing authority. Refresh the public profile before relying on its presentation.",
            );
            return;
        }
        if value["state"] == "conflict" {
            label(
                world,
                parent,
                "This host has conflicting profile edits. The displayed copy is one branch; the owner must resolve them.",
            );
        }
        if value["state"] == "withdrawn" || profile["state"] == "withdrawn" {
            label(world, parent, "This public profile has been withdrawn.");
            return;
        }
        label(
            world,
            parent,
            profile["fields"]["name"]
                .as_str()
                .unwrap_or("Profile saved"),
        );
        label(world, parent, value["status"].as_str().unwrap_or("Saved"));
        label(
            world,
            parent,
            profile["fields"]["description"]
                .as_str()
                .unwrap_or_default(),
        );
        for key in ["area", "contact"] {
            if let Some(text) = profile["fields"][key]
                .as_str()
                .filter(|value| !value.is_empty())
            {
                label(world, parent, &format!("{key}: {text}"));
            }
        }
        for key in ["avatar", "banner"] {
            if let Some(hash) = profile["fields"][key].as_str() {
                form(
                    world,
                    owner,
                    parent,
                    &format!("Load public {key}"),
                    action(
                        json!({"command":"fetch-profile-image","organ":profile["authority"]["organ"],"hash":hash,"services":profile["destinations"]}),
                    ),
                    vec![],
                    None,
                );
            }
        }
    }
}

#[derive(Clone)]
struct PickImage;

#[derive(Component)]
struct ImagePicker(tokio::sync::oneshot::Receiver<Result<Option<(String, String)>, String>>);

impl Action for PickImage {
    fn apply(&self, world: &mut World, target: Entity) {
        if world.get::<ImagePicker>(target).is_some() {
            return;
        }
        let (send, receive) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .add_filter("PNG/JPEG", &["png", "jpg", "jpeg"])
                .pick_file();
            let result = (|| {
                use base64::Engine as _;
                use std::io::Read;
                let Some(path) = path else { return Ok(None) };
                if !std::fs::metadata(&path)
                    .map_err(|_| "Cannot inspect the selected image")?
                    .is_file()
                {
                    return Err("Choose a regular PNG/JPEG image file".into());
                }
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .map_err(|_| "Cannot read the selected image")?
                    .take(4 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "Cannot read the selected image")?;
                if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
                    return Err("Choose an image file up to 4 MiB".into());
                }
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().chars().take(160).collect())
                    .unwrap_or_default();
                Ok(Some((
                    name,
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                )))
            })();
            let _ = send.send(result);
        });
        world.entity_mut(target).insert(ImagePicker(receive));
    }
}

type Pixels = (u32, u32, Vec<u8>);

#[derive(Component)]
struct ImageDecode {
    receiver: tokio::sync::oneshot::Receiver<Result<Pixels, String>>,
    _reservation: std::sync::Arc<ImageReservation>,
}

static IMAGE_DECODERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct ImageReservation;

impl Drop for ImageReservation {
    fn drop(&mut self) {
        IMAGE_DECODERS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

fn load_image(world: &mut World, parent: Entity, value: &Value) {
    if IMAGE_DECODERS
        .fetch_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |count| (count < 2).then_some(count + 1),
        )
        .is_err()
    {
        label(
            world,
            parent,
            "Two images are already loading. Try loading this image again when they finish.",
        );
        return;
    }
    let encoded = value["encoded"].as_str().unwrap_or_default().to_owned();
    let hash = value["image_hash"].as_str().unwrap_or_default().to_owned();
    let (send, receive) = tokio::sync::oneshot::channel();
    let reservation = std::sync::Arc::new(ImageReservation);
    let thread_reservation = reservation.clone();
    std::thread::spawn(move || {
        let _reservation = thread_reservation;
        use base64::Engine as _;
        let result = (|| {
            if encoded.len() > 175_000 {
                return Err("Public image is too large".into());
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| "Malformed public image")?;
            if nucleus::fact::sha256_hex(&bytes) != hash {
                return Err("Public image hash differs".into());
            }
            let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|_| "Invalid public image")?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(2048);
            limits.max_image_height = Some(2048);
            limits.max_alloc = Some(64 * 1024 * 1024);
            reader.limits(limits);
            let image = reader
                .decode()
                .map_err(|_| "Public image exceeds decoder limits")?
                .to_rgba8();
            Ok((image.width(), image.height(), image.into_raw()))
        })();
        let _ = send.send(result);
    });
    world.entity_mut(parent).insert(ImageDecode {
        receiver: receive,
        _reservation: reservation,
    });
}

pub(super) fn poll_media(world: &mut World) {
    let picks: Vec<_> = world
        .query::<(Entity, &mut ImagePicker)>()
        .iter_mut(world)
        .filter_map(|(entity, mut job)| match job.0.try_recv() {
            Ok(path) => Some((entity, path)),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                Some((entity, Err("Image selection stopped".into())))
            }
            Err(_) => None,
        })
        .collect();
    for (entity, result) in picks {
        world.entity_mut(entity).remove::<ImagePicker>();
        match result {
            Ok(Some((name, encoded))) => {
                if let Some(mut form) = world.get_mut::<forms::Form>(entity) {
                    form.payload["request"]["encoded"] = json!(encoded);
                }
                label(
                    world,
                    entity,
                    &format!("Selected: {name}. Prepare the image to obtain its public hash."),
                );
            }
            Err(error) => {
                label(world, entity, &error);
            }
            Ok(None) => {}
        }
    }
    let images: Vec<_> = world
        .query::<(Entity, &mut ImageDecode)>()
        .iter_mut(world)
        .filter_map(|(entity, mut job)| match job.receiver.try_recv() {
            Ok(result) => Some((entity, result)),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                Some((entity, Err("Image decoder stopped".into())))
            }
            Err(_) => None,
        })
        .collect();
    for (entity, result) in images {
        world.entity_mut(entity).remove::<ImageDecode>();
        match result {
            Ok((width, height, pixels)) => {
                world.init_resource::<Assets<Image>>();
                let handle = world.resource_mut::<Assets<Image>>().add(Image::new(
                    bevy::render::render_resource::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    pixels,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                ));
                world.spawn((
                    ChildOf(entity),
                    ImageNode::new(handle),
                    Node {
                        max_width: px(512.0),
                        width: percent(100.0),
                        aspect_ratio: Some(width as f32 / height as f32),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ));
            }
            Err(error) => {
                label(
                    world,
                    entity,
                    &format!("{error}. The text profile remains available."),
                );
            }
        }
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;

    #[test]
    fn pending_image_results_hold_the_two_worker_budget_until_consumed() {
        use base64::Engine as _;
        let mut app = crate::sand_panel::tests::app();
        app.add_plugins(OrganCastlePlugin);
        let root = app.world_mut().spawn_empty().id();
        let owner = crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            crate::sand_store::SandKind::Organ,
            "",
            bevy::math::DVec2::ZERO,
        );
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .encode_image(&image::DynamicImage::new_rgb8(1, 1))
            .unwrap();
        let value = json!({"image_hash":nucleus::fact::sha256_hex(&bytes),
            "encoded":base64::engine::general_purpose::STANDARD.encode(bytes)});
        let parents: Vec<Entity> = (0..3)
            .map(|_| app.world_mut().spawn(Node::default()).id())
            .collect();
        for parent in &parents {
            result(app.world_mut(), owner, *parent, &value);
        }
        assert!(app.world().get::<ImageDecode>(parents[0]).is_some());
        assert!(app.world().get::<ImageDecode>(parents[1]).is_some());
        assert!(app.world().get::<ImageDecode>(parents[2]).is_none());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while parents[..2]
            .iter()
            .any(|parent| app.world().get::<ImageDecode>(*parent).is_some())
        {
            poll_media(app.world_mut());
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        result(app.world_mut(), owner, parents[2], &value);
        assert!(app.world().get::<ImageDecode>(parents[2]).is_some());
        while app.world().get::<ImageDecode>(parents[2]).is_some() {
            poll_media(app.world_mut());
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
}

fn service_settings(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    settings: &Value,
    can_edit: bool,
    managed: bool,
) {
    if managed {
        label(
            world,
            parent,
            "Hosting roles and limits come from this server's deployment configuration. Change that configuration and restart the service to update them.",
        );
    }
    moderation::settings(world, owner, parent, can_edit);
    reports::settings(world, owner, parent, can_edit);
    subscriptions::settings(world, owner, parent, can_edit);
    for (title, command, confirmation) in [
        ("Refresh social service health", "service-health", None),
        (
            "Rebuild public search index",
            "rebuild-public-index",
            Some(
                "Rebuild search from currently valid public announcements? Retained withdrawal and revocation evidence will remain.",
            ),
        ),
    ] {
        let control = form(
            world,
            owner,
            parent,
            title,
            action(json!({"command":command})),
            vec![],
            confirmation,
        );
        disable_edit_form(world, control, can_edit);
    }
    label(
        world,
        parent,
        "This device can host public listings or retain encrypted messages for people who are offline. Keep it online to serve them. Each role is enabled separately; operator contact and policy become public.",
    );
    let settings_form = form(
        world,
        owner,
        parent,
        "Save social service roles",
        action(json!({"command":"configure-services","settings":settings})),
        vec![
            Field(
                "/request/settings/directory",
                "Host chosen public posts and profiles",
                boolean(),
                settings["directory"].clone(),
            ),
            Field(
                "/request/settings/townsquare",
                "Allow public browsing and search",
                boolean(),
                settings["townsquare"].clone(),
            ),
            Field(
                "/request/settings/cache_entries",
                "Maximum cached public documents",
                Kind::Number,
                settings["cache_entries"].clone(),
            ),
            Field(
                "/request/settings/mailbox",
                "Retain admitted encrypted messages",
                boolean(),
                settings["mailbox"].clone(),
            ),
            Field(
                "/request/settings/storage_bytes",
                "Configured storage budget in bytes",
                Kind::Number,
                settings["storage_bytes"].clone(),
            ),
            Field(
                "/request/settings/contact",
                "Public operator contact (optional)",
                Kind::Text,
                settings["contact"].clone(),
            ),
            Field(
                "/request/settings/policy",
                "Public operator policy (optional)",
                Kind::Text,
                settings["policy"].clone(),
            ),
            Field(
                "/request/settings/incoming_bytes_per_minute",
                "Incoming bytes per minute",
                Kind::Number,
                settings["incoming_bytes_per_minute"].clone(),
            ),
            Field(
                "/request/settings/outgoing_bytes_per_minute",
                "Outgoing bytes per minute",
                Kind::Number,
                settings["outgoing_bytes_per_minute"].clone(),
            ),
        ],
        Some("Apply these public service roles on this device?"),
    );
    disable_edit_form(world, settings_form, can_edit && !managed);
}

fn server_form(world: &mut World, owner: Entity, parent: Entity, choice: &Value, can_edit: bool) {
    let entity = form(
        world,
        owner,
        parent,
        "Remember server roles",
        action(json!({"command":"save-server","choice":choice})),
        vec![
            Field(
                "/request/choice/endpoint",
                "Pinned server endpoint ID",
                Kind::Text,
                choice["endpoint"].clone(),
            ),
            Field(
                "/request/choice/label",
                "Personal label",
                Kind::Text,
                choice["label"].clone(),
            ),
            Field(
                "/request/choice/operator",
                "Operator/group label (not verified independence)",
                Kind::Text,
                choice["operator"].clone(),
            ),
            Field(
                "/request/choice/publication",
                "Offer for future publication",
                boolean(),
                choice["publication"].clone(),
            ),
            Field(
                "/request/choice/query",
                "Offer for deliberate search and browsing",
                boolean(),
                choice["query"].clone(),
            ),
            Field(
                "/request/choice/mailbox",
                "Offer for future encrypted return mailboxes",
                boolean(),
                choice["mailbox"].clone(),
            ),
        ],
        None,
    );
    disable_edit_form(world, entity, can_edit);
}

fn gossip_view(world: &mut World, owner: Entity, parent: Entity, gossip: &Value, can_edit: bool) {
    label(
        world,
        parent,
        "Gossip means passing along public announcements. Your device and each contact must opt in separately. It keeps the author's signature and expiry; it does not share private threads or profile images.",
    );
    let enable = form(
        world,
        owner,
        parent,
        "Save this device's gossip participation",
        action(json!({"command":"configure-gossip","enabled":gossip["enabled"]})),
        vec![Field(
            "/request/enabled",
            "Allow contact gossip on this device",
            boolean(),
            gossip["enabled"].clone(),
        )],
        None,
    );
    disable_edit_form(world, enable, can_edit);
    label(
        world,
        parent,
        &format!("Pending public forwarding copies: {}", gossip["queued"]),
    );
    if let Some(error) = gossip["admission_error"].as_str() {
        label(world, parent, error);
    }
    for error in gossip["errors"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "{}: {}",
                error[0].as_str().unwrap_or_default(),
                error[1].as_str().unwrap_or_default()
            ),
        );
    }
    for contact in gossip["contacts"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "Gossip with {} · {}",
                contact["name"].as_str().unwrap_or("Contact"),
                contact["endpoint"]
                    .as_str()
                    .unwrap_or("Set and verify this contact's endpoint in contact settings first")
            ),
        );
        if contact["unreachable"] == true {
            label(
                world,
                parent,
                "This contact is marked unreachable; forwarding waits for reachability.",
            );
        }
        let choice = &contact["choice"];
        if contact["retired"] == true {
            let clear = form(
                world,
                owner,
                parent,
                "Clear retained gossip consent",
                action(
                    json!({"command":"set-gossip-contact","choice":{"organ":choice["organ"],"send":false,"receive":false}}),
                ),
                vec![],
                Some(
                    "Clear this blocked or removed contact's retained consent? Sharing is already paused.",
                ),
            );
            disable_edit_form(world, clear, can_edit);
            continue;
        }
        let entity = form(
            world,
            owner,
            parent,
            "Save separate contact gossip consent",
            action(json!({"command":"set-gossip-contact","choice":choice})),
            vec![
                Field(
                    "/request/choice/send",
                    "Send permitted public announcements to this contact",
                    boolean(),
                    choice["send"].clone(),
                ),
                Field(
                    "/request/choice/receive",
                    "Receive permitted public announcements from this contact",
                    boolean(),
                    choice["receive"].clone(),
                ),
            ],
            Some(
                "Change public announcement sharing with this contact? Previously shared public text cannot be recalled. This grants no private data or general sync.",
            ),
        );
        disable_edit_form(world, entity, can_edit);
    }
}

fn servers_view(world: &mut World, owner: Entity, parent: Entity, servers: &Value, can_edit: bool) {
    label(
        world,
        parent,
        "Remembered servers belong to this device. Search, publication and mailbox use are separate choices. Two endpoint IDs do not prove two independent operators or disks.",
    );
    server_form(
        world,
        owner,
        parent,
        &serde_json::to_value(nucleus::social::ServerChoice::default()).unwrap(),
        can_edit,
    );
    for choice in servers.as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "{} · {} · operator/group: {}",
                choice["label"].as_str().unwrap_or_default(),
                choice["endpoint"].as_str().unwrap_or_default(),
                choice["operator"].as_str().unwrap_or_default()
            ),
        );
        if let Some(endpoint) = choice["endpoint"].as_str() {
            qr::code(world, parent, endpoint);
        }
        server_form(world, owner, parent, choice, can_edit);
        form(
            world,
            owner,
            parent,
            "Inspect remembered server",
            action(json!({"command":"inspect-service","endpoint":choice["endpoint"]})),
            vec![],
            None,
        );
        let remove = form(
            world,
            owner,
            parent,
            "Remove remembered server",
            action(json!({"command":"remove-server","endpoint":choice["endpoint"]})),
            vec![],
            Some(
                "Remove this device's remembered choice? Existing posts and encrypted queues keep their reviewed destinations. Withdraw announcements or replace reply routes separately.",
            ),
        );
        disable_edit_form(world, remove, can_edit);
    }
    form(
        world,
        owner,
        parent,
        "Browse chosen servers",
        action(
            json!({"command":"search","query":nucleus::social::Search::default(),"services":chosen_servers(world,owner,"query")}),
        ),
        vec![],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Search chosen servers",
        action(
            json!({"command":"search","query":nucleus::social::Search { direction: Some(nucleus::social::Direction::Need), ..Default::default() },"services":chosen_servers(world,owner,"query")}),
        ),
        vec![
            Field(
                "/request/services",
                "Chosen directory endpoint IDs",
                Kind::TextList,
                chosen_servers(world, owner, "query"),
            ),
            Field(
                "/request/query/text",
                "Words to find",
                Kind::Text,
                json!(""),
            ),
            Field(
                "/request/query/direction",
                "Need or Contribution",
                choices(&["need", "contribution"]),
                json!("need"),
            ),
            Field(
                "/request/query/language",
                "Language (optional)",
                Kind::Text,
                json!(""),
            ),
            Field(
                "/request/query/area",
                "Declared city/region (optional)",
                Kind::Text,
                json!(""),
            ),
            Field(
                "/request/query/concept",
                "Public concept label (optional)",
                Kind::Text,
                json!(""),
            ),
            Field(
                "/request/query/unit",
                "Unit label (optional)",
                Kind::Text,
                json!(""),
            ),
        ],
        Some(
            "Send these words and filters to the displayed chosen directory endpoints? Those operators can read this query.",
        ),
    );
    if can_edit {
        compose(world, owner, parent, None, &json!({}));
    }
}
