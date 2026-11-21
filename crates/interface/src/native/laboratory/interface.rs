// non-lean-yet
// Scope: mod protein_visuals, mod protein_lifecycle, fn fast_cases, fn select_cases.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 227, 284, 370,
//   415, 423, 495, 519, 885, 1125, 1241, 1243, 1247, 1252, 1257, 1267, 1283, 1284, 1288, 1289,
//   1306, 1308, 1312, 1313, 1317, 1322, 1330, 1332, 1336, 1350, 1351, 1355, 1357, 1361, 1413.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (30, 42, 58,
//   68, 89, 106, 116, 130, 1102, 1118, 1120, 1155, 1175, 1184).
// - [review: §5 Panics] Check whether explicit panics can be reached by ordinary failure. Sites
//   982, 1045, 1075.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   573, 574, 610.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (237, 239, 240, 241, 255, 259, 312, 313, 317, 336, 340, 353, 355, 358, 360, 365,
//   366, 370, 371, 378, 380, 383, 384, 387, 392, 396, 397, 418, 433, 434, 505, 506, 507, 508, 515,
//   518, 519, 520, 522, 523, 525, 533, 535, 537, 539, 540, 541, 542, 544, 545, 546, 548, 549, 557,
//   558, 559, 560, 562, 563, 564, 565, 572, 579, 852, 854, 1372, 1380, 1398, 1570, 1573, 1589,
//   1594, 1596, 1617, 1618, 1619, 1673, 1686, 1703, 1704, 1705, 1828, 1832, 1846, 1853, 1859, 1881,
//   1911, 1918).
// - [Clippy probe: clippy::allow_attributes] Use a reasoned expect when the diagnostic reliably
//   fires. Sites 205, 265, 402, 468, 804, 1190, 1557, 1637, 1747.
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 443.

use super::driver::{Limits, Runner, Selector, Step, UiAction, UiBackend, UiCondition};
use crate::native::{actions::Action, edit_mode::EditAction};
use crate::testing::InterfaceSelection;
use bevy::{a11y::AccessibilityNode, camera::RenderTarget, math::DVec2, prelude::*};

#[path = "interface/protein_visuals.rs"]
mod protein_visuals;
#[cfg(test)]
#[path = "interface/protein_lifecycle.rs"]
mod protein_lifecycle;

pub(super) fn fast_cases() -> Vec<super::Case> {
    let mut cases = Vec::new();
    laboratory_cases(&mut cases);
    crate::native::accessibility::laboratory_cases(&mut cases);
    super::driver::laboratory_cases(&mut cases);
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
        "::field_settings_retain_unchanged_wrapping_and_apply_real_changes",
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
    let mut paint = Vec::new();
    crate::native::token_style::tests::laboratory_cases(&mut paint);
    cases.push(
        paint
            .into_iter()
            .find(|case| {
                case.name
                    .ends_with("::linear_paint_preserves_authored_colors_and_stable_component_ticks")
            })
            .expect("Missing native paint color regression"),
    );
    crate::native::token_metrics::tests::laboratory_cases(&mut cases);
    crate::native::sand::tests::laboratory_cases(&mut cases);
    crate::native::topology::surface_render::tests::laboratory_cases(&mut cases);
    crate::native::information::sync::tests::laboratory_cases(&mut cases);
    crate::native::retained::tests::laboratory_cases(&mut cases);
    crate::native::kanban::laboratory_cases(&mut cases);
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

pub(super) fn select_cases(
    cases: &[super::Case],
    selection: InterfaceSelection,
) -> std::io::Result<Vec<super::Case>> {
    let Some((mandatory, rest)) = cases.split_first() else {
        return Err(std::io::Error::other("No Interface checks are registered"));
    };
    let short_name = mandatory.name.rsplit("::").next();
    if short_name != Some(super::requirements::PROTEIN_FILTER_CASE) {
        return Err(std::io::Error::other(
            "The mandatory Protein Add filter case must run first",
        ));
    }
    let mut selected = vec![*mandatory];
    selected.extend(rest.iter().copied().filter(|case| {
        match selection {
            InterfaceSelection::All => true,
            InterfaceSelection::Protein => {
                case.name.contains("::protein_area::")
                    || case
                        .name
                        .ends_with("::shared_dropdown_captions_follow_available_width_and_theme")
            }
            InterfaceSelection::Edit => {
                case.name.contains("::record_binding::")
                    || case.name.ends_with("::edit_panel_usable_bounds")
                    || case.name.ends_with(
                        "::workspace_passwords_retain_idle_layout_and_preserve_masking_and_edits",
                    )
            }
            InterfaceSelection::Input => {
                case.name.contains("::laboratory::driver::")
                    || case.name.contains("::accessibility::")
                    || case.name.ends_with(
                        "::retained_layout_keeps_scroll_reparenting_clips_and_input_order_current",
                    )
            }
        }
    }));
    if selection != InterfaceSelection::All && selected.len() == 1 {
        return Err(std::io::Error::other(format!(
            "No Interface checks match the {:?} selection",
            selection.name()
        )));
    }
    Ok(selected)
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
fn workspace_passwords_retain_idle_layout_and_preserve_masking_and_edits() {
    let (mut app, root) = fixture(UVec2::new(1000, 1000), 1.0);
    crate::native::workspace_sync::checks::password_retention(&mut app, root);
}

#[cfg_attr(test, test)]
#[allow(
    clippy::unwrap_used,
    reason = "The font regression case must fail if a fixture glyph or text component is missing"
)]
fn inherited_font_changes_prepare_current_glyphs_once_in_the_same_update() {
    use bevy::text::{FontSize, TextLayoutInfo};
    use crate::native::tokens::{Token, TokenOverrides, TokenValue};

    let (mut app, root) = fixture(UVec2::new(800, 600), 1.0);
    let parent = app.world_mut().spawn((
        Node { width: px(500), height: px(150), ..default() }, ChildOf(root),
    )).id();
    let font = app.world().resource::<crate::native::theme::Typography>().text(20.0);
    let entities = [parent, root].map(|parent| {
        app.world_mut().spawn((
            Node {
                position_type: PositionType::Absolute,
                width: px(400),
                height: px(120),
                ..default()
            },
            Text::new("Current café 日本語 text"),
            font.clone(),
            ChildOf(parent),
        )).id()
    });
    settle(&mut app);

    for size in [40.0, 10.0, 20.0] {
        let mut overrides = TokenOverrides::default();
        overrides.set(Token::FontSize, TokenValue::Number(16.0 * size / 20.0));
        app.world_mut().entity_mut(parent).insert(overrides);
        app.world_mut().get_mut::<TextFont>(entities[1]).unwrap().font_size = FontSize::Px(size);
        app.update();
        let actual = app.world().get::<TextLayoutInfo>(entities[0]).unwrap();
        let expected = app.world().get::<TextLayoutInfo>(entities[1]).unwrap();
        assert_eq!(app.world().get::<TextFont>(entities[0]).unwrap().font_size, FontSize::Px(size));
        assert_eq!(actual.size, expected.size, "Inherited font was not measured in the same update");
        assert_eq!(actual.scale_factor, expected.scale_factor);
        assert!(!actual.glyphs.is_empty());
        assert_eq!(actual.glyphs.len(), expected.glyphs.len());
        for (actual, expected) in actual.glyphs.iter().zip(&expected.glyphs) {
            assert_eq!(actual.position, expected.position);
            assert_eq!(actual.section_index, expected.section_index);
            assert_eq!(actual.line_index, expected.line_index);
            assert_eq!(actual.atlas_info.texture, expected.atlas_info.texture);
            assert_eq!(actual.atlas_info.rect, expected.atlas_info.rect);
            assert_eq!(actual.atlas_info.offset, expected.atlas_info.offset);
        }
        let measured = entities.map(|entity| app.world().entity(entity)
            .get_ref::<bevy::ui::ContentSize>().unwrap().last_changed());
        app.update();
        for (entity, measured) in entities.into_iter().zip(measured) {
            assert_eq!(app.world().entity(entity).get_ref::<bevy::ui::ContentSize>()
                .unwrap().last_changed(), measured, "Unchanged text was measured again");
        }
    }
}

