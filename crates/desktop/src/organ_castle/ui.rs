use super::*;
use forms::{boolean, choices};

#[derive(Component)]
struct PairingPanel {
    owner: Entity,
    uid: String,
}

#[derive(Component)]
struct NearbyRow(String, bool);

#[derive(Component)]
struct NearbySummary(Entity);

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}
fn field<'a>(path: &'a str, caption: &'a str, value: &str) -> Field<'a> {
    Field(path, caption, Kind::Text, json!(value))
}
fn request(world: &mut World, owner: Entity, parent: Entity, caption: &str, payload: Value) {
    form(world, owner, parent, caption, payload, vec![], None);
}

pub(super) fn registration(world: &mut World, owner: Entity, parent: Entity) {
    label(
        world,
        parent,
        "Add an Organ using its whole lince1| pairing code. Only add codes received from someone you trust. Scanning fills the field; it does not add a contact.",
    );
    let registration = form(
        world,
        owner,
        parent,
        "Add known contact",
        json!({"action":"add-known-organ","invite":"","name":""}),
        vec![
            field("/invite", "Pairing code", ""),
            field("/name", "Local contact name", ""),
        ],
        None,
    );
    qr::scanner(
        world,
        owner,
        parent,
        forms::input_text(world, registration, "/invite").unwrap(),
    );
}

pub(super) fn organ_list(world: &mut World, owner: Entity) {
    let view = world.get::<OrganCastle>(owner).unwrap();
    let (parent, selected, rows) = (
        view.list,
        view.selected.clone(),
        view.rows.get("organs").cloned().unwrap_or_default(),
    );
    panel::clear(world, parent);
    for row in &rows {
        let uid = text(row, "uid");
        let contact = &row["contact"];
        let caption = if row["slug"] == "local-organ" {
            format!("{} — this Cell", text(row, "head"))
        } else {
            format!(
                "{} · {} · {}{}",
                text(row, "head"),
                text(contact, "trust"),
                fingerprint(&text(contact, "node_id")),
                if contact["pending_introduction"] == true {
                    " · awaiting introduction"
                } else {
                    ""
                }
            )
        };
        panel::button(world, parent, owner, &caption, Command::Select(uid));
    }
    if rows.is_empty() {
        label(world, parent, "No Organs loaded.");
    }
    if selected
        .as_ref()
        .is_none_or(|uid| !rows.iter().any(|row| row["uid"] == *uid))
    {
        world.get_mut::<OrganCastle>(owner).unwrap().selected =
            rows.first().map(|row| text(row, "uid"));
        detail(world, owner);
    }
}

fn fingerprint(key: &str) -> String {
    key.chars().take(8).collect::<String>().to_uppercase()
}

pub(super) fn detail(world: &mut World, owner: Entity) {
    let view = world.get::<OrganCastle>(owner).unwrap();
    let parent = view.detail;
    let selected = view.selected.clone();
    let row = view
        .rows
        .get("organs")
        .and_then(|rows| rows.iter().find(|row| Some(text(row, "uid")) == selected))
        .cloned();
    panel::clear(world, parent);
    let Some(row) = row else {
        label(world, parent, "Select an Organ.");
        return;
    };
    let uid = text(&row, "uid");
    label(world, parent, &format!("{} · {uid}", text(&row, "head")));
    let pairing = panel::column(world, parent);
    world.entity_mut(pairing).insert(PairingPanel {
        owner,
        uid: uid.clone(),
    });
    pairings(world, owner);
    if row["contact"].is_object() {
        contact(world, owner, parent, &row);
    } else if row["slug"] == "local-organ" {
        label(
            world,
            parent,
            "This Cell's identity, discovery, temporary LAN presence, relay addresses and disk settings are under Local settings. Device membership is under My devices.",
        );
    } else {
        form(
            world,
            owner,
            parent,
            "Save Organ",
            json!({"action":"edit-record-text","target":uid,"head":"","body":""}),
            vec![
                field("/head", "Name", &text(&row, "head")),
                field("/body", "Address", &text(&row, "body")),
            ],
            None,
        );
        form(
            world,
            owner,
            parent,
            "Delete Organ",
            json!({"action":"delete-record","target":uid}),
            vec![],
            Some("Delete this Organ Record?"),
        );
    }
    file_sync(world, owner, parent, &row);
}

