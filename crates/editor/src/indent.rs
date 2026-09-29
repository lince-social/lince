use crate::Edit;
use ropey::Rope;

pub fn changes(rope: &Rope, selection: [usize; 2], outdent: bool) -> (Vec<Edit>, [usize; 2]) {
    let selection = selection.map(|p| p.min(rope.len_chars()));
    let start = selection[0].min(selection[1]);
    let end = selection[0].max(selection[1]);
    let first = rope.char_to_line(start);
    if start == end && !outdent {
        let column = rope
            .slice(rope.line_to_char(first)..start)
            .chars()
            .fold(0, |n, ch| if ch == '\t' { (n / 4 + 1) * 4 } else { n + 1 });
        let text = " ".repeat(4 - column % 4);
        let position = start + text.len();
        return (
            vec![Edit {
                range: start..start,
                text,
            }],
            [position; 2],
        );
    }
    let last = rope.char_to_line(if end > start { end - 1 } else { end });
    let edits: Vec<_> = (first..=last)
        .filter_map(|line| {
            let start = rope.line_to_char(line);
            if outdent {
                let text = rope.line(line);
                let count = if text.chars().next() == Some('\t') {
                    1
                } else {
                    text.chars().take(4).take_while(|ch| *ch == ' ').count()
                };
                (count > 0).then(|| Edit {
                    range: start..start + count,
                    text: String::new(),
                })
            } else {
                Some(Edit {
                    range: start..start,
                    text: "    ".into(),
                })
            }
        })
        .collect();
    let mapped = selection.map(|position| {
        let mut next = position;
        for edit in &edits {
            if position >= edit.range.start {
                next = next - (position.min(edit.range.end) - edit.range.start)
                    + edit.text.chars().count();
            }
        }
        next
    });
    (edits, mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Buffer;

    #[test]
    fn multiline_indent_is_one_undo_and_excludes_the_last_empty_selection_line() {
        let mut buffer = Buffer::new("猫\r\nalpha\r\nomega").unwrap();
        let (edits, selected) = changes(&buffer.snapshot(), [0, 10], false);
        assert_eq!(edits.len(), 2);
        buffer.edit_batch(&edits).unwrap();
        assert_eq!(buffer.text(), "    猫\r\n    alpha\r\nomega");
        buffer.undo(false).unwrap();
        assert_eq!(buffer.text(), "猫\r\nalpha\r\nomega");
        buffer.undo(true).unwrap();
        let (edits, restored) = changes(&buffer.snapshot(), selected, true);
        buffer.edit_many(&edits).unwrap();
        assert_eq!(restored, [0, 10]);
        assert_eq!(buffer.text(), "猫\r\nalpha\r\nomega");
    }

    #[test]
    fn invalid_batches_do_not_partially_change_the_document() {
        let mut buffer = Buffer::new("abc").unwrap();
        assert!(
            buffer
                .edit_many(&[
                    Edit {
                        range: 0..1,
                        text: "x".into()
                    },
                    Edit {
                        range: 9..10,
                        text: "y".into()
                    }
                ])
                .is_err()
        );
        assert_eq!(buffer.text(), "abc");
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn indentation_does_not_merge_with_typing_before_or_after_it() {
        let mut buffer = Buffer::new("").unwrap();
        buffer
            .edit(Edit {
                range: 0..0,
                text: "a".into(),
            })
            .unwrap();
        let (edits, _) = changes(&buffer.snapshot(), [0, 1], false);
        buffer.edit_batch(&edits).unwrap();
        buffer
            .edit(Edit {
                range: 5..5,
                text: "b".into(),
            })
            .unwrap();
        assert_eq!(buffer.text(), "    ab");
        buffer.undo(false).unwrap();
        assert_eq!(buffer.text(), "    a");
        buffer.undo(false).unwrap();
        assert_eq!(buffer.text(), "a");
        buffer.undo(false).unwrap();
        assert_eq!(buffer.text(), "");
    }
}
