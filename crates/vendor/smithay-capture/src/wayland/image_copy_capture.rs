use std::{
    ops,
    sync::{Arc, Mutex},
    time::Duration,
};

use wayland_protocols::ext::image_copy_capture::v1::server::{
    ext_image_copy_capture_cursor_session_v1::{self, ExtImageCopyCaptureCursorSessionV1},
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1, FailureReason},
    ext_image_copy_capture_manager_v1::{self, ExtImageCopyCaptureManagerV1},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};
use wayland_server::{
    backend::GlobalId,
    protocol::{wl_buffer::WlBuffer, wl_shm},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

#[cfg(feature = "backend_drm")]
use crate::backend::{
    allocator::{Buffer as AllocBuffer, Fourcc, Modifier},
    drm::DrmNode,
};
use crate::utils::{user_data::UserDataMap, Buffer as BufferCoords, IsAlive, Rectangle, Size, Transform};
use crate::wayland::image_capture_source::ImageCaptureSource;

use crate::backend::renderer::{buffer_type, BufferType};
#[cfg(feature = "backend_drm")]
use crate::wayland::dmabuf::get_dmabuf;
use crate::wayland::shm::with_buffer_contents;

pub use wayland_protocols::ext::image_copy_capture::v1::server::ext_image_copy_capture_frame_v1::FailureReason as CaptureFailureReason;

#[derive(Debug, Clone)]
pub struct BufferConstraints {
    pub size: Size<i32, BufferCoords>,
    pub shm: Vec<wl_shm::Format>,
    #[cfg(feature = "backend_drm")]
    pub dma: Option<DmabufConstraints>,
}

#[cfg(feature = "backend_drm")]
#[derive(Debug, Clone)]
pub struct DmabufConstraints {
    pub node: DrmNode,
    pub formats: Vec<(Fourcc, Vec<Modifier>)>,
}


fn validate_buffer(buffer: &WlBuffer, constraints: &BufferConstraints) -> Result<(), FailureReason> {
    match buffer_type(buffer) {
        Some(BufferType::Shm) => validate_shm_buffer(buffer, constraints),
        #[cfg(feature = "backend_drm")]
        Some(BufferType::Dma) => validate_dmabuf(buffer, constraints),
        _ => Err(FailureReason::BufferConstraints),
    }
}

fn validate_shm_buffer(buffer: &WlBuffer, constraints: &BufferConstraints) -> Result<(), FailureReason> {
    with_buffer_contents(buffer, |_, _, data| {
        if data.width != constraints.size.w || data.height != constraints.size.h {
            return Err(FailureReason::BufferConstraints);
        }

        if !constraints.shm.contains(&data.format) {
            return Err(FailureReason::BufferConstraints);
        }

        Ok(())
    })
    .map_err(|_| FailureReason::BufferConstraints)?
}

#[cfg(feature = "backend_drm")]
fn validate_dmabuf(buffer: &WlBuffer, constraints: &BufferConstraints) -> Result<(), FailureReason> {
    let dmabuf = get_dmabuf(buffer).map_err(|_| FailureReason::BufferConstraints)?;

    let dma_constraints = constraints.dma.as_ref().ok_or(FailureReason::BufferConstraints)?;

    let size = dmabuf.size();
    if size.w < constraints.size.w || size.h < constraints.size.h {
        return Err(FailureReason::BufferConstraints);
    }

    let format = dmabuf.format();
    let format_valid = dma_constraints
        .formats
        .iter()
        .any(|(f, modifiers)| *f == format.code && modifiers.contains(&format.modifier));

    if !format_valid {
        return Err(FailureReason::BufferConstraints);
    }

    Ok(())
}


#[derive(Debug)]
struct SessionInner {
    stopped: bool,
    resource_destroyed: bool,
    constraints: Option<BufferConstraints>,
    draw_cursors: bool,
    source: ImageCaptureSource,
    active_frames: Vec<FrameTrack>,
}

impl SessionInner {
    fn new(source: ImageCaptureSource, draw_cursors: bool) -> Self {
        Self {
            stopped: false,
            resource_destroyed: false,
            constraints: None,
            draw_cursors,
            source,
            active_frames: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionRef {
    obj: ExtImageCopyCaptureSessionV1,
    inner: Arc<Mutex<SessionInner>>,
    user_data: Arc<UserDataMap>,
}

impl PartialEq for SessionRef {
    fn eq(&self, other: &Self) -> bool {
        self.obj == other.obj
    }
}

impl IsAlive for SessionRef {
    fn alive(&self) -> bool {
        self.obj.is_alive()
    }
}

impl SessionRef {
    pub fn client(&self) -> Option<Client> { self.obj.client() }

    pub fn update_constraints(&self, constraints: BufferConstraints) {
        let mut inner = self.inner.lock().unwrap();

        if !self.obj.is_alive() || inner.stopped {
            return;
        }

        self.obj
            .buffer_size(constraints.size.w as u32, constraints.size.h as u32);

        for fmt in &constraints.shm {
            self.obj.shm_format(*fmt);
        }

        #[cfg(feature = "backend_drm")]
        if let Some(dma) = constraints.dma.as_ref() {
            let node = Vec::from(dma.node.dev_id().to_ne_bytes());
            self.obj.dmabuf_device(node);
            for (fmt, modifiers) in &dma.formats {
                let modifiers = modifiers
                    .iter()
                    .flat_map(|modifier| u64::from(*modifier).to_ne_bytes())
                    .collect::<Vec<u8>>();
                self.obj.dmabuf_format(*fmt as u32, modifiers);
            }
        }

        self.obj.done();
        inner.constraints = Some(constraints);
    }

    pub fn current_constraints(&self) -> Option<BufferConstraints> {
        self.inner.lock().unwrap().constraints.clone()
    }

    pub fn source(&self) -> ImageCaptureSource {
        self.inner.lock().unwrap().source.clone()
    }

    pub fn draw_cursor(&self) -> bool {
        self.inner.lock().unwrap().draw_cursors
    }

    pub fn user_data(&self) -> &UserDataMap {
        &self.user_data
    }
}

#[derive(Debug)]
pub struct Session(SessionRef);

impl ops::Deref for Session {
    type Target = SessionRef;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq<SessionRef> for Session {
    fn eq(&self, other: &SessionRef) -> bool {
        self.0 == *other
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut inner = self.0.inner.lock().unwrap();

        if inner.resource_destroyed || !self.0.obj.is_alive() || inner.stopped {
            return;
        }

        for frame in inner.active_frames.drain(..).filter_map(|frame| frame.upgrade()) {
            frame
                .inner
                .lock()
                .unwrap()
                .fail(&frame.obj, FailureReason::Stopped);
        }

        self.0.obj.stopped();
        inner.constraints.take();
        inner.stopped = true;
    }
}

impl Session {
    pub fn stop(self) {
        drop(self);
    }

    pub fn as_ref(&self) -> SessionRef {
        self.0.clone()
    }
}


#[derive(Debug)]
struct CursorSessionInner {
    cursor_obj: Option<wayland_server::Weak<ExtImageCopyCaptureCursorSessionV1>>,
    session_obj: Option<wayland_server::Weak<ExtImageCopyCaptureSessionV1>>,
    stopped: bool,
    resource_destroyed: bool,
    constraints: Option<BufferConstraints>,
    source: ImageCaptureSource,
    position: Option<crate::utils::Point<i32, BufferCoords>>,
    hotspot: crate::utils::Point<i32, BufferCoords>,
    active_frames: Vec<FrameTrack>,
}

impl CursorSessionInner {
    fn new(source: ImageCaptureSource) -> Self {
        Self {
            cursor_obj: None,
            session_obj: None,
            stopped: false,
            resource_destroyed: false,
            constraints: None,
            source,
            position: None,
            hotspot: crate::utils::Point::from((0, 0)),
            active_frames: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CursorSessionRef {
    obj: ExtImageCopyCaptureCursorSessionV1,
    inner: Arc<Mutex<CursorSessionInner>>,
    user_data: Arc<UserDataMap>,
}

impl PartialEq for CursorSessionRef {
    fn eq(&self, other: &Self) -> bool {
        self.obj == other.obj
    }
}

impl IsAlive for CursorSessionRef {
    fn alive(&self) -> bool {
        self.obj.is_alive()
    }
}

impl CursorSessionRef {
    pub fn client(&self) -> Option<Client> { self.obj.client() }

    pub fn update_constraints(&self, constraints: BufferConstraints) {
        let mut inner = self.inner.lock().unwrap();

        if !self.obj.is_alive() || inner.stopped {
            return;
        }

        if let Some(session_obj) = inner.session_obj.as_ref().and_then(|session| session.upgrade().ok()) {
            session_obj.buffer_size(constraints.size.w as u32, constraints.size.h as u32);
            for fmt in &constraints.shm {
                session_obj.shm_format(*fmt);
            }
            #[cfg(feature = "backend_drm")]
            if let Some(dma) = constraints.dma.as_ref() {
                let node = Vec::from(dma.node.dev_id().to_ne_bytes());
                session_obj.dmabuf_device(node);
                for (fmt, modifiers) in &dma.formats {
                    let modifiers = modifiers
                        .iter()
                        .flat_map(|modifier| u64::from(*modifier).to_ne_bytes())
                        .collect::<Vec<u8>>();
                    session_obj.dmabuf_format(*fmt as u32, modifiers);
                }
            }
            session_obj.done();
        }

        inner.constraints = Some(constraints);
    }

    pub fn current_constraints(&self) -> Option<BufferConstraints> {
        self.inner.lock().unwrap().constraints.clone()
    }

    pub fn source(&self) -> ImageCaptureSource {
        self.inner.lock().unwrap().source.clone()
    }

    pub fn has_cursor(&self) -> bool {
        self.inner.lock().unwrap().position.is_some()
    }

    pub fn set_cursor_pos(&self, position: Option<crate::utils::Point<i32, BufferCoords>>) {
        if !self.obj.is_alive() {
            return;
        }

        let mut inner = self.inner.lock().unwrap();

        if inner.position == position {
            return;
        }

        if inner.position.is_none() && position.is_some() {
            self.obj.enter();
            self.obj.hotspot(inner.hotspot.x, inner.hotspot.y);
        }

        if let Some(new_pos) = position {
            self.obj.position(new_pos.x, new_pos.y);
        } else if inner.position.is_some() {
            self.obj.leave();
        }

        inner.position = position;
    }

    pub fn set_cursor_hotspot(&self, hotspot: impl Into<crate::utils::Point<i32, BufferCoords>>) {
        if !self.obj.is_alive() {
            return;
        }

        let hotspot = hotspot.into();
        let mut inner = self.inner.lock().unwrap();

        if inner.hotspot == hotspot {
            return;
        }

        inner.hotspot = hotspot;
        if inner.position.is_some() {
            self.obj.hotspot(hotspot.x, hotspot.y);
        }
    }

    pub fn user_data(&self) -> &UserDataMap {
        &self.user_data
    }
}

#[derive(Debug)]
pub struct CursorSession(CursorSessionRef);

impl ops::Deref for CursorSession {
    type Target = CursorSessionRef;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq<CursorSessionRef> for CursorSession {
    fn eq(&self, other: &CursorSessionRef) -> bool {
        self.0 == *other
    }
}

impl Drop for CursorSession {
    fn drop(&mut self) {
        let mut inner = self.0.inner.lock().unwrap();

        if inner.resource_destroyed || !self.0.obj.is_alive() || inner.stopped {
            return;
        }

        if let Some(session_obj) = inner.session_obj.as_ref().and_then(|session| session.upgrade().ok()) {
            session_obj.stopped();
        }
        inner.constraints.take();

        for frame in inner.active_frames.drain(..).filter_map(|frame| frame.upgrade()) {
            frame
                .inner
                .lock()
                .unwrap()
                .fail(&frame.obj, FailureReason::Stopped);
        }

        inner.stopped = true;
    }
}

impl CursorSession {
    pub fn stop(self) {
        drop(self);
    }

    pub fn as_ref(&self) -> CursorSessionRef {
        self.0.clone()
    }
}


#[derive(Debug)]
struct FrameInner {
    constraints: Option<BufferConstraints>,
    buffer: Option<WlBuffer>,
    damage: Vec<Rectangle<i32, BufferCoords>>,
    capture_requested: bool,
    failed: Option<FailureReason>,
    ready: bool,
}

impl FrameInner {
    fn new(constraints: Option<BufferConstraints>) -> Self {
        Self {
            constraints,
            buffer: None,
            damage: Vec::new(),
            capture_requested: false,
            failed: None,
            ready: false,
        }
    }

    fn fail(&mut self, frame: &ExtImageCopyCaptureFrameV1, reason: FailureReason) {
        if self.ready || self.failed.is_some() {
            return;
        }
        self.failed = Some(reason);
        if self.capture_requested {
            frame.failed(reason);
        }
    }
}

#[derive(Clone, Debug)]
pub struct FrameRef {
    obj: ExtImageCopyCaptureFrameV1,
    inner: Arc<Mutex<FrameInner>>,
}

#[derive(Debug)]
struct FrameTrack {
    object: wayland_server::Weak<ExtImageCopyCaptureFrameV1>,
    inner: std::sync::Weak<Mutex<FrameInner>>,
}

impl FrameTrack {
    fn new(frame: &FrameRef) -> Self { Self { object: frame.obj.downgrade(), inner: Arc::downgrade(&frame.inner) } }
    fn upgrade(&self) -> Option<FrameRef> { Some(FrameRef { obj: self.object.upgrade().ok()?, inner: self.inner.upgrade()? }) }
}

impl PartialEq for FrameRef {
    fn eq(&self, other: &Self) -> bool {
        self.obj == other.obj
    }
}

impl FrameRef {
    pub fn client(&self) -> Option<Client> { self.obj.client() }

    pub fn buffer(&self) -> WlBuffer {
        self.inner
            .lock()
            .unwrap()
            .buffer
            .clone()
            .expect("no buffer attached")
    }

    pub fn damage(&self) -> Vec<Rectangle<i32, BufferCoords>> {
        self.inner.lock().unwrap().damage.clone()
    }

    pub fn has_failed(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        !self.obj.is_alive() || inner.failed.is_some() || inner.buffer.as_ref().is_some_and(|buffer| !buffer.is_alive())
    }
}

#[derive(Debug)]
pub struct Frame(FrameRef);

impl ops::Deref for Frame {
    type Target = FrameRef;
    fn deref(&self) -> &FrameRef {
        &self.0
    }
}

impl PartialEq<FrameRef> for Frame {
    fn eq(&self, other: &FrameRef) -> bool {
        self.0 == *other
    }
}

impl Frame {
    pub fn success(
        self,
        transform: impl Into<Transform>,
        damage: impl Into<Option<Vec<Rectangle<i32, BufferCoords>>>>,
        presented: impl Into<Duration>,
    ) {
        let size = {
            let inner = self.0.inner.lock().unwrap();
            if !inner.capture_requested || inner.failed.is_some() {
                return;
            }
            inner.constraints.as_ref().map(|constraints| constraints.size)
        };

        self.0.obj.transform(transform.into().into());
        let damage = damage.into().unwrap_or_else(|| size.map(Rectangle::from_size).into_iter().collect());
        for damage in damage {
            self.0
                .obj
                .damage(damage.loc.x, damage.loc.y, damage.size.w, damage.size.h);
        }

        let time = presented.into();
        let tv_sec_hi = (time.as_secs() >> 32) as u32;
        let tv_sec_lo = (time.as_secs() & 0xFFFFFFFF) as u32;
        let tv_nsec = time.subsec_nanos();
        self.0.obj.presentation_time(tv_sec_hi, tv_sec_lo, tv_nsec);

        self.0.inner.lock().unwrap().ready = true;
        self.0.obj.ready();

    }

    pub fn fail(self, reason: FailureReason) {
        self.0.inner.lock().unwrap().fail(&self.0.obj, reason);
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.0
            .inner
            .lock()
            .unwrap()
            .fail(&self.0.obj, FailureReason::Unknown);
    }
}


pub trait ImageCopyCaptureHandler:
    GlobalDispatch<ExtImageCopyCaptureManagerV1, ImageCopyCaptureGlobalData>
    + Dispatch<ExtImageCopyCaptureManagerV1, ()>
    + Dispatch<ExtImageCopyCaptureSessionV1, SessionData>
    + Dispatch<ExtImageCopyCaptureSessionV1, CursorSessionData>
    + Dispatch<ExtImageCopyCaptureCursorSessionV1, CursorSessionData>
    + Dispatch<ExtImageCopyCaptureFrameV1, FrameData>
    + 'static
{
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState;

    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints>;

    fn cursor_capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        let _ = source;
        None
    }

    fn new_session(&mut self, session: Session);

    fn new_cursor_session(&mut self, session: CursorSession) {
        let _ = session;
    }

    fn frame(&mut self, session: &SessionRef, frame: Frame);

    fn cursor_frame(&mut self, session: &CursorSessionRef, frame: Frame) {
        let _ = session;
        frame.fail(FailureReason::Unknown);
    }

    fn frame_aborted(&mut self, frame: FrameRef) {
        let _ = frame;
    }

    fn session_destroyed(&mut self, session: SessionRef) {
        let _ = session;
    }

    fn cursor_session_destroyed(&mut self, session: CursorSessionRef) {
        let _ = session;
    }
}


#[allow(missing_debug_implementations)]
pub struct ImageCopyCaptureGlobalData {
    filter: Box<dyn Fn(&Client) -> bool + Send + Sync>,
}

impl std::fmt::Debug for ImageCopyCaptureGlobalData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageCopyCaptureGlobalData")
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct SessionData {
    inner: Arc<Mutex<SessionInner>>,
    user_data: Arc<UserDataMap>,
}

#[derive(Debug)]
pub struct CursorSessionData {
    inner: Arc<Mutex<CursorSessionInner>>,
    user_data: Arc<UserDataMap>,
}

#[derive(Debug)]
pub struct FrameData {
    inner: Arc<Mutex<FrameInner>>,
    session: FrameSession,
}

#[derive(Debug)]
enum FrameSession { Normal(SessionRef), Cursor(Option<CursorSessionRef>) }

#[derive(Debug)]
pub struct ImageCopyCaptureState {
    global: GlobalId,
    sessions: Vec<SessionRef>,
    cursor_sessions: Vec<CursorSessionRef>,
}

impl ImageCopyCaptureState {
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: ImageCopyCaptureHandler,
    {
        Self::new_with_filter::<D, _>(display, |_| true)
    }

    pub fn new_with_filter<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: ImageCopyCaptureHandler,
        F: Fn(&Client) -> bool + Clone + Send + Sync + 'static,
    {
        let global = display.create_global::<D, ExtImageCopyCaptureManagerV1, _>(
            1,
            ImageCopyCaptureGlobalData {
                filter: Box::new(filter),
            },
        );

        Self {
            global,
            sessions: Vec::new(),
            cursor_sessions: Vec::new(),
        }
    }

    pub fn global(&self) -> GlobalId {
        self.global.clone()
    }

    pub fn sessions(&self) -> &[SessionRef] {
        &self.sessions
    }

    pub fn cursor_sessions(&self) -> &[CursorSessionRef] {
        &self.cursor_sessions
    }

    pub fn cleanup(&mut self) {
        self.sessions.retain(|s| s.alive());
        self.cursor_sessions.retain(|s| s.alive());
    }
}


impl<D> GlobalDispatch<ExtImageCopyCaptureManagerV1, ImageCopyCaptureGlobalData, D> for ImageCopyCaptureState
where
    D: ImageCopyCaptureHandler,
{
    fn bind(
        _state: &mut D,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ExtImageCopyCaptureManagerV1>,
        _global_data: &ImageCopyCaptureGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, global_data: &ImageCopyCaptureGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ExtImageCopyCaptureManagerV1, (), D> for ImageCopyCaptureState
where
    D: ImageCopyCaptureHandler,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &ExtImageCopyCaptureManagerV1,
        request: ext_image_copy_capture_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_manager_v1::Request::CreateSession {
                session,
                source,
                options,
            } => {
                state.image_copy_capture_state().cleanup();
                let limited = state.image_copy_capture_state().sessions.len() >= 64;
                let options = options.into_result();
                let invalid_options = options.is_err();
                let Some(capture_source) = ImageCaptureSource::from_resource(&source).filter(|_| !limited) else {
                    let inner = Arc::new(Mutex::new(SessionInner {
                        stopped: true,
                        resource_destroyed: false,
                        constraints: None,
                        draw_cursors: false,
                        source: ImageCaptureSource::new(),
                        active_frames: Vec::new(),
                    }));
                    let user_data = Arc::new(UserDataMap::new());
                    let obj = data_init.init(
                        session,
                        SessionData {
                            inner: inner.clone(),
                            user_data: user_data.clone(),
                        },
                    );
                    obj.stopped();
                    if limited { _resource.post_error(0u32, "Park capture session limit reached"); }
                    else if invalid_options { _resource.post_error(ext_image_copy_capture_manager_v1::Error::InvalidOption, "Invalid capture options"); }
                    return;
                };

                let draw_cursors = options
                    .map(|o| o.contains(ext_image_copy_capture_manager_v1::Options::PaintCursors))
                    .unwrap_or(false);
                let inner = Arc::new(Mutex::new(SessionInner::new(
                    capture_source.clone(),
                    draw_cursors,
                )));
                let user_data = Arc::new(UserDataMap::new());

                let obj = data_init.init(
                    session,
                    SessionData {
                        inner: inner.clone(),
                        user_data: user_data.clone(),
                    },
                );

                let session_ref = SessionRef {
                    obj,
                    inner,
                    user_data,
                };

                if invalid_options {
                    _resource.post_error(ext_image_copy_capture_manager_v1::Error::InvalidOption, "Invalid capture options");
                    return;
                }

                if let Some(constraints) = state.capture_constraints(&capture_source) {
                    session_ref.update_constraints(constraints);
                    state
                        .image_copy_capture_state()
                        .sessions
                        .push(session_ref.clone());
                    state.new_session(Session(session_ref));
                } else {
                    session_ref.obj.stopped();
                    session_ref.inner.lock().unwrap().stopped = true;
                }
            }
            ext_image_copy_capture_manager_v1::Request::CreatePointerCursorSession {
                session,
                source,
                pointer: _,
            } => {
                state.image_copy_capture_state().cleanup();
                let limited = state.image_copy_capture_state().cursor_sessions.len() >= 64;
                let Some(capture_source) = ImageCaptureSource::from_resource(&source).filter(|_| !limited) else {
                    let inner = Arc::new(Mutex::new(CursorSessionInner::new(ImageCaptureSource::new())));
                    inner.lock().unwrap().stopped = true;
                    let user_data = Arc::new(UserDataMap::new());
                    let obj = data_init.init(session, CursorSessionData { inner: inner.clone(), user_data });
                    inner.lock().unwrap().cursor_obj = Some(obj.downgrade());
                    if limited { _resource.post_error(0u32, "Park cursor capture session limit reached"); }
                    return;
                };

                let inner = Arc::new(Mutex::new(CursorSessionInner::new(capture_source.clone())));
                let user_data = Arc::new(UserDataMap::new());

                let obj = data_init.init(
                    session,
                    CursorSessionData {
                        inner: inner.clone(),
                        user_data: user_data.clone(),
                    },
                );

                let session_ref = CursorSessionRef {
                    obj,
                    inner,
                    user_data,
                };
                session_ref.inner.lock().unwrap().cursor_obj = Some(session_ref.obj.downgrade());

                if let Some(constraints) = state.cursor_capture_constraints(&capture_source) {
                    session_ref.update_constraints(constraints);
                    state
                        .image_copy_capture_state()
                        .cursor_sessions
                        .push(session_ref.clone());
                    state.new_cursor_session(CursorSession(session_ref));
                } else {
                    session_ref.inner.lock().unwrap().stopped = true;
                }
            }
            ext_image_copy_capture_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch<ExtImageCopyCaptureSessionV1, SessionData, D> for ImageCopyCaptureState
where
    D: ImageCopyCaptureHandler,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        resource: &ExtImageCopyCaptureSessionV1,
        request: ext_image_copy_capture_session_v1::Request,
        data: &SessionData,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_session_v1::Request::CreateFrame { frame } => {
                let constraints = data.inner.lock().unwrap().constraints.clone();
                let inner = Arc::new(Mutex::new(FrameInner::new(constraints)));
                let session = FrameSession::Normal(SessionRef { obj: resource.clone(), inner: data.inner.clone(), user_data: data.user_data.clone() });
                let obj = data_init.init(frame, FrameData { inner: inner.clone(), session });
                data.inner.lock().unwrap().active_frames.retain(|frame| frame.upgrade().is_some());
                if !data.inner.lock().unwrap().active_frames.is_empty() {
                    resource.post_error(ext_image_copy_capture_session_v1::Error::DuplicateFrame, "Destroy the previous capture frame first");
                    return;
                }
                if data.inner.lock().unwrap().stopped { inner.lock().unwrap().failed = Some(FailureReason::Stopped); }
                data.inner
                    .lock()
                    .unwrap()
                    .active_frames
                    .push(FrameTrack::new(&FrameRef { obj, inner }));
            }
            ext_image_copy_capture_session_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: wayland_server::backend::ClientId,
        resource: &ExtImageCopyCaptureSessionV1,
        data: &SessionData,
    ) {
        data.inner.lock().unwrap().resource_destroyed = true;
        let session_ref = SessionRef {
            obj: resource.clone(),
            inner: data.inner.clone(),
            user_data: data.user_data.clone(),
        };
        state.session_destroyed(session_ref);
    }
}

impl<D> Dispatch<ExtImageCopyCaptureSessionV1, CursorSessionData, D> for ImageCopyCaptureState
where
    D: ImageCopyCaptureHandler,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        resource: &ExtImageCopyCaptureSessionV1,
        request: ext_image_copy_capture_session_v1::Request,
        data: &CursorSessionData,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_session_v1::Request::CreateFrame { frame } => {
                let constraints = data.inner.lock().unwrap().constraints.clone();
                let inner = Arc::new(Mutex::new(FrameInner::new(constraints)));
                let parent = data.inner.lock().unwrap().cursor_obj.as_ref().and_then(|parent| parent.upgrade().ok());
                let session = FrameSession::Cursor(parent.map(|obj| CursorSessionRef { obj, inner: data.inner.clone(), user_data: data.user_data.clone() }));
                let obj = data_init.init(frame, FrameData { inner: inner.clone(), session });
                data.inner.lock().unwrap().active_frames.retain(|frame| frame.upgrade().is_some());
                if !data.inner.lock().unwrap().active_frames.is_empty() {
                    resource.post_error(ext_image_copy_capture_session_v1::Error::DuplicateFrame, "Destroy the previous capture frame first");
                    return;
                }
                if data.inner.lock().unwrap().stopped { inner.lock().unwrap().failed = Some(FailureReason::Stopped); }
                data.inner
                    .lock()
                    .unwrap()
                    .active_frames
                    .push(FrameTrack::new(&FrameRef { obj, inner }));
            }
            ext_image_copy_capture_session_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch<ExtImageCopyCaptureCursorSessionV1, CursorSessionData, D> for ImageCopyCaptureState
where
    D: ImageCopyCaptureHandler,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        resource: &ExtImageCopyCaptureCursorSessionV1,
        request: ext_image_copy_capture_cursor_session_v1::Request,
        data: &CursorSessionData,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_cursor_session_v1::Request::GetCaptureSession { session } => {
                let mut inner = data.inner.lock().unwrap();

                if inner.session_obj.is_some() {
                    data_init.init(session, CursorSessionData { inner: data.inner.clone(), user_data: data.user_data.clone() });
                    resource.post_error(ext_image_copy_capture_cursor_session_v1::Error::DuplicateSession, "A cursor capture session already exists");
                    return;
                }

                let obj = data_init.init(
                    session,
                    CursorSessionData {
                        inner: data.inner.clone(),
                        user_data: data.user_data.clone(),
                    },
                );

                if inner.stopped {
                    obj.stopped();
                } else if let Some(constraints) = inner.constraints.as_ref() {
                    obj.buffer_size(constraints.size.w as u32, constraints.size.h as u32);
                    for fmt in &constraints.shm {
                        obj.shm_format(*fmt);
                    }
                    #[cfg(feature = "backend_drm")]
                    if let Some(dma) = constraints.dma.as_ref() {
                        let node = Vec::from(dma.node.dev_id().to_ne_bytes());
                        obj.dmabuf_device(node);
                        for (fmt, modifiers) in &dma.formats {
                            let modifiers = modifiers
                                .iter()
                                .flat_map(|modifier| u64::from(*modifier).to_ne_bytes())
                                .collect::<Vec<u8>>();
                            obj.dmabuf_format(*fmt as u32, modifiers);
                        }
                    }
                    obj.done();
                }

                inner.session_obj = Some(obj.downgrade());
            }
            ext_image_copy_capture_cursor_session_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: wayland_server::backend::ClientId,
        resource: &ExtImageCopyCaptureCursorSessionV1,
        data: &CursorSessionData,
    ) {
        data.inner.lock().unwrap().resource_destroyed = true;
        let session_ref = CursorSessionRef {
            obj: resource.clone(),
            inner: data.inner.clone(),
            user_data: data.user_data.clone(),
        };
        state.cursor_session_destroyed(session_ref);
    }
}

impl<D> Dispatch<ExtImageCopyCaptureFrameV1, FrameData, D> for ImageCopyCaptureState
where
    D: ImageCopyCaptureHandler,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &ExtImageCopyCaptureFrameV1,
        request: ext_image_copy_capture_frame_v1::Request,
        data: &FrameData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_frame_v1::Request::AttachBuffer { buffer } => {
                let mut inner = data.inner.lock().unwrap();
                if inner.capture_requested {
                    resource.post_error(ext_image_copy_capture_frame_v1::Error::AlreadyCaptured, "Capture was already requested");
                    return;
                }
                inner.buffer = Some(buffer);
            }
            ext_image_copy_capture_frame_v1::Request::DamageBuffer { x, y, width, height } => {
                let mut inner = data.inner.lock().unwrap();
                if inner.capture_requested {
                    resource.post_error(ext_image_copy_capture_frame_v1::Error::AlreadyCaptured, "Capture was already requested");
                    return;
                }
                if x < 0 || y < 0 || width <= 0 || height <= 0 || x.checked_add(width).is_none() || y.checked_add(height).is_none() {
                    resource.post_error(ext_image_copy_capture_frame_v1::Error::InvalidBufferDamage, "Invalid capture damage rectangle");
                    return;
                }
                if inner.damage.len() >= 256 {
                    if let Some(constraints) = &inner.constraints {
                        let size = constraints.size;
                        inner.damage.clear();
                        inner.damage.push(Rectangle::from_size(size));
                    }
                    return;
                }
                inner.damage.push(Rectangle {
                    loc: (x, y).into(),
                    size: (width, height).into(),
                });
            }
            ext_image_copy_capture_frame_v1::Request::Capture => {
                let frame_ref = FrameRef {
                    obj: resource.clone(),
                    inner: data.inner.clone(),
                };

                {
                    let mut inner = data.inner.lock().unwrap();
                    if inner.capture_requested {
                        resource.post_error(ext_image_copy_capture_frame_v1::Error::AlreadyCaptured, "Capture was already requested");
                        return;
                    }
                    inner.capture_requested = true;
                    if let Some(reason) = inner.failed { resource.failed(reason); return; }

                    let buffer = match inner.buffer.as_ref() {
                        Some(b) => b,
                        None => {
                            resource.post_error(ext_image_copy_capture_frame_v1::Error::NoBuffer, "Attach a buffer before capture");
                            return;
                        }
                    };

                    match inner.constraints.as_ref() {
                        Some(constraints) => {
                            if let Err(reason) = validate_buffer(buffer, constraints) {
                                inner.fail(resource, reason);
                                return;
                            }
                        }
                        None => {
                            inner.fail(resource, FailureReason::BufferConstraints);
                            return;
                        }
                    }
                }

                match &data.session {
                    FrameSession::Normal(session) => state.frame(session, Frame(frame_ref)),
                    FrameSession::Cursor(Some(session)) => state.cursor_frame(session, Frame(frame_ref)),
                    FrameSession::Cursor(None) => data.inner.lock().unwrap().fail(resource, FailureReason::Stopped),
                }
            }
            ext_image_copy_capture_frame_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: wayland_server::backend::ClientId,
        resource: &ExtImageCopyCaptureFrameV1,
        data: &FrameData,
    ) {
        let frame_ref = FrameRef {
            obj: resource.clone(),
            inner: data.inner.clone(),
        };

        match &data.session {
            FrameSession::Normal(session) => session.inner.lock().unwrap().active_frames.retain(|frame| frame.upgrade().is_some_and(|frame| frame != frame_ref)),
            FrameSession::Cursor(Some(session)) => session.inner.lock().unwrap().active_frames.retain(|frame| frame.upgrade().is_some_and(|frame| frame != frame_ref)),
            FrameSession::Cursor(None) => {}
        }

        state.frame_aborted(frame_ref);
    }
}


#[macro_export]
macro_rules! delegate_image_copy_capture {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        const _: () = {
            use $crate::reexports::wayland_protocols::ext::image_copy_capture::v1::server::{
                ext_image_copy_capture_cursor_session_v1::ExtImageCopyCaptureCursorSessionV1,
                ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
                ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
                ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
            };
            use $crate::reexports::wayland_server::{delegate_dispatch, delegate_global_dispatch};
            use $crate::wayland::image_copy_capture::{
                ImageCopyCaptureGlobalData, ImageCopyCaptureState,
                SessionData, CursorSessionData, FrameData,
            };

            delegate_global_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtImageCopyCaptureManagerV1: ImageCopyCaptureGlobalData] => ImageCopyCaptureState
            );
            delegate_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtImageCopyCaptureManagerV1: ()] => ImageCopyCaptureState
            );
            delegate_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtImageCopyCaptureSessionV1: SessionData] => ImageCopyCaptureState
            );
            delegate_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtImageCopyCaptureSessionV1: CursorSessionData] => ImageCopyCaptureState
            );
            delegate_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtImageCopyCaptureCursorSessionV1: CursorSessionData] => ImageCopyCaptureState
            );
            delegate_dispatch!(
                $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                $ty: [ExtImageCopyCaptureFrameV1: FrameData] => ImageCopyCaptureState
            );
        };
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use wayland_server::{Display, backend::{ClientData, ClientId, DisconnectReason}};

    struct State;
    #[derive(Debug)]
    struct ClientState;
    impl ClientData for ClientState {
        fn initialized(&self, _: ClientId) {}
        fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
    }

    impl Dispatch<ExtImageCopyCaptureSessionV1, SessionData> for State {
        fn request(_: &mut Self, _: &Client, _: &ExtImageCopyCaptureSessionV1, _: ext_image_copy_capture_session_v1::Request,
            _: &SessionData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
    }

    impl Dispatch<ExtImageCopyCaptureFrameV1, FrameData> for State {
        fn request(_: &mut Self, _: &Client, _: &ExtImageCopyCaptureFrameV1, _: ext_image_copy_capture_frame_v1::Request,
            _: &FrameData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
    }

    #[test]
    fn completed_frames_release_their_data_after_protocol_resource_destruction() {
        for success in [false, true] {
            let mut display = Display::<State>::new().unwrap();
            let mut handle = display.handle();
            let (server, _peer) = UnixStream::pair().unwrap();
            let client = handle.insert_client(server, Arc::new(ClientState)).unwrap();
            let inner = Arc::new(Mutex::new(SessionInner::new(ImageCaptureSource::new(), false)));
            let user_data = Arc::new(UserDataMap::new());
            let session = client.create_resource::<ExtImageCopyCaptureSessionV1, _, State>(&handle, 1,
                SessionData { inner: inner.clone(), user_data: user_data.clone() }).unwrap();
            let session = SessionRef { obj: session, inner: inner.clone(), user_data };
            let frame_inner = Arc::new(Mutex::new(FrameInner::new(Some(BufferConstraints {
                size: (1, 1).into(), shm: vec![wl_shm::Format::Argb8888],
                #[cfg(feature = "backend_drm")]
                dma: None,
            }))));
            frame_inner.lock().unwrap().capture_requested = true;
            let weak = Arc::downgrade(&frame_inner);
            let object = client.create_resource::<ExtImageCopyCaptureFrameV1, _, State>(&handle, 1,
                FrameData { inner: frame_inner.clone(), session: FrameSession::Normal(session.clone()) }).unwrap();
            let frame = Frame(FrameRef { obj: object.clone(), inner: frame_inner });
            inner.lock().unwrap().active_frames.push(FrameTrack::new(&frame));
            if success { frame.success(Transform::Normal, None, Duration::ZERO); }
            else { frame.fail(FailureReason::Unknown); }
            handle.backend_handle().destroy_object::<State>(&object.id()).unwrap();
            drop(object);
            if let Err(error) = display.backend().dispatch_single_client(&mut State, client.id()) {
                assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
            }
            assert!(weak.upgrade().is_none());
            assert!(inner.lock().unwrap().active_frames.iter().all(|frame| frame.upgrade().is_none()));
        }
    }
}

