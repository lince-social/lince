use super::*;
use lince_interface::host::color::{Description, Primaries, Transfer};
use smithay::{reexports::wayland_server::{DataInit, Dispatch, GlobalDispatch, New, WEnum, backend::GlobalId},
    wayland::compositor::Cacheable};
use wayland_protocols::wp::color_management::v1::server::{
    wp_color_manager_v1::{self as manager, WpColorManagerV1 as Manager},
    wp_color_management_surface_v1::{self as surface, WpColorManagementSurfaceV1 as Surface},
    wp_color_management_output_v1::{self as output, WpColorManagementOutputV1 as ColorOutput},
    wp_color_management_surface_feedback_v1::{self as feedback, WpColorManagementSurfaceFeedbackV1 as Feedback},
    wp_image_description_creator_params_v1::{self as params, WpImageDescriptionCreatorParamsV1 as Params},
    wp_image_description_creator_icc_v1::{self as icc, WpImageDescriptionCreatorIccV1 as Icc},
    wp_image_description_v1::{self as image, WpImageDescriptionV1 as Image},
    wp_image_description_info_v1::{self as info, WpImageDescriptionInfoV1 as Info},
};

#[derive(Default)]
pub(super) struct Color {
    global: Option<GlobalId>,
    hdr: bool,
    total: Arc<AtomicUsize>,
    clients: HashMap<ClientId, Arc<AtomicUsize>>,
    descriptions: BTreeMap<Description, Weak<Identity>>,
    next: u32,
    outputs: Vec<smithay::reexports::wayland_server::Weak<ColorOutput>>,
    feedback: Vec<smithay::reexports::wayland_server::Weak<Feedback>>,
    information: Vec<Info>,
}

struct Lease { client: Arc<AtomicUsize>, total: Arc<AtomicUsize> }

impl Drop for Lease {
    fn drop(&mut self) { self.client.fetch_sub(1, Ordering::AcqRel); self.total.fetch_sub(1, Ordering::AcqRel); }
}

impl Color {
    fn claim(&mut self, client: &Client) -> Option<Lease> {
        self.clients.retain(|_, count| count.load(Ordering::Acquire) > 0);
        let count = self.clients.entry(client.id()).or_default();
        if count.load(Ordering::Acquire) >= 256 || self.total.load(Ordering::Acquire) >= 4096 { return None; }
        count.fetch_add(1, Ordering::AcqRel);
        self.total.fetch_add(1, Ordering::AcqRel);
        Some(Lease { client: count.clone(), total: self.total.clone() })
    }

    fn identity(&mut self, description: Description) -> Option<Arc<Identity>> {
        self.descriptions.retain(|_, identity| identity.strong_count() > 0);
        if let Some(identity) = self.descriptions.get(&description).and_then(Weak::upgrade) { return Some(identity); }
        self.next = self.next.checked_add(1)?;
        let identity = Arc::new(Identity { id: self.next, description });
        self.descriptions.insert(description, Arc::downgrade(&identity));
        Some(identity)
    }
}

struct Identity { id: u32, description: Description }
struct ManagerData { _lease: Option<Lease> }
struct SurfaceData { surface: WlSurface, _lease: Option<Lease> }
struct OutputData { output: Option<Output>, description: Mutex<Option<Description>>, _lease: Option<Lease> }
struct FeedbackData { surface: WlSurface, preferred: Mutex<Option<Arc<Identity>>>, _lease: Option<Lease> }
struct ImageData { value: Option<Arc<Identity>>, information: bool, _lease: Option<Lease> }
struct InfoData { _lease: Option<Lease> }

#[derive(Default)]
struct Parameters {
    transfer: Option<Transfer>,
    primaries: Option<Primaries>,
    luminances: Option<[u32; 3]>,
    mastering: lince_interface::host::color::Mastering,
}

struct ParamsData { parameters: Mutex<Parameters>, _lease: Option<Lease> }

#[derive(Default)]
struct SurfaceOwner(Mutex<Option<smithay::reexports::wayland_server::Weak<Surface>>>);

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SurfaceColor { pub description: Option<Description> }

