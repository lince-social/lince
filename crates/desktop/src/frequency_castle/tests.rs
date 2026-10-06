use super::*;

fn draft() -> Draft {
    let mut draft = Draft::default();
    draft.fields[0] = "weekly".into();
    draft.fields[1] = "Weekly allowance".into();
    draft.fields[3] = "2026-09-20T09:00:00-03:00".into();
    draft.fields[2] = "1 week".into();
    draft
}

fn frequency(draft: &Draft) -> Frequency {
    let definition = draft.definition().unwrap();
    let compiled = definition.compile(&Default::default()).unwrap();
    Frequency {
        uid: "frequency-1".into(),
        slug: definition.slug.to_string(),
        status: "proven".into(),
        quantity: nucleus::DecimalValue::from_mantissa(0, 0).unwrap(),
        handle_revision: 1,
        head_revision_hash: compiled.revision_hash,
        active_revision_hash: None,
        last_run_revision_hash: None,
        last_run_parameters: None,
        definition,
        next_at_ms: None,
        next_local: None,
    }
}

#[cfg_attr(test, test)]
fn simple_editor_preserves_exact_anchor_policies_and_identity() {
    let mut row = frequency(&draft());
    row.definition.missed = nucleus::karma::MissedPolicy::Skip;
    let mut edit = Draft::edit(&row);
    assert!(edit.cadence_editable);
    assert_eq!(edit.definition().unwrap(), row.definition);
    edit.fields[2] = "2 weeks".into();
    let changed = edit.definition().unwrap();
    assert_eq!(changed.missed, row.definition.missed);
    assert_eq!(changed.slug, row.definition.slug);
    edit.fields[0] = "different".into();
    assert!(edit.definition().unwrap_err().contains("linked Karma"));
}

#[cfg_attr(test, test)]
fn calendar_definitions_round_trip_without_flattening() {
    let mut draft = draft();
    draft.fields[2] = "1 month".into();
    let row = frequency(&draft);
    let edit = Draft::edit(&row);
    assert!(edit.cadence_editable);
    assert_eq!(edit.definition().unwrap(), row.definition);
    assert_eq!(draft.definition().unwrap(), row.definition);
    let restored: Draft = serde_json::from_str(&serde_json::to_string(&draft).unwrap()).unwrap();
    assert!(restored.valid());
    assert_eq!(restored, draft);
    let mut custom = row;
    if let nucleus::karma::FrequencyCadenceAst::Calendar {
        timezone,
        gap,
        fold,
        ..
    } = &mut custom.definition.cadence
    {
        *timezone = nucleus::karma::TimeZoneId::new("America/Sao_Paulo").unwrap();
        *gap = nucleus::karma::GapPolicy::ShiftForward;
        *fold = nucleus::karma::FoldPolicy::Second;
    }
    assert_eq!(
        Draft::edit(&custom).definition().unwrap(),
        custom.definition
    );
}

#[cfg_attr(test, test)]
fn malformed_zero_fractional_and_oversized_inputs_do_not_create_schedules() {
    for value in [
        "0 days",
        "-1 day",
        "1.5 seconds",
        "4294967296ms",
        "text",
        "1 day +",
        "1 day + 0s",
        "4294967295ms + 1ms",
        "1 day + 2",
        "1 day + 🐈",
    ] {
        let mut draft = draft();
        draft.fields[2] = value.into();
        assert!(draft.definition().is_err());
    }
    let mut draft = draft();
    draft.fields[3] = "2026-09-20T09:00:00".into();
    assert!(draft.definition().is_err());
    draft.fields[3] = "x".repeat(32_769);
    assert!(!draft.valid());
    assert!(draft.definition().is_err());
}