#[cfg_attr(test, test)]
#[allow(
    clippy::unwrap_used,
    reason = "The visibility regression case requires all fixture text components"
)]
fn hidden_text_defers_visual_work_and_reveals_current_edits_and_styles() {
    use bevy::text::{EditableText, FontHinting, FontSize, TextEdit, TextLayoutInfo};
    use bevy::ui::widget::TextScroll;

    fn pair(app: &mut App, parent: Entity, display: Display, visibility: Visibility) -> [Entity; 2] {
        let font = app.world().resource::<crate::native::theme::Typography>().text(18.0);
        let node = Node {
            position_type: PositionType::Absolute,
            width: px(280),
            height: px(120),
            display,
            ..default()
        };
        [
            app.world_mut().spawn((
                node.clone(), Text::new("Initial café title"), font.clone(),
                visibility, ChildOf(parent),
            )).id(),
            app.world_mut().spawn((
                node, EditableText::new("Initial café title"), font,
                TextLayoutInfo::default(), TextScroll::default(), visibility, ChildOf(parent),
            )).id(),
        ]
    }

    fn assert_layout(actual: &TextLayoutInfo, expected: &TextLayoutInfo) {
        assert_eq!(actual.size, expected.size);
        assert_eq!(actual.scale_factor, expected.scale_factor);
        assert_eq!(actual.glyphs.len(), expected.glyphs.len());
        assert!(!actual.glyphs.is_empty());
        for (a, b) in actual.glyphs.iter().zip(&expected.glyphs) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.section_index, b.section_index);
            assert_eq!(a.line_index, b.line_index);
            assert_eq!(a.atlas_info.texture, b.atlas_info.texture);
            assert_eq!(a.atlas_info.rect, b.atlas_info.rect);
            assert_eq!(a.atlas_info.offset, b.atlas_info.offset);
        }
        assert_eq!(actual.selection_rects, expected.selection_rects);
        assert_eq!(actual.preedit_underline_rects, expected.preedit_underline_rects);
    }

    fn replace(app: &mut App, pair: [Entity; 2], text: &str, size: f32) {
        app.world_mut().get_mut::<Text>(pair[0]).unwrap().0 = text.into();
        let mut editable = app.world_mut().get_mut::<EditableText>(pair[1]).unwrap();
        editable.queue_edit(TextEdit::SelectAll);
        editable.queue_edit(TextEdit::Insert(text.into()));
        for entity in pair {
            app.world_mut().get_mut::<TextFont>(entity).unwrap().font_size = FontSize::Px(size);
        }
    }

    let (mut app, root) = fixture(UVec2::new(800, 600), 1.0);
    let parent = app.world_mut().spawn((Node {
        position_type: PositionType::Absolute,
        width: px(400),
        height: px(300),
        display: Display::None,
        ..default()
    }, ChildOf(root))).id();
    let ghost = app.world_mut().spawn((bevy::ui::experimental::GhostNode, ChildOf(parent))).id();
    let inherited = pair(&mut app, ghost, Display::Flex, Visibility::Inherited);
    let direct = pair(&mut app, root, Display::None, Visibility::Inherited);
    let preparing = pair(&mut app, root, Display::Flex, Visibility::Hidden);
    let reference = pair(&mut app, root, Display::Flex, Visibility::Inherited);
    for _ in 0..4 { app.update(); }
    for entity in inherited.into_iter().chain(direct) {
        assert!(app.world().get::<TextLayoutInfo>(entity).unwrap().glyphs.is_empty(),
            "Display::None text prepared glyphs before its first reveal");
    }
    for entity in preparing.into_iter().chain(reference) {
        assert!(!app.world().get::<TextLayoutInfo>(entity).unwrap().glyphs.is_empty(),
            "Visibility::Hidden must still prepare Castle text");
    }

    let current = "A changed café title wraps with several more words";
    for group in [inherited, direct, reference] {
        replace(&mut app, group, current, 22.0);
        for entity in group {
            app.world_mut().entity_mut(entity).insert(FontHinting::Disabled);
        }
    }
    for _ in 0..4 { app.update(); }
    for group in [inherited, direct] {
        assert_eq!(app.world().get::<EditableText>(group[1]).unwrap().value(), current);
        for entity in group {
            assert!(app.world().get::<TextLayoutInfo>(entity).unwrap().glyphs.is_empty());
        }
    }
    app.world_mut().get_mut::<Node>(parent).unwrap().display = Display::Flex;
    for entity in direct {
        app.world_mut().get_mut::<Node>(entity).unwrap().display = Display::Flex;
    }
    for _ in 0..2 { app.update(); }
    for group in [inherited, direct] {
        for (actual, expected) in group.into_iter().zip(reference) {
            assert_layout(app.world().get::<TextLayoutInfo>(actual).unwrap(),
                app.world().get::<TextLayoutInfo>(expected).unwrap());
        }
    }

    let retained = inherited.map(|entity| app.world().get::<TextLayoutInfo>(entity).unwrap().clone());
    app.world_mut().get_mut::<Node>(parent).unwrap().display = Display::None;
    app.update();
    for entity in inherited.into_iter().chain(reference) {
        app.world_mut().entity_mut(entity).insert(FontHinting::Enabled);
    }
    for _ in 0..4 { app.update(); }
    for (entity, layout) in inherited.into_iter().zip(&retained) {
        assert_layout(app.world().get::<TextLayoutInfo>(entity).unwrap(), layout);
    }
    app.world_mut().get_mut::<Node>(parent).unwrap().display = Display::Flex;
    for _ in 0..2 { app.update(); }
    for (actual, expected) in inherited.into_iter().zip(reference) {
        assert_layout(app.world().get::<TextLayoutInfo>(actual).unwrap(),
            app.world().get::<TextLayoutInfo>(expected).unwrap());
    }

    app.world_mut().get_mut::<Node>(parent).unwrap().display = Display::None;
    for group in [inherited, reference] {
        replace(&mut app, group, "Reparented résumé", 28.0);
    }
    for _ in 0..4 { app.update(); }
    assert_eq!(app.world().get::<EditableText>(inherited[1]).unwrap().value(), "Reparented résumé");
    for entity in inherited { app.world_mut().entity_mut(entity).insert(ChildOf(root)); }
    for _ in 0..2 { app.update(); }
    for (actual, expected) in inherited.into_iter().zip(reference) {
        assert_layout(app.world().get::<TextLayoutInfo>(actual).unwrap(),
            app.world().get::<TextLayoutInfo>(expected).unwrap());
    }
}