impl Cacheable for SurfaceColor {
    fn commit(&mut self, _: &DisplayHandle) -> Self { *self }
    fn merge_into(self, into: &mut Self, _: &DisplayHandle) { *into = self; }
}

impl Server {
    #[cfg(all(test, feature = "nested", feature = "server-tests"))]
    pub(crate) fn color_test_resources(&self) -> [usize; 4] {
        [self.state.color.total.load(Ordering::Acquire), self.state.color.clients.len(),
            self.state.color.descriptions.values().filter(|identity| identity.strong_count() > 0).count(), self.state.color.descriptions.len()]
    }

    pub fn enable_hdr_color_management(&mut self) -> io::Result<()> {
        if self.state.color.global.is_some() && !self.state.color.hdr {
            return Err(io::Error::other("Color capabilities cannot change after the protocol global is published"));
        }
        self.state.color.hdr = true;
        self.enable_sdr_color_management();
        Ok(())
    }

    pub fn enable_sdr_color_management(&mut self) {
        if self.state.color.global.is_none() {
            self.state.color.global = Some(self.state.handle.create_global::<State, Manager, _>(1, ()));
        }
    }
}

fn named_primaries(value: Primaries) -> manager::Primaries {
    match value { Primaries::Srgb => manager::Primaries::Srgb, Primaries::DisplayP3 => manager::Primaries::DisplayP3,
        Primaries::Bt2020 => manager::Primaries::Bt2020 }
}

fn named_transfer(value: Transfer) -> manager::TransferFunction {
    match value { Transfer::Srgb => manager::TransferFunction::Srgb, Transfer::Gamma22 => manager::TransferFunction::Gamma22,
        Transfer::Pq => manager::TransferFunction::St2084Pq, Transfer::Linear => manager::TransferFunction::ExtLinear }
}

fn limit(resource: &impl Resource) { resource.post_error(0u32, "Park color-management object limit reached"); }

impl State {
    pub(super) fn finish_color_information(&mut self) {
        for object in std::mem::take(&mut self.color.information) {
            if object.is_alive() { object.done(); }
        }
    }

    fn output_color_description(&self, output: &Output) -> Option<Description> {
        let id = self.outputs.iter().find(|(_, registered)| registered.output == *output)?.0;
        let Some(status) = self.output_control.color.get(id) else { return Some(Description::default()); };
        let effective = status.effective?;
        if effective.mode == lince_interface::host::color::ColorMode::Srgb { return Some(Description::default()); }
        let mut description = Description::hdr10();
        description.mastering.primaries = Some(Primaries::Bt2020.chromaticities());
        description.mastering.luminances = Some([50, effective.peak_nits]);
        Some(description)
    }

    fn preferred_color_description(&self, surface: &WlSurface) -> Description {
        let output = self.input_root(surface).and_then(|root| {
            self.id(&root).and_then(|id| self.presented_geometry.as_ref()?.items.iter()
                .find(|item| item.id == lince_interface::host::ItemId::Application(id)).map(|item| item.output))
                .or_else(|| self.layers.outputs.get(&root).copied())
        });
        output.and_then(|id| self.outputs.get(&id)).and_then(|registered| self.output_color_description(&registered.output))
            .filter(|_| self.color.hdr).unwrap_or_default()
    }

