use crate::{
    sand::{AccessibilityRole, SandElement, SandPackage, SandValue},
    style::ResolvedStyle,
};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use std::collections::{BTreeMap, BTreeSet};
use wgpu::{Device, MultisampleState, Queue, RenderPass, TextureFormat};

const LATO_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Lato/Lato-Regular.ttf");

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RetainedRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RetainedRect {
    pub fn contains(self, point: [f64; 2]) -> bool {
        point[0] >= f64::from(self.x)
            && point[0] <= f64::from(self.x + self.width)
            && point[1] >= f64::from(self.y)
            && point[1] <= f64::from(self.y + self.height)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetainedNode {
    pub key: String,
    pub definition_uid: String,
    pub element: SandElement,
    pub role: AccessibilityRole,
    pub label: String,
    pub description: Option<String>,
    pub rect: RetainedRect,
    pub interactive: bool,
    pub focused: bool,
    pub show_text: bool,
    pub depth: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RetainedScene {
    pub root_uid: String,
    pub nodes: Vec<RetainedNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetainedPlacement {
    pub key: String,
    pub definition_uid: String,
    pub inputs: BTreeMap<String, SandValue>,
    pub rect: RetainedRect,
}

impl RetainedScene {
    pub fn from_package(
        package: &SandPackage,
        root_uid: &str,
        root_inputs: BTreeMap<String, SandValue>,
        viewport: RetainedRect,
        focused_interactive: usize,
    ) -> Result<Self, String> {
        let root = package
            .graph
            .definitions
            .get(root_uid)
            .ok_or_else(|| format!("unknown retained root {root_uid}"))?;
        let extent = definition_extent(package, root_uid)?;
        let available_width = viewport.width.max(1.0);
        let available_height = viewport.height.max(1.0);
        let scale = (available_width / extent[0])
            .min(available_height / extent[1])
            .min(1.0);
        let rect = RetainedRect {
            x: viewport.x + (available_width - extent[0] * scale) * 0.5,
            y: viewport.y + (available_height - extent[1] * scale) * 0.5,
            width: extent[0] * scale,
            height: extent[1] * scale,
        };
        let mut nodes = Vec::new();
        append_definition(
            package,
            root_uid,
            root_uid,
            rect,
            root_inputs,
            0,
            &mut nodes,
        )?;
        let _ = root;
        let mut scene = Self {
            root_uid: root_uid.into(),
            nodes,
        };
        scene.focus_interactive(focused_interactive);
        Ok(scene)
    }

    pub fn from_placements(
        package: &SandPackage,
        root_uid: &str,
        placements: Vec<RetainedPlacement>,
        focused_interactive: usize,
    ) -> Result<Self, String> {
        let mut nodes = Vec::new();
        let mut placement_keys = BTreeSet::new();
        for placement in placements {
            if placement.key.is_empty() {
                return Err("retained placement key cannot be empty".into());
            }
            if !placement_keys.insert(placement.key.clone()) {
                return Err(format!(
                    "duplicate retained placement key {}",
                    placement.key
                ));
            }
            append_definition(
                package,
                placement.definition_uid.as_str(),
                placement.key.as_str(),
                placement.rect,
                placement.inputs,
                0,
                &mut nodes,
            )?;
        }
        let mut node_keys = BTreeSet::new();
        if let Some(duplicate) = nodes
            .iter()
            .find_map(|node| (!node_keys.insert(node.key.as_str())).then_some(node.key.as_str()))
        {
            return Err(format!("duplicate retained node key {duplicate}"));
        }
        let mut scene = Self {
            root_uid: root_uid.into(),
            nodes,
        };
        scene.focus_interactive(focused_interactive);
        Ok(scene)
    }

    pub fn interactive_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.interactive).count()
    }

    pub fn focused(&self) -> Option<&RetainedNode> {
        self.nodes.iter().find(|node| node.focused)
    }

    pub fn hit_test(&self, point: [f64; 2]) -> Option<usize> {
        let key = self
            .nodes
            .iter()
            .rev()
            .find(|node| node.interactive && node.rect.contains(point))?
            .key
            .as_str();
        self.nodes
            .iter()
            .filter(|node| node.interactive)
            .position(|node| node.key == key)
    }

    pub fn focus_interactive(&mut self, focused_interactive: usize) {
        for node in &mut self.nodes {
            node.focused = false;
        }
        let interactive = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| node.interactive.then_some(index))
            .collect::<Vec<_>>();
        if let Some(index) =
            interactive.get(focused_interactive.min(interactive.len().saturating_sub(1)))
        {
            self.nodes[*index].focused = true;
        }
    }
}

fn append_definition(
    package: &SandPackage,
    definition_uid: &str,
    key: &str,
    rect: RetainedRect,
    inherited_inputs: BTreeMap<String, SandValue>,
    depth: usize,
    nodes: &mut Vec<RetainedNode>,
) -> Result<(), String> {
    let definition = package
        .graph
        .definitions
        .get(definition_uid)
        .ok_or_else(|| format!("missing retained definition {definition_uid}"))?;
    let mut inputs = definition
        .inputs
        .iter()
        .filter_map(|input| {
            input
                .default
                .clone()
                .map(|value| (input.name.clone(), value))
        })
        .collect::<BTreeMap<_, _>>();
    inputs.extend(inherited_inputs);
    let label = node_label(
        definition.display_name.as_str(),
        definition.element,
        &inputs,
    );
    nodes.push(RetainedNode {
        key: key.into(),
        definition_uid: definition_uid.into(),
        element: definition.element,
        role: definition.accessibility.role,
        label,
        description: definition.accessibility.description.clone(),
        rect,
        interactive: is_interactive(definition.element, definition.accessibility.role),
        focused: false,
        show_text: shows_text(definition.element),
        depth,
    });
    if definition.children.is_empty() {
        return Ok(());
    }
    let extent = definition_extent(package, definition_uid)?;
    let scale_x = rect.width / extent[0].max(1.0);
    let scale_y = rect.height / extent[1].max(1.0);
    let mut children = definition.children.iter().collect::<Vec<_>>();
    children.sort_by_key(|child| child.sibling_order);
    for child in children {
        let child_inputs = definition
            .exports
            .iter()
            .filter(|export| {
                export.direction == crate::sand::PortDirection::Input
                    && export.child_uid == child.local_uid
            })
            .filter_map(|export| {
                inputs
                    .get(&export.name)
                    .cloned()
                    .map(|value| (export.child_port.clone(), value))
            })
            .collect::<BTreeMap<_, _>>();
        let child_rect = RetainedRect {
            x: rect.x + child.transform.x as f32 * scale_x,
            y: rect.y + child.transform.y as f32 * scale_y,
            width: child.transform.width as f32 * scale_x,
            height: child.transform.height as f32 * scale_y,
        };
        append_definition(
            package,
            child.definition.uid.as_str(),
            format!("{key}/{}", child.local_uid).as_str(),
            child_rect,
            child_inputs,
            depth + 1,
            nodes,
        )?;
    }
    Ok(())
}

fn definition_extent(package: &SandPackage, uid: &str) -> Result<[f32; 2], String> {
    let definition = package
        .graph
        .definitions
        .get(uid)
        .ok_or_else(|| format!("missing retained definition {uid}"))?;
    let width = definition
        .children
        .iter()
        .map(|child| (child.transform.x + child.transform.width) as f32)
        .fold(0.0, f32::max)
        .max(80.0);
    let height = definition
        .children
        .iter()
        .map(|child| (child.transform.y + child.transform.height) as f32)
        .fold(0.0, f32::max)
        .max(28.0);
    Ok([width, height])
}

fn is_interactive(element: SandElement, role: AccessibilityRole) -> bool {
    matches!(
        element,
        SandElement::Icon
            | SandElement::Button
            | SandElement::Field
            | SandElement::Textarea
            | SandElement::Checkbox
            | SandElement::Radio
            | SandElement::Disclosure
            | SandElement::Select
            | SandElement::Tooltip
    ) || matches!(
        role,
        AccessibilityRole::Button
            | AccessibilityRole::TextInput
            | AccessibilityRole::MultilineTextInput
            | AccessibilityRole::Checkbox
            | AccessibilityRole::Radio
            | AccessibilityRole::Disclosure
            | AccessibilityRole::Select
    )
}

fn shows_text(element: SandElement) -> bool {
    !matches!(
        element,
        SandElement::Card
            | SandElement::Panel
            | SandElement::Stack
            | SandElement::Row
            | SandElement::List
    )
}

fn node_label(
    fallback: &str,
    element: SandElement,
    inputs: &BTreeMap<String, SandValue>,
) -> String {
    let preferred = match element {
        SandElement::Button | SandElement::Icon => ["label", "value", "title"].as_slice(),
        SandElement::Heading => ["value", "title", "label"].as_slice(),
        SandElement::Text | SandElement::Textarea | SandElement::Field => {
            ["value", "content", "description", "detail", "label"].as_slice()
        }
        SandElement::Quantity => ["value", "quantity"].as_slice(),
        SandElement::Badge => ["value", "state", "age", "send-timing"].as_slice(),
        SandElement::Checkbox | SandElement::Radio => ["value", "label"].as_slice(),
        _ => ["title", "label", "value"].as_slice(),
    };
    preferred
        .iter()
        .find_map(|name| inputs.get(*name))
        .map(value_label)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.into())
}

