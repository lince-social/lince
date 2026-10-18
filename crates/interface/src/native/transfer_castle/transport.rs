use super::*;
use nucleus::transfer::transport::{Activity, Direction, Journey, Step, Terms};


fn endpoint(world: &World, owner: Entity, uid: &str) -> String {
    world
        .get::<View>(owner)
        .and_then(|view| view.records.iter().find(|row| row["uid"] == uid))
        .map(title)
        .unwrap_or_else(|| "Address unavailable in this view".into())
}

pub(super) fn render(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let Some((promise, terms)) = array(transfer, "promises").iter().find_map(|promise| {
        serde_json::from_value::<Terms>(promise["transport"].clone())
            .ok()
            .map(|terms| (promise, terms))
    }) else {
        return;
    };
    let block = ui::stack(world, parent);
    let caption = format!(
        "{} · {} · {} of {}",
        if terms.direction == Direction::Need {
            "Need"
        } else {
            "Offer"
        },
        if terms.activity == Activity::Ride {
            "Ride"
        } else {
            "Delivery"
        },
        terms.count,
        terms.capacity
    );
    crate::native::edit_mode::label(world, block, &caption, 17.0);
    if let Some(available) = transfer["transport_capacity"]["available"].as_u64() {
        crate::native::edit_mode::label(
            world,
            block,
            &format!("Available capacity: {available}"),
            14.0,
        );
    }
    for (label, uid) in [
        ("Origin", &terms.origin),
        ("Destination", &terms.destination),
    ] {
        let caption = format!("{label}: {}", endpoint(world, owner, uid));
        ui::button(
            world,
            block,
            owner,
            &caption,
            ui::Command::Record(uid.clone()),
        );
    }
    crate::native::edit_mode::label(
        world,
        block,
        &format!(
            "Pickup: {} → {}",
            text(promise, "window_start"),
            text(promise, "window_end")
        ),
        14.0,
    );
    let person = person(world, owner);
    if !person.is_empty()
        && array(transfer, "parties")
            .iter()
            .any(|party| party["actor"] == person)
    {
        let role = if promise["giver"] == person || promise["party"] == person {
            "driver/courier"
        } else {
            "passenger/recipient"
        };
        crate::native::edit_mode::label(
            world,
            block,
            "Each participant chooses recipients, precision and duration independently. Sharing stays off until Start and source-device approval.",
            13.0,
        );
        crate::native::location::sharing_review(
            world,
            block,
            &crate::transfers::transport::sharing_record(transfer, &person),
            Some(&text(transfer, "uid")),
            Some(&person),
            &format!("Review my {role} location sharing"),
        );
    }
    progress_controls(world, owner, block, transfer);
    crate::native::organ_castle::transport_discovery(
        world,
        owner,
        block,
        &text(transfer, "uid"),
        terms.activity,
    );
}

fn progress_controls(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let Ok(state) = serde_json::from_value::<Journey>(transfer["journey"].clone()) else {
        return;
    };
    let person = person(world, owner);
    for progress in &state.participants {
        let name = array(transfer, "parties")
            .iter()
            .find(|party| party["actor"] == progress.person)
            .map(title)
            .unwrap_or_else(|| "Participant".into());
        crate::native::edit_mode::label(
            world,
            parent,
            &format!(
                "{name}: {}",
                progress
                    .step
                    .map(Step::caption)
                    .unwrap_or("No progress reported")
            ),
            13.0,
        );
    }
    let Some(_) = state
        .participants
        .iter()
        .find(|progress| progress.person == person)
    else {
        return;
    };
    if !state.agreed {
        crate::native::edit_mode::label(
            world,
            parent,
            "Agree the current Transfer terms to report journey progress.",
            13.0,
        );
        return;
    }
    for form in crate::transfers::transport::journey_forms(transfer, &person) {
        let caption = form.title.clone();
        ui::button(world, parent, owner, &caption, ui::Command::Open(form));
    }
}