pub(super) fn pairings(world: &mut World, owner: Entity) {
    let panels: Vec<_> = world
        .query::<(Entity, &PairingPanel)>()
        .iter(world)
        .filter(|(_, panel)| panel.owner == owner)
        .map(|(entity, panel)| (entity, panel.uid.clone()))
        .collect();
    for (parent, uid) in panels {
        let view = world.get::<OrganCastle>(owner).unwrap();
        let invite = view
            .rows
            .get("pairing")
            .and_then(|rows| rows.iter().find(|row| row["uid"] == uid))
            .map(|row| text(&row["extension"], "invite"))
            .unwrap_or_default();
        let root_key = view
            .rows
            .get("roster")
            .and_then(|rows| rows.iter().find(|row| row["uid"] == uid))
            .map(|row| text(&row["extension"], "root_key"))
            .unwrap_or_default();
        panel::clear(world, parent);
        label(world, parent, "This Organ's pairing code");
        if invite.is_empty() {
            label(world, parent, "Pairing code not available yet.");
        } else {
            qr::code(world, parent, &invite);
        }
        if !root_key.is_empty() {
            label(
                world,
                parent,
                &format!("Identity key for comparison: {root_key}"),
            );
        }
    }
}

fn contact(world: &mut World, owner: Entity, parent: Entity, row: &Value) {
    let uid = text(row, "uid");
    let c = &row["contact"];
    label(
        world,
        parent,
        &format!(
            "Verify by fingerprint: {}\nNodeId: {}",
            fingerprint(&text(c, "node_id")),
            text(c, "node_id")
        ),
    );
    form(
        world,
        owner,
        parent,
        "Rename locally",
        json!({"action":"rename-organ-contact","target":uid,"name":""}),
        vec![field("/name", "Local name", &text(row, "head"))],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Save trust",
        json!({"action":"set-contact-trust","target":uid,"trust":""}),
        vec![Field(
            "/trust",
            "Trust",
            choices(&["unknown", "known", "blocked"]),
            c["trust"].clone(),
        )],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Save proximity",
        json!({"action":"set-contact-proximity","target":uid,"proximity":0}),
        vec![Field(
            "/proximity",
            "Proximity",
            Kind::Number,
            c["proximity"].clone(),
        )],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Save feed direction",
        json!({"action":"set-sync-policy","target":uid,"sync_out":false,"sync_in":false}),
        vec![
            Field(
                "/sync_out",
                "Send changes",
                boolean(),
                c["sync_out"].clone(),
            ),
            Field(
                "/sync_in",
                "Receive changes",
                boolean(),
                c["sync_in"].clone(),
            ),
        ],
        None,
    );
    for (key, broken, action, caption, explanation) in [
        (
            "scope_fields",
            "scope_unreadable",
            "set-contact-scope",
            "Save outgoing fields",
            "Minimum only sends identifying fields. Widening sends past changes too.",
        ),
        (
            "accept_fields",
            "accept_unreadable",
            "set-contact-accept-scope",
            "Save incoming fields",
            "Minimum only accepts deletions. Incoming and outgoing limits are independent.",
        ),
    ] {
        label(world, parent, explanation);
        if c[broken] == true {
            label(
                world,
                parent,
                "This saved limit could not be read and is being ignored: nothing is narrowed right now. Fix it before relying on it.",
            );
        }
        form(
            world,
            owner,
            parent,
            caption,
            json!({"action":action,"target":uid,"fields":null}),
            vec![Field("/fields", caption, Kind::Scope, c[key].clone())],
            Some(
                "Apply this sharing limit? Widening outgoing sharing sends earlier changes, including changes made before now.",
            ),
        );
    }
    label(
        world,
        parent,
        "Hidden Records · hiding does not remove anything they already received. Unhiding sends the whole Record, including what changed while hidden.",
    );
    form(
        world,
        owner,
        parent,
        "Hide Record",
        json!({"action":"hide-record-from-contact","target":uid,"record":"","hidden":true}),
        vec![field("/record", "Record slug or UID", "")],
        None,
    );
    match c["hidden_records"].as_array() {
        None => {
            label(world, parent, "Not loaded.");
        }
        Some(rows) if rows.is_empty() => {
            label(world, parent, "Nothing hidden.");
        }
        Some(rows) => {
            for hidden in rows {
                form(
                    world,
                    owner,
                    parent,
                    &format!(
                        "Unhide {}",
                        hidden["head"]
                            .as_str()
                            .or(hidden["slug"].as_str())
                            .unwrap_or("deleted Record")
                    ),
                    json!({"action":"hide-record-from-contact","target":uid,"record":hidden["uid"],"hidden":false}),
                    vec![],
                    Some("Send this Record and its earlier changes to this contact?"),
                );
            }
        }
    }
    label(
        world,
        parent,
        "Refused changes · changes skipped by your incoming policy are not listed here.",
    );
    match c["quarantined"].as_array() {
        None => {
            label(world, parent, "Not loaded.");
        }
        Some(rows) if rows.is_empty() => {
            label(world, parent, "Nothing refused.");
        }
        Some(rows) => {
            for refused in rows {
                let item = label(
                    world,
                    parent,
                    &format!("{} · {}", text(refused, "at"), text(refused, "reason")),
                );
                world
                    .entity_mut(item)
                    .insert(crate::icons::Tooltip(text(refused, "payload")));
            }
        }
    }
    let conversations = row["conversations"].as_array().cloned().unwrap_or_default();
    for conversation in &conversations {
        panel::button(
            world,
            parent,
            owner,
            "Open conversation",
            Command::Open(text(conversation, "uid"), false),
        );
    }
    if conversations.is_empty() {
        form(
            world,
            owner,
            parent,
            "Start conversation",
            json!({"action":"start-conversation","contact":uid,"title":""}),
            vec![field("/title", "Conversation title", "Hello")],
            None,
        );
    }
    request(
        world,
        owner,
        parent,
        "Audit contact's roster",
        json!({"action":"audit-contact","contact":uid}),
    );
    form(
        world,
        owner,
        parent,
        "Grant login",
        json!({"action":"grant-organ-login","organ":uid,"person_name":""}),
        vec![field("/person_name", "Person name", "")],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Revoke login",
        json!({"action":"revoke-organ-login","organ":uid}),
        vec![],
        Some("Remove this Organ's login access?"),
    );
    panel::button(
        world,
        parent,
        owner,
        "Open live Organ",
        Command::Open(uid.clone(), true),
    );
    form(
        world,
        owner,
        parent,
        "Forget contact",
        json!({"action":"forget-organ-contact","target":uid}),
        vec![],
        Some(
            "Forget this contact locally? Their original Record will not be deleted on their Organ.",
        ),
    );
}

