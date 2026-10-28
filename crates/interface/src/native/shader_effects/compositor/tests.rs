use super::*;

fn fixture() -> (World, Entity, [Entity; 3]) {
    let mut world = World::new();
    crate::native::laboratory::isolate(&mut world);
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::native::theme::Typography>();
    world.init_resource::<ShaderSettings>();
    world.init_resource::<InputFocus>();
    let root = world.spawn((Node::default(), crate::native::workspace::Workspaces::default(), InheritedVisibility::VISIBLE)).id();
    let panel = world.spawn((Node::default(), ChildOf(root), InheritedVisibility::VISIBLE)).id();
    super::super::controls(&mut world, root, panel);
    let inputs = world.query_filtered::<Entity, With<Field>>().iter(&world).collect::<Vec<_>>();
    for input in &inputs { world.entity_mut(*input).insert(InheritedVisibility::VISIBLE); }
    (world, root, inputs.try_into().unwrap())
}

fn draft(world: &mut World, field: Entity, value: &str) {
    let mut text = world.get_mut::<EditableText>(field).unwrap();
    text.editor.set_text(value);
    text.pending_edits.clear();
    text.pending_paste = None;
}

#[test]
fn appearance_fields_apply_independently_preserve_invalid_and_conflicting_drafts_and_cancel_explicitly() {
    let (mut world, _, [shadow, blur, fade]) = fixture();
    for (field, value) in [(shadow, "12"), (blur, "8"), (fade, "120")] {
        draft(&mut world, field, value);
        Edit::Apply.apply(&mut world, field);
    }
    fields(&mut world);
    assert_eq!(world.resource::<ShaderSettings>().compositor, Settings { shadow_radius: 12.0, blur_radius: 8.0, appearance_fade_ms: 120 });
    for value in ["NaN", "inf", "-1", "33"] {
        draft(&mut world, shadow, value);
        Edit::Apply.apply(&mut world, shadow);
        fields(&mut world);
        assert_eq!(world.resource::<ShaderSettings>().compositor.shadow_radius, 12.0);
        assert_eq!(world.get::<EditableText>(shadow).unwrap().value(), value);
        let status = world.get::<Field>(shadow).unwrap().status;
        assert!(!world.get::<Text>(status).unwrap().0.is_empty());
    }
    draft(&mut world, fade, "120.5");
    Edit::Apply.apply(&mut world, fade);
    fields(&mut world);
    assert_eq!(world.resource::<ShaderSettings>().compositor.appearance_fade_ms, 120);
    draft(&mut world, shadow, "16");
    world.resource_mut::<ShaderSettings>().compositor.shadow_radius = 20.0;
    Edit::Apply.apply(&mut world, shadow);
    fields(&mut world);
    fields(&mut world);
    assert_eq!(world.get::<EditableText>(shadow).unwrap().value(), "16");
    assert_eq!(world.resource::<ShaderSettings>().compositor.shadow_radius, 20.0);
    Edit::Cancel.apply(&mut world, shadow);
    fields(&mut world);
    assert_eq!(world.get::<EditableText>(shadow).unwrap().value(), "20");
    assert_eq!(world.resource::<ShaderSettings>().compositor.blur_radius, 8.0);
}

#[test]
fn replaced_canvas_and_locked_controls_cannot_apply_an_older_appearance_action() {
    let (mut world, root, [shadow, _, _]) = fixture();
    draft(&mut world, shadow, "12");
    world.get_mut::<crate::native::workspace::Workspaces>(root).unwrap().canvas_id = nucleus::new_uid("canvas");
    Edit::Apply.apply(&mut world, shadow);
    fields(&mut world);
    assert_eq!(world.resource::<ShaderSettings>().compositor.shadow_radius, 0.0);
    assert_eq!(world.get::<EditableText>(shadow).unwrap().value(), "12");
    let (mut world, _, [shadow, _, _]) = fixture();
    draft(&mut world, shadow, "12");
    world.insert_resource(crate::host::control::Status { available: true, active: true, locked: true, ..Default::default() });
    Edit::Apply.apply(&mut world, shadow);
    fields(&mut world);
    assert_eq!(world.resource::<ShaderSettings>().compositor.shadow_radius, 0.0);
    assert_eq!(world.get::<EditableText>(shadow).unwrap().value(), "12");
}

