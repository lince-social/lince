use super::*;
use api::authoring::{Measure, Style};

fn measure(value: Val) -> Measure {
    match value {
        Val::Px(value) => Measure::Px(value),
        Val::Percent(value) => Measure::Percent(value),
        Val::Vw(value) => Measure::Vw(value),
        Val::Vh(value) => Measure::Vh(value),
        Val::VMin(value) => Measure::Vmin(value),
        Val::VMax(value) => Measure::Vmax(value),
        _ => Measure::Auto,
    }
}

fn val(value: &Measure) -> Val {
    match value {
        Measure::Auto => Val::Auto,
        Measure::Px(value) => px(*value),
        Measure::Percent(value) => percent(*value),
        Measure::Vw(value) => Val::Vw(*value),
        Measure::Vh(value) => Val::Vh(*value),
        Measure::Vmin(value) => Val::VMin(*value),
        Measure::Vmax(value) => Val::VMax(*value),
    }
}

pub(super) fn capture(node: &Node) -> Style {
    let dimensions = [
        ("width", node.width),
        ("height", node.height),
        ("min-width", node.min_width),
        ("min-height", node.min_height),
        ("max-width", node.max_width),
        ("max-height", node.max_height),
        ("flex-basis", node.flex_basis),
        ("left", node.left),
        ("right", node.right),
        ("top", node.top),
        ("bottom", node.bottom),
        ("row-gap", node.row_gap),
        ("column-gap", node.column_gap),
        ("padding-left", node.padding.left),
        ("padding-right", node.padding.right),
        ("padding-top", node.padding.top),
        ("padding-bottom", node.padding.bottom),
        ("margin-left", node.margin.left),
        ("margin-right", node.margin.right),
        ("margin-top", node.margin.top),
        ("margin-bottom", node.margin.bottom),
        ("border-left", node.border.left),
        ("border-right", node.border.right),
        ("border-top", node.border.top),
        ("border-bottom", node.border.bottom),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), measure(value)))
    .collect();
    let mut numbers = BTreeMap::from([
        ("flex-grow".into(), node.flex_grow),
        ("flex-shrink".into(), node.flex_shrink),
    ]);
    if let Some(ratio) = node.aspect_ratio {
        numbers.insert("aspect-ratio".into(), ratio);
    }
    let choices = BTreeMap::from([
        (
            "display".into(),
            match node.display {
                Display::None => "none",
                Display::Grid => "grid",
                _ => "flex",
            }
            .into(),
        ),
        (
            "direction".into(),
            match node.flex_direction {
                FlexDirection::Row => "row",
                FlexDirection::Column => "column",
                FlexDirection::RowReverse => "row-reverse",
                FlexDirection::ColumnReverse => "column-reverse",
            }
            .into(),
        ),
        (
            "wrap".into(),
            match node.flex_wrap {
                FlexWrap::Wrap => "wrap",
                FlexWrap::WrapReverse => "wrap-reverse",
                FlexWrap::NoWrap => "no-wrap",
            }
            .into(),
        ),
        (
            "position".into(),
            match node.position_type {
                PositionType::Absolute => "absolute",
                _ => "relative",
            }
            .into(),
        ),
        (
            "align-items".into(),
            match node.align_items {
                AlignItems::Start => "start",
                AlignItems::End => "end",
                AlignItems::FlexStart => "flex-start",
                AlignItems::FlexEnd => "flex-end",
                AlignItems::Center => "center",
                AlignItems::Stretch => "stretch",
                AlignItems::Baseline => "baseline",
                _ => "default",
            }
            .into(),
        ),
        (
            "align-self".into(),
            match node.align_self {
                AlignSelf::Start => "start",
                AlignSelf::End => "end",
                AlignSelf::FlexStart => "flex-start",
                AlignSelf::FlexEnd => "flex-end",
                AlignSelf::Center => "center",
                AlignSelf::Stretch => "stretch",
                AlignSelf::Baseline => "baseline",
                _ => "default",
            }
            .into(),
        ),
        (
            "justify-content".into(),
            match node.justify_content {
                JustifyContent::Start => "start",
                JustifyContent::End => "end",
                JustifyContent::FlexStart => "flex-start",
                JustifyContent::FlexEnd => "flex-end",
                JustifyContent::Center => "center",
                JustifyContent::SpaceBetween => "space-between",
                JustifyContent::SpaceAround => "space-around",
                JustifyContent::SpaceEvenly => "space-evenly",
                _ => "default",
            }
            .into(),
        ),
        (
            "overflow-x".into(),
            match node.overflow.x {
                OverflowAxis::Clip => "clip",
                OverflowAxis::Hidden => "hidden",
                OverflowAxis::Scroll => "scroll",
                _ => "visible",
            }
            .into(),
        ),
        (
            "overflow-y".into(),
            match node.overflow.y {
                OverflowAxis::Clip => "clip",
                OverflowAxis::Hidden => "hidden",
                OverflowAxis::Scroll => "scroll",
                _ => "visible",
            }
            .into(),
        ),
    ]);
    Style {
        dimensions,
        numbers,
        choices,
    }
}

