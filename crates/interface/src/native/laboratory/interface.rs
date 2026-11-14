use super::driver::{Selector, UiCondition};
use crate::native::{actions::Action, edit_mode::EditAction};
use bevy::{a11y::AccessibilityNode, math::DVec2, prelude::*};

pub(super) fn fast_cases() -> Vec<super::Case> {
    let mut cases = Vec::new();
    laboratory_cases(&mut cases);
    let mut readiness = Vec::new();
    crate::native::castle::tests::laboratory_cases(&mut readiness);
    for name in [
        "::layout_snapshots_track_optional_components_commands_and_removals",
        "::presented_castles_keep_their_surface_while_replacement_content_settles",
        "::repeated_glyph_pages_still_require_every_atlas_and_follow_removal",
        "::nested_castles_wait_for_every_image_without_hiding_unrelated_sands",
        "::new_members_and_pending_work_hold_the_whole_group",
        "::readiness_follows_reparenting_visibility_and_late_layout_changes",
    ] {
        let case = readiness
            .iter()
            .position(|case| case.name.ends_with(name))
            .expect("Missing native Castle readiness regression");
        cases.push(readiness.remove(case));
    }
    let mut desktop = Vec::new();
    crate::native::layout::desktop::tests::laboratory_cases(&mut desktop);
    for name in [
        "::two_outputs_resolve_one_canonical_root_without_reparenting_items",
        "::mapping_an_unrelated_window_preserves_fullscreen_and_restores_every_window",
    ] {
        let case = desktop
            .iter()
            .position(|case| case.name.ends_with(name))
            .expect("Missing native desktop lifecycle regression");
        cases.push(desktop.remove(case));
    }
    let mut records = Vec::new();
    crate::native::protein_area::tests::laboratory_cases(&mut records);
    for name in [
        "::local_record_text_refreshes_only_dependent_queries",
        "::record_projection_preserves_requested_fields_and_exact_values",
        "::inactive_rows_defer_materialization_but_follow_visibility_and_revocation",
        "::remote_rows_are_isolated_cancelled_replies_are_ignored_and_large_views_are_bounded",
        "::independent_sorting_uses_live_protein_order_without_spawning_and_clears_on_stop",
    ] {
        let query = records
            .iter()
            .position(|case| case.name.ends_with(name))
            .expect("Missing native query regression");
        cases.push(records.remove(query));
    }
    let mut selected = records.into_iter().filter(|case| {
        case.name
            .ends_with("::record_source_stays_stable_while_paused_and_typing_resumes_at_selection")
    });
    cases.push(
        selected
            .next()
            .expect("Missing native Record selection regression"),
    );
    assert!(
        selected.next().is_none(),
        "Duplicate native Record selection regression"
    );
    crate::native::record_binding::laboratory_cases(&mut cases);
    crate::native::topology::presentation::tests::laboratory_cases(&mut cases);
    crate::native::description::shader::extent::checks::laboratory_cases(&mut cases);
    let mut styles = Vec::new();
    crate::native::token_style::resolver::tests::laboratory_cases(&mut styles);
    cases.extend(styles);
    crate::native::token_metrics::tests::laboratory_cases(&mut cases);
    crate::native::sand::tests::laboratory_cases(&mut cases);
    crate::native::topology::surface_render::tests::laboratory_cases(&mut cases);
    crate::native::information::sync::tests::laboratory_cases(&mut cases);
    crate::native::retained::tests::laboratory_cases(&mut cases);
    crate::native::area_effects::tests::laboratory_cases(&mut cases);
    crate::native::influence_report::tests::laboratory_cases(&mut cases);
    crate::native::topology::influence::tests::laboratory_cases(&mut cases);
    crate::native::topology::area_summary::hover_checks::laboratory_cases(&mut cases);
    crate::native::output_views::tests::laboratory_cases(&mut cases);
    let mut inspection = Vec::new();
    crate::native::inspection::tests::laboratory_cases(&mut inspection);
    cases.push(inspection.into_iter().find(|case| case.name.ends_with(
        "::click_off_hides_inspection_even_when_canvas_selection_sets_a_target"
    )).expect("Missing inspection demand regression"));
    let mut delivery = Vec::new();
    crate::native::cell_bridge::tests::laboratory_cases(&mut delivery);
    cases.push(
        delivery
            .into_iter()
            .find(|case| {
                case.name
                    .ends_with("::independent_consumers_share_payloads_and_preserve_message_order")
            })
            .expect("Missing shared delivery regression"),
    );
    crate::native::workspace::storage::tests::laboratory_cases(&mut cases);
    crate::native::time_castle::wake_checks::laboratory_cases(&mut cases);
    let mut karma = Vec::new();
    crate::native::karma_castle::tests::laboratory_cases(&mut karma);
    cases.push(
        karma
            .into_iter()
            .find(|case| {
                case.name.ends_with(
                    "::execution_failures_show_retry_and_stopped_status_and_clear_after_recovery",
                )
            })
            .expect("Missing Karma runtime status regression"),
    );
    super::showoff::screenshot::laboratory_cases(&mut cases);
    cases
}

