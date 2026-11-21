use std::fmt::{self, Display, Write};

use serde::Serialize;
use serde_json::Value;

mod frequency;
mod program;
mod values;

pub use frequency::format_frequency;
pub use program::format_program;

fn render(write: impl FnOnce(&mut String) -> fmt::Result) -> String {
    let mut output = String::new();
    write(&mut output).expect("writing to String cannot fail");
    output
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a Rust string cannot fail")
}

pub(super) fn enum_atom(value: &impl Serialize) -> String {
    match serde_json::to_value(value).expect("serializing a closed enum cannot fail") {
        Value::String(atom) => atom,
        _ => unreachable!("closed enum must serialize as a string atom"),
    }
}

fn write_joined(
    output: &mut impl Write,
    values: impl IntoIterator<Item = impl Display>,
) -> fmt::Result {
    for (index, value) in values.into_iter().enumerate() {
        if index != 0 {
            output.write_str(", ")?;
        }
        write!(output, "{value}")?;
    }
    Ok(())
}