    pub(super) fn refresh_color_feedback(&mut self) {
        let mut done = Vec::<(Output, Client)>::new();
        let outputs = std::mem::take(&mut self.color.outputs);
        for weak in outputs {
            let Ok(resource) = weak.upgrade() else { continue };
            let Some(data) = resource.data::<OutputData>() else { continue };
            let Some(output) = data.output.as_ref().filter(|output| self.outputs.values().any(|registered| registered.output == **output)) else { continue };
            let description = self.output_color_description(output);
            let mut previous = data.description.lock().unwrap_or_else(|error| error.into_inner());
            if *previous != description {
                *previous = description;
                resource.image_description_changed();
                if let Some(client) = resource.client()
                    && !done.iter().any(|(old_output, old_client)| old_output == output && old_client.id() == client.id()) {
                    done.push((output.clone(), client));
                }
            }
            self.color.outputs.push(weak);
        }
        for (output, client) in done {
            for resource in output.client_outputs(&client).filter(|resource| resource.version() >= 2) { resource.done(); }
        }
        let feedback = std::mem::take(&mut self.color.feedback);
        for weak in feedback {
            let Ok(resource) = weak.upgrade() else { continue };
            let Some(data) = resource.data::<FeedbackData>() else { continue };
            if !data.surface.is_alive() { continue; }
            let description = self.preferred_color_description(&data.surface);
            let mut previous = data.preferred.lock().unwrap_or_else(|error| error.into_inner());
            if previous.as_ref().is_none_or(|previous| previous.description != description)
                && let Some(preferred) = self.color.identity(description) {
                resource.preferred_changed(preferred.id);
                *previous = Some(preferred);
            }
            self.color.feedback.push(weak);
        }
    }

    fn color_image(&mut self, client: &Client, object: New<Image>, description: Option<Description>, information: bool, data: &mut DataInit<'_, Self>) {
        let lease = self.color.claim(client);
        let valid = lease.is_some();
        let value = description.and_then(|description| self.color.identity(description));
        let object = data.init(object, ImageData { value: value.clone(), information, _lease: lease });
        if !valid { limit(&object); return; }
        match value {
            Some(value) => object.ready(value.id),
            None => object.failed(if description.is_some() { image::Cause::OperatingSystem } else { image::Cause::NoOutput }, "The color description or its output is unavailable.".into()),
        }
    }
}

impl GlobalDispatch<Manager, ()> for State {
    fn bind(state: &mut Self, _: &DisplayHandle, client: &Client, object: New<Manager>, _: &(), data: &mut DataInit<'_, Self>) {
        let lease = state.color.claim(client);
        let valid = lease.is_some();
        let object = data.init(object, ManagerData { _lease: lease });
        if !valid { limit(&object); return; }
        object.supported_intent(manager::RenderIntent::Perceptual);
        for feature in [manager::Feature::Parametric, manager::Feature::SetLuminances] { object.supported_feature(feature); }
        for primaries in [Primaries::Srgb, Primaries::DisplayP3] { object.supported_primaries_named(named_primaries(primaries)); }
        for transfer in [Transfer::Srgb, Transfer::Gamma22] { object.supported_tf_named(named_transfer(transfer)); }
        if state.color.hdr {
            object.supported_feature(manager::Feature::SetMasteringDisplayPrimaries);
            object.supported_primaries_named(named_primaries(Primaries::Bt2020));
            for transfer in [Transfer::Pq, Transfer::Linear] { object.supported_tf_named(named_transfer(transfer)); }
        }
        object.done();
    }
}

