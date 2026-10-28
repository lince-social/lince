use super::*;
use model::FieldKind;
use ui::{Command, button, row};

fn public_document(document: &Value) -> bool {
    matches!(
        document["kind"].as_str(),
        Some("publication" | "proof_fragment")
    )
}

fn approved_review(document: &Value) -> bool {
    matches!(serde_json::from_value::<nucleus::transfer::receipt::Document>(document.clone()), Ok(nucleus::transfer::receipt::Document::Review(review)) if review.is_complete())
}

pub(super) fn person_approval_form(
    person: &str,
    receipt: &Value,
    scope: &Value,
    role: &str,
) -> Result<Form, String> {
    let scope: nucleus::transfer::receipt::Scope =
        serde_json::from_value(scope.clone()).map_err(|error| error.to_string())?;
    let role: nucleus::transfer::receipt::ApprovalRole =
        serde_json::from_value(json!(role)).map_err(|error| error.to_string())?;
    let approval = nucleus::transfer::receipt::PersonApproval {
        scope: scope.id()?,
        role,
        person: person.into(),
        issued_at: nucleus::execution::now().timestamp(),
        key_id: String::new(),
        signature: String::new(),
    };
    let mut form = action_form(
        person,
        "Consent to public Person attribution",
        json!({"kind":"approve_person","record":receipt["record"],"expected_version":receipt["version"],"approval":approval}),
    );
    form.review["Public identity"] = json!({"Person":person,"Role":role,"Scope":scope});
    form.review["Consent"] = json!(
        "Your Person key signs consent to identify you in this exact receipt scope. This does not confirm delivery, give Organ authority or attribute other Organ activity to you. Changing scope requires fresh consent."
    );
    if scope.party_attribution
        == nucleus::transfer::receipt::PartyAttribution::OriginalOrganAuthority
    {
        form.data["request"]["private_transfer"] = Value::Null;
        form.field(
            "/request/private_transfer",
            "Private Transfer reference for your original role",
            FieldKind::Optional,
        );
        form.review["Private reference"] = json!(
            "This reference opens your original role locally and is omitted from public consent and exported receipts."
        );
    }
    Ok(form)
}

pub(super) fn verification_message(code: Option<&str>, message: &str) -> String {
    let explanation = match code {
        Some("transfer_receipt_unknown_identity") => {
            "Unknown identity: verify the original Organ independently before accepting this proof."
        }
        Some("transfer_receipt_incomplete" | "incomplete") => {
            "Incomplete proof: original event or Need evidence is missing. This does not mean nothing was delivered."
        }
        Some("transfer_receipt_authority_withheld") => {
            "Authority withheld: this Lince cannot currently accept the signing authority. Signed history is retained; physical delivery remains a separate question."
        }
        Some("transfer_receipt_invalid" | "invalid") => {
            "Invalid proof: verification failed and this evidence is excluded from accepted history."
        }
        _ => return message.into(),
    };
    format!("{explanation} {message}")
}

pub(super) fn action_form(person: &str, title: &str, request: Value) -> Form {
    let mut form = Form::action(
        title,
        json!({"action":"transfer-receipt","person":person,"request":request}),
    );
    form.review = json!({
        "Acting Person":person,
        "Privacy":"The saved Record is private to you initially. This action does not change the Transfer or its quantities.",
        "Approvals":"Origin, giver and receiver approve the same disclosed fields and retention date. Approval permits sharing; it does not confirm delivery.",
        "Attribution":"Sharing approval alone does not establish historical Organ representation. Check the original authority proofs in the reviewed scope.",
    });
    form
}

pub(super) fn approval_form(person: &str, receipt: &Value, scope: &Value, role: &str) -> Form {
    let mut form = action_form(
        person,
        &format!("Approve {role} sharing"),
        json!({"kind":"approve","record":receipt["record"],"expected_version":receipt["version"],"role":role,"authority_record":""}),
    );
    form.review["Disclosed scope"] = scope.clone();
    if role != "origin" && scope["party_attribution"] == "original_organ_authority" {
        form.data["request"]["private_transfer"] = Value::Null;
        form.field(
            "/request/private_transfer",
            "Private Transfer reference for your original role (optional for administrators)",
            FieldKind::Optional,
        );
        form.review["Private reference"] = json!(
            "Use the original Transfer's Record UID to prove your own role without importing its canonical state. This reference is omitted from the exported receipt."
        );
    }
    form.field(
        "/request/authority_record",
        "Receipt signing authority Record",
        FieldKind::Reference("record".into()),
    );
    form
}