fn file_sync(world: &mut World, owner: Entity, parent: Entity, row: &Value) {
    let uid = text(row, "uid");
    let mut config = row["extension"].as_object().cloned().unwrap_or_default();
    for (key, fallback) in [
        ("enabled", json!(false)),
        ("path", json!("")),
        ("filter", json!("")),
        ("formats", json!(["lingua"])),
    ] {
        config.entry(key).or_insert(fallback);
    }
    let formats = config.get("formats").cloned().unwrap();
    let selected_formats =
        if row["extension"]["formats"].is_null() && row["extension"]["format"].is_string() {
            json!([row["extension"]["format"]])
        } else {
            formats
        };
    let filter = config["filter"].clone();
    let path = config["path"].clone();
    let enabled = config["enabled"].clone();
    label(
        world,
        parent,
        "File Sync · mirror Records from this Organ to a local directory. An empty filter selects all. A custom filter uses Protein conditions such as {\"kind_eq\":\"plain\"}.",
    );
    if filter
        .as_str()
        .is_some_and(|s| !s.is_empty() && serde_json::from_str::<protein::Predicate>(s).is_err())
    {
        label(
            world,
            parent,
            "The saved filter could not be read and is being ignored, so everything from this Organ is syncing. Fix or clear it.",
        );
    }
    if config.contains_key("protein") {
        label(
            world,
            parent,
            "A saved Protein currently selects these Records. Saving this filter replaces that selection.",
        );
    }
    config.remove("protein");
    config.remove("format");
    form(
        world,
        owner,
        parent,
        "Save File Sync",
        json!({"action":"set-extension","target":uid,"namespace":"lince.file_sync","fds":config}),
        vec![
            Field("/fds/enabled", "File Sync", boolean(), enabled),
            Field("/fds/path", "Absolute directory", Kind::Text, path),
            Field(
                "/fds/formats",
                "File formats",
                Kind::Choice(vec![
                    (".lingua".into(), json!(["lingua"])),
                    ("Markdown".into(), json!(["markdown"])),
                    ("Both".into(), json!(["lingua", "markdown"])),
                ]),
                selected_formats,
            ),
            Field(
                "/fds/filter",
                "Protein filter (blank means all)",
                Kind::Filter,
                filter,
            ),
        ],
        None,
    );
    request(
        world,
        owner,
        parent,
        "Check File Sync conflicts",
        json!({"action":"file-sync-status","organ":uid}),
    );
}

