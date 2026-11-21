use bevy::prelude::*;

use super::super::{Field, Intent, Panel};
use super::{button, input, label};

pub(super) fn draw(world: &mut World, entity: Entity, panel: &mut Panel) {
    let field = input(
        world,
        entity,
        "Private location reference",
        &panel.reference,
    );
    world
        .get_mut::<bevy::text::EditableText>(field)
        .expect("the reference input was spawned with an editor")
        .max_characters = Some(1024);
    panel.fields.push((Field::Reference, field));
    button(
        world,
        entity,
        entity,
        "Use location reference",
        Intent::LoadReference,
    );
    label(
        world,
        entity,
        "A reference identifies a view. Its current visibility rules still decide access.",
    );
    if let Some(grant) = panel
        .invitation
        .as_ref()
        .and_then(|invitation| invitation.grant.as_ref())
    {
        let expiry = chrono::DateTime::from_timestamp_millis(grant.expires_at_ms)
            .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "unavailable".into());
        label(
            world,
            entity,
            &format!(
                "Intended Person: {}. Minimum shared radius: {}. Expires at {expiry} on the source clock. Authenticate as this recipient; current permissions can further restrict or deny access.",
                grant.recipient,
                grant.radius_metres.map_or_else(
                    || "permitted exact position".into(),
                    |radius| format!("{radius} metres")
                )
            ),
        );
    }
    panel.fields.push((
        Field::Person,
        input(
            world,
            entity,
            if panel
                .invitation
                .as_ref()
                .is_some_and(|invitation| invitation.grant.is_some())
            {
                "Named recipient Person UID (required)"
            } else {
                "Named recipient Person UID (optional for Organ access)"
            },
            &panel.person,
        ),
    ));
    panel.fields.push((
        Field::Record,
        input(world, entity, "Shared location Record UID", &panel.record),
    ));
    panel.fields.push((
        Field::Node,
        input(
            world,
            entity,
            "Shared location viewing endpoint",
            &panel.node,
        ),
    ));
    label(
        world,
        entity,
        "This view grants no access to the Record or Transfer terms.",
    );
}