pub(super) fn event_form(person: &str, receipt: &Value, source: Value) -> Form {
    use nucleus::transfer::receipt::Stage;
    let kind = text(&source, "kind");
    let stage = match kind.as_str() {
        "settlement" => Some(Stage::Settled),
        "settlement_status" => Some(Stage::SettlementStatus),
        "cancellation" => Some(Stage::Cancelled),
        "compensation" => Some(Stage::Compensated),
        "dispute" => Some(Stage::Dispute),
        _ => None,
    };
    let mut form = action_form(
        person,
        match kind.as_str() {
            "dispute" => "Review dispute event",
            "settlement" => "Review canonical settlement event",
            "settlement_status" => "Review partial or full settlement status",
            "cancellation" => "Review applied cancellation",
            "compensation" => "Review your private application compensation",
            _ => "Review outcome event",
        },
        json!({"kind":"preview_event","request":{
            "record":receipt["record"],"expected_version":receipt["version"],"authority_record":"","source":source,
        }}),
    );
    form.field(
        "/request/request/authority_record",
        "Receipt signing authority Record",
        FieldKind::Reference("record".into()),
    );
    if stage.is_some() {
        form.field(
            "/request/request/source/fact",
            match kind.as_str() {
                "dispute" => "Your original dispute evidence Fact",
                "cancellation" => "Applied cancellation Fact (not a proposal)",
                "compensation" => "Your original compensation Fact",
                _ => "Original settlement evidence Fact",
            },
            FieldKind::Text,
        );
        form.field(
            "/request/request/source/supersedes",
            "Previous public event (for an update)",
            FieldKind::OptionalChoice(
                array(&receipt["document"]["value"]["publication"], "events")
                    .iter()
                    .filter_map(|value| {
                        let signed: nucleus::transfer::receipt::SignedEvent =
                            serde_json::from_value(value.clone()).ok()?;
                        if Some(signed.event.content.stage()) != stage {
                            return None;
                        }
                        let description = receipt_history::description(&value["event"]["content"]);
                        let id = signed.event.id().ok()?;
                        let time = chrono::DateTime::from_timestamp(signed.event.recorded_at, 0)?
                            .format("%Y-%m-%d %H:%M:%S UTC")
                            .to_string();
                        Some((id.as_str().to_owned(), format!("{description} · {time}")))
                    })
                    .collect(),
            ),
        );
    } else {
        form.field(
            "/request/request/source/record",
            "Your Need binding or outcome Record",
            FieldKind::Reference("record".into()),
        );
    }
    form.review["Next step"] = json!(
        "Preview the exact public event before signing. Need links and correction predecessors must already be present in this receipt. Private Record references are omitted from the public event."
    );
    if matches!(
        kind.as_str(),
        "settlement_status" | "cancellation" | "compensation"
    ) {
        form.review["Lifecycle disclosure"] = json!(
            "Requires its own approved report type. Full settlement is completion of the canonical obligation at that event, not proof the original Need was fulfilled. Cancellation is not delivery. Compensation reverses only the named owner's private application, without erasing settlement or another party's receipt. Private quantities and Record mappings remain omitted."
        );
    }
    form
}

pub(super) fn source_form(
    person: &str,
    receipt: &Value,
    outcome: Value,
    private_transfer: Value,
    public_need: Value,
) -> Form {
    let statement = outcome["kind"] == "state";
    let mut form = action_form(
        person,
        if statement {
            "Record receipt outcome source"
        } else {
            "Freeze your Need for this receipt"
        },
        json!({"kind":"record_source","request":{"receipt":receipt["record"],"expected_version":receipt["version"],"private_transfer":private_transfer,"outcome":outcome,"public_need":public_need}}),
    );
    form.field(
        "/request/request/private_transfer",
        "Original Transfer reference for your own role",
        FieldKind::Optional,
    );
    if statement {
        form.field(
            "/request/request/outcome/amount",
            "Reported amount (exact decimal)",
            FieldKind::Text,
        );
        form.field(
            "/request/request/outcome/withdrawn",
            "Withdraw this statement",
            FieldKind::Choice(vec!["false".into(), "true".into()]),
        );
        form.field(
            "/request/request/outcome/role",
            "Your reporting role",
            FieldKind::Choice(vec!["giver".into(), "receiver".into()]),
        );
    }
    form.review["Privacy"] = json!(
        "Save a signed private snapshot of your own Need or report. This does not share it, complete or settle a Transfer, or change any Record quantities."
    );
    form.review["Source"] = outcome;
    form
}

