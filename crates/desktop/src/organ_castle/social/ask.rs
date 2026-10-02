use super::*;

pub(super) fn view(world: &mut World, owner: Entity, parent: Entity, asks: &Value, can_edit: bool) {
    label(
        world,
        parent,
        "Ask contacts searches their public caches. They can read your question. Their separate onward consent may disclose it to more contacts within the same thirty-second deadline and shared work budget.",
    );
    let enable = form(
        world,
        owner,
        parent,
        "Save this device's contact search participation",
        action(json!({"command":"configure-ask","enabled":asks["enabled"]})),
        vec![Field(
            "/request/enabled",
            "Allow contact queries on this device",
            boolean(),
            asks["enabled"].clone(),
        )],
        None,
    );
    disable_edit_form(world, enable, can_edit);
    form(
        world,
        owner,
        parent,
        "Refresh contact query progress",
        action(json!({"command":"ask-status"})),
        vec![],
        None,
    );
    for contact in asks["contacts"].as_array().into_iter().flatten() {
        let name = contact["name"].as_str().unwrap_or("Contact");
        let choice = &contact["choice"];
        label(world, parent, &format!("Contact search with {name}"));
        if contact["retired"] == true {
            let clear = form(
                world,
                owner,
                parent,
                "Clear removed or blocked contact's query consent",
                action(
                    json!({"command":"set-ask-contact","choice":{"organ":choice["organ"],"ask":false,"answer":false,"forward":false}}),
                ),
                vec![],
                Some(
                    "Clear this retained consent? Query sharing with this contact is already paused.",
                ),
            );
            disable_edit_form(world, clear, can_edit);
            continue;
        }
        let consent = form(
            world,
            owner,
            parent,
            "Save this contact's separate query permissions",
            action(json!({"command":"set-ask-contact","choice":choice})),
            vec![
                Field(
                    "/request/choice/ask",
                    "Let me ask this contact",
                    boolean(),
                    choice["ask"].clone(),
                ),
                Field(
                    "/request/choice/answer",
                    "Answer this contact from permitted public announcements",
                    boolean(),
                    choice["answer"].clone(),
                ),
                Field(
                    "/request/choice/forward",
                    "Let questions received from this contact continue onward",
                    boolean(),
                    choice["forward"].clone(),
                ),
            ],
            Some(
                "Change query disclosure with this contact? These choices grant no private conversation history or general sync.",
            ),
        );
        disable_edit_form(world, consent, can_edit);
        if contact["endpoint"].is_null() || contact["unreachable"] == true {
            label(
                world,
                parent,
                "Queries wait for a verified contact endpoint and reachability.",
            );
        }
        if asks["enabled"] == true
            && choice["ask"] == true
            && contact["endpoint"].is_string()
            && contact["unreachable"] != true
        {
            let ask = form(
                world,
                owner,
                parent,
                &format!("Ask {name}"),
                action(
                    json!({"command":"start-ask","query":nucleus::social::Search::default(),"contacts":[choice["organ"]]}),
                ),
                vec![
                    Field(
                        "/request/query/text",
                        "Words to find",
                        Kind::Text,
                        json!(""),
                    ),
                    Field(
                        "/request/query/direction",
                        "Kind of announcement",
                        Kind::Choice(vec![
                            ("Both".into(), Value::Null),
                            ("Needs".into(), json!("need")),
                            ("Contributions".into(), json!("contribution")),
                        ]),
                        Value::Null,
                    ),
                    Field(
                        "/request/query/language",
                        "Language (optional)",
                        Kind::Text,
                        json!(""),
                    ),
                    Field(
                        "/request/query/area",
                        "Declared area (optional)",
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
                        "Unit (optional)",
                        Kind::Text,
                        json!(""),
                    ),
                ],
                Some(
                    "Send these words and filters to this contact? They can read the question and may pass it onward under separate consent. Cancel stops local work but cannot undo disclosure.",
                ),
            );
            disable_edit_form(world, ask, can_edit);
        }
    }
    for query in asks["queries"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "Question: {} · {} · saved results: {} · deadline: {}",
                query["query"]["text"].as_str().unwrap_or(""),
                query["state"].as_str().unwrap_or("pending"),
                query["count"],
                query["deadline"]
            ),
        );
        if let Some(error) = query["error"].as_str() {
            label(world, parent, error);
        }
        if query["state"] == "pending" {
            let cancel = form(
                world,
                owner,
                parent,
                "Cancel this contact query",
                action(json!({"command":"cancel-ask","id":query["id"]})),
                vec![],
                None,
            );
            disable_edit_form(world, cancel, can_edit);
        } else {
            form(
                world,
                owner,
                parent,
                "View this query's saved answers",
                action(json!({"command":"ask-results","id":query["id"]})),
                vec![],
                None,
            );
        }
    }
    let clear = form(
        world,
        owner,
        parent,
        "Clear finished local contact-query history",
        action(json!({"command":"clear-asks"})),
        vec![],
        Some(
            "Remove finished local questions and their saved result lists? Already disclosed questions and permitted public cache entries can remain.",
        ),
    );
    disable_edit_form(world, clear, can_edit);
}
