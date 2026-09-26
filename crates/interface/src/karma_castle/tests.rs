use super::*;
use model::{FieldDraft, SharedField, Suggestion};
use nucleus::karma::rule_field::RuleFieldKind;
use serde_json::json;

fn field() -> SharedField {
    SharedField {
        uid: "shared-condition".into(),
        kind: RuleFieldKind::Condition,
        source: "@balance * freq(@weekly)".into(),
        revision: 2,
    }
}

#[test]
fn header_filter_hides_unmatched_rules_without_changing_the_draft() {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(KarmaCastlePlugin);
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        KarmaCastle::default(),
    );
    let header = app.world().get::<Children>(owner).unwrap()[0];
    assert_eq!(app.world().get::<Children>(header).unwrap().len(), 3);
    let search = app.world().get::<Children>(header).unwrap()[2];
    let status = app.world().get::<View>(owner).unwrap().status;
    assert_eq!(
        app.world().get::<Node>(status).unwrap().display,
        Display::None
    );
    app.world_mut().get_mut::<View>(owner).unwrap().rules = vec![Rule {
        uid: "rule".into(),
        fields: vec![field()],
        revision: 1,
        state: "active".into(),
    }];
    ui::Command::New.apply(app.world_mut(), owner);
    let form = app.world().get::<View>(owner).unwrap().form;
    let children: Vec<_> = app.world().get::<Children>(form).unwrap().iter().collect();
    let list = app.world().get::<View>(owner).unwrap().list;
    for (query, count) in [("missing", 0), ("@balance", 1), ("WEEKLY", 1)] {
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(search)
            .unwrap()
            .editor
            .set_text(query);
        ui::inputs(app.world_mut());
        assert_eq!(
            app.world()
                .get::<Children>(list)
                .map_or(0, |children| children.len()),
            count
        );
        assert_eq!(
            app.world()
                .get::<Children>(form)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            children
        );
    }
}

#[test]
fn typing_filters_shared_conditions_and_basic_records_without_copying_identity() {
    let rules = vec![Rule {
        uid: "rule".into(),
        fields: vec![field()],
        revision: 1,
        state: "active".into(),
    }];
    let choices = model::suggestions(
        RuleFieldKind::Condition,
        "@balance",
        &rules,
        &[json!({"slug":"balance"}), json!({"slug":"different"})],
        &[],
    );
    assert!(choices.iter().any(
        |choice| matches!(choice, Suggestion::Shared(value) if value.uid == "shared-condition")
    ));
    assert!(
        choices
            .iter()
            .any(|choice| matches!(choice, Suggestion::Element(value) if value == "@balance"))
    );
    assert!(
        !choices
            .iter()
            .any(|choice| matches!(choice, Suggestion::Element(value) if value == "@different"))
    );
    let linked = FieldDraft {
        text: field().source.clone(),
        linked: Some(field()),
    };
    assert!(matches!(
        linked.input(),
        nucleus::karma::rule_field::RuleFieldInput::Reference { revision: 2, .. }
    ));
    let copied = FieldDraft {
        text: linked.text.clone(),
        linked: None,
    };
    assert!(matches!(
        copied.input(),
        nucleus::karma::rule_field::RuleFieldInput::Text { .. }
    ));
}

#[test]
fn tokens_and_countdowns_keep_precise_dates_and_update_each_second() {
    let parts = model::fragments("-1 * freq(@weekly) * @balance = @target");
    let names: Vec<_> = parts
        .iter()
        .filter_map(|(_, name)| name.as_deref())
        .collect();
    assert_eq!(names, ["weekly", "balance", "target"]);
    assert_eq!(
        parts
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<String>(),
        "-1 * freq(@weekly) * @balance = @target"
    );
    let frequency = json!({"next_at_ms":10000,"status":"active"});
    assert!(model::frequency_hint(&frequency, 8000).contains("02s"));
    assert!(model::frequency_hint(&frequency, 9000).contains("01s"));
    assert!(model::frequency_hint(&frequency, 10000).contains("Due now"));
    assert!(model::frequency_hint(&json!({"status":"paused"}), 0).contains("no scheduled beat"));
}

#[test]
fn new_form_is_first_and_unlinking_clears_only_the_chosen_field() {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(KarmaCastlePlugin);
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        KarmaCastle::default(),
    );
    ui::Command::New.apply(app.world_mut(), owner);
    ui::Command::Link(0, field()).apply(app.world_mut(), owner);
    assert!(
        app.world()
            .get::<KarmaCastle>(owner)
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .fields[0]
            .linked
            .is_some()
    );
    ui::Command::Unlink(0).apply(app.world_mut(), owner);
    let draft = app
        .world()
        .get::<KarmaCastle>(owner)
        .unwrap()
        .draft
        .as_ref()
        .unwrap();
    assert!(draft.fields[0].linked.is_none());
    assert!(draft.fields[0].text.is_empty());
    let view = app.world().get::<View>(owner).unwrap();
    let parent = app.world().get::<ChildOf>(view.form).unwrap().parent();
    let children = app.world().get::<Children>(parent).unwrap();
    assert!(
        children.iter().position(|entity| entity == view.form)
            < children.iter().position(|entity| entity == view.list)
    );
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].valid());
}

