use super::*;
use bevy::prelude::{Entity, With};
use lince_interface::native::{self, actions::Action};

pub(super) fn prepare_flat(canvas: &mut crate::presentation::Presentation) {
    for _ in 0..4 {
        canvas.probe_frame().unwrap().wait_for_probe().unwrap();
    }
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    assert!(native::workspace_config::set_physics(
        canvas.world_mut(),
        root,
        1,
        false
    ));
}

fn prepare(canvas: &mut crate::presentation::Presentation) {
    prepare_flat(canvas);
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
    for _ in 0..4 {
        canvas.probe_frame().unwrap().wait_for_probe().unwrap();
    }
    assert!(native::topology::view::spatial(canvas.world_mut(), root));
}

fn prepare_pair(canvas: &mut crate::presentation::Presentation) {
    prepare(canvas);
    canvas.desktop_action(DesktopAction::Workspace(2)).unwrap();
    for _ in 0..4 {
        canvas.probe_frame().unwrap().wait_for_probe().unwrap();
    }
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    assert!(native::workspace_config::set_physics(
        canvas.world_mut(),
        root,
        2,
        false
    ));
    if !native::topology::view::spatial(canvas.world_mut(), root) {
        native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
    }
    for _ in 0..4 {
        canvas.probe_frame().unwrap().wait_for_probe().unwrap();
    }
    assert!(native::topology::view::spatial(canvas.world_mut(), root));
    canvas.desktop_action(DesktopAction::Workspace(1)).unwrap();
    for _ in 0..4 {
        canvas.probe_frame().unwrap().wait_for_probe().unwrap();
    }
    assert!(native::topology::view::spatial(canvas.world_mut(), root));
}

pub(super) fn fixture(
    runtime: &tokio::runtime::Runtime,
    directory: &Path,
    size: [u32; 2],
    spatial: bool,
    multiple: bool,
) -> Fixture {
    let mut fixture = if spatial {
        Fixture::prepared(
            runtime,
            directory,
            size,
            Some(if multiple { prepare_pair } else { prepare }),
        )
    } else {
        Fixture::new(runtime, directory, size)
    };
    if multiple {
        super::outputs::configure(&mut fixture);
    }
    fixture
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn partially_visible_spatial_tiles_retain_client_press_and_one_committed_destination() {
    super::csd(true, false);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn projected_width_and_row_resize_preserve_neighbor_geometry_constraints_and_input() {
    super::resize(true, false);
}
