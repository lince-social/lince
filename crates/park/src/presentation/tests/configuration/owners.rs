use super::*;
use lince_interface::{host::{DesktopAction, Direction}, native::actions::Modifiers};

const DWELL: &str = "File drag hover delay (100–2000 ms)";
const SENSITIVITY: &str = "Gesture sensitivity (10–800%; 100% is normal)";
const THRESHOLD: &str = "Workspace/overview threshold (16–400 logical units)";

fn field(canvas: &mut Presentation, caption: &str) -> String {
    let entity = labelled(canvas, caption);
    canvas.app.world().get::<bevy::text::EditableText>(entity).unwrap().value().to_string()
}

fn initial(directory: &Path, size: [u32; 2], scale: f64) -> (Presentation, Receiver<HostRequest>) {
    let (mut canvas, receiver) = launch(directory, size, scale);
    press(&mut canvas, "Edit overview transfers", size);
    write(&mut canvas, DWELL, "500", size);
    press(&mut canvas, "Save overview transfers", size);
    press(&mut canvas, "Hide overview settings", size);
    (canvas, receiver)
}

fn replace(path: &Path, bytes: &[u8]) {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
    file.write_all(bytes).unwrap();
    file.as_file().sync_all().unwrap();
    file.persist(path).unwrap();
}

fn authored(directory: &Path, size: [u32; 2], scale: f64, edit: impl FnOnce(&mut Presentation)) -> Vec<u8> {
    let author = tempfile::tempdir().unwrap();
    std::fs::copy(directory.join("interface.lingua"), author.path().join("interface.lingua")).unwrap();
    let (mut canvas, _receiver) = launch(author.path(), size, scale);
    edit(&mut canvas);
    if !active_dwell(&mut canvas, 900) {
        press(&mut canvas, "Edit overview transfers", size);
        write(&mut canvas, DWELL, "900", size);
        press(&mut canvas, "Save overview transfers", size);
    }
    canvas.finish().unwrap();
    std::fs::read(author.path().join("interface.lingua")).unwrap()
}

fn count_choices(canvas: &mut Presentation) -> usize {
    let world = canvas.world_mut();
    world.query::<&Name>().iter(world).filter(|name| name.as_str().starts_with("configuration.review.choice.") && name.as_str().ends_with(".local")).count()
}

fn choose(canvas: &mut Presentation, index: usize, file: bool, pointer: bool, size: [u32; 2]) {
    eprintln!("Choosing conflict {index}: file={file}, pointer={pointer}");
    let name = format!("configuration.review.choice.{index}.{}", if file { "file" } else { "local" });
    let entity = named(canvas, &name).unwrap();
    tab_to(canvas, entity);
    if pointer { reveal(canvas, entity); click(canvas, entity, size); settle(canvas); }
    else { key(canvas, KeyCode::Space, true); key(canvas, KeyCode::Space, false); }
}

fn continue_review(canvas: &mut Presentation, size: [u32; 2]) {
    let confirm = named(canvas, "configuration.review.confirm").unwrap();
    assert!(canvas.app.world().get::<bevy::ui::InteractionDisabled>(confirm).is_none());
    tab_to(canvas, confirm);
    key(canvas, KeyCode::Space, true);
    key(canvas, KeyCode::Space, false);
    assert!(named(canvas, "configuration.review").is_none());
    let frame = settle(canvas);
    capture(&format!("configuration-reviewed-{}", size[0]), size, &frame);
}