#[cfg_attr(test, test)]
#[allow(
    clippy::unwrap_used,
    reason = "The cursor regression case treats absent fixture editor state as a failed assertion"
)]
fn logical_end_cursor_preserves_text_and_selection_without_eager_shaping() {
    use bevy::text::{FontCx, LayoutCx, TextBrush};

    fn check(
        editor: &mut parley::PlainEditor<TextBrush>,
        fonts: &mut parley::FontContext,
        layouts: &mut parley::LayoutContext<TextBrush>,
    ) {
        let dirty = editor.try_layout().is_none();
        let mut reference = editor.clone();
        reference.refresh_layout(fonts, layouts);
        let expected = reference.raw_selection().move_lines(
            reference.try_layout().unwrap(),
            isize::MAX,
            false,
        );
        let text = editor.raw_text().to_string();
        let compose = editor.raw_compose().clone();
        editor.driver(fonts, layouts).move_to_text_end();
        assert_eq!(editor.try_layout().is_none(), dirty, "Cursor movement shaped the buffer");
        assert_eq!(editor.raw_selection().focus(), expected.focus());
        assert_eq!(editor.raw_selection().anchor(), expected.anchor());
        assert_eq!(editor.raw_text(), text);
        assert_eq!(editor.raw_compose(), &compose);
        editor.refresh_layout(fonts, layouts);
        assert_eq!(editor.raw_selection().focus(), expected.focus());
        assert_eq!(editor.raw_selection().anchor(), expected.anchor());
        let actual = editor.try_layout().unwrap();
        let reference = reference.try_layout().unwrap();
        assert_eq!(actual.width(), reference.width());
        assert_eq!(actual.height(), reference.height());
        assert_eq!(actual.len(), reference.len());
        assert_eq!(editor.raw_selection().focus().geometry(actual, 1.0),
            expected.focus().geometry(reference, 1.0));
    }

    let (mut app, _) = fixture(UVec2::new(800, 600), 1.0);
    app.world_mut().resource_scope(|world, mut fonts: Mut<FontCx>| {
        let mut layouts = world.resource_mut::<LayoutCx>();
        let long = "Unicode café 日本語 and another wrapped line.\n".repeat(100);
        for text in ["", "Plain title", "e\u{301}", "👩‍👩‍👧‍👦", "العربية", "abc אבג",
            "first\nsecond", "trailing\n", "trailing\r\n", "trailing\u{2028}",
            "trailing\u{2029}", long.as_str()]
        {
            let mut editor = parley::PlainEditor::<TextBrush>::new(18.0);
            editor.set_text(text);
            editor.set_width(Some(180.0));
            check(&mut editor, &mut fonts.context, &mut layouts.0);
            check(&mut editor, &mut fonts.context, &mut layouts.0);
            editor.driver(&mut fonts.context, &mut layouts.0).insert_or_replace_selection("é");
            assert_eq!(editor.raw_text(), format!("{text}é"));
            editor.set_text("Short replacement");
            check(&mut editor, &mut fonts.context, &mut layouts.0);
            editor.driver(&mut fonts.context, &mut layouts.0).set_compose("東京", Some((6, 6)));
            editor.set_width(Some(80.0));
            check(&mut editor, &mut fonts.context, &mut layouts.0);
            assert!(editor.is_composing());
        }
    });
}

