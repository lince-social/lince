use nucleus::DecimalValue;

pub const LINGUA_EXTENSION: &str = "lingua";
const FENCE: &str = "---";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Projection {
    pub uid: String,
    pub assertions: Vec<Line>,
    pub quantity: Option<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub predicate: String,
    pub identity: bool,
    pub object: Option<Link>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub title: String,
    pub uid: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinguaError {
    NoPrelude,
    BadLine(String),
}

impl std::fmt::Display for LinguaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPrelude => write!(f, "the file has no --- metadata block"),
            Self::BadLine(line) => write!(f, "cannot read this line: {line}"),
        }
    }
}

pub fn split(text: &str) -> (Option<&str>, &str) {
    let Some(rest) = text.strip_prefix(FENCE) else {
        return (None, text);
    };
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == FENCE {
            let prelude = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (Some(prelude), body.strip_prefix('\n').unwrap_or(body));
        }
        offset += line.len();
    }
    (None, text)
}

pub fn render_file(projection: &Projection, body: &str) -> String {
    let mut out = String::from(FENCE);
    out.push('\n');
    if !projection.uid.is_empty() {
        out.push_str("uid: ");
        out.push_str(&projection.uid);
        out.push('\n');
    }
    for line in &projection.assertions {
        out.push('@');
        if line.identity {
            out.push('@');
        }
        out.push_str(&line.predicate);
        if let Some(link) = &line.object {
            out.push_str(" [[");
            out.push_str(&link.title);
            out.push('|');
            out.push_str(&link.uid);
            out.push_str("]]");
        }
        if let Some(quantity) = &line.quantity {
            out.push(' ');
            out.push_str(quantity);
            if let Some(unit) = &line.unit {
                out.push_str(" @");
                out.push_str(unit);
            }
        }
        out.push('\n');
    }
    if let Some((amount, unit)) = &projection.quantity {
        out.push_str("quantity: ");
        out.push_str(amount);
        if let Some(unit) = unit {
            out.push_str(" @");
            out.push_str(unit);
        }
        out.push('\n');
    }
    out.push_str(FENCE);
    out.push_str("\n\n");
    out.push_str(body);
    out
}

pub fn parse_prelude(prelude: &str) -> Result<Projection, LinguaError> {
    let mut out = Projection::default();
    for raw in prelude.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(uid) = line.strip_prefix("uid:") {
            out.uid = uid.trim().to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("quantity:") {
            let (amount, unit) = split_amount(rest.trim());
            out.quantity = Some((amount.to_string(), unit.map(str::to_string)));
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
            out.assertions.push(parse_assertion(rest)?);
            continue;
        }
        return Err(LinguaError::BadLine(raw.to_string()));
    }
    Ok(out)
}

pub fn parse_file(text: &str) -> Result<(Option<Projection>, String), LinguaError> {
    let (prelude, body) = split(text);
    match prelude {
        None => Ok((None, body.to_string())),
        Some(prelude) => Ok((Some(parse_prelude(prelude)?), body.to_string())),
    }
}

fn parse_assertion(rest: &str) -> Result<Line, LinguaError> {
    let (identity, rest) = match rest.strip_prefix('@') {
        Some(rest) => (true, rest),
        None => (false, rest),
    };
    let rest = rest.trim();
    let (predicate, mut tail) = match rest.find(char::is_whitespace) {
        Some(at) => (&rest[..at], rest[at..].trim()),
        None => (rest, ""),
    };
    if predicate.is_empty() {
        return Err(LinguaError::BadLine(format!("@{rest}")));
    }
    let mut object = None;
    if let Some(inner) = tail.strip_prefix("[[") {
        let Some(close) = inner.find("]]") else {
            return Err(LinguaError::BadLine(format!("@{rest}")));
        };
        let target = &inner[..close];
        let Some((title, uid)) = target.rsplit_once('|') else {
            return Err(LinguaError::BadLine(format!("@{rest}")));
        };
        object = Some(Link {
            title: title.trim().to_string(),
            uid: uid.trim().to_string(),
        });
        tail = inner[close + 2..].trim();
    }
    let (quantity, unit) = if tail.is_empty() {
        (None, None)
    } else {
        let (amount, unit) = split_amount(tail);
        (Some(amount.to_string()), unit.map(str::to_string))
    };
    Ok(Line {
        predicate: predicate.to_string(),
        identity,
        object,
        quantity,
        unit,
    })
}

