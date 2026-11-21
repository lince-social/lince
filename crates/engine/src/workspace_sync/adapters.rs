// non-lean-yet
// Scope: fn action_change, fn component_records, fn visit_component_records, fn component_contains.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 31, 34, 172, 304,
//   455, 478, 480, 511, 512.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 3.
// - [Clippy probe: clippy::wildcard_imports] Use explicit imports while preserving framework and
//   test conventions. Sites 1.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   3, 103, 166, 179, 338.

use super::*;

pub fn action_change(action: &Value) -> Result<Change, EngineError> {
    if action.get("action").and_then(Value::as_str) == Some("workspace-record") {
        if action.as_object().is_none_or(|fields| {
            fields
                .keys()
                .any(|key| !matches!(key.as_str(), "action" | "record" | "changes"))
        }) {
            return Err(invalid("Unknown shared control field"));
        }
        let record = action
            .get("record")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Choose a Record UID"))?
            .to_owned();
        let mut value = action.get("changes").cloned().unwrap_or(Value::Null);
        if let Some(fields) = value.as_object_mut() {
            for key in ["assert", "retract", "assign", "unassign"] {
                fields.entry(key).or_insert_with(|| json!([]));
            }
        }
        let changes = serde_json::from_value(value)?;
        let change = Change::ChangeRecord { record, changes };
        change_valid(&change)?;
        return Ok(change);
    }
    let parsed = crate::actions::Action::deserialize(action)?;
    let quantity = match &parsed {
        crate::actions::Action::SetQuantityExact { target, amount } => {
            Some((target.clone(), format!("={amount}")))
        }
        crate::actions::Action::AddQuantityExact { target, delta } => {
            Some((target.clone(), format!("+={delta}")))
        }
        _ => None,
    };
    if let Some((record, quantity)) = quantity {
        let change = Change::ChangeRecord {
            record,
            changes: crate::area_transition::RecordChanges {
                quantity: Some(quantity),
                ..Default::default()
            },
        };
        change_valid(&change)?;
        return Ok(change);
    }
    let (record, edit) = match parsed {
        crate::actions::Action::EditRecordText { target, head, body } => {
            (target, RecordEdit::Text { head, body })
        }
        crate::actions::Action::SetSlug { target, slug } => (target, RecordEdit::Slug { slug }),
        crate::actions::Action::SetUnit { target, unit } => (target, RecordEdit::Unit { unit }),
        crate::actions::Action::SetExtension {
            target,
            namespace,
            fds,
        } => (
            target,
            RecordEdit::Extension {
                namespace,
                value: fds,
            },
        ),
        crate::actions::Action::AssertRecord {
            subject,
            predicate,
            object,
            quantity,
            unit,
        } => (
            subject,
            RecordEdit::Assert {
                predicate,
                object,
                quantity,
                unit,
            },
        ),
        crate::actions::Action::RefineAssertion {
            subject,
            predicate,
            object,
        } => (subject, RecordEdit::Refine { predicate, object }),
        crate::actions::Action::SetIdentity { subject, predicate } => {
            (subject, RecordEdit::Identity { predicate })
        }
        _ => {
            return Err(invalid(
                "This action has no transactional shared-workspace adapter. Use its dedicated controls.",
            ));
        }
    };
    let change = Change::EditRecord {
        record,
        edits: vec![edit],
    };
    change_valid(&change)?;
    Ok(change)
}

pub fn component_records(component: &Component) -> BTreeSet<String> {
    let mut records = BTreeSet::new();
    let _: ControlFlow<()> = visit_component_records(component, &mut |record| {
        records.insert(record.to_owned());
        ControlFlow::Continue(())
    });
    records
}

pub(super) fn visit_component_records<B>(
    component: &Component,
    visit: &mut impl FnMut(&str) -> ControlFlow<B>,
) -> ControlFlow<B> {
    fn origins<B>(
        component: &Component,
        visit: &mut impl FnMut(&str) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        if let Component::Composition { composition } = component {
            if let Some(origin) = &composition.origin {
                visit(&origin.agent)?;
                visit(&origin.thread)?;
            }
            for part in &composition.parts {
                origins(&part.component, visit)?;
            }
        }
        ControlFlow::Continue(())
    }

    component.visit_records(visit)?;
    origins(component, visit)?;
    visit_controls(component, |_, _, action| {
        if let Ok(change) = action_change(action) {
            visit_change_records(&change, visit)?;
        }
        ControlFlow::Continue(())
    })
}