fn source_options(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    person: &str,
    receipt: &Value,
    scope: &Value,
) {
    button(
        world,
        parent,
        owner,
        "Local Needs and source snapshots",
        Command::Open(action_form(
            person,
            "Review local receipt sources",
            json!({"kind":"source_options","receipt":receipt["record"]}),
        )),
    );
    let options = &receipt["source_options"];
    let local = world.get::<View>(owner).unwrap().context["organ"].clone();
    let anchored = public_document(&receipt["document"])
        || array(&receipt["document"]["value"], "approvals")
            .iter()
            .any(|approval| approval["role"] == "origin");
    if !anchored {
        crate::native::edit_mode::label(
            world,
            parent,
            "The origin must approve the source reference before a Need is frozen.",
            12.0,
        );
    }
    if anchored && local == scope["receiver"] {
        for option in array(&options["needs"], "options") {
            let terms = &option["terms"];
            let outcome = json!({"kind":"bind_need","selection":terms["selection"],"expected_measure":terms["measure"],"supersedes":null});
            let mut form = source_form(person, receipt, outcome, Value::Null, Value::Null);
            form.review["Requested Need"] = terms.clone();
            button(
                world,
                parent,
                owner,
                &format!("Freeze Need · {}", text(option, "label")),
                Command::Open(form),
            );
        }
    }
    for source in array(options, "sources") {
        let snapshot = &source["snapshot"];
        let content = &snapshot["content"];
        button(
            world,
            parent,
            owner,
            &format!("Open {} snapshot", model::human(&text(content, "kind"))),
            Command::Open(action_form(
                person,
                "Open private source snapshot",
                json!({"kind":"inspect_source","record":snapshot["record"]}),
            )),
        );
        if content["kind"] == "need_binding" {
            let outcome = json!({"kind":"state","role":"receiver","measure":content["terms"]["measure"],"amount":"0","withdrawn":false,"need_binding":snapshot["record"],"supersedes":null});
            let mut form = source_form(
                person,
                receipt,
                outcome,
                snapshot["private_transfer"].clone(),
                Value::Null,
            );
            form.review["Frozen Need"] = content["terms"].clone();
            button(
                world,
                parent,
                owner,
                "Report received against this Need…",
                Command::Open(form),
            );
        } else {
            let mut outcome = content.clone();
            outcome["kind"] = json!("state");
            outcome["amount"] = content["quantity"]["value"].clone();
            outcome["supersedes"] = snapshot["record"].clone();
            outcome.as_object_mut().unwrap().remove("quantity");
            button(
                world,
                parent,
                owner,
                "Correct this source report…",
                Command::Open(source_form(
                    person,
                    receipt,
                    outcome,
                    snapshot["private_transfer"].clone(),
                    snapshot["public_need"].clone(),
                )),
            );
        }
        if public_document(&receipt["document"]) || approved_review(&receipt["document"]) {
            button(
                world,
                parent,
                owner,
                "Preview this source's public event…",
                Command::Open(event_form(
                    person,
                    receipt,
                    json!({"kind":"snapshot","record":snapshot["record"]}),
                )),
            );
        }
    }
    if options["partial"] == true || options["needs"]["more"] == true {
        crate::native::edit_mode::label(
            world,
            parent,
            "Partial source choices: only the first permitted page is shown.",
            12.0,
        );
    }
    if local == scope["giver"] || local == scope["receiver"] {
        for signed in array(&receipt["document"]["value"]["publication"], "events") {
            let Ok(signed) =
                serde_json::from_value::<nucleus::transfer::receipt::SignedEvent>(signed.clone())
            else {
                continue;
            };
            if let nucleus::transfer::receipt::Content::Need {
                measurement,
                requested,
                ..
            } = &signed.event.content
            {
                let Ok(id) = signed.event.id() else { continue };
                let outcome = json!({"kind":"state","role":if local==scope["giver"] {"giver"}else{"receiver"},"measure":{"meaning":measurement.meaning,"unit":measurement.unit,"object":null},"amount":"0","withdrawn":false,"need_binding":null,"supersedes":null});
                let mut form = source_form(person, receipt, outcome, Value::Null, json!(id));
                form.review["Public Need"] =
                    json!({"measurement":measurement,"requested":requested,"event":id});
                button(
                    world,
                    parent,
                    owner,
                    "Report against this signed public Need…",
                    Command::Open(form),
                );
            }
        }
    }
}

