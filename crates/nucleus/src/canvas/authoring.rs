use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum TokenValue {
    Color([u8; 4]),
    Number(f32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "unit",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Measure {
    Auto,
    Px(f32),
    Percent(f32),
    Vw(f32),
    Vh(f32),
    Vmin(f32),
    Vmax(f32),
}

impl Measure {
    pub fn validate(&self) -> bool {
        match self {
            Self::Auto => true,
            Self::Px(value)
            | Self::Percent(value)
            | Self::Vw(value)
            | Self::Vh(value)
            | Self::Vmin(value)
            | Self::Vmax(value) => value.is_finite() && value.abs() <= 100_000.0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Style {
    #[serde(default)]
    pub dimensions: BTreeMap<String, Measure>,
    #[serde(default)]
    pub numbers: BTreeMap<String, f32>,
    #[serde(default)]
    pub choices: BTreeMap<String, String>,
}

impl Style {
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
                || !value.is_finite()
                || !(0.0..=10_000.0).contains(value)
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartEdit {
    pub parent: Option<String>,
    pub order: Option<usize>,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub tokens: BTreeMap<String, TokenValue>,
    #[serde(default)]
    pub token_uses: BTreeMap<String, String>,
    #[serde(default)]
    pub settings: BTreeMap<String, serde_json::Value>,
    pub text: Option<String>,
    pub native_actions_enabled: Option<bool>,
    pub geometry: Option<Geometry>,
    pub layout: Option<serde_json::Value>,
    pub layout_parent: Option<String>,
    #[serde(default)]
    pub layout_detached: bool,
    #[serde(default)]
    pub removed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddedPart {
    pub parent: String,
    pub component: Option<Component>,
    pub size: [f64; 2],
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartDefaults {
    pub style: Style,
    pub tokens: BTreeMap<String, Option<TokenValue>>,
    pub token_uses: BTreeMap<String, Option<String>>,
    pub parent: Option<String>,
    pub order: usize,
    pub text: Option<String>,
    pub geometry: Option<Geometry>,
    pub aspect_ratio: Option<Measure>,
    pub layout: Option<serde_json::Value>,
    pub layout_parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Behavior {
    pub part: String,
    pub event: String,
    pub action: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub event: String,
    pub receive: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edits {
    pub size: Option<[f64; 2]>,
    #[serde(default)]
    pub defaults: BTreeMap<String, PartDefaults>,
    #[serde(default)]
    pub parts: BTreeMap<String, PartEdit>,
    #[serde(default)]
    pub added: BTreeMap<String, AddedPart>,
    #[serde(default)]
    pub behaviors: BTreeMap<String, Behavior>,
    #[serde(default)]
    pub links: BTreeMap<String, Link>,
    #[serde(default)]
    pub groups: BTreeMap<String, Vec<String>>,
}

pub fn part_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        Err("Use a nonempty stable part identity of at most 512 bytes.".into())
    } else {
        Ok(())
    }
}

impl Edits {
    pub fn validate(&self, _registry: &[Descriptor]) -> Result<(), String> {
        if let Some(size) = self.size {
            Geometry {
                position: [0.0; 2],
                size,
            }
            .validate()?;
        }
        if self.defaults.len() > 512 {
            return Err("Factory defaults exceed 512 edited parts.".into());
        }
        for (id, defaults) in &self.defaults {
            part_id(id)?;
            defaults.style.validate()?;
            if let Some(geometry) = &defaults.geometry {
                geometry.validate()?;
            }
            if defaults.tokens.len() > 64
                || defaults.token_uses.len() > 16
                || defaults.order > 2048
                || defaults
                    .text
                    .as_ref()
                    .is_some_and(|text| text.len() > 65536)
            {
                return Err("Invalid authored defaults.".into());
            }
        }
        if self.parts.len() + self.added.len() > 512
            || self.behaviors.len() > 128
            || self.links.len() > 128
        {
            return Err(
                "An editing layer supports 512 part entries and 128 behaviors or links.".into(),
            );
        }
        for (id, edit) in &self.parts {
            part_id(id)?;
            edit.style.validate()?;
            if let Some(geometry) = &edit.geometry {
                geometry.validate()?;
            }
            if let Some(parent) = &edit.parent {
                part_id(parent)?;
            }
            if let Some(parent) = &edit.layout_parent {
                part_id(parent)?;
                if parent == id || edit.layout_detached {
                    return Err(
                        "Layout attachment needs a distinct parent and cannot also detach.".into(),
                    );
                }
            }
            if edit.order.is_some_and(|order| order > 512)
                || edit.tokens.len() > 64
                || edit.token_uses.len() > 16
            {
                return Err("Part edits exceed their bounds.".into());
            }
            if edit.settings.len() > 64 || edit.text.as_ref().is_some_and(|text| text.len() > 65536)
            {
                return Err("Part text or settings exceed their bounds.".into());
            }
            for (token, value) in &edit.tokens {
                part_id(token)?;
                if matches!(value, TokenValue::Number(value) if !value.is_finite()) {
                    return Err("Token values must be finite.".into());
                }
            }
            for (usage, token) in &edit.token_uses {
                if !matches!(
                    usage.as_str(),
                    "background"
                        | "border"
                        | "text"
                        | "outline"
                        | "cursor"
                        | "font-size"
                        | "spacing"
                        | "padding"
                        | "icon-size"
                        | "icon-padding"
                ) {
                    return Err("Unsupported token use.".into());
                }
                part_id(token)?;
            }
        }
        for (id, part) in &self.added {
            if !id.starts_with("user/") {
                return Err("New part identities start with user/.".into());
            }
            part_id(id)?;
            part_id(&part.parent)?;
            Geometry {
                position: [0.0; 2],
                size: part.size,
            }
            .validate()?;
        }
        for id in self.parts.keys().chain(self.added.keys()) {
            let mut seen = BTreeSet::from([id.as_str()]);
            let mut parent = self
                .parts
                .get(id)
                .and_then(|edit| edit.parent.as_deref())
                .or_else(|| self.added.get(id).map(|part| part.parent.as_str()));
            while let Some(current) = parent {
                if !seen.insert(current) || seen.len() > 32 {
                    return Err("Part ownership cannot contain cycles or exceed 32 levels.".into());
                }
                parent = self
                    .parts
                    .get(current)
                    .and_then(|edit| edit.parent.as_deref())
                    .or_else(|| self.added.get(current).map(|part| part.parent.as_str()));
            }
        }
        for (id, behavior) in &self.behaviors {
            part_id(id)?;
            part_id(&behavior.part)?;
            event_name(&behavior.event)?;
            if !behavior.action.is_object() || !behavior.action["action"].is_string() {
                return Err("A behavior uses an ordinary registered Lince Action.".into());
            }
        }
        for (id, link) in &self.links {
            part_id(id)?;
            part_id(&link.from)?;
            part_id(&link.to)?;
            event_name(&link.event)?;
            event_name(&link.receive)?;
        }
        if self.groups.len() > 64 {
            return Err("An item supports at most 64 transform groups.".into());
        }
        let mut members = BTreeSet::new();
        for (id, group) in &self.groups {
            if id.len() != 32
                || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
                || group.is_empty()
                || group.len() > 256
            {
                return Err(
                    "A transform group has a hexadecimal identity and 1–256 members.".into(),
                );
            }
            for member in group {
                part_id(member)?;
                if !members.insert(member) {
                    return Err("A part belongs to at most one authored transform group.".into());
                }
            }
        }
        bounded(self)
    }

    pub fn apply(&mut self, change: &Change) -> Result<(), String> {
        match change {
            Change::SetPart { part, edit } => {
                self.parts.insert(part.clone(), edit.clone());
            }
            Change::ResetPart { part } => {
                self.parts.remove(part);
            }
            Change::InsertPart { part, definition } => {
                if self.added.contains_key(part) {
                    return Err("This authored part already exists.".into());
                }
                self.added.insert(part.clone(), definition.clone());
            }
            Change::ConfigurePart { part, component } => {
                self.added.get_mut(part).ok_or("Configure an authored part's factory; use its registered settings for renderer-owned parts.")?.component = component.clone();
            }
            Change::RemovePart { part, cascade } => {
                if part == "root" {
                    return Err("Remove the placement to delete its root.".into());
                }
                let mut removed = BTreeSet::from([part.clone()]);
                loop {
                    let count = removed.len();
                    for (id, definition) in &self.added {
                        let parent = self
                            .parts
                            .get(id)
                            .and_then(|edit| edit.parent.as_ref())
                            .unwrap_or(&definition.parent);
                        if removed.contains(parent) {
                            removed.insert(id.clone());
                        }
                    }
                    if count == removed.len() {
                        break;
                    }
                }
                let references = self
                    .links
                    .values()
                    .any(|link| removed.contains(&link.from) || removed.contains(&link.to))
                    || self
                        .behaviors
                        .values()
                        .any(|behavior| removed.contains(&behavior.part))
                    || self
                        .groups
                        .values()
                        .any(|group| group.iter().any(|member| removed.contains(member)))
                    || self.parts.iter().any(|(id, edit)| {
                        !removed.contains(id)
                            && edit
                                .parent
                                .as_ref()
                                .is_some_and(|parent| removed.contains(parent))
                    });
                if !cascade && (removed.len() > 1 || references) {
                    return Err("This part has children or incoming references; use an explicit cascade or detach them first.".into());
                }
                if references && *cascade {
                    self.links.retain(|_, link| {
                        !removed.contains(&link.from) && !removed.contains(&link.to)
                    });
                    self.behaviors
                        .retain(|_, behavior| !removed.contains(&behavior.part));
                    for group in self.groups.values_mut() {
                        group.retain(|member| !removed.contains(member));
                    }
                    self.groups.retain(|_, members| !members.is_empty());
                    for edit in self.parts.values_mut() {
                        if edit
                            .parent
                            .as_ref()
                            .is_some_and(|parent| removed.contains(parent))
                        {
                            edit.parent = None;
                        }
                    }
                }
                for id in removed {
                    self.parts.remove(&id);
                    if self.added.remove(&id).is_none() {
                        self.parts.entry(id).or_default().removed = true;
                    } else {
                        self.defaults
                            .retain(|part, _| part != &id && !part.starts_with(&format!("{id}/")));
                    }
                }
            }
            Change::SetBehavior { id, behavior } => {
                self.behaviors.insert(id.clone(), behavior.clone());
            }
            Change::RemoveBehavior { id } => {
                if self.behaviors.remove(id).is_none() {
                    return Err("This behavior does not exist.".into());
                }
            }
            Change::SetLink { id, link } => {
                self.links.insert(id.clone(), link.clone());
            }
            Change::RemoveLink { id } => {
                if self.links.remove(id).is_none() {
                    return Err("This link does not exist.".into());
                }
            }
            Change::SetGroup { id, members } => {
                self.groups.insert(id.clone(), members.clone());
            }
            Change::RemoveGroup { id } => {
                if self.groups.remove(id).is_none() {
                    return Err("This transform group does not exist.".into());
                }
            }
        }
        Ok(())
    }
}

fn event_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        Err("Use an event name of 1–128 bytes.".into())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "change", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    SetPart {
        part: String,
        edit: PartEdit,
    },
    ResetPart {
        part: String,
    },
    InsertPart {
        part: String,
        definition: AddedPart,
    },
    ConfigurePart {
        part: String,
        component: Option<Component>,
    },
    RemovePart {
        part: String,
        cascade: bool,
    },
    SetBehavior {
        id: String,
        behavior: Behavior,
    },
    RemoveBehavior {
        id: String,
    },
    SetLink {
        id: String,
        link: Link,
    },
    RemoveLink {
        id: String,
    },
    SetGroup {
        id: String,
        members: Vec<String>,
    },
    RemoveGroup {
        id: String,
    },
}

impl Change {
    pub fn validate(&self) -> Result<(), String> {
        let id = match self {
            Self::SetPart { part, .. }
            | Self::ResetPart { part }
            | Self::InsertPart { part, .. }
            | Self::ConfigurePart { part, .. }
            | Self::RemovePart { part, .. } => part,
            Self::SetBehavior { id, .. }
            | Self::RemoveBehavior { id }
            | Self::SetLink { id, .. }
            | Self::RemoveLink { id }
            | Self::SetGroup { id, .. }
            | Self::RemoveGroup { id } => id,
        };
        part_id(id)?;
        bounded(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TokenInfo {
    pub token: String,
    pub name: String,
    pub range: Option<[f32; 2]>,
    pub authored: Option<TokenValue>,
    pub effective: TokenValue,
    pub source: String,
    pub source_part: Option<String>,
    pub uses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PartInfo {
    pub id: String,
    pub parent: Option<String>,
    pub name: String,
    pub role: String,
    pub style: Style,
    pub measured_size: [f32; 2],
    pub geometry: Option<Geometry>,
    pub group: Option<String>,
    pub layout: Option<serde_json::Value>,
    pub layout_parent: Option<String>,
    pub tokens: Vec<TokenInfo>,
    pub text: Option<String>,
    pub settings: serde_json::Value,
    pub removed: bool,
    pub structural_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Element {
    pub revision: u64,
    pub placement: Placement,
    pub parts: Vec<PartInfo>,
    pub total_parts: usize,
    pub next_offset: Option<usize>,
    pub capabilities: Vec<String>,
}

pub fn base(component: &Component) -> &Component {
    match component {
        Component::Authored { base, .. } => base,
        _ => component,
    }
}

pub fn edits(component: &Component) -> Edits {
    match component {
        Component::Authored { edits, .. } => edits.clone(),
        _ => Edits::default(),
    }
}

pub fn rebind(
    component: &Component,
    records: &BTreeMap<String, String>,
) -> Result<Component, String> {
    if records
        .keys()
        .any(|record| !component.records().contains(&record.as_str()))
    {
        return Err("Rebinding keys must be declared Record bindings in this item.".into());
    }
    fn replace(
        component: &mut Component,
        records: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        match component {
            Component::Builtin { state } => {
                for binding in state.records_mut() {
                    if let Some(record) = records.get(binding) {
                        *binding = record.clone();
                    }
                }
            }
            Component::Native {
                bindings, settings, ..
            } => {
                if bindings.iter().any(|binding| records.contains_key(binding))
                    && settings.values().any(|value| {
                        value.as_str().is_some_and(|value| {
                            records.keys().any(|record| value.contains(record))
                        })
                    })
                {
                    return Err("This native factory embeds bindings in typed settings. Configure its state and bindings together instead of rewriting arbitrary text.".into());
                }
                for binding in bindings {
                    if let Some(record) = records.get(binding) {
                        *binding = record.clone();
                    }
                }
            }
            Component::Composition { composition } => {
                for part in &mut composition.parts {
                    replace(&mut part.component, records)?;
                }
            }
            Component::Authored { base, edits } => {
                replace(base, records)?;
                for part in edits.added.values_mut() {
                    if let Some(component) = &mut part.component {
                        replace(component, records)?;
                    }
                }
            }
        }
        Ok(())
    }
    let mut component = component.clone();
    replace(&mut component, records)?;
    Ok(component)
}

pub fn hash(value: &impl Serialize) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    fn canonical(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(values) => {
                let sorted: BTreeMap<_, _> = values
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect();
                serde_json::Value::Object(sorted.into_iter().collect())
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(canonical).collect())
            }
            value => value,
        }
    }
    let value = canonical(serde_json::to_value(value).map_err(|error| error.to_string())?);
    Ok(
        Sha256::digest(serde_json::to_vec(&value).map_err(|error| error.to_string())?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}
