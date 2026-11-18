//! 3D viewport for HoudonRef, drawn inside egui through a wgpu paint callback.
//!
//! Everything here stays within the WebGL2 / OpenGL ES 3.0 feature level so
//! the same renderer runs on weak desktops, in the browser and on Android.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::{self, util::DeviceExt as _};
use glam::{Mat4, Vec3};
use houdon_core::{Model, Vertex};

/// Lighting of the scene, in linear colour space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Direction towards the key light, world space.
    pub key_direction: Vec3,
    pub key_color: Vec3,
    pub key_intensity: f32,
    pub sky_color: Vec3,
    pub ground_color: Vec3,
    pub ambient_intensity: f32,
}

impl Lighting {
    /// Direction from azimuth (around the vertical axis, 0 = front) and
    /// elevation (above the horizon), both in radians.
    pub fn direction_from_angles(azimuth: f32, elevation: f32) -> Vec3 {
        let (sin_az, cos_az) = azimuth.sin_cos();
        let (sin_el, cos_el) = elevation.sin_cos();
        Vec3::new(cos_el * sin_az, sin_el, cos_el * cos_az)
    }
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            // Classic three-quarter key from the upper left.
            key_direction: Self::direction_from_angles(-45f32.to_radians(), 45f32.to_radians()),
            key_color: Vec3::ONE,
            key_intensity: 1.0,
            sky_color: Vec3::new(0.85, 0.9, 1.0),
            ground_color: Vec3::new(0.35, 0.3, 0.25),
            ambient_intensity: 0.15,
        }
    }
}

/// Neutral light-grey clay, linear.
pub const CLAY_COLOR: Vec3 = Vec3::new(0.62, 0.6, 0.57);

/// A model shared with the GPU. The `id` tells the renderer when to
/// re-upload: give every newly loaded model a new id.
#[derive(Clone)]
pub struct SceneMesh {
    pub id: u64,
    pub model: Arc<Model>,
}

/// One frame of the viewport: what to draw and from where.
pub struct ViewportCallback {
    pub mesh: Option<SceneMesh>,
    pub view_proj: Mat4,
    pub lighting: Lighting,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    sky_color: [f32; 4],
    ground_color: [f32; 4],
    base_color: [f32; 4],
}

struct GpuMesh {
    id: u64,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// GPU resources of the viewport, stored in egui-wgpu's callback resources.
pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    gamma_encode: bool,
    mesh: Option<GpuMesh>,
}

impl SceneRenderer {
    /// Creates the renderer and registers it with egui-wgpu. Call once at
    /// startup. `depth_format` must match the depth buffer eframe was
    /// configured with.
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

    fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("houdon_scene_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("houdon_scene_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("houdon_scene_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("houdon_scene_uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("houdon_scene_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
            gamma_encode: !target_format.is_srgb(),
            mesh: None,
        }
    }

    fn upload(&mut self, device: &wgpu::Device, scene_mesh: Option<&SceneMesh>) {
        let Some(scene_mesh) = scene_mesh else {
            self.mesh = None;
            return;
        };
        if self.mesh.as_ref().is_some_and(|m| m.id == scene_mesh.id) {
            return;
        }
        let model = &scene_mesh.model;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("houdon_mesh_vertices"),
            contents: bytemuck::cast_slice(&model.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("houdon_mesh_indices"),
            contents: bytemuck::cast_slice(&model.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.mesh = Some(GpuMesh {
            id: scene_mesh.id,
            vertex_buffer,
            index_buffer,
            index_count: model.indices.len() as u32,
        });
    }
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
        let Some(renderer) = callback_resources.get_mut::<SceneRenderer>() else {
            log::error!("SceneRenderer::install was not called");
            return Vec::new();
        };
        renderer.upload(device, self.mesh.as_ref());

        let l = &self.lighting;
        let uniforms = Uniforms {
            view_proj: self.view_proj.to_cols_array_2d(),
            light_dir: l
                .key_direction
                .normalize_or(Vec3::Y)
                .extend(if renderer.gamma_encode { 1.0 } else { 0.0 })
                .to_array(),
            light_color: l.key_color.extend(l.key_intensity).to_array(),
            sky_color: l.sky_color.extend(l.ambient_intensity).to_array(),
            ground_color: l.ground_color.extend(0.0).to_array(),
            base_color: CLAY_COLOR.extend(1.0).to_array(),
        };
        queue.write_buffer(&renderer.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(renderer) = callback_resources.get::<SceneRenderer>() else {
            return;
        };
        let Some(mesh) = &renderer.mesh else {
            return;
        };
        render_pass.set_pipeline(&renderer.pipeline);
        render_pass.set_bind_group(0, &renderer.bind_group, &[]);
        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniforms_are_16_byte_aligned() {
        // WebGL2 uniform blocks use std140 layout.
        assert_eq!(size_of::<Uniforms>() % 16, 0);
    }

    #[test]
    fn light_angles() {
        let up = Lighting::direction_from_angles(0.0, std::f32::consts::FRAC_PI_2);
        assert!((up - Vec3::Y).length() < 1e-6);
        let front = Lighting::direction_from_angles(0.0, 0.0);
        assert!((front - Vec3::Z).length() < 1e-6);
    }
}
