use protein::{Predicate, Protein};
use serde_json::{Value, json};

use crate::{
    State,
    auth::{Failure, internal},
};

pub(crate) fn permits(user: Option<&store::auth::AuthUser>, permission: &str) -> bool {
    user.is_some_and(|user| user.permits(permission))
}

pub(crate) async fn read(
    state: &State,
    subject: Option<&str>,
    filter: Vec<Predicate>,
    limit: usize,
) -> Result<Vec<Value>, Failure> {
    let query = Protein {
        source: protein::Source::Record,
        filter,
        fields: Some(
            [
                "head",
                "body",
                "slug",
                "quantity",
                "unit",
                "concept_name",
                "created_at",
                "updated_at",
            ]
            .map(String::from)
            .to_vec(),
        ),
        include: Default::default(),
        aggregate: None,
        order: vec![protein::Order::Desc("created_at".into())],
        limit: Some(limit),
    };
    protein::execute_for(&state.cell.store, &query, subject)
        .await
        .map_err(internal)
}

pub(crate) async fn visible(
    state: &State,
    subject: Option<&str>,
    uid: &str,
) -> Result<bool, Failure> {
    Ok(!read(state, subject, vec![Predicate::UidEq(uid.into())], 1)
        .await?
        .is_empty())
}

fn relation(kind: &str, other: &str) -> Predicate {
    Predicate::Relation {
        kind: kind.into(),
        direction: protein::LinkDirection::Out,
        other: Some(other.into()),
    }
}

pub(crate) async fn snapshot(
    state: &State,
    user: Option<&store::auth::AuthUser>,
    selected: &str,
    search: &str,
) -> Result<Value, Failure> {
    let subject = user.map(|u| u.uid.as_str());
    let mut filter = vec![Predicate::KindEq("plain".into())];
    if !search.is_empty() {
        filter.push(Predicate::TextContains(search.into()));
    }
    let mut rows = read(state, subject, filter, 501).await?;
    rows.retain(|row| row["slug"] != crate::settings::SLUG);
    let truncated = rows.len() > 500;
    rows.truncate(500);
    for row in &mut rows {
        row.as_object_mut().expect("Record object").remove("body");
    }
    let record = if selected.is_empty() {
        None
    } else {
        read(state, subject, vec![Predicate::UidEq(selected.into())], 1)
            .await?
            .into_iter()
            .next()
    };
    let mut threads = Vec::new();
    let mut messages = Vec::new();
    let mut assertions = Vec::new();
    if record.is_some() {
        threads = read(
            state,
            subject,
            vec![
                Predicate::KindEq("thread".into()),
                relation("thread-of", selected),
            ],
            50,
        )
        .await?;
        if !threads.is_empty() {
            let links = threads
                .iter()
                .filter_map(|row| row["uid"].as_str())
                .map(|uid| relation("message-in", uid))
                .collect();
            messages = read(
                state,
                subject,
                vec![Predicate::KindEq("message".into()), Predicate::Any(links)],
                200,
            )
            .await?;
            messages.reverse();
            let ids = messages
                .iter()
                .filter_map(|r| r["uid"].as_str().map(String::from))
                .collect::<Vec<_>>();
            let links = store::assertions::for_subjects(&state.cell.store.pool, &ids)
                .await
                .map_err(internal)?;
            for message in &mut messages {
                if let Some(link) = links.iter().find(|link| {
                    link.predicate == "message-in"
                        && message["uid"] == link.subject_uid
                        && threads
                            .iter()
                            .any(|thread| thread["uid"].as_str() == link.object_uid.as_deref())
                }) {
                    message["thread_uid"] = json!(link.object_uid);
                }
            }
        }
        for assertion in store::assertions::for_subjects(&state.cell.store.pool, &[selected.into()])
            .await
            .map_err(internal)?
        {
            if let Some(object) = &assertion.object_uid
                && !visible(state, subject, object).await?
            {
                continue;
            }
            assertions
                .push(json!({"predicate": assertion.predicate, "object": assertion.object_uid}));
        }
    }
    let permits = |key: &str| permits(user, key);
    let editable = record.as_ref().is_some_and(|r| r["kind"] == "plain");
    let mut snapshot = json!({"records": rows, "record": record.unwrap_or(json!({})), "threads": threads, "messages": messages,
        "assertions": assertions, "canedit": editable && permits("record:update"), "cancomment": editable && permits("record:create"),
        "selected": selected, "truncated": truncated, "signedin": user.is_some(), "ready": true,
        "cancreate":permits("record:create"),"canmove":permits("record:update"),"cancarddelete":permits("record:delete"),"candelete":editable && permits("record:delete"),
        "name": user.map_or("", |u| if u.name.trim().is_empty() {u.username.as_str()} else {u.name.as_str()})});
    snapshot.as_object_mut().unwrap().extend(
        crate::settings::snapshot(state, user)
            .await?
            .as_object()
            .unwrap()
            .clone(),
    );
    snapshot.as_object_mut().unwrap().extend(
        crate::users::snapshot(state, user)
            .await?
            .as_object()
            .unwrap()
            .clone(),
    );
    Ok(snapshot)
}
