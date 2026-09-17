use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::ActionButton,
    container::BoxRoot,
    icons::Tooltip,
    instinct::{Instinct, SeedInstinct},
};

fn setup(mut commands: Commands) {
    commands.spawn((BoxRoot, SeedInstinct));
}

fn capture(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 10 {
        let (owner, action) = world
            .query::<(&Tooltip, &ActionButton)>()
            .iter(world)
            .find(|(tip, _)| tip.0 == "Areas of Influence")
            .map(|(_, button)| (button.target, button.actions.clone()))
            .unwrap();
        action.run(world, owner);
    }
    if frame != 40 {
        return;
    }
    let (instinct, node) = world
        .query::<(&Instinct, &ComputedNode)>()
        .single(world)
        .unwrap();
    assert_eq!(instinct.page.as_deref(), Some("areas-of-influence"));
    assert!(node.size().x >= 700.0 && node.size().y >= 500.0);
    let article = world
        .query::<(
            &bevy::a11y::AccessibilityNode,
            &ComputedNode,
            &ScrollPosition,
        )>()
        .iter(world)
        .find(|(node, _, _)| node.label() == Some("Interface"))
        .unwrap()
        .1;
    assert!(article.size().x >= 350.0 && article.size().y >= 300.0);
    assert!(article.content_size().y > article.size().y);
    let path = std::env::args().nth(1).expect("provide output path");
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
}

#[tokio::main]
async fn main() {
    lince_interface::app::interface_app()
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, setup)
        .add_systems(Last, capture)
        .run();
}
