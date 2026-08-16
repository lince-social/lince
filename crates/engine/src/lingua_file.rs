//! The `.lingua` file shape: a Record's Lingua state above its body.
//!
//! ```text
//! ---
//! uid: rec_01J...
//! @@task
//! @chapter 1
//! @references [[Project A|rec_01K...]]
//! quantity: 12 @hour
//! ---
//!
//! Write the project brief.
//! ```
//!
//! **This is a PROJECTION, not a second database.** Records, Concepts and
//! Assertions stay authoritative; the prelude is generated from them on every
//! tick and compared against what is on disk. The delimiters look like front
//! matter and deliberately are NOT YAML — every line is Lingua.
//!
//! Three shapes that are easy to get wrong and are settled here:
//!
//! - **`@@` is the IDENTITY concept, `@` is an ordinary assertion.** A Record
//!   carries `identity_predicate_uid` naming which of its assertions says what
//!   the Record *is*, as opposed to what is merely true of it. Rendering both
//!   as `@task` would make the format silently lossy — a round trip would turn
//!   an identity into an ordinary tag, and nobody would notice for months.
//! - **A link carries the title AND the uid** (`[[Title|uid]]`). The title is
//!   for the person reading the file; the uid is what identifies the Record.
//!   Titles collide and get renamed, so a title-only link would retarget
//!   itself the day two Records share a name.
//! - **Rendering is DETERMINISTIC** — assertions come back ordered by
//!   predicate name, then object, then uid. Not for tidiness: a render that
//!   reorders between ticks rewrites the file, the disk half reads its own
//!   rewrite back as an edit, and that becomes an op that travels to every
//!   peer. Stable order is what keeps a no-op tick silent.
//!
//! **Everything here round-trips, the `quantity:` line included.** A level is
//! a fold of Ledger Facts rather than a column, so setting it from a file
//! appends the exact DIFFERENCE between the file and the fold as an ordinary
//! user-caused Fact: you write 12, it is 12, and the Ledger still explains how
//! it got there. Assertion quantities (`@chapter 1`) are ordinary exact
//! columns. An edit to the block is a real retract and assert, and applies in
//! full or not at all — a file must never invent a meaning, so one unknown
//! Concept refuses the whole block rather than applying the readable half.

use nucleus::DecimalValue;

pub const LINGUA_EXTENSION: &str = "lingua";
const FENCE: &str = "---";

/// A Record's Lingua state, as it appears between the delimiters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Projection {
    /// The Record this file is. Empty when a person wrote the file by hand and
    /// Lince has not adopted it yet.
    pub uid: String,
    pub assertions: Vec<Line>,
    /// Exact decimal text plus the unit's concept name.
    pub quantity: Option<(String, Option<String>)>,
}

/// One assertion line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Canonical concept name of the predicate, without the `@`.
    pub predicate: String,
    /// `true` for `@@` — this assertion is what the Record IS.
    pub identity: bool,
    pub object: Option<Link>,
    /// Exact decimal text, never a float.
    pub quantity: Option<String>,
    /// Unit concept name.
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub title: String,
    pub uid: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinguaError {
    /// No opening or no closing `---`.
    NoPrelude,
    /// A line inside the delimiters that is not Lingua.
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

/// Split a file into its prelude text and its body.
///
/// A file with no prelude is all body — that is how a plain note someone
/// dropped into the folder is read, and it must not be an error.
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
    // An opening fence with no closing one is not a prelude at all. Treating
    // it as one would swallow the whole file into metadata and lose the text.
    (None, text)
}

/// Render a projection plus a body into the file a Record wants on disk.
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

/// Read the delimited block back into a projection.
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

/// Convenience: the whole file in one call.
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
        // `Title|uid`. A link with no uid half is not addressed at all — it
        // names a title, and titles are not identity.
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

/// `12 @hour` -> `("12", Some("hour"))`.
fn split_amount(text: &str) -> (&str, Option<&str>) {
    match text.split_once(" @") {
        Some((amount, unit)) => (amount.trim(), Some(unit.trim())),
        None => (text.trim(), None),
    }
}

/// Exact decimal text for a projection. Never `to_f64`: the whole point of
/// storing `(mantissa, scale)` is that a projection cannot disagree with the
/// Ledger by a rounding step.
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
        // The lossy version of this format renders `@@task` as `@task`, and
        // then a round trip quietly demotes what the Record IS to a tag.
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
        // Losing the text would be the expensive failure here, so an opening
        // fence with no closing one is not treated as a prelude at all.
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