impl Dispatch<Manager, ManagerData> for State {
    fn request(state: &mut Self, client: &Client, resource: &Manager, request: manager::Request, _: &ManagerData, _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        match request {
            manager::Request::Destroy => {}
            manager::Request::GetSurface { id, surface: target } => {
                let lease = state.color.claim(client);
                let valid = lease.is_some();
                let object = data.init(id, SurfaceData { surface: target.clone(), _lease: lease });
                if !valid { limit(resource); return; }
                if !target.is_alive() { resource.post_error(manager::Error::SurfaceExists, "The color surface is unavailable"); return; }
                with_states(&target, |states| {
                    states.data_map.insert_if_missing(SurfaceOwner::default);
                    let mut owner = states.data_map.get::<SurfaceOwner>().unwrap().0.lock().unwrap_or_else(|error| error.into_inner());
                    if owner.as_ref().and_then(|owner| owner.upgrade().ok()).is_some() {
                        resource.post_error(manager::Error::SurfaceExists, "This surface already has a color-management object");
                    } else { *owner = Some(object.downgrade()); }
                });
            }
            manager::Request::GetOutput { id, output } => {
                let lease = state.color.claim(client); let valid = lease.is_some();
                let output = Output::from_resource(&output);
                let description = output.as_ref().and_then(|output| state.output_color_description(output));
                let object = data.init(id, OutputData { output, description: Mutex::new(description), _lease: lease });
                if !valid { limit(resource); } else { state.color.outputs.push(object.downgrade()); }
            }
            manager::Request::GetSurfaceFeedback { id, surface } => {
                let lease = state.color.claim(client); let valid = lease.is_some();
                let description = state.preferred_color_description(&surface);
                let preferred = state.color.identity(description);
                let object = data.init(id, FeedbackData { surface, preferred: Mutex::new(preferred.clone()), _lease: lease });
                if !valid { limit(resource); return; }
                if let Some(preferred) = preferred { object.preferred_changed(preferred.id); }
                state.color.feedback.push(object.downgrade());
            }
            manager::Request::CreateParametricCreator { obj } => {
                let lease = state.color.claim(client); let valid = lease.is_some();
                data.init(obj, ParamsData { parameters: Default::default(), _lease: lease });
                if !valid { limit(resource); }
            }
            manager::Request::CreateIccCreator { obj } => {
                data.init(obj, ());
                resource.post_error(manager::Error::UnsupportedFeature, "ICC profile import is not advertised");
            }
            manager::Request::CreateWindowsScrgb { image_description } => {
                data.init(image_description, ImageData { value: None, information: false, _lease: None });
                resource.post_error(manager::Error::UnsupportedFeature, "Extended linear HDR surfaces are not advertised by the SDR pipeline");
            }
            _ => resource.post_error(manager::Error::UnsupportedFeature, "This color-management feature is not advertised by Park"),
        }
    }
}

impl Dispatch<Surface, SurfaceData> for State {
    fn request(_: &mut Self, _: &Client, resource: &Surface, request: surface::Request, data: &SurfaceData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if !data.surface.is_alive() {
            if !matches!(request, surface::Request::Destroy) { resource.post_error(surface::Error::Inert, "The color surface no longer exists"); }
            return;
        }
        let description = match request {
            surface::Request::SetImageDescription { image_description, render_intent } => {
                if render_intent != WEnum::Value(manager::RenderIntent::Perceptual) {
                    resource.post_error(surface::Error::RenderIntent, "This rendering intent is not advertised"); return;
                }
                let Some(description) = image_description.data::<ImageData>().and_then(|image| image.value.as_ref()).map(|image| image.description) else {
                    resource.post_error(surface::Error::ImageDescription, "The image description is not ready"); return;
                };
                Some(description)
            }
            surface::Request::UnsetImageDescription | surface::Request::Destroy => None,
            _ => return,
        };
        with_states(&data.surface, |states| states.cached_state.get::<SurfaceColor>().pending().description = description);
    }

    fn destroyed(_: &mut Self, _: ClientId, resource: &Surface, data: &SurfaceData) {
        if !data.surface.is_alive() { return; }
        with_states(&data.surface, |states| {
            let Some(owner) = states.data_map.get::<SurfaceOwner>() else { return; };
            let mut owner = owner.0.lock().unwrap_or_else(|error| error.into_inner());
            if owner.as_ref().is_some_and(|owner| owner.id() == resource.id()) {
                *owner = None;
                states.cached_state.get::<SurfaceColor>().pending().description = None;
            }
        });
    }
}

impl Dispatch<ColorOutput, OutputData> for State {
    fn request(state: &mut Self, client: &Client, _: &ColorOutput, request: output::Request, output: &OutputData, _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        if let output::Request::GetImageDescription { image_description } = request {
            let description = output.output.as_ref().and_then(|output| state.output_color_description(output));
            state.color_image(client, image_description, description, true, data);
        }
    }
}

impl Dispatch<Feedback, FeedbackData> for State {
    fn request(state: &mut Self, client: &Client, resource: &Feedback, request: feedback::Request, feedback: &FeedbackData, _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        match request {
            feedback::Request::Destroy => {}
            feedback::Request::GetPreferred { image_description } | feedback::Request::GetPreferredParametric { image_description } => {
                if !feedback.surface.is_alive() {
                    data.init(image_description, ImageData { value: None, information: false, _lease: None });
                    resource.post_error(feedback::Error::Inert, "The color surface no longer exists"); return;
                }
                let description = feedback.preferred.lock().unwrap_or_else(|error| error.into_inner()).as_ref().map(|identity| identity.description);
                state.color_image(client, image_description, description, true, data);
            }
            _ => {}
        }
    }
}

