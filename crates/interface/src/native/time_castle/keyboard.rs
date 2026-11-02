use super::*;
use crate::native::actions::{Action, KeyBinding, KeyBindings, Modifiers};
use bevy::input_focus::{FocusCause, InputFocus};

#[derive(Component)]
struct Marker;

pub(super) fn attach(world: &mut World, owner: Entity) {
    let mut bindings = world.get::<KeyBindings>(owner).cloned().unwrap_or_default();
    for (key, step) in [
        (KeyCode::KeyJ, 1),
        (KeyCode::ArrowDown, 1),
        (KeyCode::KeyK, -1),
        (KeyCode::ArrowUp, -1),
    ] {
        let mut binding = KeyBinding::new(
            key,
            Modifiers::NONE,
            crate::native::actions![Navigate(step)],
        )
        .outside_text();
        binding.repeat = true;
        bindings.0.push(binding);
    }
    bindings.0.push(
        KeyBinding::new(
            KeyCode::Space,
            Modifiers::NONE,
            crate::native::actions![Complete],
        )
        .outside_text(),
    );
    world.entity_mut(owner).insert(bindings);
}

fn available(world: &World, owner: Entity) -> bool {
    !chrome::controls_open(world, owner)
        && world
            .get::<View>(owner)
            .and_then(|view| world.get::<Node>(view.upcoming))
            .is_some_and(|node| node.display != Display::None)
}

#[derive(Clone)]
pub(super) struct Activate(pub String);

impl Action for Activate {
    fn apply(&self, world: &mut World, owner: Entity) {
        if !available(world, owner) {
            return;
        }
        let Some(mut view) = world.get_mut::<View>(owner) else {
            return;
        };
        if !view.upcoming_rows.iter().any(|(id, _)| *id == self.0) {
            return;
        }
        view.active_row = Some(self.0.clone());
        view.reveal_row = true;
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            focus.set(owner, FocusCause::Navigated);
        }
    }
}

#[derive(Clone)]
struct Navigate(isize);

impl Action for Navigate {
    fn apply(&self, world: &mut World, owner: Entity) {
        if !available(world, owner) {
            return;
        }
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        if view.upcoming_rows.is_empty() {
            return;
        }
        let current = view
            .active_row
            .as_ref()
            .and_then(|active| view.upcoming_rows.iter().position(|(id, _)| id == active));
        let index = current.map_or(0, |index| {
            index
                .saturating_add_signed(self.0)
                .min(view.upcoming_rows.len() - 1)
        });
        Activate(view.upcoming_rows[index].0.clone()).apply(world, owner);
    }
}

#[derive(Clone)]
struct Complete;

impl Action for Complete {
    fn apply(&self, world: &mut World, owner: Entity) {
        if !available(world, owner) {
            return;
        }
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let active = view
            .active_row
            .as_ref()
            .or_else(|| view.upcoming_rows.first().map(|(id, _)| id));
        let Some(entry) =
            active.and_then(|active| view.entries.iter().find(|entry| &entry.id == active))
        else {
            return;
        };
        if entry.preview {
            status(world, owner, "Projection previews are read only");
            return;
        }
        let binding = crate::native::protein_area::RecordBinding {
            area: owner,
            uid: entry.record_uid.clone(),
            source: view.source.clone(),
        };
        let request = engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: binding.uid.clone(),
            mutation: engine::record_change::Mutation::Quantity { value: "0".into() },
        };
        match crate::native::record_binding::submit(world, &binding, request) {
            Ok(()) => status(world, owner, "Saving quantity"),
            Err(error) => status(world, owner, &error),
        }
    }
}

pub(super) fn marker(world: &mut World, row: Entity) {
    world.spawn((
        Marker,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(4),
            bottom: px(4),
            width: px(2),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::NONE),
        ChildOf(row),
    ));
}

