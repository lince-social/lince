// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DispatchOutcome, Engine, EngineError, RecordKind,
    invalid_dispatch, validate_saved_protein_shape,
};

impl Engine {
    pub(super) async fn dispatch_saved_proteins(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::SaveProtein { slug, head, ast } => {
                validate_saved_protein_shape(&ast)?;
                let parsed: protein::Protein =
                    serde_json::from_value(ast.clone()).map_err(|error| {
                        EngineError::Consequence(format!("invalid Protein: {error}"))
                    })?;
                protein::validate(&parsed).map_err(|error| {
                    EngineError::Consequence(format!("invalid Protein: {error}"))
                })?;
                let before = serde_json::to_value(&parsed)?;
                let bound = serde_json::to_value(
                    protein::bind_meanings(&self.store, &parsed, actor.as_deref()).await?,
                )?;
                let mut ast = ast;
                for key in ["where", "order"] {
                    if before[key] != bound[key] {
                        ast[key] = bound[key].clone();
                    }
                }
                for key in ["links", "relation_context"] {
                    if before["include"][key] != bound["include"][key] {
                        ast["include"][key] = bound["include"][key].clone();
                    }
                }
                let body = serde_json::to_string_pretty(&ast).unwrap_or_default();
                let uid = match store::records::resolve(&self.store.pool, &slug).await? {
                    Some(existing) => {
                        if existing.kind != RecordKind::Protein.as_str() {
                            return Err(EngineError::Consequence(format!(
                                "slug `{slug}` is a {} record, not a saved protein",
                                existing.kind
                            )));
                        }
                        self.write_record_text(&existing.uid, Some(&head), Some(&body))
                            .await?;
                        if existing.quantity.is_zero() {
                            Box::pin(self.act(
                                Action::SetQuantity {
                                    target: existing.uid.clone(),
                                    value: 1.0,
                                },
                                actor.clone(),
                            ))
                            .await?;
                        }
                        existing.uid
                    }
                    None => {
                        store::records::create(
                            &self.store.pool,
                            store::records::NewRecord {
                                slug: Some(&slug),
                                kind: RecordKind::Protein,
                                head: &head,
                                body: &body,
                                quantity: store::exact::one(),
                            },
                        )
                        .await?
                        .uid
                    }
                };
                store::records::set_extension(&self.store.pool, &uid, "lince.protein", &ast)
                    .await?;
                outcome.data = Some(serde_json::json!({"query":ast}));
                outcome.created = Some(uid);
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