pub(super) fn nearby(world: &mut World, owner: Entity) {
    let view = world.get::<OrganCastle>(owner).unwrap();
    let (parent, rows) = (
        view.nearby,
        view.rows.get("nearby").cloned().unwrap_or_default(),
    );
    let summary = if let Some(summary) = world.get::<NearbySummary>(parent) {
        summary.0
    } else {
        let summary = label(world, parent, "");
        world.entity_mut(parent).insert(NearbySummary(summary));
        summary
    };
    panel::status(
        world,
        summary,
        format!(
            "{} nearby Cells. Enable LAN discovery under Local settings to find others. Accept unknown conversations allows strangers to request a chat; adding a known contact is separate.",
            rows.len()
        ),
    );
    let existing: Vec<_> = world
        .get::<Children>(parent)
        .into_iter()
        .flatten()
        .filter_map(|entity| {
            world
                .get::<NearbyRow>(*entity)
                .map(|row| (*entity, row.0.clone(), row.1))
        })
        .collect();
    for (entity, node_id, known) in &existing {
        if !rows
            .iter()
            .any(|row| row["node_id"] == *node_id && (row["known"] == true) == *known)
        {
            world.despawn(*entity);
        }
    }
    for row in rows {
        if existing.iter().any(|(_, node_id, known)| {
            row["node_id"] == *node_id && (row["known"] == true) == *known
        }) {
            continue;
        }
        let parent = panel::column(world, parent);
        world
            .entity_mut(parent)
            .insert(NearbyRow(text(&row, "node_id"), row["known"] == true));
        label(
            world,
            parent,
            &format!(
                "{} · {} · {}",
                row["name"]
                    .as_str()
                    .or(row["claimed_name"].as_str())
                    .unwrap_or("Unnamed claim"),
                text(&row, "fingerprint"),
                if row["known"] == true {
                    "known"
                } else {
                    "unverified name"
                }
            ),
        );
        let contact = world
            .get::<OrganCastle>(owner)
            .unwrap()
            .rows
            .get("organs")
            .and_then(|rows| {
                rows.iter()
                    .find(|organ| organ["contact"]["node_id"] == row["node_id"])
            })
            .cloned();
        if let Some(contact) = &contact {
            panel::button(
                world,
                parent,
                owner,
                "Open saved contact",
                NearbyContact(text(contact, "uid")),
            );
        }
        let conversation = contact
            .as_ref()
            .and_then(|row| row["conversations"].as_array())
            .and_then(|rows| rows.first())
            .map(|row| text(row, "uid"));
        if let Some(conversation) = conversation {
            panel::button(
                world,
                parent,
                owner,
                "Open conversation",
                Command::Open(conversation, false),
            );
        } else {
            form(
                world,
                owner,
                parent,
                "Chat",
                json!({"action":"nearby-chat","node_id":row["node_id"],"name":""}),
                vec![field("/name", "Conversation title", "Hello")],
                None,
            );
        }
        if row["known"] != true {
            form(
                world,
                owner,
                parent,
                "Add known",
                json!({"action":"nearby-pair","node_id":row["node_id"],"name":""}),
                vec![field(
                    "/name",
                    "Local contact name",
                    row["name"]
                        .as_str()
                        .or(row["claimed_name"].as_str())
                        .unwrap_or_default(),
                )],
                Some("Trust this Cell after comparing its fingerprint with the person you know?"),
            );
        }
    }
}

