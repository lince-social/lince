use super::*;
use nucleus::trust_analysis::{claims::Document, contacts::Audience};

#[derive(Component)]
struct Editor {
    owner: Entity,
    form: Entity,
    subject: String,
}

fn at(time: i64) -> String {
    chrono::DateTime::from_timestamp(time, 0).map(|time| time.to_rfc3339()).unwrap_or_else(|| "unavailable".into())
}

fn caption(document: &Document) -> String {
    let statement = &document.statement;
    format!(
        "{} · {:?} {} · {:?} · revision {} · as of {} · expires {}{}",
        statement.title,
        statement.comparator,
        statement.amount,
        statement.descriptor.metric,
        statement.revision,
        at(statement.as_of),
        at(statement.expires_at),
        if statement.withdrawn {
            " · withdrawn"
        } else {
            ""
        }
    )
}

pub(super) fn editor(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    save: Entity,
    subject: &str,
) {
    let body = panel::column(world, parent);
    world.entity_mut(body).insert(Editor {
        owner,
        form: save,
        subject: subject.into(),
    });
    render_editor(world, body);
    form(
        world,
        owner,
        parent,
        "Choose retained public claim snapshots",
        json!({"action":"trust-analysis","request_id":nucleus::new_uid("request"),"request":{"kind":"claims","request":{"kind":"list","subject":{"kind":"organ","uid":subject},"before":null,"as_organ":false}}}),
        vec![],
        None,
    );
    label(
        world,
        parent,
        "Choose signed public claims about this Organ. Selection only changes this draft; review and save the profile to publish. Snapshots retain their original revision and expiry and do not update automatically with a recipe.",
    );
}

fn render_editor(world: &mut World, body: Entity) {
    let Some(editor) = world.get::<Editor>(body) else {
        return;
    };
    let claims = world
        .get::<forms::Form>(editor.form)
        .map(|form| form.payload["request"]["fields"]["claims"].clone())
        .unwrap_or(Value::Null);
    panel::clear(world, body);
    label(world, body, "Up to four public snapshots. Remove one before selecting another when the profile is full; total profile size is checked before publication.");
    for (index, value) in claims.as_array().into_iter().flatten().enumerate() {
        if let Ok(document) = serde_json::from_value::<Document>(value.clone()) {
            label(world, body, &caption(&document));
            panel::button(
                world,
                body,
                body,
                "Remove claim from profile draft",
                Remove(index),
            );
        }
    }
}

#[derive(Clone)]
struct Remove(usize);
impl Action for Remove {
    fn apply(&self, world: &mut World, body: Entity) {
        let Some(form) = world.get::<Editor>(body).map(|editor| editor.form) else {
            return;
        };
        if let Some(mut form) = world.get_mut::<forms::Form>(form)
            && let Some(claims) = form.payload["request"]["fields"]["claims"].as_array_mut()
            && self.0 < claims.len()
        {
            claims.remove(self.0);
        }
        render_editor(world, body);
    }
}

#[derive(Clone)]
struct Select {
    owner: Entity,
    document: Document,
}
impl Action for Select {
    fn apply(&self, world: &mut World, _: Entity) {
        let document = &self.document;
        if !matches!(document.statement.audience, Audience::Public) {
            return;
        }
        let editors = world
            .query::<(Entity, &Editor)>()
            .iter(world)
            .filter(|(_, editor)| {
                editor.owner == self.owner
                    && document.statement.descriptor.subject
                        == nucleus::trust_analysis::Identity::Organ(editor.subject.clone())
            })
            .map(|(entity, editor)| (entity, editor.form))
            .collect::<Vec<_>>();
        for (body, form) in editors {
            if let Some(mut form) = world.get_mut::<forms::Form>(form) {
                let value = &mut form.payload["request"]["fields"]["claims"];
                if !value.is_array() {
                    *value = json!([]);
                }
                let claims = value.as_array_mut().unwrap();
                let series = document.statement.series().ok();
                claims.retain(|value| {
                    serde_json::from_value::<Document>(value.clone())
                        .is_ok_and(|previous| previous.statement.series().ok() != series)
                });
                if claims.len() < 4 {
                    claims.push(serde_json::to_value(document).unwrap());
                }
            }
            render_editor(world, body);
        }
    }
}