#[cfg_attr(test, test)]
#[allow(
    clippy::unwrap_used,
    reason = "The editor change-tick case requires text, cursor, and fixture observation state"
)]
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
    let generation = app.world().get::<EditableText>(field).unwrap().editor().generation();
    let layout = app.world().get::<TextLayoutInfo>(field).unwrap().clone();
    app.world_mut().get_mut::<Node>(field).unwrap().height = px(100);
    app.update();
    assert_eq!(app.world().get::<ComputedNode>(field).unwrap().size().y, 100.0);
    assert!(app.world().get::<EditableText>(field).unwrap().editor().generation() == generation,
        "Height-only geometry change reshaped the editor");
    let retained = app.world().get::<TextLayoutInfo>(field).unwrap();
    assert_eq!(retained.size, layout.size);
    assert_eq!(retained.glyphs.len(), layout.glyphs.len());
    for (actual, expected) in retained.glyphs.iter().zip(&layout.glyphs) {
        assert_eq!(actual.position, expected.position);
        assert_eq!(actual.atlas_info.texture, expected.atlas_info.texture);
        assert_eq!(actual.atlas_info.rect, expected.atlas_info.rect);
    }
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

const INTERFACE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub fn run_interface_checks() -> std::io::Result<Vec<super::CaseResult>> {
    run_interface_checks_with_timeout(INTERFACE_TIMEOUT)
}

pub fn run_interface_checks_with_timeout(
    timeout: std::time::Duration,
) -> std::io::Result<Vec<super::CaseResult>> {
    run_selected_interface_checks(InterfaceSelection::All, timeout)
}

