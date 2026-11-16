use nucleus::component::{ComponentState, Presentation};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::{Engine, EngineError, actions::ActionOutcome};

impl Engine {
    pub fn subscribe_components(&self) -> broadcast::Receiver<Presentation> {
        self.component_presentations.subscribe()
    }

    pub(crate) async fn resolve_component(
        &self,
        mut component: ComponentState,
    ) -> Result<ComponentState, EngineError> {
        self.resolve_component_in_place(&mut component).await?;
        Ok(component)
    }

    pub(crate) async fn resolve_component_in_place(
        &self,
        component: &mut ComponentState,
    ) -> Result<(), EngineError> {
        // Stage only resolved references. A failure leaves the whole component
        // unchanged without cloning its text, settings and action payloads.
        let records = self.resolve_component_records(component).await?;
        for (record, resolved) in component.records_mut().into_iter().zip(records) {
            *record = resolved;
        }
        Ok(())
    }

    /// Validate and resolve references without copying the presentation tree.
    /// Validation-only callers may discard the resolved identifiers.
    pub(crate) async fn resolve_component_records(
        &self,
        component: &ComponentState,
    ) -> Result<Vec<String>, EngineError> {
        let _resolution = crate::performance::Span::new("component-reference-resolution-await");
        component.validate().map_err(EngineError::Consequence)?;
        component
            .visit(&mut |component| {
                if let ComponentState::Button { action, .. } = component {
                    crate::actions::Action::deserialize(action).map_err(|e| e.to_string())?;
                }
                Ok(())
            })
            .map_err(EngineError::Consequence)?;
        component
            .visit(&mut |component| {
                if let ComponentState::Composition { composition } = component {
                    for binding in composition.parts.iter().flat_map(|part| &part.events) {
                        crate::actions::Action::deserialize(&binding.action)
                            .map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            })
            .map_err(EngineError::Consequence)?;
        let references = component.records();
        let mut resolved = Vec::with_capacity(references.len());
        for record in references {
            resolved.push(self.resolve(record).await?);
        }
        Ok(resolved)
    }

    pub(crate) async fn authorize_component(
        &self,
        component: &ComponentState,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let _authorization = crate::performance::Span::new("component-authorization-await");
        component.validate().map_err(EngineError::Consequence)?;
        for record in component.records() {
            let uid = self.resolve(record).await?;
            self.refuse_unreadable_karma_inputs(actor, &[uid]).await?;
        }
        let mut actions = Vec::new();
        let mut origins = Vec::new();
        let mut calls = Vec::new();
        component
            .visit(&mut |component| {
                match component {
                    ComponentState::Button { action, .. } => actions.push(
                        crate::actions::Action::deserialize(action)
                            .map_err(|error| error.to_string())?,
                    ),
                    ComponentState::Composition { composition } => {
                        for binding in composition.parts.iter().flat_map(|part| &part.events) {
                            actions.push(
                                crate::actions::Action::deserialize(&binding.action)
                                    .map_err(|error| error.to_string())?,
                            );
                        }
                        if let Some(origin) = &composition.origin {
                            origins.extend([origin.agent.clone(), origin.thread.clone()]);
                        }
                    }
                    ComponentState::Record {
                        record,
                        start_call: Some(start),
                        ..
                    } => calls.push((record, start)),
                    _ => {}
                }
                Ok(())
            })
            .map_err(EngineError::Consequence)?;
        self.refuse_unreadable(actor, &origins).await?;
        for action in actions {
            self.authorize_action(&action, actor).await?;
        }
        for (record, start) in calls {
            if actor.is_some_and(|actor| actor != start.person) {
                return Err(EngineError::Forbidden(
                    "A call cannot use another person's identity".into(),
                ));
            }
            if !store::people::is_active(&self.store.pool, &start.person).await? {
                return Err(EngineError::Consequence(
                    "Choose an active Person for the automatic call".into(),
                ));
            }
            let predicate = store::vocabulary::feature_read(&self.store.pool, "thread")
                .await?
                .ok_or_else(|| {
                    EngineError::Consequence(
                        "The call thread is not attached to this Record".into(),
                    )
                })?;
            if !store::assertions::object_exists(
                &self.store.pool,
                &start.thread,
                &predicate,
                Some(record),
            )
            .await?
            {
                return Err(EngineError::Consequence(
                    "The call thread is not attached to this Record".into(),
                ));
            }
            let context = self
                .call_context(&start.thread, Some(&start.person))
                .await?;
            if context.group.is_some() && !context.admitted.contains(&start.person) {
                return Err(EngineError::Consequence(
                    "Your Organ must admit this Person to the group".into(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) async fn present_component(
        &self,
        target: String,
        component: ComponentState,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        let target = self.resolve(&target).await?;
        self.refuse_unreadable_karma_inputs(actor, std::slice::from_ref(&target))
            .await?;
        let component = self.resolve_component(component).await?;
        self.authorize_component(&component, actor).await?;
        if actor.is_some() {
            let mut passive = true;
            component.visit(&mut |component| {
                passive &= match component {
                    ComponentState::Text { .. } => true,
                    ComponentState::Composition { composition } => composition.parts.iter().all(|part| part.events.is_empty()),
                    _ => false,
                };
                Ok(())
            }).map_err(EngineError::Consequence)?;
            if !passive {
                return Err(EngineError::Forbidden(
                    "Native controls require the local interface session. Use a shared workspace for controls executed under your login.".into(),
                ));
            }
        }
        let slot = format!("{target}:{}", component.kind());
        let mut component = if crate::operation_origin::is_fiote()
            && !matches!(component, ComponentState::Composition { .. })
        {
            ComponentState::Composition {
                composition: nucleus::component::Composition {
                    name: "Fiote interaction".into(),
                    origin: None,
                    parts: vec![nucleus::component::Part {
                        settings: Default::default(),
                        id: "content".into(),
                        events: Vec::new(),
                        position: [0, 0],
                        size: [840, 680],
                        component,
                    }],
                },
            }
        } else {
            component
        };
        if let ComponentState::Composition { composition } = &mut component
            && let Some(origin) = crate::operation_origin::component_origin()
        {
            composition.origin = Some(origin);
        }
        component.validate().map_err(EngineError::Consequence)?;
        let presentation = Presentation { slot, component };
        let receivers = self
            .component_presentations
            .send(presentation)
            .map_err(|_| {
                EngineError::Consequence("No native interface is receiving components".into())
            })?;
        Ok(ActionOutcome {
            data: Some(serde_json::json!({"receivers": receivers})),
            ..Default::default()
        })
    }
}