#[cfg_attr(test, test)]
fn combined_intervals_round_trip_and_keep_subsecond_precision() {
    for (text, expected) in [
        (
            "1 month + 1 day",
            nucleus::karma::CadenceStep {
                months: 1,
                days: 1,
                ..default()
            },
        ),
        (
            "1 second + 100ms",
            nucleus::karma::CadenceStep {
                seconds: 1,
                milliseconds: 100,
                ..default()
            },
        ),
        (
            "1 day + 10s + 100ms",
            nucleus::karma::CadenceStep {
                days: 1,
                seconds: 10,
                milliseconds: 100,
                ..default()
            },
        ),
    ] {
        assert_eq!(super::interval::parse(text).unwrap(), expected);
        let mut draft = draft();
        draft.fields[2] = text.into();
        let row = frequency(&draft);
        let edit = Draft::edit(&row);
        assert!(edit.cadence_editable);
        assert_eq!(edit.definition().unwrap(), row.definition);
        assert_eq!(super::interval::parse(&edit.fields[2]).unwrap(), expected);
        let restored: Draft = serde_json::from_str(&serde_json::to_string(&edit).unwrap()).unwrap();
        assert_eq!(restored, edit);
    }
}

#[cfg_attr(test, test)]
fn weekdays_apply_after_all_time_components_and_survive_edits() {
    let mut draft = draft();
    draft.fields[2] = "1 month + 1 day + 10s + 100ms".into();
    draft.fields[3] = "2026-01-01T23:59:55Z".into();
    draft.weekdays = vec![nucleus::karma::CivilWeekday::Monday];
    let row = frequency(&draft);
    let compiled = row.definition.compile(&Default::default()).unwrap();
    let nucleus::karma::CompiledSchedule::Calendar { schedule } = compiled.schedule else {
        panic!("expected calendar");
    };
    let anchor = chrono::DateTime::parse_from_rfc3339(&draft.fields[3])
        .unwrap()
        .to_utc();
    let until = chrono::DateTime::parse_from_rfc3339("2026-02-10T00:00:00Z")
        .unwrap()
        .to_utc();
    let dates = schedule
        .cadence
        .between(anchor, anchor, until)
        .unwrap()
        .dates;
    assert_eq!(
        dates[1].to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "2026-02-09T00:00:05.100Z"
    );
    let mut edit = Draft::edit(&row);
    assert_eq!(edit.weekdays, draft.weekdays);
    edit.fields[1] = "New name".into();
    assert_eq!(edit.definition().unwrap().cadence, row.definition.cadence);
    edit.weekdays.clear();
    assert_ne!(edit.definition().unwrap().cadence, row.definition.cadence);
    assert!(model::schedule(&row).contains("then Mon"));
}

#[cfg_attr(test, test)]
fn countdown_uses_scheduler_time_and_stops_at_due_now() {
    let mut row = frequency(&draft());
    let before = row.clone();
    row.next_at_ms = Some(10_001);
    assert!(before.same_layout(&row));
    assert!(model::next(&row, 9_000).contains("02s"));
    assert!(model::next(&row, 9_001).contains("01s"));
    assert!(model::next(&row, 10_001).contains("Due now"));
    row.next_at_ms = None;
    assert_eq!(model::next(&row, 0), "No scheduled beat");
}

#[cfg_attr(test, test)]
fn editor_shows_the_next_date_without_moving_an_unchanged_schedule() {
    let mut row = frequency(&draft());
    row.next_at_ms = Some(1_790_251_200_000);
    let mut edit = Draft::edit(&row);
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&edit.fields[3])
            .unwrap()
            .timestamp_millis(),
        row.next_at_ms.unwrap()
    );
    assert_eq!(edit.definition().unwrap(), row.definition);
    edit.fields[1] = "Renamed".into();
    assert_eq!(edit.definition().unwrap().cadence, row.definition.cadence);
    edit.fields[3] = "2026-09-21T09:55:00-03:00".into();
    assert_ne!(edit.definition().unwrap().cadence, row.definition.cadence);
}

