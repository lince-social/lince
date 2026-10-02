use super::*;

fn editor(world: &mut World, owner: Entity, parent: Entity, filter: &Value, can_edit: bool) {
    let control = form(
        world,
        owner,
        parent,
        "Save this search filter",
        action(json!({"command":"save-subscription","filter":filter})),
        vec![
            Field(
                "/request/filter/label",
                "Private filter name",
                Kind::Text,
                filter["label"].clone(),
            ),
            Field(
                "/request/filter/query/text",
                "Public search words",
                Kind::Text,
                filter["query"]["text"].clone(),
            ),
            Field(
                "/request/filter/query/direction",
                "Need or contribution",
                Kind::Choice(vec![
                    ("Either".into(), Value::Null),
                    ("Need".into(), json!("need")),
                    ("Contribution".into(), json!("contribution")),
                ]),
                filter["query"]["direction"].clone(),
            ),
            Field(
                "/request/filter/query/language",
                "Language filter",
                Kind::Text,
                filter["query"]["language"].clone(),
            ),
            Field(
                "/request/filter/query/area",
                "Declared area filter",
                Kind::Text,
                filter["query"]["area"].clone(),
            ),
            Field(
                "/request/filter/query/concept",
                "Public concept filter",
                Kind::Text,
                filter["query"]["concept"].clone(),
            ),
            Field(
                "/request/filter/query/unit",
                "Unit filter",
                Kind::Text,
                filter["query"]["unit"].clone(),
            ),
            Field(
                "/request/filter/services",
                "Chosen directory endpoints (one per line)",
                Kind::TextList,
                filter["services"].clone(),
            ),
            Field(
                "/request/filter/interval_minutes",
                "Check interval in minutes (60–10,080)",
                Kind::Number,
                filter["interval_minutes"].clone(),
            ),
            Field(
                "/request/filter/enabled",
                "Periodic checks for this filter",
                boolean(),
                filter["enabled"].clone(),
            ),
            Field(
                "/request/filter/notifications",
                "In-app notifications for new matches",
                boolean(),
                filter["notifications"].clone(),
            ),
            Field(
                "/request/filter/quiet_start_hour",
                "Quiet hours start (local hour, 0–23)",
                Kind::Number,
                filter["quiet_start_hour"].clone(),
            ),
            Field(
                "/request/filter/quiet_end_hour",
                "Quiet hours end (local hour, 0–23; equal hours disable quiet hours)",
                Kind::Number,
                filter["quiet_end_hour"].clone(),
            ),
        ],
        Some(
            "Save these private filter settings across your owned devices? Periodic checks send the displayed filters only to the chosen directories after this device also opts in. Notifications remain a separate choice.",
        ),
    );
    disable_edit_form(world, control, can_edit);
}

pub(super) fn settings(world: &mut World, owner: Entity, parent: Entity, can_edit: bool) {
    let control = form(
        world,
        owner,
        parent,
        "Saved searches and notification settings",
        action(json!({"command":"subscriptions"})),
        vec![],
        None,
    );
    disable_edit_form(world, control, can_edit);
}

pub(super) fn view(world: &mut World, owner: Entity, parent: Entity, value: &Value) -> bool {
    let Some(filters) = value["saved_searches"].as_array() else {
        return false;
    };
    let can_edit = value["can_manage_services"] == true;
    label(world, parent, "Private saved searches");
    label(world, parent, value["status"].as_str().unwrap_or_default());
    label(
        world,
        parent,
        "Saving a filter starts no search. Each device needs its own periodic-check opt-in. Notifications respect local quiet hours and use the existing in-app feed.",
    );
    let enable = value["device_enabled"] != true;
    let control = form(
        world,
        owner,
        parent,
        if enable {
            "Enable periodic checks on this device"
        } else {
            "Stop periodic checks on this device"
        },
        action(json!({"command":"configure-subscriptions","enabled":enable})),
        vec![],
        if enable {
            Some(
                "Let this device periodically send each enabled saved filter to its explicitly chosen directories? Other network roles and private sharing remain independently configured.",
            )
        } else {
            None
        },
    );
    disable_edit_form(world, control, can_edit);
    let mut fresh = serde_json::to_value(nucleus::social::subscriptions::Subscription {
        label: "Saved search".into(),
        ..Default::default()
    })
    .unwrap();
    fresh["services"] = chosen_servers(world, owner, "query");
    label(world, parent, "Create a saved filter");
    editor(world, owner, parent, &fresh, can_edit);
    for entry in filters {
        let filter = &entry["filter"];
        label(world, parent, filter["label"].as_str().unwrap_or_default());
        if !entry["runtime"].is_null() {
            label(
                world,
                parent,
                entry["runtime"]["source"].as_str().unwrap_or_default(),
            );
            label(
                world,
                parent,
                &format!(
                    "Last completed {} · next check {}",
                    entry["runtime"]["last_completed"], entry["runtime"]["next_attempt"]
                ),
            );
            label(
                world,
                parent,
                entry["runtime"]["error"].as_str().unwrap_or_default(),
            );
        }
        editor(world, owner, parent, filter, can_edit);
        for (title, command, confirmation) in [
            ("Review retained matches", "subscription-results", None),
            (
                "Clear displayed matches",
                "clear-subscription-matches",
                Some(
                    "Clear this device's displayed matches? Tracking remains until expiry to prevent repeat alerts.",
                ),
            ),
            (
                "Remove this saved filter",
                "remove-subscription",
                Some(
                    "Remove this saved filter across your owned devices and stop future checks? A query already sent cannot be recalled.",
                ),
            ),
        ] {
            let control = form(
                world,
                owner,
                parent,
                title,
                action(json!({"command":command,"id":filter["id"]})),
                vec![],
                confirmation,
            );
            disable_edit_form(world, control, can_edit);
        }
        let control = form(
            world,
            owner,
            parent,
            "Search these chosen directories now",
            action(
                json!({"command":"search","query":filter["query"],"services":filter["services"]}),
            ),
            vec![],
            Some(
                "Send this saved filter to these chosen directories now? This deliberate search works independently from periodic checks.",
            ),
        );
        disable_edit_form(world, control, can_edit);
    }
    if let Some(after) = value["next_after"].as_str() {
        form(
            world,
            owner,
            parent,
            "Next saved filters",
            action(json!({"command":"subscriptions","after":after})),
            vec![],
            None,
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_search_settings_use_typed_opt_ins_and_never_start_queries_from_rendering() {
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
        let filter = nucleus::social::subscriptions::Subscription {
            id: nucleus::new_uid("sub"),
            label: "Bicycle help".into(),
            ..Default::default()
        };
        assert!(view(
            app.world_mut(),
            owner,
            parent,
            &json!({"saved_searches":[{"filter":filter,"runtime":null}],"device_enabled":false,"can_manage_services":false})
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
        let saves: Vec<_> = forms
            .iter()
            .filter(|(_, payload, _)| payload["request"]["command"] == "save-subscription")
            .collect();
        assert_eq!(saves.len(), 2);
        assert!(
            saves.iter().all(
                |(_, payload, _)| payload["request"]["filter"]["enabled"] == false
                    && payload["request"]["filter"]["notifications"] == false
            )
        );
        for (entity, payload, confirmation) in forms {
            let _: engine::actions::Action =
                serde_json::from_value(forms::payload(app.world(), entity).unwrap()).unwrap();
            if payload["request"]["command"] != "subscription-results" {
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
