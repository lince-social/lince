use super::*;

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let state = (
        view.context.clone(),
        view.person.clone(),
        view.contact.clone(),
        view.contact_organ,
        view.issuer_organ,
        view.public,
        view.readers.clone(),
        view.term.clone(),
        view.revision.clone(),
        view.withdrawn,
        view.as_organ,
        view.import.clone(),
        view.anchor.clone(),
        view.anchor_organ,
        view.depth.clone(),
        view.unilateral,
        view.excluded.clone(),
        view.rules.clone(),
        view.review.clone(),
        view.result.clone(),
        view.notice.clone(),
        view.pending.is_some(),
    );
    panel::clear(world, owner);
    let (
        context,
        person,
        contact,
        contact_organ,
        issuer_organ,
        public,
        readers,
        term,
        revision,
        withdrawn,
        as_organ,
        import,
        anchor,
        anchor_organ,
        depth,
        unilateral,
        excluded,
        rules,
        review,
        result,
        notice,
        pending,
    ) = state;
    label(
        world,
        owner,
        "Signed contacts help explain permitted evidence paths. They do not prove reliability, independent people or physical delivery.",
    );
    label(world, owner, &notice);
    if pending {
        label(world, owner, "Waiting for the Cell…");
    }
    panel::button(
        world,
        owner,
        owner,
        "Refresh identity choices",
        Command::Context,
    );
    if context["person"].is_null() {
        field(world, owner, owner, Key::Person, "Acting Person", &person);
    } else {
        label(world, owner, &format!("Acting Person: {person}"));
    }
    if context["person"].is_null() {
        for profile in context["profiles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|profile| profile["kind"] == "person")
            .take(16)
        {
            panel::button(
                world,
                owner,
                owner,
                &format!("Use {}", profile["head"].as_str().unwrap_or("Person")),
                Command::Person(profile["uid"].as_str().unwrap_or_default().into()),
            );
        }
    }
    if world
        .get::<View>(owner)
        .is_some_and(|view| view.roots.focused)
    {
        root_controls::render(world, owner);
        return;
    }
    if world
        .get::<View>(owner)
        .is_some_and(|view| view.serving.focused)
    {
        serving_controls::render(world, owner);
        return;
    }
    if context["organ_admin"] == true {
        panel::button(
            world,
            owner,
            owner,
            if as_organ {
                "Reader: Organ grants (administration)"
            } else {
                "Reader: Person grants"
            },
            Command::Toggle("as_organ"),
        );
    }
    let row = panel::row(world, owner);
    panel::button(
        world,
        row,
        owner,
        "My retained disclosures",
        Command::List(None),
    );
    panel::button(world, row, owner, "Clear private results", Command::Clear);
    label(world, owner, "Disclose a contact");
    if context["organ_admin"] == true {
        panel::button(
            world,
            owner,
            owner,
            if issuer_organ {
                "Issuer: this Organ"
            } else {
                "Issuer: my Person"
            },
            Command::Toggle("issuer"),
        );
    }
    panel::button(
        world,
        owner,
        owner,
        if contact_organ {
            "Contact kind: Organ"
        } else {
            "Contact kind: Person"
        },
        Command::Toggle("contact"),
    );
    field(
        world,
        owner,
        owner,
        Key::Contact,
        "Contact profile Record",
        &contact,
    );
    for profile in context["profiles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|profile| profile["uid"].as_str() != Some(&person))
        .take(12)
    {
        let uid = profile["uid"].as_str().unwrap_or_default().to_owned();
        let identity = if profile["kind"] == "organ" {
            Identity::Organ(uid)
        } else {
            Identity::Person(uid)
        };
        panel::button(
            world,
            owner,
            owner,
            &format!("Choose {}", profile["head"].as_str().unwrap_or("profile")),
            Command::Contact(identity),
        );
    }
    panel::button(
        world,
        owner,
        owner,
        if public {
            "Publication audience: public"
        } else {
            "Publication audience: selected (private)"
        },
        Command::Toggle("public"),
    );
    if !public {
        field(
            world,
            owner,
            owner,
            Key::Readers,
            "Other viewers: person:r_… or organ:r_… (comma separated)",
            &readers,
        );
        label(
            world,
            owner,
            "Your acting Person is included explicitly. Record visibility remains private until separately shared.",
        );
    }
    field(
        world,
        owner,
        owner,
        Key::Term,
        "Term in days (1–366)",
        &term,
    );
    field(
        world,
        owner,
        owner,
        Key::Revision,
        "Current issuer revision (0 for a first disclosure)",
        &revision,
    );
    panel::button(
        world,
        owner,
        owner,
        if withdrawn {
            "Statement: withdraw this relationship"
        } else {
            "Statement: I know this contact"
        },
        Command::Toggle("withdrawn"),
    );
    panel::button(
        world,
        owner,
        owner,
        "Preview exact disclosure",
        Command::Preview,
    );
    if let Some((review, _)) = review {
        label(
            world,
            owner,
            &format!(
                "Issuer: {} · Contact: {} · revision {} · expires {}",
                review.document.statement.issuer.uid(),
                review.document.statement.contact.uid(),
                review.document.statement.revision,
                review.document.statement.expires_at
            ),
        );
        label(
            world,
            owner,
            &format!(
                "Audience: {} · signing key {}",
                serde_json::to_string(&review.document.statement.audience).unwrap_or_default(),
                review.document.key_id
            ),
        );
        panel::button(
            world,
            owner,
            owner,
            "Sign and retain reviewed disclosure",
            Command::Confirm,
        );
    }
    label(world, owner, "Import signed evidence");
    if let Ok(document) = nucleus::trust_analysis::contacts::Document::parse(import.as_bytes()) {
        if matches!(document.statement.issuer, Identity::Person(_)) {
            label(
                world,
                owner,
                "A key inside a disclosure does not establish identity. Receive the Person profile and compare the key or fingerprint independently before recognizing it.",
            );
            panel::button(
                world,
                owner,
                owner,
                "Compare candidate Person signing key",
                Command::PreviewIdentity,
            );
        } else {
            label(
                world,
                owner,
                "Recognize Organ roots independently through normal Organ identity controls.",
            );
        }
    }
    field(
        world,
        owner,
        owner,
        Key::Import,
        "Portable contact disclosure",
        &import,
    );
    panel::button(
        world,
        owner,
        owner,
        "Verify and retain imported disclosure",
        Command::Import,
    );
    label(world, owner, "Search my permitted contact evidence");
    panel::button(
        world,
        owner,
        owner,
        if anchor_organ {
            "Anchor kind: Organ"
        } else {
            "Anchor kind: Person"
        },
        Command::Toggle("anchor"),
    );
    field(
        world,
        owner,
        owner,
        Key::Anchor,
        "Starting profile Record (does not change reader permissions)",
        &anchor,
    );
    field(
        world,
        owner,
        owner,
        Key::Depth,
        "Maximum contacts away (1–6)",
        &depth,
    );
    panel::button(
        world,
        owner,
        owner,
        if unilateral {
            "Include labelled unilateral paths"
        } else {
            "Mutual disclosures only"
        },
        Command::Toggle("unilateral"),
    );
    field(
        world,
        owner,
        owner,
        Key::Excluded,
        "Exclude everywhere: typed profile identities",
        &excluded,
    );
    for (index, rule) in rules.iter().enumerate() {
        rule_controls(world, owner, index, rule);
    }
    for (label, scope, exclusion) in [
        ("Add target inclusion", RuleScope::Target, false),
        ("Add target exclusion", RuleScope::Target, true),
        ("Add intermediary inclusion", RuleScope::Intermediary, false),
        ("Add intermediary exclusion", RuleScope::Intermediary, true),
        ("Add issuer inclusion", RuleScope::Issuer, false),
        ("Add issuer exclusion", RuleScope::Issuer, true),
        ("Add counterparty inclusion", RuleScope::Counterparty, false),
        ("Add counterparty exclusion", RuleScope::Counterparty, true),
        ("Add query endpoint inclusion", RuleScope::Endpoint, false),
        ("Add query endpoint exclusion", RuleScope::Endpoint, true),
    ] {
        panel::button(
            world,
            owner,
            owner,
            label,
            Command::AddRule(scope, exclusion),
        );
    }
    label(
        world,
        owner,
        "Conditions combine with OR; criteria within one condition use AND. Exclusions win. Proximity is a local contact setting, separate from hops and reliability.",
    );
    panel::button(
        world,
        owner,
        owner,
        "Find permitted observed paths",
        Command::Search,
    );
    analysis::render(world, owner);
    remote_controls::render(world, owner);
    root_controls::render(world, owner);
    serving_controls::render(world, owner);
    custody_controls::render(world, owner);
    profile_claims::render(world, owner);
    claims::render(world, owner);
    recipe_controls::render(world, owner);
    match result["kind"].as_str() {
        Some("identity")
            if result["value"]["kind"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("person_")) =>
        {
            let value = &result["value"];
            label(
                world,
                owner,
                &format!("Person signing identity: {}", value["key"]),
            );
            label(
                world,
                owner,
                &format!("Compare independently: {}", value["fingerprint"]),
            );
            if value["kind"] == "person_key_preview" && context["organ_admin"] == true {
                label(
                    world,
                    owner,
                    "Recognizing this exact key does not endorse reliability or physical delivery.",
                );
                panel::button(
                    world,
                    owner,
                    owner,
                    "Accept independently compared Person key",
                    Command::AcceptIdentity,
                );
            } else if value["kind"] == "person_key_accepted" {
                label(
                    world,
                    owner,
                    "Signing identity recognized. Verify and import the disclosure separately.",
                );
            }
        }
        Some("contact_list") => {
            for entry in result["entries"].as_array().into_iter().flatten() {
                label(
                    world,
                    owner,
                    &format!(
                        "{} · {} → {} · revision {} · current visible copy: {}",
                        entry["state"].as_str().unwrap_or("proof"),
                        entry["document"]["statement"]["issuer"]["uid"]
                            .as_str()
                            .unwrap_or_default(),
                        entry["document"]["statement"]["contact"]["uid"]
                            .as_str()
                            .unwrap_or_default(),
                        entry["document"]["statement"]["revision"],
                        entry["current_revision"].is_number()
                    ),
                );
                let record = entry["record"].as_str().unwrap_or_default().to_owned();
                panel::button(
                    world,
                    owner,
                    owner,
                    "Inspect proof",
                    Command::Inspect(record.clone()),
                );
                panel::button(
                    world,
                    owner,
                    owner,
                    "Prepare export",
                    Command::Export(record),
                );
                let issuer = &entry["document"]["statement"]["issuer"];
                if entry["current_revision"] == entry["document"]["statement"]["revision"]
                    && ((issuer["kind"] == "person" && issuer["uid"].as_str() == Some(&person))
                        || (issuer["kind"] == "organ"
                            && issuer["uid"] == context["organ"]
                            && context["organ_admin"] == true))
                {
                    panel::button(
                        world,
                        owner,
                        owner,
                        "Revise disclosure",
                        Command::Revise(entry["document"].clone(), false),
                    );
                    panel::button(
                        world,
                        owner,
                        owner,
                        "Review withdrawal",
                        Command::Revise(entry["document"].clone(), true),
                    );
                }
            }
            if let Some(next) = result["next"].as_str() {
                panel::button(
                    world,
                    owner,
                    owner,
                    "Older disclosures",
                    Command::List(Some(next.into())),
                );
            }
        }
        Some("contact_document") => {
            if let Some(record) = result["record"].as_str() {
                evidence_serving_controls(
                    world,
                    owner,
                    record,
                    nucleus::trust_analysis::serving::SourceKind::Contact,
                );
                panel::button(
                    world,
                    owner,
                    owner,
                    "Open disclosure Record and visibility",
                    Command::Record(record.into()),
                );
            }
            label(
                world,
                owner,
                &format!(
                    "Record {} · authority at last check: {}",
                    result["record"].as_str().unwrap_or_default(),
                    result["authority_current"]
                ),
            );
            label(
                world,
                owner,
                &serde_json::to_string_pretty(&result["document"]).unwrap_or_default(),
            );
            if result["export"].is_string() {
                panel::button(
                    world,
                    owner,
                    owner,
                    "Copy reviewed portable disclosure",
                    Command::Copy,
                );
            }
        }
        Some("contact_paths") => {
            label(
                world,
                owner,
                "Observed permitted paths; absence does not establish that no relationship exists.",
            );
            if result["partial"] == true {
                label(
                    world,
                    owner,
                    "Partial results: a depth, identity, proof, byte or time bound was reached, or evidence is unavailable.",
                );
            }
            for path in result["result"]["paths"].as_array().into_iter().flatten() {
                let identities = path["identities"].as_array().cloned().unwrap_or_default();
                let names = identities
                    .iter()
                    .map(|identity| {
                        let uid = identity["uid"].as_str().unwrap_or_default();
                        context["profiles"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|profile| profile["uid"].as_str() == Some(uid))
                            .and_then(|profile| profile["head"].as_str())
                            .filter(|head| !head.is_empty())
                            .unwrap_or(uid)
                    })
                    .collect::<Vec<_>>()
                    .join(" → ");
                label(
                    world,
                    owner,
                    &format!(
                        "{} contacts away · {}{}",
                        identities.len().saturating_sub(1),
                        names,
                        if path["unilateral"] == true {
                            " · includes unilateral evidence"
                        } else {
                            " · mutual disclosures"
                        }
                    ),
                );
                for proof in path["proofs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|hop| hop.as_array().into_iter().flatten())
                {
                    if let Some(record) = proof
                        .as_str()
                        .and_then(|proof| result["proof_records"][proof].as_str())
                    {
                        panel::button(
                            world,
                            owner,
                            owner,
                            "Inspect signed hop proof",
                            Command::Inspect(record.into()),
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

pub(super) fn rule_controls(world: &mut World, owner: Entity, index: usize, rule: &Rule) {
    let scope = match rule.scope {
        RuleScope::Target => "Path target",
        RuleScope::Intermediary => "Path intermediary",
        RuleScope::Issuer => "Original evidence issuer",
        RuleScope::Counterparty => "Transfer counterparty",
        RuleScope::Endpoint => "Remote query endpoint",
        RuleScope::Serving => "Evidence query reader",
        RuleScope::CustodySender => "Custody sender",
        RuleScope::CustodyReader => "Custody reader",
        RuleScope::CustodyForwardPeer => "Evidence forwarding peer",
    };
    label(
        world,
        owner,
        &format!(
            "{} {} condition {}",
            scope,
            if rule.exclusion { "exclude" } else { "include" },
            index + 1
        ),
    );
    field(
        world,
        owner,
        owner,
        Key::RuleIds(index),
        "Typed identities (empty means any)",
        &rule.identities,
    );
    panel::button(
        world,
        owner,
        owner,
        match rule.known {
            1 => "Known contacts",
            2 => "Unknown contacts",
            _ => "Known or unknown contacts",
        },
        Command::Known(index),
    );
    field(
        world,
        owner,
        owner,
        Key::RuleLower(index),
        "Minimum Organ proximity (optional)",
        &rule.lower,
    );
    panel::button(
        world,
        owner,
        owner,
        if rule.lower_inclusive {
            "Minimum included"
        } else {
            "Strictly above minimum"
        },
        Command::Bound(index, true),
    );
    field(
        world,
        owner,
        owner,
        Key::RuleUpper(index),
        "Maximum Organ proximity (optional)",
        &rule.upper,
    );
    panel::button(
        world,
        owner,
        owner,
        if rule.upper_inclusive {
            "Maximum included"
        } else {
            "Strictly below maximum"
        },
        Command::Bound(index, false),
    );
    panel::button(
        world,
        owner,
        owner,
        "Remove condition",
        Command::RemoveRule(index),
    );
}
