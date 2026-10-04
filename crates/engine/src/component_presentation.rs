use nucleus::component::{ComponentState, Presentation};
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
        component.validate().map_err(EngineError::Consequence)?;
        component
            .visit(&mut |component| {
                if let ComponentState::Button { action, .. } = component {
                    serde_json::from_value::<crate::actions::Action>(action.clone())
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })
            .map_err(EngineError::Consequence)?;
        component
            .visit(&mut |component| {
                if let ComponentState::Composition { composition } = component {
                    for binding in composition.parts.iter().flat_map(|part| &part.events) {
                        serde_json::from_value::<crate::actions::Action>(binding.action.clone())
                            .map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            })
            .map_err(EngineError::Consequence)?;
        for record in component.records_mut() {
            *record = self.resolve(record).await?;
        }
        Ok(component)
    }

    pub(crate) async fn authorize_component(
        &self,
        component: &ComponentState,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        component.validate().map_err(EngineError::Consequence)?;
        for record in component.records() {
            let uid = self.resolve(record).await?;
            self.refuse_unreadable_karma_inputs(actor, &[uid]).await?;
        }
        let mut actions = Vec::new();
        let mut origins = Vec::new();
        component.visit(&mut |component| {
            match component {
                ComponentState::Button { action, .. } => actions.push(serde_json::from_value::<crate::actions::Action>(action.clone()).map_err(|error| error.to_string())?),
                ComponentState::Composition { composition } => {
                    for binding in composition.parts.iter().flat_map(|part| &part.events) {
                        actions.push(serde_json::from_value::<crate::actions::Action>(binding.action.clone()).map_err(|error| error.to_string())?);
                    }
                    if let Some(origin) = &composition.origin { origins.extend([origin.agent.clone(), origin.thread.clone()]); }
                }
                _ => {}
            }
            Ok(())
        }).map_err(EngineError::Consequence)?;
        self.refuse_unreadable(actor, &origins).await?;
        for action in actions { self.authorize_action(&action, actor).await?; }
        let mut calls = Vec::new();
        component
            .visit(&mut |component| {
                if let ComponentState::Record {
                    record,
                    start_call: Some(start),
                    ..
                } = component
                {
                    calls.push((record.clone(), start.clone()));
                }
                Ok(())
            })
            .map_err(EngineError::Consequence)?;
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
            let predicate =
                store::concepts::resolve(&self.store.pool, crate::threads::THREAD_OF_PREDICATE)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence(
                            "The call thread is not attached to this Record".into(),
                        )
                    })?;
            let parents = store::assertions::objects_from_subject(
                &self.store.pool,
                &start.thread,
                &predicate,
            )
            .await?;
            if !parents.iter().any(|parent| parent.uid == record) {
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