#[cfg_attr(test, test)]
fn performance_traces_preserve_concurrent_samples_and_report_overflow() {
    #[cfg(feature = "native-presentation")]
    crate::host::performance::checks::backend_phases_share_the_monitor_and_only_allow_declared_numeric_readings();
    crate::host::performance::checks::concurrent_trace_producers_preserve_every_sample_until_capacity();
    crate::host::performance::checks::input_traces_are_bounded_nonblocking_and_report_loss_without_payloads();
    crate::host::performance::checks::system_samples_survive_concurrent_snapshots_and_shard_reuse();
}

#[cfg_attr(test, test)]
fn retained_exclusive_queries_follow_new_archetypes_and_membership_changes() {
    crate::native::kanban::query_checks::retained_queries_follow_late_boards_parts_and_released_cards();
}

#[cfg_attr(test, test)]
fn untouched_editors_preserve_change_ticks_while_edits_and_cursor_updates_remain_live() {
    use bevy::{
        input_focus::{FocusCause, InputFocus},
        text::{EditableText, EditableTextGeneration, FontSize, PreeditCursor, TextEdit, TextLayoutInfo},
        ui::{ContentSize, widget::TextScroll},
    };
    let (mut app, root) = fixture(UVec2::new(800, 600), 1.0);
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_millis(10),
    ));
    let font = app.world().resource::<crate::native::theme::Typography>().text(18.0);
    let editors = (0..8).map(|index| {
        let mut text = EditableText::new("Idle editor");
        text.visible_lines = Some(1.0);
        app.world_mut().spawn((
            Node {
                position_type: PositionType::Absolute,
                width: px(260),
                height: px(80),
                top: px(index as f32 * 60.0),
                ..default()
            },
            text,
            font.clone(),
            TextLayoutInfo::default(),
            TextScroll::default(),
            ChildOf(root),
        )).id()
    }).collect::<Vec<_>>();
    for _ in 0..6 { app.update(); }
    let ticks = |world: &World, entity: Entity| {
        let entity = world.entity(entity);
        [
            entity.get_ref::<EditableText>().unwrap().last_changed().get(),
            entity.get_ref::<EditableTextGeneration>().unwrap().last_changed().get(),
            entity.get_ref::<ContentSize>().unwrap().last_changed().get(),
            entity.get_ref::<TextLayoutInfo>().unwrap().last_changed().get(),
        ]
    };
    let previous = editors.iter().map(|entity| ticks(app.world(), *entity)).collect::<Vec<_>>();
    for _ in 0..12 { app.update(); }
    for (entity, expected) in editors.iter().zip(&previous) {
        assert_eq!(ticks(app.world(), *entity), *expected, "Untouched editor was marked changed");
        assert!(!app.world().get::<TextLayoutInfo>(*entity).unwrap().glyphs.is_empty());
    }
    let field = editors[0];
    app.world_mut().get_mut::<EditableText>(field).unwrap().queue_edit(TextEdit::SelectAll);
    app.update();
    assert!(!app.world().get::<TextLayoutInfo>(field).unwrap().selection_rects.is_empty());
    let text = "héllo world with several words";
    app.world_mut().get_mut::<EditableText>(field).unwrap().queue_edit(TextEdit::Insert(text.into()));
    app.update();
    assert_eq!(app.world().get::<EditableText>(field).unwrap().value(), text);
    assert!(app.world().get::<TextLayoutInfo>(field).unwrap().selection_rects.is_empty());
    let wide = app.world().get::<TextLayoutInfo>(field).unwrap().size;
    app.world_mut().get_mut::<Node>(field).unwrap().width = px(80);
    app.update();
    assert!(app.world().get::<TextLayoutInfo>(field).unwrap().size.y > wide.y);
    let before_font = app.world().get::<TextLayoutInfo>(field).unwrap().size;
    app.world_mut().get_mut::<TextFont>(field).unwrap().font_size = FontSize::Px(28.0);
    app.update();
    assert!(app.world().get::<TextLayoutInfo>(field).unwrap().size.y > before_font.y);
    let mut input = app.world_mut().get_mut::<EditableText>(field).unwrap();
    input.queue_edit(TextEdit::SelectAll);
    input.queue_edit(TextEdit::Insert("base".into()));
    input.queue_edit(TextEdit::ImeSetCompose {
        value: "é".into(),
        cursor: Some(PreeditCursor { anchor: 2, focus: 2 }),
    });
    app.update();
    assert_eq!(app.world().get::<EditableText>(field).unwrap().value(), "base");
    assert!(app.world().get::<EditableText>(field).unwrap().is_composing());
    assert!(!app.world().get::<TextLayoutInfo>(field).unwrap().preedit_underline_rects.is_empty());
    app.world_mut().get_mut::<EditableText>(field).unwrap().queue_edit(TextEdit::ImeCommit { value: "é".into() });
    app.update();
    assert_eq!(app.world().get::<EditableText>(field).unwrap().value(), "baseé");
    assert!(!app.world().get::<EditableText>(field).unwrap().is_composing());
    assert!(app.world().get::<TextLayoutInfo>(field).unwrap().preedit_underline_rects.is_empty());
    app.world_mut().get_mut::<EditableText>(field).unwrap().cursor_blink_period = std::time::Duration::from_millis(80);
    app.world_mut().resource_mut::<InputFocus>().set(field, FocusCause::Navigated);
    for _ in 0..3 { app.update(); }
    let retained = ticks(app.world(), field);
    let mut cursor_states = [false; 2];
    for _ in 0..16 {
        app.update();
        let cursor = app.world().get::<TextLayoutInfo>(field).unwrap().cursor.unwrap();
        cursor_states[usize::from(cursor.0)] = true;
        assert_eq!(&ticks(app.world(), field)[..3], &retained[..3]);
    }
    assert_eq!(cursor_states, [true, true]);
    app.world_mut().resource_mut::<InputFocus>().clear();
    app.update();
    assert!(!app.world().get::<TextLayoutInfo>(field).unwrap().cursor.unwrap().0);
    for (entity, expected) in editors.iter().zip(&previous).skip(1) {
        assert_eq!(ticks(app.world(), *entity), *expected, "Another editor's update invalidated this field");
    }
}

