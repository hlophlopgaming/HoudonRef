//! GPU side of the scene: pipeline, per-model meshes and per-object data.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::{self, util::DeviceExt as _};
use glam::Vec3;
use houdon_core::{Light, LightKind, MAX_LIGHTS, ModelId, Vertex};

use crate::Frame;

/// Neutral light-grey clay, linear.
pub const CLAY_COLOR: Vec3 = Vec3::new(0.62, 0.6, 0.57);

/// Colour mixed into a selected object, linear, and how much of it. A cool
/// interface blue, so the highlight is not mistaken for a material.
const SELECTION_COLOR: [f32; 4] = [0.15, 0.45, 1.0, 0.35];

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    /// rgb: ambient colour from above (the backdrop); a: ambient intensity.
    sky_color: [f32; 4],
    /// rgb: ambient colour from below (the floor).
    ground_color: [f32; 4],
    /// x: number of lights; y: 1 if the output needs gamma encoding.
    params: [u32; 4],
    lights: [GpuLight; MAX_LIGHTS],
}

/// `spot.w` of a light without a cone; the shader tests `spot.w > -1.5`.
const NO_CONE: f32 = -2.0;

/// One light as the shader sees it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuLight {
    /// xyz: position (w = 1) or direction towards the light (w = 0).
    position: [f32; 4],
    /// rgb: colour times intensity.
    color: [f32; 4],
    /// xyz: direction the spot points; w: cosine of the outer half angle,
    /// or -2 for lights without a cone.
    spot: [f32; 4],
    /// x: cosine of the inner half angle.
    spot_inner: [f32; 4],
}