#[derive(Clone)]
struct NearbyContact(String);
impl Action for NearbyContact {
    fn apply(&self, world: &mut World, owner: Entity) {
        Command::Select(self.0.clone()).apply(world, owner);
        Command::Page(1).apply(world, owner);
    }
}

pub(super) fn devices(world: &mut World, owner: Entity) {
    let view = world.get::<OrganCastle>(owner).unwrap();
    let parent = view.devices;
    let roster = view
        .rows
        .get("roster")
        .and_then(|rows| rows.iter().find(|row| row["slug"] == "local-organ"))
        .map(|row| row["extension"].clone())
        .unwrap_or(Value::Null);
    panel::clear(world, parent);
    if roster.is_null() {
        label(
            world,
            parent,
            "Create an Organ on this device, or use a device enrolment code to join the Organ on another device.",
        );
        form(
            world,
            owner,
            parent,
            "Create my Organ",
            json!({"action":"roster-create-organ"}),
            vec![],
            Some(
                "Create this Organ's identity? Joining another Organ afterward requires a fresh device profile.",
            ),
        );
    }
    label(
        world,
        parent,
        &format!(
            "Personal roster · version {} · expires {}",
            roster["version"],
            text(&roster, "not_after")
        ),
    );
    if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(&text(&roster, "not_after")) {
        if expiry <= chrono::Utc::now() {
            label(
                world,
                parent,
                "This roster has expired. It must be renewed before it can authorize devices.",
            );
        }
    }
    let Some(cells) = roster["cells"].as_array() else {
        label(world, parent, "No device roster loaded yet.");
        return;
    };
    for cell in cells {
        form(
            world,
            owner,
            parent,
            "Save device name",
            json!({"action":"roster-rename-cell","cell_uid":cell["cell_uid"],"label":cell["label"]}),
            vec![field("/label", "Device name", &text(cell, "label"))],
            None,
        );
        label(
            world,
            parent,
            &format!(
                "{} · {}{}",
                text(cell, "label"),
                text(cell, "cell_uid"),
                if cell["front_door"] == true {
                    " · front door"
                } else {
                    ""
                }
            ),
        );
        label(
            world,
            parent,
            &format!(
                "Fingerprint: {} · permissions: {}",
                fingerprint(&text(cell, "node_id")),
                cell["capabilities"]
            ),
        );
        let karma_enabled = cell["capabilities"].as_array().is_some_and(|capabilities| capabilities.iter().any(|capability| capability == "karma"));
        label(world, parent, if karma_enabled { "Karma permission: enabled" } else { "Karma permission: disabled; definitions still sync for editing" });
        if karma_enabled {
            form(world, owner, parent, "Disable Karma on this device", json!({"action":"roster-set-karma-execution","cell_uid":cell["cell_uid"],"enabled":false,"additional":false,"expected_roster_version":roster["version"]}), vec![], None);
        }
        form(world, owner, parent, "Use this device as the only Karma executor", json!({"action":"roster-set-karma-execution","cell_uid":cell["cell_uid"],"enabled":true,"additional":false,"expected_roster_version":roster["version"]}), vec![], None);
        if cells.len() > 1 && !karma_enabled {
            form(world, owner, parent, "Allow this device to run Karma alongside the others", json!({"action":"roster-set-karma-execution","cell_uid":cell["cell_uid"],"enabled":true,"additional":true,"expected_roster_version":roster["version"]}), vec![], Some("Allow more than one Cell to run the synced Karma definitions? The same Rule can then fire on each enabled Cell."));
        }
        if cell["sealing_key"].is_null() {
            label(world, parent, "No mail key yet.");
        } else {
            label(
                world,
                parent,
                &format!(
                    "Mail key {} · expires {}",
                    text(&cell["sealing_key"], "key_id"),
                    text(&cell["sealing_key"], "not_after")
                ),
            );
        }
        if cells.len() > 1 {
            form(
                world,
                owner,
                parent,
                "Remove device",
                json!({"action":"roster-revoke-cell","cell_uid":cell["cell_uid"]}),
                vec![],
                Some(
                    "Remove this device from your identity? Other devices stop accepting it as each syncs. An offline device keeps accepting it until it comes back.",
                ),
            );
        }
    }
}