pub fn run_interface_checks() -> std::io::Result<Vec<super::CaseResult>> {
    let (sender, incoming) = std::sync::mpsc::sync_channel(1);
    let wake = crate::native::wake::WakeSignal::new(move || {
        let _ = sender.try_send(());
    });
    let mut run = super::BehaviorRun::start_interface(Some(wake))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut reported = 0;
    loop {
        run.poll();
        for result in &run.results[reported..] {
            eprintln!(
                "{} {} ({:.1} ms)",
                if result.error.is_none() {
                    "PASS"
                } else {
                    "FAIL"
                },
                result.name,
                result.milliseconds
            );
        }
        reported = run.results.len();
        if run.finished() {
            if run.results.len() != run.total || run.total == 0 {
                return Err(std::io::Error::other(
                    "Laboratory interface coverage is incomplete",
                ));
            }
            return Ok(std::mem::take(&mut run.results));
        }
        if incoming
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .is_err()
        {
            run.cancel();
            return Err(std::io::Error::other(
                "Laboratory interface checks timed out",
            ));
        }
    }
}

#[test]
fn fast_interface_rules() {
    let results = run_interface_checks().unwrap();
    assert!(
        results[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    for name in [
        "::layout_snapshots_track_optional_components_commands_and_removals",
        "::presented_castles_keep_their_surface_while_replacement_content_settles",
        "::two_outputs_resolve_one_canonical_root_without_reparenting_items",
        "::mapping_an_unrelated_window_preserves_fullscreen_and_restores_every_window",
    ] {
        assert!(results.iter().any(|result| result.name.ends_with(name)));
    }
    assert!(results.iter().any(|result| {
        result
            .name
            .ends_with("::local_record_text_refreshes_only_dependent_queries")
    }));
    assert!(results.iter().any(|result| {
        result
            .name
            .ends_with("::record_projection_preserves_requested_fields_and_exact_values")
    }));
    assert!(results.iter().any(|result| {
        result
            .name
            .ends_with("::record_source_stays_stable_while_paused_and_typing_resumes_at_selection")
    }));
    assert!(results.iter().any(|result| {
        result
            .name
            .ends_with("::splice_preserves_unicode_mutations_and_cursors")
    }));
    assert!(results.iter().any(|result| {
        result
            .name
            .ends_with("::journal_updates_remain_incremental_across_acknowledgements")
    }));
    let expected = fast_cases();
    assert_eq!(results.len(), expected.len());
    assert!(
        results
            .iter()
            .zip(&expected)
            .all(|(result, case)| result.name == case.name)
    );
    let catalogue = super::catalogue();
    let missing_or_duplicate: Vec<_> = results.iter().filter(|result| {
        catalogue.iter().filter(|case| case.name == result.name).count() != 1
    }).map(|result| result.name.as_str()).collect();
    assert!(missing_or_duplicate.is_empty(), "{missing_or_duplicate:?}");
    let failures: Vec<_> = results
        .iter()
        .filter(|result| result.error.is_some())
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
}

fn fixture(size: UVec2, scale: f32) -> (App, Entity) {
    let mut app = App::new();
    super::isolate(app.world_mut());
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::input::InputPlugin,
        bevy::picking::DefaultPickingPlugins,
        bevy::window::WindowPlugin {
            primary_window: Some(Window {
                resolution: (
                    (size.as_vec2() * scale).x as u32,
                    (size.as_vec2() * scale).y as u32,
                )
                    .into(),
                visible: false,
                focused: true,
                ..default()
            }),
            ..default()
        },
        bevy::image::ImagePlugin::default(),
        bevy::text::TextPlugin,
        bevy::transform::TransformPlugin,
        bevy::input_focus::InputFocusPlugin,
        bevy::ui_widgets::ButtonPlugin,
        bevy::ui::UiPlugin,
        crate::native::theme::ThemePlugin,
        crate::native::sand::SandPlugin,
        crate::native::workspace::WorkspacePlugin,
        crate::native::edit_mode::EditModePlugin,
    ))
    .add_plugins((
        bevy::mesh::MeshPlugin,
        bevy::camera::visibility::VisibilityPlugin,
        crate::native::icons::IconPlugin,
    ))
    .init_resource::<Assets<TextureAtlasLayout>>()
    .insert_resource(UiScale(scale));
    app.world_mut().spawn((
        Camera2d,
        Camera {
            computed: bevy::camera::ComputedCameraValues {
                target_info: Some(bevy::camera::RenderTargetInfo {
                    physical_size: (size.as_vec2() * scale).as_uvec2(),
                    scale_factor: 1.0,
                }),
                ..default()
            },
            ..default()
        },
    ));
    let root = app
        .world_mut()
        .spawn((
            crate::native::container::BoxRoot,
            Node {
                width: px(size.x),
                height: px(size.y),
                ..default()
            },
        ))
        .id();
    app.update();
    let toggle = app
        .world()
        .get::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .toggle;
    let toolbar = app.world().get::<ChildOf>(toggle).unwrap().parent();
    app.world_mut()
        .entity_mut(toolbar)
        .insert(Visibility::Visible);
    (app, root)
}

