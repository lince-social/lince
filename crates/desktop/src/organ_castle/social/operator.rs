use super::*;

fn timestamp(value: &Value) -> String {
    value
        .as_i64()
        .filter(|at| *at > 0)
        .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
        .map(|at| at.to_rfc3339())
        .unwrap_or_else(|| "Unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_renders_dates_unknown_ages_and_separate_delivery_groups_without_actions() {
        let mut app = crate::sand_panel::tests::app();
        app.add_plugins(OrganCastlePlugin);
        let parent = app.world_mut().spawn_empty().id();
        let health = json!({"observed_at":1,"settings":{"directory":false,"townsquare":false,"mailbox":true,"cache_entries":10000,"storage_bytes":1024},"workers":[{"name":"private-send","state":{"running":true,"last_completed_at":null,"last_succeeded":null,"restarts":0,"failed_passes":0}}],"queues":{"publication":0,"messages":1,"gossip":0,"queries":0,"publication_oldest_age_seconds":null,"message_oldest_age_seconds":null},"delivery":{"preparation":{"messages":1,"due":0,"delayed":1,"elapsed":0,"errors":1,"oldest_age_seconds":null,"next_retry_seconds":30,"unknown_age":1,"future_age":0},"copies":{"copies":2,"message_records":1,"pending":1,"stored":1,"ready":0,"held":0,"expired":0,"cancelled":0,"elapsed":0,"payload_bytes":10,"oldest_age_seconds":null,"unknown_age":2,"future_age":0},"destinations":{"attempts":2,"due":0,"delayed":2,"failed":0,"cancelled":0,"errors":1,"next_retry_seconds":30,"recipient_durable":0,"recipient_refused":0,"unknown_receipts":0},"pickup":{},"receive_failures":{},"host_admissions":{},"key_retention":{"active":0,"dormant":1,"retired":0,"review":0,"next_retirement_at":2,"signed_decisions":0,"deleted_block_overrides":0},"blocks":{"active_blocks":0,"unblocks":0,"unknown_metadata":0}},"storage":{"cache_entries":0,"public_payload_bytes":0,"mail_entries":0,"mail_payload_bytes":0,"authority_floor_entries":0,"main_database_allocated_bytes":4096}});
        view(app.world_mut(), parent, &health);
        let labels: Vec<String> = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect();
        assert!(
            labels
                .iter()
                .any(|label| label.contains("1970-01-01T00:00:01+00:00"))
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("last completed Unknown"))
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("Message preparation: 1 logical Messages"))
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("Encrypted copies: 2 for 1 retained Message Records"))
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("Oldest known age Unknown seconds"))
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("Destination attempts: 2"))
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("1970-01-01T00:00:02+00:00"))
        );
        assert!(
            app.world_mut()
                .query::<&forms::Form>()
                .iter(app.world())
                .next()
                .is_none()
        );
        assert!(app.world().resource::<Requests>().actions.is_empty());
    }
}

fn number(value: &Value) -> String {
    value
        .as_i64()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "Unknown".into())
}