fn run_selected_interface_checks(
    selection: InterfaceSelection,
    timeout: std::time::Duration,
) -> std::io::Result<Vec<super::CaseResult>> {
    let (sender, incoming) = std::sync::mpsc::sync_channel(1);
    let wake = crate::native::wake::WakeSignal::new(move || {
        let _ = sender.try_send(());
    });
    let mut run = super::BehaviorRun::start_interface_selected(Some(wake), selection)?;
    let deadline = std::time::Instant::now() + timeout;
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
            if let Some(source) = result.source {
                eprintln!(
                    "  Registered at {}:{}",
                    source.path, source.registration_line
                );
            }
            for rule in &result.rules {
                eprintln!(
                    "  [{}] {}:{}",
                    rule.requirement.id, rule.source, rule.requirement.line
                );
            }
            if let Some(error) = &result.error {
                eprintln!("  {error}");
            }
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
    let selection = match std::env::var(InterfaceSelection::ENV) {
        Ok(value) => value.parse().unwrap_or_else(|error| panic!("{error}")),
        Err(std::env::VarError::NotPresent) => InterfaceSelection::All,
        Err(error) => panic!("Could not read Interface selection: {error}"),
    };
    let results = run_selected_interface_checks(selection, INTERFACE_TIMEOUT).unwrap();
    let expected = select_cases(&fast_cases(), selection).unwrap();
    assert_eq!(results.len(), expected.len());
    for (result, case) in results.iter().zip(expected) {
        assert_eq!(result.name, case.name);
        let source = result
            .source
            .expect("Every completed Interface case has a source");
        assert_eq!(source.path, case.source.path);
        assert_eq!(source.registration_line, case.source.registration_line);
    }
    assert!(
        results[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    assert_eq!(results[0].rules.len(), 2);
    let failures: Vec<_> = results
        .iter()
        .filter(|result| result.error.is_some())
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
    if selection != InterfaceSelection::All {
        return;
    }
    for name in [
        "::layout_snapshots_track_optional_components_commands_and_removals",
        "::presented_castles_keep_their_surface_while_replacement_content_settles",
        "::two_outputs_resolve_one_canonical_root_without_reparenting_items",
        "::mapping_an_unrelated_window_preserves_fullscreen_and_restores_every_window",
        "::column_labels_follow_late_creation_reparenting_and_removed_roles",
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
    let catalogue = super::catalogue();
    let missing_or_duplicate: Vec<_> = results
        .iter()
        .filter(|result| {
            catalogue
                .iter()
                .filter(|case| case.name == result.name)
                .count()
                != 1
        })
        .map(|result| result.name.as_str())
        .collect();
    assert!(missing_or_duplicate.is_empty(), "{missing_or_duplicate:?}");
}

#[test]
fn interface_selections_preserve_order_and_require_a_matching_feature() {
    let cases = fast_cases();
    for selection in [
        InterfaceSelection::Protein,
        InterfaceSelection::Edit,
        InterfaceSelection::Input,
    ] {
        let subset = select_cases(&cases, selection).unwrap();
        assert_eq!(subset[0].name, cases[0].name);
        assert!(subset.len() > 1 && subset.len() < cases.len());
        let positions: Vec<_> = subset
            .iter()
            .map(|case| {
                cases
                    .iter()
                    .position(|candidate| candidate.name == case.name)
                    .unwrap()
            })
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(select_cases(&cases[..1], selection).is_err());
    }
    assert!(select_cases(&[], InterfaceSelection::All).is_err());
    assert!(select_cases(&cases[1..], InterfaceSelection::All).is_err());
    assert!("".parse::<InterfaceSelection>().is_err());
    assert!("missing-feature".parse::<InterfaceSelection>().is_err());
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
    ))
    .add_plugins((
        bevy::mesh::MeshPlugin,
        bevy::camera::visibility::VisibilityPlugin,
        bevy::input_focus::InputDispatchPlugin,
    ));
    prepare_fixture(app, size, scale, RenderTarget::default())
}

#[allow(
    clippy::unwrap_used,
    reason = "Missing edit controls make the shared interface fixture invalid"
)]
fn prepare_fixture(mut app: App, size: UVec2, scale: f32, target: RenderTarget) -> (App, Entity) {
    app.add_plugins((
        crate::native::theme::ThemePlugin,
        crate::native::sand::SandPlugin,
        crate::native::workspace::WorkspacePlugin,
        crate::native::edit_mode::EditModePlugin,
        bevy::input_focus::tab_navigation::TabNavigationPlugin,
        bevy::input_focus::pointer_focus::PointerFocusPlugin,
        crate::native::icons::IconPlugin,
    ))
    .init_resource::<Assets<TextureAtlasLayout>>()
    .insert_resource(UiScale(scale))
    .insert_resource(ClearColor(crate::native::theme::PAPER));
    app.world_mut().spawn((
        Camera2d,
        target,
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
    app.finish();
    app.cleanup();
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

fn rendered_fixture(size: UVec2, configure: impl FnOnce(&mut App)) -> (App, Entity) {
    use bevy::{
        picking::{input::PointerInputPlugin, pointer::PointerId},
        render::render_resource::TextureFormat,
    };
    let mut app = App::new();
    super::isolate(app.world_mut());
    let mut plugins = super::showoff::screenshot::offscreen_plugins(size.to_array())
        .disable::<PointerInputPlugin>();
    if plugins.contains::<bevy::ui_widgets::MenuPlugin>() {
        plugins = plugins.disable::<bevy::ui_widgets::MenuPlugin>();
    }
    app.add_plugins(plugins);
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            size.x,
            size.y,
            TextureFormat::Bgra8UnormSrgb,
            None,
        ));
    app.world_mut().spawn(PointerId::Mouse);
    let target = RenderTarget::Image(target.into());
    app.insert_resource(crate::native::app::OutputTarget(target.clone()));
    configure(&mut app);
    prepare_fixture(app, size, 1.0, target)
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
                            .is_none_or(|background| {
                                background.0.to_linear() != color.0.to_linear()
                            })
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

fn drive_step(
    app: &mut App,
    root: Entity,
    step: Step<UiAction, UiCondition>,
) -> crate::testing::Report {
    let mut runner = Runner::new(Limits {
        timeout: std::time::Duration::from_secs(2),
        max_polls: 16,
    });
    let target = app
        .world()
        .get_resource::<crate::native::app::OutputTarget>()
        .and_then(|target| target.0.normalize(None));
    let mut backend = UiBackend::new(app, root);
    if let Some(target) = target {
        backend = backend.with_pointer_target(target);
    }
    runner.run(&mut backend, &step)
}

fn click_step(name: &str, condition: UiCondition) -> Step<UiAction, UiCondition> {
    Step::action(
        name,
        Selector::new(name).with_role("Button"),
        UiAction::Click,
        condition,
    )
}

fn click(app: &mut App, root: Entity, name: &str, condition: UiCondition) {
    let report = drive_step(app, root, click_step(name, condition));
    assert!(report.is_success(), "{report:?}");
}

fn protein_step(app: &mut App, root: Entity, step: Step<UiAction, UiCondition>) -> bool {
    let report = drive_step(app, root, step);
    if report.outcome == crate::testing::Outcome::Cancelled {
        return false;
    }
    assert!(
        report.is_success(),
        "{report:?}\n{}",
        protein_visuals::describe(app.world(), root, &report.selector.name)
    );
    true
}

fn protein_steps(
    app: &mut App,
    root: Entity,
    steps: impl IntoIterator<Item = Step<UiAction, UiCondition>>,
) -> bool {
    steps.into_iter().all(|step| protein_step(app, root, step))
}

fn protein_paint(app: &mut App, editor: Entity, phase: &str, label: &str, caption: &str) -> bool {
    let control = named(app.world_mut(), editor, label);
    match protein_visuals::verify_paint(app, control, caption) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => false,
        Err(error) => panic!(
            "{phase}: {error}\n{}",
            protein_visuals::describe(app.world(), editor, label)
        ),
    }
}

async fn wait_for_protein(
    app: &mut App,
    wake: &tokio::sync::Notify,
    condition: impl Fn(&World) -> bool,
) -> bool {
    let cancellation = crate::testing::Cancellation::current();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    for _ in 0..32 {
        if cancellation.is_cancelled() {
            return false;
        }
        app.update();
        if condition(app.world()) {
            return true;
        }
        tokio::select! {
            biased;
            () = cancellation.cancelled() => return false,
            wake = tokio::time::timeout_at(deadline, wake.notified()) => {
                wake.expect("The private Protein fixture did not publish a ready result before the deadline");
            }
        }
    }
    panic!("The private Protein fixture exceeded its readiness wake budget");
}

struct PrivateProteinFixture {
    app: App,
    root: Entity,
    area: Entity,
    records: Vec<String>,
    ready: std::sync::Arc<tokio::sync::Notify>,
}

async fn private_protein_fixture(
    seeds: &[(nucleus::RecordKind, &str)],
    configure: impl FnOnce(&mut App),
) -> PrivateProteinFixture {
    use crate::native::{
        app::CellHandle,
        cell_bridge::CellBridgePlugin,
        protein_area::{self, ProteinAreaPlugin},
        protein_castle::ProteinCastlePlugin,
        wake::WakeSignal,
    };
    use std::sync::Arc;

    let engine = Arc::new(
        engine::Engine::open_memory()
            .await
            .expect("Private Protein store"),
    );
    let mut records = Vec::new();
    for &(kind, title) in seeds {
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind,
                    head: title.into(),
                    body: "Private Laboratory data".into(),
                    quantity: 1.0,
                },
                None,
            )
            .await
            .expect("Seed the private Protein store")
            .created
            .expect("The seeded Record has a UID");
        records.push(uid);
    }
    let cell = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let ready = Arc::new(tokio::sync::Notify::new());
    let wake = Arc::clone(&ready);
    let (mut app, root) = rendered_fixture(UVec2::new(800, 720), |app| {
        app.insert_resource(CellHandle(cell))
            .insert_resource(WakeSignal::new(move || wake.notify_one()))
            .init_resource::<bevy::clipboard::Clipboard>()
            .add_plugins((
                CellBridgePlugin,
                ProteinAreaPlugin,
                ProteinCastlePlugin,
                crate::controls::editing::TextEditingPlugin,
            ));
        configure(app);
    });
    EditAction::Open.apply(app.world_mut(), root);
    let mut area = crate::native::area::InfluenceArea::new(
        crate::native::area::AreaShape::Square,
        DVec2::ZERO,
        DVec2::splat(320.0),
    );
    area.protein = Some(protein_area::Config::default());
    let area = crate::native::area::spawn_area(app.world_mut(), root, 1, area)
        .expect("The fixture has an active workspace");
    PrivateProteinFixture {
        app,
        root,
        area,
        records,
        ready,
    }
}

