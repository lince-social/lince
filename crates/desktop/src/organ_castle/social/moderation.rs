use super::*;

pub(super) fn controls(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    document: &Value,
    can_edit: bool,
) {
    for (title, whole_author, confirmation) in [
        (
            "Hide this announcement",
            false,
            "Hide this public announcement on your owned devices?",
        ),
        (
            "Hide this public author",
            true,
            "Hide announcements from this public posting identity on your owned devices? An anonymous identity is not linked to an Organ profile.",
        ),
    ] {
        let control = form(
            world,
            owner,
            parent,
            title,
            action(
                json!({"command":"mute-post","post":document["id"],"whole_author":whole_author}),
            ),
            vec![],
            Some(confirmation),
        );
        disable_edit_form(world, control, can_edit);
    }
}

pub(super) fn settings(world: &mut World, owner: Entity, parent: Entity, can_edit: bool) {
    for (title, command) in [
        ("Hidden announcements and authors", "mutes"),
        ("Review this host's removed listings", "removed-listings"),
    ] {
        let control = form(
            world,
            owner,
            parent,
            title,
            action(json!({"command":command})),
            vec![],
            None,
        );
        disable_edit_form(world, control, can_edit);
    }
    let control = form(
        world,
        owner,
        parent,
        "Remove a listing on this host",
        action(json!({"command":"remove-listing","post":"","reason":""})),
        vec![
            Field(
                "/request/post",
                "Public announcement ID",
                Kind::Text,
                json!(""),
            ),
            Field(
                "/request/reason",
                "Operator's removal reason (optional)",
                Kind::Text,
                json!(""),
            ),
        ],
        Some(
            "Remove this listing from this host? Other operators make independent decisions. Signed evidence will remain.",
        ),
    );
    disable_edit_form(world, control, can_edit);
}

pub(super) fn view(world: &mut World, owner: Entity, parent: Entity, value: &Value) -> bool {
    if let Some(entries) = value["mutes"].as_object() {
        label(world, parent, "Hidden announcements and public authors");
        label(world, parent, value["status"].as_str().unwrap_or_default());
        if entries.is_empty() {
            label(
                world,
                parent,
                "No public announcements or authors are hidden.",
            );
        }
        for (key, entry) in entries {
            label(
                world,
                parent,
                &format!(
                    "{} · {}",
                    entry["title"].as_str().unwrap_or_default(),
                    entry["kind"].as_str().unwrap_or("Public target")
                ),
            );
            let control = form(
                world,
                owner,
                parent,
                "Show this target again",
                action(json!({"command":"unmute","key":key})),
                vec![],
                None,
            );
            disable_edit_form(world, control, value["can_manage_services"] == true);
        }
        if let Some(after) = value["next_after"].as_str() {
            form(
                world,
                owner,
                parent,
                "Next hidden targets",
                action(json!({"command":"mutes","after":after})),
                vec![],
                None,
            );
        }
        return true;
    }
    if let Some(entries) = value["removed_listings"].as_array() {
        label(world, parent, "Listings removed on this host");
        label(world, parent, value["status"].as_str().unwrap_or_default());
        if entries.is_empty() {
            label(world, parent, "No more removed listings on this page.");
        }
        for entry in entries {
            label(world, parent, entry["post"].as_str().unwrap_or_default());
            label(
                world,
                parent,
                entry["document"]["title"]
                    .as_str()
                    .unwrap_or("The display cache no longer retains this announcement"),
            );
            label(world, parent, entry["reason"].as_str().unwrap_or_default());
            let control = form(
                world,
                owner,
                parent,
                "Restore this listing if still valid",
                action(json!({"command":"restore-listing","post":entry["post"]})),
                vec![],
                Some(
                    "Restore this host's listing after checking its current signature, authority and expiry?",
                ),
            );
            disable_edit_form(world, control, value["can_manage_services"] == true);
        }
        if let Some(after) = value["next_after"].as_str() {
            form(
                world,
                owner,
                parent,
                "Next removed listings",
                action(json!({"command":"removed-listings","after":after})),
                vec![],
                None,
            );
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moderation_controls_are_typed_deliberate_and_disable_changes_for_viewing_devices() {
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
        let parent = app.world_mut().spawn_empty().id();
        let post = nucleus::new_uid("post");
        settings(app.world_mut(), owner, parent, false);
        controls(app.world_mut(), owner, parent, &json!({"id":post}), false);
        let key = format!("mute_{}", "a".repeat(64));
        assert!(view(
            app.world_mut(),
            owner,
            parent,
            &json!({"mutes":{key:{"title":"Bicycle tools","kind":"anonymous-author"}},"can_manage_services":false})
        ));
        assert!(view(
            app.world_mut(),
            owner,
            parent,
            &json!({"removed_listings":[{"post":post,"document":{"title":"Removed listing"},"reason":"Host policy"}],"can_manage_services":false})
        ));
        let forms: Vec<_> = app
            .world_mut()
            .query::<(Entity, &forms::Form)>()
            .iter(app.world())
            .filter_map(|(entity, form)| {
                let mut node = entity;
                while let Some(child) = app.world().get::<ChildOf>(node) {
                    node = child.parent();
                    if node == parent {
                        return Some((entity, form.payload.clone(), form.confirmation.clone()));
                    }
                }
                None
            })
            .collect();
        let mutations: Vec<_> = forms
            .iter()
            .filter(|(_, payload, _)| {
                matches!(
                    payload["request"]["command"].as_str(),
                    Some("mute-post" | "unmute" | "remove-listing" | "restore-listing")
                )
            })
            .collect();
        assert_eq!(mutations.len(), 5);
        for (entity, payload, confirmation) in forms {
            let _: engine::actions::Action =
                serde_json::from_value(forms::payload(app.world(), entity).unwrap()).unwrap();
            if matches!(
                payload["request"]["command"].as_str(),
                Some("remove-listing" | "restore-listing" | "mute-post")
            ) {
                assert!(confirmation.is_some());
            }
            let buttons: Vec<_> = app
                .world_mut()
                .query::<(Entity, &crate::actions::ActionButton)>()
                .iter(app.world())
                .filter_map(|(button, _)| {
                    let mut node = button;
                    while let Some(child) = app.world().get::<ChildOf>(node) {
                        node = child.parent();
                        if node == entity {
                            return Some(button);
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
        }
        assert!(app.world().resource::<Requests>().actions.is_empty());
    }
}
