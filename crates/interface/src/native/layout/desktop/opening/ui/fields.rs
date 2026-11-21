// non-lean-yet
// Scope: struct Form, fn dimension, fn parse_dimension, fn workspace_name.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 130, 141, 166.
// - [review: §3 Ownership] Check that these inputs can be borrowed without changing ownership
//   semantics. Sites selector:selected → &str (54).
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 27.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   162, 198, 203.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (57, 62, 67, 72, 74, 80, 86, 136, 140).

use super::*;
use crate::native::{actions::ActionSequence, dropdown::{self, Dropdown}};

pub(super) struct Form {
    pub workspace: Option<u64>,
    pub output: Option<String>,
    pub flags: [Option<bool>; 3],
    workspace_control: Entity,
    output_control: Entity,
    dimensions: [Entity; 2],
    flag_controls: [(Entity, Entity); 3],
    presentation: Presentation,
}

fn dimension(value: Option<Dimension>) -> String {
    match value {
        None => String::new(),
        Some(Dimension::Fraction([numerator, denominator])) => format!("{numerator}/{denominator}"),
        Some(Dimension::Pixels(value)) => format!("{value}px"),
    }
}

fn parse_dimension(value: &str) -> Result<Option<Dimension>, String> {
    let value = value.trim();
    if value.is_empty() { return Ok(None); }
    let dimension = if let Some(pixels) = value.strip_suffix("px") {
        Dimension::Pixels(pixels.trim().parse::<u32>().map_err(|_| "Use whole logical pixels, such as 800px, or a fraction such as 1/2.".to_owned())?)
    } else {
        Dimension::Fraction(shortcuts::size_ratio(value)?)
    };
    if !dimension.valid() { return Err("Use positive logical pixels up to 100000 or a fraction no greater than one.".into()); }
    Ok(Some(dimension))
}

fn workspace_name(world: &World, root: Entity, workspace: Option<u64>) -> String {
    match workspace {
        None => "Inherit workspace".into(),
        Some(id) => world.get::<Workspaces>(root).and_then(|spaces| spaces.entries.iter().find(|entry| entry.id == id))
            .map(|entry| format!("{} · {id}", entry.name)).unwrap_or_else(|| format!("Unavailable workspace · {id}")),
    }
}

fn output_name(world: &World, output: Option<&str>) -> String {
    match output {
        None => "Inherit output".into(),
        Some(name) => {
            let available = world.get_resource::<crate::host::display::DisplaySnapshot>()
                .is_some_and(|snapshot| snapshot.heads.iter().any(|head| head.name == name));
            if available { name.into() } else { format!("Unavailable output · {name}") }
        }
    }
}

fn selector(world: &mut World, parent: Entity, owner: Entity, name: &str, selected: String, choices: Vec<(String, ActionSequence)>) -> Entity {
    let toggle = dropdown::spawn(world, parent, owner, name, &selected, choices);
    if let Some(group) = world.get::<ChildOf>(toggle).map(ChildOf::parent) {
        let mut node = world.get_mut::<Node>(group).unwrap();
        node.width = percent(100);
        node.min_width = px(0);
    }
    {
        let mut node = world.get_mut::<Node>(toggle).unwrap();
        node.width = percent(100);
        node.min_width = px(0);
    }
    if let Some(caption) = world.get::<Children>(toggle).and_then(|children| children.first()).copied() {
        let mut node = world.get_mut::<Node>(caption).unwrap();
        node.min_width = px(0);
        node.flex_shrink = 1.0;
        world.entity_mut(caption).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    }
    let menu = world.get::<Dropdown>(toggle).unwrap().menu;
    world.entity_mut(menu).insert((ScrollPosition::default(), crate::native::edit_mode::FocusScroll));
    let mut node = world.get_mut::<Node>(menu).unwrap();
    node.width = percent(100);
    node.max_height = px(200);
    node.overflow = Overflow::scroll_y();
    let choices: Vec<_> = world.get::<Children>(menu).map(|children| children.iter().collect()).unwrap_or_default();
    for choice in choices {
        let mut node = world.get_mut::<Node>(choice).unwrap();
        node.width = percent(100);
        node.min_width = px(0);
        let captions: Vec<_> = world.get::<Children>(choice).map(|children| children.iter().collect()).unwrap_or_default();
        for entity in captions {
            if world.get::<Text>(entity).is_some() {
                world.get_mut::<Node>(entity).unwrap().min_width = px(0);
                world.entity_mut(entity).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
            }
        }
    }
    toggle
}