pub(super) fn view(world: &mut World, parent: Entity, health: &Value) {
    label(world, parent, "Social service health");
    label(
        world,
        parent,
        if health["managed"] == true {
            "Hosting is managed by the deployment configuration."
        } else {
            "Hosting follows this device's native settings."
        },
    );
    label(
        world,
        parent,
        &format!(
            "Observed at {}. Roles: directory {} · browsing {} · mailbox {}",
            timestamp(&health["observed_at"]),
            health["settings"]["directory"],
            health["settings"]["townsquare"],
            health["settings"]["mailbox"]
        ),
    );
    for worker in health["workers"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "{}: running {} · last completed {} · last pass succeeded {} · restarts {} · failed passes {}",
                worker["name"].as_str().unwrap_or("Worker"),
                worker["state"]["running"],
                timestamp(&worker["state"]["last_completed_at"]),
                worker["state"]["last_succeeded"],
                worker["state"]["restarts"],
                worker["state"]["failed_passes"]
            ),
        );
    }
    label(
        world,
        parent,
        &format!(
            "Work: pending publications {} · retained message preparation {} · queued gossip {} · current contact queries {}",
            health["queues"]["publication"],
            health["queues"]["messages"],
            health["queues"]["gossip"],
            health["queues"]["queries"]
        ),
    );
    label(
        world,
        parent,
        &format!(
            "Oldest publication/message preparation age: {}/{} seconds. An unknown age does not prove an empty queue.",
            number(&health["queues"]["publication_oldest_age_seconds"]),
            number(&health["queues"]["message_oldest_age_seconds"])
        ),
    );
    let delivery = &health["delivery"];
    let preparation = &delivery["preparation"];
    label(
        world,
        parent,
        &format!(
            "Message preparation: {} logical Messages · due {} · delayed {} · elapsed {} · retained errors {}. Oldest known age {} seconds; next retry in {} seconds. Unknown dates {}; future dates {}.",
            preparation["messages"],
            preparation["due"],
            preparation["delayed"],
            preparation["elapsed"],
            preparation["errors"],
            number(&preparation["oldest_age_seconds"]),
            number(&preparation["next_retry_seconds"]),
            preparation["unknown_age"],
            preparation["future_age"]
        ),
    );
    let copies = &delivery["copies"];
    label(
        world,
        parent,
        &format!(
            "Encrypted copies: {} for {} retained Message Records · pending {} · stored {} · ready {} · held {} · expired {} · cancelled {}. Elapsed local deadlines {}; {} payload bytes. Oldest known age {} seconds; unknown dates {}; future dates {}.",
            copies["copies"],
            copies["message_records"],
            copies["pending"],
            copies["stored"],
            copies["ready"],
            copies["held"],
            copies["expired"],
            copies["cancelled"],
            copies["elapsed"],
            copies["payload_bytes"],
            number(&copies["oldest_age_seconds"]),
            copies["unknown_age"],
            copies["future_age"]
        ),
    );
    let destinations = &delivery["destinations"];
    label(
        world,
        parent,
        &format!(
            "Destination attempts: {} · due {} · delayed {} · failed {} · cancelled {} · retained errors {}. Next retry in {} seconds. Retained receipt stages: recipient-durable {} · recipient-refused {} · unknown {}.",
            destinations["attempts"],
            destinations["due"],
            destinations["delayed"],
            destinations["failed"],
            destinations["cancelled"],
            destinations["errors"],
            number(&destinations["next_retry_seconds"]),
            destinations["recipient_durable"],
            destinations["recipient_refused"],
            destinations["unknown_receipts"]
        ),
    );
    let pickup = &delivery["pickup"];
    let failures = &delivery["receive_failures"];
    label(
        world,
        parent,
        &format!(
            "Pickup jobs: {} · due {} · delayed {} · errors {} · next retry in {} seconds. Unreadable copies: deferred {} · Discard pending {} · elapsed {} · Discard due {} · delayed {}.",
            pickup["jobs"],
            pickup["due"],
            pickup["delayed"],
            pickup["errors"],
            number(&pickup["next_retry_seconds"]),
            failures["deferred"],
            failures["discard_pending"],
            failures["elapsed"],
            failures["due"],
            failures["delayed"]
        ),
    );
    let admissions = &delivery["host_admissions"];
    label(
        world,
        parent,
        &format!(
            "Host admission metadata: {} retained · current {} · accepted {} · provisional {} · elapsed {} · blocked evidence {} · closed evidence {}.",
            admissions["retained"],
            admissions["current"],
            admissions["accepted"],
            admissions["provisional"],
            admissions["elapsed"],
            admissions["blocked_evidence"],
            admissions["closed_evidence"]
        ),
    );
    let retention = &delivery["key_retention"];
    let blocks = &delivery["blocks"];
    label(
        world,
        parent,
        &format!(
            "Key retention: active {} · dormant {} · retired {} · review {} · next retirement {}. Permanent signed decisions {}; deleted-context block decisions {}. Effective active blocks {}; unblocks {}; unknown block metadata {}.",
            retention["active"],
            retention["dormant"],
            retention["retired"],
            retention["review"],
            timestamp(&retention["next_retirement_at"]),
            retention["signed_decisions"],
            retention["deleted_block_overrides"],
            blocks["active_blocks"],
            blocks["unblocks"],
            blocks["unknown_metadata"]
        ),
    );
    label(
        world,
        parent,
        "These are retained local metadata counts. Multiple host copies remain one logical Message. Elapsed local deadlines do not prove deletion of remote copies; receipt stages here are not an independent delivery audit.",
    );
    label(
        world,
        parent,
        &format!(
            "Public cache: {} / {} entries · {} payload bytes. Mailboxes: {} envelopes / {} payload bytes. Retained authority floors: {}.",
            health["storage"]["cache_entries"],
            health["settings"]["cache_entries"],
            health["storage"]["public_payload_bytes"],
            health["storage"]["mail_entries"],
            health["storage"]["mail_payload_bytes"],
            health["storage"]["authority_floor_entries"]
        ),
    );
    label(
        world,
        parent,
        &format!(
            "Main database allocation: {} bytes. Payload budget: {} bytes; WAL, blobs and backups require extra disk headroom.",
            health["storage"]["main_database_allocated_bytes"], health["settings"]["storage_bytes"]
        ),
    );
    label(
        world,
        parent,
        &format!(
            "Mailbox admission reserve accounting: {} / {} entries · {} / {} budget bytes. Content admission tier: {} entries / {} budget bytes. Estimated overhead and control reserves are included here; physical disk use still requires separate measurement.",
            health["storage"]["mailbox_reserved_entries"],
            health["storage"]["mailbox_entry_limit"],
            health["storage"]["mailbox_reserved_budget_bytes"],
            health["storage"]["mailbox_byte_limit"],
            health["storage"]["mailbox_data_entry_limit"],
            health["storage"]["mailbox_data_byte_limit"]
        ),
    );
}