#[test]
fn large_libraries_bound_suggestions_and_preserve_expression_prefixes() {
    let records: Vec<_> = (0..10_000)
        .map(|index| json!({"slug":format!("record-{index}")}))
        .collect();
    let choices = model::suggestions(RuleFieldKind::Condition, "@record-", &[], &records, &[]);
    assert_eq!(choices.len(), 16);
    assert_eq!(
        model::insert_element("-1 * @bal", "@balance", RuleFieldKind::Condition),
        "-1 * @balance"
    );
    assert_eq!(
        model::insert_element("@bal", "@balance", RuleFieldKind::Consequence),
        "@balance"
    );
}

#[test]
fn consequence_suggestions_offer_complete_record_destinations_without_assignment_syntax() {
    let choices = model::suggestions(
        RuleFieldKind::Consequence,
        "@esc",
        &[],
        &[json!({"slug":"escovar-dentes"}), json!({"slug":"other"})],
        &[],
    );
    assert_eq!(choices.len(), 1);
    let Suggestion::Element(destination) = &choices[0] else {
        panic!("expected a Record destination")
    };
    assert_eq!(destination, "@escovar-dentes");
    let parsed = nucleus::karma::rule_field::RuleConsequence::parse(destination).unwrap();
    assert!(matches!(
        parsed.consequences.as_slice(),
        [nucleus::karma::Consequence::SetQuantity { value: None }]
    ));
    let all = model::suggestions(RuleFieldKind::Consequence, "", &[], &[], &[]);
    assert!(all.iter().all(|choice| !matches!(choice, Suggestion::Element(value) if value.contains("result") || value.contains('='))));
}

#[test]
fn completing_a_frequency_inside_its_call_does_not_nest_it() {
    assert_eq!(
        model::insert_at(
            "2 * freq(@wee",
            13..13,
            "freq(@weekly)",
            RuleFieldKind::Condition
        ),
        "2 * freq(@weekly)"
    );
    assert_eq!(
        model::insert_at("@bal * 2", 4..4, "@balance", RuleFieldKind::Condition),
        "@balance * 2"
    );
    assert_eq!(
        model::reading_at("sum(@balance, 30d)", 4, 12),
        "sum(@balance, 30d)"
    );
}

#[tokio::test]
async fn castle_saves_through_the_cell_and_receives_live_protein_fields() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    for slug in ["source", "target"] {
        engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: Some(slug.into()),
                    kind: nucleus::RecordKind::Plain,
                    head: slug.into(),
                    body: String::new(),
                    quantity: 3.0,
                },
                None,
            )
            .await
            .unwrap();
    }
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        information: None,
    };
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(KarmaCastlePlugin)
        .insert_resource(crate::app::CellHandle(runtime))
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
        KarmaCastle::default(),
    );
    until(&mut app, |world| world.get::<View>(owner).unwrap().ready).await;
    ui::Command::New.apply(app.world_mut(), owner);
    app.world_mut()
        .get_mut::<KarmaCastle>(owner)
        .unwrap()
        .draft
        .as_mut()
        .unwrap()
        .fields = ["@source * 2", ">0", "@target"].map(|text| FieldDraft {
        text: text.into(),
        linked: None,
    });
    ui::render_form(app.world_mut(), owner);
    ui::Command::Save.apply(app.world_mut(), owner);
    until(&mut app, |world| {
        world.get::<View>(owner).unwrap().rules.len() == 1
            && world.get::<KarmaCastle>(owner).unwrap().draft.is_none()
    })
    .await;
    let view = app.world().get::<View>(owner).unwrap();
    assert_eq!(view.rules[0].fields.len(), 3);
    assert!(view.record_lookup.contains_key("source"));
    let condition = view.rules[0]
        .fields
        .iter()
        .find(|field| field.kind == RuleFieldKind::Condition)
        .unwrap()
        .clone();
    ui::Command::New.apply(app.world_mut(), owner);
    ui::Command::Link(0, condition.clone()).apply(app.world_mut(), owner);
    ui::Command::Edit(condition).apply(app.world_mut(), owner);
    assert!(
        app.world()
            .get::<KarmaCastle>(owner)
            .unwrap()
            .suspended
            .is_some()
    );
    ui::Command::Cancel.apply(app.world_mut(), owner);
    assert!(
        app.world()
            .get::<KarmaCastle>(owner)
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .fields[0]
            .linked
            .is_some()
    );
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
