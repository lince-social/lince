use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    calendar::{Calendar, CalendarSand},
    container::BoxRoot,
    icons::Tooltip,
};

fn setup(world: &mut World) {
    let root = world.spawn(BoxRoot).id();
    let entity = lince_interface::calendar::spawn(
        world,
        root,
        1,
        DVec2::ZERO,
        Calendar {
            year: 2026,
            month: 9,
            start: Some("2026-09-12".into()),
            end: Some("2026-09-16".into()),
            ..Calendar::default()
        },
    );
    world
        .entity_mut(entity)
        .remove::<lince_interface::canvas::CanvasItem>();
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.position_type = PositionType::Absolute;
    node.left = px(24);
    node.top = px(24);
    node.width = px(840);
    node.height = px(680);
}

fn capture(world: &mut World) {
    if world.resource::<FrameCount>().0 != 40 {
        return;
    }
    let calendar = world
        .query_filtered::<&ComputedNode, With<CalendarSand>>()
        .single(world)
        .unwrap();
    assert!(calendar.size().x >= 800.0 && calendar.size().y >= 640.0);
    let days: Vec<_> = world
        .query::<(&Tooltip, &ComputedNode)>()
        .iter(world)
        .filter(|(tip, _)| tip.0.starts_with("Select 2026-09-"))
        .map(|(_, node)| node.size())
        .collect();
    assert_eq!(days.len(), 30);
    assert!(days.iter().all(|size| size.x > 80.0 && size.y > 20.0));
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
