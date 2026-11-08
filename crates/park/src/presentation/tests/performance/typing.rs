use super::*;
use bevy::{
    input::keyboard::Key,
    input_focus::{FocusCause, InputFocus},
    text::EditableText,
};
use native::{
    actions::Action,
    edit_mode::EditAction,
    sand_store::{SandKind, StoredSand},
    sand_text::{self, SandText},
};

#[test]
#[ignore = "private hardware text autosave regression with real input and rendered glyph pixels"]
fn edit_panel_typing_paints_content_without_invalidating_text_geometry() {
    use lince_interface::host::performance::{self, Phase};
    let _observer = crate::presentation::observe_systems();
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(checks.first().is_some_and(|check| {
        check
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    }));
    assert!(
        checks.iter().all(|check| check.error.is_none()),
        "{checks:?}"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    let neighbors = std::env::var("LINCE_PARK_LAYOUT_NEIGHBORS")
        .ok()
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(neighbors <= 256);
    for kind in [SandKind::Text, SandKind::EditableText] {
        performance::set_phase(Phase::Warmup);
        let path = directory.path().join(kind.name());
        std::fs::create_dir_all(&path).unwrap();
        let mut canvas = super::super::appearance::launch(&path.join("workspace.json"), size);
        let root = canvas.canvas_root().unwrap();
        let workspace = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
        canvas
            .desktop_action(lince_interface::host::DesktopAction::SetLayout(
                lince_interface::host::WorkspaceLayout::Free,
            ))
            .unwrap();
        *canvas
            .world_mut()
            .get_mut::<native::canvas::CanvasView>(root)
            .unwrap() = default();
        let sand = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            workspace,
            kind,
            "iiii",
            default(),
        );
        let text = canvas
            .app
            .world()
            .get::<StoredSand>(sand)
            .unwrap()
            .content
            .unwrap();
        let neighbors = (0..neighbors)
            .map(|index| {
                let entity = native::sand_store::spawn_sand(
                    canvas.world_mut(),
                    root,
                    workspace,
                    SandKind::Square,
                    "",
                    bevy::math::DVec2::new(10000.0 + index as f64 * 400.0, 0.0),
                );
                canvas
                    .world_mut()
                    .entity_mut(entity)
                    .insert(native::layout::LayoutBox::new(Vec2::splat(100.0)));
                entity
            })
            .collect::<Vec<_>>();
        let pixels = |canvas: &mut Presentation| {
            let frame = super::super::shortcuts::settle(canvas);
            probe_pixels(frame.dmabuf().unwrap()).unwrap()
        };
        let before = pixels(&mut canvas);
        EditAction::Open.apply(canvas.world_mut(), root);
        EditAction::EditSand(sand).apply(canvas.world_mut(), root);
        super::super::shortcuts::settle(&mut canvas);
        assert!(
            canvas
                .app
                .world()
                .get::<native::edit_mode::EditMode>(root)
                .unwrap()
                .enabled
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active,
            workspace
        );
        let field = super::super::displays::labelled(&mut canvas, "Text");
        super::super::displays::reveal(&mut canvas, field);
        canvas
            .world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        canvas
            .key(KeyCode::End, Key::End, None, true, false)
            .unwrap();
        canvas
            .key(KeyCode::End, Key::End, None, false, false)
            .unwrap();
        super::super::shortcuts::settle(&mut canvas);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<EditableText>(field)
                .unwrap()
                .value(),
            "iiii"
        );
        let tick = canvas
            .app
            .world()
            .entity(text)
            .get_ref::<SandText>()
            .unwrap()
            .last_changed();
        let geometry = canvas.app.world().get::<SandText>(text).unwrap().clone();
        let retained = neighbors
            .iter()
            .map(|entity| {
                let world = canvas.app.world();
                (
                    *entity,
                    *world.get::<native::layout::LayoutRuntime>(*entity).unwrap(),
                    world
                        .entity(*entity)
                        .get_ref::<native::layout::LayoutRuntime>()
                        .unwrap()
                        .last_changed(),
                )
            })
            .collect::<Vec<_>>();
        performance::set_phase(Phase::Typing);
        for _ in 0..4 {
            for pressed in [true, false] {
                canvas
                    .key(
                        KeyCode::KeyW,
                        Key::Character("W".into()),
                        pressed.then(|| "W".into()),
                        pressed,
                        false,
                    )
                    .unwrap();
            }
            drop(canvas.probe_frame().unwrap());
            assert_eq!(
                canvas
                    .app
                    .world()
                    .entity(text)
                    .get_ref::<SandText>()
                    .unwrap()
                    .last_changed(),
                tick
            );
        }
        lince_interface::host::performance::flush();
        for (entity, value, tick) in retained {
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::layout::LayoutRuntime>(entity),
                Some(&value)
            );
            assert_eq!(
                canvas
                    .app
                    .world()
                    .entity(entity)
                    .get_ref::<native::layout::LayoutRuntime>()
                    .unwrap()
                    .last_changed(),
                tick
            );
        }
        let value = sand_text::value(canvas.app.world(), text);
        assert_eq!(value, "iiiiWWWW");
        assert_eq!(sand_text::snapshot(canvas.app.world(), sand)[0].text, value);
        assert_eq!(canvas.app.world().get::<SandText>(text), Some(&geometry));
        EditAction::Close.apply(canvas.world_mut(), root);
        canvas.world_mut().resource_mut::<InputFocus>().clear();
        let after = pixels(&mut canvas);
        let changed = before
            .chunks_exact(4)
            .zip(after.chunks_exact(4))
            .filter(|(left, right)| left[..3] != right[..3])
            .count();
        assert!(
            changed > 100,
            "The saved text did not change visible glyph pixels: {changed}"
        );
        for (value, reference) in [("iiii", &before), ("iiiiWWWW", &after)] {
            if let Some(mut editor) = canvas.world_mut().get_mut::<EditableText>(text) {
                editor.editor.set_text(value);
            } else {
                canvas.world_mut().get_mut::<Text>(text).unwrap().0 = value.into();
            }
            assert_eq!(
                &pixels(&mut canvas),
                reference,
                "Text pixels did not match the independently restored reference"
            );
        }
        if let Some(output) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
            let output = std::path::PathBuf::from(output).join(kind.name());
            std::fs::create_dir_all(&output).unwrap();
            std::fs::write(output.join("before.rgba"), &before).unwrap();
            std::fs::write(output.join("after.rgba"), &after).unwrap();
            std::fs::write(output.join("result.json"), serde_json::to_vec_pretty(&serde_json::json!({"size":size,"changed_pixels":changed,"unchanged_geometry":true,"real_keyboard_autosave":true,"exact_restored_references":true})).unwrap()).unwrap();
        }
        canvas.finish().unwrap();
        drop(canvas);
        performance::set_phase(Phase::Reopen);
        let mut restored = super::super::appearance::launch(&path.join("workspace.json"), size);
        let contents = restored
            .world_mut()
            .query::<&StoredSand>()
            .iter(restored.app.world())
            .filter(|stored| stored.kind == kind)
            .map(|stored| sand_text::value(restored.app.world(), stored.content.unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(contents, ["iiiiWWWW"]);
        restored.finish().unwrap();
    }
    assert!(performance::finish(std::time::Duration::from_secs(2)), "Final typing diagnostic report was not delivered");
}
