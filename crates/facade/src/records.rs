use engine::actions::Action;
use protein::{Predicate, Protein};
use serde_json::{Value, json};

use crate::{
    State,
    auth::{Failure, internal},
};

pub(crate) fn permits(user: Option<&store::auth::AuthUser>, permission: &str) -> bool {
    user.is_none_or(|u| u.role == "admin" || u.permissions.iter().any(|p| p == permission))
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

pub(crate) async fn allow(
    state: &State,
    user: Option<&store::auth::AuthUser>,
    selected: &str,
    action: &Action,
) -> Result<(), Failure> {
    let subject = user.map(|u| u.uid.as_str());
    let denied = || {
        (
            axum::http::StatusCode::FORBIDDEN,
            "This change is not available here.".into(),
        )
    };
    let management = match action {
        Action::SetRoleReadRules { role, rules, .. } => {
            if !crate::settings::administrator(user) || role == "admin" || rules.validate().is_err()
            {
                return Err(denied());
            }
            Some(("role:update", Some(role.as_str())))
        }
        Action::CreateUser {
            role,
            username,
            name,
            password,
        } => {
            if username.trim().is_empty()
                || username.len() > 256
                || name.len() > 500
                || password.is_empty()
                || password.len() > 1024
            {
                return Err(denied());
            }
            if !permits(user, "user:assign_role") {
                return Err(denied());
            }
            Some(("user:create", Some(role.as_str())))
        }
        Action::CreateRole { name } if !name.trim().is_empty() && name.len() <= 100 => {
            Some(("role:create", None))
        }
        Action::AssignRole { user: target, role } => {
            let old = store::auth::user_by_uid(&state.cell.store.pool, target)
                .await
                .map_err(internal)?
                .ok_or_else(denied)?;
            if old.permissions.iter().any(|key| !permits(user, key)) {
                return Err(denied());
            }
            if old.role == "admin" {
                let mut other_active = false;
                for admin in store::auth::admins(&state.cell.store.pool)
                    .await
                    .map_err(internal)?
                {
                    if admin != *target
                        && store::people::is_active(&state.cell.store.pool, &admin)
                            .await
                            .map_err(internal)?
                    {
                        other_active = true;
                    }
                }
                if user.is_none_or(|u| u.role != "admin" || u.uid == *target) || !other_active {
                    return Err(denied());
                }
            }
            Some(("user:assign_role", Some(role.as_str())))
        }
        Action::GrantPermission { role, permission }
        | Action::RevokePermission { role, permission } => {
            if role == "admin"
                || !permits(user, permission)
                || !utils::auth::all_permission_keys().contains(permission)
            {
                return Err(denied());
            }
            Some(("permission:assign", None))
        }
        Action::SetPersonReadFilter { person, .. } => {
            let target = store::auth::user_by_uid(&state.cell.store.pool, person)
                .await
                .map_err(internal)?
                .ok_or_else(denied)?;
            if target.role == "admin" || target.permissions.iter().any(|key| !permits(user, key)) {
                return Err(denied());
            }
            Some(("user:update", None))
        }
        _ => None,
    };
    if let Some((permission, role)) = management {
        let Some(user) = user else {
            return Err(denied());
        };
        if !permits(Some(user), permission) {
            return Err(denied());
        }
        if let Some(role) = role {
            let keys = store::auth::role_permission_keys(&state.cell.store.pool, role)
                .await
                .map_err(internal)?;
            if (role == "admin" && user.role != "admin")
                || keys.iter().any(|key| !permits(Some(user), key))
            {
                return Err(denied());
            }
        }
        return Ok(());
    }
    if let Action::CreateRecord {
        kind: nucleus::RecordKind::Plain,
        slug: None,
        head,
        body,
        ..
    } = action
    {
        return if permits(user, "record:create")
            && !head.trim().is_empty()
            && head.len() <= 500
            && body.len() <= 40000
        {
            Ok(())
        } else {
            Err(denied())
        };
    }
    if let Action::CreateRecordWithTags {
        head,
        body,
        quantity,
        tags,
    } = action
    {
        return if permits(user, "record:create")
            && !head.trim().is_empty()
            && head.len() <= 500
            && body.len() <= 40000
            && quantity.is_finite()
            && tags.len() <= 40
        {
            Ok(())
        } else {
            Err(denied())
        };
    }
    if let Action::SetExtension {
        target,
        namespace,
        fds,
    } = action
    {
        crate::settings::validate(namespace, fds)?;
        if namespace == crate::settings::CUSTOM && !crate::settings::administrator(user) {
            return Err(denied());
        }
        let facade = store::records::resolve(&state.cell.store.pool, crate::settings::SLUG)
            .await
            .map_err(internal)?;
        let is_facade = facade.is_some_and(|r| r.uid == *target);
        if !permits(user, "record:update")
            || (!(is_facade && crate::settings::administrator(user))
                && !visible(state, subject, target).await?)
        {
            return Err(denied());
        }
        if (is_facade && !permits(user, "configuration:update"))
            || (!is_facade && (namespace != crate::settings::COLUMNS || target != selected))
        {
            return Err(denied());
        }
        return Ok(());
    }
    if let Action::DeleteRecord { target } | Action::SetQuantity { target, .. } = action {
        let rows = read(
            state,
            subject,
            vec![
                Predicate::UidEq(target.clone()),
                Predicate::KindEq("plain".into()),
            ],
            1,
        )
        .await?;
        let permission = if matches!(action, Action::DeleteRecord { .. }) {
            "record:delete"
        } else {
            "record:update"
        };
        return if permits(user, permission)
            && rows
                .first()
                .is_some_and(|row| row["slug"] != crate::settings::SLUG)
        {
            Ok(())
        } else {
            Err(denied())
        };
    }
    let rows = read(
        state,
        subject,
        vec![
            Predicate::UidEq(selected.into()),
            Predicate::KindEq("plain".into()),
        ],
        1,
    )
    .await?;
    if rows.is_empty() {
        return Err(denied());
    }
    let (target, permission) = match action {
        Action::EditRecordText { target, .. }
        | Action::SetQuantity { target, .. }
        | Action::SetQuantityExact { target, .. }
        | Action::SetSlug { target, .. }
        | Action::SetUnit { target, .. } => (target.as_str(), "record:update"),
        Action::SetIdentity { subject, .. }
        | Action::AssertRecord { subject, .. }
        | Action::RetractRecord { subject, .. } => (subject.as_str(), "record:update"),
        Action::CreateThread { target, .. } => (target.as_str(), "record:create"),
        Action::CreateMessage {
            thread,
            author: None,
            parent: None,
            references,
            state: nucleus::MessageState::Finished,
            ..
        } if references.is_empty() => {
            let threads = read(
                state,
                subject,
                vec![
                    Predicate::UidEq(thread.clone()),
                    Predicate::KindEq("thread".into()),
                    relation("thread-of", selected),
                ],
                1,
            )
            .await?;
            if threads.is_empty() {
                return Err(denied());
            }
            (selected, "record:create")
        }
        _ => return Err(denied()),
    };
    if target != selected || !permits(user, permission) {
        return Err(denied());
    }
    if let Action::AssertRecord {
        object: Some(object),
        ..
    } = action
    {
        let resolved = store::records::resolve(&state.cell.store.pool, object)
            .await
            .map_err(internal)?
            .ok_or_else(denied)?;
        if !visible(state, subject, &resolved.uid).await? {
            return Err(denied());
        }
    }
    Ok(())
}
