use crate::app::{Intent, Mobile, button, input, label};
use crate::navigation::Page;
use bevy::prelude::*;
use engine::actions::Action;
use lince_interface::records::display;
use serde_json::Value;

fn rows(world: &World, topic: &str) -> Vec<Value> {
    world
        .resource::<Mobile>()
        .rows
        .get(topic)
        .cloned()
        .unwrap_or_default()
}

pub fn login(world: &mut World, parent: Entity) {
    label(world, parent, "Person sign-in", 22.0);
    label(
        world,
        parent,
        "Use a Person account authorized in this Organ. Passwords are not saved. After signing in, reopening this profile requires sign-in again.",
        16.0,
    );
    input(world, parent, "login", "username", "Username", "", false);
    input(world, parent, "login", "password", "Password", "", false);
    button(world, parent, "Sign in", Intent::Login);
}

pub fn render(world: &mut World, parent: Entity) {
    let status = rows(world, "roster")
        .into_iter()
        .next()
        .unwrap_or(Value::Null);
    let setup = world.resource::<Mobile>().setup_required;
    match world.resource::<Mobile>().identity.clone() {
        crate::session::Identity::Person(person) => {
            label(
                world,
                parent,
                &format!("Signed in as Person: {person}"),
                16.0,
            );
            button(world, parent, "Sign out and lock profile", Intent::Logout);
        }
        crate::session::Identity::Owner => {
            label(world, parent, "Using local device-owner access", 16.0);
            login(world, parent);
        }
        crate::session::Identity::Locked => {
            login(world, parent);
            return;
        }
    }
    let profiles = world.resource::<Mobile>().profiles.clone();
    let profile_name = profiles
        .selected
        .as_ref()
        .and_then(|id| profiles.names.get(id))
        .map(String::as_str)
        .unwrap_or("Original profile");
    label(world, parent, &format!("Profile: {profile_name}"), 16.0);
    label(
        world,
        parent,
        "My Organ · this device and my other devices",
        24.0,
    );
    if setup {
        label(
            world,
            parent,
            "Create your Organ here, or join the Organ already on your other device. Joining keeps each device's own key.",
            18.0,
        );
        button(
            world,
            parent,
            "Create my Organ",
            Intent::Ask(Action::RosterCreateOrgan),
        );
    } else if let Some(local) = rows(world, "organs")
        .into_iter()
        .find(|row| row["slug"] == "local-organ")
        && let Some(uid) = local["uid"].as_str()
    {
        button(
            world,
            parent,
            "Open my Organ",
            Intent::Open(Page::Record(uid.into())),
        );
    }
    label(
        world,
        parent,
        &format!("This Cell: {}", display(&status["this_cell"])),
        14.0,
    );
    if setup {
        label(world, parent, "Join my existing Organ", 22.0);
        if let Some(reason) = status["enrolment_error"].as_str() {
            label(world, parent, reason, 14.0);
        }
        button(world, parent, "Scan device enrolment QR", Intent::ScanQr);
        input(
            world,
            parent,
            "organ",
            "enrol",
            "Device enrolment code",
            "",
            true,
        );
        button(world, parent, "Join that Organ", Intent::JoinOrgan);
    }
    button(
        world,
        parent,
        "Find devices on this Wi-Fi",
        Intent::Discovery(true),
    );
    label(
        world,
        parent,
        "Visible for 15 minutes. Uses LAN only.",
        14.0,
    );
    if status["discovery"]["local"] == true
        && let Some(until) = status["discovery"]["local_until"].as_str()
    {
        label(
            world,
            parent,
            &format!("LAN visibility until {until}"),
            14.0,
        );
    }
    button(
        world,
        parent,
        "Stop LAN discovery",
        Intent::Discovery(false),
    );
    button(world, parent, "Sync now", Intent::Act(Action::SyncNow));
    if let Some(recovery) = status["sync"]["recovery"].as_str() {
        label(world, parent, recovery, 18.0);
    }
    label(
        world,
        parent,
        "Organ records sync between enrolled devices. Nearby devices and other Organ contacts do not gain this access.",
        14.0,
    );
    for cell in status["sync"]["cells"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &lince_interface::organ::delivery_label(cell),
            16.0,
        );
        if let Some(checkpoint) = cell["delivery"]["covered_seq"].as_i64() {
            label(
                world,
                parent,
                &format!("Confirmed local operation checkpoint: {checkpoint}"),
                12.0,
            );
        }
    }
    input(
        world,
        parent,
        "organ",
        "peer_port",
        "Peer UDP port (0 chooses automatically)",
        &status["peer_port"]
            .as_u64()
            .unwrap_or(u64::from(engine::wire::DEFAULT_PEER_PORT))
            .to_string(),
        false,
    );
    button(world, parent, "Save peer port", Intent::SavePeerPort);
    label(
        world,
        parent,
        &lince_interface::organ::peer_network_label(&status["peer_network"]),
        14.0,
    );
    label(
        world,
        parent,
        "Nearby devices · discovery does not grant access",
        18.0,
    );
    let nearby = rows(world, "nearby");
    if nearby.is_empty() {
        label(
            world,
            parent,
            "No nearby devices yet. Enable LAN discovery on both devices and keep them on the same network.",
            16.0,
        );
    }
    for peer in nearby {
        label(
            world,
            parent,
            &format!(
                "{} · {}",
                display(&peer["claimed_name"]),
                display(&peer["fingerprint"])
            ),
            16.0,
        );
        label(world, parent, &display(&peer["node_id"]), 12.0);
    }
    button(
        world,
        parent,
        "Show devices",
        Intent::Act(Action::RosterStatus),
    );
    if status["can_manage"] == true {
        button(
            world,
            parent,
            "Create enrolment code",
            Intent::Act(Action::RosterEnrolToken),
        );
    }
    for row in rows(world, "enrolment") {
        if let Some(code) = row["code"].as_str() {
            crate::qr::show(world, parent, code);
            input(
                world,
                parent,
                "enrolment",
                "code",
                "Use this code on the other device",
                code,
                true,
            );
            label(
                world,
                parent,
                &format!("Expires in {} minutes", display(&row["expires_in_minutes"])),
                16.0,
            );
        }
    }
    for row in rows(world, "pairing").into_iter().filter(|_| !setup) {
        if let Some(invite) = row["extension"]["invite"].as_str() {
            input(
                world,
                parent,
                "pairing",
                "code",
                "This Organ’s pairing code",
                invite,
                true,
            );
        }
    }
    for row in rows(world, "roster_records") {
        let roster = &row["extension"];
        if !roster["cells"].is_array() {
            continue;
        }
        label(
            world,
            parent,
            &format!("Organ: {}", display(&row["uid"])),
            14.0,
        );
        label(
            world,
            parent,
            &format!("Identity key: {}", display(&roster["root_key"])),
            14.0,
        );
        label(
            world,
            parent,
            &format!("Roster expires {}", display(&roster["not_after"])),
            16.0,
        );
        for device in roster["cells"].as_array().into_iter().flatten() {
            let Some(uid) = device["cell_uid"].as_str() else {
                continue;
            };
            label(world, parent, device["label"].as_str().unwrap_or(uid), 18.0);
            label(world, parent, uid, 12.0);
            let visibility = if status["this_cell"] == device["cell_uid"] {
                "This device"
            } else if rows(world, "nearby")
                .iter()
                .any(|peer| peer["node_id"] == device["node_id"])
            {
                "Visible on this LAN"
            } else {
                "Not currently visible on this LAN"
            };
            label(world, parent, visibility, 14.0);
            label(
                world,
                parent,
                &format!("Capabilities: {}", display(&device["capabilities"])),
                14.0,
            );
            if status["can_manage"] == true {
                input(
                    world,
                    parent,
                    "device",
                    uid,
                    "Device name",
                    device["label"].as_str().unwrap_or(""),
                    false,
                );
                button(
                    world,
                    parent,
                    "Save device name",
                    Intent::RenameCell(uid.into()),
                );
            }
            if status["can_manage"] == true && status["this_cell"] != device["cell_uid"] {
                button(
                    world,
                    parent,
                    "Revoke device",
                    Intent::Ask(Action::RosterRevokeCell {
                        cell_uid: uid.into(),
                    }),
                );
            }
        }
    }
    profile_controls(world, parent);
    if setup {
        return;
    }
    label(world, parent, "Other Organ contacts", 22.0);
    input(world, parent, "organ", "invite", "Pairing code", "", true);
    input(world, parent, "organ", "name", "Contact name", "", false);
    button(world, parent, "Add known Organ", Intent::Pair);
    for row in rows(world, "organs") {
        if row["slug"] == "local-organ" {
            continue;
        }
        let Some(uid) = row["uid"].as_str() else {
            continue;
        };
        label(world, parent, row["head"].as_str().unwrap_or(uid), 22.0);
        button(
            world,
            parent,
            "Open Organ",
            Intent::Open(Page::Record(uid.into())),
        );
        for (title, trust) in [("Trust", "known"), ("Block", "blocked")] {
            button(
                world,
                parent,
                title,
                Intent::Ask(Action::SetContactTrust {
                    target: uid.into(),
                    trust: trust.into(),
                }),
            );
        }
        button(
            world,
            parent,
            "Forget contact",
            Intent::Ask(Action::ForgetOrganContact { target: uid.into() }),
        );
    }
}

pub fn profile_controls(world: &mut World, parent: Entity) {
    let profiles = world.resource::<Mobile>().profiles.clone();
    label(world, parent, "Separate device profiles", 22.0);
    label(
        world,
        parent,
        "To join another Organ, create a fresh profile. The current profile, keys, records and drafts stay on this device. Only the open profile connects and syncs.",
        16.0,
    );
    input(
        world,
        parent,
        "profile",
        "name",
        "New profile name",
        "",
        false,
    );
    button(
        world,
        parent,
        "Create and open fresh profile",
        Intent::FreshProfile,
    );
    if profiles.selected.is_some() {
        button(
            world,
            parent,
            "Open original profile",
            Intent::SwitchProfile(None),
        );
    }
    for (id, name) in &profiles.names {
        if profiles.selected.as_ref() != Some(id) {
            button(
                world,
                parent,
                &format!("Open {name}"),
                Intent::SwitchProfile(Some(id.clone())),
            );
        }
    }
}
