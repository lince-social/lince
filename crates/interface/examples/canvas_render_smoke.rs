use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    edit_mode::EditAction,
    topology::presentation::Surface,
};

#[derive(Resource, Default)]
struct Progress(u32);

fn exercise(world: &mut World) {
    world.resource_mut::<Progress>().0 += 1;
    let frame = world.resource::<Progress>().0;
    let Some(root) = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    match frame {
        10 => {
            EditAction::Open.apply(world, root);
            EditAction::AddSand(lince_interface::sand_store::SandKind::EditableText)
                .apply(world, root);
            EditAction::Store.apply(world, root);
        }
        35 => {
            assert!(world.get::<BackgroundColor>(root).is_none());
            let mode = world
                .get::<lince_interface::edit_mode::EditMode>(root)
                .unwrap();
            let edit = mode.toggle;
            let toolbar = world.get::<ChildOf>(edit).unwrap().parent();
            let children = world.get::<Children>(toolbar).unwrap();
            let index = children.iter().position(|entity| entity == edit).unwrap();
            let toggle = children[index - 1];
            assert!(world.get::<Children>(toggle).unwrap().iter().any(|entity| {
                world
                    .get::<Text>(entity)
                    .is_some_and(|text| text.0 == "2D / 3D")
            }));
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-canvas-review/normal.png"))
                .observe(|event: On<ScreenshotCaptured>| {
                    let width = event.image.width() as usize;
                    let data = event.image.data.as_ref().unwrap();
                    let mut low = 255;
                    let mut high = 0;
                    for y in 70..170 {
                        for x in 40..140 {
                            let value = data[(y * width + x) * 4];
                            low = low.min(value);
                            high = high.max(value);
                        }
                    }
                    assert!(
                        high > low + 10,
                        "The canvas grid must be visible: {low}..{high}"
                    );
                });
        }
        45 => {
            world.get_mut::<CanvasView>(root).unwrap().set_zoom(3.0);
            let panel = world
                .get::<lince_interface::edit_mode::EditMode>(root)
                .unwrap()
                .panel;
            world.get_mut::<ScrollPosition>(panel).unwrap().0.y = 600.0;
            for mut text in world.query_filtered::<&mut bevy::text::EditableText, With<lince_interface::sand_text::SandText>>().iter_mut(world) {
                text.editor.set_text("Sharp text at 300%");
            }
        }
        65 => {
            let panel = world
                .get::<lince_interface::edit_mode::EditMode>(root)
                .unwrap()
                .panel;
            let panel_top = world.get::<UiGlobalTransform>(panel).unwrap().translation.y
                - world.get::<ComputedNode>(panel).unwrap().size().y * 0.5;
            let close = world
                .query::<(Entity, &lince_interface::edit_mode::EditControl)>()
                .iter(world)
                .find(|(_, control)| control.action == EditAction::Close)
                .unwrap()
                .0;
            let close_y = world.get::<UiGlobalTransform>(close).unwrap().translation.y;
            assert!(
                (panel_top..panel_top + 64.0).contains(&close_y),
                "Close must stay visible while the panel scrolls"
            );
            for (entity, item, surface) in
                world.query::<(Entity, &CanvasItem, &Surface)>().iter(world)
            {
                assert!(surface.pixels.x as f32 >= item.size.x * 3.0);
                assert!(surface.density >= 3.0);
                assert_eq!(
                    world
                        .get::<ComputedUiRenderTargetInfo>(entity)
                        .unwrap()
                        .scale_factor(),
                    surface.density
                );
                assert_eq!(
                    world
                        .get::<Camera>(surface.camera)
                        .unwrap()
                        .target_scaling_factor(),
                    Some(surface.density)
                );
                let mut pending = vec![entity];
                while let Some(descendant) = pending.pop() {
                    if let Some(children) = world.get::<Children>(descendant) {
                        pending.extend(children.iter());
                    }
                    if let Some(layout) = world.get::<bevy::text::TextLayoutInfo>(descendant) {
                        assert_eq!(
                            layout.scale_factor, surface.density,
                            "Text must rerasterize at the surface resolution"
                        );
                    }
                }
            }
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-canvas-review/zoom.png"));
        }
        80 => {
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    lince_interface::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .init_resource::<Progress>()
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .run();
}
