#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyMention {
    pub title: String,
    pub uid: Option<String>,
    pub start: usize,
    pub end: usize,
}

pub fn mentions(body: &str) -> Vec<BodyMention> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;
    while index + 1 < bytes.len() {
        if bytes[index] != b'[' || bytes[index + 1] != b'[' {
            index += 1;
            continue;
        }
        let open = index + 2;
        let Some(offset) = body[open..].find("]]") else {
            break;
        };
        let close = open + offset;
        let inner = &body[open..close];
        if !inner.contains("[[") {
            let (title, uid) = match inner.split_once('|') {
                Some((title, uid)) => (title.trim(), Some(uid.trim().to_string())),
                None => (inner.trim(), None),
            };
            if !title.is_empty() {
                out.push(BodyMention {
                    title: title.to_string(),
                    uid: uid.filter(|uid| !uid.is_empty()),
                    start: index,
                    end: close + 2,
                });
            }
        }
        index = close + 2;
    }
    out
}

pub fn link_first_mentions(body: &str, known: &[(String, String)], self_uid: &str) -> String {
    let already: Vec<String> = mentions(body)
        .into_iter()
        .filter_map(|mention| mention.uid)
        .collect();
    let spans: Vec<(usize, usize)> = mentions(body)
        .into_iter()
        .map(|mention| (mention.start, mention.end))
        .collect();

    let mut targets: Vec<&(String, String)> = known
        .iter()
        .filter(|(_, uid)| uid != self_uid && !already.contains(uid))
        .filter(|(title, _)| !title.trim().is_empty())
        .collect();
    targets.sort_by(|left, right| {
        right
            .0
            .chars()
            .count()
            .cmp(&left.0.chars().count())
            .then_with(|| left.1.cmp(&right.1))
    });

    let mut out = body.to_string();
    let mut claimed: Vec<(usize, usize)> = spans;
    for (title, uid) in targets {
        let Some(at) = first_free_whole_word(&out, title, &claimed) else {
            continue;
        };
        let end = at + title.len();
        let replacement = format!("[[{title}|{uid}]]");
        out.replace_range(at..end, &replacement);
        let shift = replacement.len() - title.len();
        for span in &mut claimed {
            if span.0 >= end {
                span.0 += shift;
                span.1 += shift;
            }
        }
        claimed.push((at, at + replacement.len()));
    }
    out
}

fn first_free_whole_word(
    haystack: &str,
    needle: &str,
    claimed: &[(usize, usize)],
) -> Option<usize> {
    let mut from = 0usize;
    while let Some(offset) = haystack[from..].find(needle) {
        let at = from + offset;
        let end = at + needle.len();
        let inside = claimed
            .iter()
            .any(|(start, stop)| at < *stop && end > *start);
        if !inside && boundary_before(haystack, at) && boundary_after(haystack, end) {
            return Some(at);
        }
        from = at + needle.len().max(1);
        if from >= haystack.len() {
            break;
        }
    }
    None
}

fn boundary_before(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|character| !character.is_alphanumeric())
}

fn boundary_after(text: &str, at: usize) -> bool {
    text[at..]
        .chars()
        .next()
        .is_none_or(|character| !character.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> Vec<(String, String)> {
        vec![
            ("Project A".to_string(), "rec_a".to_string()),
            ("Project".to_string(), "rec_p".to_string()),
        ]
    }

    #[test]
    fn only_the_first_mention_becomes_a_link() {
        let body = "Project A is late. Project A again.";
        let out = link_first_mentions(body, &known()[..1], "rec_self");
        assert_eq!(out, "[[Project A|rec_a]] is late. Project A again.");
    }

    #[test]
    fn a_link_already_written_is_left_alone_and_counts_as_the_mention() {
        let body = "See [[Project A|rec_a]] and Project A.";
        let out = link_first_mentions(body, &known()[..1], "rec_self");
        assert_eq!(out, body);
    }

    #[test]
    fn a_record_never_links_to_itself() {
        let out = link_first_mentions("Project A stands alone.", &known()[..1], "rec_a");
        assert_eq!(out, "Project A stands alone.");
    }

    #[test]
    fn the_longer_title_wins_the_overlap() {
        let out = link_first_mentions("Project A ships.", &known(), "rec_self");
        assert_eq!(out, "[[Project A|rec_a]] ships.");
    }

    #[test]
    fn a_title_inside_a_word_is_not_a_mention() {
        let out = link_first_mentions("Projects everywhere.", &known()[1..], "rec_self");
        assert_eq!(out, "Projects everywhere.");
    }

    #[test]
    fn a_bare_title_link_parses_with_no_uid() {
        let found = mentions("ping [[Project A]] here");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Project A");
        assert_eq!(found[0].uid, None);
    }

    #[test]
    fn the_uid_is_read_even_when_the_title_has_been_renamed() {
        let found = mentions("[[Whatever They Call It Now|rec_a]]");
        assert_eq!(found[0].uid.as_deref(), Some("rec_a"));
    }
}