impl Dispatch<Params, ParamsData> for State {
    fn request(state: &mut Self, client: &Client, resource: &Params, request: params::Request, parameters: &ParamsData, _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        let mut parameters = parameters.parameters.lock().unwrap_or_else(|error| error.into_inner());
        match request {
            params::Request::SetTfNamed { tf } => {
                if parameters.transfer.is_some() { resource.post_error(params::Error::AlreadySet, "Transfer characteristic is already set"); return; }
                parameters.transfer = match tf {
                    WEnum::Value(manager::TransferFunction::Srgb) => Some(Transfer::Srgb),
                    WEnum::Value(manager::TransferFunction::Gamma22) => Some(Transfer::Gamma22),
                    WEnum::Value(manager::TransferFunction::St2084Pq) if state.color.hdr => Some(Transfer::Pq),
                    WEnum::Value(manager::TransferFunction::ExtLinear) if state.color.hdr => Some(Transfer::Linear),
                    _ => { resource.post_error(params::Error::InvalidTf, "The transfer characteristic is not advertised"); return; }
                };
            }
            params::Request::SetPrimariesNamed { primaries } => {
                if parameters.primaries.is_some() { resource.post_error(params::Error::AlreadySet, "Primaries are already set"); return; }
                parameters.primaries = match primaries {
                    WEnum::Value(manager::Primaries::Srgb) => Some(Primaries::Srgb),
                    WEnum::Value(manager::Primaries::DisplayP3) => Some(Primaries::DisplayP3),
                    WEnum::Value(manager::Primaries::Bt2020) if state.color.hdr => Some(Primaries::Bt2020),
                    _ => { resource.post_error(params::Error::InvalidPrimariesNamed, "These primaries are not advertised"); return; }
                };
            }
            params::Request::SetLuminances { min_lum, max_lum, reference_lum } => {
                if parameters.luminances.is_some() { resource.post_error(params::Error::AlreadySet, "Luminances are already set"); return; }
                if u64::from(reference_lum) * 10000 <= u64::from(min_lum) {
                    resource.post_error(params::Error::InvalidLuminance, "Reference luminance must exceed minimum luminance"); return;
                }
                parameters.luminances = Some([min_lum, max_lum, reference_lum]);
            }
            params::Request::SetMasteringDisplayPrimaries { r_x, r_y, g_x, g_y, b_x, b_y, w_x, w_y } => {
                if !state.color.hdr { resource.post_error(params::Error::UnsupportedFeature, "Mastering primaries are not advertised"); return; }
                if parameters.mastering.primaries.is_some() { resource.post_error(params::Error::AlreadySet, "Mastering primaries are already set"); return; }
                parameters.mastering.primaries = Some([[r_x, r_y], [g_x, g_y], [b_x, b_y], [w_x, w_y]]);
            }
            params::Request::SetMasteringLuminance { min_lum, max_lum } => {
                if !state.color.hdr { resource.post_error(params::Error::UnsupportedFeature, "Mastering luminance is not advertised"); return; }
                if parameters.mastering.luminances.is_some() { resource.post_error(params::Error::AlreadySet, "Mastering luminance is already set"); return; }
                if u64::from(min_lum) >= u64::from(max_lum) * 10000 {
                    resource.post_error(params::Error::InvalidLuminance, "Mastering maximum must exceed its minimum"); return;
                }
                parameters.mastering.luminances = Some([min_lum, max_lum]);
            }
            params::Request::SetMaxCll { max_cll } => {
                if parameters.mastering.max_cll.is_some() { resource.post_error(params::Error::AlreadySet, "Maximum content light level is already set"); return; }
                parameters.mastering.max_cll = Some(max_cll);
            }
            params::Request::SetMaxFall { max_fall } => {
                if parameters.mastering.max_fall.is_some() { resource.post_error(params::Error::AlreadySet, "Maximum frame-average light level is already set"); return; }
                parameters.mastering.max_fall = Some(max_fall);
            }
            params::Request::Create { image_description } => {
                let lease = state.color.claim(client); let valid = lease.is_some();
                let description = parameters.transfer.zip(parameters.primaries).map(|(transfer, primaries)| {
                    let mut description = Description { transfer, primaries, mastering: parameters.mastering,
                        ..if transfer == Transfer::Pq { Description::hdr10() } else { Description::default() } };
                    if let Some([minimum, maximum, reference]) = parameters.luminances {
                        description.minimum_luminance = minimum; description.maximum_luminance = maximum; description.reference_luminance = reference;
                    }
                    if transfer == Transfer::Pq { description.maximum_luminance = 10000; }
                    description
                });
                let invalid = description.is_some_and(|description| {
                    let metadata = description.mastering;
                    let [minimum, maximum] = metadata.luminances.unwrap_or([description.minimum_luminance, description.maximum_luminance]);
                    !description.valid() || metadata.max_cll.zip(metadata.max_fall).is_some_and(|(cll, fall)| fall > cll)
                        || [metadata.max_cll, metadata.max_fall].into_iter().flatten().any(|value| {
                            u64::from(value) * 10000 <= u64::from(minimum) || value > maximum
                        })
                });
                let supported = description.filter(|description| !invalid && if state.color.hdr { description.supported() } else { description.supported_sdr() });
                let value = supported.and_then(|description| state.color.identity(description));
                let object = data.init(image_description, ImageData { value: value.clone(), information: false, _lease: lease });
                if !valid { limit(resource); return; }
                if description.is_none() { resource.post_error(params::Error::IncompleteSet, "Transfer characteristic and primaries are required"); }
                else if invalid { resource.post_error(params::Error::InvalidLuminance, "The luminance or content-light range is invalid"); }
                else if let Some(value) = value { object.ready(value.id); }
                else { object.failed(image::Cause::Unsupported, "This color volume is not supported by the active color pipeline.".into()); }
            }
            _ => resource.post_error(params::Error::UnsupportedFeature, "This parametric feature is not advertised"),
        }
    }
}