fn open_protein_editor(app: &mut App, root: Entity, area: Entity) -> Entity {
    use crate::native::protein_area::{ConfigureProtein, QueryEditor};

    ConfigureProtein.apply(app.world_mut(), area);
    EditAction::Close.apply(app.world_mut(), root);
    let editor = app
        .world_mut()
        .query::<(Entity, &QueryEditor)>()
        .iter(app.world())
        .find(|(_, link)| link.0 == area)
        .expect("An Area opens its Protein editor")
        .0;
    size_protein_editor(app.world_mut(), editor);
    editor
}

fn size_protein_editor(world: &mut World, editor: Entity) {
    let mut node = world
        .get_mut::<Node>(editor)
        .expect("The Protein editor is laid out");
    node.width = px(720);
    node.height = px(680);
}

#[cfg_attr(test, tokio::test)]
#[allow(
    clippy::unwrap_used,
    reason = "The mandatory Protein Area case must fail if a filter control or rendered choice is missing"
)]
async fn protein_area_filter_visible_and_selectable() {
    use crate::native::{
        protein_area,
        protein_castle::{ProteinCastle, ProteinResults},
    };
    use bevy::input::keyboard::Key;

    const MATCHING_TITLE: &str = "Visible filter fixture";
    const FILTER_VALUE: &str = "plain";
    const RUN_QUERY: &str = "Run this query and keep results live";

    let PrivateProteinFixture {
        mut app,
        root,
        area,
        records,
        ready,
    } = private_protein_fixture(
        &[
            (nucleus::RecordKind::Plain, MATCHING_TITLE),
            (nucleus::RecordKind::Rule, "Unmatched filter fixture"),
        ],
        |_| {},
    )
    .await;
    if !wait_for_protein(&mut app, &ready, |world| {
        protein_area::feed_ready(world, area)
            && protein_area::calendar_feed(world, area).is_some_and(|(rows, _)| {
                records.iter().all(|uid| {
                    rows.iter()
                        .any(|row| row["uid"].as_str() == Some(uid.as_str()))
                })
            })
    })
    .await
    {
        return;
    }
    let editor = open_protein_editor(&mut app, root, area);
    let add = Selector::new("Add a condition").with_role("Button");
    let condition = Selector::new("Condition").with_role("Button");
    let text = Selector::new("Text contains").with_role("Button");
    let kind = Selector::new("Kind").with_role("Button");
    let value = Selector::new("Record kind").with_role("TextInput");
    let tab = |target: &Selector| {
        Step::action(
            "Tab to the visible choice",
            target.clone(),
            UiAction::Key(KeyCode::Tab, Key::Tab),
            UiCondition::Focused(target.clone()),
        )
    };
    let open = || {
        UiCondition::Ready(kind.clone(), |world, choice| {
            protein_visuals::caption(world, choice, "Kind")
        })
    };
    let pending_caption = || {
        UiCondition::Ready(condition.clone(), |world, control| {
            protein_visuals::caption(world, control, "Text contains")
        })
    };
    let selected_caption = || {
        UiCondition::Ready(condition.clone(), |world, control| {
            protein_visuals::caption(world, control, "Kind")
        })
    };
    if !protein_steps(
        &mut app,
        editor,
        [
            Step::observe(
                "lay out the filter editor",
                add.clone(),
                UiCondition::Ready(add, |_, _| true),
            ),
            click_step("Add a condition", pending_caption()),
        ],
    ) || !protein_paint(&mut app, editor, "closed", "Condition", "Text contains")
    {
        return;
    }
    if !protein_steps(
        &mut app,
        editor,
        [
            click_step("Condition", open()),
            Step::observe(
                "ordinary pointer input focuses the condition",
                condition.clone(),
                UiCondition::Focused(condition.clone()),
            ),
            Step::observe(
                "shape the first open choice",
                text.clone(),
                UiCondition::Ready(text.clone(), |world, control| {
                    protein_visuals::caption(world, control, "Text contains")
                }),
            ),
        ],
    ) || !protein_paint(&mut app, editor, "expanded", "Kind", "Kind")
    {
        return;
    }
    if !protein_steps(
        &mut app,
        editor,
        [
            tab(&text),
            tab(&kind),
            Step::action(
                "Escape from the open menu",
                kind.clone(),
                UiAction::Key(KeyCode::Escape, Key::Escape),
                UiCondition::Focused(condition.clone()),
            ),
            Step::observe(
                "Escape closes without selecting",
                kind.clone(),
                UiCondition::Absent(kind.clone()),
            ),
            Step::observe(
                "Escape preserves the caption",
                condition.clone(),
                pending_caption(),
            ),
            Step::action(
                "open the current condition with Enter",
                condition.clone(),
                UiAction::Key(KeyCode::Enter, Key::Enter),
                open(),
            ),
            tab(&text),
            tab(&kind),
            Step::action(
                "select Kind with Enter",
                kind.clone(),
                UiAction::Key(KeyCode::Enter, Key::Enter),
                UiCondition::Absent(kind.clone()),
            ),
            Step::observe(
                "shape the selected condition's caption",
                condition.clone(),
                selected_caption(),
            ),
        ],
    ) || !protein_paint(&mut app, editor, "selected", "Condition", "Kind")
    {
        return;
    }
    if !protein_steps(
        &mut app,
        editor,
        [
            Step::observe(
                "lay out the condition value",
                value.clone(),
                UiCondition::Ready(value.clone(), |_, _| true),
            ),
            Step::action(
                "click the condition value",
                value.clone(),
                UiAction::Click,
                UiCondition::Focused(value.clone()),
            ),
            Step::action(
                "type the condition value",
                value.clone(),
                UiAction::Text(FILTER_VALUE.into()),
                UiCondition::Value(value, FILTER_VALUE.into()),
            ),
        ],
    ) {
        return;
    }
    let query = &app
        .world()
        .get::<ProteinCastle>(editor)
        .unwrap()
        .draft
        .query;
    assert_eq!(
        query["where"],
        serde_json::json!([{"all":[{"kind_eq":FILTER_VALUE}]}])
    );

    let previous_revision = app.world().get::<ProteinResults>(editor).unwrap().revision;
    if !protein_step(
        &mut app,
        editor,
        click_step(RUN_QUERY, UiCondition::Check(|_| true)),
    ) {
        return;
    }
    if !wait_for_protein(&mut app, &ready, |world| {
        protein_area::calendar_status(world, area) == "Live"
            && world
                .get::<ProteinResults>(editor)
                .is_some_and(|results| results.current && results.revision != previous_revision)
    })
    .await
    {
        return;
    }
    let rows = &app.world().get::<ProteinResults>(editor).unwrap().rows;
    let observed: Vec<_> = rows
        .iter()
        .map(|row| (row["uid"].as_str(), row["head"].as_str()))
        .collect();
    assert_eq!(
        observed,
        [(Some(records[0].as_str()), Some(MATCHING_TITLE))]
    );
    let run = Selector::new(RUN_QUERY).with_role("Button");
    protein_step(
        &mut app,
        editor,
        Step::observe(
            "shape the title of the returned record",
            run.clone(),
            UiCondition::Ready(run, |world, button| {
                protein_visuals::result_caption(world, button, MATCHING_TITLE)
            }),
        ),
    );
}

