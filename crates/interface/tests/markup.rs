#![cfg(feature = "models")]

use lince_interface::markup;

#[test]
fn transclusion_is_distinct_from_links_images_and_literal_code() {
    let blocks = markup::parse(
        "before ![[Title|r_example]] after [[link]]\n\n`![[literal]]`\n\n```text\n![[code]]\n```\n\n![Image](asset:r_test/hash)\n\n\\![[escaped]]",
    );
    assert_eq!(
        blocks
            .iter()
            .filter_map(|block| block.transclusion.as_deref())
            .collect::<Vec<_>>(),
        vec!["r_example"]
    );
    assert!(
        blocks
            .iter()
            .flat_map(|block| &block.runs)
            .any(|run| run.link.as_deref() == Some("link"))
    );
    assert!(
        blocks
            .iter()
            .any(|block| block.image.as_deref() == Some("asset:r_test/hash"))
    );
    let mixed = markup::parse("\\![[literal]] and ![[included]]");
    assert_eq!(mixed.iter().filter_map(|block| block.transclusion.as_deref()).collect::<Vec<_>>(), vec!["included"]);
}

#[test]
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
    let blocks = markup::parse(
        "Hi [@old-slug](record:r_example), [@Jane \\[work\\]](record:r_person) and @record_slug.",
    );
    let links: Vec<_> = blocks[0]
        .runs
        .iter()
        .filter_map(|run| run.link.as_ref())
        .collect();
    assert!(links.iter().any(|link| link.as_str() == "record:r_example"));
    assert!(links.iter().any(|link| link.as_str() == "record:r_person"));
    assert!(links.iter().any(|link| link.as_str() == "record_slug"));
}