impl Dispatch<Image, ImageData> for State {
    fn request(state: &mut Self, client: &Client, resource: &Image, request: image::Request, image: &ImageData, _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        if let image::Request::GetInformation { information } = request {
            let lease = state.color.claim(client); let valid = lease.is_some();
            let object = data.init(information, InfoData { _lease: lease });
            if !valid { limit(resource); return; }
            let Some(identity) = &image.value else { resource.post_error(image::Error::NotReady, "The image description is not ready"); return; };
            if !image.information { resource.post_error(image::Error::NoInformation, "Client-created parametric descriptions do not allow information requests"); return; }
            let value = identity.description;
            let [r, g, b, w] = value.primaries.chromaticities();
            object.primaries(r[0], r[1], g[0], g[1], b[0], b[1], w[0], w[1]);
            object.primaries_named(named_primaries(value.primaries));
            object.tf_named(named_transfer(value.transfer));
            object.luminances(value.minimum_luminance, value.maximum_luminance, value.reference_luminance);
            if let Some([r, g, b, w]) = value.mastering.primaries {
                object.target_primaries(r[0], r[1], g[0], g[1], b[0], b[1], w[0], w[1]);
            }
            if let Some([minimum, maximum]) = value.mastering.luminances { object.target_luminance(minimum, maximum); }
            if let Some(value) = value.mastering.max_cll { object.target_max_cll(value); }
            if let Some(value) = value.mastering.max_fall { object.target_max_fall(value); }
            state.color.information.push(object);
        }
    }
}

impl Dispatch<Info, InfoData> for State {
    fn request(_: &mut Self, _: &Client, _: &Info, _: info::Request, _: &InfoData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<Icc, ()> for State {
    fn request(_: &mut Self, _: &Client, _: &Icc, _: icc::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}