#[cfg_attr(test, test)]
fn header_is_compact_and_live_dates_do_not_overwrite_user_edits() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        FrequencyCastle::default(),
    );
    let header = app.world().get::<Children>(owner).unwrap()[0];
    assert_eq!(app.world().get::<Children>(header).unwrap().len(), 3);
    let status = app.world().get::<View>(owner).unwrap().status;
    assert_eq!(
        app.world().get::<Node>(status).unwrap().display,
        Display::None
    );
    let mut row = frequency(&draft());
    row.next_at_ms = Some(1_790_251_200_000);
    app.world_mut().get_mut::<View>(owner).unwrap().rows = vec![row.clone()];
    ui::Command::Edit(row.uid.clone()).apply(app.world_mut(), owner);
    row.next_at_ms = Some(row.next_at_ms.unwrap() + 86_400_000);
    app.world_mut().get_mut::<View>(owner).unwrap().rows = vec![row.clone()];
    ui::refresh_next_date(app.world_mut(), owner);
    let changed = app
        .world()
        .get::<FrequencyCastle>(owner)
        .unwrap()
        .draft
        .as_ref()
        .unwrap();
    assert_eq!(changed.fields[3], Draft::edit(&row).fields[3]);
    assert_eq!(changed.definition().unwrap(), row.definition);
    app.world_mut()
        .get_mut::<FrequencyCastle>(owner)
        .unwrap()
        .draft
        .as_mut()
        .unwrap()
        .fields[3] = "2026-09-21T09:55:00-03:00".into();
    ui::render_form(app.world_mut(), owner);
    row.next_at_ms = Some(row.next_at_ms.unwrap() + 86_400_000);
    app.world_mut().get_mut::<View>(owner).unwrap().rows = vec![row];
    ui::refresh_next_date(app.world_mut(), owner);
    assert_eq!(
        app.world()
            .get::<FrequencyCastle>(owner)
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .fields[3],
        "2026-09-21T09:55:00-03:00"
    );
    let labels: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.as_str())
        .collect();
    for removed in [
        "Advanced definition",
        "Apply saved defaults",
        "Resume last run",
        "×",
    ] {
        assert!(!labels.contains(&removed));
    }
}

#[cfg_attr(test, test)]
fn rows_keep_indicator_name_slug_and_icons_on_one_line() {
    let mut app = app();
    let world = app.world_mut();
    let root = world.spawn(crate::workspace::Workspaces::default()).id();
    let owner = spawn(world, root, 1, DVec2::ZERO, FrequencyCastle::default());
    let header = world.get::<Children>(owner).unwrap()[0];
    let controls = world.get::<Children>(header).unwrap()[1];
    let add = world.get::<Children>(controls).unwrap()[0];
    let add_node = world.get::<Node>(add).unwrap();
    assert_eq!(add_node.height, px(22));
    assert_eq!(add_node.justify_content, JustifyContent::Center);
    let search = world.get::<Children>(header).unwrap()[2];
    assert_eq!(world.get::<Children>(search).unwrap().len(), 2);
    let mut row = frequency(&draft());
    for quantity in [0, 1] {
        let previous = row.clone();
        row.quantity = nucleus::DecimalValue::from_mantissa(0, quantity).unwrap();
        if quantity == 1 {
            assert!(!previous.same_layout(&row));
        }
        world.get_mut::<View>(owner).unwrap().rows = vec![row.clone()];
        ui::render_list(world, owner);
        let list = world.get::<View>(owner).unwrap().list;
        let block = world.get::<Children>(list).unwrap()[0];
        let lines = world.get::<Children>(block).unwrap();
        assert_eq!(lines.len(), 2);
        let heading = lines[0];
        let details = lines[1];
        assert_eq!(
            world.get::<Node>(heading).unwrap().flex_wrap,
            FlexWrap::NoWrap
        );
        let children = world.get::<Children>(heading).unwrap();
        assert_eq!(children.len(), 5);
        let indicator = children[0];
        assert_eq!(world.get::<Node>(indicator).unwrap().height, px(17));
        assert_eq!(
            !world
                .get::<BackgroundColor>(indicator)
                .unwrap()
                .0
                .is_fully_transparent(),
            quantity != 0
        );
        assert_eq!(
            world.get::<Text>(children[1]).unwrap().0,
            "Weekly allowance"
        );
        assert_eq!(world.get::<Text>(children[2]).unwrap().0, "@weekly");
        for icon in [children[3], children[4]] {
            assert_eq!(world.get::<Node>(icon).unwrap().height, px(17));
            assert!(world.get::<crate::actions::ActionButton>(icon).is_some());
        }
        assert!(
            world
                .get::<Text>(details)
                .unwrap()
                .0
                .starts_with("Every 1 week | Next: ")
        );
        assert!(!world.get::<Text>(details).unwrap().0.contains("Saved:"));
    }
}

