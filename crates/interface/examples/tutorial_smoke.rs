use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{actions::ActionButton, icons::Tooltip};

#[derive(Resource, Default)]
struct Progress {
    phase: usize,
    frames: u32,
    wait: u32,
}

fn click(world: &mut World, title: &str) -> bool {
    let button = world
        .query::<(Entity, &Tooltip, &ActionButton)>()
        .iter(world)
        .find(|(_, tip, _)| tip.0 == title)
        .map(|(entity, _, button)| (entity, button.target, button.actions.clone()));
    let Some((entity, owner, actions)) = button else {
        return false;
    };
    if world.get::<bevy::ui::InteractionDisabled>(entity).is_some() {
        return false;
    }
    actions.run(world, owner);
    true
}

fn exercise(world: &mut World) {
    let mut progress = world.resource_mut::<Progress>();
    progress.frames += 1;
    progress.wait += 1;
    assert!(
        progress.frames < 600,
        "Tutorial smoke timed out at phase {}",
        progress.phase
    );
    let (phase, wait) = (progress.phase, progress.wait);
    if wait < 30 {
        return;
    }
    let advance = match phase {
        0 => click(world, "Areas of Influence"),
        1 => {
            assert_eq!(
                world
                    .query::<&lince_interface::instinct::Instinct>()
                    .single(world)
                    .unwrap()
                    .page
                    .as_deref(),
                Some("areas-of-influence")
            );
            assert!(
                world
                    .query::<(&bevy::a11y::AccessibilityNode, &ScrollPosition)>()
                    .iter(world)
                    .any(|(node, scroll)| node.label() == Some("Interface") && scroll.0.y > 0.0)
            );
            let article = world
                .query::<(
                    &bevy::a11y::AccessibilityNode,
                    &ComputedNode,
                    &UiGlobalTransform,
                )>()
                .iter(world)
                .find(|(node, _, _)| {
                    node.label() == Some("Interface") && node.role() == accesskit::Role::ScrollView
                })
                .map(|(_, node, position)| {
                    (
                        position.translation.y - node.size().y * 0.5,
                        position.translation.y + node.size().y * 0.5,
                    )
                })
                .unwrap();
            let tutorial = world
                .query::<(&Tooltip, &ActionButton, &UiGlobalTransform)>()
                .iter(world)
                .find(|(tip, _, _)| tip.0 == "Tutorial: Areas of Influence")
                .unwrap()
                .2
                .translation
                .y;
            assert!(
                tutorial > article.0 && tutorial < article.1,
                "The selected Record must be visible without manual scrolling"
            );
            click(world, "Tutorial: Areas of Influence")
        }
        2 => {
            if highlighted(world, "Show controls") {
                let corner = world
                    .query::<(Entity, &bevy::a11y::AccessibilityNode)>()
                    .iter(world)
                    .find(|(_, node)| node.label() == Some("Show controls"))
                    .unwrap()
                    .0;
                world
                    .resource_mut::<bevy::input_focus::InputFocus>()
                    .set(corner, bevy::input_focus::FocusCause::Navigated);
                return;
            }
            assert_eq!(
                world
                    .query::<&lince_interface::workspace::Workspaces>()
                    .single(world)
                    .unwrap()
                    .entries
                    .len(),
                1
            );
            assert_eq!(
                world
                    .query::<&lince_interface::area::InfluenceArea>()
                    .iter(world)
                    .count(),
                0
            );
            highlighted(world, "Edit mode") && click(world, "Edit mode")
        }
        3 => highlighted(world, "Areas of influence") && click(world, "Areas of influence"),
        4 => highlighted(world, "Add square") && click(world, "Add square"),
        5 => {
            highlighted(world, "Make this a Protein Area")
                && click(world, "Make this a Protein Area")
        }
        6 => {
            assert!(highlighted(
                world,
                "Edit the query in a Protein Castle; changes return to this Area"
            ));
            let height = world.query::<&Window>().single(world).unwrap().height();
            let (shell, geometry, transform) = world
                .query::<(
                    &lince_interface::fiote::Fiote,
                    &ComputedNode,
                    &UiGlobalTransform,
                )>()
                .single(world)
                .unwrap();
            assert!(geometry.size().x > 350.0 && geometry.size().y > 100.0);
            let bubble = world.get::<ComputedNode>(shell.bubble).unwrap();
            let size = bubble.size() * bubble.inverse_scale_factor();
            assert!(
                size.x < 430.0 && size.y <= height * 0.72 + 34.0,
                "Tutorial bubble is too large: {size:?}, window height {height}"
            );
            let right = transform.translation.x + geometry.size().x * 0.5;
            let mode = world
                .query::<&lince_interface::edit_mode::EditMode>()
                .single(world)
                .unwrap();
            let panel = world.get::<ComputedNode>(mode.panel).unwrap();
            let panel_position = world.get::<UiGlobalTransform>(mode.panel).unwrap();
            assert!(right < panel_position.translation.x - panel.size().x * 0.5);
            assert!(!click(world, "Next"));
            assert!(!click(world, "Apply attraction"));
            let path = std::env::args().nth(1).expect("provide screenshot path");
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::Success);
                    },
                );
            true
        }
        _ => false,
    };
    if advance {
        let mut progress = world.resource_mut::<Progress>();
        progress.phase += 1;
        progress.wait = 0;
    }
}

fn highlighted(world: &mut World, title: &str) -> bool {
    let outline = world
        .query::<(
            &lince_interface::tutorial::TutorialHighlight,
            &Node,
            &ComputedNode,
            &UiGlobalTransform,
            &Pickable,
        )>()
        .iter(world)
        .find(|(highlight, _, _, _, _)| {
            world
                .get::<Tooltip>(highlight.target)
                .is_some_and(|tip| tip.0 == title)
                || world
                    .get::<bevy::a11y::AccessibilityNode>(highlight.target)
                    .is_some_and(|node| node.label() == Some(title))
        });
    let Some((highlight, node, geometry, position, pickable)) = outline else {
        return false;
    };
    if geometry.size().min_element() <= 0.0 {
        return false;
    }
    assert_eq!(node.display, Display::Flex);
    assert!(!pickable.is_hoverable && !pickable.should_block_lower);
    let target = world.get::<UiGlobalTransform>(highlight.target).unwrap();
    assert!(
        (target.translation - position.translation).length() < 5.0,
        "Highlight must follow its real control"
    );
    true
}

#[tokio::main]
async fn main() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        information: None,
    };
    let directory = tempfile::tempdir().unwrap();
    let mut app = lince_interface::app::connected_app(runtime);
    app.insert_resource(lince_interface::workspace::WorkspaceFile::new(
        directory.path().join("interface.json"),
    ));
    app.insert_resource(WinitSettings::continuous())
        .init_resource::<Progress>()
        .add_systems(Last, exercise)
        .run();
}
