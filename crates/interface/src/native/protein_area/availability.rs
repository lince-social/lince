use super::*;
use crate::native::actions::{Action, ActionButton};

#[derive(Component)]
struct Panel {
    row: Entity,
    binding: RecordBinding,
    label: Entity,
    retry: Entity,
    retry_label: Entity,
}

#[derive(Component)]
struct Hidden(Display);

#[derive(Component)]
struct RowPanel(Entity);

#[derive(Component)]
struct Missing;

pub(super) fn hidden(world: &World, row: Entity) -> bool {
    world.get::<Missing>(row).is_some()
}

pub(super) fn chrome(world: &World, entity: Entity) -> bool {
    world.get::<Panel>(entity).is_some()
        || world
            .get::<crate::native::full_record::window::Header>(entity)
            .is_some()
}

#[derive(Clone)]
struct Refresh(RecordBinding);

pub(super) fn target(config: &Config) -> Option<String> {
    if !config.enabled || !config.record_cards || config.fiote || config.command.is_some() {
        return None;
    }
    let query = config.query().ok()?;
    if query.source != protein::Source::Record || query.limit != Some(1) {
        return None;
    }
    fn uid(predicates: &[protein::Predicate]) -> Option<&str> {
        match predicates {
            [protein::Predicate::UidEq(uid) | protein::Predicate::RecordEq(uid)] => Some(uid),
            [protein::Predicate::All(children)] => uid(children),
            _ => None,
        }
    }
    uid(&query.filter)
        .filter(|uid| nucleus::valid_uid(uid, "r"))
        .map(str::to_string)
}

pub(super) fn retain(world: &World, entity: Entity, config: &Config) -> bool {
    let Some(row) = world.get::<rows::Row>(entity) else {
        return false;
    };
    world
        .get::<RecordBinding>(entity)
        .or_else(|| {
            world.get::<rows::Loading>(entity)?;
            let panel = world.get::<RowPanel>(entity)?;
            Some(&world.get::<Panel>(panel.0)?.binding)
        })
        .is_some_and(|binding| {
            target(config).as_deref() == Some(binding.uid.as_str())
                && binding.source == config.source
                && row.config.source == config.source
                && row.config.draft.query == config.draft.query
        })
}

