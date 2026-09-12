use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    container::BoxRoot,
    information::{InformationState, OpenInformation},
};

#[derive(Resource)]
struct Capture(String);

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 6 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        OpenInformation.apply(world, root);
    }
    if frame == 24 {
        let texts = world
            .query::<&Text>()
            .iter(world)
            .map(|text| text.0.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(texts.contains("Version: 0.7.0"));
        assert!(texts.contains("127.0.0.1:6174"));
        assert!(texts.contains("nixos-rebuild"));
        let path = world.resource::<Capture>().0.clone();
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
    }
    assert!(frame < 600, "Information screenshot timed out");
}

fn main() {
    let mut app = lince_interface::app::interface_app();
    app.world_mut().resource_mut::<InformationState>().current = Some(cell::information::Information {
        version: "0.7.0".into(), revision: "8fc0b6e28ea14773c63e8d7eb43c12cc514e307b".into(),
        directory: "/home/user/.local/share/lince".into(), executable: "/nix/store/lince/bin/lince".into(),
        address: Some("127.0.0.1:6174".into()), last_updated: Some("2026-09-12T12:00:00Z".into()),
        last_checked: Some("2026-09-12T13:00:00Z".into()), automatic: false, server: false,
        checks_enabled: true, phase: cell::information::UpdatePhase::Idle,
        update: Some(cell::information::UpdateStatus {
            availability: cell::information::Availability::Available,
            version: "0.7.1".into(), revision: "a387ecda9fc04de59bfd03d70bbe62fbc17ab33f".into(),
            channel: "rolling".into(), asset_name: None, asset_url: None, asset_sha256: None,
            can_self_apply: false,
            self_apply_note: "Running from the Nix store. Update with: nix flake update lince && nixos-rebuild switch (or home-manager switch).".into(),
        }), note: String::new(), error: None,
    });
    app.insert_resource(Capture(
        std::env::args().nth(1).expect("provide a screenshot path"),
    ))
    .insert_resource(WinitSettings::continuous())
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(BoxRoot);
    })
    .add_systems(Update, exercise)
    .run();
}
