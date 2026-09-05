use crate::{
    primitive_gallery::{GalleryVisualState, PRIMITIVE_COUNT},
    retained_ui::RetainedScene,
    sand::SandElement,
    style::{ResolvedStyle, StyleError},
};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

const MAX_NODES: usize = 256;

const SHADER: &str = r#"
struct Node {
    center_half: vec4<f32>,
    color: vec4<f32>,
    style: vec4<f32>,
    border_color: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> nodes: array<Node>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) style: vec4<f32>,
    @location(3) border_color: vec4<f32>,
}

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let node = nodes[instance_index];
    let local = corners[vertex_index];
    var output: VertexOutput;
    output.position = vec4<f32>(node.center_half.xy + local * node.center_half.zw, 0.0, 1.0);
    output.local = local;
    output.color = node.color;
    output.style = node.style;
    output.border_color = node.border_color;
    return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let radius = clamp(input.style.x, 0.02, 0.96);
    let point = abs(input.local) - vec2<f32>(1.0 - radius);
    let distance = length(max(point, vec2<f32>(0.0))) + min(max(point.x, point.y), 0.0) - radius;
    let alpha = 1.0 - smoothstep(-0.025, 0.025, distance);
    let border = smoothstep(-0.18, -0.04, distance);
    let surface = mix(input.color, input.border_color, border);
    return vec4<f32>(surface.rgb, surface.a * alpha);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct NodeInstance {
    center_half: [f32; 4],
    color: [f32; 4],
    style: [f32; 4],
    border_color: [f32; 4],
}

pub struct NodeLayer {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    count: u32,
}

impl NodeLayer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let instances = vec![NodeInstance::zeroed(); MAX_NODES];
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lince-joined-node-buffer"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lince-joined-node-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lince-joined-node-bind-group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lince-joined-node-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lince-joined-node-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lince-joined-node-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            buffer,
            bind_group,
            pipeline,
            count: 0,
        }
    }

    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        positions: &[[f32; 2]],
        style: &ResolvedStyle,
        gallery_focus: usize,
        gallery_state: GalleryVisualState,
        gallery_active: bool,
    ) -> Result<(), StyleError> {
        let count = positions.len().min(MAX_NODES);
        let colors = [
            style.color_linear("--lynx-need")?,
            style.color_linear("--lynx-contribution")?,
            style.color_linear("--lynx-info")?,
            style.color_linear("--lynx-accent")?,
        ];
        let border_color = style.color_linear("--lynx-border")?;
        let accent = style.color_linear("--lynx-accent")?;
        let danger = style.color_linear("--lynx-danger")?;
        let surface = style.color_linear("--lynx-surface-raised")?;
        let hover = style.color_linear("--lynx-surface-hover")?;
        let density_scale = style.scalar("--lynx-density-scale")?;
        let radius = (style.length_px("--lynx-radius-control")? / 12.0).clamp(0.02, 0.96);
        let instances = positions
            .iter()
            .take(count)
            .enumerate()
            .map(|(index, position)| {
                let column = index % 4;
                let color = colors[column];
                let mut instance = NodeInstance {
                    center_half: [
                        (position[0] / 1_100.0).clamp(-0.96, 0.96) - 0.42,
                        (position[1] / 650.0).clamp(-0.93, 0.93),
                        0.035 * density_scale,
                        0.024 * density_scale,
                    ],
                    color,
                    style: [radius, 0.0, 0.0, 0.0],
                    border_color,
                };
                if index < PRIMITIVE_COUNT {
                    let column = index % 2;
                    let row = index / 2;
                    instance.center_half = [
                        if column == 0 { -0.77 } else { -0.31 },
                        0.69 - row as f32 * 0.055,
                        0.21,
                        0.022,
                    ];
                    instance.color = if index == gallery_focus {
                        match gallery_state {
                            GalleryVisualState::Hover => hover,
                            GalleryVisualState::Invalid => danger,
                            GalleryVisualState::Active | GalleryVisualState::Selected => accent,
                            _ => surface,
                        }
                    } else {
                        surface
                    };
                    instance.border_color = if gallery_active && index == gallery_focus {
                        accent
                    } else {
                        border_color
                    };
                }
                instance
            })
            .collect::<Vec<_>>();
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&instances));
        self.count = count as u32;
        Ok(())
    }

    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..self.count);
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn update_retained(
        &mut self,
        queue: &wgpu::Queue,
        scene: &RetainedScene,
        viewport: [u32; 2],
        style: &ResolvedStyle,
    ) -> Result<(), StyleError> {
        let canvas = style.color_linear("--lynx-surface-canvas")?;
        let raised = style.color_linear("--lynx-surface-raised")?;
        let input = style.color_linear("--lynx-surface-input")?;
        let hover = style.color_linear("--lynx-surface-hover")?;
        let accent = style.color_linear("--lynx-accent")?;
        let border = style.color_linear("--lynx-border")?;
        let focus = style.color_linear("--lynx-focus")?;
        let radius = (style.length_px("--lynx-radius-control")? / 12.0).clamp(0.02, 0.96);
        let width = viewport[0].max(1) as f32;
        let height = viewport[1].max(1) as f32;
        let instances = scene
            .nodes
            .iter()
            .take(MAX_NODES)
            .map(|node| {
                let color = match node.element {
                    SandElement::Compound => canvas,
                    SandElement::Card | SandElement::Panel | SandElement::Stack => raised,
                    SandElement::Field
                    | SandElement::Textarea
                    | SandElement::Select
                    | SandElement::Checkbox
                    | SandElement::Radio => input,
                    SandElement::Button | SandElement::Icon if node.focused => accent,
                    SandElement::Button | SandElement::Icon => hover,
                    SandElement::ValidationMessage => {
                        style.color_linear("--lynx-danger").unwrap_or(accent)
                    }
                    _ => raised,
                };
                let center_x = node.rect.x + node.rect.width * 0.5;
                let center_y = node.rect.y + node.rect.height * 0.5;
                NodeInstance {
                    center_half: [
                        center_x * 2.0 / width - 1.0,
                        1.0 - center_y * 2.0 / height,
                        node.rect.width / width,
                        node.rect.height / height,
                    ],
                    color,
                    style: [radius, 0.0, 0.0, 0.0],
                    border_color: if node.focused { focus } else { border },
                }
            })
            .collect::<Vec<_>>();
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&instances));
        self.count = instances.len() as u32;
        Ok(())
    }
}