fn bind(canvas: &mut Presentation, command: &str, function: KeyCode, file: bool, size: [u32; 2]) {
    eprintln!("Capturing {command}: {function:?}, file={file}");
    let button = super::super::shortcuts::capture_button(canvas, command);
    click(canvas, button, size);
    let capture = settle(canvas);
    assert!(capture.input.keyboard_capture && capture.input.bindings.is_empty());
    let modifier = if file { KeyCode::AltLeft } else { KeyCode::ControlLeft };
    for (physical, pressed) in [(KeyCode::SuperLeft, true), (modifier, true), (function, true), (function, false), (modifier, false), (KeyCode::SuperLeft, false)] {
        let logical = match physical {
            KeyCode::SuperLeft => bevy::input::keyboard::Key::Super,
            KeyCode::AltLeft => bevy::input::keyboard::Key::Alt,
            KeyCode::ControlLeft => bevy::input::keyboard::Key::Control,
            _ => bevy::input::keyboard::Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
        };
        canvas.key(physical, logical, None, pressed, false).unwrap();
    }
    settle(canvas);
    let validate = named_button(canvas, "Validate combination");
    let disabled = canvas.app.world().get::<bevy::ui::InteractionDisabled>(validate).is_some();
    let observed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let received = observed.clone();
    canvas.world_mut().entity_mut(validate).observe(move |_: On<bevy::ui_widgets::Activate>| {
        received.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    press(canvas, "Validate combination", size);
    let frame = settle(canvas);
    let activations = observed.load(std::sync::atomic::Ordering::SeqCst);
    if disabled || activations != 1 || frame.input.keyboard_capture {
        super::super::shortcuts::capture("configuration-binding-validation", size, &frame);
        let world = canvas.world_mut();
        let status = world.query::<&Text>().iter(world).filter(|text| ["captur", "ombination", "reserved", "Mod+", "Ctrl+", "Alt+"].iter().any(|value| text.0.contains(value)))
            .map(|text| text.0.clone()).collect::<Vec<_>>();
        panic!("{command} ({function:?}, file={file}) did not validate: disabled={disabled}, activations={activations}, capture={}, status={status:?}", frame.input.keyboard_capture);
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn eight_rendered_binding_conflicts_scroll_to_the_last_choice_preserve_parent_draft_and_reload_the_mixed_saved_chords() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let commands = [
        ("focus.down", DesktopAction::Focus(Direction::Down), KeyCode::F1),
        ("focus.left", DesktopAction::Focus(Direction::Left), KeyCode::F2),
        ("focus.right", DesktopAction::Focus(Direction::Right), KeyCode::F3),
        ("focus.up", DesktopAction::Focus(Direction::Up), KeyCode::F4),
        ("item.move-down", DesktopAction::Move(Direction::Down), KeyCode::F5),
        ("item.move-left", DesktopAction::Move(Direction::Left), KeyCode::F6),
        ("item.move-right", DesktopAction::Move(Direction::Right), KeyCode::F7),
        ("item.move-up", DesktopAction::Move(Direction::Up), KeyCode::F8),
    ];
    for (size, scale) in [([360, 720], 1.5)] {
        let profile = directory.path().join(size[0].to_string());
        std::fs::create_dir_all(&profile).unwrap();
        let (mut canvas, receiver) = initial(&profile, size, scale);
        let external = authored(&profile, size, scale, |canvas| {
            press(canvas, "Edit all shortcuts", size);
            for (command, _, function) in commands { bind(canvas, command, function, true, size); }
            press(canvas, "Save all changes", size);
        });
        press(&mut canvas, "Edit all shortcuts", size);
        for (command, _, function) in commands { bind(&mut canvas, command, function, false, size); }
        replace(&profile.join("interface.lingua"), &external);
        wait_dwell(&mut canvas, 900);
        press(&mut canvas, "Review draft against file", size);
        assert_eq!(count_choices(&mut canvas), 8);
        press(&mut canvas, "Cancel review and keep draft", size);
        assert_eq!(std::fs::read(profile.join("interface.lingua")).unwrap(), external);
        bind(&mut canvas, commands[0].0, commands[0].2, false, size);
        press(&mut canvas, "Review draft against file", size);
        assert_eq!(count_choices(&mut canvas), 8);
        let confirm = named(&mut canvas, "configuration.review.confirm").unwrap();
        for index in 0..8 {
            assert!(canvas.app.world().get::<bevy::ui::InteractionDisabled>(confirm).is_some());
            choose(&mut canvas, index, index % 2 == 1, index % 2 == 0, size);
        }
        let overlay = named(&mut canvas, "configuration.review").unwrap();
        assert!(canvas.world_mut().query::<(Entity, &ScrollPosition)>().iter(canvas.app.world()).any(|(entity, scroll)| {
            if scroll.0.y <= 0.0 { return false; }
            let mut current = Some(entity);
            for _ in 0..native::layout::MAX_DEPTH {
                let Some(entity) = current else { return false; };
                if entity == overlay { return true; }
                current = canvas.app.world().get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        }), "The actual long conflict list did not scroll to its last row");
        let frame = settle(&mut canvas);
        capture(&format!("configuration-binding-last-choice-{}", size[0]), size, &frame);
        continue_review(&mut canvas, size);
        assert_eq!(std::fs::read(profile.join("interface.lingua")).unwrap(), external);
        press(&mut canvas, "Save all changes", size);
        assert!(active_dwell(&mut canvas, 900));
        let close = labelled(&mut canvas, lince_interface::i18n::Token::CloseEditMode.text());
        reveal(&mut canvas, close);
        click(&mut canvas, close, size);
        let verify = |frame: &Frame| {
            for (index, (_, action, function)) in commands.iter().enumerate() {
                let modifiers = Modifiers { super_key: true, control: index % 2 == 0, alt: index % 2 == 1, shift: false };
                let bindings = frame.input.bindings.iter().filter(|binding| binding.key == *function && binding.modifiers == modifiers).collect::<Vec<_>>();
                assert_eq!(bindings.len(), 1);
                assert_eq!(bindings[0].action, *action);
            }
        };
        verify(&settle(&mut canvas));
        canvas.finish().unwrap();
        drop(canvas);
        drop(receiver);
        let (mut reopened, _receiver) = launch(&profile, size, scale);
        assert!(active_dwell(&mut reopened, 900));
        let close = labelled(&mut reopened, lince_interface::i18n::Token::CloseEditMode.text());
        reveal(&mut reopened, close);
        click(&mut reopened, close, size);
        verify(&settle(&mut reopened));
        reopened.finish().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_layout_focus_gesture_and_overview_owners_merge_independent_file_fields_then_save_and_reopen_their_real_controls() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        for family in 0..4 {
            let profile = directory.path().join(format!("owner-{family}-{}", size[0]));
            std::fs::create_dir_all(&profile).unwrap();
            let (mut canvas, receiver) = initial(&profile, size, scale);
            let external = authored(&profile, size, scale, |canvas| match family {
                0 => {
                    press(canvas, "Edit window layout", size);
                    for (caption, value) in [("Width preset", "75%"), ("Row height preset", "75%"), ("Default opening width", "40%"), ("Free movement step", "96"), ("Resize step (logical pixels)", "96"), ("Window gaps (0 to 256)", "12")] { write(canvas, caption, value, size); }
                    press(canvas, "Save window layout", size);
                    press(canvas, "Edit focus settings", size);
                    press(canvas, "Focus follows pointer: Off", size);
                    press(canvas, "Save focus settings", size);
                }
                1 => {
                    press(canvas, "Edit focus settings", size);
                    press(canvas, "Focus follows pointer: Off", size);
                    press(canvas, "Warp cursor after keyboard focus: Off", size);
                    press(canvas, "Save focus settings", size);
                }
                2 => {
                    press(canvas, "Edit desktop gestures", size);
                    write(canvas, SENSITIVITY, "150", size);
                    write(canvas, THRESHOLD, "160", size);
                    press(canvas, "Desktop gestures: On", size);
                    press(canvas, "Save desktop gestures", size);
                }
                _ => {
                    press(canvas, "Edit overview transfers", size);
                    write(canvas, DWELL, "900", size);
                    write(canvas, "Edge scroll speed (0–100% of viewport/s)", "25", size);
                    press(canvas, "Save overview transfers", size);
                }
            });
            let (open, review, save) = match family {
                0 => ("Edit window layout", "Review draft against file", "Save window layout"),
                1 => ("Edit focus settings", "Review focus draft against file", "Save focus settings"),
                2 => ("Edit desktop gestures", "Review gesture draft against file", "Save desktop gestures"),
                _ => ("Edit overview transfers", "Review overview draft against file", "Save overview transfers"),
            };
            press(&mut canvas, open, size);
            match family {
                0 => for (caption, value) in [("Width preset", "80%"), ("Row height preset", "80%"), ("Default opening width", "60%"), ("Free movement step", "80"), ("Resize step (logical pixels)", "80"), ("Window gaps (0 to 256)", "10")] { write(&mut canvas, caption, value, size); },
                1 => {
                    press(&mut canvas, "Click to focus: On", size);
                    press(&mut canvas, "Warp cursor after keyboard focus: Off", size);
                }
                2 => write(&mut canvas, SENSITIVITY, "125", size),
                _ => write(&mut canvas, DWELL, "650", size),
            }
            replace(&profile.join("interface.lingua"), &external);
            wait_dwell(&mut canvas, 900);
            press(&mut canvas, review, size);
            let count = match family { 0 => 6, 1 => 0, _ => 1 };
            assert_eq!(count_choices(&mut canvas), count);
            for index in 0..count { choose(&mut canvas, index, family == 0 && index % 2 == 1, index % 2 == 0, size); }
            continue_review(&mut canvas, size);
            assert_eq!(std::fs::read(profile.join("interface.lingua")).unwrap(), external);
            let check = |canvas: &mut Presentation| match family {
                0 => {
                    for (caption, expected) in [("Width preset", "75%"), ("Row height preset", "80%"), ("Default opening width", "60%"), ("Free movement step", "96"), ("Resize step (logical pixels)", "80"), ("Window gaps (0 to 256)", "12")] { assert_eq!(field(canvas, caption), expected); }
                    assert!(native::layout::desktop::focus::policy(canvas.app.world()).pointer_follow);
                }
                1 => {
                    for caption in ["Click to focus: Off", "Focus follows pointer: On", "Warp cursor after keyboard focus: On"] { let _ = named_button(canvas, caption); }
                    assert!(canvas.world_mut().query::<&Text>().iter(canvas.app.world()).any(|text| text.0.contains("cannot warp the system cursor")));
                }
                2 => {
                    assert_eq!(field(canvas, SENSITIVITY), "125");
                    assert_eq!(field(canvas, THRESHOLD), "160");
                    let _ = named_button(canvas, "Desktop gestures: Off");
                }
                _ => {
                    assert_eq!(field(canvas, DWELL), "650");
                    assert_eq!(field(canvas, "Edge scroll speed (0–100% of viewport/s)"), "25");
                }
            };
            check(&mut canvas);
            press(&mut canvas, save, size);
            if family == 0 { press(&mut canvas, open, size); }
            check(&mut canvas);
            let frame = settle(&mut canvas);
            capture(&format!("configuration-owner-{family}-{}", size[0]), size, &frame);
            canvas.finish().unwrap();
            drop(canvas);
            drop(receiver);
            let (mut reopened, _receiver) = launch(&profile, size, scale);
            press(&mut reopened, open, size);
            check(&mut reopened);
            reopened.finish().unwrap();
        }
    }
}

fn rule_control(canvas: &mut Presentation, uid: &str, action: usize, size: [u32; 2]) {
    let entity = named(canvas, &format!("opening.rule.{uid}.control.{action}")).unwrap();
    reveal(canvas, entity);
    click(canvas, entity, size);
    settle(canvas);
}

fn save_order(canvas: &mut Presentation, size: [u32; 2]) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        settle(canvas);
        let save = named_button(canvas, "Save rule order");
        if canvas.app.world().get::<bevy::ui::InteractionDisabled>(save).is_none() { break; }
        assert!(Instant::now() < deadline, "The real Cell rule snapshots did not permit saving the ordinary rule-order draft");
        std::thread::sleep(Duration::from_millis(3));
    }
    press(canvas, "Save rule order", size);
}

fn add_rule(canvas: &mut Presentation, uid: &str, size: [u32; 2]) {
    write(canvas, "Existing opening-rule Record UID", uid, size);
    press(canvas, "Add existing rule", size);
}

fn order_labels(canvas: &mut Presentation, expected: &[&str]) {
    let owner = named(canvas, "park.opening-rules").unwrap();
    let world = canvas.world_mut();
    for (index, title) in expected.iter().enumerate() {
        let prefix = format!("{}. {title} ·", index + 1);
        assert!(world.query::<(Entity, &Text)>().iter(world).any(|(entity, text)| {
            if !text.0.starts_with(&prefix) { return false; }
            let mut current = Some(entity);
            for _ in 0..native::layout::MAX_DEPTH {
                let Some(entity) = current else { return false; };
                if entity == owner { return true; }
                current = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        }), "The ordinary rule-order list did not retain {prefix}");
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_opening_order_review_selects_one_complete_list_preserves_real_rule_records_and_reloads_before_startup() {
    use std::sync::Arc;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let profile = directory.path().join(size[0].to_string());
        std::fs::create_dir_all(&profile).unwrap();
        let engine = Arc::new(runtime.block_on(engine::Engine::open_memory()).unwrap());
        let cell = native::app::CellHandle(cell::CellRuntime { store: engine.store.clone(), engine: engine.clone(), commands: Default::default(), lanes: Arc::new(cell::LaneHub::new()), wire: Default::default(), information: None, speech: None, fiote: None });
        let mut uids = Vec::new();
        for title in ["Rule alpha", "Rule beta", "Rule gamma", "Rule delta"] {
            let uid = runtime.block_on(engine.act(engine::actions::Action::CreateRecord {
                slug: None, kind: nucleus::RecordKind::Plain, head: title.into(), body: "Review keeps this owner-authored rule body".into(), quantity: 0.0,
            }, None)).unwrap().created.unwrap();
            let rule = nucleus::window_opening::Rule { revision: 1, enabled: true, match_all: true,
                conditions: vec![nucleus::property_rule::PropertyRule { property: nucleus::property_rule::Property::ApplicationId, value: "org.example.review".into() }], properties: Default::default() };
            runtime.block_on(engine.act(engine::actions::Action::SetExtension { target: uid.clone(), namespace: nucleus::window_opening::RULE_NAMESPACE.into(), fds: serde_json::to_value(rule).unwrap() }, None)).unwrap();
            uids.push(uid);
        }
        let original_rules = runtime.block_on(store::records::all_extensions(&engine.store.pool, nucleus::window_opening::RULE_NAMESPACE)).unwrap();
        let (mut canvas, receiver) = launch_with_cell(&profile, size, scale, Some(cell.clone()));
        press(&mut canvas, "Edit opening rules", size);
        for uid in &uids[..3] { add_rule(&mut canvas, uid, size); }
        save_order(&mut canvas, size);
        order_labels(&mut canvas, &["Rule alpha", "Rule beta", "Rule gamma"]);
        let author = tempfile::tempdir().unwrap();
        std::fs::copy(profile.join("interface.lingua"), author.path().join("interface.lingua")).unwrap();
        let (mut external_canvas, _external_receiver) = launch_with_cell(author.path(), size, scale, Some(cell.clone()));
        press(&mut external_canvas, "Edit opening rules", size);
        rule_control(&mut external_canvas, &uids[2], 3, size);
        rule_control(&mut external_canvas, &uids[2], 3, size);
        add_rule(&mut external_canvas, &uids[3], size);
        save_order(&mut external_canvas, size);
        press(&mut external_canvas, "Edit focus settings", size);
        press(&mut external_canvas, "Focus follows pointer: Off", size);
        press(&mut external_canvas, "Save focus settings", size);
        press(&mut external_canvas, "Edit overview transfers", size);
        write(&mut external_canvas, DWELL, "900", size);
        press(&mut external_canvas, "Save overview transfers", size);
        external_canvas.finish().unwrap();
        let external = std::fs::read(author.path().join("interface.lingua")).unwrap();
        drop(external_canvas);
        drop(_external_receiver);
        rule_control(&mut canvas, &uids[1], 3, size);
        order_labels(&mut canvas, &["Rule beta", "Rule alpha", "Rule gamma"]);
        replace(&profile.join("interface.lingua"), &external);
        wait_dwell(&mut canvas, 900);
        order_labels(&mut canvas, &["Rule beta", "Rule alpha", "Rule gamma"]);
        press(&mut canvas, "Review rule order against file", size);
        assert_eq!(count_choices(&mut canvas), 1);
        choose(&mut canvas, 0, false, true, size);
        continue_review(&mut canvas, size);
        assert_eq!(std::fs::read(profile.join("interface.lingua")).unwrap(), external);
        order_labels(&mut canvas, &["Rule beta", "Rule alpha", "Rule gamma"]);
        save_order(&mut canvas, size);
        assert!(native::layout::desktop::focus::policy(canvas.app.world()).pointer_follow);
        assert!(active_dwell(&mut canvas, 900));
        assert_eq!(runtime.block_on(store::records::all_extensions(&engine.store.pool, nucleus::window_opening::RULE_NAMESPACE)).unwrap(), original_rules);
        for uid in &uids { assert_eq!(runtime.block_on(store::records::get(&engine.store.pool, uid)).unwrap().unwrap().body, "Review keeps this owner-authored rule body"); }
        let frame = settle(&mut canvas);
        capture(&format!("configuration-rule-order-{}", size[0]), size, &frame);
        canvas.finish().unwrap();
        drop(canvas);
        drop(receiver);
        let (mut reopened, _receiver) = launch_with_cell(&profile, size, scale, Some(cell));
        press(&mut reopened, "Edit opening rules", size);
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            settle(&mut reopened);
            let decision = native::layout::desktop::opening::capture(reopened.app.world(), &serde_json::json!({"app_id":"org.example.review"}), None);
            if decision.matched.as_ref().is_some_and(|matched| matched.uid == uids[1]) { break; }
            assert!(Instant::now() < deadline, "The saved whole rule order was not loaded through actual Startup/Cell subscriptions");
            std::thread::sleep(Duration::from_millis(3));
        }
        order_labels(&mut reopened, &["Rule beta", "Rule alpha", "Rule gamma"]);
        assert!(native::layout::desktop::focus::policy(reopened.app.world()).pointer_follow);
        assert!(active_dwell(&mut reopened, 900));
        assert_eq!(runtime.block_on(store::records::all_extensions(&engine.store.pool, nucleus::window_opening::RULE_NAMESPACE)).unwrap(), original_rules);
        reopened.finish().unwrap();
    }
}
