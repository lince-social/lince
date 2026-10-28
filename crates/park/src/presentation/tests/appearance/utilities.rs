use super::*;
use lince_interface::layout::desktop::PlacementId;

fn named(canvas: &mut Presentation, name: &str) -> Entity {
    let world = canvas.world_mut();
    world.query::<(Entity, &Name)>().iter(world).find_map(|(entity, current)| (current.as_str() == name).then_some(entity)).unwrap()
}

fn choose(canvas: &mut Presentation, entity: Entity, size: [u32; 2]) {
    super::super::displays::reveal(canvas, entity);
    click(canvas, entity, size);
    drop(settle(canvas));
}

fn colored(pixel: &[u8], private: bool) -> bool {
    if private { pixel[0] > 230 && pixel[1] < 30 && pixel[2] > 230 }
    else { pixel[0] < 30 && pixel[1] > 230 && pixel[2] < 30 }
}

fn wait(canvas: &mut Presentation, owner: Entity, hidden: Entity, size: [u32; 2], private: bool) -> (Frame, Vec<u8>) {
    let id = ItemId::Native(canvas.app.world().get::<LayoutBox>(owner).unwrap().id);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let frame = canvas.probe_frame().unwrap();
        let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let count = pixels.chunks_exact(4).filter(|pixel| colored(pixel, private)).count();
        if count > 200 && decorations(canvas, owner) == 2 && decorations(canvas, hidden) == 0 {
            let geometry = frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == id).unwrap();
            for (index, pixel) in pixels.chunks_exact(4).enumerate().filter(|(_, pixel)| colored(pixel, private)) {
                let point = [(index % size[0] as usize) as f64 + 0.5, (index / size[0] as usize) as f64 + 0.5];
                assert!((-1..=1).any(|dy| (-1..=1).any(|dx| geometry.surface_point([point[0] + f64::from(dx), point[1] + f64::from(dy)]).is_some())), "The visible compact member painted outside its actual clipped input footprint at {point:?}: {pixel:?}");
            }
            return (frame, pixels);
        }
        assert!(Instant::now() < deadline, "The selected utility item did not acquire pixels/effects or its hidden member retained decorations");
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_compact_choices_and_scratch_visibility_retire_effects_and_exclude_private_members() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, output_scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let mut canvas = launch(&directory.path().join(format!("utilities-{}.json", size[0])), size);
        scale(&mut canvas, size, output_scale);
        let root = canvas.canvas_root().unwrap();
        native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
        open(&mut canvas);
        for (caption, value) in [(SHADOW, "12"), (BLUR, "8")] { apply(&mut canvas, caption, value, size); }
        close(&mut canvas, size);
        canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Tiled)).unwrap();
        let first = item(&mut canvas, root, Vec2::new(240.0, 180.0), [0.0; 2], Color::srgb(0.0, 1.0, 0.0), 1, false);
        let second = item(&mut canvas, root, Vec2::new(240.0, 180.0), [300.0, 0.0], Color::srgb(1.0, 0.0, 1.0), 2, true);
        let third = item(&mut canvas, root, Vec2::new(240.0, 180.0), [600.0, 0.0], Color::srgb(0.0, 0.1, 1.0), 3, false);
        let gate = PolicyGate::default();
        gate.invalidate();
        canvas.world_mut().insert_resource(gate);
        canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
        drop(settle(&mut canvas));
        let ids = [first, second, third].map(|owner| canvas.app.world().get::<LayoutBox>(owner).unwrap().id);
        let neighbor = canvas.app.world().get::<CanvasItem>(third).unwrap().size;
        canvas.desktop_action(DesktopAction::SelectPlacement { workspace: 1, item: Some(ids[0]) }).unwrap();
        drop(settle(&mut canvas));
        canvas.desktop_action(DesktopAction::CreateCompactGroup).unwrap();
        drop(settle(&mut canvas));
        let choice = named(&mut canvas, &format!("compact.choice.{}", second.to_bits()));
        choose(&mut canvas, choice, size);
        assert!(canvas.app.world().get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop.compact_groups().is_empty());
        let confirm = named_button(&mut canvas, "Create selected group");
        choose(&mut canvas, confirm, size);
        assert_eq!(canvas.app.world().get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop.compact_groups().len(), 1);
        let (frame, pixels) = wait(&mut canvas, first, second, size, false);
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| colored(pixel, true)).count(), 0, "A hidden private group member still contributed owner pixels");
        assert!(!frame.geometry.as_ref().unwrap().items.iter().any(|item| item.id == ItemId::Native(ids[1])));
        assert_eq!(canvas.app.world().get::<CanvasItem>(third).unwrap().size, neighbor);
        super::super::layers::capture(&format!("appearance-group-green-{}", size[0]), size, &pixels);
        let selector = named(&mut canvas, &format!("compact.member.{}", second.to_bits()));
        choose(&mut canvas, selector, size);
        let (frame, pixels) = wait(&mut canvas, second, first, size, true);
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| colored(pixel, false)).count(), 0, "The replaced compact member retained visible content");
        let public = super::super::capture::pixels(&frame, OutputId(1));
        assert_eq!(public.chunks_exact(4).filter(|pixel| colored(pixel, true)).count(), 0, "A selected private member leaked through its own image or backdrop");
        super::super::layers::capture(&format!("appearance-group-private-safe-{}", size[0]), size, &public);
        let selector = named(&mut canvas, &format!("compact.member.{}", first.to_bits()));
        choose(&mut canvas, selector, size);
        drop(wait(&mut canvas, first, second, size, false));
        canvas.desktop_action(DesktopAction::DissolveCompactGroup).unwrap();
        drop(settle(&mut canvas));
        for (owner, id) in [(third, ids[2]), (first, ids[0])] {
            canvas.desktop_action(DesktopAction::SelectPlacement { workspace: 1, item: Some(id) }).unwrap();
            drop(settle(&mut canvas));
            canvas.desktop_action(DesktopAction::MarkScratch).unwrap();
            drop(settle(&mut canvas));
            canvas.desktop_action(DesktopAction::HideScratch).unwrap();
            drop(settle(&mut canvas));
            assert_eq!(decorations(&mut canvas, owner), 0);
        }
        canvas.desktop_action(DesktopAction::SelectPlacement { workspace: 1, item: Some(ids[1]) }).unwrap();
        drop(settle(&mut canvas));
        canvas.desktop_action(DesktopAction::ShowScratch).unwrap();
        drop(settle(&mut canvas));
        let choice = {
            let world = canvas.world_mut();
            world.query::<(Entity, &Name, &native::topology::presentation::VisualOwner)>().iter(world)
                .find_map(|(entity, name, owner)| (name.as_str().starts_with("scratch.choice.") && owner.0 == first).then_some(entity)).unwrap()
        };
        choose(&mut canvas, choice, size);
        let confirm = named_button(&mut canvas, "Show selected item here");
        choose(&mut canvas, confirm, size);
        let (_, pixels) = wait(&mut canvas, first, third, size, false);
        assert_eq!(canvas.app.world().get::<LayoutBox>(first).unwrap().id, ids[0]);
        let model = &canvas.app.world().get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop;
        let scratch = model.scratch(PlacementId(ids[0])).unwrap();
        assert!(scratch.shown());
        assert_eq!(scratch.home_workspace(), 1);
        assert!(model.items().iter().find(|item| item.id == PlacementId(ids[0])).unwrap().floating);
        super::super::layers::capture(&format!("appearance-scratch-shown-{}", size[0]), size, &pixels);
        canvas.desktop_action(DesktopAction::HideScratch).unwrap();
        let frame = settle(&mut canvas);
        let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| colored(pixel, false)).count(), 0);
        assert_eq!(decorations(&mut canvas, first), 0);
        assert_eq!(canvas.app.world().get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop.focused(), Some(PlacementId(ids[1])));
        assert!(!frame.geometry.as_ref().unwrap().items.iter().any(|item| item.id == ItemId::Native(ids[0])));
        for (owner, id) in [first, second, third].into_iter().zip(ids) { assert_eq!(canvas.app.world().get::<LayoutBox>(owner).unwrap().id, id); }
        canvas.finish().unwrap();
    }
}