impl GpuLight {
    fn new(light: &Light, centre: Vec3) -> Self {
        let color = (light.color * light.intensity).extend(0.0).to_array();
        match light.kind {
            LightKind::Directional => Self {
                position: light.direction().extend(0.0).to_array(),
                color,
                spot: [0.0, 0.0, 0.0, NO_CONE],
                spot_inner: [0.0; 4],
            },
            LightKind::Point | LightKind::Spot => {
                let position = light.position(centre);
                let (spot, spot_inner) = if light.kind == LightKind::Spot {
                    let (outer, inner) = light.cone_cosines();
                    let aim = (centre - position).normalize_or(Vec3::NEG_Y);
                    (aim.extend(outer).to_array(), [inner, 0.0, 0.0, 0.0])
                } else {
                    ([0.0, 0.0, 0.0, NO_CONE], [0.0; 4])
                };
                Self {
                    position: position.extend(1.0).to_array(),
                    color,
                    spot,
                    spot_inner,
                }
            }
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ObjectUniforms {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    /// rgb: highlight colour; a: how much of it to mix in.
    highlight: [f32; 4],
    /// rgb: surface colour below `color_below.a` (world height).
    color_below: [f32; 4],
    /// rgb: surface colour above `color_above.a` (world height).
    color_above: [f32; 4],
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// GPU resources for drawing frames. One renders the viewport (stored in
/// egui-wgpu's callback resources); offscreen pictures own another.
pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    object_layout: wgpu::BindGroupLayout,
    object_buffer: wgpu::Buffer,
    object_bind_group: wgpu::BindGroup,
    /// Bytes between two objects' uniforms, respecting the device's alignment.
    object_stride: u64,
    object_capacity: u64,
    gamma_encode: bool,
    meshes: HashMap<ModelId, GpuMesh>,
    /// Model of each object drawn by the last prepared frame.
    draw_list: Vec<ModelId>,
}

impl SceneRenderer {
    /// Creates the viewport renderer and registers it with egui-wgpu. Call
    /// once at startup. `depth_format` must match eframe's depth buffer.
    pub fn install(
        render_state: &egui_wgpu::RenderState,
        depth_format: Option<wgpu::TextureFormat>,
    ) {
        let renderer = Self::new(
            &render_state.device,
            render_state.target_format,
            depth_format,
        );
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(renderer);
    }

    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("houdon_scene_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let globals_layout =
            uniform_layout(device, "houdon_globals_layout", size_of::<Globals>(), false);
        let object_layout = uniform_layout(
            device,
            "houdon_object_layout",
            size_of::<ObjectUniforms>(),
            true,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("houdon_scene_pipeline_layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&object_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("houdon_scene_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // Imported meshes are often open or single-sided; draw both
                // sides and flip the normal in the shader.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("houdon_globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("houdon_globals_bind_group"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let object_stride = (size_of::<ObjectUniforms>() as u64).div_ceil(alignment) * alignment;
        let object_capacity = 16;
        let (object_buffer, object_bind_group) =
            object_resources(device, &object_layout, object_stride, object_capacity);

        Self {
            pipeline,
            globals_buffer,
            globals_bind_group,
            object_layout,
            object_buffer,
            object_bind_group,
            object_stride,
            object_capacity,
            gamma_encode: !target_format.is_srgb(),
            meshes: HashMap::new(),
            draw_list: Vec::new(),
        }
    }

    /// Uploads what `frame` needs. Call before [`Self::draw`] in the same frame.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame) {
        // Meshes: upload new models, drop models no longer drawn.
        self.meshes
            .retain(|id, _| frame.objects.iter().any(|o| o.model_id == *id));
        for object in &frame.objects {
            self.meshes
                .entry(object.model_id)
                .or_insert_with(|| upload_mesh(device, &object.model));
        }

        // Per-object uniforms.
        let count = frame.objects.len() as u64;
        if count > self.object_capacity {
            self.object_capacity = count.next_power_of_two();
            (self.object_buffer, self.object_bind_group) = object_resources(
                device,
                &self.object_layout,
                self.object_stride,
                self.object_capacity,
            );
        }
        let mut bytes = vec![0u8; (self.object_stride * count) as usize];
        for (i, object) in frame.objects.iter().enumerate() {
            let uniforms = ObjectUniforms {
                model: object.transform.to_cols_array_2d(),
                normal: object.transform.inverse().transpose().to_cols_array_2d(),
                color_below: object.colors.below.extend(object.colors.from_y).to_array(),
                color_above: object.colors.above.extend(object.colors.to_y).to_array(),
                highlight: if object.selected {
                    SELECTION_COLOR
                } else {
                    [0.0; 4]
                },
            };
            let start = i * self.object_stride as usize;
            bytes[start..start + size_of::<ObjectUniforms>()]
                .copy_from_slice(bytemuck::bytes_of(&uniforms));
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.object_buffer, 0, &bytes);
        }
        self.draw_list = frame.objects.iter().map(|o| o.model_id).collect();

        let mut lights = [GpuLight::zeroed(); MAX_LIGHTS];
        let count = frame.lights.len().min(MAX_LIGHTS);
        for (slot, light) in lights.iter_mut().zip(&frame.lights) {
            *slot = GpuLight::new(light, frame.light_centre);
        }
        let room = &frame.room;
        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            sky_color: room
                .backdrop_color
                .extend(room.ambient_intensity)
                .to_array(),
            ground_color: room.floor_color.extend(0.0).to_array(),
            params: [count as u32, u32::from(self.gamma_encode), 0, 0],
            lights,
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
    }

    /// Draws the last prepared frame into `render_pass`.
    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        for (i, id) in self.draw_list.iter().enumerate() {
            let Some(mesh) = self.meshes.get(id) else {
                continue;
            };
            let offset = (i as u64 * self.object_stride) as u32;
            render_pass.set_bind_group(1, &self.object_bind_group, &[offset]);
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
    }
}

fn uniform_layout(
    device: &wgpu::Device,
    label: &str,
    size: usize,
    dynamic: bool,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: std::num::NonZeroU64::new(size as u64),
            },
            count: None,
        }],
    })
}

fn object_resources(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    stride: u64,
    capacity: u64,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("houdon_objects"),
        size: stride * capacity,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("houdon_objects_bind_group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: std::num::NonZeroU64::new(size_of::<ObjectUniforms>() as u64),
            }),
        }],
    });
    (buffer, bind_group)
}

fn upload_mesh(device: &wgpu::Device, model: &houdon_core::Model) -> GpuMesh {
    GpuMesh {
        vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("houdon_mesh_vertices"),
            contents: bytemuck::cast_slice(&model.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("houdon_mesh_indices"),
            contents: bytemuck::cast_slice(&model.indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
        index_count: model.indices.len() as u32,
    }
}

/// The Studio viewport for one egui frame.
pub struct ViewportCallback {
    pub frame: Frame,
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        match callback_resources.get_mut::<SceneRenderer>() {
            Some(renderer) => renderer.prepare(device, queue, &self.frame),
            None => log::error!("SceneRenderer::install was not called"),
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(renderer) = callback_resources.get::<SceneRenderer>() {
            renderer.draw(render_pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shader_light_array_matches_max_lights() {
        let declaration = format!("const MAX_LIGHTS: u32 = {MAX_LIGHTS}u;");
        assert!(include_str!("shader.wgsl").contains(&declaration));
    }

    #[test]
    fn uniforms_are_16_byte_aligned() {
        // WebGL2 uniform blocks use std140 layout.
        assert_eq!(size_of::<Globals>() % 16, 0);
        assert_eq!(size_of::<ObjectUniforms>() % 16, 0);
    }
}
