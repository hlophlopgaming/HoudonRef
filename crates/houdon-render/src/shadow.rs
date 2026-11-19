//! Shadow maps: one depth atlas shared by every shadow-casting light.
//!
//! A directional or spot light uses one tile of the atlas, a point light six
//! (the faces of a cube). More lights mean smaller tiles: the atlas is the
//! shadow budget. The atlas is only redrawn when a light or a caster
//! changes, never because the camera moved.

use std::hash::{Hash, Hasher};

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3};
use houdon_core::{Bounds, Light, LightKind, MAX_LIGHTS};

use crate::Frame;

/// Most light views: one per cube face when every light is a point light.
pub const MAX_SHADOW_VIEWS: usize = MAX_LIGHTS * CUBE_FACES.len();

/// Default side of the shadow atlas, texels: the default shadow budget.
pub const DEFAULT_ATLAS_SIZE: u32 = 2048;

pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Directions of a point light's cube faces, in the order the shader picks
/// them: +X, -X, +Y, -Y, +Z, -Z; each with an up vector.
const CUBE_FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::NEG_Y),
    (Vec3::NEG_X, Vec3::NEG_Y),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::NEG_Z),
    (Vec3::Z, Vec3::NEG_Y),
    (Vec3::NEG_Z, Vec3::NEG_Y),
];

/// The light views of one light, from the light towards the casters.
pub fn light_views(light: &Light, centre: Vec3, casters: &Bounds) -> Vec<Mat4> {
    let radius = casters.radius().max(0.05);
    match light.kind {
        LightKind::Directional => {
            let target = casters.center();
            let direction = light.direction();
            let eye = target + direction * (radius * 2.0 + 1.0);
            let up = if direction.y.abs() > 0.99 {
                Vec3::Z
            } else {
                Vec3::Y
            };
            let far = radius * 4.0 + 2.0;
            vec![
                directx::orthographic(-radius, radius, -radius, radius, 0.01, far)
                    * view::look_at_mat4(eye, target, up),
            ]
        }
        LightKind::Spot => {
            let position = light.position(centre);
            let aim = centre - position;
            let up = if aim.normalize_or_zero().y.abs() > 0.99 {
                Vec3::Z
            } else {
                Vec3::Y
            };
            let fov = (light.cone_angle * 1.05).clamp(0.05, 3.0);
            let far = light.distance + radius * 2.0 + 10.0;
            vec![
                directx::perspective(fov, 1.0, 0.05, far)
                    * view::look_at_mat4(position, centre, up),
            ]
        }
        LightKind::Point => {
            let position = light.position(centre);
            let far = reach(position, casters);
            let projection = directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.05, far);
            CUBE_FACES
                .iter()
                .map(|&(forward, up)| projection * view::look_to_mat4(position, forward, up))
                .collect()
        }
    }
}

/// How far a light at `position` must see to cover every caster, with room
/// for the shadows they cast beyond themselves.
fn reach(position: Vec3, casters: &Bounds) -> f32 {
    (casters.center() - position).length() + casters.radius() * 2.0 + 10.0
}

/// Where light view `index` sits in the atlas, in whole texels: the shadow
/// pass draws there and the shader reads there, from this one function.
pub fn tile_rect(index: u32, views: usize, atlas_size: u32) -> (u32, u32, u32) {
    let grid = (views as f32).sqrt().ceil().max(1.0) as u32;
    let size = atlas_size / grid;
    ((index % grid) * size, (index / grid) * size, size)
}

/// Shadow data for the scene shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ShadowUniforms {
    pub view_proj: [[[f32; 4]; 4]; MAX_SHADOW_VIEWS],
    /// xy: tile origin in atlas uv; z: tile size in uv; w unused.
    pub tiles: [[f32; 4]; MAX_SHADOW_VIEWS],
    /// x: atlas size in texels.
    pub params: [f32; 4],
}

/// Where each light's views went in the atlas, as the scene shader needs it.
pub struct ShadowLayout {
    /// For each light of the frame, its first view, or `None` without shadows.
    pub first_view: Vec<Option<u32>>,
    pub views: Vec<Mat4>,
}

impl ShadowLayout {
    pub fn new(frame: &Frame) -> Self {
        let casters = caster_bounds(frame);
        let mut views = Vec::new();
        let first_view = frame
            .lights
            .iter()
            .map(|light| {
                if !light.casts_shadows || casters.is_empty() {
                    return None;
                }
                let first = views.len() as u32;
                views.extend(light_views(light, frame.light_centre, &casters));
                Some(first)
            })
            .collect();
        Self { first_view, views }
    }

    pub fn uniforms(&self, atlas_size: u32) -> ShadowUniforms {
        let mut uniforms = ShadowUniforms::zeroed();
        let texel = 1.0 / atlas_size as f32;
        for (i, view) in self.views.iter().enumerate().take(MAX_SHADOW_VIEWS) {
            uniforms.view_proj[i] = view.to_cols_array_2d();
            let (x, y, size) = tile_rect(i as u32, self.views.len(), atlas_size);
            uniforms.tiles[i] = [x as f32 * texel, y as f32 * texel, size as f32 * texel, 0.0];
        }
        uniforms.params = [atlas_size as f32, 0.0, 0.0, 0.0];
        uniforms
    }

