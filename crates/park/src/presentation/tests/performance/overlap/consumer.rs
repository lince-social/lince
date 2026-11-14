use super::*;
use smithay::{
    backend::{
        allocator::{Buffer, Fourcc},
        egl::{EGLContext, EGLDevice, EGLDisplay},
        renderer::{
            Bind, ExportMem, Frame as _, ImportDma, Offscreen, Renderer,
            gles::{GlesRenderbuffer, GlesRenderer},
        },
    },
    utils::{Physical, Rectangle, Size, Transform},
};

pub(super) fn process_cpu_ns() -> u64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    u64::try_from(i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec)).unwrap()
}

pub(super) struct Consumer {
    renderer: GlesRenderer,
    targets: [GlesRenderbuffer; 2],
    item: ItemId,
    original: bevy::math::DVec2,
    secondary_reference: Option<u64>,
}

impl Consumer {
    pub(super) fn new(warm: &Frame, item: ItemId, original: bevy::math::DVec2) -> Self {
        assert!(warm.ready().unwrap());
        let device = EGLDevice::enumerate()
            .unwrap()
            .find(|device| !device.is_software())
            .expect("A hardware EGL device is required");
        let display = unsafe { EGLDisplay::new(device) }.unwrap();
        let context = EGLContext::new(&display).unwrap();
        let mut renderer = unsafe { GlesRenderer::new(context) }.unwrap();
        let targets = std::array::from_fn(|index| {
            let buffer = output_buffer(warm, OutputId(index as u64 + 1));
            assert_eq!(buffer.size(), (640, 480).into());
            renderer
                .create_buffer(Fourcc::Abgr8888, buffer.size())
                .unwrap()
        });
        let mut consumer = Self {
            renderer,
            targets,
            item,
            original,
            secondary_reference: None,
        };
        consumer.consume(warm, Instant::now());
        let pixels = consumer.pixels();
        for (index, bytes) in pixels.iter().enumerate() {
            assert_eq!(
                bytes,
                &probe_pixels(output_buffer(warm, OutputId(index as u64 + 1))).unwrap(),
                "The warmed GLES consumer does not match the exported output {}",
                index + 1
            );
        }
        consumer
    }

    pub(super) fn consume(&mut self, frame: &Frame, origin: Instant) -> serde_json::Value {
        assert!(frame.ready().unwrap());
        let started = origin.elapsed().as_secs_f64() * 1000.0;
        let mut outputs = Vec::with_capacity(2);
        let mut completions = Vec::with_capacity(2);
        let mut textures = Vec::with_capacity(2);
        for (index, target) in self.targets.iter_mut().enumerate() {
            let id = OutputId(index as u64 + 1);
            let buffer = output_buffer(frame, id);
            assert_eq!(buffer.size(), (640, 480).into());
            let imported = origin.elapsed().as_secs_f64() * 1000.0;
            let texture = self.renderer.import_dmabuf(buffer, None).unwrap();
            let drawing = origin.elapsed().as_secs_f64() * 1000.0;
            let size: Size<i32, Physical> = (640, 480).into();
            let damage = Rectangle::from_size(size);
            let mut framebuffer = self.renderer.bind(target).unwrap();
            let mut draw = self
                .renderer
                .render(&mut framebuffer, size, Transform::Normal)
                .unwrap();
            draw.clear([0.0, 0.0, 0.0, 0.0].into(), &[damage]).unwrap();
            draw.render_texture_at(
                &texture,
                (0, 0).into(),
                1,
                1.0,
                Transform::Normal,
                &[damage],
                &[],
                1.0,
            )
            .unwrap();
            completions.push(draw.finish().unwrap());
            textures.push(texture);
            outputs.push(
                serde_json::json!({"output":id.0,"import_ms":[imported,drawing],
                "draw_submit_ms":[drawing,origin.elapsed().as_secs_f64()*1000.0]}),
            );
        }
        let submitted = origin.elapsed().as_secs_f64() * 1000.0;
        for completion in completions {
            completion.wait().unwrap();
        }
        let completed = origin.elapsed().as_secs_f64() * 1000.0;
        drop(textures);
        serde_json::json!({"started_ms":started,"submitted_ms":submitted,"completed_ms":completed,
            "fence_wait_ms":completed-submitted,"outputs":outputs})
    }

    fn pixels(&mut self) -> [Vec<u8>; 2] {
        std::array::from_fn(|index| {
            let mut framebuffer = self.renderer.bind(&mut self.targets[index]).unwrap();
            let mapping = self
                .renderer
                .copy_framebuffer(
                    &mut framebuffer,
                    Rectangle::from_size((640, 480).into()),
                    Fourcc::Abgr8888,
                )
                .unwrap();
            self.renderer.map_texture(&mapping).unwrap().to_vec()
        })
    }

    pub(super) fn check(&mut self, frame: &Frame, sample: &serde_json::Value) -> serde_json::Value {
        let [primary, secondary] = self.pixels();
        check_frame_pixels(
            frame,
            sample,
            self.item,
            self.original,
            1.0,
            "gles-consumer",
            &mut self.secondary_reference,
            [&primary, &secondary],
        )
    }
}
