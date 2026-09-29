use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}
#[derive(Debug, Default, PartialEq)]
pub struct Block {
    pub runs: Vec<Run>,
    pub heading: u8,
    pub code: Option<String>,
    pub quote: bool,
    pub image: Option<String>,
}

pub fn parse(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut style = Run::default();
    let mut numbered = Vec::new();
    let mut quotes = 0usize;
    let flush = |blocks: &mut Vec<Block>, block: &mut Block| {
        if !block.runs.is_empty() || block.image.is_some() {
            blocks.push(std::mem::take(block));
        }
    };
    let push = |block: &mut Block, style: &Run, value: &str| {
        if let Some(last) = block.runs.last_mut()
            && last.bold == style.bold
            && last.italic == style.italic
            && last.code == style.code
            && last.strike == style.strike
            && last.link == style.link
        {
            last.text.push_str(value);
        } else {
            block.runs.push(Run {
                text: value.into(),
                ..style.clone()
            });
        }
    };
    for event in Parser::new_ext(
        source,
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
    ) {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                flush(&mut blocks, &mut block);
                block.image = Some(dest_url.into_string());
            }
            Event::End(TagEnd::Image) => flush(&mut blocks, &mut block),
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut blocks, &mut block);
                block.heading = level as u8;
            }
            Event::Start(Tag::Paragraph) => {
                flush(&mut blocks, &mut block);
                block.quote = quotes > 0;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush(&mut blocks, &mut block);
                block.code = Some(match kind {
                    CodeBlockKind::Fenced(language) => language.into_string(),
                    CodeBlockKind::Indented => String::new(),
                });
                style.code = true;
            }
            Event::Start(Tag::Strong) => style.bold = true,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.into_string()),
            Event::End(TagEnd::Link) => style.link = None,
            Event::Start(Tag::List(first)) => numbered.push(first),
            Event::End(TagEnd::List(_)) => {
                numbered.pop();
            }
            Event::Start(Tag::Item) => {
                flush(&mut blocks, &mut block);
                let marker = match numbered.last_mut() {
                    Some(Some(value)) => {
                        let marker = format!("{value}. ");
                        *value += 1;
                        marker
                    }
                    _ => "• ".into(),
                };
                push(&mut block, &style, &marker);
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut blocks, &mut block);
                quotes += 1;
                block.quote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(&mut blocks, &mut block);
                quotes = quotes.saturating_sub(1);
                block.quote = quotes > 0;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush(&mut blocks, &mut block);
                style.code = false;
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::TableRow,
            ) => flush(&mut blocks, &mut block),
            Event::End(TagEnd::TableCell) => push(&mut block, &style, "   │   "),
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                push(&mut block, &style, &text)
            }
            Event::Code(text) => push(
                &mut block,
                &Run {
                    code: true,
                    ..style.clone()
                },
                &text,
            ),
            Event::SoftBreak => push(&mut block, &style, " "),
            Event::HardBreak => push(&mut block, &style, "\n"),
            Event::TaskListMarker(done) => {
                push(&mut block, &style, if done { "[x] " } else { "[ ] " })
            }
            Event::Rule => {
                flush(&mut blocks, &mut block);
                push(&mut block, &style, "────────────");
                flush(&mut blocks, &mut block);
            }
            _ => {}
        }
    }
    flush(&mut blocks, &mut block);
    for block in &mut blocks {
        if block.code.is_none() {
            block.runs = std::mem::take(&mut block.runs)
                .into_iter()
                .flat_map(links)
                .flat_map(slugs)
                .collect();
        }
    }
    blocks
}

fn links(run: Run) -> Vec<Run> {
    if run.code || run.link.is_some() {
        return vec![run];
    }
    let mut out = Vec::new();
    let mut rest = run.text.as_str();
    while let Some(start) = rest.find("[[") {
        let Some(end) = rest[start + 2..].find("]]").map(|end| end + start + 2) else {
            break;
        };
        let (title, reference) = rest[start + 2..end]
            .split_once('|')
            .unwrap_or((&rest[start + 2..end], &rest[start + 2..end]));
        if start > 0 {
            out.push(Run {
                text: rest[..start].into(),
                ..run.clone()
            });
        }
        out.push(Run {
            text: title.into(),
            link: Some(reference.into()),
            ..run.clone()
        });
        rest = &rest[end + 2..];
    }
    if !rest.is_empty() {
        out.push(Run {
            text: rest.into(),
            ..run.clone()
        });
    }
    out
}

fn slugs(run: Run) -> Vec<Run> {
    if run.code || run.link.is_some() {
        return vec![run];
    }
    let mut out = Vec::new();
    let mut pending = 0;
    for (start, character) in run.text.char_indices() {
        if character != '@'
            || run.text[..start]
                .chars()
                .next_back()
                .is_some_and(|previous| previous.is_alphanumeric() || previous == '_')
        {
            continue;
        }
        let end = run.text[start + 1..]
            .char_indices()
            .find(|(_, character)| {
                !character.is_alphanumeric() && *character != '-' && *character != '_'
            })
            .map_or(run.text.len(), |(end, _)| start + 1 + end);
        if end == start + 1 {
            continue;
        }
        if pending < start {
            out.push(Run {
                text: run.text[pending..start].into(),
                ..run.clone()
            });
        }
        out.push(Run {
            text: run.text[start..end].into(),
            link: Some(run.text[start + 1..end].into()),
            ..run.clone()
        });
        pending = end;
    }
    if pending < run.text.len() {
        out.push(Run {
            text: run.text[pending..].into(),
            ..run.clone()
        });
    }
    out
}
