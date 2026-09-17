use bevy::{
    diagnostic::FrameCount,
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    ui_widgets::Activate,
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action, canvas::CanvasItem, canvas_selection::SandSelection, container::BoxRoot,
    deletion::DeleteSelected, edit_mode::EditAction, workspace::WorkspaceMember,
};

#[derive(Resource)]
struct Target(Entity, Entity);

fn control(world: &mut World, name: &str) -> Entity {
    let label = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0 == name)
        .unwrap()
        .0;
    let button = world.get::<ChildOf>(label).unwrap().parent();
    let size = world.get::<ComputedNode>(button).unwrap().size();
    assert!(size.x > 100.0 && size.y >= 40.0, "{name}: {size:?}");
    assert!(
        world
            .get::<lince_interface::actions::ActionButton>(button)
            .is_some()
    );
    button
}

fn exercise(world: &mut World) {
    match world.resource::<FrameCount>().0 {
        10 => {
            let root = world
                .query_filtered::<Entity, With<BoxRoot>>()
                .single(world)
                .unwrap();
            EditAction::Open.apply(world, root);
            let item = world
                .spawn((
                    CanvasItem {
                        position: DVec2::ZERO,
                        size: Vec2::splat(100.0),
                    },
                    WorkspaceMember(1),
                    ChildOf(root),
                ))
                .id();
            world.insert_resource(Target(root, item));
            world.entity_mut(root).insert(SandSelection(vec![item]));
            world
                .resource_mut::<InputFocus>()
                .set(root, FocusCause::Pressed);
            DeleteSelected.apply(world, root);
        }
        20 => {
            control(world, "Delete");
            let entity = control(world, "Cancel");
            world.trigger(Activate { entity });
        }
        24 => {
            let Target(root, item) = *world.resource::<Target>();
            assert!(world.get_entity(item).is_ok());
            world.entity_mut(root).insert(SandSelection(vec![item]));
            DeleteSelected.apply(world, root);
        }
        30 => {
            control(world, "Cancel");
            control(world, "Delete");
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-deletion-smoke.png"));
        }
        60 => {
            let entity = control(world, "Delete");
            world.trigger(Activate { entity });
        }
        65 => {
            assert!(world.get_entity(world.resource::<Target>().1).is_err());
            println!("Deletion smoke passed: visible buttons, Cancel preserves, Delete removes.");
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    lince_interface::app::interface_app()
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .run();
}