pub(super) fn current_receipt(world: &World, owner: Entity, occurrence: &Value) -> Option<Value> {
    let receipt = world.get::<View>(owner)?.receipt.as_ref()?;
    if receipt["historical"] == true {
        return None;
    }
    let document = &receipt["document"];
    let scope = if public_document(document) {
        &document["value"]["publication"]["scope"]
    } else if approved_review(document) {
        &document["value"]["scope"]
    } else {
        return None;
    };
    let expected = nucleus::transfer::receipt::Source::occurrence_id(
        scope["source"]["origin"].as_str()?,
        occurrence["uid"].as_str()?,
    )
    .ok()?;
    (scope["source"]["occurrence"].as_str() == Some(expected.as_str())).then(|| receipt.clone())
}

pub(super) fn publish_event_form(person: &str, preview: &Value) -> Form {
    let mut form = action_form(
        person,
        "Sign reviewed receipt event",
        json!({
            "kind":"publish_event","request":preview["request"],"reviewed_at":preview["reviewed_at"],"expected_projection":preview["expected_projection"],
        }),
    );
    form.review["Public event"] = preview["event"].clone();
    if preview["artifact_kind"] == "proof_fragment" {
        form.review["Proof packaging"] = json!(
            "This signed fragment keeps the original scope approvals. Its predecessor and Need proofs remain separate artifacts; readers need those permitted proofs before accepting the outcome."
        );
    }
    form.review["Confirmation"] = json!(
        "Sign this exact projection within five minutes. Changed source history or authority requires another preview. The resulting receipt Record is private initially; export and sharing are separate actions."
    );
    form
}

pub(super) fn clear_form(world: &mut World, owner: Entity) {
    let close = world.get::<TransferCastle>(owner).is_some_and(|castle| {
        castle.form.as_ref().is_some_and(|form| {
            matches!(
                form.data["action"].as_str(),
                Some("transfer-receipt" | "identity")
            )
        })
    });
    if close {
        world.get_mut::<TransferCastle>(owner).unwrap().form = None;
        forms::render(world, owner);
    }
}

pub(super) fn participation_form(person: &str, request: &Value) -> Form {
    let mut form = action_form(
        person,
        "Approve original Organ participation",
        json!({
            "kind":"authorize_participation", "cell":"", "request":if request.is_null() { String::new() } else { request.to_string() },
        }),
    );
    form.field(
        "/request/request",
        "Private participation request",
        FieldKind::Text,
    );
    form.review = json!({
        "Acting Person":person,
        "Private request":request,
        "Authority":"Approve original representation for this exact signed agreement. When an optional public-history manifest is present, this also grants the giver/receiver publication rights inside that manifest; the origin must still attest each occurrence. Record this approval during activation.",
        "Privacy":"The request contains private Transfer and Person references. Exchange it only with the named approving Organ. The resulting approval omits those references.",
        "Meaning":"Publication rights cover only the displayed optional manifest. Delivery confirmation, public Person attribution and Transfer quantities require their separate controls.",
    });
    form
}

pub(super) fn review_participation_form(form: &mut Form) -> Result<(), String> {
    form.commit_fields()?;
    let raw = form.data["request"]["request"]
        .as_str()
        .ok_or("Paste a private participation request")?
        .to_owned();
    if raw.len() > 8192 {
        return Err("Participation request exceeds its byte budget".into());
    }
    let request: nucleus::transfer::receipt::ParticipationRequest =
        serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    request.validate()?;
    form.review["Private request"] =
        serde_json::to_value(request).map_err(|error| error.to_string())?;
    form.review["Reviewed request"] = json!(raw);
    form.mode = "participation-reviewed".into();
    Ok(())
}