#[cfg(test)]
#[tokio::test]
#[ignore = "private offscreen cross-feature coverage case"]
async fn first_protein_result_frame_uses_the_theme_changed_during_held_delivery() {
    use crate::native::{
        cell_bridge::{CellBridge, CellMessage, RECORDS, ReceiveCell},
        protein_castle::ProteinResults,
        tokens::{ThemeSettings, Token, TokenValue},
    };
    use bevy::render::view::screenshot::Screenshot;
    use std::time::{Duration, Instant};

    const TITLE: &str = "First asynchronous result";
    const RUN: &str = "Run this query and keep results live";
    #[derive(Resource, Default)]
    struct FirstResultFrame(Option<(Entity, Entity)>);

    let PrivateProteinFixture {
        mut app,
        root,
        area,
        records,
        ..
    } = private_protein_fixture(&[(nucleus::RecordKind::Plain, TITLE)], |app| {
        app.configure_sets(Update, ReceiveCell.run_if(|| false));
    })
    .await;
    let editor = open_protein_editor(&mut app, root, area);
    app.update();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let cancellation = crate::testing::Cancellation::current();
    let mut held = Vec::new();
    let mut found = false;
    while held.len() < 32 && !found {
        let mut bridge = app.world_mut().non_send_mut::<CellBridge>();
        let message = tokio::select! {
            biased;
            () = cancellation.cancelled() => return,
            message = tokio::time::timeout_at(deadline, bridge.incoming.recv()) => {
                message.expect("The private Area should submit its initial query").expect("Private Cell disconnected")
            }
        };
        found = matches!(&message, cell::ServerMessage::Snapshot { id, rows }
            if id != RECORDS && rows.iter().any(|row| row["uid"] == records[0]));
        held.push(message);
    }
    assert!(
        found,
        "The first private query result did not reach the receive boundary"
    );
    assert!(
        app.world()
            .get::<ProteinResults>(editor)
            .unwrap()
            .rows
            .is_empty()
    );

    let ink = TokenValue::Color([145, 235, 250, 255]);
    assert!(
        app.world_mut()
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Ink, ink)
    );
    app.update();
    assert!(protein_visuals::result_text(app.world(), editor, TITLE).is_none());
    let run = named(app.world_mut(), editor, RUN);
    let run_icon = app
        .world()
        .get::<Children>(run)
        .unwrap()
        .iter()
        .find(|child| app.world().get::<ImageNode>(*child).is_some())
        .expect("The first-open editor shell has a Run icon");

    let target = protein_visuals::paint_target(app.world()).unwrap();
    app.init_resource::<FirstResultFrame>()
        .add_systems(Last, move |world: &mut World| {
            if world.resource::<FirstResultFrame>().0.is_some() {
                return;
            }
            let Some(text) = protein_visuals::result_text(world, editor, TITLE) else {
                return;
            };
            if !world
                .get::<ComputedNode>(text)
                .is_some_and(|node| node.size().min_element() > 0.0)
            {
                return;
            }
            let screenshot = world.spawn(Screenshot::image(target.clone())).id();
            world.resource_mut::<FirstResultFrame>().0 = Some((screenshot, text));
        });
    for message in held {
        app.world_mut().write_message(CellMessage::new(message));
    }
    for _ in 0..8 {
        if cancellation.is_cancelled() {
            return;
        }
        app.update();
        if app.world().resource::<FirstResultFrame>().0.is_some() {
            break;
        }
    }
    let (request, text) = app
        .world()
        .resource::<FirstResultFrame>()
        .0
        .expect("The held result should reach the first laid-out frame");
    let results = app.world().get::<ProteinResults>(editor).unwrap();
    assert!(results.current && results.rows.iter().any(|row| row["uid"] == records[0]));
    assert_eq!(
        app.world().get::<TextColor>(text).unwrap().0.to_linear(),
        ink.linear_color().to_linear()
    );
    assert_eq!(
        app.world()
            .get::<ImageNode>(run_icon)
            .unwrap()
            .color
            .to_linear(),
        ink.linear_color().to_linear()
    );
    let regions = [
        protein_visuals::PaintRegion::text(app.world(), text, TITLE).unwrap(),
        protein_visuals::PaintRegion::icon(app.world(), run_icon, RUN).unwrap(),
    ];
    let deadline = Instant::now() + Duration::from_secs(5);
    let frame = super::showoff::screenshot::read_capture(&mut app, request, deadline).unwrap();
    protein_visuals::verify_frame(&mut app, frame, &regions, deadline).unwrap();
}

