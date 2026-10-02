use super::*;

fn timestamp(value: &Value) -> String {
    value
        .as_i64()
        .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| "No retained check time".into())
}

pub(super) fn summary(world: &mut World, owner: Entity, parent: Entity, value: &Value) {
    if let Some(ranking) = value["ranking"].as_str() {
        label(world, parent, ranking);
    }
    for conflict in value["conflicts"].as_array().into_iter().flatten() {
        if conflict["withdrawn"] == true {
            label(
                world,
                parent,
                "The known signed withdrawal remains in force and can still propagate. Conflicting evidence cannot reopen this post.",
            );
        }
        label(
            world,
            parent,
            &format!(
                "Hidden announcement {} · revision {}: two valid signatures disagree. It stays hidden until its author publishes a newer signed revision or withdrawal.",
                conflict["post"].as_str().unwrap_or_default(),
                conflict["revision"].as_str().unwrap_or_default()
            ),
        );
        if conflict["evidence_limited"] == true {
            label(
                world,
                parent,
                "The evidence quota is full. The hidden state is retained even though the complete signed pair could not be saved.",
            );
        } else {
            label(
                world,
                parent,
                &format!(
                    "Signed evidence retained: {} / {} · observed {}",
                    conflict["hash"].as_str().unwrap_or_default(),
                    conflict["conflicting_hash"].as_str().unwrap_or_default(),
                    timestamp(&conflict["observed_at"])
                ),
            );
        }
    }
    if let Some(after) = value["next_conflict_after"].as_str() {
        let mut query = value["query"].clone();
        query["after"] = json!(after);
        form(
            world,
            owner,
            parent,
            "Review the next local conflict page",
            action(json!({"command":"search","query":query,"services":[]})),
            vec![],
            None,
        );
    }
}

pub(super) fn row(world: &mut World, parent: Entity, row: &Value) {
    label(
        world,
        parent,
        &format!(
            "Cached via: {} · newest retained source check {} · announcement expires {}. Availability remains unconfirmed.",
            row["source"].as_str().unwrap_or_default(),
            timestamp(&row["checked_at"]),
            timestamp(&row["document"]["expires_at"])
        ),
    );
    let sources = row["sources"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if !sources.is_empty() {
        label(
            world,
            parent,
            &format!(
                "{} retained source observations. These are copies, not independent votes or proof of physical nearness.",
                sources.len()
            ),
        );
        for source in sources.iter().take(8) {
            label(
                world,
                parent,
                &format!(
                    "{} · checked {}",
                    source["source"].as_str().unwrap_or_default(),
                    timestamp(&source["checked_at"])
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_and_source_reviews_are_local_and_offer_no_publication_action() {
        let mut app = crate::sand_panel::tests::app();
        app.add_plugins(OrganCastlePlugin);
        let owner = app.world_mut().spawn_empty().id();
        let parent = app.world_mut().spawn_empty().id();
        summary(
            app.world_mut(),
            owner,
            parent,
            &json!({"ranking":"Within this bounded page","conflicts":[{"post":"post-visible-public-id","revision":"3","evidence_limited":true}]}),
        );
        row(
            app.world_mut(),
            parent,
            &json!({"source":"Chosen directory","checked_at":1790899200i64,"document":{"expires_at":1790985600i64},"sources":[{"source":"Chosen directory","checked_at":1790899200i64}]}),
        );
        let text: Vec<String> = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect();
        assert!(
            text.iter()
                .any(|text| text.contains("two valid signatures disagree"))
        );
        assert!(
            text.iter()
                .any(|text| text.contains("hidden state is retained"))
        );
        assert!(
            text.iter()
                .any(|text| text.contains("not independent votes"))
        );
        assert!(
            text.iter()
                .any(|text| text.contains("2026-10-02T00:00:00+00:00"))
        );
        assert_eq!(
            app.world_mut()
                .query::<&forms::Form>()
                .iter(app.world())
                .count(),
            0
        );
        assert!(app.world().resource::<Requests>().actions.is_empty());
    }

    #[test]
    fn conflict_continuation_preserves_filters_and_queries_only_the_local_cache() {
        let mut app = crate::sand_panel::tests::app();
        app.add_plugins(OrganCastlePlugin);
        let owner = app.world_mut().spawn_empty().id();
        let parent = app.world_mut().spawn_empty().id();
        let query = nucleus::social::Search {
            text: "bicycle".into(),
            language: "pt".into(),
            area: "Recife".into(),
            direction: Some(nucleus::social::Direction::Need),
            ..Default::default()
        };
        summary(
            app.world_mut(),
            owner,
            parent,
            &json!({"query":query,"next_conflict_after":"post-last-raw-candidate","services":["unused-external-service"]}),
        );
        let form = app
            .world_mut()
            .query::<&forms::Form>()
            .single(app.world())
            .unwrap();
        let _: engine::actions::Action = serde_json::from_value(form.payload.clone()).unwrap();
        let request: nucleus::social::Command =
            serde_json::from_value(form.payload["request"].clone()).unwrap();
        let nucleus::social::Command::Search {
            query: next,
            services,
        } = request
        else {
            panic!("Expected a local search continuation")
        };
        assert_eq!(next.text, query.text);
        assert_eq!(next.language, query.language);
        assert_eq!(next.area, query.area);
        assert_eq!(next.direction, query.direction);
        assert_eq!(next.after.as_deref(), Some("post-last-raw-candidate"));
        assert!(services.is_empty());
        assert!(app.world().resource::<Requests>().actions.is_empty());
    }
}