pub(super) fn update(world: &mut World, owner: Entity, palette: &palette::Palette) {
    let mut focus = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let mut focused = false;
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = focus else { break };
        if entity == owner {
            focused = true;
            break;
        }
        if world.get::<bevy::text::EditableText>(entity).is_some() {
            break;
        }
        focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    let available = available(world, owner);
    let mut view = world.get_mut::<View>(owner).unwrap();
    if !view
        .upcoming_rows
        .iter()
        .any(|(id, _)| Some(id) == view.active_row.as_ref())
    {
        view.active_row = view.upcoming_rows.first().map(|(id, _)| id.clone());
        view.reveal_row = true;
    }
    let panel = view.upcoming;
    let active = view.active_row.clone();
    let reveal = view.reveal_row && focused && available;
    let rows: Vec<_> = world
        .query::<(Entity, &ui::UpcomingRow, &ChildOf)>()
        .iter(world)
        .filter(|(_, _, parent)| parent.parent() == panel)
        .map(|(entity, row, _)| (entity, Some(&row.0) == active.as_ref()))
        .collect();
    for (row, selected) in rows {
        let markers: Vec<_> = world
            .get::<Children>(row)
            .into_iter()
            .flat_map(|children| children.iter())
            .filter(|entity| world.get::<Marker>(*entity).is_some())
            .collect();
        for marker in markers {
            let display = if selected && focused && available {
                Display::Flex
            } else {
                Display::None
            };
            let mut node = world.get_mut::<Node>(marker).unwrap();
            if node.display != display {
                node.display = display;
            }
            world
                .get_mut::<BackgroundColor>(marker)
                .unwrap()
                .set_if_neq(BackgroundColor(palette.event));
        }
        if selected
            && reveal
            && crate::native::scroll_sand::reveal(world, panel, row, 2.0).is_some()
        {
            world.get_mut::<View>(owner).unwrap().reveal_row = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, Entity, palette::Palette) {
        let mut world = World::new();
        world.insert_resource(crate::native::theme::Typography(Handle::default()));
        world.init_resource::<InputFocus>();
        let owner = world
            .spawn((Node::default(), TimeSettings(Settings::default())))
            .id();
        populate(&mut world, owner);
        world.get_mut::<View>(owner).unwrap().entries = (0..8).map(|index| {
            serde_json::from_value(serde_json::json!({
                "uid": format!("task-{index}"), "record_uid": format!("record-{index}"),
                "head": format!("Task {index}"), "quantity": "-1", "category": "timed",
                "time": {"from_ms": 10_000 + index * 1000}, "origin": {"kind": "manual"}, "preview": false
            })).unwrap()
        }).collect();
        let palette = palette::Palette::resolve(&world, owner);
        ui::upcoming(&mut world, owner, 1000, true, &palette);
        world
            .resource_mut::<InputFocus>()
            .set(owner, FocusCause::Navigated);
        update(&mut world, owner, &palette);
        (world, owner, palette)
    }

    #[test]
    fn navigation_marks_and_reveals_the_active_row_and_preserves_it_on_ticks() {
        let (mut world, owner, palette) = fixture();
        assert_eq!(
            world.get::<View>(owner).unwrap().active_row.as_deref(),
            Some("task-0")
        );
        for _ in 0..6 {
            Navigate(1).apply(&mut world, owner);
        }
        let panel = world.get::<View>(owner).unwrap().upcoming;
        let row = world
            .query::<(Entity, &ui::UpcomingRow)>()
            .iter(&world)
            .find(|(_, row)| row.0 == "task-6")
            .unwrap()
            .0;
        world.entity_mut(panel).insert((
            ComputedNode {
                size: Vec2::new(200.0, 100.0),
                content_size: Vec2::new(200.0, 400.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
            UiGlobalTransform::default(),
        ));
        world.entity_mut(row).insert((
            ComputedNode {
                size: Vec2::new(200.0, 40.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
            UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(0.0, 180.0))),
        ));
        update(&mut world, owner, &palette);
        assert!(world.get::<ScrollPosition>(panel).unwrap().0.y > 0.0);
        assert!(!world.get::<View>(owner).unwrap().reveal_row);
        let visible: Vec<_> = world
            .query_filtered::<(&Node, &ChildOf), With<Marker>>()
            .iter(&world)
            .filter(|(node, _)| node.display == Display::Flex)
            .map(|(_, parent)| parent.parent())
            .collect();
        assert_eq!(visible, vec![row]);
        ui::upcoming(&mut world, owner, 2000, true, &palette);
        assert_eq!(
            world.get::<View>(owner).unwrap().active_row.as_deref(),
            Some("task-6")
        );
        Navigate(-1).apply(&mut world, owner);
        assert_eq!(
            world.get::<View>(owner).unwrap().active_row.as_deref(),
            Some("task-5")
        );
        world.resource_mut::<InputFocus>().clear();
        update(&mut world, owner, &palette);
        assert!(
            world
                .query_filtered::<&Node, With<Marker>>()
                .iter(&world)
                .all(|node| node.display == Display::None)
        );
    }

    #[tokio::test]
    async fn completion_saves_zero_and_leaves_projection_previews_unchanged() {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Clock task".into(),
                    body: String::new(),
                    quantity: -1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut app = App::new();
        app.insert_resource(crate::native::theme::Typography(Handle::default()))
            .init_resource::<InputFocus>();
        let owner = app
            .world_mut()
            .spawn((Node::default(), TimeSettings(Settings::default())))
            .id();
        populate(app.world_mut(), owner);
        let entry = serde_json::from_value(serde_json::json!({
            "uid":"task-0", "record_uid":uid, "head":"Clock task", "quantity":"-1", "category":"timed",
            "time":{"from_ms":10_000}, "origin":{"kind":"manual"}, "preview":true
        })).unwrap();
        app.world_mut().get_mut::<View>(owner).unwrap().entries = vec![entry];
        let palette = palette::Palette::resolve(app.world(), owner);
        ui::upcoming(app.world_mut(), owner, 1000, true, &palette);
        app.insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .insert_resource(crate::native::app::CellHandle(cell::CellRuntime {
                commands: Default::default(),
                engine: engine.clone(),
                store: engine.store.clone(),
                lanes: std::sync::Arc::new(cell::LaneHub::new()),
                wire: Default::default(),
                fiote: None,
                speech: None,
                information: None,
            }))
            .add_plugins((
                crate::native::cell_bridge::CellBridgePlugin,
                crate::native::record_binding::RecordBindingPlugin,
            ));
        {
            let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
            view.entries[0].record_uid = uid.clone();
            view.entries[0].preview = true;
        }
        Complete.apply(app.world_mut(), owner);
        assert_eq!(
            store::records::quantity(&engine.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .to_string(),
            "-1"
        );
        app.world_mut().get_mut::<View>(owner).unwrap().entries[0].preview = false;
        Complete.apply(app.world_mut(), owner);
        for _ in 0..500 {
            app.update();
            if store::records::quantity(&engine.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .is_zero()
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("Clock quantity did not save");
    }
}