fn value_label(value: &SandValue) -> String {
    match value {
        SandValue::Text(value) | SandValue::Record(value) => value.clone(),
        SandValue::Number(value) => format!("{value:.2}"),
        SandValue::Boolean(value) => {
            if *value {
                "On".into()
            } else {
                "Off".into()
            }
        }
        SandValue::Json(value) => value.to_string(),
    }
}

pub struct RetainedTextLayer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffers: Vec<Buffer>,
    width: u32,
    height: u32,
    scale_factor: f64,
}

impl RetainedTextLayer {
    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> Self {
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);
        let mut font_system = FontSystem::new();
        font_system.db_mut().load_font_data(LATO_REGULAR.to_vec());
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            viewport,
            atlas,
            renderer,
            buffers: Vec::new(),
            width,
            height,
            scale_factor,
        }
    }

    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32, scale_factor: f64) {
        self.width = width;
        self.height = height;
        self.scale_factor = scale_factor;
        self.viewport.update(queue, Resolution { width, height });
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        scene: Option<&RetainedScene>,
        style: &ResolvedStyle,
    ) -> Result<(), String> {
        let visible = scene
            .into_iter()
            .flat_map(|scene| scene.nodes.iter())
            .filter(|node| node.show_text && node.rect.width >= 24.0 && node.rect.height >= 14.0)
            .collect::<Vec<_>>();
        let font_size = style
            .length_px("--lynx-text-size-body")
            .map_err(|error| error.to_string())?
            .clamp(10.0, 16.0);
        while self.buffers.len() < visible.len() {
            self.buffers.push(Buffer::new(
                &mut self.font_system,
                Metrics::new(font_size, font_size * 1.3),
            ));
        }
        for (buffer, node) in self.buffers.iter_mut().zip(&visible) {
            buffer.set_metrics(
                &mut self.font_system,
                Metrics::new(font_size, font_size * 1.3),
            );
            buffer.set_size(
                &mut self.font_system,
                Some((node.rect.width - 12.0).max(1.0)),
                Some((node.rect.height - 6.0).max(1.0)),
            );
            buffer.set_text(
                &mut self.font_system,
                node.label.as_str(),
                &Attrs::new().family(Family::Name("Lato")),
                Shaping::Advanced,
                None,
            );
            buffer.shape_until_scroll(&mut self.font_system, false);
        }
        let ink = style
            .color_srgba8("--lynx-ink-primary")
            .map_err(|error| error.to_string())?;
        let accent = style
            .color_srgba8("--lynx-focus")
            .map_err(|error| error.to_string())?;
        let areas = self
            .buffers
            .iter()
            .zip(visible)
            .map(|(buffer, node)| {
                let color = if node.focused { accent } else { ink };
                let left = node.rect.x + 6.0;
                let top = node.rect.y + 3.0;
                TextArea {
                    buffer,
                    left,
                    top,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: left as i32,
                        top: top as i32,
                        right: (node.rect.x + node.rect.width - 4.0) as i32,
                        bottom: (node.rect.y + node.rect.height - 2.0) as i32,
                    },
                    default_color: Color::rgba(color[0], color[1], color[2], color[3]),
                    custom_glyphs: &[],
                }
            })
            .collect::<Vec<_>>();
        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash_cache,
            )
            .map_err(|error| error.to_string())
    }

    pub fn render<'a>(&'a self, pass: &mut RenderPass<'a>) -> Result<(), String> {
        self.renderer
            .render(&self.atlas, &self.viewport, pass)
            .map_err(|error| error.to_string())
    }

    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::official_sands::official_sand_package;

    #[test]
    fn retained_scene_uses_recursive_exports_and_stable_paths() {
        let scene = RetainedScene::from_package(
            &official_sand_package(),
            "record",
            BTreeMap::from([
                ("source".into(), SandValue::Record("record-a".into())),
                ("title".into(), SandValue::Text("Visible title".into())),
                (
                    "description".into(),
                    SandValue::Text("Visible description".into()),
                ),
                ("quantity".into(), SandValue::Number(-4.0)),
                ("state".into(), SandValue::Text("Need".into())),
            ]),
            RetainedRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 800.0,
            },
            0,
        )
        .unwrap();
        assert!(
            scene.nodes.iter().any(|node| {
                node.key == "record/summary/title" && node.label == "Visible title"
            })
        );
        assert!(scene.nodes.iter().any(|node| {
            node.key == "record/summary/description" && node.label == "Visible description"
        }));
        assert!(scene.interactive_count() > 0);
        assert!(scene.focused().is_some());
    }

    #[test]
    fn retained_placements_repeat_one_definition_with_stable_instance_paths() {
        let inputs = |uid: &str, title: &str| {
            BTreeMap::from([
                ("record".into(), SandValue::Record(uid.into())),
                ("title".into(), SandValue::Text(title.into())),
            ])
        };
        let scene = RetainedScene::from_placements(
            &official_sand_package(),
            "table",
            vec![
                RetainedPlacement {
                    key: "table/rows/record-61".into(),
                    definition_uid: "official-table-row".into(),
                    inputs: inputs("a", "First"),
                    rect: RetainedRect {
                        x: 0.0,
                        y: 0.0,
                        width: 406.0,
                        height: 26.0,
                    },
                },
                RetainedPlacement {
                    key: "table/rows/record-62".into(),
                    definition_uid: "official-table-row".into(),
                    inputs: inputs("b", "Second"),
                    rect: RetainedRect {
                        x: 0.0,
                        y: 30.0,
                        width: 406.0,
                        height: 26.0,
                    },
                },
            ],
            1,
        )
        .unwrap();
        assert!(
            scene
                .nodes
                .iter()
                .any(|node| { node.key == "table/rows/record-61/title" && node.label == "First" })
        );
        assert!(
            scene
                .nodes
                .iter()
                .any(|node| { node.key == "table/rows/record-62/title" && node.label == "Second" })
        );
        assert_eq!(scene.interactive_count(), 2);
        assert!(
            scene
                .focused()
                .is_some_and(|node| { node.key == "table/rows/record-62/open/control" })
        );
    }

    #[test]
    fn retained_placements_reject_duplicate_instance_paths() {
        let placement = RetainedPlacement {
            key: "table/rows/record-61".into(),
            definition_uid: "official-table-row".into(),
            inputs: BTreeMap::new(),
            rect: RetainedRect {
                x: 0.0,
                y: 0.0,
                width: 406.0,
                height: 26.0,
            },
        };
        let error = RetainedScene::from_placements(
            &official_sand_package(),
            "table",
            vec![placement.clone(), placement],
            0,
        )
        .unwrap_err();
        assert_eq!(
            error,
            "duplicate retained placement key table/rows/record-61"
        );
    }
}