#[cfg_attr(test, test)]
#[allow(
    clippy::unwrap_used,
    reason = "The edit panel geometry case requires known fixture bounds and visible workspace controls"
)]
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
        if scale == 1.0 {
            settle(&mut app);
            let toggle = app
                .world()
                .get::<crate::native::edit_mode::EditMode>(root)
                .unwrap()
                .toggle;
            let caption = app
                .world()
                .get::<AccessibilityNode>(toggle)
                .unwrap()
                .label()
                .unwrap()
                .to_owned();
            click(
                &mut app,
                root,
                &caption,
                UiCondition::Check(|world| {
                    world.iter_entities().any(|entity| {
                        entity
                            .get::<crate::native::edit_mode::EditMode>()
                            .is_some_and(|mode| mode.enabled)
                    })
                }),
            );
        } else {
            EditAction::Open.apply(app.world_mut(), root);
        }
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
#[allow(
    clippy::unwrap_used,
    reason = "The dropdown caption case treats missing fixture labels and layout state as failures"
)]
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
#[allow(
    clippy::unwrap_used,
    reason = "The retained layout case requires complete fixture geometry and input ordering evidence"
)]
fn retained_layout_keeps_scroll_reparenting_clips_and_input_order_current() {
    use crate::native::tokens::{Token, TokenOverrides, TokenValue};
    use bevy::text::FontSize;

    #[derive(Resource, Default)]
    struct Activated(Option<Entity>);

    let (mut app, root) = fixture(UVec2::new(1000, 700), 1.0);
    app.init_resource::<Activated>();
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
    for name in ["Scrolled control", "Overlapping control"] {
        let mut semantics = accesskit::Node::new(accesskit::Role::Button);
        semantics.set_label(name);
        controls.push(
            app.world_mut()
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(80),
                        width: px(80),
                        height: px(30),
                        ..default()
                    },
                    crate::native::sand::button(0),
                    AccessibilityNode(semantics),
                    ChildOf(body),
                ))
                .observe(
                    |event: On<bevy::ui_widgets::Activate>, mut activated: ResMut<Activated>| {
                        activated.0 = Some(event.entity);
                    },
                )
                .id(),
        );
    }
    let font = app
        .world()
        .resource::<crate::native::theme::Typography>()
        .text(16.0);
    app.world_mut()
        .entity_mut(controls[0])
        .insert((Text::new("Scroll"), font));
    settle(&mut app);
    let original = app
        .world()
        .get::<UiGlobalTransform>(body)
        .unwrap()
        .translation;
    let viewport = Rect::from_corners(Vec2::ZERO, Vec2::new(1000.0, 700.0));
    let clip_bounds = |world: &World| {
        let clip = world.get::<bevy::ui::CalculatedClip>(body).unwrap();
        crate::native::canvas_clip::clipped_rect(viewport, clip)
    };
    let original_clip = clip_bounds(app.world());
    let mut overrides = TokenOverrides::default();
    overrides.set(Token::FontSize, TokenValue::Number(24.0));
    app.world_mut()
        .entity_mut(panels[0])
        .insert((ScrollPosition(Vec2::new(0.0, 40.0)), overrides));
    app.world_mut().entity_mut(controls[0]).insert(ZIndex(1));
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
    assert_eq!(clip_bounds(app.world()), original_clip);
    assert_eq!(
        app.world().get::<TextFont>(controls[0]).unwrap().font_size,
        FontSize::Px(24.0)
    );
    let report = drive_step(
        &mut app,
        root,
        click_step(
            "Scrolled control",
            UiCondition::Check(|world| world.resource::<Activated>().0.is_some()),
        ),
    );
    if report.outcome == crate::testing::Outcome::Cancelled {
        return;
    }
    assert!(report.is_success(), "{report:?}");
    assert_eq!(app.world().resource::<Activated>().0, Some(controls[0]));
    app.world_mut().entity_mut(controls[0]).remove::<ZIndex>();
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
        clip_bounds(app.world()),
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
    async protein_area_filter_visible_and_selectable,
    shared_dropdown_captions_follow_available_width_and_theme,
    edit_panel_usable_bounds,
    sync_history_controls_draw_and_respond_while_live_status_stays_visible,
    performance_traces_preserve_concurrent_samples_and_report_overflow,
    retained_exclusive_queries_follow_new_archetypes_and_membership_changes,
    workspace_passwords_retain_idle_layout_and_preserve_masking_and_edits,
    inherited_font_changes_prepare_current_glyphs_once_in_the_same_update,
    hidden_text_defers_visual_work_and_reveals_current_edits_and_styles,
    logical_end_cursor_preserves_text_and_selection_without_eager_shaping,
    untouched_editors_preserve_change_ticks_while_edits_and_cursor_updates_remain_live,
    retained_layout_keeps_scroll_reparenting_clips_and_input_order_current,
}
