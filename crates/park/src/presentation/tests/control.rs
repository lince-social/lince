use super::*;
use super::{displays::reveal, shortcuts::{capture, click, named_button, settle}};
use lince_interface::{host::{HostRequest, control::Status}, native::actions::Action};

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn the_park_tab_explains_availability_and_revokes_only_through_its_host_binding() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 720], [360, 720]] {
        let mut canvas = Presentation::new(size, directory.path().join(format!("assets-{}", size[0])), WakeSignal::new(|| {})).unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(8);
        canvas.host_requests(sender);
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let button = named_button(&mut canvas, "Revoke automation connections");
        assert!(canvas.app.world().get::<bevy::ui::InteractionDisabled>(button).is_some());
        canvas.world_mut().insert_resource(Status { available: true, active: true, readers: 1, controllers: 1, pending: 2, locked: false });
        settle(&mut canvas);
        let button = named_button(&mut canvas, "Revoke automation connections");
        assert!(canvas.app.world().get::<bevy::ui::InteractionDisabled>(button).is_none());
        reveal(&mut canvas, button);
        let frame = settle(&mut canvas);
        capture(&format!("automation-status-{}", size[0]), size, &frame);
        click(&mut canvas, button, size);
        settle(&mut canvas);
        assert!(matches!(receiver.try_recv().unwrap(), HostRequest::ControlRevoke));
        assert!(receiver.try_recv().is_err());
        canvas.world_mut().insert_resource(Status { available: true, ..Default::default() });
        settle(&mut canvas);
        assert!(canvas.app.world().get::<bevy::ui::InteractionDisabled>(button).is_some());
        canvas.finish().unwrap();
    }
}