pub(super) fn activation_form(transfer: &Value, action: Value, listing: &Value) -> Form {
    let mut form = detail::party_form("Activate occurrence", action, transfer);
    form.data["original_authorities"] = json!([]);
    form.field(
        "/original_authorities",
        "Original participation approvals",
        FieldKind::Selections(
            array(listing, "entries")
                .iter()
                .filter_map(|entry| {
                    let terms = &entry["participation"];
                    if terms.is_null() {
                        return None;
                    }
                    let uid = entry["record"].as_str()?;
                    Some((
                        uid.to_owned(),
                        format!(
                            "{} · {} · agreement {} · {}",
                            if terms["party_commitment"].is_null() {
                                "Source Organ"
                            } else {
                                "Party Organ"
                            },
                            text(terms, "organ"),
                            text(terms, "agreement_revision"),
                            uid
                        ),
                    ))
                })
                .collect(),
        ),
    );
    form.review["Original authority"] = json!(
        "Refresh My receipt Records after importing approvals. Include source, giver and receiver approvals before activation for the original Organ authority category. Leaving this empty still permits an ordinary Transfer; later approval cannot rewrite its activation evidence."
    );
    form
}

pub(super) fn prepare_form(transfer: &Value, occurrence: &Value, person: &str) -> Form {
    let records: Vec<_> = array(&occurrence["outcomes"], "entries")
        .iter()
        .take(32)
        .filter_map(|entry| entry["evidence"]["record"].as_str())
        .collect();
    let stages = if records.is_empty() {
        json!(["settled", "dispute"])
    } else {
        json!(["need", "giver", "receiver", "settled", "dispute"])
    };
    let mut form = action_form(
        person,
        "Review receipt sharing",
        json!({"kind":"prepare","scope":{
            "occurrence":occurrence["uid"],"expected_revision":transfer["revision"],"outcome_records":records,
            "stages":stages,"publish_agreed_quantity":false,"public_person_roles":[],"use_upfront_terms":true,
            "public_measurements":"[]",
            "expires_at":(nucleus::execution::now()+chrono::Duration::days(365)).to_rfc3339(),
        }}),
    );
    form.transfer = transfer["uid"].as_str().map(str::to_owned);
    form.revision = transfer["revision"].as_u64();
    form.data["request"]["scope"]["audience"] = json!({"kind":"public"});
    if let Some(terms) = transfer
        .pointer("/revision_evidence/current/terms/transfer/public_history")
        .cloned()
        .and_then(|terms| {
            serde_json::from_value::<nucleus::transfer::public_history::Terms>(terms).ok()
        })
    {
        form.data["request"]["scope"]["audience"] = json!(terms.audience);
        form.data["request"]["scope"]["stages"] = json!(terms.stages);
        form.data["request"]["scope"]["publish_agreed_quantity"] = json!(terms.agreed_quantity);
        form.data["request"]["scope"]["public_measurements"] =
            json!(serde_json::to_string(&terms.measurements).unwrap());
        form.data["request"]["scope"]["expires_at"] = json!(
            (nucleus::execution::now()
                + chrono::Duration::seconds(i64::from(terms.retention_seconds)))
            .to_rfc3339()
        );
        form.review["Original public-history terms"] = json!(terms);
    }
    form.field(
        "/request/scope/expires_at",
        "Retain until (date and time with timezone)",
        FieldKind::Text,
    );
    form.field(
        "/request/scope/publish_agreed_quantity",
        "Include agreed quantity",
        FieldKind::Choice(vec!["false".into(), "true".into()]),
    );
    form.field(
        "/request/scope/outcome_records",
        "Included outcome Records",
        FieldKind::Selections(
            array(&occurrence["outcomes"], "entries")
                .iter()
                .take(32)
                .filter_map(|entry| {
                    let uid = entry["evidence"]["record"].as_str()?;
                    let content = &entry["evidence"]["content"];
                    let label = format!(
                        "{} {} · {}",
                        model::human(&text(content, "role")),
                        model::human(&text(content, "kind")),
                        display(
                            content
                                .get("quantity")
                                .unwrap_or(&content["terms"]["requested"])
                        )
                    );
                    Some((uid.to_owned(), label))
                })
                .collect(),
        ),
    );
    form.field(
        "/request/scope/stages",
        "Included report types",
        FieldKind::Selections(
            [
                ("need", "Receiver's Need terms"),
                ("giver", "Giver statements"),
                ("receiver", "Receiver confirmations"),
                ("settled", "Canonical settlement"),
                (
                    "settlement_status",
                    "Partial/full settlement and remaining amount",
                ),
                (
                    "cancelled",
                    "Applied cancellation and exact cancelled amount",
                ),
                (
                    "compensated",
                    "Owner's private application compensation status",
                ),
                ("dispute", "Disputes"),
            ]
            .into_iter()
            .map(|(value, label)| (value.into(), label.into()))
            .collect(),
        ),
    );
    form.review["Visible fields"] = json!([
        "Approving Organs",
        "Occurrence commitment",
        "Selected meanings and units",
        "Selected report types",
        "Sharing term"
    ]);
    form.review["Private fields omitted"] = json!([
        "Unselected Persons",
        "Need mappings",
        "Private Record effects",
        "Addresses and location"
    ]);
    form.field(
        "/request/scope/public_measurements",
        "Additional permitted measurements (paste a measurement proposal)",
        FieldKind::Text,
    );
    form.field(
        "/request/scope/public_person_roles",
        "Identify Persons publicly (each must consent)",
        FieldKind::Selections(vec![
            ("giver".into(), "Giver Person".into()),
            ("receiver".into(), "Receiver Person".into()),
        ]),
    );
    form.field(
        "/request/scope/use_upfront_terms",
        "Use original agreed terms (clear for a separately approved renewal or expansion)",
        FieldKind::Choice(vec!["true".into(), "false".into()]),
    );
    super::forms::audience_fields(&mut form, "/request/scope/audience");
    form
}