#[cfg_attr(test, test)]
fn large_lists_only_render_one_page_and_filter_without_rebuilding_the_draft() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        FrequencyCastle::default(),
    );
    let row = frequency(&draft());
    app.world_mut().get_mut::<View>(owner).unwrap().rows = (0..1000)
        .map(|index| {
            let mut row = row.clone();
            row.uid = format!("frequency-{index}");
            row.slug = format!("frequency-{index}");
            row
        })
        .collect();
    ui::render_list(app.world_mut(), owner);
    let list = app.world().get::<View>(owner).unwrap().list;
    assert_eq!(app.world().get::<Children>(list).unwrap().len(), 21);
    app.world_mut()
        .get_mut::<FrequencyCastle>(owner)
        .unwrap()
        .search = "frequency-999".into();
    ui::render_list(app.world_mut(), owner);
    assert_eq!(app.world().get::<Children>(list).unwrap().len(), 1);
}

fn app() -> App {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(FrequencyCastlePlugin);
    app
}

#[cfg_attr(test, test)]
fn drafts_stay_first_survive_live_updates_and_persist() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        FrequencyCastle {
            draft: Some(draft()),
            search: String::new(),
        },
    );
    let form = app.world().get::<View>(owner).unwrap().form;
    let editor = app.world().get::<Children>(form).unwrap()[2];
    app.world_mut()
        .get_mut::<View>(owner)
        .unwrap()
        .rows
        .push(frequency(&draft()));
    ui::render_list(app.world_mut(), owner);
    assert!(app.world().get_entity(editor).is_ok());
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].valid());
    let encoded = serde_json::to_string(&saved).unwrap();
    let restored: Vec<SavedFrequencyCastle> = serde_json::from_str(&encoded).unwrap();
    assert!(restored[0].valid());
    let before = app
        .world()
        .get::<FrequencyCastle>(owner)
        .unwrap()
        .draft
        .clone();
    ui::Command::New.apply(app.world_mut(), owner);
    assert_eq!(
        app.world().get::<FrequencyCastle>(owner).unwrap().draft,
        before
    );
    let view = app.world().get::<View>(owner).unwrap();
    let parent = app.world().get::<ChildOf>(view.form).unwrap().parent();
    let children = app.world().get::<Children>(parent).unwrap();
    assert!(
        children.iter().position(|entity| entity == view.form)
            < children.iter().position(|entity| entity == view.list)
    );
}

