use super::*;

pub(super) struct Summary(Vec<(String, f32)>);

impl Summary {
    pub(super) fn new(value: &Value, depth: usize) -> Self {
        let _span = nucleus::performance::Span::new("transfer-review-summary");
        let mut lines = Vec::new();
        visit(value, depth, &mut |text, size| lines.push((text, size)));
        Self::prepared(lines)
    }

    pub(super) fn object<'a>(entries: impl IntoIterator<Item = (&'a str, &'a Value)>) -> Self {
        let _span = nucleus::performance::Span::new("transfer-review-summary");
        let mut lines = Vec::new();
        for (key, value) in entries {
            entry(key, value, 0, &mut |text, size| lines.push((text, size)));
        }
        Self::prepared(lines)
    }

    fn prepared(lines: Vec<(String, f32)>) -> Self {
        nucleus::performance::add("transfer-review-lines-prepared", lines.len() as u64);
        Self(lines)
    }

    pub(super) fn render(self, world: &mut World, parent: Entity) {
        for (text, size) in self.0 {
            crate::native::edit_mode::label(world, parent, &text, size);
        }
    }
}

fn entry(key: &str, value: &Value, depth: usize, emit: &mut impl FnMut(String, f32)) {
    if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
        return;
    }
    match value {
        Value::Object(_) | Value::Array(_) => {
            emit(model::human(key), 14.0);
            visit(value, depth + 1, emit);
        }
        _ => emit(format!("{}: {}", model::human(key), display(value)), 12.0),
    }
}

fn visit(value: &Value, depth: usize, emit: &mut impl FnMut(String, f32)) {
    if depth > 6 {
        emit(display(value), 12.0);
        return;
    }
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                entry(key, value, depth, emit);
            }
        }
        Value::Array(values) => {
            if values.len() > 150 {
                emit(
                    format!(
                        "Showing the first 150 of {} entries in this summary. The complete artifact remains unchanged.",
                        values.len()
                    ),
                    12.0,
                );
            }
            for (index, value) in values.iter().enumerate().take(150) {
                if value.is_object() {
                    emit((index + 1).to_string(), 12.0);
                }
                visit(value, depth + 1, emit);
            }
        }
        _ => emit(display(value), 12.0),
    }
}

pub(in super::super) fn render(world: &mut World, parent: Entity, value: &Value, depth: usize) {
    visit(value, depth, &mut |text, size| {
        crate::native::edit_mode::label(world, parent, &text, size);
    });
}
