use super::*;

pub(super) fn controls(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    document: &Value,
    can_edit: bool,
) {
    let services = chosen_servers(world, owner, "query");
    let selected = services
        .as_array()
        .and_then(|values| values.first())
        .cloned()
        .unwrap_or(json!(""));
    let control = form(
        world,
        owner,
        parent,
        "Preview a report to one operator",
        action(
            json!({"command":"preview-report","post":document["id"],"service":selected,"explanation":""}),
        ),
        vec![
            Field(
                "/request/service",
                "Chosen operator endpoint",
                Kind::Text,
                selected,
            ),
            Field(
                "/request/explanation",
                "Your explanation (optional, up to 2,000 characters)",
                Kind::Text,
                json!(""),
            ),
        ],
        None,
    );
    disable_edit_form(world, control, can_edit);
}

pub(super) fn settings(world: &mut World, owner: Entity, parent: Entity, can_edit: bool) {
    for (title, command) in [
        ("My submitted reports", "reports"),
        ("Review this operator's report inbox", "received-reports"),
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
}

pub(super) fn view(world: &mut World, owner: Entity, parent: Entity, value: &Value) -> bool {
    if let Some(report) = value.get("report_preview") {
        label(world, parent, "Review this public report before sending");
        label(world, parent, value["status"].as_str().unwrap_or_default());
        label(
            world,
            parent,
            &format!(
                "Chosen operator: {}",
                report["service"].as_str().unwrap_or_default()
            ),
        );
        label(
            world,
            parent,
            report["document"]["title"].as_str().unwrap_or_default(),
        );
        label(
            world,
            parent,
            report["document"]["text"].as_str().unwrap_or_default(),
        );
        label(
            world,
            parent,
            &serde_json::to_string_pretty(&report["document"]).unwrap_or_default(),
        );
        label(
            world,
            parent,
            report["explanation"].as_str().unwrap_or_default(),
        );
        let control = form(
            world,
            owner,
            parent,
            "Send this exact report",
            action(
                json!({"command":"send-report","document":report,"preview_hash":value["preview_hash"]}),
            ),
            vec![],
            Some(
                "Send this displayed public announcement and your explanation to the chosen operator? Its transport can observe your endpoint. No private source or Organ profile is attached.",
            ),
        );
        disable_edit_form(world, control, value["can_manage_services"] == true);
        return true;
    }
    if let Some(reports) = value["reports"].as_array() {
        label(world, parent, "My report work on this device");
        label(world, parent, value["status"].as_str().unwrap_or_default());
        if reports.is_empty() {
            label(world, parent, "No retained report work.");
        }
        for report in reports {
            label(
                world,
                parent,
                &format!(
                    "{} · {}",
                    report["title"].as_str().unwrap_or_default(),
                    report["state"].as_str().unwrap_or_default()
                ),
            );
            label(
                world,
                parent,
                &format!(
                    "Operator: {} · expires at {}",
                    report["service"].as_str().unwrap_or_default(),
                    report["expires_at"]
                ),
            );
            label(world, parent, report["error"].as_str().unwrap_or_default());
        }
        let control = form(
            world,
            owner,
            parent,
            "Clear my report work",
            action(json!({"command":"clear-reports"})),
            vec![],
            Some(
                "Clear your report status and stop pending retries on this device? An in-flight or already accepted report cannot be recalled.",
            ),
        );
        disable_edit_form(world, control, value["can_manage_services"] == true);
        return true;
    }
    if let Some(reports) = value["received_reports"].as_array() {
        label(world, parent, "This operator's report inbox");
        label(world, parent, value["status"].as_str().unwrap_or_default());
        if reports.is_empty() {
            label(world, parent, "No more received reports on this page.");
        }
        for entry in reports {
            let report = &entry["document"];
            label(
                world,
                parent,
                report["document"]["title"].as_str().unwrap_or_default(),
            );
            label(
                world,
                parent,
                report["document"]["text"].as_str().unwrap_or_default(),
            );
            label(
                world,
                parent,
                report["explanation"].as_str().unwrap_or_default(),
            );
            label(
                world,
                parent,
                &format!(
                    "Public post: {} · report received at {}",
                    report["document"]["id"].as_str().unwrap_or_default(),
                    entry["received_at"]
                ),
            );
            let control = form(
                world,
                owner,
                parent,
                "Dismiss this report",
                action(json!({"command":"dismiss-report","id":entry["id"]})),
                vec![],
                Some(
                    "Dismiss this report's retained evidence on this host? Replay protection and daily admission limits remain.",
                ),
            );
            disable_edit_form(world, control, value["can_manage_services"] == true);
            let control = form(
                world,
                owner,
                parent,
                "Remove this host's current listing",
                action(
                    json!({"command":"remove-listing","post":report["document"]["id"],"reason":""}),
                ),
                vec![Field(
                    "/request/reason",
                    "Operator's removal reason (optional)",
                    Kind::Text,
                    json!(""),
                )],
                Some(
                    "Remove this public listing on this host? Check current listing evidence before deciding; a reporter's claim alone is not proof.",
                ),
            );
            disable_edit_form(world, control, value["can_manage_services"] == true);
        }
        if let Some(after) = value["next_after"].as_str() {
            form(
                world,
                owner,
                parent,
                "Next received reports",
                action(json!({"command":"received-reports","after":after})),
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
    fn report_preview_and_operator_controls_are_deliberate_typed_and_permission_aware() {
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
        let post = json!({"protocol":"lince.snippet.1","id":nucleus::new_uid("post"),"nonce":"public","revision":"1","created_at":1,"issued_at":1,"expires_at":604801,"mode":"anonymous","signing_key":"public key","alias":"","title":"Bicycle help","text":"Public evidence only","direction":"need","language":"","area":"","availability":"","state":"active","redistribute":false,"destinations":[],"signature":"public signature"});
        let report = json!({"protocol":"lince.public-report.1","id":nucleus::new_uid("report"),"service":"chosen endpoint","document":post,"explanation":"Please review","created_at":1,"expires_at":604801});
        settings(app.world_mut(), owner, parent, false);
        controls(app.world_mut(), owner, parent, &post, false);
        assert!(view(
            app.world_mut(),
            owner,
            parent,
            &json!({"report_preview":report,"preview_hash":"reviewed hash","can_manage_services":false})
        ));
        assert!(view(
            app.world_mut(),
            owner,
            parent,
            &json!({"reports":[{"id":report["id"],"state":"pending","title":"Bicycle help","service":"chosen endpoint","expires_at":604801}],"can_manage_services":false})
        ));
        assert!(view(
            app.world_mut(),
            owner,
            parent,
            &json!({"received_reports":[{"id":report["id"],"document":report,"received_at":1}],"can_manage_services":false})
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
        assert_eq!(forms.len(), 7);
        for (entity, payload, confirmation) in forms {
            let _: engine::actions::Action =
                serde_json::from_value(forms::payload(app.world(), entity).unwrap()).unwrap();
            if matches!(
                payload["request"]["command"].as_str(),
                Some("send-report" | "clear-reports" | "dismiss-report" | "remove-listing")
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