#[cfg_attr(test, tokio::test(flavor = "multi_thread", worker_threads = 2))]
async fn native_castle_creates_edits_reads_and_confirms_deletion_through_cell() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                "frequency-ui-test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let mut app = app();
    app.insert_resource(crate::app::CellHandle(runtime))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        FrequencyCastle::default(),
    );
    until(&mut app, |world| world.get::<View>(owner).unwrap().ready).await;
    ui::Command::New.apply(app.world_mut(), owner);
    let mut combined = draft();
    combined.fields[2] = "1 month + 1 day + 10s + 100ms".into();
    combined.weekdays = vec![nucleus::karma::CivilWeekday::Monday];
    let expected = combined.definition().unwrap();
    app.world_mut()
        .get_mut::<FrequencyCastle>(owner)
        .unwrap()
        .draft = Some(combined);
    ui::render_form(app.world_mut(), owner);
    ui::Command::Save.apply(app.world_mut(), owner);
    until(&mut app, |world| {
        world.get::<View>(owner).unwrap().rows.len() == 1
            && world.get::<FrequencyCastle>(owner).unwrap().draft.is_none()
    })
    .await;
    assert_eq!(
        app.world().get::<View>(owner).unwrap().rows[0].definition,
        expected
    );
    let uid = app.world().get::<View>(owner).unwrap().rows[0].uid.clone();
    ui::Command::Edit(uid.clone()).apply(app.world_mut(), owner);
    app.world_mut()
        .get_mut::<FrequencyCastle>(owner)
        .unwrap()
        .draft
        .as_mut()
        .unwrap()
        .fields[1] = "Renamed purpose".into();
    ui::render_form(app.world_mut(), owner);
    ui::Command::Save.apply(app.world_mut(), owner);
    until(&mut app, |world| {
        world.get::<View>(owner).unwrap().rows[0].definition.purpose == "Renamed purpose"
            && world.get::<FrequencyCastle>(owner).unwrap().draft.is_none()
    })
    .await;
    assert_eq!(app.world().get::<View>(owner).unwrap().rows[0].uid, uid);
    ui::Command::ConfirmDelete(uid.clone()).apply(app.world_mut(), owner);
    assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
    ui::Command::Delete(uid.clone()).apply(app.world_mut(), owner);
    assert_eq!(
        app.world().get::<View>(owner).unwrap().deleting,
        Some(uid.clone())
    );
    assert_eq!(app.world().get::<View>(owner).unwrap().rows.len(), 1);
    ui::Command::ConfirmDelete(uid).apply(app.world_mut(), owner);
    until(&mut app, |world| {
        world.get::<View>(owner).unwrap().rows.is_empty()
            && world.get::<View>(owner).unwrap().pending.is_none()
    })
    .await;
}

async fn until(app: &mut App, predicate: impl Fn(&World) -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            app.update();
            if predicate(app.world()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[cfg_attr(test, test)]
fn creation_controls_replace_the_plus_only_in_the_header() {
    let mut app = app();
    let world = app.world_mut();
    let root = world.spawn(crate::workspace::Workspaces::default()).id();
    let owner = spawn(world, root, 1, DVec2::ZERO, FrequencyCastle::default());
    let controls = world.get::<View>(owner).unwrap().controls;
    assert_eq!(world.get::<Children>(controls).unwrap().len(), 1);
    ui::Command::New.apply(world, owner);
    let buttons = world.get::<Children>(controls).unwrap();
    assert_eq!(buttons.len(), 2);
    let create = world.get::<Children>(buttons[0]).unwrap()[0];
    assert_eq!(world.get::<Text>(create).unwrap().0, "Create");
    let form = world.get::<View>(owner).unwrap().form;
    assert!(
        world
            .get::<Children>(form)
            .unwrap()
            .iter()
            .all(|child| world.get::<crate::actions::ActionButton>(child).is_none())
    );
    ui::Command::Cancel.apply(world, owner);
    assert!(world.get::<FrequencyCastle>(owner).unwrap().draft.is_none());
    assert_eq!(world.get::<Children>(controls).unwrap().len(), 1);
}

crate::laboratory_cases! {
    async native_castle_creates_edits_reads_and_confirms_deletion_through_cell,
    simple_editor_preserves_exact_anchor_policies_and_identity,
    calendar_definitions_round_trip_without_flattening,
    malformed_zero_fractional_and_oversized_inputs_do_not_create_schedules,
    combined_intervals_round_trip_and_keep_subsecond_precision,
    weekdays_apply_after_all_time_components_and_survive_edits,
    countdown_uses_scheduler_time_and_stops_at_due_now,
    editor_shows_the_next_date_without_moving_an_unchanged_schedule,
    header_is_compact_and_live_dates_do_not_overwrite_user_edits,
    rows_keep_indicator_name_slug_and_icons_on_one_line,
    large_lists_only_render_one_page_and_filter_without_rebuilding_the_draft,
    drafts_stay_first_survive_live_updates_and_persist,
    creation_controls_replace_the_plus_only_in_the_header,
}