fn selected(world: &mut World, toggle: Entity, value: &str) {
    if let Some(entity) = world
        .get::<Children>(toggle)
        .and_then(|children| children.first())
        .copied()
    {
        text(world, entity, value);
    }
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(toggle)
        && node.value() != Some(value)
    {
        node.set_value(value);
        engine::performance::add("native-settings-label-writes", 1);
    }
}

fn close_menu(world: &mut World, toggle: Entity) {
    if let Some(menu) = world.get::<Dropdown>(toggle).map(|dropdown| dropdown.menu)
        && let Some(mut node) = world.get_mut::<Node>(menu) { node.display = Display::None; }
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(toggle) { node.set_expanded(false); }
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(toggle, FocusCause::Navigated); }
}

impl Form {
    pub(super) fn spawn(world: &mut World, parent: Entity, owner: Entity, root: Entity, properties: &Properties) -> Self {
        wrapped(world, parent, "Initial preferences", 18.0);
        wrapped(world, parent, "Blank sizes and Inherit leave the next preference source in control. Use 1/2, 50% or 800px. Focus requests still need normal compositor authorization.", 14.0);
        let mut workspaces = vec![("Inherit workspace".into(), crate::native::actions![EditChange::Workspace(None)])];
        if let Some(spaces) = world.get::<Workspaces>(root) {
            workspaces.extend(spaces.entries.iter().filter(|entry| !crate::native::laboratory::temporary(world, root, entry.id))
                .map(|entry| (format!("{} · {}", entry.name, entry.id), crate::native::actions![EditChange::Workspace(Some(entry.id))])));
        }
        let selected_workspace = workspace_name(world, root, properties.workspace);
        let workspace_control = selector(world, parent, owner, "Initial workspace", selected_workspace, workspaces);
        let mut outputs = vec![("Inherit output".into(), crate::native::actions![OutputChoice(None)])];
        if let Some(snapshot) = world.get_resource::<crate::host::display::DisplaySnapshot>() {
            outputs.extend(snapshot.heads.iter().map(|head| (head.name.clone(), crate::native::actions![OutputChoice(Some(head.name.clone()))])));
        }
        let selected_output = output_name(world, properties.output.as_deref());
        let output_control = selector(world, parent, owner, "Initial output", selected_output, outputs);
        let dimensions = ["Initial width (blank inherits)", "Initial height (blank inherits)"].map(|caption| field(world, parent, caption, 32));
        for (entity, value) in dimensions.into_iter().zip([properties.width, properties.height]) {
            world.get_mut::<EditableText>(entity).unwrap().editor.set_text(&dimension(value));
        }
        let flags = [properties.floating, properties.maximized, properties.focus];
        let flag_controls = ["Floating", "Maximized", "Initial focus"].into_iter().enumerate()
            .map(|(index, name)| button(world, parent, owner, name, EditChange::Flag(index))).collect::<Vec<_>>().try_into().unwrap();
        let mut form = Self { workspace: properties.workspace, output: properties.output.clone(), flags, workspace_control, output_control, dimensions, flag_controls, presentation: Default::default() };
        form.refresh(world, root);
        form
    }

    pub(super) fn inputs(&self) -> [Entity; 2] {
        self.dimensions
    }