pub(super) fn choices(world: &mut World, owner: Entity, parent: Entity, value: &Value) -> bool {
    let result = &value["trust"];
    if result["kind"] != "claim_list" {
        return false;
    }
    for entry in result["entries"].as_array().into_iter().flatten() {
        if entry["authority_current"] != true || entry["current_retained"] != true {
            continue;
        }
        if let Ok(document) = serde_json::from_value::<Document>(entry["document"].clone())
            && matches!(document.statement.audience, Audience::Public)
        {
            label(world, parent, &caption(&document));
            panel::button(
                world,
                parent,
                parent,
                "Select this public snapshot for profile review",
                Select { owner, document },
            );
        }
    }
    if let Some(next) = result["next"].as_str() {
        form(
            world,
            owner,
            parent,
            "More retained claim snapshots",
            json!({"action":"trust-analysis","request_id":nucleus::new_uid("request"),"request":{"kind":"claims","request":{"kind":"list","subject":result["subject"],"before":next,"as_organ":false}}}),
            vec![],
            None,
        );
    }
    true
}

pub(super) fn cards(world: &mut World, parent: Entity, fields: &Value) {
    for value in fields["claims"].as_array().into_iter().flatten() {
        if let Ok(document) = serde_json::from_value::<Document>(value.clone()) {
            label(world, parent, &caption(&document));
            if document.statement.withdrawn { label(world, parent, "Withdrawn statement: no current numerical contribution claim."); }
            if document.statement.expires_at <= nucleus::execution::now().timestamp() {
                label(
                    world,
                    parent,
                    "Expired snapshot; request a newer signed revision before checking current support.",
                );
            }
            label(
                world,
                parent,
                "Publisher statement. Its amount and signing identity have not been independently checked here. A snapshot may be expired or superseded; opening it makes no remote request.",
            );
            crate::native::trust_castle::claim_controls(world, parent, &document);
        }
    }
}

pub(super) fn confirmation(world: &mut World, parent: Entity, fields: &Value) {
    for value in fields["claims"].as_array().into_iter().flatten() {
        if let Ok(document) = serde_json::from_value::<Document>(value.clone()) {
            label(
                world,
                parent,
                &format!(
                    "Public claim snapshot: {} · issuer {}:{} · activity {} · evidence {:?} · role {:?}",
                    caption(&document),
                    document.statement.issuer.kind(),
                    document.statement.issuer.uid(),
                    document
                        .statement
                        .descriptor
                        .measurement
                        .meaning
                        .definition_hash,
                    document.statement.descriptor.level,
                    document.statement.descriptor.role
                ),
            );
        }
    }
}