fn split_amount(text: &str) -> (&str, Option<&str>) {
    match text.split_once(" @") {
        Some((amount, unit)) => (amount.trim(), Some(unit.trim())),
        None => (text.trim(), None),
    }
}

pub fn decimal_text(value: DecimalValue) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(predicate: &str) -> Line {
        Line {
            predicate: predicate.to_string(),
            identity: false,
            object: None,
            quantity: None,
            unit: None,
        }
    }

    #[test]
    fn a_rendered_file_reads_back_as_what_was_rendered() {
        let projection = Projection {
            uid: "rec_1".to_string(),
            assertions: vec![
                Line {
                    identity: true,
                    ..line("task")
                },
                Line {
                    quantity: Some("1".to_string()),
                    ..line("chapter")
                },
                Line {
                    object: Some(Link {
                        title: "Project A".to_string(),
                        uid: "rec_2".to_string(),
                    }),
                    ..line("references")
                },
                Line {
                    quantity: Some("12".to_string()),
                    unit: Some("hour".to_string()),
                    ..line("estimate")
                },
            ],
            quantity: Some(("3.50".to_string(), Some("hour".to_string()))),
        };
        let file = render_file(&projection, "Write the brief.\n");
        let (parsed, body) = parse_file(&file).expect("parses");
        assert_eq!(parsed.as_ref(), Some(&projection));
        assert_eq!(body, "Write the brief.\n");
    }

    #[test]
    fn identity_survives_the_round_trip_as_identity() {
        let file = render_file(
            &Projection {
                uid: "rec_1".to_string(),
                assertions: vec![
                    Line {
                        identity: true,
                        ..line("task")
                    },
                    line("urgent"),
                ],
                quantity: None,
            },
            "",
        );
        assert!(file.contains("@@task\n"), "identity marked: {file}");
        let (parsed, _) = parse_file(&file).expect("parses");
        let parsed = parsed.expect("has a prelude");
        assert!(parsed.assertions[0].identity, "identity read back");
        assert!(!parsed.assertions[1].identity, "a tag stays a tag");
    }

    #[test]
    fn a_plain_note_with_no_prelude_is_all_body() {
        let (prelude, body) = split("Just some text.\n");
        assert_eq!(prelude, None);
        assert_eq!(body, "Just some text.\n");
    }

    #[test]
    fn an_unclosed_fence_is_body_rather_than_swallowed_metadata() {
        let text = "---\n@task\nstill writing";
        let (prelude, body) = split(text);
        assert_eq!(prelude, None);
        assert_eq!(body, text);
    }

    #[test]
    fn a_link_without_a_uid_is_refused_rather_than_resolved_by_title() {
        let err = parse_prelude("@references [[Project A]]\n").expect_err("refused");
        assert_eq!(
            err,
            LinguaError::BadLine("@references [[Project A]]".to_string())
        );
    }

    #[test]
    fn a_line_that_is_not_lingua_is_an_error_not_a_guess() {
        let err = parse_prelude("title: Something\n").expect_err("refused");
        assert_eq!(err, LinguaError::BadLine("title: Something".to_string()));
    }

    #[test]
    fn a_body_containing_a_fence_is_not_cut_short() {
        let projection = Projection {
            uid: "rec_1".to_string(),
            assertions: vec![line("task")],
            quantity: None,
        };
        let body = "First.\n\n---\n\nSecond.\n";
        let file = render_file(&projection, body);
        let (parsed, read_back) = parse_file(&file).expect("parses");
        assert_eq!(parsed.as_ref(), Some(&projection));
        assert_eq!(read_back, body, "a fence in the body stays in the body");
    }
}
