use super::*;
use nucleus::trust_analysis::{claims::Document, query::Query};

#[derive(Clone)]
pub(super) struct OpenSnapshot(pub Document);

impl Action for OpenSnapshot {
    fn apply(&self, world: &mut World, entity: Entity) {
        let mut cursor = Some(entity);
        while let Some(entity) = cursor {
            let root = if world
                .get::<crate::native::workspace::Workspaces>(entity)
                .is_some()
            {
                Some(entity)
            } else {
                world
                    .get::<crate::native::sand::InBox>(entity)
                    .map(|root| root.0)
            };
            if let Some(root) = root {
                if let Some(owner) =
                    open_view(world, root, Some(&self.0.statement.descriptor.subject))
                {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    load_snapshot(&mut view, &self.0);
                    view.notice = "Profile snapshot opened. Independently recognize its issuer before checking; no evidence was imported and no remote search started.".into();
                    drop(view);
                    render(world, owner);
                }
                return;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

pub(super) fn load_snapshot(view: &mut View, document: &Document) {
    let descriptor = &document.statement.descriptor;
    if let Ok(query) = descriptor.checking_query(
        &Query::for_subject(descriptor.subject.clone()),
        document.statement.as_of,
    ) {
        analysis::load(view, query);
    }
    view.anchor = view.person.clone();
    view.anchor_organ = false;
    view.claims.snapshot = Some(document.clone());
    view.analyze_after_context = true;
}

pub(super) fn cache(view: &mut View, result: &Value) {
    let Ok(document) = serde_json::from_value::<Document>(result["document"].clone()) else {
        return;
    };
    let Ok(query) = serde_json::from_value::<Query>(result["query"].clone()) else {
        return;
    };
    let (Ok(document_hash), Ok(query_hash)) = (document.id(), query.id()) else {
        return;
    };
    view.claims.checks.retain(|entry| {
        entry["document_hash"] != document_hash.as_str()
            || entry["query_hash"] != query_hash.as_str()
            || entry["as_organ"] != view.as_organ
    });
    let entry = json!({"person":view.person,"as_organ":view.as_organ,"document_hash":document_hash,"query_hash":query_hash,"check":result});
    let size = serde_json::to_vec(&entry).map_or(usize::MAX, |bytes| bytes.len());
    if size > nucleus::trust_analysis::MAX_BYTES {
        return;
    }
    while view.claims.checks.len() >= 8
        || serde_json::to_vec(&view.claims.checks)
            .map_or(usize::MAX, |bytes| bytes.len())
            .saturating_add(size)
            .saturating_add(1)
            > nucleus::trust_analysis::MAX_BYTES
    {
        if view.claims.checks.is_empty() {
            return;
        }
        view.claims.checks.remove(0);
    }
    view.claims.checks.push(entry);
}

fn cached(view: &View, document: &Document) -> Option<Value> {
    let query = document
        .statement
        .descriptor
        .checking_query(&analysis::build(view).ok()?, document.statement.as_of)
        .ok()?;
    let hash = document.id().ok()?;
    let policy = query.id().ok()?;
    view.claims
        .checks
        .iter()
        .rev()
        .find(|entry| {
            entry["person"] == view.person
                && entry["as_organ"] == view.as_organ
                && entry["document_hash"] == hash.as_str()
                && entry["query_hash"] == policy.as_str()
        })
        .map(|entry| entry["check"].clone())
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let Some(document) = view.claims.snapshot.clone() else {
        return;
    };
    let check = cached(view, &document);
    let showing_check = matches!(
        view.result["kind"].as_str(),
        Some("claim_check" | "credential_check")
    );
    let statement = &document.statement;
    label(world, owner, statement.category.label());
    label(
        world,
        owner,
        &analysis::measurement_caption(&statement.descriptor.measurement),
    );
    label(
        world,
        owner,
        &format!(
            "{:?} · {:?} · reporting period {:?} to {:?}",
            statement.descriptor.role,
            statement.descriptor.level,
            statement.descriptor.from,
            statement.descriptor.until
        ),
    );
    if statement.withdrawn {
        label(
            world,
            owner,
            "This snapshot withdraws the claim; it asserts no current contribution total.",
        );
    }
    if statement.expires_at <= nucleus::execution::now().timestamp() {
        label(
            world,
            owner,
            "This profile snapshot has expired. Obtain a newer signed revision before checking current support.",
        );
    }
    label(
        world,
        owner,
        &format!(
            "Profile claim snapshot: {} · issuer {}:{} · {:?} {} · {:?} · revision {}",
            statement.title,
            statement.issuer.kind(),
            statement.issuer.uid(),
            statement.comparator,
            statement.amount,
            statement.descriptor.metric,
            statement.revision
        ),
    );
    label(
        world,
        owner,
        &format!(
            "As of {}; expires {}. {}. The profile does not establish its amount or identity. Retained newer withdrawals and conflicts take precedence when checked.",
            statement.as_of,
            statement.expires_at,
            claims::provenance(&statement.authorship)
        ),
    );
    panel::button(
        world,
        owner,
        owner,
        "Inspect snapshot signing authority without importing",
        Command::Claim(claims::UiCommand::InspectSnapshot),
    );
    panel::button(
        world,
        owner,
        owner,
        "Check snapshot locally using my source policy",
        Command::Claim(claims::UiCommand::CheckSnapshot),
    );
    if let Some(check) = check {
        label(
            world,
            owner,
            &format!(
                "Cached independent local check at {} for this exact document and source policy. It may be stale; explicitly check again to refresh. No remote search runs in the background.",
                check["checked_at"]
            ),
        );
        if !showing_check {
            claims::render_check(world, owner, &check, 0);
        }
    }
}
