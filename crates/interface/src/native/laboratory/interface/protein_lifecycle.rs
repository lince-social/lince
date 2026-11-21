// non-lean-yet
// Scope: const TITLE, const UPDATED, const RUN, fn results_are.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 33, 47.

use super::*;
use crate::native::{
    app::CellHandle,
    area::{InfluenceArea, RecordProperties},
    area_panel::{AreaAction, AreaEditor},
    deletion::Decision,
    protein_area::{self, ProteinAction as AreaProtein, RecordBinding, Source},
    protein_castle::{ProteinAction as CastleProtein, ProteinCastle, ProteinResults},
};
use nucleus::RecordKind;
use serde_json::json;

const TITLE: &str = "Lifecycle cedar";
const UPDATED: &str = "Lifecycle cedar updated";
const RUN: &str = "Run this query and keep results live";

fn results_are(world: &World, editor: Entity, uid: &str, title: &str) -> bool {
    world.get::<ProteinResults>(editor).is_some_and(|result| {
        result.current
            && result.rows.len() == 1
            && result.rows[0]["uid"].as_str() == Some(uid)
            && result.rows[0]["head"].as_str() == Some(title)
    })
}

fn result_is_readable(app: &mut App, editor: Entity, check: fn(&World, Entity) -> bool) -> bool {
    let run = Selector::new(RUN).with_role("Button");
    protein_step(
        app,
        editor,
        Step::observe(
            "show readable live result text",
            run.clone(),
            UiCondition::Ready(run, check),
        ),
    )
}

fn spawns(world: &mut World, area: Entity) -> Vec<(Entity, String)> {
    world
        .query::<(Entity, &RecordBinding, &RecordProperties)>()
        .iter(world)
        .filter(|(_, binding, _)| binding.area == area)
        .map(|(entity, binding, data)| {
            assert_eq!(binding.source, Source::Local);
            assert_eq!(data.0["uid"].as_str(), Some(binding.uid.as_str()));
            (entity, binding.uid.clone())
        })
        .collect()
}

fn preview_contains(world: &World, panel: Entity, caption: &str) -> bool {
    let Some(children) = world.get::<Children>(panel) else {
        return false;
    };
    let mut children = children.iter();
    if !children.any(|child| {
        world
            .get::<Text>(child)
            .is_some_and(|text| text.0 == "Preview")
    }) {
        return false;
    }
    children
        .next()
        .is_some_and(|preview| protein_visuals::result_text(world, preview, caption).is_some())
}

