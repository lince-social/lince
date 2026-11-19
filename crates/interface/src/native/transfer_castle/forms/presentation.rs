use super::*;
use summary::Summary;

pub(super) struct Prepared {
    title: String,
    activity: bool,
    counteroffer: bool,
    step: Option<usize>,
    public_history: Option<bool>,
    effects: Option<usize>,
    fields: Vec<fields::Field>,
    footer: Footer,
}

enum Footer {
    Composer {
        step: usize,
        disclosure: Option<Disclosure>,
        removals: Option<(&'static str, &'static str, usize)>,
        review: Option<(Summary, Vec<(String, String)>)>,
    },
    Preview,
    Application {
        review: Option<Summary>,
        delta: Summary,
    },
    Journey(Summary),
    Receipt {
        review: Summary,
        participation: bool,
        approved: bool,
    },
    Other(Summary),
}

pub(super) struct Disclosure {
    people: Vec<String>,
    review: Option<Summary>,
}

impl Disclosure {
    pub(super) fn prepare(data: &Value, viewer: Option<&Option<String>>, draft: bool) -> Self {
        let mut people: std::collections::BTreeSet<String> = if draft {
            std::iter::once(text(data, "creator"))
                .chain(array(data, "invitees").iter().map(model::display))
                .collect()
        } else {
            array(data, "parties")
                .iter()
                .map(|party| text(party, "actor"))
                .collect()
        };
        for promise in array(data, "promises") {
            if let Ok(item) =
                nucleus::transfer::disclosure::TransferItem::deserialize(&promise["item"])
            {
                for (_, audience) in item.disclosure.fields() {
                    people.extend(audience.people.iter().cloned());
                }
            }
        }
        Self {
            people: people
                .into_iter()
                .filter(|person| !person.is_empty())
                .collect(),
            review: viewer.map(|viewer| {
                Summary::new(
                    &model::disclosure_preview(data, viewer.as_deref(), draft),
                    0,
                )
            }),
        }
    }

    pub(super) fn render(self, world: &mut World, owner: Entity, parent: Entity) {
        crate::native::edit_mode::label(
            world,
            parent,
            "Field preview for people allowed to open this Transfer",
            13.0,
        );
        let controls = row(world, parent);
        button(
            world,
            controls,
            owner,
            "View as public",
            Command::ViewAs(None),
        );
        for person in self.people {
            button(
                world,
                controls,
                owner,
                &format!("View as {person}"),
                Command::ViewAs(Some(person)),
            );
        }
        if let Some(review) = self.review {
            review.render(world, parent);
        }
    }
}

pub(super) fn prepare(form: &Form, view: &View) -> Prepared {
    let _span = nucleus::performance::Span::new("transfer-form-view-preparation");
    let fields = fields::prepare(form, view);
    let footer = if let Some(step) = form.step {
        Footer::Composer {
            step,
            disclosure: (step == 4 && form.mode != "counteroffer-transfer")
                .then(|| Disclosure::prepare(&form.data, view.disclosure_preview.as_ref(), true)),
            removals: match step {
                2 => Some(("promises", "promise", array(&form.data, "promises").len())),
                3 => Some((
                    "dependencies",
                    "dependency",
                    array(&form.data, "dependencies").len(),
                )),
                _ => None,
            },
            review: (step == 4).then(|| {
                (
                    Summary::new(&model::review_draft(form), 0),
                    array(&form.data, "promises")
                        .iter()
                        .map(|promise| {
                            (
                                format!("Simulate {}", text(&promise["item"], "title")),
                                text(promise, "uid"),
                            )
                        })
                        .collect(),
                )
            }),
        }
    } else if form.mode != "reviewed"
        && matches!(
            model::display_ref(&form.data["action"]).as_ref(),
            "settle-transfer-occurrence" | "complete-transfer-occurrence-claims-bulk"
        )
    {
        Footer::Preview
    } else if form.data["action"] == "apply-transfer-application" {
        Footer::Application {
            review: form
                .review
                .is_array()
                .then(|| Summary::new(&form.review, 0)),
            delta: Summary::object([
                ("Changes to my Record", &form.data["expected_local_delta"]),
                (
                    "Earlier changes for this delivery",
                    &form.data["expected_local_cumulative_before"],
                ),
                ("Person", &form.data["person"]),
            ]),
        }
    } else if form.data["action"] == "record-journey-progress" {
        Footer::Journey(Summary::new(&form.review, 0))
    } else if form.data["action"] == "transfer-receipt" {
        let review = if let Some(object) = form.review.as_object() {
            Summary::object(
                object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "Reviewed request")
                    .map(|(key, value)| (key.as_str(), value)),
            )
        } else {
            Summary::new(&form.review, 0)
        };
        Footer::Receipt {
            review,
            participation: form.data["request"]["kind"] == "authorize_participation",
            approved: form.mode == "participation-reviewed",
        }
    } else {
        let paths: HashSet<_> = form
            .fields
            .iter()
            .filter_map(|field| field.path.strip_prefix('/'))
            .collect();
        Footer::Other(Summary::object(
            form.data
                .as_object()
                .into_iter()
                .flat_map(|object| object.iter())
                .filter(|(key, _)| {
                    !matches!(key.as_str(), "request_id" | "action")
                        && !paths.contains(key.as_str())
                })
                .map(|(key, value)| (key.as_str(), value)),
        ))
    };
    Prepared {
        title: form.title.clone(),
        activity: super::super::workflows::ACTIVITIES.contains(&form.preset.as_str()),
        counteroffer: form.mode == "counteroffer-transfer",
        step: form.step,
        public_history: (form.step == Some(3)).then(|| form.data["public_history"].is_object()),
        effects: (form.data["action"] == "set-transfer-private-application-policy")
            .then(|| array(&form.data, "effects").len()),
        fields,
        footer,
    }
}

