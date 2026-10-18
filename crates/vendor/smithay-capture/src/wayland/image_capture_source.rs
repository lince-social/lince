use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

use wayland_protocols::ext::image_capture_source::v1::server::{
    ext_foreign_toplevel_image_capture_source_manager_v1::{
        self, ExtForeignToplevelImageCaptureSourceManagerV1,
    },
    ext_image_capture_source_v1::{self, ExtImageCaptureSourceV1},
    ext_output_image_capture_source_manager_v1::{self, ExtOutputImageCaptureSourceManagerV1},
};
use wayland_server::{
    backend::GlobalId, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, Weak,
};

use crate::output::Output;
use crate::utils::user_data::UserDataMap;
use crate::wayland::foreign_toplevel_list::ForeignToplevelHandle;


static NEXT_SOURCE_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
struct ImageCaptureSourceInner {
    id: usize,
    alive: AtomicBool,
    instances: Mutex<Vec<Weak<ExtImageCaptureSourceV1>>>,
}

#[derive(Debug, Clone)]
pub struct ImageCaptureSourceWeakHandle {
    inner: std::sync::Weak<(ImageCaptureSourceInner, UserDataMap)>,
}

impl ImageCaptureSourceWeakHandle {
    pub fn upgrade(&self) -> Option<ImageCaptureSource> {
        Some(ImageCaptureSource {
            inner: self.inner.upgrade()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ImageCaptureSource {
    inner: Arc<(ImageCaptureSourceInner, UserDataMap)>,
}

impl PartialEq for ImageCaptureSource {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for ImageCaptureSource {}

impl std::hash::Hash for ImageCaptureSource {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl ImageCaptureSource {
    pub fn new() -> Self {
        Self {
            inner: Arc::new((
                ImageCaptureSourceInner {
                    id: NEXT_SOURCE_ID.fetch_add(1, Ordering::Relaxed),
                    alive: AtomicBool::new(true),
                    instances: Mutex::new(Vec::new()),
                },
                UserDataMap::new(),
            )),
        }
    }

    pub fn id(&self) -> usize {
        self.inner.0.id
    }

    pub fn alive(&self) -> bool {
        self.inner.0.alive.load(Ordering::Acquire)
    }

    pub fn user_data(&self) -> &UserDataMap {
        &self.inner.1
    }

    pub fn downgrade(&self) -> ImageCaptureSourceWeakHandle {
        ImageCaptureSourceWeakHandle {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub fn from_resource(resource: &ExtImageCaptureSourceV1) -> Option<Self> {
        resource
            .data::<ImageCaptureSourceData>()
            .map(|d| d.source.clone())
    }

    pub fn add_instance(&self, resource: &ExtImageCaptureSourceV1) {
        self.inner.0.instances.lock().unwrap().push(resource.downgrade());
    }

    fn mark_destroyed(&self) {
        self.inner.0.alive.store(false, Ordering::Release);
    }
}

impl Default for ImageCaptureSource {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct ImageCaptureSourceData {
    pub source: ImageCaptureSource,
}


#[derive(Debug, Default)]
pub struct ImageCaptureSourceState;

impl ImageCaptureSourceState {
    pub fn new() -> Self {
        Self
    }
}

pub trait ImageCaptureSourceHandler:
    Dispatch<ExtImageCaptureSourceV1, ImageCaptureSourceData> + 'static
{
    fn source_destroyed(&mut self, source: ImageCaptureSource) {
        let _ = source;
    }
}

impl<D> Dispatch<ExtImageCaptureSourceV1, ImageCaptureSourceData, D> for ImageCaptureSourceState
where
    D: ImageCaptureSourceHandler,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        _resource: &ExtImageCaptureSourceV1,
        request: ext_image_capture_source_v1::Request,
        _data: &ImageCaptureSourceData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_capture_source_v1::Request::Destroy => {
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: wayland_server::backend::ClientId,
        _resource: &ExtImageCaptureSourceV1,
        data: &ImageCaptureSourceData,
    ) {
        data.source.mark_destroyed();
        state.source_destroyed(data.source.clone());
    }
}


#[allow(missing_debug_implementations)]
pub struct OutputCaptureSourceGlobalData {
    filter: Box<dyn Fn(&Client) -> bool + Send + Sync>,
}

#[derive(Debug)]
pub struct OutputCaptureSourceState {
    global: GlobalId,
}

impl OutputCaptureSourceState {
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: OutputCaptureSourceHandler,
    {
        Self::new_with_filter::<D, _>(display, |_| true)
    }

    pub fn new_with_filter<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: OutputCaptureSourceHandler,
        F: Fn(&Client) -> bool + Send + Sync + 'static,
    {
        let global = display.create_global::<D, ExtOutputImageCaptureSourceManagerV1, _>(
            1,
            OutputCaptureSourceGlobalData {
                filter: Box::new(filter),
            },
        );

        Self { global }
    }

    pub fn global(&self) -> GlobalId {
        self.global.clone()
    }
}

pub trait OutputCaptureSourceHandler:
    ImageCaptureSourceHandler
    + GlobalDispatch<ExtOutputImageCaptureSourceManagerV1, OutputCaptureSourceGlobalData>
    + Dispatch<ExtOutputImageCaptureSourceManagerV1, ()>
{
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState;

    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        let _ = (source, output);
    }
}

impl<D> GlobalDispatch<ExtOutputImageCaptureSourceManagerV1, OutputCaptureSourceGlobalData, D>
    for OutputCaptureSourceState
where
    D: OutputCaptureSourceHandler,
{
    fn bind(
        _state: &mut D,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ExtOutputImageCaptureSourceManagerV1>,
        _global_data: &OutputCaptureSourceGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, global_data: &OutputCaptureSourceGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ExtOutputImageCaptureSourceManagerV1, (), D> for OutputCaptureSourceState
where
    D: OutputCaptureSourceHandler,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &ExtOutputImageCaptureSourceManagerV1,
        request: ext_output_image_capture_source_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_output_image_capture_source_manager_v1::Request::CreateSource { source, output } => {
                let Some(output_inner) = Output::from_resource(&output) else {
                    resource.post_error(0u32, "invalid output");
                    return;
                };

                let capture_source = ImageCaptureSource::new();

                let source_resource = data_init.init(
                    source,
                    ImageCaptureSourceData {
                        source: capture_source.clone(),
                    },
                );

                capture_source.add_instance(&source_resource);
                state.output_source_created(capture_source, &output_inner);
            }
            ext_output_image_capture_source_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}


#[allow(missing_debug_implementations)]
pub struct ToplevelCaptureSourceGlobalData {
    filter: Box<dyn Fn(&Client) -> bool + Send + Sync>,
}

#[derive(Debug)]
pub struct ToplevelCaptureSourceState {
    global: GlobalId,
}

impl ToplevelCaptureSourceState {
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: ToplevelCaptureSourceHandler,
    {
        Self::new_with_filter::<D, _>(display, |_| true)
    }

    pub fn new_with_filter<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: ToplevelCaptureSourceHandler,
        F: Fn(&Client) -> bool + Send + Sync + 'static,
    {
        let global = display.create_global::<D, ExtForeignToplevelImageCaptureSourceManagerV1, _>(
            1,
            ToplevelCaptureSourceGlobalData {
                filter: Box::new(filter),
            },
        );

        Self { global }
    }

    pub fn global(&self) -> GlobalId {
        self.global.clone()
    }
}

pub trait ToplevelCaptureSourceHandler:
    ImageCaptureSourceHandler
    + GlobalDispatch<ExtForeignToplevelImageCaptureSourceManagerV1, ToplevelCaptureSourceGlobalData>
    + Dispatch<ExtForeignToplevelImageCaptureSourceManagerV1, ()>
{
    fn toplevel_capture_source_state(&mut self) -> &mut ToplevelCaptureSourceState;

    fn toplevel_source_created(&mut self, source: ImageCaptureSource, toplevel: &ForeignToplevelHandle) {
        let _ = (source, toplevel);
    }
}

impl<D> GlobalDispatch<ExtForeignToplevelImageCaptureSourceManagerV1, ToplevelCaptureSourceGlobalData, D>
    for ToplevelCaptureSourceState
where
    D: ToplevelCaptureSourceHandler,
{
    fn bind(
        _state: &mut D,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ExtForeignToplevelImageCaptureSourceManagerV1>,
        _global_data: &ToplevelCaptureSourceGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, global_data: &ToplevelCaptureSourceGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ExtForeignToplevelImageCaptureSourceManagerV1, (), D> for ToplevelCaptureSourceState
where
    D: ToplevelCaptureSourceHandler,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &ExtForeignToplevelImageCaptureSourceManagerV1,
        request: ext_foreign_toplevel_image_capture_source_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_foreign_toplevel_image_capture_source_manager_v1::Request::CreateSource {
                source,
                toplevel_handle,
            } => {
                let capture_source = ImageCaptureSource::new();

                let source_resource = data_init.init(
                    source,
                    ImageCaptureSourceData {
                        source: capture_source.clone(),
                    },
                );

                capture_source.add_instance(&source_resource);
                if let Some(handle) = ForeignToplevelHandle::from_resource(&toplevel_handle).filter(|handle| !handle.is_closed()) {
                    state.toplevel_source_created(capture_source, &handle);
                }
            }
            ext_foreign_toplevel_image_capture_source_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}


#[macro_export]
macro_rules! delegate_image_capture_source {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        const _: () = {
            use $crate::reexports::wayland_protocols::ext::image_capture_source::v1::server::{
                ext_image_capture_source_v1::ExtImageCaptureSourceV1,
            };
            use $crate::reexports::wayland_server::delegate_dispatch;
            use $crate::wayland::image_capture_source::{
                ImageCaptureSourceData, ImageCaptureSourceState,
            };

            delegate_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtImageCaptureSourceV1: ImageCaptureSourceData] => ImageCaptureSourceState
            );
        };
    };
}

#[macro_export]
macro_rules! delegate_output_capture_source {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        const _: () = {
            use $crate::reexports::wayland_protocols::ext::image_capture_source::v1::server::{
                ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
            };
            use $crate::reexports::wayland_server::{delegate_dispatch, delegate_global_dispatch};
            use $crate::wayland::image_capture_source::{
                OutputCaptureSourceGlobalData, OutputCaptureSourceState,
            };

            delegate_global_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtOutputImageCaptureSourceManagerV1: OutputCaptureSourceGlobalData] => OutputCaptureSourceState
            );
            delegate_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtOutputImageCaptureSourceManagerV1: ()] => OutputCaptureSourceState
            );
        };
    };
}

#[macro_export]
macro_rules! delegate_toplevel_capture_source {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        const _: () = {
            use $crate::reexports::wayland_protocols::ext::image_capture_source::v1::server::{
                ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
            };
            use $crate::reexports::wayland_server::{delegate_dispatch, delegate_global_dispatch};
            use $crate::wayland::image_capture_source::{
                ToplevelCaptureSourceGlobalData, ToplevelCaptureSourceState,
            };

            delegate_global_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtForeignToplevelImageCaptureSourceManagerV1: ToplevelCaptureSourceGlobalData] => ToplevelCaptureSourceState
            );
            delegate_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtForeignToplevelImageCaptureSourceManagerV1: ()] => ToplevelCaptureSourceState
            );
        };
    };
}