pub(super) fn populate(
    world: &mut World,
    row: Entity,
    config: &Config,
    binding: Option<&RecordBinding>,
) {
    let Some(binding) =
        binding.filter(|binding| target(config).as_deref() == Some(binding.uid.as_str()))
    else {
        return;
    };
    let entity = world
        .spawn((
            Name::new("record.availability"),
            Node {
                display: Display::None,
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(row),
        ))
        .id();
    let label = crate::native::edit_mode::label(world, entity, "", 14.0);
    world.entity_mut(label).insert((
        Name::new("record.availability.status"),
        TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
    ));
    {
        let mut node = world.get_mut::<Node>(label).unwrap();
        node.width = percent(100);
        node.min_width = px(0);
    }
    let retry = world
        .spawn((
            crate::native::sand::Square,
            crate::native::sand::button(0),
            ActionButton::new(row, crate::native::actions![Refresh(binding.clone())]),
            Node {
                padding: UiRect::all(px(6)),
                min_height: px(28),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(entity),
        ))
        .id();
    let retry_label = crate::native::edit_mode::label(world, retry, "Refresh Record", 14.0);
    world.entity_mut(entity).insert(Panel {
        row,
        binding: binding.clone(),
        label,
        retry,
        retry_label,
    });
    world.entity_mut(row).insert(RowPanel(entity));
}

pub(super) fn synchronize(world: &mut World, owner: Entity, state: &State) {
    let panels: Vec<_> = state
        .single_record
        .as_ref()
        .and_then(|uid| state.row_entities.get(uid))
        .and_then(|row| world.get::<RowPanel>(*row))
        .and_then(|marker| world.get::<Panel>(marker.0).map(|panel| (marker.0, panel)))
        .filter(|(_, panel)| panel.binding.area == owner)
        .map(|(entity, panel)| {
            (
                entity,
                panel.row,
                panel.binding.uid.clone(),
                panel.label,
                panel.retry,
                panel.retry_label,
            )
        })
        .into_iter()
        .collect();
    for (panel, row, uid, label, retry, retry_label) in panels {
        let creating = state.applied.as_ref().is_some_and(Config::creating);
        let creation = world.get::<crate::native::full_record::window::Creation>(owner);
        let can_retry_creation = creation.is_some_and(|creation| creation.retry_available());
        let available = !creating
            && state
                .data
                .iter()
                .any(|data| data["uid"].as_str() == Some(uid.as_str()));
        let binding = RecordBinding {
            area: owner,
            uid: uid.clone(),
            source: state.applied.as_ref().unwrap().source.clone(),
        };
        let suffix = if world.get::<rows::Loading>(row).is_some() {
            ""
        } else {
            " Your draft is kept."
        };
        let message = if creating {
            creation
                .map(|creation| creation.message())
                .unwrap_or_else(|| {
                    "Window Record creation is waiting for its original Park session.".into()
                })
        } else if state.login {
            format!("Log in to this Record's Organ.{suffix}")
        } else if state.refreshing {
            format!("Loading Record…{suffix}")
        } else if !available && state.ready {
            format!(
                "Record unavailable. It may have been deleted or access may have changed.{suffix}"
            )
        } else if !state.ready {
            format!(
                "{}.{suffix}",
                state.status.chars().take(1024).collect::<String>()
            )
        } else if crate::native::record_binding::failed(world, &binding) {
            crate::native::record_binding::status(world, &binding)
                .map(|message| message.chars().take(1024).collect())
                .unwrap_or_else(|| "The Record could not be saved. Your draft is kept.".into())
        } else {
            String::new()
        };
        let retry_caption = if creating {
            "Retry Record creation"
        } else {
            "Refresh Record"
        };
        if world
            .get::<Text>(retry_label)
            .is_some_and(|text| text.0 != retry_caption)
        {
            world.get_mut::<Text>(retry_label).unwrap().0 = retry_caption.into();
        }
        if world
            .get::<Text>(label)
            .is_some_and(|text| text.0 != message)
        {
            world.get_mut::<Text>(label).unwrap().0 = message.clone();
        }
        set_display(
            world,
            panel,
            if message.is_empty() {
                Display::None
            } else {
                Display::Flex
            },
        );
        set_display(
            world,
            retry,
            if creating && !can_retry_creation || !creating && (state.refreshing || state.login) {
                Display::None
            } else {
                Display::Flex
            },
        );
        let forms = world
            .get::<Children>(panel)
            .map(|children| {
                children
                    .iter()
                    .filter(|child| world.get::<login::Form>(*child).is_some())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if state.login && forms.is_empty() {
            if let Some(organ) = state.remote.as_deref() {
                login::form(world, panel, owner, organ, false);
            }
        } else if !state.login && !forms.is_empty() {
            login::clear_passwords(world, owner);
            for form in forms {
                world.despawn(form);
            }
        }
        if !available && !hidden(world, row) {
            world.entity_mut(row).insert(Missing);
            world
                .entity_mut(row)
                .remove::<crate::native::area::RecordProperties>();
        } else if available && hidden(world, row) {
            world.entity_mut(row).remove::<Missing>();
            if let Some(data) = state
                .data
                .iter()
                .find(|data| data["uid"].as_str() == Some(uid.as_str()))
            {
                world
                    .entity_mut(row)
                    .insert(crate::native::area::RecordProperties(data.clone()));
            }
        }
        let children = world
            .get::<Children>(row)
            .map(|children| children.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for child in children
            .into_iter()
            .filter(|child| !chrome(world, *child))
            .collect::<Vec<_>>()
        {
            if !available {
                if world.get::<Hidden>(child).is_none()
                    && let Some(node) = world.get::<Node>(child)
                {
                    let previous = node.display;
                    world.entity_mut(child).insert(Hidden(previous));
                }
                set_display(world, child, Display::None);
            } else if let Some(previous) = world.entity_mut(child).take::<Hidden>() {
                set_display(world, child, previous.0);
            }
        }
        if !available
            && let Some(focused) = world
                .get_resource::<bevy::input_focus::InputFocus>()
                .and_then(bevy::input_focus::InputFocus::get)
        {
            let mut cursor = Some(focused);
            for _ in 0..crate::native::layout::MAX_DEPTH {
                let Some(entity) = cursor else { break };
                if entity == panel {
                    break;
                }
                if entity == row {
                    world
                        .resource_mut::<bevy::input_focus::InputFocus>()
                        .clear();
                    break;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
        }
    }
}

fn set_display(world: &mut World, entity: Entity, display: Display) {
    if world
        .get::<Node>(entity)
        .is_some_and(|node| node.display != display)
    {
        world.get_mut::<Node>(entity).unwrap().display = display;
    }
}

impl Action for Refresh {
    fn apply(&self, world: &mut World, _: Entity) {
        let binding = &self.0;
        let Some(config) = configuration(world, binding.area).filter(|config| {
            config.source == binding.source
                && target(config).as_deref() == Some(binding.uid.as_str())
        }) else {
            return;
        };
        if crate::native::laboratory::blocked(world, binding.area)
            || crate::native::laboratory::suspended(world, binding.area)
        {
            return;
        }
        if config.creating() {
            if let Some(mut creation) =
                world.get_mut::<crate::native::full_record::window::Creation>(binding.area)
            {
                creation.request_retry();
            }
            if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                wake.ring();
            }
            return;
        }
        let protein = match query(world, binding.area, &config) {
            Ok(protein) => protein,
            Err(error) => {
                status(world, binding.area, error);
                return;
            }
        };
        let Some(mut state) = world.resource_mut::<Runtime>().areas.remove(&binding.area) else {
            return;
        };
        if state.refreshing {
            world
                .resource_mut::<Runtime>()
                .areas
                .insert(binding.area, state);
            return;
        }
        let cancelled = local::interrupt(&mut state);
        let subscription = id(world);
        if let Some(previous) = state.subscription.replace(subscription.clone()) {
            if let Some(organ) = &state.remote {
                sessions::unsubscribe(world, organ, previous);
            } else {
                state
                    .pending
                    .push_back(ClientMessage::Unsubscribe { id: previous });
            }
        }
        if let Source::Organ(organ) = &config.source {
            sessions::reconnect(world, organ);
            sessions::attach(world, organ, &mut state);
        }
        state.refreshing = true;
        if !state.login {
            state.status = "Loading".into();
        }
        state.pending.push_back(ClientMessage::Subscribe {
            id: subscription,
            protein,
        });
        synchronize(world, binding.area, &state);
        world
            .resource_mut::<Runtime>()
            .areas
            .insert(binding.area, state);
        local::failed(
            world,
            cancelled,
            "The Record is refreshing. Your draft is kept.",
        );
        crate::native::record_binding::retry(world, binding);
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

#[cfg(test)]
#[path = "availability/recovery.rs"]
mod recovery;

#[cfg(test)]
#[path = "availability/authentication.rs"]
mod authentication;

#[cfg(test)]
#[path = "availability/loading.rs"]
mod loading;

#[cfg(test)]
#[path = "availability/creation.rs"]
mod creation;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::text::EditableText;

    #[test]
    fn a_single_record_inspector_keeps_its_draft_when_the_record_disappears() {
        let (mut app, _, owner) = super::super::tests::fixture();
        let uid = nucleus::new_uid("r");
        app.world_mut()
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .protein = Some(crate::native::full_record::config(&uid, Source::Local));
        super::super::tests::authoring_record(
            app.world_mut(),
            owner,
            serde_json::json!({
                "uid":uid,"head":"Owner title","body":"Owner description","kind":"plain",
                "quantity":0,"assertions":[],"threads":[]
            }),
        );
        let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
        let editor = app
            .world_mut()
            .query::<(Entity, &EditableText, &RecordBinding)>()
            .iter(app.world())
            .find(|(_, text, binding)| {
                binding.uid == uid && text.value().to_string() == "Owner description"
            })
            .unwrap()
            .0;
        app.world_mut()
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("Unsaved owner draft");
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Pressed);
        let subscription = "availability-test".to_string();
        app.world_mut()
            .resource_mut::<Runtime>()
            .areas
            .get_mut(&owner)
            .unwrap()
            .subscription = Some(subscription.clone());
        receive(
            app.world_mut(),
            owner,
            ServerMessage::Update {
                id: subscription,
                rows: Vec::new(),
            },
        );
        rows::reconcile(app.world_mut(), owner);
        rows::commit_edits(app.world_mut());
        assert!(
            app.world().get_entity(row).is_ok(),
            "Losing a query result must not destroy a single Record's draft"
        );
        assert_eq!(
            app.world()
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Unsaved owner draft"
        );
        assert_eq!(app.world().get::<RecordBinding>(editor).unwrap().uid, uid);
        assert!(
            app.world()
                .get::<crate::native::area::RecordProperties>(row)
                .is_none()
        );
        let panel = app.world().get::<RowPanel>(row).unwrap().0;
        let details = app.world().get::<Panel>(panel).unwrap();
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::Flex
        );
        assert!(
            app.world()
                .get::<Text>(details.label)
                .unwrap()
                .0
                .contains("Record unavailable")
        );
        assert!(
            app.world()
                .get::<Children>(row)
                .unwrap()
                .iter()
                .filter(|entity| *entity != panel)
                .all(|entity| app
                    .world()
                    .get::<Node>(entity)
                    .is_none_or(|node| node.display == Display::None))
        );
        assert_eq!(
            app.world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            None
        );
        let binding = app.world().get::<RecordBinding>(editor).unwrap().clone();
        assert!(
            execute(
                app.world_mut(),
                &binding,
                editor,
                engine::actions::Action::EditRecordText {
                    target: uid.clone(),
                    head: None,
                    body: Some("Must not be written".into()),
                }
            )
            .is_err()
        );
        assert!(
            app.world().resource::<Runtime>().areas[&owner]
                .pending
                .is_empty()
        );
        assert!(
            app.world().resource::<Runtime>().areas[&owner]
                .data
                .is_empty()
        );
    }
}