pub(super) fn render(world: &mut World, owner: Entity, parent: Entity, prepared: Prepared) {
    let Prepared {
        title,
        activity,
        counteroffer,
        step,
        public_history,
        effects,
        fields,
        footer,
    } = prepared;
    let heading = row(world, parent);
    crate::native::edit_mode::label(world, heading, &title, 20.0);
    if activity {
        crate::native::edit_mode::label(
            world,
            parent,
            "Choose the contribution, its unit and who provides it. A Need may request attention or participation so you can contribute. Private Record effects are reviewed separately; this draft grants no agreement, delivery or sharing.",
            13.0,
        );
    }
    if counteroffer {
        crate::native::edit_mode::label(
            world,
            parent,
            "Unavailable fields and private source settings stay unchanged.",
            13.0,
        );
    }
    button(world, heading, owner, "Cancel", Command::Cancel);
    if let Some(step) = step {
        let steps = row(world, parent);
        for (index, label) in model::STEPS.iter().enumerate() {
            button(
                world,
                steps,
                owner,
                &format!(
                    "{}{}. {label}",
                    if step == index { "● " } else { "" },
                    index + 1
                ),
                Command::Step(index),
            );
        }
    }
    if let Some(proposed) = public_history {
        crate::native::edit_mode::label(
            world,
            parent,
            "Public-history terms are optional and require agreement. They cover receipt fields and retention; live location and private bookkeeping stay separate.",
            13.0,
        );
        if proposed {
            button(
                world,
                parent,
                owner,
                "Remove proposed public-history terms",
                Command::Remove("public_history".into(), 0),
            );
        } else {
            button(
                world,
                parent,
                owner,
                "Add optional public-history terms",
                Command::Add("public_history".into()),
            );
        }
    }
    if let Some(count) = effects {
        crate::native::edit_mode::label(world, parent, "This outcome meets…", 16.0);
        crate::native::edit_mode::label(
            world,
            parent,
            "Quantity changes add or deduct stock. Fulfilment meets an outstanding Need up to zero. These choices are private.",
            13.0,
        );
        let controls = row(world, parent);
        button(
            world,
            controls,
            owner,
            "+ Record or Need",
            Command::Add("effects".into()),
        );
        for index in 0..count {
            button(
                world,
                controls,
                owner,
                &format!("Remove effect {}", index + 1),
                Command::Remove("effects".into(), index),
            );
        }
    }
    fields::render(world, owner, parent, fields);
    match footer {
        Footer::Composer {
            step,
            disclosure,
            removals,
            review,
        } => {
            if let Some(disclosure) = disclosure {
                disclosure.render(world, owner, parent);
            }
            if let Some((key, label, count)) = removals {
                let controls = row(world, parent);
                button(
                    world,
                    controls,
                    owner,
                    if step == 2 {
                        "+ Promise"
                    } else {
                        "+ Dependency"
                    },
                    Command::Add(key.into()),
                );
                for index in 0..count {
                    button(
                        world,
                        controls,
                        owner,
                        &format!("Remove {label} {}", index + 1),
                        Command::Remove(key.into(), index),
                    );
                }
            }
            if let Some((review, simulations)) = review {
                review.render(world, parent);
                for (label, uid) in simulations {
                    button(world, parent, owner, &label, Command::Simulate(uid));
                }
            }
            let controls = row(world, parent);
            if step > 0 {
                button(world, controls, owner, "Back", Command::Step(step - 1));
            }
            if step < 4 {
                button(world, controls, owner, "Continue", Command::Step(step + 1));
            } else {
                button(
                    world,
                    controls,
                    owner,
                    "Sign and save transfer",
                    Command::Submit,
                );
            }
        }
        Footer::Preview => {
            button(world, parent, owner, "Refresh review", Command::Preview);
            render_preview(world, owner);
        }
        Footer::Application { review, delta } => {
            if let Some(review) = review {
                review.render(world, parent);
            }
            delta.render(world, parent);
            button(
                world,
                parent,
                owner,
                "Apply all reviewed changes",
                Command::Submit,
            );
        }
        Footer::Journey(review) => {
            review.render(world, parent);
            button(world, parent, owner, &title, Command::Submit);
        }
        Footer::Receipt {
            review,
            participation,
            approved,
        } => {
            review.render(world, parent);
            if participation {
                button(
                    world,
                    parent,
                    owner,
                    "Review current request",
                    Command::ReviewParticipation,
                );
                if approved {
                    button(
                        world,
                        parent,
                        owner,
                        "Approve this participation",
                        Command::Submit,
                    );
                }
            } else {
                button(world, parent, owner, "Confirm", Command::Submit);
            }
        }
        Footer::Other(review) => {
            review.render(world, parent);
            button(world, parent, owner, "Confirm", Command::Submit);
        }
    }
}
