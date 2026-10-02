use super::*;

fn timestamp(value: &Value) -> String {
    value
        .as_i64()
        .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| "Unknown".into())
}

pub(super) fn view(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    value: &Value,
    can_edit: bool,
) {
    label(world, parent, "Retained private Message delivery");
    label(
        world,
        parent,
        value["metadata"]["content"]["kind"]["text"]
            .as_str()
            .unwrap_or_default(),
    );
    label(
        world,
        parent,
        &format!(
            "Original creation: {} · current deadline: {} · stage: {}",
            timestamp(&value["metadata"]["content"]["issued_at"]),
            timestamp(&value["delivery"]["expires_at"]),
            value["delivery"]["stage"].as_str().unwrap_or("Unknown")
        ),
    );
    if let Some(error) = value["delivery"]["error"].as_str() {
        label(world, parent, error);
    }
    if value["can_resend"] == true {
        label(
            world,
            parent,
            &format!(
                "A confirmed resend keeps this Message and gives new encrypted copies up to thirty days from confirmation (approximately {} from this review). Sending still waits for current authorized keys.",
                timestamp(&value["new_expires_at"])
            ),
        );
        let control = form(
            world,
            owner,
            parent,
            "Resend this expired Message",
            action(json!({"command":"resend-expired-private","message":value["message"]})),
            vec![],
            Some(
                "Resend this same retained text with a fresh thirty-day delivery window? Its original creation date and history remain. Earlier remote copies cannot be recalled. Only an accepted, open conversation permits this action.",
            ),
        );
        disable_edit_form(world, control, can_edit);
    } else if value["delivery"]["expires_at"]
        .as_i64()
        .is_some_and(|expiry| expiry > value["observed_at"].as_i64().unwrap_or(i64::MAX))
        && !matches!(
            value["delivery"]["stage"].as_str(),
            Some("recipient-durable" | "recipient-refused")
        )
    {
        let control = form(
            world,
            owner,
            parent,
            "Resume this saved Message on this device",
            action(json!({"command":"resume-private","message":value["message"]})),
            vec![],
            None,
        );
        disable_edit_form(world, control, can_edit);
        label(world, parent, "Resume keeps the current delivery deadline.");
    } else {
        label(
            world,
            parent,
            "This Message cannot be renewed here. Recipient-durable/refused messages, introductions, controls and closed conversations retain their history and original deadline.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resend_is_confirmed_permission_aware_and_separate_from_resume_and_thread_review() {
        let mut app = crate::sand_panel::tests::app();
        app.add_plugins(OrganCastlePlugin);
        let owner = app.world_mut().spawn_empty().id();
        let parent = app.world_mut().spawn_empty().id();
        let message = nucleus::new_uid("r");
        let mut value = json!({"message":message,"metadata":{"content":{"issued_at":1,"kind":{"kind":"text","text":"Original immutable text"}}},"delivery":{"stage":"expired","expires_at":2},"observed_at":3,"new_expires_at":2592003,"can_resend":true});
        view(app.world_mut(), owner, parent, &value, false);
        value["can_resend"] = json!(false);
        value["delivery"]["expires_at"] = json!(4);
        view(app.world_mut(), owner, parent, &value, true);
        value["delivery"]["stage"] = json!("recipient-refused");
        let refused = app.world_mut().spawn_empty().id();
        view(app.world_mut(), owner, refused, &value, true);
        crate::organ_castle::social_delivery_controls(app.world_mut(), owner, parent, &message);
        let forms: Vec<(Entity, Value, Option<String>)> = app
            .world_mut()
            .query::<(Entity, &forms::Form)>()
            .iter(app.world())
            .map(|(entity, form)| (entity, form.payload.clone(), form.confirmation.clone()))
            .collect();
        assert_eq!(forms.len(), 3);
        for (_, payload, _) in &forms {
            assert!(matches!(
                serde_json::from_value::<engine::actions::Action>(payload.clone()).unwrap(),
                engine::actions::Action::Social { .. }
            ));
        }
        let resend = forms
            .iter()
            .find(|(_, payload, _)| payload["request"]["command"] == "resend-expired-private")
            .unwrap();
        assert!(resend.2.is_some());
        let buttons: Vec<Entity> = app
            .world_mut()
            .query::<(Entity, &crate::actions::ActionButton)>()
            .iter(app.world())
            .filter_map(|(entity, _)| {
                let mut node = entity;
                while let Some(child) = app.world().get::<ChildOf>(node) {
                    node = child.parent();
                    if node == resend.0 {
                        return Some(entity);
                    }
                }
                None
            })
            .collect();
        assert!(!buttons.is_empty());
        assert!(buttons.iter().all(|button| {
            app.world()
                .get::<bevy::ui::InteractionDisabled>(*button)
                .is_some()
        }));
        assert!(
            forms
                .iter()
                .any(|(_, payload, confirmation)| payload["request"]["command"]
                    == "resume-private"
                    && confirmation.is_none())
        );
        assert!(
            forms
                .iter()
                .any(|(_, payload, _)| payload["request"]["command"] == "private-delivery-status")
        );
        assert!(app.world().resource::<Requests>().actions.is_empty());
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == "Original immutable text")
        );
    }
}
