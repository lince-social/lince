use super::*;

impl Engine {
    pub(super) fn dispatch_part_1(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        _verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::SetExtension {
                    target,
                    namespace,
                    fds,
                } => {
                    if namespace == "lince.command"
                        && !crate::commands::COMMAND_AUTHORING
                            .try_with(|allowed| *allowed)
                            .unwrap_or(false)
                    {
                        return Err(EngineError::Forbidden(
                            "Use command authoring to change executable configuration".into(),
                        ));
                    }
                    if namespace.starts_with("lince.social.") {
                        return Err(EngineError::Forbidden("Use social controls to change publication or private conversation state".into()));
                    }
                    if namespace == crate::groups::NAMESPACE
                        || namespace == crate::groups::ADMISSION_NAMESPACE
                    {
                        return Err(EngineError::Forbidden(
                            "Use the group membership controls".into(),
                        ));
                    }
                    if namespace == "lince.file_sync" && actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can configure directory sync".into(),
                        ));
                    }
                    let uid = self.resolve(&target).await?;
                    if namespace == "lince.message" {
                        return Err(EngineError::Forbidden(
                            "Use message actions to change authorship or lifecycle.".into(),
                        ));
                    }
                    let mut expected_content = None;
                    if namespace == "lince.message-content" {
                        let metadata =
                            store::records::get_extension(&self.store.pool, &uid, "lince.message")
                                .await?
                                .ok_or_else(|| {
                                    EngineError::Consequence(
                                        "Content must belong to a message.".into(),
                                    )
                                })?;
                        let (_, operator) = self.message_authorship(None, actor.as_deref()).await?;
                        if metadata["state"] != "writing" {
                            let before =
                                store::records::get_extension(&self.store.pool, &uid, &namespace)
                                    .await?
                                    .unwrap_or_default();
                            let old: Vec<nucleus::message::StoredPart> =
                                serde_json::from_value(before["parts"].clone())
                                    .map_err(|error| EngineError::Consequence(error.to_string()))?;
                            let new: Vec<nucleus::message::StoredPart> =
                                serde_json::from_value(fds["parts"].clone())
                                    .map_err(|error| EngineError::Consequence(error.to_string()))?;
                            if old == new {
                                return Err(EngineError::Consequence(
                                    "These message contents have already been saved.".into(),
                                ));
                            }
                            if old.len() != new.len() {
                                return Err(EngineError::Consequence(
                                    "Only existing steps and questions can be edited here.".into(),
                                ));
                            }
                            for (old, new) in old.iter().zip(&new) {
                                match (old, new) {
                                _ if old == new => {},
                                (nucleus::message::StoredPart::Steps { .. }, nucleus::message::StoredPart::Steps { steps }) => {
                                    if metadata["operator"] != operator || metadata["author"] != operator { return Err(EngineError::Forbidden("Only the author can edit their step list.".into())); }
                                    nucleus::operation::validate_steps(steps).map_err(EngineError::Consequence)?;
                                }
                                (nucleus::message::StoredPart::Question { question: old }, nucleus::message::StoredPart::Question { question: new }) => old.response(new, &operator, metadata["operator"].as_str().unwrap_or_default(), now.timestamp_millis().max(0) as u64).map_err(EngineError::Forbidden)?,
                                _ => return Err(EngineError::Consequence("Message attachments cannot be changed through form or step controls.".into())),
                            }
                            }
                            expected_content = Some(before);
                        } else if metadata["operator"] != operator {
                            return Err(EngineError::Forbidden(
                                "Only the message operator may change its contents.".into(),
                            ));
                        }
                    }
                    if namespace == "lince.fiote" {
                        return Err(EngineError::Consequence("Use configure-fiote to change prompt ancestry and assignment behavior.".into()));
                    }
                    if namespace == "work" {
                        return self
                            .change_record(
                                crate::record_change::Request {
                                    id: nucleus::new_uid("op"),
                                    record_uid: uid,
                                    mutation: crate::record_change::Mutation::WorkMetadata {
                                        value: fds,
                                    },
                                },
                                actor.as_deref(),
                            )
                            .await
                            .map(ControlFlow::Break);
                    }
                    if let Some(expected) = expected_content {
                        store::message_content::replace(&self.store.pool, &uid, &expected, &fds)
                            .await?;
                    } else {
                        store::records::set_extension(&self.store.pool, &uid, &namespace, &fds)
                            .await?;
                    }
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "extension": namespace }),
                            now,
                        )
                        .await?;
                }
                Action::RenameOrganContact { target, name } => {
                    let uid = self.resolve(&target).await?;
                    let name = name.trim();
                    if name.is_empty() {
                        return Err(EngineError::Consequence(
                            "give this contact a name you will recognise".into(),
                        ));
                    }
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ is renamed like any record"
                                .into(),
                        ));
                    }
                    store::organs::rename_contact(&self.store.pool, &uid, name).await?;
                }
                Action::SetSyncPolicy {
                    target,
                    sync_out,
                    sync_in,
                } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — there is no feed to open with this Cell's own Organ"
                                .into(),
                        ));
                    }
                    store::organs::set_sync_policy(&self.store.pool, &uid, sync_out, sync_in)
                        .await?;
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "sync_out": sync_out, "sync_in": sync_in }),
                            now,
                        )
                        .await?;
                }
                Action::SetContactAcceptScope { target, fields } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ sends us nothing to accept"
                                .into(),
                        ));
                    }
                    validate_scope(fields.as_deref())?;
                    store::organs::set_contact_accept_scope(
                        &self.store.pool,
                        &uid,
                        fields.as_deref(),
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "accept_fields": fields }),
                            now,
                        )
                        .await?;
                }
                Action::DeleteConversation { conversation } => {
                    let uid = self.resolve(&conversation).await?;
                    let root = store::replica::root_of(&self.store.pool, &uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence("that is not part of a conversation".into())
                        })?;
                    self.social_require_safe_generic_deletion(&root).await?;
                    let removed =
                        store::replica::delete_root_locally(&self.store.pool, &root).await?;
                    outcome.data = Some(serde_json::json!({ "removed": removed }));
                }
                Action::SendRecordCopy { thread, record } => {
                    let thread_uid = self.resolve(&thread).await?;
                    let source_uid = self.resolve(&record).await?;
                    let root = store::replica::root_of(&self.store.pool, &thread_uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(
                                "that thread is not inside a conversation".into(),
                            )
                        })?;
                    let source = store::records::get(&self.store.pool, &source_uid)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no such record to copy".into()))?;
                    if let Some(existing) =
                        store::replica::root_of(&self.store.pool, &source_uid).await?
                    {
                        if existing != root {
                            return Err(EngineError::Consequence(
                                "that record belongs to another conversation and cannot be copied \
                             into this one"
                                    .into(),
                            ));
                        }
                    }
                    let copy = store::records::create_in_root(
                        &self.store.pool,
                        store::records::NewRecord {
                            slug: None,
                            kind: nucleus::RecordKind::parse(&source.kind)
                                .unwrap_or(nucleus::RecordKind::Plain),
                            head: &source.head,
                            body: &source.body,
                            quantity: store::exact::zero(),
                        },
                        Some(&root),
                    )
                    .await?;
                    self.link_in(&copy.uid, &thread_uid, crate::threads::MESSAGE_IN_PREDICATE)
                        .await?;
                    outcome.created = Some(copy.uid.clone());
                    outcome.facts = self
                        .annotate(
                            copy.uid,
                            actor,
                            serde_json::json!({ "copied_from": source_uid }),
                            now,
                        )
                        .await?;
                }
                Action::HideRecordFromContact {
                    target,
                    record,
                    hidden,
                } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ has no feed to hide from".into(),
                        ));
                    }
                    let record_uid = self.resolve(&record).await?;
                    if store::records::get(&self.store.pool, &record_uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence("no such record to hide".into()));
                    }
                    store::visibility::set_hidden_from_organ(
                        &self.store.pool,
                        &uid,
                        &record_uid,
                        hidden,
                    )
                    .await?;
                    if !hidden {
                        store::sync_ops::enqueue_record_for_contact(
                            &self.store.pool,
                            &uid,
                            &record_uid,
                        )
                        .await?;
                    }
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "hidden_record": record_uid, "hidden": hidden }),
                            now,
                        )
                        .await?;
                }
                Action::SetContactScope { target, fields } => {
                    let uid = self.resolve(&target).await?;
                    let Some(contact) = store::organs::contact(&self.store.pool, &uid).await?
                    else {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ has no scope to narrow".into(),
                        ));
                    };
                    let before = contact.scope_fields;
                    validate_scope(fields.as_deref())?;
                    store::organs::set_contact_scope(&self.store.pool, &uid, fields.as_deref())
                        .await?;
                    if widens_scope(before.as_deref(), fields.as_deref()) {
                        store::sync_ops::enqueue_widened_for_contact(
                            &self.store.pool,
                            &uid,
                            before.as_deref(),
                            fields.as_deref(),
                        )
                        .await?;
                    }
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "scope_fields": fields }),
                            now,
                        )
                        .await?;
                }
                Action::SetContactShare { target, protein } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ shares with nobody".into(),
                        ));
                    }
                    let raw = match &protein {
                        Some(value) => {
                            let raw = value.to_string();
                            let Some(parsed) = crate::share::parse(&raw) else {
                                return Err(EngineError::Consequence(
                                    "this selection is not a Protein query".into(),
                                ));
                            };
                            protein::matching_records(&self.store, &parsed, None).await?;
                            Some(raw)
                        }
                        None => None,
                    };
                    store::organs::set_contact_share_protein(
                        &self.store.pool,
                        &uid,
                        raw.as_deref(),
                    )
                    .await?;
                    if let Some(contact) = store::organs::contact(&self.store.pool, &uid).await? {
                        crate::share::reconcile_contact(self, &contact).await?;
                    }
                    outcome.facts = self
                        .annotate(uid, actor, serde_json::json!({ "share": protein }), now)
                        .await?;
                }
                Action::MoveRecordTo { record, target } => {
                    let record_uid = self.resolve(&record).await?;
                    let contact_uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &contact_uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "a Record can only be handed to a contact".into(),
                        ));
                    }
                    if store::records::get(&self.store.pool, &record_uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence("no such Record".into()));
                    }
                    if let Some(existing) =
                        store::record_move::of_record(&self.store.pool, &record_uid).await?
                    {
                        if existing.contact_organ != contact_uid {
                            return Err(EngineError::Consequence(
                                "this Record is already on its way to somebody else — cancel that \
                             move first"
                                    .into(),
                            ));
                        }
                    }
                    store::record_move::begin(&self.store.pool, &record_uid, &contact_uid).await?;
                    store::sync_ops::enqueue_record_for_contact(
                        &self.store.pool,
                        &contact_uid,
                        &record_uid,
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            record_uid,
                            actor,
                            serde_json::json!({ "moving_to": contact_uid }),
                            now,
                        )
                        .await?;
                }
                Action::CancelRecordMove { record } => {
                    let record_uid = self.resolve(&record).await?;
                    if let Some(moving) =
                        store::record_move::of_record(&self.store.pool, &record_uid).await?
                    {
                        store::offers::refuse(
                            &self.store.pool,
                            store::offers::OfferKind::RecordMove,
                            &record_uid,
                            &moving.contact_organ,
                        )
                        .await?;
                    }
                    store::record_move::forget(&self.store.pool, &record_uid).await?;
                    outcome.facts = self
                        .annotate(
                            record_uid,
                            actor,
                            serde_json::json!({ "moving_to": null }),
                            now,
                        )
                        .await?;
                }
                Action::ForgetOrganContact { target } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ cannot be forgotten".into(),
                        ));
                    }
                    store::organs::forget_contact(&self.store.pool, &uid).await?;
                }
                Action::AddKnownOrgan { invite, name } => {
                    let invite = crate::pairing::PairingInvite::decode(&invite)?;
                    let name = name.trim();
                    if name.is_empty() {
                        return Err(EngineError::Consequence(
                            "give this contact a name you will recognise".into(),
                        ));
                    }
                    let existing =
                        store::organs::contact_by_node_id(&self.store.pool, &invite.node_id)
                            .await?;
                    if let Some(contact) = &existing {
                        if contact.trust == "blocked" {
                            return Err(EngineError::Consequence(
                                "this Organ is blocked. Unblock them first if that is what you \
                             meant — adding by code must not undo a block."
                                    .into(),
                            ));
                        }
                    }
                    let organ_uid = match &existing {
                        Some(contact) => contact.record_uid.clone(),
                        None => {
                            let organ_uid = format!("o-{}", &invite.node_id);
                            store::organs::add_contact(
                                &self.store.pool,
                                &organ_uid,
                                None,
                                name,
                                "",
                                1,
                            )
                            .await?;
                            store::organs::set_node_id(
                                &self.store.pool,
                                &organ_uid,
                                Some(&invite.node_id),
                            )
                            .await?;
                            organ_uid
                        }
                    };
                    if existing.is_some() {
                        store::organs::rename_contact(&self.store.pool, &organ_uid, name).await?;
                    }
                    if let Some(root_key) = &invite.root_key {
                        crate::trust::adopt_key(
                            &self.store,
                            &organ_uid,
                            crate::roster::ROOT_KEY_ID,
                            root_key,
                        )
                        .await?;
                    } else {
                        outcome.warnings.push(
                            "this code carried no identity key, so future key changes cannot be \
                         verified against it. Prefer a code that includes one."
                                .into(),
                        );
                    }
                    store::organs::set_trust(&self.store.pool, &organ_uid, "known").await?;
                    if existing.is_none() {
                        store::organs::set_pending_introduction(&self.store.pool, &organ_uid, true)
                            .await?;
                        outcome.warnings.push(
                            "added — but they are not reachable for sync until this Cell has \
                         connected to them once and learned their identity."
                                .into(),
                        );
                    }
                    outcome.created = Some(organ_uid);
                }
                Action::ProposeGroup {
                    thread,
                    title,
                    organs,
                } => {
                    let group = self
                        .propose_group(&thread, &title, &organs, actor.as_deref())
                        .await?;
                    outcome.created = Some(group.membership.root);
                }
                Action::SetGroupPerson {
                    root,
                    person,
                    allowed,
                } => {
                    self.set_group_person(&root, &person, allowed, actor.as_deref())
                        .await?;
                }
                Action::RemoveGroupOrgan { root, organ } => {
                    self.remove_group_organ(&root, &organ, actor.as_deref())
                        .await?;
                }
                Action::StartConversation { contact, title } => {
                    let contact_uid = self.resolve(&contact).await?;
                    let (conversation, thread) =
                        self.start_conversation(&contact_uid, title.trim()).await?;
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    contact_uid,
                                    store::exact::zero(),
                                    Cause {
                                        kind: CauseKind::Sync,
                                        uid: Some(conversation.clone()),
                                    },
                                )
                            },
                            now,
                        )
                        .await?;
                    outcome.created = Some(
                        serde_json::json!({
                            "conversation": conversation,
                            "thread": thread,
                        })
                        .to_string(),
                    );
                }
                Action::OpenThread {
                    conversation,
                    title,
                } => {
                    let conversation_uid = self.resolve(&conversation).await?;
                    outcome.created =
                        Some(self.open_thread(&conversation_uid, title.trim()).await?);
                }
                Action::GrantOrganLogin { organ, person_name } => {
                    let organ_uid = self.resolve(&organ).await?;
                    let contact = store::organs::contact(&self.store.pool, &organ_uid)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("not a contact".into()))?;
                    if contact.trust != "known" {
                        return Err(EngineError::Consequence(
                            "only a known contact may be given a login".into(),
                        ));
                    }
                    let person_name = person_name.trim();
                    if person_name.is_empty() {
                        return Err(EngineError::Consequence(
                            "give the Person a name you will recognise".into(),
                        ));
                    }
                    let person = store::records::create(
                        &self.store.pool,
                        store::records::NewRecord {
                            slug: None,
                            kind: nucleus::RecordKind::Person,
                            head: person_name,
                            body: "",
                            quantity: store::exact::zero(),
                        },
                    )
                    .await?;
                    store::logins::grant(&self.store.pool, &organ_uid, &person.uid).await?;
                    outcome.warnings.push(
                    "Assign this Person a role and grant visibility to the records they may use."
                        .into(),
                );
                    outcome.created = Some(person.uid);
                }
                Action::RevokeOrganLogin { organ } => {
                    let organ_uid = self.resolve(&organ).await?;
                    store::logins::revoke(&self.store.pool, &organ_uid).await?;
                }
                Action::AcceptThreadInvite { invite } => {
                    let root = self.accept_invite(&invite).await?;
                    outcome.created = Some(root);
                }
                Action::DeclineThreadInvite { invite } => {
                    self.decline_invite(&invite).await?;
                }
                Action::ShareMyKey { thread } => {
                    let thread_uid = self.resolve(&thread).await?;
                    let local = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
                    let invite = store::records::get_extension(
                        &self.store.pool,
                        &local.uid,
                        "lince.pairing",
                    )
                    .await?
                    .and_then(|fields| {
                        fields
                            .get("invite")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
                    .ok_or_else(|| {
                        EngineError::Consequence(
                            "this Cell has no pairing code yet — it needs a network endpoint"
                                .into(),
                        )
                    })?;
                    let uid = self.send_message(&thread_uid, &local.head, &invite).await?;
                    outcome.created = Some(uid);
                }
                Action::RosterCreateOrgan => {
                    let roster = self.create_organ_identity().await?;
                    outcome.data = Some(serde_json::json!({"organ_uid": roster.roster.organ_uid}));
                    outcome
                        .warnings
                        .push("Organ created. You can now enrol your other devices.".into());
                }
                Action::RosterSetKarmaExecution {
                    cell_uid,
                    enabled,
                    additional,
                    expected_roster_version,
                } => {
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Forbidden(
                            "The Organ root key is needed to change device execution permission"
                                .into(),
                        )
                    })?;
                    let roster = self
                        .choose_karma_executor(
                            &root,
                            &cell_uid,
                            enabled,
                            additional,
                            expected_roster_version,
                        )
                        .await?;
                    outcome.data = Some(
                        serde_json::json!({"version":roster.roster.version,"karma":self.karma_device_execution().await?}),
                    );
                }
                Action::RosterRenameCell { cell_uid, label } => {
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "Manage device names on the Cell holding the Organ root key".into(),
                        )
                    })?;
                    self.rename_roster_cell(&root, &cell_uid, &label).await?;
                }
                Action::RosterEnrolToken => {
                    if self.root_signer().await?.is_none() {
                        return Err(EngineError::Consequence(
                            "the root key is not on this Cell — bring it back to enrol a device"
                                .into(),
                        ));
                    }
                    let token = self.issue_enrolment_token().await?;
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
                    let root_key =
                        crate::trust::key_of(&self.store, &organ.uid, crate::roster::ROOT_KEY_ID)
                            .await?
                            .unwrap_or_default();
                    let pairing =
                        self.current_pairing_invite()
                            .await?
                            .or(store::records::get_extension(
                                &self.store.pool,
                                &organ.uid,
                                "lince.pairing",
                            )
                            .await?
                            .and_then(|fields| {
                                fields
                                    .get("invite")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string)
                            })
                            .and_then(|encoded| {
                                crate::pairing::PairingInvite::decode(&encoded).ok()
                            }));
                    match pairing {
                        Some(pairing) => {
                            let invite = crate::pairing::EnrolmentInvite {
                                node_id: pairing.node_id,
                                organ_uid: organ.uid.clone(),
                                root_key,
                                token: token.clone(),
                                addrs: pairing.addrs,
                            };
                            outcome.created = Some(invite.encode());
                            outcome.data = Some(serde_json::json!({
                                "code": invite.encode(),
                                "qr_svg": invite.qr_svg().unwrap_or_default(),
                                "expires_in_minutes": crate::roster::ENROLMENT_TOKEN_TTL_MINUTES,
                            }));
                        }
                        None => {
                            return Err(EngineError::Consequence(
                                "this Cell has no network identity yet, so a device cannot be \
                             told where to reach it. Wait for the endpoint to bind and try \
                             again."
                                    .into(),
                            ));
                        }
                    }
                }
                Action::MailboxSavedStatus => {
                    outcome.data = Some(
                        serde_json::json!({"saved_mail":store::mailbox::delivery::inbox_status(&self.store.pool).await?}),
                    );
                }
                Action::MailboxSetCopies { copies } => {
                    if !(1..=2).contains(&copies) {
                        return Err(EngineError::Consequence(
                            "Choose one or two mailbox copies".into(),
                        ));
                    }
                    let mut config = store::cells::config(&self.store.pool, "lince.social")
                        .await?
                        .filter(serde_json::Value::is_object)
                        .unwrap_or_else(|| serde_json::json!({}));
                    config["mailbox_copies"] = copies.into();
                    store::cells::set_config(&self.store.pool, "lince.social", &config).await?;
                    outcome.data = Some(
                        serde_json::json!({"mailbox_copies":copies,"applies_to":"new outgoing envelopes"}),
                    );
                }
                Action::MailboxRetrySaved { uid } => {
                    if !store::mailbox::delivery::retry(&self.store.pool, &uid).await? {
                        return Err(EngineError::Consequence(
                            "There is no pending saved message to recover".into(),
                        ));
                    }
                    let imported = self.process_recovered_mail().await?;
                    outcome.data = Some(
                        serde_json::json!({"imported":imported,"saved_mail":store::mailbox::delivery::inbox_status(&self.store.pool).await?}),
                    );
                }
                Action::MailboxStatus => {
                    let swept = self.sweep_mailbox().await?;
                    let mut carrying = Vec::new();
                    for registration in store::mailbox::registrations(&self.store.pool).await? {
                        let held =
                            store::mailbox::carried_for(&self.store.pool, &registration.organ_uid)
                                .await?;
                        let known_as =
                            store::organs::contact(&self.store.pool, &registration.organ_uid)
                                .await?
                                .map(|contact| contact.head)
                                .filter(|name| !name.is_empty())
                                .unwrap_or_else(|| registration.label.clone());
                        carrying.push(serde_json::json!({
                            "organ_uid": registration.organ_uid,
                            "known_as": known_as,
                            "quota_bytes": registration.quota_bytes,
                            "held_bundles": held.bundles,
                            "held_bytes": held.bytes,
                            "registered_at": registration.registered_at,
                        }));
                    }
                    let pending = store::mailbox::pending_notices(&self.store.pool).await?;
                    outcome.data = Some(serde_json::json!({
                        "carrying_for": carrying,
                        "swept_just_now": swept,
                        "expiry_notices_pending": pending.len(),
                        "retention_days": crate::seal::RETENTION_DAYS,
                        "max_bundle_bytes": crate::mailbox::MAX_BUNDLE_BYTES,
                    }));
                }
                Action::MailboxCarryFor {
                    organ_uid,
                    label,
                    quota_bytes,
                } => {
                    let root_key =
                        crate::trust::key_of(&self.store, &organ_uid, crate::roster::ROOT_KEY_ID)
                            .await?
                            .ok_or_else(|| {
                                EngineError::Consequence(
                        "no root key is held for that Organ: pair with them before offering to \
                         carry their mail"
                            .into(),
                    )
                            })?;
                    let quota = if quota_bytes > 0 {
                        quota_bytes
                    } else {
                        crate::mailbox::DEFAULT_QUOTA_BYTES
                    };
                    store::mailbox::register(
                        &self.store.pool,
                        &organ_uid,
                        &root_key,
                        &label,
                        quota,
                    )
                    .await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "quota_bytes": quota,
                    }));
                }
                Action::MailboxStopCarrying { organ_uid } => {
                    let held = store::mailbox::carried_for(&self.store.pool, &organ_uid).await?;
                    store::mailbox::deregister(&self.store.pool, &organ_uid).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "discarded_bundles": held.bundles,
                    }));
                }
                Action::MailboxPickupPoints => {
                    let points = self.own_pickup_points().await?;
                    let mut published = Vec::new();
                    for point in &points {
                        let probe = self.carrier_probe(&point.node_id).await;
                        let known_as = store::organs::contact(&self.store.pool, &point.organ_uid)
                            .await?
                            .map(|contact| contact.head)
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| point.label.clone());
                        let (state, waiting) = match &probe {
                            crate::wire::CarrierProbe::Carrying(waiting) => (
                                "carrying",
                                serde_json::json!({
                                    "bundles": waiting.bundles,
                                    "bytes": waiting.bytes,
                                    "oldest_expires_at": waiting.oldest_expires_at,
                                }),
                            ),
                            crate::wire::CarrierProbe::Refused => {
                                ("refused", serde_json::Value::Null)
                            }
                            crate::wire::CarrierProbe::Unreachable => {
                                ("unreachable", serde_json::Value::Null)
                            }
                        };
                        published.push(serde_json::json!({
                            "organ_uid": point.organ_uid,
                            "node_id": point.node_id,
                            "label": point.label,
                            "known_as": known_as,
                            "state": state,
                            "waiting": waiting,
                        }));
                    }
                    let candidates: Vec<serde_json::Value> =
                        store::organs::contacts(&self.store.pool)
                            .await?
                            .into_iter()
                            .filter(|contact| contact.trust == "known")
                            .filter(|contact| {
                                !points
                                    .iter()
                                    .any(|point| point.organ_uid == contact.record_uid)
                            })
                            .map(|contact| {
                                serde_json::json!({
                                    "organ_uid": contact.record_uid,
                                    "known_as": contact.head,
                                    "node_id": contact.node_id,
                                })
                            })
                            .collect();
                    outcome.data = Some(serde_json::json!({
                        "pickup": published,
                        "candidates": candidates,
                        "retention_days": crate::seal::RETENTION_DAYS,
                        "may_change": self.root_signer().await?.is_some(),
                    }));
                }
                Action::MailboxAddPickup {
                    organ_uid,
                    node_id,
                    label,
                } => {
                    let contact = store::organs::contact(&self.store.pool, &organ_uid).await?;
                    let node_id = if node_id.is_empty() {
                        contact
                            .as_ref()
                            .and_then(|contact| contact.node_id.clone())
                            .filter(|id| !id.is_empty())
                            .ok_or_else(|| {
                                EngineError::Consequence(
                                    "we have no address for that contact, so there is nothing to \
                                 publish."
                                        .into(),
                                )
                            })?
                    } else {
                        node_id
                    };
                    match self.carrier_probe(&node_id).await {
                        crate::wire::CarrierProbe::Carrying(_) => {}
                        crate::wire::CarrierProbe::Refused => {
                            return Err(EngineError::Consequence(
                                "they are not carrying mail for you. They have to add you first, \
                             and asking them from here is not built yet — nothing was \
                             published."
                                    .into(),
                            ));
                        }
                        crate::wire::CarrierProbe::Unreachable => {
                            return Err(EngineError::Consequence(
                                "they did not answer just now, so we cannot tell whether they \
                             carry mail for you. Nothing was published — try again."
                                    .into(),
                            ));
                        }
                    }
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "publishing a pickup point re-signs the roster, which needs the root \
                         key this Cell does not currently hold."
                                .into(),
                        )
                    })?;
                    let label = if label.is_empty() {
                        contact
                            .map(|contact| contact.head)
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| organ_uid.clone())
                    } else {
                        label
                    };
                    let mut points = self.own_pickup_points().await?;
                    points.retain(|point| point.organ_uid != organ_uid);
                    points.push(crate::roster::PickupPoint {
                        organ_uid: organ_uid.clone(),
                        node_id,
                        label,
                    });
                    let published = points.len();
                    self.set_pickup_points(&root, points).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "pickup_points": published,
                        "single_point_of_failure": published < 2,
                    }));
                }
                Action::MailboxRemovePickup { organ_uid } => {
                    let mut points = self.own_pickup_points().await?;
                    let Some(going) = points
                        .iter()
                        .find(|point| point.organ_uid == organ_uid)
                        .cloned()
                    else {
                        return Err(EngineError::Consequence(
                            "that is not one of your pickup points".into(),
                        ));
                    };
                    let stranded = match self.carrier_probe(&going.node_id).await {
                        crate::wire::CarrierProbe::Carrying(waiting) => Some(waiting.bundles),
                        _ => None,
                    };
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "removing a pickup point re-signs the roster, which needs the root \
                         key this Cell does not currently hold."
                                .into(),
                        )
                    })?;
                    points.retain(|point| point.organ_uid != organ_uid);
                    self.set_pickup_points(&root, points).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "stranded_bundles": stranded,
                    }));
                }
                Action::MailboxCollectNow => {
                    let imported = self.collect_mail_now().await?;
                    outcome.data = Some(serde_json::json!({ "imported_ops": imported }));
                }
                Action::MailboxRequests => {
                    let pool = &self.store.pool;
                    let mut asks = Vec::new();
                    for row in store::mailbox::requests(pool).await? {
                        let known_as = store::organs::contact(pool, &row.organ_uid)
                            .await?
                            .and_then(|contact| contact.slug);
                        asks.push(serde_json::json!({
                            "organ_uid": row.organ_uid,
                            "known_as": known_as,
                            "claims_to_be": row.label,
                            "asked_at": row.asked_at,
                            "already_carried": store::mailbox::registration(pool, &row.organ_uid)
                                .await?
                                .is_some(),
                        }));
                    }
                    let invites: Vec<serde_json::Value> = store::mailbox::invites(pool)
                        .await?
                        .into_iter()
                        .map(|row| {
                            serde_json::json!({
                                "label": row.label,
                                "quota_bytes": row.quota_bytes,
                                "expires_at": row.expires_at,
                                "created_at": row.created_at,
                                "used_at": row.used_at,
                                "used_by": row.used_by,
                            })
                        })
                        .collect();
                    outcome.data = Some(serde_json::json!({
                        "requests": asks,
                        "invites": invites,
                        "invite_days": crate::mailbox::INVITE_TTL_DAYS,
                        "default_quota_bytes": crate::mailbox::DEFAULT_QUOTA_BYTES,
                    }));
                }
                _ => {
                    return Err(EngineError::Consequence(
                        "Invalid internal action dispatch".into(),
                    ));
                }
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