pub(super) fn device_controls(world: &mut World, owner: Entity, parent: Entity) {
    label(world, parent, "Local Karma running state is separate from the signed execution permission.");
    for (caption, running) in [("Stop Karma on this Cell", false), ("Run permitted Karma on this Cell", true)] {
        form(world, owner, parent, caption, json!({"action":"set-cell-config","namespace":"lince.karma-runtime","fds":{"running":running}}), vec![], None);
    }

    request(
        world,
        owner,
        parent,
        "Refresh device status and waiting visitors",
        json!({"action":"roster-status"}),
    );
    label(
        world,
        parent,
        "Add a device · the one-use code expires after ten minutes. Keep it private.",
    );
    request(
        world,
        owner,
        parent,
        "Issue device enrolment code",
        json!({"action":"roster-enrol-token"}),
    );
    let join = form(
        world,
        owner,
        parent,
        "Join my existing Organ",
        json!({"action":"roster-join-organ","code":""}),
        vec![field("/code", "Enrolment code (lincecell1|)", "")],
        Some(
            "Enrol this fresh Cell in that Organ? A Cell with its own published identity or Records cannot be replaced here. Compare the Organ identity with the device showing the code.",
        ),
    );
    qr::scanner(
        world,
        owner,
        parent,
        forms::input_text(world, join, "/code").unwrap(),
    );
    form(
        world,
        owner,
        parent,
        "Export root key",
        json!({"action":"root-key-export","destination":""}),
        vec![field("/destination", "Offline backup destination", "")],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Detach root key",
        json!({"action":"root-key-detach","copy_at":""}),
        vec![field("/copy_at", "Verified backup path", "")],
        Some(
            "Remove the root key from this device? Adding or revoking devices will require bringing it back. The saved copy is checked before removal.",
        ),
    );
}

pub(super) fn mail(world: &mut World, owner: Entity, parent: Entity) {
    label(
        world,
        parent,
        "Choose one or two servers for new outgoing envelopes. Two copies improve availability. Server acceptance does not mean the recipient read the message.",
    );
    form(
        world,
        owner,
        parent,
        "Save mailbox copy policy",
        json!({"action":"mailbox-set-copies","copies":2}),
        vec![Field(
            "/copies",
            "Requested server copies (1 or 2)",
            Kind::Number,
            json!(2),
        )],
        None,
    );
    label(
        world,
        parent,
        "Mail · carry sealed mail for others, choose pickup points, and deliver changes while a contact is offline.",
    );
    for (caption, action) in [
        ("Refresh carried mail", "mailbox-status"),
        ("Check saved message recovery", "mailbox-saved-status"),
        ("Refresh requests", "mailbox-requests"),
        ("Refresh pickup points", "mailbox-pickup-points"),
        ("Refresh outgoing mail", "mailbox-outbound"),
        ("Collect mail now", "mailbox-collect-now"),
        ("Issue carrying invite", "mailbox-issue-invite"),
    ] {
        request(world, owner, parent, caption, json!({"action":action}));
    }
    for (caption, action) in [
        ("Offer to carry mail", "mailbox-carry-for"),
        ("Ask them to carry mail", "mailbox-ask-carry"),
        ("Add pickup point", "mailbox-add-pickup"),
        ("Send mail now", "mailbox-mail-now"),
        ("Stop carrying", "mailbox-stop-carrying"),
        ("Remove pickup point", "mailbox-remove-pickup"),
    ] {
        form(
            world,
            owner,
            parent,
            caption,
            json!({"action":action,"organ_uid":""}),
            vec![field("/organ_uid", "Organ UID", "")],
            if action == "mailbox-stop-carrying" {
                Some("Stop carrying mail for this Organ? Stored envelopes will be removed.")
            } else {
                None
            },
        );
    }
    form(
        world,
        owner,
        parent,
        "Answer carrying request",
        json!({"action":"mailbox-answer-request","organ_uid":"","accept":false}),
        vec![
            field("/organ_uid", "Requesting Organ UID", ""),
            Field("/accept", "Accept request", boolean(), json!(false)),
        ],
        None,
    );
    form(
        world,
        owner,
        parent,
        "Use carrying invite",
        json!({"action":"mailbox-use-invite","code":""}),
        vec![field("/code", "Mailbox invitation (lincemail1|)", "")],
        None,
    );
}

