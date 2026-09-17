use super::*;

#[cfg_attr(test, test)]
fn rich_text_preserves_code_and_resolves_record_link_targets() {
    let blocks = markup::parse(
        "## Heading\n\nText **bold** and _italic_, [[Record title|r_example]].\n\n```mermaid\ngraph LR\n A --> B\n```\n\n`[[not a link|uid]]`",
    );
    assert_eq!(blocks[0].heading, 2);
    assert!(
        blocks[1]
            .runs
            .iter()
            .any(|run| run.bold && run.text == "bold")
    );
    assert!(
        blocks[1]
            .runs
            .iter()
            .any(|run| run.italic && run.text == "italic")
    );
    assert!(
        blocks[1]
            .runs
            .iter()
            .any(|run| run.text == "Record title" && run.link.as_deref() == Some("r_example"))
    );
    assert_eq!(blocks[2].code.as_deref(), Some("mermaid"));
    assert!(blocks[2].runs[0].text.contains("A --> B"));
    assert!(blocks[3].runs.iter().all(|run| run.link.is_none()));
    let blocks = markup::parse("See @philosophy, or name@example.com and `@literal`. ~~old~~");
    assert_eq!(
        blocks[0]
            .runs
            .iter()
            .filter(|run| run.link.is_some())
            .count(),
        1
    );
    assert!(
        blocks[0]
            .runs
            .iter()
            .any(|run| run.link.as_deref() == Some("philosophy"))
    );
    assert!(
        blocks[0]
            .runs
            .iter()
            .any(|run| run.strike && run.text == "old")
    );
}

#[cfg_attr(test, test)]
fn mermaid_renders_pixels_and_reports_invalid_diagrams() {
    let (width, height, pixels) = diagram::rasterize("graph LR\n A[Readable text]").unwrap();
    let label_pixels = pixels.chunks_exact(4).enumerate().filter(|(index, rgba)| {
        let x = *index as u32 % width;
        let y = *index as u32 / width;
        x > width / 4 && x < width * 3 / 4 && y > height / 3 && y < height * 2 / 3
            && rgba[0] < 150 && rgba[1] < 150 && rgba[2] < 150 && rgba[3] > 0
    }).count();
    assert!(label_pixels > 10, "Diagram labels must be rendered with a bundled font");
    let (width, height, pixels) = diagram::rasterize("graph LR\n A[Protein] --> B[Sand]").unwrap();
    assert!(width > 20 && height > 20);
    assert_eq!(pixels.len(), width as usize * height as usize * 4);
    assert!(pixels.chunks_exact(4).any(|rgba| rgba[3] > 0));
    assert!(diagram::rasterize(&"x".repeat(16_385)).is_err());
    assert!(diagram::rasterize("this is not mermaid").is_err());
}

#[cfg_attr(test, test)]
fn preview_keeps_the_same_editor_and_follows_its_changes() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let parent = world.spawn(Node::default()).id();
    let input = world
        .spawn((
            crate::sand::text_editor("**First**", world.resource::<crate::theme::Typography>(), 0),
            ChildOf(parent),
        ))
        .id();
    attach_editor(
        &mut world,
        parent,
        input,
        Context {
            owner: parent,
            source: Source::Local,
        },
    );
    let preview = world.get::<Editor>(parent).unwrap().preview;
    assert_eq!(world.get::<Node>(input).unwrap().display, Display::None);
    Mode(true).apply(&mut world, parent);
    assert_eq!(world.get::<Node>(input).unwrap().display, Display::Flex);
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("## Changed");
    sync_editors(&mut world);
    assert_eq!(
        world.get::<Description>(preview).unwrap().source,
        "## Changed"
    );
    Mode(false).apply(&mut world, parent);
    assert_eq!(world.get::<Editor>(parent).unwrap().input, input);
    assert!(protects(&world, preview));
    assert!(!protects(&world, input));
}

crate::laboratory_cases! {
    rich_text_preserves_code_and_resolves_record_link_targets,
    mermaid_renders_pixels_and_reports_invalid_diagrams,
    preview_keeps_the_same_editor_and_follows_its_changes,
    record_links_keep_their_source_and_navigate_embedded_records,
}

#[cfg_attr(test, test)]
fn record_links_keep_their_source_and_navigate_embedded_records() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let root = world
        .spawn((
            crate::workspace::Workspaces::default(),
            crate::canvas::CanvasView::default(),
        ))
        .id();
    let instinct = crate::instinct::spawn(
        &mut world,
        root,
        1,
        bevy::math::DVec2::ZERO,
        crate::instinct::Instinct::default(),
    );
    Link {
        reference: "@tool".into(),
        context: Context {
            owner: instinct,
            source: Source::Local,
        },
    }
    .apply(&mut world, instinct);
    assert_eq!(
        world
            .get::<crate::instinct::Instinct>(instinct)
            .unwrap()
            .page
            .as_deref(),
        Some("tool")
    );
    let source = Source::Organ("another-organ".into());
    Link {
        reference: "another-record".into(),
        context: Context {
            owner: root,
            source: source.clone(),
        },
    }
    .apply(&mut world, root);
    let area = world
        .query::<&crate::area::InfluenceArea>()
        .single(&world)
        .unwrap();
    assert_eq!(area.protein.as_ref().unwrap().source, source);
    assert_eq!(
        area.protein.as_ref().unwrap().draft.query["where"][0]["all"][0]["slug_eq"],
        "another-record"
    );
}