#[cfg_attr(test, test)]
fn selecting_profile_claims_only_changes_the_reviewed_draft_and_keeps_reader_policies_private() {
    use nucleus::trust_analysis::{
        Identity,
        claims::{Authorship, Comparator, Descriptor, Metric, Statement},
        query::{CounterpartyBasis, EvidenceLevel},
    };
    let mut app = crate::native::sand_panel::tests::app();
    let owner = app.world_mut().spawn_empty().id();
    let parent = app.world_mut().spawn(Node::default()).id();
    let subject = nucleus::new_uid("r");
    let issuer = nucleus::new_uid("r");
    let signer = engine::trust::Signer::generate(&issuer, "profile-native");
    let now = nucleus::execution::now().timestamp();
    let mut document = Document {
        statement: Statement {
            title: "Public food deliveries".into(),
            issuer: Identity::Person(issuer),
            claim: nucleus::new_uid("r"),
            revision: 1,
            descriptor: Descriptor {
                subject: Identity::Organ(subject.clone()),
                role: nucleus::transfer::outcome::Role::Giver,
                measurement: nucleus::transfer::receipt::Measurement {
                    meaning: nucleus::vocabulary::reference::MeaningReference {
                        vocabulary_uid: nucleus::new_uid("g"),
                        concept_uid: nucleus::new_uid("c"),
                        revision: 1,
                        definition_hash: "a".repeat(64),
                    },
                    unit: None,
                    object_commitment: None,
                },
                level: EvidenceLevel::NeedLinkedReceiver,
                metric: Metric::Quantity,
                counterparty_basis: CounterpartyBasis::Organ,
                from: None,
                until: None,
            },
            comparator: Comparator::AtLeast,
            amount: nucleus::DecimalValue::parse_inferred("100").unwrap(),
            as_of: now,
            issued_at: now,
            expires_at: now + 3600,
            withdrawn: false,
            audience: Audience::Public,
            authorship: Authorship::Manual,
        },
        key_id: signer.key_id.clone(),
        public_key: signer.public_key_b64(),
        signature: String::new(),
    };
    engine::trust_analysis::sign_claim_document(&mut document, &signer).unwrap();
    let save = form(
        app.world_mut(),
        owner,
        parent,
        "Save profile",
        action(
            json!({"command":"save-profile","fields":{"name":"Food workshop","claims":[]},"parents":[],"destinations":[]}),
        ),
        vec![],
        Some("Review before publishing"),
    );
    editor(app.world_mut(), owner, parent, save, &subject);
    let request = app.world_mut().query::<&forms::Form>().iter(app.world()).find(|form| form.payload["action"]=="trust-analysis").unwrap().payload.clone();
    let request: engine::actions::Action = serde_json::from_value(request).unwrap();
    assert!(matches!(request, engine::actions::Action::TrustAnalysis { request, .. } if !request.is_mutation() && request.permission()=="record:read"));
    Select {
        owner,
        document: document.clone(),
    }
    .apply(app.world_mut(), parent);
    let payload = &app.world().get::<forms::Form>(save).unwrap().payload;
    assert_eq!(payload["request"]["fields"]["claims"], json!([document]));
    assert!(
        app.world()
            .get::<forms::Form>(save)
            .unwrap()
            .pending
            .is_none()
    );
    Select {
        owner,
        document: document.clone(),
    }
    .apply(app.world_mut(), parent);
    assert_eq!(
        app.world().get::<forms::Form>(save).unwrap().payload["request"]["fields"]["claims"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut restricted = document.clone();
    restricted.statement.audience = Audience::default();
    Select {
        owner,
        document: restricted,
    }
    .apply(app.world_mut(), parent);
    assert_eq!(
        app.world().get::<forms::Form>(save).unwrap().payload["request"]["fields"]["claims"],
        json!([document])
    );
    let mut unrelated = document.clone();
    unrelated.statement.descriptor.subject = Identity::Person(nucleus::new_uid("r"));
    Select {
        owner,
        document: unrelated,
    }
    .apply(app.world_mut(), parent);
    assert_eq!(
        app.world().get::<forms::Form>(save).unwrap().payload["request"]["fields"]["claims"],
        json!([document])
    );
    let body = app
        .world_mut()
        .query::<(Entity, &Editor)>()
        .iter(app.world())
        .find(|(_, editor)| editor.owner == owner)
        .unwrap()
        .0;
    Remove(0).apply(app.world_mut(), body);
    assert_eq!(
        app.world().get::<forms::Form>(save).unwrap().payload["request"]["fields"]["claims"],
        json!([])
    );
    assert!(
        app.world()
            .get::<forms::Form>(save)
            .unwrap()
            .pending
            .is_none()
    );
}

#[cfg_attr(test, test)]
fn public_profile_opens_the_actual_organ_experience_without_discovery() {
    let mut app = crate::native::sand_panel::tests::app();
    let root = app.world_mut().spawn(crate::native::workspace::Workspaces::default()).id();
    let owner = app.world_mut().spawn((Node::default(), ChildOf(root))).id();
    let parent = app.world_mut().spawn((Node::default(), ChildOf(owner))).id();
    let organ = nucleus::new_uid("r");
    super::result(app.world_mut(), owner, parent, &json!({"profile":{"authority":{"organ":organ},"fields":{"name":"Food workshop"}}}));
    crate::native::trust_castle::tests::assert_subject_entry(app.world_mut(), parent, nucleus::trust_analysis::Identity::Organ(organ));
}

#[cfg_attr(test, test)]
fn optional_helper_form_keeps_preset_pins_roles_and_explicit_enable() {
    let mut app = crate::native::sand_panel::tests::app();
    let owner = app.world_mut().spawn(Node::default()).id();
    super::servers_view(app.world_mut(),owner,owner,&json!([]),true);
    let helper = app.world_mut().query::<(Entity,&forms::Form)>().iter(app.world()).find(|(_,form)|form.payload["request"]["choice"]["preset"] == "institute_discovery").unwrap().0;
    let original = forms::payload(app.world(),helper).unwrap();
    assert_eq!(original["request"]["choice"]["query"],false);
    assert_eq!(original["request"]["choice"]["publication"],false);
    assert_eq!(original["request"]["choice"]["mailbox"],false);
    let organ = nucleus::new_uid("r");
    forms::prepare_values(app.world_mut(),helper,&[("/request/choice/organ",json!(organ)),("/request/choice/endpoint",json!("pinned-endpoint")),("/request/choice/query",json!(true))]);
    let edited = forms::payload(app.world(),helper).unwrap();
    assert_eq!(edited["request"]["choice"]["preset"],"institute_discovery");
    assert_eq!(edited["request"]["choice"]["organ"],organ);
    assert_eq!(edited["request"]["choice"]["endpoint"],"pinned-endpoint");
    assert_eq!(edited["request"]["choice"]["query"],true);
    assert_eq!(edited["request"]["choice"]["publication"],false);
    assert_eq!(edited["request"]["choice"]["mailbox"],false);
    assert!(app.world().get::<forms::Form>(helper).unwrap().pending.is_none());
}

#[cfg_attr(test, test)]
fn received_private_location_invitation_opens_scoped_controls_without_observing() {
    let mut app=crate::native::sand_panel::tests::app();
    app.add_message::<crate::location::Request>();
    let owner=app.world_mut().spawn(Node::default()).id();
    let recipient=nucleus::new_uid("r");
    let invitation=nucleus::location::Invitation {record_uid:nucleus::new_uid("r"),authority_node_id:"a".repeat(64),grant:Some(nucleus::location::InvitationGrant {uid:nucleus::new_uid("li"),issuer:nucleus::new_uid("r"),recipient:recipient.clone(),radius_metres:Some(1000),expires_at_ms:1_800_000_000_000})};
    super::result(app.world_mut(),owner,owner,&json!({"requests":[{"record":nucleus::new_uid("r"),"state":{"state":"accepted"},"messages":[{"body":format!("Private location invitation\n{}",invitation.reference().unwrap())}]}]}));
    let label=app.world_mut().query::<(Entity,&Text)>().iter(app.world()).find(|(_,text)|text.0=="Open location invitation").unwrap().0;
    let button=app.world().get::<ChildOf>(label).unwrap().parent();
    let action=app.world().get::<crate::native::actions::ActionButton>(button).unwrap();
    let (target,actions)=(action.target,action.actions.clone());
    actions.run(app.world_mut(),target);
    assert_eq!(app.world_mut().query_filtered::<Entity,With<crate::location::Panel>>().iter(app.world()).count(),1);
    assert!(app.world().resource::<Messages<crate::location::Request>>().is_empty());
    assert!(app.world_mut().query::<&Text>().iter(app.world()).any(|text|text.0.contains(&format!("Intended Person: {recipient}"))));
}

crate::native::laboratory_cases! { selecting_profile_claims_only_changes_the_reviewed_draft_and_keeps_reader_policies_private, public_profile_opens_the_actual_organ_experience_without_discovery, optional_helper_form_keeps_preset_pins_roles_and_explicit_enable, received_private_location_invitation_opens_scoped_controls_without_observing, }