fn named(world: &mut World, parent: Entity, name: &str) -> Entity {
    let matches: Vec<_> = world
        .query::<(Entity, &AccessibilityNode)>()
        .iter(world)
        .filter(|(entity, node)| {
            if node.label() != Some(name) {
                return false;
            }
            let mut cursor = Some(*entity);
            while let Some(entity) = cursor {
                if entity == parent {
                    return true;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        })
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(matches.len(), 1, "Expected one {name:?}: {matches:?}");
    matches[0]
}

fn glyphs(world: &World, parent: Entity, expected: &str) -> bool {
    world.get::<Children>(parent).is_some_and(|children| {
        children.iter().any(|entity| {
            world
                .get::<Text>(entity)
                .is_some_and(|text| text.0 == expected)
                && world
                    .get::<ComputedNode>(entity)
                    .is_some_and(|node| node.size().min_element() > 0.0)
                && world
                    .get::<bevy::text::TextLayoutInfo>(entity)
                    .is_some_and(|layout| {
                        layout.glyphs.iter().any(|glyph| {
                            world
                                .resource::<Assets<Image>>()
                                .get(glyph.atlas_info.texture)
                                .is_some_and(|image| {
                                    let rect = glyph.atlas_info.rect;
                                    (rect.min.y as u32..rect.max.y as u32).any(|y| {
                                        (rect.min.x as u32..rect.max.x as u32).any(|x| {
                                            image
                                                .get_color_at(x, y)
                                                .is_ok_and(|color| !color.is_fully_transparent())
                                        })
                                    })
                                })
                        })
                    })
                && world.get::<TextColor>(entity).is_some_and(|color| {
                    !color.0.is_fully_transparent()
                        && world
                            .get::<BackgroundColor>(parent)
                            .is_none_or(|background| background.0 != color.0)
                })
        })
    })
}

fn settle(app: &mut App) {
    let mut previous = Vec::new();
    let mut stable = 0;
    for _ in 0..16 {
        app.update();
        let next: Vec<_> = app
            .world_mut()
            .query::<(
                Entity,
                &ComputedNode,
                &UiGlobalTransform,
                Option<&bevy::text::TextLayoutInfo>,
            )>()
            .iter(app.world())
            .map(|(entity, node, transform, text)| {
                (
                    entity,
                    node.size(),
                    transform.translation,
                    text.map_or(0, |text| text.glyphs.len()),
                )
            })
            .collect();
        stable = if next == previous { stable + 1 } else { 0 };
        if stable >= 2 && next.iter().any(|(_, _, _, glyphs)| *glyphs > 0) {
            return;
        }
        previous = next;
    }
    panic!("Native UI layout did not settle within 16 frames");
}

fn click(app: &mut App, root: Entity, name: &str, condition: UiCondition) {
    use super::driver::{Limits, Runner, Selector, Step, UiAction, UiBackend};
    let mut runner = Runner::new(Limits {
        timeout: std::time::Duration::from_secs(2),
        max_polls: 16,
    });
    let step = Step::action(
        name,
        Selector::new(name).with_role("Button"),
        UiAction::Click,
        condition,
    );
    let report = runner.run(&mut UiBackend::new(app, root), &step);
    assert!(report.is_success(), "{report:?}");
}

#[cfg_attr(test, test)]
fn protein_area_filter_visible_and_selectable() {
    let (mut app, root) = fixture(UVec2::new(1000, 800), 1.0);
    EditAction::Open.apply(app.world_mut(), root);
    let mut area = crate::native::area::InfluenceArea::new(
        crate::native::area::AreaShape::Square,
        DVec2::ZERO,
        DVec2::splat(320.0),
    );
    area.protein = Some(crate::native::protein_area::Config::default());
    let area = crate::native::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    crate::native::protein_area::ConfigureProtein.apply(app.world_mut(), area);
    EditAction::Close.apply(app.world_mut(), root);
    let editor = app
        .world_mut()
        .query::<(Entity, &crate::native::protein_area::QueryEditor)>()
        .iter(app.world())
        .find(|(_, link)| link.0 == area)
        .unwrap()
        .0;
    {
        let mut node = app.world_mut().get_mut::<Node>(editor).unwrap();
        node.width = px(720);
        node.height = px(680);
    }
    settle(&mut app);
    click(
        &mut app,
        editor,
        "Add a condition",
        UiCondition::Visible(Selector::new("Condition").with_role("Button")),
    );
    settle(&mut app);
    let toggle = named(app.world_mut(), editor, "Condition");
    assert!(
        glyphs(app.world(), toggle, "Text contains"),
        "The closed Protein condition caption has no visible glyphs"
    );
    click(
        &mut app,
        editor,
        "Condition",
        UiCondition::Visible(Selector::new("Kind").with_role("Button")),
    );
    settle(&mut app);
    let menu = app
        .world()
        .get::<crate::native::dropdown::Dropdown>(toggle)
        .unwrap()
        .menu;
    let choice = named(app.world_mut(), menu, "Kind");
    assert!(
        glyphs(app.world(), choice, "Kind"),
        "The Protein filter choice has no visible glyphs"
    );
    click(
        &mut app,
        editor,
        "Kind",
        UiCondition::Absent(Selector::new("Kind").with_role("Button")),
    );
    settle(&mut app);
    let toggle = named(app.world_mut(), editor, "Condition");
    assert!(glyphs(app.world(), toggle, "Kind"));
    let query = &app
        .world()
        .get::<crate::native::protein_castle::ProteinCastle>(editor)
        .unwrap()
        .draft
        .query;
    assert!(
        query["where"][0]["all"][0].get("kind_eq").is_some(),
        "{query}"
    );
}

#[cfg_attr(test, test)]
fn edit_panel_usable_bounds() {
    use crate::host::{OutputId, OutputViewport, Rectangle};
    for (size, scale) in [(UVec2::new(360, 640), 1.0), (UVec2::new(1000, 1200), 1.25)] {
        let (mut app, root) = fixture(size, scale);
        app.world_mut()
            .entity_mut(root)
            .insert(crate::native::layout::desktop::HostViewport(
                OutputViewport {
                    id: OutputId(1),
                    bounds: Rectangle::new([0.0; 2], size.as_dvec2().to_array()).unwrap(),
                    usable: Some(
                        Rectangle::new([0.0, 40.0], [f64::from(size.x), f64::from(size.y) - 40.0])
                            .unwrap(),
                    ),
                    physical_size: (size.as_vec2() * scale).as_uvec2().to_array(),
                    raster: crate::host::raster::RasterMapping {
                        origin: [0.0; 2],
                        scale: [f64::from(scale); 2],
                    },
                    workspace: 1,
                    rotation: Default::default(),
                },
            ));
        EditAction::Open.apply(app.world_mut(), root);
        settle(&mut app);
        let mode = app
            .world()
            .get::<crate::native::edit_mode::EditMode>(root)
            .unwrap();
        let panel = crate::native::inspection::bounds(app.world(), mode.panel).unwrap();
        let toggle = crate::native::inspection::bounds(app.world(), mode.toggle).unwrap();
        assert!(
            panel.min.x.abs() < 0.1 && (panel.min.y - 40.0).abs() < 0.1,
            "Panel inset: {panel:?}"
        );
        assert!(
            (panel.max.y - (toggle.min.y - 8.0)).abs() < 0.1,
            "Panel does not fill usable height: {panel:?}, controls {toggle:?}"
        );
        assert!(
            (panel.width() - 420.0_f32.min(size.x as f32)).abs() < 0.1,
            "Panel width: {panel:?}"
        );
        assert!(panel.max.x <= size.x as f32 + 0.1 && panel.max.y <= toggle.min.y);
    }
}

#[cfg_attr(test, test)]
fn shared_dropdown_captions_follow_available_width_and_theme() {
    let (mut app, root) = fixture(UVec2::new(1000, 800), 1.0);
    let panel = app
        .world_mut()
        .spawn((
            Node {
                width: px(120),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    let toggle = crate::native::dropdown::spawn(
        app.world_mut(),
        panel,
        root,
        "Shared selector",
        "First option",
        vec![
            ("First option".into(), Default::default()),
            ("Second option".into(), Default::default()),
        ],
    );
    settle(&mut app);
    for (scheme, width) in [
        (crate::native::tokens::ColorScheme::Dark, 120.0),
        (crate::native::tokens::ColorScheme::Light, 300.0),
    ] {
        app.world_mut()
            .resource_mut::<crate::native::tokens::ThemeSettings>()
            .scheme = scheme;
        app.world_mut().get_mut::<Node>(panel).unwrap().width = px(width);
        settle(&mut app);
        assert!(glyphs(app.world(), toggle, "First option"));
        click(
            &mut app,
            panel,
            "Shared selector",
            UiCondition::Visible(Selector::new("Second option").with_role("Button")),
        );
        settle(&mut app);
        let menu = app
            .world()
            .get::<crate::native::dropdown::Dropdown>(toggle)
            .unwrap()
            .menu;
        for name in ["First option", "Second option"] {
            let choice = named(app.world_mut(), menu, name);
            assert!(
                glyphs(app.world(), choice, name),
                "Missing {name} at {width} in {scheme:?}"
            );
        }
        click(
            &mut app,
            panel,
            "Second option",
            UiCondition::Absent(Selector::new("Second option").with_role("Button")),
        );
        settle(&mut app);
    }
    let caption = app.world().get::<Children>(toggle).unwrap()[0];
    app.world_mut().get_mut::<Node>(caption).unwrap().width = px(0);
    app.world_mut().get_mut::<Node>(caption).unwrap().flex_grow = 0.0;
    settle(&mut app);
    assert!(!glyphs(app.world(), toggle, "First option"));
}

#[cfg_attr(test, test)]
fn sync_history_controls_draw_and_respond_while_live_status_stays_visible() {
    let (mut app, root) = fixture(UVec2::new(1000, 1000), 1.0);
    for enabled in [false, true] {
        crate::native::information::sync::tests::present(app.world_mut(), root, enabled);
        settle(&mut app);
        let caption = if enabled {
            "Save sync history: on"
        } else {
            "Save sync history: off"
        };
        let button = named(app.world_mut(), root, caption);
        assert!(
            glyphs(app.world(), button, caption),
            "History choice must have visible glyphs"
        );
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0.contains("3 waiting to send")
                    && text.0.contains("1 need attention"))
        );
        click(
            &mut app,
            root,
            caption,
            UiCondition::Visible(Selector::new(caption).with_role("Button")),
        );
        assert_eq!(
            crate::native::information::sync::tests::connection_error(app.world()),
            Some("Sync is not connected.")
        );
    }
}

#[cfg_attr(test, test)]
fn retained_layout_keeps_scroll_reparenting_clips_and_input_order_current() {
    let (mut app, root) = fixture(UVec2::new(1000, 700), 1.0);
    let mut panels = Vec::new();
    for left in [20.0, 420.0] {
        panels.push(
            app.world_mut()
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(left),
                        top: px(20),
                        width: px(300),
                        height: px(200),
                        overflow: bevy::ui::Overflow::scroll(),
                        ..default()
                    },
                    ChildOf(root),
                ))
                .id(),
        );
    }
    let body = app
        .world_mut()
        .spawn((
            Node {
                width: px(250),
                height: px(700),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(panels[0]),
        ))
        .id();
    let mut controls = Vec::new();
    for _ in 0..2 {
        controls.push(
            app.world_mut()
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(80),
                        height: px(30),
                        ..default()
                    },
                    ChildOf(body),
                ))
                .id(),
        );
    }
    settle(&mut app);
    let original = app
        .world()
        .get::<UiGlobalTransform>(body)
        .unwrap()
        .translation;
    let original_clip = app
        .world()
        .get::<bevy::ui::CalculatedClip>(body)
        .unwrap()
        .clip;
    app.world_mut()
        .entity_mut(panels[0])
        .insert(ScrollPosition(Vec2::new(0.0, 40.0)));
    app.update();
    assert_eq!(
        app.world()
            .get::<ComputedNode>(panels[0])
            .unwrap()
            .scroll_position,
        Vec2::new(0.0, 40.0)
    );
    assert_eq!(
        app.world()
            .get::<UiGlobalTransform>(body)
            .unwrap()
            .translation,
        original - Vec2::new(0.0, 40.0)
    );
    assert_eq!(
        app.world()
            .get::<bevy::ui::CalculatedClip>(body)
            .unwrap()
            .clip,
        original_clip
    );
    app.world_mut().entity_mut(body).insert(ChildOf(panels[1]));
    app.update();
    assert_eq!(
        app.world()
            .get::<UiGlobalTransform>(body)
            .unwrap()
            .translation,
        original + Vec2::new(400.0, 0.0)
    );
    assert_eq!(
        app.world()
            .get::<bevy::ui::CalculatedClip>(body)
            .unwrap()
            .clip,
        Rect {
            min: original_clip.min + Vec2::new(400.0, 0.0),
            max: original_clip.max + Vec2::new(400.0, 0.0)
        }
    );
    let order = |world: &World, entity| {
        world
            .resource::<bevy::ui::UiStack>()
            .uinodes
            .iter()
            .position(|candidate| *candidate == entity)
    };
    assert!(order(app.world(), controls[0]) < order(app.world(), controls[1]));
    app.world_mut().entity_mut(controls[0]).insert(ZIndex(100));
    app.update();
    assert!(order(app.world(), controls[0]) > order(app.world(), controls[1]));
    app.world_mut().entity_mut(controls[0]).remove::<ZIndex>();
    app.update();
    assert!(order(app.world(), controls[0]) < order(app.world(), controls[1]));
    app.world_mut()
        .entity_mut(panels[1])
        .insert(bevy::ecs::entity_disabling::Disabled);
    app.update();
    assert_eq!(order(app.world(), controls[0]), None);
    app.world_mut().get_mut::<Node>(body).unwrap().width = px(190);
    app.world_mut()
        .entity_mut(panels[1])
        .remove::<bevy::ecs::entity_disabling::Disabled>();
    app.update();
    assert!(order(app.world(), controls[0]).is_some());
    assert_eq!(
        app.world().get::<ComputedNode>(body).unwrap().size().x,
        190.0
    );
}

crate::native::laboratory_cases! {
    protein_area_filter_visible_and_selectable,
    shared_dropdown_captions_follow_available_width_and_theme,
    edit_panel_usable_bounds,
    sync_history_controls_draw_and_respond_while_live_status_stays_visible,
    performance_traces_preserve_concurrent_samples_and_report_overflow,
    retained_exclusive_queries_follow_new_archetypes_and_membership_changes,
    untouched_editors_preserve_change_ticks_while_edits_and_cursor_updates_remain_live,
    retained_layout_keeps_scroll_reparenting_clips_and_input_order_current,
}