#[tokio::test]
#[ignore = "expanded private Protein lifecycle with rendered layout"]
async fn protein_preview_spawn_update_and_remove_preserve_the_private_record() {
    let PrivateProteinFixture {
        mut app,
        root,
        area,
        records,
        ready,
    } = private_protein_fixture(
        &[
            (RecordKind::Plain, TITLE),
            (RecordKind::Plain, "Different local Record"),
            (RecordKind::Rule, "Lifecycle cedar other kind"),
        ],
        |_| {},
    )
    .await;
    let uid = &records[0];
    let initial_caption =
        |world: &World, button: Entity| protein_visuals::result_caption(world, button, TITLE);
    let engine = app.world().resource::<CellHandle>().0.engine.clone();
    protein_area::set_configuration(app.world_mut(), area, None);
    EditAction::Close.apply(app.world_mut(), root);

    CastleProtein::Create.apply(app.world_mut(), root);
    let preview = app
        .world_mut()
        .query_filtered::<Entity, With<ProteinCastle>>()
        .single(app.world())
        .expect("Create opens one Protein Castle");
    size_protein_editor(app.world_mut(), preview);
    CastleProtein::Append("/where/0/all".into(), json!({"kind_eq":"plain"}))
        .apply(app.world_mut(), preview);
    CastleProtein::Append(
        "/where/0/all".into(),
        json!({"text_contains":"Lifecycle cedar"}),
    )
    .apply(app.world_mut(), preview);
    CastleProtein::Set("/fields".into(), json!(["uid", "head", "quantity"]))
        .apply(app.world_mut(), preview);
    let draft = app
        .world()
        .get::<ProteinCastle>(preview)
        .unwrap()
        .draft
        .clone();
    assert_eq!(draft.query["fields"], json!(["uid", "head", "quantity"]));
    if !protein_step(
        &mut app,
        preview,
        click_step(RUN, UiCondition::Check(|_| true)),
    ) || !wait_for_protein(&mut app, &ready, |world| {
        results_are(world, preview, uid, TITLE)
    })
    .await
        || !result_is_readable(&mut app, preview, initial_caption)
    {
        return;
    }
    assert!(spawns(app.world_mut(), area).is_empty());
    CastleProtein::Delete.apply(app.world_mut(), preview);
    Decision(true).apply(app.world_mut(), root);
    assert!(app.world().get_entity(preview).is_err());

    EditAction::Open.apply(app.world_mut(), root);
    EditAction::Areas.apply(app.world_mut(), root);
    EditAction::Area(AreaAction::Select(area)).apply(app.world_mut(), root);
    assert_eq!(
        app.world().get::<AreaEditor>(root).unwrap().selected,
        Some(area)
    );
    AreaProtein::Enable.apply(app.world_mut(), area);
    let mut config = protein_area::configuration(app.world(), area).unwrap();
    config.draft = draft;
    config.enabled = false;
    protein_area::set_configuration(app.world_mut(), area, Some(config));
    AreaProtein::RemoveField(1).apply(app.world_mut(), area);
    AreaProtein::Add("quantity".into()).apply(app.world_mut(), area);
    let template = protein_area::configuration(app.world(), area).unwrap();
    let fields: Vec<_> = template
        .bindings
        .iter()
        .map(|field| field.property.as_str())
        .collect();
    assert_eq!(fields, ["head", "quantity"]);
    let template_panel = app.world_mut().spawn(Node::default()).id();
    protein_area::controls(app.world_mut(), root, template_panel, area);
    let quantity = protein::record_schema::FIELDS
        .iter()
        .find(|field| field.key == "quantity")
        .unwrap();
    let head = protein::record_schema::FIELDS
        .iter()
        .find(|field| field.key == "head")
        .unwrap();
    assert!(preview_contains(app.world(), template_panel, head.title));
    assert!(preview_contains(
        app.world(),
        template_panel,
        quantity.title
    ));
    app.world_mut().despawn(template_panel);
    assert!(spawns(app.world_mut(), area).is_empty());

    let editor = open_protein_editor(&mut app, root, area);
    assert_eq!(
        app.world()
            .get::<ProteinCastle>(editor)
            .unwrap()
            .draft
            .query["where"],
        template.draft.query["where"]
    );
    if !protein_step(
        &mut app,
        editor,
        click_step(RUN, UiCondition::Check(|_| true)),
    ) || !wait_for_protein(&mut app, &ready, |world| {
        results_are(world, editor, uid, TITLE)
            && protein_area::projection_stats(world, area)
                .is_some_and(|stats| stats.ready && stats.materialized == 1)
    })
    .await
        || !result_is_readable(&mut app, editor, initial_caption)
    {
        return;
    }
    let rows = spawns(app.world_mut(), area);
    let [(row, bound)] = rows.as_slice() else {
        panic!("Expected one approved Record presentation");
    };
    assert_eq!(bound, uid);
    let row = *row;
    engine
        .act(
            engine::actions::Action::EditRecordText {
                target: uid.clone(),
                head: Some(UPDATED.into()),
                body: None,
            },
            None,
        )
        .await
        .unwrap();
    if !wait_for_protein(&mut app, &ready, |world| {
        results_are(world, editor, uid, UPDATED)
            && world
                .get::<RecordProperties>(row)
                .is_some_and(|data| data.0["head"] == UPDATED)
    })
    .await
        || !result_is_readable(&mut app, editor, |world, button| {
            protein_visuals::result_caption(world, button, UPDATED)
        })
    {
        return;
    }
    assert_eq!(spawns(app.world_mut(), area), [(row, uid.clone())]);

    CastleProtein::Delete.apply(app.world_mut(), editor);
    Decision(true).apply(app.world_mut(), root);
    assert!(app.world().get_entity(editor).is_err());
    EditAction::Open.apply(app.world_mut(), root);
    EditAction::Areas.apply(app.world_mut(), root);
    AreaProtein::Remove.apply(app.world_mut(), area);
    app.update();
    assert!(
        app.world()
            .get::<InfluenceArea>(area)
            .unwrap()
            .protein
            .is_none()
    );
    assert!(protein_area::projection_stats(app.world(), area).is_none());
    assert!(spawns(app.world_mut(), area).is_empty());
    assert!(app.world().get_entity(row).is_err());
    assert_eq!(engine.doc_text(uid).await.unwrap().0, UPDATED);
    assert_eq!(
        engine.doc_text(&records[1]).await.unwrap().0,
        "Different local Record"
    );
    assert_eq!(
        engine.doc_text(&records[2]).await.unwrap().0,
        "Lifecycle cedar other kind"
    );
}