pub(super) fn apply(node: &mut Node, style: &Style) {
    for (key, value) in &style.dimensions {
        let target = match key.as_str() {
            "width" => &mut node.width,
            "height" => &mut node.height,
            "min-width" => &mut node.min_width,
            "min-height" => &mut node.min_height,
            "max-width" => &mut node.max_width,
            "max-height" => &mut node.max_height,
            "flex-basis" => &mut node.flex_basis,
            "left" => &mut node.left,
            "right" => &mut node.right,
            "top" => &mut node.top,
            "bottom" => &mut node.bottom,
            "row-gap" => &mut node.row_gap,
            "column-gap" => &mut node.column_gap,
            "padding-left" => &mut node.padding.left,
            "padding-right" => &mut node.padding.right,
            "padding-top" => &mut node.padding.top,
            "padding-bottom" => &mut node.padding.bottom,
            "margin-left" => &mut node.margin.left,
            "margin-right" => &mut node.margin.right,
            "margin-top" => &mut node.margin.top,
            "margin-bottom" => &mut node.margin.bottom,
            "border-left" => &mut node.border.left,
            "border-right" => &mut node.border.right,
            "border-top" => &mut node.border.top,
            "border-bottom" => &mut node.border.bottom,
            _ => continue,
        };
        *target = val(value);
    }
    for (key, value) in &style.numbers {
        match key.as_str() {
            "flex-grow" => node.flex_grow = *value,
            "flex-shrink" => node.flex_shrink = *value,
            "aspect-ratio" => node.aspect_ratio = Some(*value),
            _ => {}
        }
    }
    for (key, value) in &style.choices {
        let value = value.as_str();
        match key.as_str() {
            "display" => {
                node.display = match value {
                    "none" => Display::None,
                    "grid" => Display::Grid,
                    _ => Display::Flex,
                }
            }
            "direction" => {
                node.flex_direction = match value {
                    "column" => FlexDirection::Column,
                    "row-reverse" => FlexDirection::RowReverse,
                    "column-reverse" => FlexDirection::ColumnReverse,
                    _ => FlexDirection::Row,
                }
            }
            "wrap" => {
                node.flex_wrap = match value {
                    "wrap" => FlexWrap::Wrap,
                    "wrap-reverse" => FlexWrap::WrapReverse,
                    _ => FlexWrap::NoWrap,
                }
            }
            "position" => {
                node.position_type = if value == "absolute" {
                    PositionType::Absolute
                } else {
                    PositionType::Relative
                }
            }
            "align-items" => {
                node.align_items = match value {
                    "start" => AlignItems::Start,
                    "end" => AlignItems::End,
                    "flex-start" => AlignItems::FlexStart,
                    "flex-end" => AlignItems::FlexEnd,
                    "center" => AlignItems::Center,
                    "stretch" => AlignItems::Stretch,
                    "baseline" => AlignItems::Baseline,
                    _ => AlignItems::Default,
                }
            }
            "align-self" => {
                node.align_self = match value {
                    "start" => AlignSelf::Start,
                    "end" => AlignSelf::End,
                    "flex-start" => AlignSelf::FlexStart,
                    "flex-end" => AlignSelf::FlexEnd,
                    "center" => AlignSelf::Center,
                    "stretch" => AlignSelf::Stretch,
                    "baseline" => AlignSelf::Baseline,
                    _ => AlignSelf::Auto,
                }
            }
            "justify-content" => {
                node.justify_content = match value {
                    "start" => JustifyContent::Start,
                    "end" => JustifyContent::End,
                    "flex-start" => JustifyContent::FlexStart,
                    "flex-end" => JustifyContent::FlexEnd,
                    "center" => JustifyContent::Center,
                    "space-between" => JustifyContent::SpaceBetween,
                    "space-around" => JustifyContent::SpaceAround,
                    "space-evenly" => JustifyContent::SpaceEvenly,
                    _ => JustifyContent::Default,
                }
            }
            "overflow-x" | "overflow-y" => {
                let target = if key == "overflow-x" {
                    &mut node.overflow.x
                } else {
                    &mut node.overflow.y
                };
                *target = match value {
                    "clip" => OverflowAxis::Clip,
                    "hidden" => OverflowAxis::Hidden,
                    "scroll" => OverflowAxis::Scroll,
                    _ => OverflowAxis::Visible,
                };
            }
            _ => {}
        }
    }
}

pub(super) fn reset(node: &mut Node, original: &Node, previous: &Style) {
    let mut restored = capture(original);
    restored
        .dimensions
        .retain(|key, _| previous.dimensions.contains_key(key));
    restored
        .numbers
        .retain(|key, _| previous.numbers.contains_key(key));
    restored
        .choices
        .retain(|key, _| previous.choices.contains_key(key));
    apply(node, &restored);
    if previous.numbers.contains_key("aspect-ratio") {
        node.aspect_ratio = original.aspect_ratio;
    }
    for key in previous.choices.keys() {
        match key.as_str() {
            "align-items" => node.align_items = original.align_items,
            "align-self" => node.align_self = original.align_self,
            "justify-content" => node.justify_content = original.justify_content,
            "overflow-x" => node.overflow.x = original.overflow.x,
            "overflow-y" => node.overflow.y = original.overflow.y,
            _ => {}
        }
    }
}