    pub(super) fn properties(&self, world: &World) -> Result<Properties, String> {
        let mut sizes = [None, None];
        for (index, entity) in self.dimensions.into_iter().enumerate() {
            let input = world
                .get::<EditableText>(entity)
                .ok_or("The opening-preference field is unavailable.")?;
            if input.is_composing()
                || !input.pending_edits.is_empty()
                || input.pending_paste.is_some()
            {
                return Err("Finish text composition or the pending paste before saving.".into());
            }
            sizes[index] = parse_dimension(&input.value().to_cow())?;
        }
        let properties = Properties {
            workspace: self.workspace,
            output: self.output.clone(),
            width: sizes[0],
            height: sizes[1],
            floating: self.flags[0],
            maximized: self.flags[1],
            focus: self.flags[2],
        };
        properties.validate()?;
        Ok(properties)
    }

    pub(super) fn refresh(&mut self, world: &mut World, root: Entity) {
        self.presentation
            .prepare(world, root, self.workspace, self.output.as_deref());
        selected(
            world,
            self.workspace_control,
            &self.presentation.workspace_text,
        );
        selected(world, self.output_control, &self.presentation.output_text);
        for (index, captions) in [
            ["Floating: Inherit", "Floating: Yes", "Floating: No"],
            ["Maximized: Inherit", "Maximized: Yes", "Maximized: No"],
            [
                "Initial focus: Inherit",
                "Initial focus: Yes",
                "Initial focus: No",
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let selected = match self.flags[index] {
                None => 0,
                Some(true) => 1,
                Some(false) => 2,
            };
            caption(world, self.flag_controls[index], captions[selected]);
        }
    }

    pub(super) fn close_workspace(&self, world: &mut World) { close_menu(world, self.workspace_control); }
    pub(super) fn close_output(&self, world: &mut World) { close_menu(world, self.output_control); }
}

pub(super) fn summary(properties: &Properties) -> String {
    let mut values = Vec::new();
    if let Some(workspace) = properties.workspace { values.push(format!("workspace {workspace}")); }
    if let Some(output) = &properties.output { values.push(format!("output {output}")); }
    if properties.width.is_some() { values.push(format!("width {}", dimension(properties.width))); }
    if properties.height.is_some() { values.push(format!("height {}", dimension(properties.height))); }
    for (name, flag) in [("floating", properties.floating), ("maximized", properties.maximized), ("initial focus", properties.focus)] {
        if let Some(flag) = flag { values.push(format!("{name} {}", if flag { "yes" } else { "no" })); }
    }
    if values.is_empty() { "inherit normal/retained preferences".into() } else { values.join(" · ") }
}

#[derive(Default)]
struct Presentation {
    ready: bool,
    workspace: Option<u64>,
    workspace_name: Option<String>,
    workspace_text: String,
    output: Option<String>,
    output_available: bool,
    output_text: String,
}

impl Presentation {
    fn prepare(
        &mut self,
        world: &World,
        root: Entity,
        workspace: Option<u64>,
        output: Option<&str>,
    ) {
        let name = workspace
            .and_then(|id| {
                world
                    .get::<Workspaces>(root)
                    .and_then(|spaces| spaces.entries.iter().find(|entry| entry.id == id))
            })
            .map(|entry| entry.name.as_str());
        if !self.ready || self.workspace != workspace || self.workspace_name.as_deref() != name {
            self.workspace = workspace;
            self.workspace_name = name.map(str::to_owned);
            self.workspace_text = workspace_name(world, root, workspace);
            engine::performance::add("opening-editor-form-labels-prepared", 1);
        }
        let available = output.is_some_and(|name| {
            world
                .get_resource::<crate::host::display::DisplaySnapshot>()
                .is_some_and(|snapshot| snapshot.heads.iter().any(|head| head.name == name))
        });
        if !self.ready || self.output.as_deref() != output || self.output_available != available {
            self.output = output.map(str::to_owned);
            self.output_available = available;
            self.output_text = output_name(world, output);
            engine::performance::add("opening-editor-form-labels-prepared", 1);
        }
        self.ready = true;
    }
}
