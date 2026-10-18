use super::*;
use smithay::reexports::wayland_server::{DataInit, Dispatch, GlobalDispatch, New, WEnum, protocol::wl_output};
use wayland_protocols_wlr::output_management::v1::server::{
    zwlr_output_manager_v1::{self as manager, ZwlrOutputManagerV1 as Manager},
    zwlr_output_head_v1::{self as head, ZwlrOutputHeadV1 as Head},
    zwlr_output_mode_v1::ZwlrOutputModeV1 as ModeObject,
    zwlr_output_configuration_v1::{self as configuration, ZwlrOutputConfigurationV1 as Configuration},
    zwlr_output_configuration_head_v1::{self as settings, ZwlrOutputConfigurationHeadV1 as Settings},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputMode { pub size: [i32; 2], pub refresh_mhz: i32, pub preferred: bool }

#[derive(Clone, Debug, PartialEq)]
pub struct OutputHead {
    pub id: OutputId,
    pub name: String,
    pub modes: Vec<OutputMode>,
    pub current: Option<usize>,
    pub position: [i32; 2],
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutputSettings { pub mode: OutputMode, pub position: [i32; 2], pub scale: f64 }

#[derive(Clone, Debug)]
pub struct OutputRequest {
    pub token: u64,
    pub serial: u32,
    pub test: bool,
    pub heads: BTreeMap<OutputId, Option<OutputSettings>>,
}

struct Tracked { object: Head, snapshot: OutputHead, modes: Vec<(ModeObject, OutputMode)> }
struct Binding { manager: Manager, heads: BTreeMap<OutputId, Tracked>, suppressed: BTreeSet<OutputId>, stopped: bool, initialized: bool }
struct Draft { settings: OutputSettings, mask: u8, valid: bool }
struct Config {
    object: Configuration, serial: u32,
    heads: BTreeMap<OutputId, Option<Draft>>, used: bool, token: Option<u64>,
    children: Vec<Settings>,
}
pub(super) struct ModeData { id: OutputId, mode: OutputMode }
pub(super) struct SettingsData { config: Configuration, id: OutputId }

pub(super) struct Control {
    catalog: Vec<OutputHead>, serial: u32, bindings: Vec<Binding>, configs: Vec<Config>,
    requests: VecDeque<OutputRequest>, next_token: u64,
}

fn permitted(client: &Client) -> bool {
    client.get_data::<ClientState>().is_some_and(|data| data.capabilities.contains(&crate::launch::Capability::OutputControl))
}

pub(crate) fn validate_catalog(catalog: &[OutputHead]) -> io::Result<()> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    if catalog.len() > 16 || catalog.iter().any(|head| !ids.insert(head.id) || !names.insert(&head.name)
        || head.name.is_empty() || head.name.len() > 512 || !head.name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || head.modes.is_empty() || head.modes.len() > 128 || head.current.is_some_and(|index| index >= head.modes.len())
        || !head.scale.is_finite() || !(0.5..=4.0).contains(&head.scale)
        || head.modes.iter().any(|mode| validate_size(mode.size).is_err() || mode.refresh_mhz < 0)) {
        return Err(io::Error::other("Invalid output-control catalog"));
    }
    Ok(())
}

fn valid_heads(heads: &BTreeMap<OutputId, Option<OutputSettings>>) -> bool {
    let mut pixels = 0_u64;
    let mut logical_pixels = 0_u64;
    let mut enabled = 0;
    for settings in heads.values().flatten() {
        if validate_size(settings.mode.size).is_err() || settings.mode.refresh_mhz < 0 || !settings.scale.is_finite()
            || !(0.5..=4.0).contains(&settings.scale) || (settings.scale * 120.0 - (settings.scale * 120.0).round()).abs() > 0.000001
            || settings.position.iter().any(|position| *position < 0) { return false; }
        let logical = settings.mode.size.map(|size| (f64::from(size) / settings.scale).round() as i32);
        if validate_size(logical).is_err() || settings.position.iter().zip(logical).any(|(position, size)| position.checked_add(size).is_none_or(|extent| extent > 16384)) { return false; }
        pixels += settings.mode.size[0] as u64 * settings.mode.size[1] as u64;
        logical_pixels += logical[0] as u64 * logical[1] as u64;
        enabled += 1;
    }
    enabled > 0 && enabled <= 16 && pixels <= 64 * 1024 * 1024 && logical_pixels <= 64 * 1024 * 1024
}

impl Control {
    pub(super) fn new(display: &DisplayHandle) -> Self {
        display.create_global::<State, Manager, _>(3, ());
        Self { catalog: Vec::new(), serial: smithay::utils::SERIAL_COUNTER.next_serial().into(), bindings: Vec::new(),
            configs: Vec::new(), requests: VecDeque::new(), next_token: 0 }
    }

    fn head_id(&self, resource: &Head) -> Option<OutputId> {
        self.bindings.iter().flat_map(|binding| binding.heads.iter())
            .find_map(|(id, tracked)| (&tracked.object == resource && resource.is_alive()).then_some(*id))
    }
}

impl Server {
    pub fn registered_output_catalog(&mut self) -> io::Result<()> {
        let catalog = self.state.outputs.iter().filter_map(|(id, registered)| {
            let output = &registered.output;
            let mode = output.current_mode()?;
            let position = output.current_location();
            Some(OutputHead { id: *id, name: output.name(), modes: vec![OutputMode { size: [mode.size.w, mode.size.h], refresh_mhz: mode.refresh, preferred: true }],
                current: Some(0), position: [position.x, position.y], scale: output.current_scale().fractional_scale() })
        }).collect();
        self.output_catalog(catalog)
    }

    pub fn apply_virtual_output_configuration(&mut self, request: &OutputRequest, bounds: [u32; 2]) -> io::Result<bool> {
        if !self.output_configuration_current(request) || !valid_heads(&request.heads) { return Ok(false); }
        let mut catalog = self.state.output_control.catalog.clone();
        if request.heads.len() != catalog.len() { return Ok(false); }
        let mut monitors = Vec::new();
        let mut scales = BTreeMap::new();
        for head in &mut catalog {
            let Some(settings) = request.heads.get(&head.id) else { return Ok(false) };
            if let Some(settings) = settings {
                let Some(index) = head.modes.iter().position(|mode| mode == &settings.mode) else { return Ok(false) };
                if settings.position.iter().zip(settings.mode.size).zip(bounds).any(|((position, size), bound)|
                    f64::from(*position) + (f64::from(size) / settings.scale).round() > f64::from(bound)) { return Ok(false); }
                head.current = Some(index);
                head.position = settings.position;
                head.scale = settings.scale;
                monitors.push(Monitor { id: head.id, name: head.name.clone(), origin: settings.position, size: settings.mode.size });
                scales.insert(head.id, settings.scale);
            } else { head.current = None; }
        }
        if request.test { return Ok(true); }
        self.configure_scaled_outputs(&monitors, &scales)?;
        self.output_catalog(catalog)?;
        Ok(true)
    }

    pub fn output_catalog(&mut self, catalog: Vec<OutputHead>) -> io::Result<()> {
        validate_catalog(&catalog)?;
        if self.state.output_control.catalog != catalog {
            self.state.output_control.catalog = catalog;
            self.state.output_control.serial = smithay::utils::SERIAL_COUNTER.next_serial().into();
            self.state.sync_output_control();
        }
        Ok(())
    }

    pub fn output_requests(&mut self) -> Vec<OutputRequest> {
        let requests: Vec<_> = self.state.output_control.requests.drain(..).collect();
        requests.into_iter().filter(|request| self.state.output_control.configs.iter().any(|config| config.token == Some(request.token) && config.object.is_alive())).collect()
    }

    pub fn output_configuration_current(&self, request: &OutputRequest) -> bool {
        !self.locked() && request.serial == self.state.output_control.serial
            && self.state.output_control.configs.iter().any(|config| config.token == Some(request.token) && config.object.is_alive())
    }

    pub fn output_configuration_finished(&mut self, token: u64, accepted: bool) {
        let serial = self.state.output_control.serial;
        if let Some(config) = self.state.output_control.configs.iter_mut().find(|config| config.token == Some(token)) {
            config.token = None;
            if config.object.is_alive() {
                if accepted { config.object.succeeded(); }
                else if config.serial != serial { config.object.cancelled(); }
                else { config.object.failed(); }
            }
        }
    }
}

impl State {
    pub(super) fn sync_output_control(&mut self) {
        let catalog = &self.output_control.catalog;
        for binding in &mut self.output_control.bindings {
            if binding.stopped || !binding.manager.is_alive() { continue; }
            let Some(client) = binding.manager.client() else { continue };
            let mut changed = !binding.initialized;
            binding.suppressed.retain(|id| catalog.iter().any(|head| head.id == *id));
            binding.heads.retain(|id, tracked| {
                if catalog.iter().any(|head| head.id == *id && head.name == tracked.snapshot.name) { return true; }
                for (mode, _) in &tracked.modes { if mode.is_alive() { mode.finished(); } }
                if tracked.object.is_alive() { tracked.object.finished(); }
                changed = true;
                false
            });
            for head in catalog {
                if binding.suppressed.contains(&head.id) { continue; }
                if let Some(tracked) = binding.heads.get(&head.id) && tracked.snapshot == *head { continue; }
                let mut tracked = if let Some(tracked) = binding.heads.remove(&head.id) { tracked } else {
                    let Ok(object) = client.create_resource::<Head, _, State>(&self.handle, 3, head.id) else { continue };
                    binding.manager.head(&object);
                    object.name(head.name.clone());
                    object.description(format!("Park output {}", head.name));
                    object.physical_size(0, 0);
                    Tracked { object, snapshot: head.clone(), modes: Vec::new() }
                };
                if tracked.modes.iter().map(|(_, mode)| mode).ne(head.modes.iter()) {
                    for (mode, _) in tracked.modes.drain(..) { if mode.is_alive() { mode.finished(); } }
                    for mode in &head.modes {
                        let Ok(object) = client.create_resource::<ModeObject, _, State>(&self.handle, 3, ModeData { id: head.id, mode: mode.clone() }) else { continue };
                        tracked.object.mode(&object);
                        object.size(mode.size[0], mode.size[1]);
                        object.refresh(mode.refresh_mhz);
                        if mode.preferred { object.preferred(); }
                        tracked.modes.push((object, mode.clone()));
                    }
                }
                tracked.object.enabled(i32::from(head.current.is_some()));
                if let Some(index) = head.current {
                    if let Some((mode, _)) = tracked.modes.get(index).filter(|(mode, _)| mode.is_alive()) { tracked.object.current_mode(mode); }
                    tracked.object.position(head.position[0], head.position[1]);
                    tracked.object.transform(wl_output::Transform::Normal);
                    tracked.object.scale(head.scale);
                }
                tracked.snapshot = head.clone();
                binding.heads.insert(head.id, tracked);
                changed = true;
            }
            if changed { binding.manager.done(self.output_control.serial); binding.initialized = true; }
        }
        self.output_control.bindings.retain(|binding| binding.manager.is_alive());
    }
}

impl GlobalDispatch<Manager, ()> for State {
    fn bind(state: &mut Self, _: &DisplayHandle, _: &Client, resource: New<Manager>, _: &(), data: &mut DataInit<'_, Self>) {
        let manager = data.init(resource, ());
        if state.output_control.bindings.len() >= 128 { manager.finished(); return; }
        state.output_control.bindings.push(Binding { manager, heads: BTreeMap::new(), suppressed: BTreeSet::new(), stopped: false, initialized: false });
        state.sync_output_control();
    }
    fn can_view(client: Client, _: &()) -> bool { permitted(&client) }
}

impl Dispatch<Manager, ()> for State {
    fn request(state: &mut Self, _: &Client, resource: &Manager, request: manager::Request, _: &(), _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        match request {
            manager::Request::Stop => {
                if let Some(binding) = state.output_control.bindings.iter_mut().find(|binding| &binding.manager == resource) { binding.stopped = true; }
                resource.finished();
            }
            manager::Request::CreateConfiguration { id, serial } => {
                let object = data.init(id, ());
                if state.output_control.configs.len() >= 64 { object.post_error(0u32, "Park output configuration limit reached"); return; }
                state.output_control.configs.push(Config { object, serial, heads: BTreeMap::new(), used: false, token: None, children: Vec::new() });
            }
            _ => {}
        }
    }
}

impl Dispatch<Configuration, ()> for State {
    fn request(state: &mut Self, _: &Client, resource: &Configuration, request: configuration::Request, _: &(), _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        let selected = match &request {
            configuration::Request::EnableHead { head, .. } | configuration::Request::DisableHead { head } => state.output_control.head_id(head),
            _ => None,
        };
        let enabling = matches!(&request, configuration::Request::EnableHead { .. });
        let request = match request {
            configuration::Request::EnableHead { id, head } => {
                let child = data.init(id, SettingsData { config: resource.clone(), id: selected.unwrap_or(OutputId(0)) });
                if let Some(config) = state.output_control.configs.iter_mut().find(|config| &config.object == resource) { config.children.push(child); }
                configuration::Request::DisableHead { head }
            }
            request => request,
        };
        let Some(config) = state.output_control.configs.iter_mut().find(|config| &config.object == resource) else { return };
        if matches!(request, configuration::Request::Destroy) { return; }
        if config.used { resource.post_error(configuration::Error::AlreadyUsed, "Output configuration was already submitted"); return; }
        match request {
            configuration::Request::DisableHead { .. } if enabling => {
                let head = selected.and_then(|id| state.output_control.catalog.iter().find(|head| head.id == id));
                let Some(head) = head else { config.used = true; resource.cancelled(); return };
                if config.heads.contains_key(&head.id) { resource.post_error(configuration::Error::AlreadyConfiguredHead, "Output configured twice"); return; }
                let index = head.current.or_else(|| head.modes.iter().position(|mode| mode.preferred)).unwrap_or(0);
                config.heads.insert(head.id, Some(Draft { settings: OutputSettings { mode: head.modes[index].clone(), position: head.position, scale: head.scale }, mask: 0, valid: true }));
            }
            configuration::Request::DisableHead { .. } => {
                let Some(id) = selected else { config.used = true; resource.cancelled(); return };
                if config.heads.insert(id, None).is_some() { resource.post_error(configuration::Error::AlreadyConfiguredHead, "Output configured twice"); }
            }
            configuration::Request::Apply | configuration::Request::Test => {
                let test = matches!(request, configuration::Request::Test);
                config.used = true;
                if config.serial != state.output_control.serial { resource.cancelled(); return; }
                if config.heads.len() != state.output_control.catalog.len() { resource.post_error(configuration::Error::UnconfiguredHead, "A complete output configuration is required"); return; }
                let heads: BTreeMap<_, _> = config.heads.iter().map(|(id, draft)| (*id, draft.as_ref().map(|draft| draft.settings.clone()))).collect();
                if state.lock.owner.is_some() || config.heads.values().flatten().any(|draft| !draft.valid) || !valid_heads(&heads) || state.output_control.requests.len() >= 8 {
                    resource.failed(); return;
                }
                let Some(token) = state.output_control.next_token.checked_add(1) else { resource.failed(); return };
                state.output_control.next_token = token;
                config.token = Some(token);
                state.output_control.requests.push_back(OutputRequest { token, serial: config.serial, test, heads });
            }
            _ => {}
        }
    }

    fn destroyed(state: &mut Self, _: ClientId, resource: &Configuration, _: &()) {
        let tokens: Vec<_> = state.output_control.configs.iter().filter(|config| &config.object == resource).filter_map(|config| config.token).collect();
        let children: Vec<_> = state.output_control.configs.iter().filter(|config| &config.object == resource).flat_map(|config| config.children.iter().cloned()).collect();
        state.output_control.requests.retain(|request| !tokens.contains(&request.token));
        state.output_control.configs.retain(|config| &config.object != resource);
        for child in children { let _ = state.handle.backend_handle().destroy_object::<State>(&child.id()); }
    }
}

impl Dispatch<Settings, SettingsData> for State {
    fn request(state: &mut Self, _: &Client, resource: &Settings, request: settings::Request, data: &SettingsData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        let Some(config) = state.output_control.configs.iter_mut().find(|config| config.object == data.config) else { return };
        if config.used { config.object.post_error(configuration::Error::AlreadyUsed, "Output configuration was already submitted"); return; }
        let Some(Some(draft)) = config.heads.get_mut(&data.id) else { return };
        let mask = match &request { settings::Request::SetMode { .. } | settings::Request::SetCustomMode { .. } => 1,
            settings::Request::SetPosition { .. } => 2, settings::Request::SetTransform { .. } => 4,
            settings::Request::SetScale { .. } => 8, _ => return };
        if draft.mask & mask != 0 { resource.post_error(settings::Error::AlreadySet, "Output property configured twice"); return; }
        draft.mask |= mask;
        match request {
            settings::Request::SetMode { mode } => {
                let mode_data = mode.data::<ModeData>();
                let current = state.output_control.bindings.iter().flat_map(|binding| binding.heads.values()).any(|head| head.modes.iter().any(|(object, _)| object == &mode));
                if let Some(mode) = mode_data.filter(|mode| mode.id == data.id && current) { draft.settings.mode = mode.mode.clone(); }
                else { resource.post_error(settings::Error::InvalidMode, "Mode does not belong to this output"); }
            }
            settings::Request::SetCustomMode { width, height, refresh } => {
                if width <= 0 || height <= 0 || refresh < 0 { resource.post_error(settings::Error::InvalidCustomMode, "Invalid custom output mode"); return; }
                let mode = state.output_control.catalog.iter().find(|head| head.id == data.id).and_then(|head| head.modes.iter().find(|mode| mode.size == [width, height] && (refresh == 0 || mode.refresh_mhz == refresh)));
                if let Some(mode) = mode { draft.settings.mode = mode.clone(); } else { draft.valid = false; }
            }
            settings::Request::SetPosition { x, y } => draft.settings.position = [x, y],
            settings::Request::SetTransform { transform } => match transform {
                WEnum::Value(wl_output::Transform::Normal) => {}
                WEnum::Value(_) => draft.valid = false,
                WEnum::Unknown(_) => resource.post_error(settings::Error::InvalidTransform, "Invalid output transform"),
            },
            settings::Request::SetScale { scale } => {
                if !scale.is_finite() || scale <= 0.0 { resource.post_error(settings::Error::InvalidScale, "Output scale must be positive"); return; }
                draft.settings.scale = (scale * 120.0).round() / 120.0;
            }
            _ => {}
        }
    }
}

impl Dispatch<Head, OutputId> for State {
    fn request(_: &mut Self, _: &Client, _: &Head, _: head::Request, _: &OutputId, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}

    fn destroyed(state: &mut Self, _: ClientId, resource: &Head, id: &OutputId) {
        for binding in &mut state.output_control.bindings {
            if binding.heads.get(id).is_some_and(|head| &head.object == resource) {
                binding.heads.remove(id);
                binding.suppressed.insert(*id);
            }
        }
    }
}

impl Dispatch<ModeObject, ModeData> for State {
    fn request(_: &mut Self, _: &Client, _: &ModeObject, _: <ModeObject as Resource>::Request, _: &ModeData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}