pub(super) fn result(world: &mut World, owner: Entity, parent: Entity, value: &Value) {
    if let Some(rows) = value["saved_outgoing"].as_array() {
        if rows.is_empty() {
            label(world, parent, "No saved outgoing envelopes.");
        }
        for row in rows {
            label(
                world,
                parent,
                &format!(
                    "Outgoing message · {} of {} servers accepted · {} · {}",
                    row["copies"],
                    row["requested_copies"],
                    row["state"].as_str().unwrap_or_default(),
                    row["error"]
                        .as_str()
                        .unwrap_or("Waiting for recipient collection")
                ),
            );
        }
        let mut remaining = value.clone();
        remaining.as_object_mut().unwrap().remove("saved_outgoing");
        result(world, owner, parent, &remaining);
        return;
    }
    if let Some(rows) = value["saved_mail"].as_array() {
        if rows.is_empty() {
            label(world, parent, "All saved mail has been processed.");
        }
        for row in rows {
            label(
                world,
                parent,
                &format!(
                    "Saved message · {} · {}",
                    row["state"].as_str().unwrap_or_default(),
                    row["error"].as_str().unwrap_or("Waiting for recovery")
                ),
            );
            if row["state"] != "expired" {
                request(
                    world,
                    owner,
                    parent,
                    "Retry recovery",
                    json!({"action":"mailbox-retry-saved","uid":row["uid"]}),
                );
            }
        }
        return;
    }
    let sync = value.get("sync").unwrap_or(value);
    if let Some(recovery) = sync["recovery"].as_str() {
        label(world, parent, recovery);
    }
    for cell in sync["cells"].as_array().into_iter().flatten() {
        if cell.get("delivery").is_some() {
            label(world, parent, &lince_interface::organ::delivery_label(cell));
        }
    }
    if value["has_roster"] == true
        && value["capabilities"]
            .as_array()
            .is_some_and(|caps| !caps.iter().any(|cap| cap == "write"))
    {
        label(
            world,
            parent,
            "This device is a relay. It cannot write on behalf of your Organ.",
        );
    }
    if value["checked"] == false {
        label(world, parent, "This folder has not been checked yet.");
    }
    if value["reached"] == false {
        label(
            world,
            parent,
            "The contact could not be reached. Its roster has not been checked.",
        );
    }
    if let Some(code) = value["code"].as_str() {
        qr::code(world, parent, code);
    }
    if let Some(conversation) = value["conversation"].as_str().or(value
        .pointer("/created/conversation")
        .and_then(Value::as_str))
    {
        panel::button(
            world,
            parent,
            owner,
            "Open conversation",
            Command::Open(conversation.into(), false),
        );
    }
    describe(world, parent, value, 0);
}

fn describe(world: &mut World, parent: Entity, value: &Value, depth: usize) {
    if depth > 6 {
        return;
    }
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key == "qr_svg" || key == "code" || value.is_null() {
                    continue;
                }
                if value.is_object() || value.is_array() {
                    label(world, parent, &key.replace('_', " "));
                    describe(world, parent, value, depth + 1);
                } else {
                    label(
                        world,
                        parent,
                        &format!(
                            "{}: {}",
                            key.replace('_', " "),
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string())
                        ),
                    );
                }
            }
        }
        Value::Array(rows) => {
            if rows.is_empty() {
                label(world, parent, "None.");
            }
            for value in rows.iter().take(100) {
                describe(world, parent, value, depth + 1);
            }
            if rows.len() > 100 {
                label(world, parent, "Showing the first 100 entries.");
            }
        }
        _ => {
            label(
                world,
                parent,
                &value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            );
        }
    }
}