#[test]
fn reduced_motion_and_protection_remove_only_the_transient_fade_without_rewriting_preferences() {
    let (mut world, root, _) = fixture();
    let item = world.spawn((crate::native::canvas::CanvasItem { position: default(), size: Vec2::new(200.0, 100.0) }, ChildOf(root))).id();
    world.resource_mut::<ShaderSettings>().compositor = Settings { shadow_radius: 12.0, blur_radius: 8.0, appearance_fade_ms: 120 };
    world.entity_mut(item).insert((Opacity(0.2), Fade { started: Some(Instant::now()), from: 0.0, focused: false, finished: false }));
    world.resource_mut::<ShaderSettings>().reduced_motion = true;
    animate(&mut world);
    assert!(world.get::<Opacity>(item).is_none() && world.get::<Fade>(item).is_none());
    assert_eq!(world.resource::<ShaderSettings>().compositor.appearance_fade_ms, 120);
    world.resource_mut::<ShaderSettings>().reduced_motion = false;
    world.entity_mut(item).insert((Opacity(0.2), Fade { started: Some(Instant::now()), from: 0.0, focused: false, finished: false }));
    world.insert_resource(crate::host::control::Status { available: true, active: true, locked: true, ..Default::default() });
    animate(&mut world);
    assert!(world.get::<Opacity>(item).is_none() && world.get::<Fade>(item).is_none());
    assert_eq!(world.resource::<ShaderSettings>().compositor.shadow_radius, 12.0);
    world.insert_resource(crate::host::control::Status::default());
    world.entity_mut(item).insert((Opacity(0.2), Fade { started: Some(Instant::now()), from: 0.0, focused: false, finished: false }));
    super::super::set_compositor_presentation_active(&mut world, false);
    animate(&mut world);
    assert!(world.get::<Opacity>(item).is_none() && world.get::<Fade>(item).is_none());
    assert_eq!(world.resource::<ShaderSettings>().compositor.appearance_fade_ms, 120);
}

#[test]
fn appearance_status_reports_pending_and_limited_effects_without_rewriting_saved_settings() {
    let (mut world, _, _) = fixture();
    let settings = Settings { shadow_radius: 12.0, blur_radius: 8.0, appearance_fade_ms: 120 };
    world.resource_mut::<ShaderSettings>().compositor = settings;
    world.insert_resource(crate::native::topology::compositor::Status { shadow_limited: 1, blur_limited: 2, blur_pending: 3,
        shadow_limits: std::collections::BTreeSet::from([crate::native::topology::compositor::Limitation::Budget]),
        blur_limits: std::collections::BTreeSet::from([crate::native::topology::compositor::Limitation::Shader]),
    });
    fields(&mut world);
    let label = world.query_filtered::<Entity, With<RenderStatus>>().single(&world).unwrap();
    let message = &world.get::<Text>(label).unwrap().0;
    assert!(message.contains("shadows") && message.contains("background blur") && message.contains("Preparing"));
    assert!(message.contains("budget is full") && message.contains("shader"));
    assert_eq!(world.resource::<ShaderSettings>().compositor, settings);
    world.insert_resource(crate::native::topology::compositor::Status::default());
    fields(&mut world);
    assert!(world.get::<Text>(label).unwrap().0.is_empty());
    assert_eq!(world.resource::<ShaderSettings>().compositor, settings);
}

#[test]
fn appearance_toggles_refresh_visible_and_accessible_state_without_replacing_numeric_drafts() {
    let (mut world, root, [shadow, _, _]) = fixture();
    draft(&mut world, shadow, "16");
    let buttons = world.query::<(Entity, &super::super::SettingsToggle)>().iter(&world).map(|(entity, kind)| (entity, *kind)).collect::<Vec<_>>();
    super::super::ToggleAll.apply(&mut world, root);
    super::super::ToggleMotion.apply(&mut world, root);
    fields(&mut world);
    for (button, kind) in buttons {
        let caption = match kind {
            super::super::SettingsToggle::Effects => "Shader effects: Off",
            super::super::SettingsToggle::Motion => "Shader reduced motion: On",
        };
        assert_eq!(world.get::<bevy::a11y::AccessibilityNode>(button).unwrap().label(), Some(caption));
        assert_eq!(world.get::<crate::native::icons::Tooltip>(button).unwrap().0, caption);
        assert!(world.get::<Children>(button).unwrap().into_iter().any(|child| world.get::<Text>(*child).is_some_and(|text| text.0 == caption)));
    }
    assert_eq!(world.get::<EditableText>(shadow).unwrap().value(), "16");
    assert_eq!(world.resource::<ShaderSettings>().compositor.shadow_radius, 0.0);
    assert!(world.resource::<ShaderSettings>().disabled && world.resource::<ShaderSettings>().reduced_motion);
}
