use super::{Measure, Style};

const MAX_LAYOUT_MEASURE: f32 = 100_000.0;
const MAX_LAYOUT_NUMBER: f32 = 10_000.0;

impl Measure {
    /// Checks whether this unit has a finite value within the layout bound.
    pub fn validate(&self) -> bool {
        match self {
            Self::Auto => true,
            Self::Px(value)
            | Self::Percent(value)
            | Self::Vw(value)
            | Self::Vh(value)
            | Self::Vmin(value)
            | Self::Vmax(value) => value.abs() <= MAX_LAYOUT_MEASURE,
        }
    }
}

impl Style {
    /// Checks that all dimensions, numbers, and choices are supported.
    ///
    /// # Errors
    ///
    /// Returns the name of the first invalid style entry.
    pub fn validate(&self) -> Result<(), String> {
        for (key, value) in &self.dimensions {
            if !matches!(
                key.as_str(),
                "width"
                    | "height"
                    | "min-width"
                    | "min-height"
                    | "max-width"
                    | "max-height"
                    | "flex-basis"
                    | "left"
                    | "right"
                    | "top"
                    | "bottom"
                    | "row-gap"
                    | "column-gap"
                    | "padding-left"
                    | "padding-right"
                    | "padding-top"
                    | "padding-bottom"
                    | "margin-left"
                    | "margin-right"
                    | "margin-top"
                    | "margin-bottom"
                    | "border-left"
                    | "border-right"
                    | "border-top"
                    | "border-bottom"
            ) || !value.validate()
            {
                return Err(format!("Unsupported or invalid layout dimension: {key}."));
            }
        }
        for (key, value) in &self.numbers {
            if !matches!(key.as_str(), "flex-grow" | "flex-shrink" | "aspect-ratio")
                || !(0.0..=MAX_LAYOUT_NUMBER).contains(value)
                || key == "aspect-ratio" && *value == 0.0
            {
                return Err(format!("Unsupported or invalid layout number: {key}."));
            }
        }
        for (key, value) in &self.choices {
            let allowed: &[&str] = match key.as_str() {
                "display" => &["flex", "none", "grid"],
                "direction" => &["row", "column", "row-reverse", "column-reverse"],
                "wrap" => &["wrap", "no-wrap", "wrap-reverse"],
                "position" => &["relative", "absolute"],
                "align-items" | "align-self" => &[
                    "default",
                    "start",
                    "end",
                    "flex-start",
                    "flex-end",
                    "center",
                    "stretch",
                    "baseline",
                ],
                "justify-content" => &[
                    "default",
                    "start",
                    "end",
                    "flex-start",
                    "flex-end",
                    "center",
                    "space-between",
                    "space-around",
                    "space-evenly",
                ],
                "overflow-x" | "overflow-y" => &["visible", "clip", "hidden", "scroll"],
                _ => &[],
            };
            if !allowed.contains(&value.as_str()) {
                return Err(format!("Unsupported layout choice: {key}={value}."));
            }
        }
        Ok(())
    }
}
