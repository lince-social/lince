use super::*;

pub(crate) fn retry(world: &mut World, record: &RecordBinding) {
    if crate::native::laboratory::blocked(world, record.area) {
        return;
    }
    let Some(mut bindings) = world.get_resource_mut::<Bindings>() else {
        return;
    };
    let key = key(record);
    let principal = bindings.principals.get(&key.0).cloned();
    let Some(document) = bindings.documents.get_mut(&key) else {
        return;
    };
    if document.principal != principal {
        return;
    }
    document.failed = false;
    document.joined = false;
    document.joining = None;
    document.recovery_join = Some(format!("binding-rejoin-{}", nucleus::new_uid("op")));
    document.inflight = None;
    document.writable.clear();
    document.incoming.clear();
    document.cursors.clear();
    document.presence_value = None;
    document.status = "Reconnecting Record… Your draft is kept.".into();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, RecordBinding, String, Vec<String>) {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        world.init_resource::<Bindings>();
        let uid = nucleus::new_uid("r");
        let other = nucleus::new_uid("r");
        let binding = RecordBinding {
            area: world.spawn_empty().id(),
            uid,
            source: Source::Local,
        };
        assert!(!crate::native::laboratory::blocked(&world, binding.area));
        let key = key(&binding);
        let mut document = Document::new(Source::Local, Some("owner".into()));
        let before = document.doc.oplog_vv();
        document
            .doc
            .get_text("body")
            .insert(0, "My retained draft")
            .unwrap();
        document.queue(&binding.uid, &before).unwrap();
        document.failed = true;
        document.joined = true;
        document.writable = vec!["body".into()];
        document.inflight = Some((document.pending.front().unwrap().id.clone(), Instant::now()));
        let requests = document
            .pending
            .iter()
            .map(|request| request.id.clone())
            .collect();
        let mut bindings = world.resource_mut::<Bindings>();
        bindings.principals.insert(key.0.clone(), "owner".into());
        bindings.documents.insert(
            (key.0.clone(), other.clone()),
            Document::new(Source::Local, Some("owner".into())),
        );
        bindings.documents.insert(key, document);
        drop(bindings);
        (world, binding, other, requests)
    }

    #[test]
    fn explicit_record_retry_keeps_durable_requests_and_requires_fresh_write_authority() {
        let (mut world, record, other, requests) = fixture();
        retry(&mut world, &record);
        let bindings = world.resource::<Bindings>();
        let document = &bindings.documents[&key(&record)];
        assert!(!document.failed);
        assert!(!document.joined);
        assert!(document.joining.is_none());
        assert!(document.recovery_join.is_some());
        assert!(document.inflight.is_none());
        assert!(document.writable.is_empty());
        assert_eq!(
            document.doc.get_text("body").to_string(),
            "My retained draft"
        );
        assert_eq!(
            document
                .pending
                .iter()
                .map(|request| request.id.clone())
                .collect::<Vec<_>>(),
            requests
        );
        assert!(
            bindings.documents[&(key(&record).0, other)]
                .doc
                .get_text("body")
                .is_empty()
        );
    }

    #[test]
    fn record_retry_cannot_resume_a_previous_logins_pending_edits() {
        let (mut world, record, _, requests) = fixture();
        world
            .resource_mut::<Bindings>()
            .principals
            .insert(key(&record).0, "another-owner".into());
        retry(&mut world, &record);
        let document = &world.resource::<Bindings>().documents[&key(&record)];
        assert!(document.failed);
        assert_eq!(document.principal.as_deref(), Some("owner"));
        assert_eq!(
            document
                .pending
                .iter()
                .map(|request| request.id.clone())
                .collect::<Vec<_>>(),
            requests
        );
        assert_eq!(
            document.doc.get_text("body").to_string(),
            "My retained draft"
        );
    }

    #[test]
    fn a_record_retry_rejects_stale_join_authority_for_the_same_uid() {
        let (mut world, record, _, _) = fixture();
        retry(&mut world, &record);
        let current = world.resource::<Bindings>().documents[&key(&record)]
            .recovery_join
            .clone()
            .unwrap();
        let response = |id: String| ServerMessage::CollabState {
            id,
            record_uid: record.uid.clone(),
            snapshot_base64: String::new(),
            version: String::new(),
            writable: vec!["body".into()],
        };
        receive(
            &mut world,
            Source::Local,
            response(format!("binding-join-{}", record.uid)),
        );
        assert!(
            world.resource::<Bindings>().documents[&key(&record)]
                .incoming
                .is_empty()
        );
        receive(&mut world, Source::Local, response(current));
        assert_eq!(
            world.resource::<Bindings>().documents[&key(&record)]
                .incoming
                .len(),
            1
        );
        assert!(!world.resource::<Bindings>().documents[&key(&record)].joined);
        assert!(
            world.resource::<Bindings>().documents[&key(&record)]
                .writable
                .is_empty()
        );
    }
}