pub(super) fn render(world: &mut World, owner: Entity, parent: Entity) {
    let person = person(world, owner);
    if person.is_empty() {
        return;
    }
    crate::native::edit_mode::label(world, parent, "Receipt publication reviews", 15.0);
    button(
        world,
        parent,
        owner,
        "Open receipt Message…",
        Command::Open(receipt_delivery::inspect_form(&person)),
    );
    let controls = row(world, parent);
    receipt_identity::controls(world, owner, controls, &person);
    button(
        world,
        controls,
        owner,
        "Receipt inbox",
        Command::Open(receipt_delivery::inbox_form(&person, None)),
    );
    button(
        world,
        controls,
        owner,
        "Approve original participation…",
        Command::Open(participation_form(&person, &Value::Null)),
    );
    for (label, request) in [
        ("My receipt Records", json!({"kind":"list"})),
        (
            "Authorize this device",
            json!({"kind":"issue_authority","cell":""}),
        ),
    ] {
        button(
            world,
            controls,
            owner,
            label,
            Command::Open(action_form(&person, label, request)),
        );
    }
    let mut import = action_form(
        &person,
        "Import receipt document",
        json!({"kind":"import","document":""}),
    );
    import.field("/request/document", "Receipt document", FieldKind::Text);
    import.review["Verification"] = json!(
        "Signatures need previously recognized Organ identities. Imported documents cannot perform Actions. An incomplete review remains a draft."
    );
    button(
        world,
        controls,
        owner,
        "Import receipt…",
        Command::Open(import),
    );
    button(
        world,
        controls,
        owner,
        "Import receipt file…",
        Command::ImportReceiptFile,
    );
    let view = world.get::<View>(owner).unwrap();
    let receipt = view.receipt.clone();
    let listing = view.receipt_list.clone();
    let export_ready = view.receipt_export.is_some();
    for entry in array(&listing, "entries") {
        button(
            world,
            parent,
            owner,
            &format!(
                "{} · version {}",
                text(entry, "head"),
                text(entry, "version")
            ),
            Command::Open(action_form(
                &person,
                "Open receipt Record",
                json!({"kind":"inspect","record":entry["record"]}),
            )),
        );
    }
    if listing["partial"] == true {
        crate::native::edit_mode::label(
            world,
            parent,
            "Only the latest 64 receipt Records are listed.",
            12.0,
        );
    }
    let Some(receipt) = receipt else { return };
    let document = &receipt["document"];
    if receipt_identity::render(world, owner, parent, document) {
        return;
    }
    if receipt_delivery::render(world, owner, parent, &person, document) {
        return;
    }
    if receipt_history::render(world, owner, parent, &person, document) {
        return;
    }
    if receipt_browsing::render(world, owner, parent, &person, document) {
        return;
    }
    if document["kind"] == "source_snapshot" {
        crate::native::edit_mode::label(
            world,
            parent,
            "Private signed source snapshot. Its quantities and Need terms remain frozen when the live Record changes.",
            13.0,
        );
        forms::review(world, parent, &document["value"]["snapshot"]["content"], 0);
        button(
            world,
            parent,
            owner,
            "Open snapshot and visibility",
            Command::Record(text(&receipt, "record")),
        );
        if export_ready {
            button(
                world,
                parent,
                owner,
                "Copy minimal measurement proposal",
                Command::CopyReceipt,
            );
        }
        if let Some(record) = receipt["source_receipt"].as_str() {
            button(
                world,
                parent,
                owner,
                "Back to receipt review",
                Command::Open(action_form(
                    &person,
                    "Open receipt review",
                    json!({"kind":"inspect","record":record}),
                )),
            );
        }
        return;
    }
    if document["kind"] == "event_preview" {
        crate::native::edit_mode::label(
            world,
            parent,
            "Unsigned event preview. Review the public content before signing.",
            13.0,
        );
        forms::review(world, parent, &document["value"]["event"], 0);
        button(
            world,
            parent,
            owner,
            "Sign reviewed event…",
            Command::Open(publish_event_form(&person, &document["value"])),
        );
        return;
    }
    if document["kind"] == "participation_request" {
        let terms = &document["value"];
        receipt_delivery::options(
            world,
            owner,
            parent,
            &person,
            json!({"kind":"participation","transfer":terms["transfer"],"expected_revision":terms["terms"]["agreement_revision"],"party":terms["party"]}),
        );
        crate::native::edit_mode::label(
            world,
            parent,
            "Private unsigned participation request. Send it only to the named Organ for approval before activation.",
            13.0,
        );
        forms::review(world, parent, &document["value"], 0);
        if document["value"]["terms"]["organ"] == world.get::<View>(owner).unwrap().context["organ"]
        {
            button(
                world,
                parent,
                owner,
                "Review Organ approval…",
                Command::Open(participation_form(&person, &document["value"])),
            );
        }
        if export_ready {
            button(
                world,
                parent,
                owner,
                "Copy private participation request",
                Command::CopyReceipt,
            );
        }
        return;
    }
    crate::native::edit_mode::label(
        world,
        parent,
        &format!(
            "{} · version {}",
            model::human(&text(&receipt, "state")),
            text(&receipt, "version")
        ),
        14.0,
    );
    receipt_browsing::controls(world, owner, parent, &person, &receipt);
    if document["kind"] == "proof_fragment" {
        crate::native::edit_mode::label(
            world,
            parent,
            "Authenticated signed fragment. Source history checks its permitted local predecessors and Need proofs; the fragment alone does not establish complete history.",
            13.0,
        );
    }
    if receipt["historical"] == true {
        crate::native::edit_mode::label(
            world,
            parent,
            "Saved version: read-only. Approval and publication controls use the current Record.",
            13.0,
        );
        let request = json!({"kind":"export_version","record":receipt["record"],"version":receipt["version"]});
        button(
            world,
            parent,
            owner,
            "Prepare exact-version export",
            Command::Open(action_form(
                &person,
                "Export original saved version",
                request,
            )),
        );
        button(
            world,
            parent,
            owner,
            "Save exact-version file…",
            Command::ExportReceiptFile,
        );
        if export_ready {
            button(
                world,
                parent,
                owner,
                "Copy exact-version document",
                Command::CopyReceipt,
            );
        }
        button(
            world,
            parent,
            owner,
            "Open current receipt",
            Command::Open(action_form(
                &person,
                "Open current receipt",
                json!({"kind":"inspect","record":receipt["record"]}),
            )),
        );
        return;
    }
    receipt_delivery::options(
        world,
        owner,
        parent,
        &person,
        json!({"kind":"document","record":receipt["record"],"expected_version":receipt["version"]}),
    );
    button(
        world,
        parent,
        owner,
        "Open Record and visibility",
        Command::Record(text(&receipt, "record")),
    );
    if matches!(
        document["kind"].as_str(),
        Some("publication" | "proof_fragment")
    ) {
        crate::native::trust_castle::evidence_serving_controls(
            world,
            parent,
            &text(&receipt, "record"),
            nucleus::trust_analysis::serving::SourceKind::Receipt,
        );
    }
    let scope = if document["kind"] == "review" {
        &document["value"]["scope"]
    } else {
        &document["value"]["publication"]["scope"]
    };
    if !scope.is_null() {
        button(
            world,
            parent,
            owner,
            "Source outcome history",
            Command::Open(receipt_history::history_form(&person, scope)),
        );
        source_options(world, owner, parent, &person, &receipt, scope);
        crate::native::edit_mode::label(
            world,
            parent,
            if scope["party_attribution"] == "original_organ_authority" {
                "Original Organ authority recorded during activation. This does not confirm delivery or establish a Person's contribution history."
            } else {
                "Later Organ endorsement. This does not establish historical Organ representation or a Person's contribution history."
            },
            13.0,
        );
        let mut summary = scope.clone();
        if let Some(object) = summary.as_object_mut() {
            object.remove("source");
        }
        if let Some(terms) = summary["upfront_terms"].as_object_mut() {
            terms.remove("nonce");
        }
        if let Some(expiry) = scope["expires_at"]
            .as_i64()
            .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
        {
            summary["expires_at"] = json!(expiry.to_rfc3339());
        }
        forms::review(world, parent, &summary, 0);
    }
    if document["kind"] == "review" {
        let approvals = array(&document["value"], "approvals");
        let upfront = serde_json::from_value::<nucleus::transfer::receipt::Scope>(scope.clone())
            .map(|scope| {
                scope
                    .upfront_roles()
                    .into_iter()
                    .map(|role| model::display(&json!(role)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let covered = approvals
            .iter()
            .filter_map(|approval| approval["role"].as_str().map(str::to_owned))
            .chain(upfront.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        let personal = array(&document["value"], "person_approvals");
        let targets = array(scope, "public_persons");
        if !targets.is_empty() {
            crate::native::edit_mode::label(
                world,
                parent,
                &format!(
                    "{} of {} personal identity consents. Public receipts require each selected Person's signature.",
                    personal.len(),
                    targets.len()
                ),
                13.0,
            );
            for target in targets {
                if target["person"] != person
                    || personal
                        .iter()
                        .any(|approval| approval["role"] == target["role"])
                {
                    continue;
                }
                match person_approval_form(&person, &receipt, scope, &text(target, "role")) {
                    Ok(form) => button(
                        world,
                        parent,
                        owner,
                        "Consent to identifying me…",
                        Command::Open(form),
                    ),
                    Err(error) => crate::native::edit_mode::label(world, parent, &error, 12.0),
                };
            }
        }
        crate::native::edit_mode::label(
            world,
            parent,
            &format!(
                "{} of 3 role authorizations; {} use signed upfront terms. The origin's occurrence attestation remains required.",
                covered.len(),
                upfront.len()
            ),
            13.0,
        );
        let local = world.get::<View>(owner).unwrap().context["organ"].clone();
        for (role, organ) in [
            ("origin", &scope["source"]["origin"]),
            ("giver", &scope["giver"]),
            ("receiver", &scope["receiver"]),
        ] {
            if *organ != local || covered.contains(role) {
                continue;
            }
            button(
                world,
                parent,
                owner,
                &format!("Approve as {role}…"),
                Command::Open(approval_form(&person, &receipt, scope, role)),
            );
        }
    }
    if public_document(document) || approved_review(document) {
        for (label, source) in [
            (
                "Review outcome event…",
                json!({"kind":"outcome","record":""}),
            ),
            (
                "Review settlement event…",
                json!({"kind":"settlement","fact":"","supersedes":null}),
            ),
            (
                "Review settlement status…",
                json!({"kind":"settlement_status","fact":"","supersedes":null}),
            ),
            (
                "Review applied cancellation…",
                json!({"kind":"cancellation","fact":"","supersedes":null}),
            ),
            (
                "Review application compensation…",
                json!({"kind":"compensation","fact":"","supersedes":null}),
            ),
            (
                "Review dispute event…",
                json!({"kind":"dispute","fact":"","supersedes":null}),
            ),
        ] {
            let stage = match source["kind"].as_str() {
                Some("settlement_status") => Some("settlement_status"),
                Some("cancellation") => Some("cancelled"),
                Some("compensation") => Some("compensated"),
                _ => None,
            };
            if stage.is_some_and(|stage| !array(scope, "stages").contains(&json!(stage))) {
                continue;
            }
            button(
                world,
                parent,
                owner,
                label,
                Command::Open(event_form(&person, &receipt, source)),
            );
        }
    }
    let mut export = action_form(
        &person,
        "Export receipt document",
        json!({"kind":"export","record":receipt["record"]}),
    );
    export.review["Sharing"] = json!(if document["kind"] == "review" {
        "Share this private draft with the named approving Organs for review. It is not an accepted outcome receipt."
    } else {
        "Export only the minimal signed document, without private Record mappings or local audit Facts."
    });
    button(
        world,
        parent,
        owner,
        "Prepare export",
        Command::Open(export),
    );
    button(
        world,
        parent,
        owner,
        "Save receipt file…",
        Command::ExportReceiptFile,
    );
    if export_ready {
        button(
            world,
            parent,
            owner,
            "Copy receipt document",
            Command::CopyReceipt,
        );
    }
}