    /// A fingerprint of everything the atlas depends on: the light views and
    /// the casters. The camera is not part of it.
    pub fn fingerprint(&self, frame: &Frame, atlas_size: u32) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        atlas_size.hash(&mut hasher);
        for view in &self.views {
            bytemuck::cast_slice::<f32, u8>(&view.to_cols_array()).hash(&mut hasher);
        }
        for object in frame.objects.iter().filter(|o| o.casts_shadows) {
            object.model_id.hash(&mut hasher);
            bytemuck::cast_slice::<f32, u8>(&object.transform.to_cols_array()).hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// Bounds of the objects that cast shadows, in scene space.
fn caster_bounds(frame: &Frame) -> Bounds {
    frame
        .objects
        .iter()
        .filter(|o| o.casts_shadows && !o.model.bounds.is_empty())
        .fold(Bounds::EMPTY, |acc, object| {
            let b = object.model.bounds;
            (0..8).fold(acc, |acc, i| {
                let corner = Vec3::new(
                    if i & 1 == 0 { b.min.x } else { b.max.x },
                    if i & 2 == 0 { b.min.y } else { b.max.y },
                    if i & 4 == 0 { b.min.z } else { b.max.z },
                );
                acc.extended(object.transform.transform_point3(corner))
            })
        })
}

/// GPU side of the shadows: the atlas, the depth pass and the data the
/// scene shader reads (bind group 2).
pub struct ShadowResources {
    /// Layout of bind group 2 of the scene pipeline.
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    uniforms_buffer: wgpu::Buffer,
    atlas_view: wgpu::TextureView,
    atlas_size: u32,
    view_buffer: wgpu::Buffer,
    view_bind_group: wgpu::BindGroup,
    view_stride: u64,
    pipeline: wgpu::RenderPipeline,
    fingerprint: Option<u64>,
    /// How many times the atlas was redrawn.
    pub builds: u64,
}

impl ShadowResources {
    /// `atlas_size` is the shadow budget: the side of the atlas in texels.
    pub fn new(
        device: &wgpu::Device,
        object_layout: &wgpu::BindGroupLayout,
        atlas_size: u32,
    ) -> Self {
        let atlas_size = atlas_size.min(device.limits().max_texture_dimension_2d);
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("houdon_shadow_atlas"),
            size: wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());

        let uniforms_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("houdon_shadow_uniforms"),
            size: size_of::<ShadowUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("houdon_shadow_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(
                            size_of::<ShadowUniforms>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        // Lit where the surface is no farther than the nearest caster;
        // linear filtering blends neighbouring comparisons.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("houdon_shadow_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("houdon_shadow_bind_group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // One view-projection matrix per light view, at aligned offsets.
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let view_stride = 64u64.div_ceil(alignment) * alignment;
        let view_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("houdon_shadow_views"),
            size: view_stride * MAX_SHADOW_VIEWS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("houdon_shadow_view_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: std::num::NonZeroU64::new(64),
                },
                count: None,
            }],
        });
        let view_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("houdon_shadow_view_bind_group"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &view_buffer,
                    offset: 0,
                    size: std::num::NonZeroU64::new(64),
                }),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("houdon_shadow_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("houdon_shadow_pipeline_layout"),
            bind_group_layouts: &[Some(&view_layout), Some(object_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("houdon_shadow_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<houdon_core::Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                // Push depths back a little against shadow acne.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });

        Self {
            layout,
            bind_group,
            uniforms_buffer,
            atlas_view,
            atlas_size,
            view_buffer,
            view_bind_group,
            view_stride,
            pipeline,
            fingerprint: None,
            builds: 0,
        }
    }

    /// Writes the shadow data for `frame` and, when a light or a caster
    /// changed, redraws the atlas into `encoder`. `draw_casters` draws the
    /// frame's shadow-casting objects into the pass it is given.
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        layout: &ShadowLayout,
        draw_casters: impl Fn(&mut wgpu::RenderPass<'_>),
    ) {
        queue.write_buffer(
            &self.uniforms_buffer,
            0,
            bytemuck::bytes_of(&layout.uniforms(self.atlas_size)),
        );
        let fingerprint = layout.fingerprint(frame, self.atlas_size);
        if self.fingerprint == Some(fingerprint) {
            return;
        }
        self.fingerprint = Some(fingerprint);
        self.builds += 1;

        let mut views = vec![0u8; (self.view_stride * MAX_SHADOW_VIEWS as u64) as usize];
        for (i, view) in layout.views.iter().enumerate().take(MAX_SHADOW_VIEWS) {
            let start = i * self.view_stride as usize;
            views[start..start + 64].copy_from_slice(bytemuck::cast_slice(&view.to_cols_array()));
        }
        queue.write_buffer(&self.view_buffer, 0, &views);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("houdon_shadow_pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.atlas_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        for i in 0..layout.views.len().min(MAX_SHADOW_VIEWS) as u32 {
            let (x, y, size) = tile_rect(i, layout.views.len(), self.atlas_size);
            pass.set_viewport(x as f32, y as f32, size as f32, size as f32, 0.0, 1.0);
            pass.set_bind_group(
                0,
                &self.view_bind_group,
                &[(u64::from(i) * self.view_stride) as u32],
            );
            draw_casters(&mut pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_never_overlap_and_stay_in_the_atlas() {
        for views in [1, 6, 7, 16, 37, MAX_SHADOW_VIEWS] {
            let tiles: Vec<_> = (0..views as u32)
                .map(|i| tile_rect(i, views, DEFAULT_ATLAS_SIZE))
                .collect();
            for (i, &(x, y, size)) in tiles.iter().enumerate() {
                assert!(
                    size > 0 && x + size <= DEFAULT_ATLAS_SIZE && y + size <= DEFAULT_ATLAS_SIZE
                );
                for &(ox, oy, _) in &tiles[i + 1..] {
                    assert!(ox >= x + size || oy >= y + size || ox + size <= x || oy + size <= y);
                }
            }
        }
    }
}