pub(super) fn component_contains(component: &Component, target: &str) -> bool {
    visit_component_records(component, &mut |record| {
        if record == target {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })
    .is_break()
}

pub(super) fn size(component: &Component) -> usize {
    match component {
        Component::Composition { composition } => {
            1 + composition
                .parts
                .iter()
                .map(|part| size(&part.component))
                .sum::<usize>()
        }
        _ => 1,
    }
}

pub fn controls(component: &Component) -> Vec<(Vec<String>, Option<String>, Value)> {
    let mut output = Vec::new();
    let _: ControlFlow<()> = visit_controls(component, |path, event, action| {
        output.push((
            path.iter().map(|part| (*part).to_owned()).collect(),
            event.map(str::to_owned),
            action.clone(),
        ));
        ControlFlow::Continue(())
    });
    output
}

pub fn visit_controls<'a, B>(
    component: &'a Component,
    mut visit: impl FnMut(&[&'a str], Option<&'a str>, &'a Value) -> ControlFlow<B>,
) -> ControlFlow<B> {
    walk_controls(component, &mut Vec::new(), None, &mut visit)
}

fn walk_controls<'a, B>(
    component: &'a Component,
    path: &mut Vec<&'a str>,
    selected_path: Option<&[String]>,
    visit: &mut impl FnMut(&[&'a str], Option<&'a str>, &'a Value) -> ControlFlow<B>,
) -> ControlFlow<B> {
    match component {
        Component::Builtin {
            state: ComponentState::Button { action, .. },
        } if selected_path.is_none_or(|remaining| remaining.is_empty()) => {
            visit(path, None, action)?
        }
        Component::Composition { composition } => {
            for part in &composition.parts {
                let remaining = if let Some(selected) = selected_path {
                    let Some((id, rest)) = selected.split_first() else {
                        break;
                    };
                    if id != &part.id {
                        continue;
                    }
                    Some(rest)
                } else {
                    None
                };
                path.push(&part.id);
                if remaining.is_none_or(|remaining| remaining.is_empty()) {
                    for binding in &part.events {
                        visit(path, Some(&binding.event), &binding.action)?;
                    }
                }
                walk_controls(&part.component, path, remaining, visit)?;
                path.pop();
            }
        }
        _ => {}
    }
    ControlFlow::Continue(())
}

pub(super) fn has_controls(component: &Component) -> bool {
    visit_controls(component, |_, _, _| ControlFlow::Break(())).is_break()
}

fn control_actions(component: &Component, limit: usize) -> Vec<&Value> {
    let mut output = Vec::new();
    let _: ControlFlow<()> = visit_controls(component, |_, _, action| {
        output.push(action);
        if output.len() >= limit {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    output
}

pub(super) fn valid(component: &Component) -> bool {
    fn visit(component: &Component, depth: usize, count: &mut usize) -> bool {
        *count += 1;
        if depth >= 8 || *count > 256 {
            return false;
        }
        match component {
            Component::Builtin {
                state: ComponentState::Text { text },
            } => text.len() <= 65536,
            Component::Builtin {
                state:
                    ComponentState::Record {
                        record,
                        start_call: None,
                        ..
                    },
            } => nucleus::valid_uid(record, "r"),
            Component::Builtin {
                state: ComponentState::Area { strength, .. },
            } => depth == 0 && (0..=100000).contains(strength),
            Component::Builtin {
                state: ComponentState::Button { label, action },
            } => !label.trim().is_empty() && label.len() <= 320 && action_change(action).is_ok(),
            Component::Composition { composition } => {
                let mut ids = BTreeSet::new();
                !composition.name.trim().is_empty()
                    && composition.name.len() <= 320
                    && composition.origin.as_ref().is_none_or(|origin| {
                        nucleus::valid_uid(&origin.agent, "r")
                            && nucleus::valid_uid(&origin.thread, "r")
                    })
                    && composition.parts.iter().all(|part| {
                        !part.id.is_empty()
                            && part.id.len() <= 80
                            && ids.insert(&part.id)
                            && part.geometry.validate().is_ok()
                            && part.events.len() <= 16
                            && part.events.iter().all(|binding| {
                                !binding.event.trim().is_empty()
                                    && binding.event.len() <= 80
                                    && !binding.event.chars().any(char::is_control)
                                    && action_change(&binding.action).is_ok()
                            })
                            && visit(&part.component, depth + 1, count)
                    })
            }
            _ => false,
        }
    }
    visit(component, 0, &mut 0)
}

pub(super) fn resolve(layout: &Layout, change: &Change) -> Result<Change, EngineError> {
    let Change::Invoke {
        element,
        path,
        event,
        action,
    } = change
    else {
        return Ok(change.clone());
    };
    if layout.disabled_controls.contains(element) {
        return Err(invalid(
            "This control is suspended. Review its configuration under the current policy first.",
        ));
    }
    let component = &layout
        .elements
        .iter()
        .find(|item| &item.id == element)
        .ok_or_else(|| invalid("Control unavailable"))?
        .component;
    if walk_controls(
        component,
        &mut Vec::new(),
        Some(path),
        &mut |_, declared_event, declared| {
            if declared_event == event.as_deref() && declared == action {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
    )
    .is_continue()
    {
        return Err(invalid(
            "The declared control changed. Refresh before using it.",
        ));
    }
    action_change(action)
}

pub fn portable(component: Component) -> Component {
    match component {
        Component::Native {
            kind,
            settings,
            bindings,
        } if bindings.is_empty() && matches!(kind.as_str(), "text" | "editabletext" | "square") => {
            Component::Builtin {
                state: ComponentState::Text {
                    text: settings
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                },
            }
        }
        Component::Composition { mut composition } => {
            composition.parts = composition
                .parts
                .into_iter()
                .map(|mut part| {
                    part.component = portable(part.component);
                    part
                })
                .collect();
            Component::Composition { composition }
        }
        Component::Builtin {
            state: ComponentState::Composition { composition },
        } => Component::Composition {
            composition: nucleus::canvas::Composition {
                name: composition.name,
                origin: composition.origin,
                parts: composition
                    .parts
                    .into_iter()
                    .map(|part| nucleus::canvas::Part {
                        id: part.id,
                        geometry: Geometry {
                            position: part.position.map(f64::from),
                            size: part.size.map(f64::from),
                        },
                        component: portable(Component::Builtin {
                            state: part.component,
                        }),
                        events: part.events,
                    })
                    .collect(),
            },
        },
        component => component,
    }
}

impl Engine {
    pub(super) async fn workspace_controls_ceiling_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        hosted: &Hosted,
    ) -> Result<(), EngineError> {
        let _verification = crate::performance::Span::new("workspace-control-verification-await");
        let mut count = 0;
        for element in &hosted.layout.elements {
            if hosted.layout.disabled_controls.contains(&element.id) {
                continue;
            }
            for action in control_actions(&element.component, 257 - count) {
                count += 1;
                if count > 256 {
                    return Err(invalid(
                        "A workspace supports at most 256 declared controls",
                    ));
                }
                store::sqlx::query("SAVEPOINT workspace_control")
                    .execute(&mut **tx)
                    .await?;
                let change = action_change(action)?;
                let result = self
                    .workspace_record_change(tx, hosted, &change, None, nucleus::execution::now())
                    .await;
                store::sqlx::query("ROLLBACK TO workspace_control")
                    .execute(&mut **tx)
                    .await?;
                store::sqlx::query("RELEASE workspace_control")
                    .execute(&mut **tx)
                    .await?;
                crate::performance::add("workspace-controls-checked", 1);
                result?;
            }
        }
        Ok(())
    }

    pub(super) async fn workspace_import_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        name: &str,
        policy: Value,
        layout: Value,
    ) -> Result<Value, EngineError> {
        self.require_permission_on(tx, actor, "workspace:create")
            .await?;
        self.require_permission_on(tx, actor, "workspace:access_control")
            .await?;
        let policy: Policy = serde_json::from_value(policy)?;
        policy.validate()?;
        if !name_valid(name) {
            return Err(invalid("Use a workspace name of 1–80 characters"));
        }
        let mut layout: Layout = serde_json::from_value(layout)?;
        if layout.elements.len() > 256 || nucleus::json::encoded_len(&layout)? > 262144 {
            return Err(invalid("Import at most 256 placements and 256 KiB"));
        }
        let mut converted = vec![];
        for element in &mut layout.elements {
            let portable = portable(element.component.clone());
            if portable != element.component {
                converted.push(json!({"element":element.id,"notice":"Convert to portable presentation. Native text styling and composition part settings are not transferred."}));
            }
            if visit_controls(&portable, |_, event, _| {
                if event.is_some() {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .is_break()
            {
                converted.push(json!({"element":element.id,"notice":"Named event bindings become explicit host controls; they do not run automatically on this computer."}));
            }
            element.component = portable;
        }
        let mut rejected = vec![];
        for element in &layout.elements {
            let single = Hosted {
                uid: "import".into(),
                name: name.into(),
                revision: 1,
                policy: policy.clone(),
                layout: Layout {
                    elements: vec![element.clone()],
                    ..Default::default()
                },
            };
            let reason = if !valid(&element.component) || element.geometry.validate().is_err() {
                Some("Unsupported component or geometry")
            } else if self.workspace_layout_ceiling(tx, &single).await.is_err()
                || self
                    .workspace_admission_on(tx, actor, &single)
                    .await
                    .is_err()
            {
                Some("References or admission exceed your authority or the workspace ceiling")
            } else if self
                .workspace_controls_ceiling_on(tx, &single)
                .await
                .is_err()
            {
                Some("Declared actions exceed the workspace ceiling or domain constraints")
            } else {
                None
            };
            if let Some(reason) = reason {
                rejected.push(json!({"element":element.id,"reason":reason}));
            }
        }
        let layout_error = layout.validate().err().map(|error| error.to_string());
        let hosted = Hosted {
            uid: "import".into(),
            name: name.into(),
            revision: 1,
            policy: policy.clone(),
            layout: layout.clone(),
        };
        let mut policy_error = None;
        for changes in layout.areas.values() {
            if self
                .workspace_recipe_ceiling(tx, &policy, changes)
                .await
                .is_err()
            {
                policy_error = Some("An Area recipe exceeds the workspace ceiling");
            }
        }
        if self
            .workspace_controls_ceiling_on(tx, &hosted)
            .await
            .is_err()
        {
            policy_error =
                Some("A declared control exceeds the workspace ceiling or domain constraints");
        }
        Ok(
            json!({"ready":rejected.is_empty() && layout_error.is_none() && policy_error.is_none(),"rejected":rejected,"converted":converted,"layout_error":layout_error,"policy_error":policy_error,"elements":layout.elements.len()}),
        )
    }
}
