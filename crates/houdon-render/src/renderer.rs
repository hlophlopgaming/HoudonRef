//! GPU side of the scene: pipeline, per-model meshes and per-object data.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::{self, util::DeviceExt as _};
use glam::Vec3;
use houdon_core::{
    Light, LightKind, MAX_LIGHTS, MeshPart, ModelId, ShadingMode, SolidLighting, Vertex,
};

use crate::composite::{
    AntiAliasing, Composite, NORMAL_FORMAT, SCENE_DEPTH_FORMAT, needs_surfaces,
};
use crate::light_mask::MaskTextures;
use crate::matcap::MatcapTexture;
use crate::material::MaterialResources;
use crate::shadow::{ShadowLayout, ShadowResources};
use crate::{Frame, FrameObject, FrameSurface, Highlight, ObjectColors};

/// Roughness of plain colours (Clay, Random, the room): matte.
const PLAIN_ROUGHNESS: f32 = 0.9;

/// Colour mixed into a selected object, linear, and how much of it. A cool
/// interface blue, so the highlight is not mistaken for a material.
const SELECTION_COLOR: [f32; 4] = [0.15, 0.45, 1.0, 0.35];

/// The same colour, fainter, on what is under the pointer.
const HOVER_COLOR: [f32; 4] = [0.15, 0.45, 1.0, 0.15];

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    /// rgb: ambient colour from above (the backdrop); a: ambient intensity.
    backdrop_color: [f32; 4],
    /// rgb: ambient colour from below (the floor).
    floor_color: [f32; 4],
    /// xyz: camera position.
    eye: [f32; 4],
    /// x: number of lights; y: 1 if the output needs gamma encoding.
    params: [u32; 4],
    /// x: how surfaces are lit, one of the `SHADE_*` codes.
    shading: [f32; 4],
    /// xyz: the camera's right and up directions.
    camera_right: [f32; 4],
    camera_up: [f32; 4],
    /// x, y: geometry cavity ridge and hollow strength (0 when it is off).
    cavity: [f32; 4],
    /// rgb: wireframe colour, linear; a: its opacity.
    wire: [f32; 4],
    lights: [GpuLight; MAX_LIGHTS],
}

/// `Globals::shading.x`: the Scene's lights, or one of the Solid lights.
const SHADE_LIT: f32 = 0.0;
const SHADE_STUDIO: f32 = 1.0;
const SHADE_MATCAP: f32 = 2.0;
const SHADE_FLAT: f32 = 3.0;

/// How much ambient occlusion darkens the room's light: 0 when it is off,
/// and outside the Lit mode.
fn ambient_occlusion_strength(shading: &houdon_core::Shading) -> f32 {
    let ao = &shading.ambient_occlusion;
    if ao.enabled && shading.mode == ShadingMode::Lit {
        ao.strength.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// `spot.w` of a light without a cone; the shader tests `spot.w > -1.5`.
const NO_CONE: f32 = -2.0;

/// `spot_inner.y` of a light without shadows; otherwise its first shadow view.
const NO_SHADOW: f32 = -1.0;

/// `spot_inner.z` of a light without a mask; otherwise its mask layer.
const NO_MASK: f32 = -1.0;

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
    /// x: cosine of the inner half angle; y: first shadow view, or -1;
    /// z: mask layer, or -1; w: a directional light's mask pattern size.
    spot_inner: [f32; 4],
}

impl GpuLight {
    fn new(
        light: &Light,
        centre: Vec3,
        first_shadow_view: Option<u32>,
        mask_layer: Option<u32>,
    ) -> Self {
        let shadow = first_shadow_view.map_or(NO_SHADOW, |v| v as f32);
        let mask = mask_layer.map_or(NO_MASK, |layer| layer as f32);
        let pattern = light
            .mask
            .as_ref()
            .map_or(1.0, |m| m.pattern_size.max(0.01));
        let color = (light.color * light.intensity).extend(0.0).to_array();
        match light.kind {
            LightKind::Directional => Self {
                position: light.direction().extend(0.0).to_array(),
                color,
                spot: [0.0, 0.0, 0.0, NO_CONE],
                spot_inner: [0.0, shadow, mask, pattern],
            },
            LightKind::Point | LightKind::Spot => {
                let position = light.position(centre);
                let (spot, spot_inner) = if light.kind == LightKind::Spot {
                    let (outer, inner) = light.cone_cosines();
                    let aim = (centre - position).normalize_or(Vec3::NEG_Y);
                    (aim.extend(outer).to_array(), [inner, shadow, mask, pattern])
                } else {
                    ([0.0, 0.0, 0.0, NO_CONE], [0.0, shadow, NO_MASK, 0.0])
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

/// One draw: a part of an object. Mirrored by `ObjectUniforms` in both
/// shaders (a test keeps them equal).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ObjectUniforms {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    /// rgb: highlight colour; a: how much of it to mix in.
    highlight: [f32; 4],
    /// rgb: plain surface colour below `color_below.a` (world height).
    color_below: [f32; 4],
    /// rgb: plain surface colour above `color_above.a` (world height).
    color_above: [f32; 4],
    /// Material base colour factor, linear RGBA.
    base_color: [f32; 4],
    /// x: metallic; y: roughness; z: 1 to use the material, 0 for the
    /// plain colours; w: normal map scale.
    material: [f32; 4],
    /// Fibre lines: xyz the direction they run, w how many go round (0 for
    /// none).
    fibre_axis: [f32; 4],
    /// xyz: a point on the muscle's middle line.
    fibre_centre: [f32; 4],
}

/// One part of one object, as drawn in the last prepared frame.
struct DrawEntry {
    model_id: ModelId,
    part: MeshPart,
    material: Option<(ModelId, usize)>,
    one_sided: bool,
    casts_shadows: bool,
    /// Draw the object's wireframe with this entry: its first part, never
    /// the room.
    wireframe: bool,
    /// Blended over what is behind; drawn after every solid entry.
    see_through: bool,
}

pub(crate) struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    /// One curvature per vertex, for the geometry cavity.
    curvature_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    /// Pairs of indices, one per edge, for the wireframe.
    edge_buffer: wgpu::Buffer,
    edge_index_count: u32,
}

/// GPU resources for drawing frames. One renders the viewport (stored in
/// egui-wgpu's callback resources); offscreen pictures own another.
pub struct SceneRenderer {
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    /// Rebuilt when the multisampling changes.
    pipelines: ScenePipelines,
    surface_pipelines: SurfacePipelines,
    globals_buffer: wgpu::Buffer,
    globals_layout: wgpu::BindGroupLayout,
    globals_bind_group: wgpu::BindGroup,
    /// Stands in for ambient occlusion until the first picture has its own.
    no_occlusion: wgpu::TextureView,
    /// The composite target generation the globals bind group was made for.
    bound_generation: u64,
    masks: MaskTextures,
    matcap: MatcapTexture,
    /// Whether the last prepared frame draws wireframes.
    wireframe: bool,
    object_layout: wgpu::BindGroupLayout,
    object_buffer: wgpu::Buffer,
    object_bind_group: wgpu::BindGroup,
    /// Bytes between two objects' uniforms, respecting the device's alignment.
    object_stride: u64,
    object_capacity: u64,
    gamma_encode: bool,
    meshes: HashMap<ModelId, GpuMesh>,
    shadows: ShadowResources,
    materials: MaterialResources,
    composite: Composite,
    /// The parts drawn by the last prepared frame, one uniform slot each.
    draw_list: Vec<DrawEntry>,
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
            &render_state.queue,
            render_state.target_format,
            depth_format,
            crate::DEFAULT_ATLAS_SIZE,
        );
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(renderer);
    }

    /// `shadow_atlas_size` is the shadow budget shared by all lights, the
    /// side of the shadow atlas in texels.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        shadow_atlas_size: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("houdon_scene_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let globals_layout = globals_layout(device);
        let masks = MaskTextures::new(device);
        let matcap = MatcapTexture::new(device);
        let object_layout = uniform_layout(
            device,
            "houdon_object_layout",
            size_of::<ObjectUniforms>(),
            true,
        );

        let shadows = ShadowResources::new(device, &object_layout, shadow_atlas_size);
        let materials = MaterialResources::new(device, queue);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("houdon_scene_pipeline_layout"),
            bind_group_layouts: &[
                Some(&globals_layout),
                Some(&object_layout),
                Some(&shadows.layout),
                Some(&materials.layout),
            ],
            immediate_size: 0,
        });

        let pipelines = ScenePipelines::new(device, &pipeline_layout, &shader, target_format, 1);
        let surface_pipelines = SurfacePipelines::new(device, &pipeline_layout, &shader);

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("houdon_globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let no_occlusion = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("houdon_no_occlusion"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::composite::OCCLUSION_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let globals_bind_group = globals_bind_group(
            device,
            &globals_layout,
            &globals_buffer,
            &masks,
            &matcap,
            &no_occlusion,
        );

        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let object_stride = (size_of::<ObjectUniforms>() as u64).div_ceil(alignment) * alignment;
        let object_capacity = 16;
        let (object_buffer, object_bind_group) =
            object_resources(device, &object_layout, object_stride, object_capacity);

        Self {
            shader,
            pipeline_layout,
            target_format,
            pipelines,
            surface_pipelines,
            globals_buffer,
            globals_layout,
            globals_bind_group,
            no_occlusion,
            bound_generation: 0,
            masks,
            matcap,
            wireframe: false,
            object_layout,
            object_buffer,
            object_bind_group,
            object_stride,
            object_capacity,
            gamma_encode: !target_format.is_srgb(),
            shadows,
            materials,
            composite: Composite::new(device, target_format, depth_format),
            meshes: HashMap::new(),
            draw_list: Vec::new(),
        }
    }

    /// The side of the shadow atlas in texels.
    pub fn shadow_atlas_size(&self) -> u32 {
        self.shadows.atlas_size()
    }

    /// How many times the shadow maps were redrawn so far.
    pub fn shadow_map_builds(&self) -> u64 {
        self.shadows.builds
    }

    /// Uploads what `frame` needs, redraws the shadow maps when needed and
    /// renders the scene into its own `size` picture, all into `encoder`.
    /// Call before [`Self::draw`] in the same frame.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        size: [u32; 2],
        anti_aliasing: AntiAliasing,
    ) {
        self.shadows
            .set_atlas_size(device, frame.quality.shadow_atlas_size);
        self.sync_meshes(device, frame);
        self.prepare_draw_list(device, queue, frame);
        let shadow_layout = ShadowLayout::new(frame);
        self.write_globals(device, queue, frame, &shadow_layout);
        self.draw_shadow_maps(queue, encoder, frame, &shadow_layout);
        self.prepare_targets(device, queue, frame, size, anti_aliasing);
        self.draw_scene_passes(encoder, frame);
    }

    /// Uploads the meshes of new models and drops those no longer drawn,
    /// with their material textures.
    fn sync_meshes(&mut self, device: &wgpu::Device, frame: &Frame) {
        let drawn = |id: ModelId| frame.objects.iter().any(|o| o.model_id == id);
        self.meshes.retain(|id, _| drawn(*id));
        self.materials.retain(drawn);
        for object in &frame.objects {
            self.meshes
                .entry(object.model_id)
                .or_insert_with(|| upload_mesh(device, &object.model));
        }
    }

    /// One draw per part, with its uniforms and its material.
    fn prepare_draw_list(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame) {
        let clay = ObjectColors::uniform(frame.shading.clay_color);
        let mut entries = Vec::new();
        let mut uniforms = Vec::new();
        // Solid objects first, then see-through ones from far to near, so
        // each blends over everything behind it.
        let distance = |object: &&FrameObject| {
            let centre = object
                .transform
                .transform_point3(object.model.bounds.center());
            centre.distance_squared(frame.eye)
        };
        let (solid, mut see_through): (Vec<&FrameObject>, Vec<&FrameObject>) =
            frame.objects.iter().partition(|o| o.opacity >= 1.0);
        see_through.sort_by(|a, b| distance(b).total_cmp(&distance(a)));
        for object in solid.into_iter().chain(see_through) {
            let opacity = object.opacity.clamp(0.0, 1.0);
            let model = &object.model;
            for (part_index, part) in model.parts.iter().enumerate() {
                let material = match object.surface {
                    FrameSurface::Materials => part.material,
                    FrameSurface::Colors(_) => None,
                };
                let colors = match object.surface {
                    FrameSurface::Colors(colors) => colors,
                    FrameSurface::Materials => clay,
                };
                let (base_color, material_params) = match material.map(|m| &model.materials[m]) {
                    Some(m) => (
                        (m.base_color * glam::Vec4::new(1.0, 1.0, 1.0, opacity)).to_array(),
                        [m.metallic, m.roughness, 1.0, m.normal_scale],
                    ),
                    None => ([1.0, 1.0, 1.0, opacity], [0.0, PLAIN_ROUGHNESS, 0.0, 1.0]),
                };
                if let Some(index) = material {
                    self.materials
                        .prepare(device, queue, object.model_id, model, index);
                }
                uniforms.push(ObjectUniforms {
                    model: object.transform.to_cols_array_2d(),
                    normal: object.transform.inverse().transpose().to_cols_array_2d(),
                    highlight: match object.highlight {
                        Highlight::Selected => SELECTION_COLOR,
                        Highlight::Hovered => HOVER_COLOR,
                        Highlight::None => [0.0; 4],
                    },
                    color_below: colors.below.extend(colors.from_y).to_array(),
                    color_above: colors.above.extend(colors.to_y).to_array(),
                    base_color,
                    material: material_params,
                    fibre_axis: object
                        .fibres
                        .map_or([0.0; 4], |f| f.axis.extend(f.lines).to_array()),
                    fibre_centre: object
                        .fibres
                        .map_or([0.0; 4], |f| f.centre.extend(0.0).to_array()),
                });
                entries.push(DrawEntry {
                    model_id: object.model_id,
                    part: *part,
                    material: material.map(|m| (object.model_id, m)),
                    one_sided: object.one_sided || frame.shading.backface_culling,
                    casts_shadows: object.casts_shadows,
                    wireframe: part_index == 0 && !object.one_sided,
                    see_through: opacity < 1.0,
                });
            }
        }
        let count = uniforms.len() as u64;
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
        for (i, slot) in uniforms.iter().enumerate() {
            let start = i * self.object_stride as usize;
            bytes[start..start + size_of::<ObjectUniforms>()]
                .copy_from_slice(bytemuck::bytes_of(slot));
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.object_buffer, 0, &bytes);
        }
        self.draw_list = entries;
    }

    /// The frame's camera, room, shading and lights for the shaders.
    fn write_globals(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Frame,
        shadow_layout: &ShadowLayout,
    ) {
        let mask_layers = self.masks.prepare(queue, &frame.lights);
        let mut lights = [GpuLight::zeroed(); MAX_LIGHTS];
        let count = frame.lights.len().min(MAX_LIGHTS);
        for (((slot, light), first_view), mask_layer) in lights
            .iter_mut()
            .zip(&frame.lights)
            .zip(&shadow_layout.first_view)
            .zip(&mask_layers)
        {
            *slot = GpuLight::new(light, frame.light_centre, *first_view, *mask_layer);
        }
        let room = &frame.room;
        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            backdrop_color: room
                .backdrop_color
                .extend(room.ambient_intensity)
                .to_array(),
            floor_color: room.floor_color.extend(0.0).to_array(),
            eye: frame.eye.extend(1.0).to_array(),
            params: [count as u32, u32::from(self.gamma_encode), 0, 0],
            shading: [
                self.shade_code(device, queue, frame),
                ambient_occlusion_strength(&frame.shading),
                0.0,
                0.0,
            ],
            camera_right: frame.camera_right.extend(0.0).to_array(),
            camera_up: frame.camera_up.extend(0.0).to_array(),
            wire: frame
                .shading
                .lines
                .wireframe_color
                .extend(frame.shading.lines.wireframe_opacity.clamp(0.0, 1.0))
                .to_array(),
            cavity: if frame.shading.cavity.geometry {
                [
                    frame.shading.cavity.geometry_ridge,
                    frame.shading.cavity.geometry_hollow,
                    0.0,
                    0.0,
                ]
            } else {
                [0.0; 4]
            },
            lights,
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
    }

    /// Redraws the shadow atlas when a light or a caster changed.
    fn draw_shadow_maps(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        shadow_layout: &ShadowLayout,
    ) {
        let casters: Vec<(usize, &GpuMesh, MeshPart)> = self
            .draw_list
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.casts_shadows)
            .filter_map(|(i, entry)| Some((i, self.meshes.get(&entry.model_id)?, entry.part)))
            .collect();
        let (object_bind_group, object_stride) = (&self.object_bind_group, self.object_stride);
        self.shadows
            .prepare(queue, encoder, frame, shadow_layout, |pass| {
                for (i, mesh, part) in &casters {
                    let offset = (*i as u64 * object_stride) as u32;
                    pass.set_bind_group(1, object_bind_group, &[offset]);
                    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(part_range(part), 0, 0..1);
                }
            });
    }

    /// Pipelines for the anti-aliasing and the scene's pictures at `size`.
    fn prepare_targets(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Frame,
        size: [u32; 2],
        anti_aliasing: AntiAliasing,
    ) {
        let anti_aliasing = anti_aliasing.normalized();
        if self.pipelines.samples != anti_aliasing.samples {
            self.pipelines = ScenePipelines::new(
                device,
                &self.pipeline_layout,
                &self.shader,
                self.target_format,
                anti_aliasing.samples,
            );
        }
        self.wireframe =
            frame.shading.lines.wireframe && frame.shading.lines.wireframe_opacity > 0.0;
        self.composite
            .prepare(device, queue, size, anti_aliasing, frame);
        if self.composite.generation != self.bound_generation {
            self.rebind_globals(device);
        }
    }

    /// Draws the scene into its pictures, and the wireframe over it.
    fn draw_scene_passes(&mut self, encoder: &mut wgpu::CommandEncoder, frame: &Frame) {
        let Some(target) = &self.composite.target else {
            return;
        };
        // The backdrop colour behind everything, in the picture's encoding.
        let [r, g, b] = if self.gamma_encode {
            frame.clear_color().map(|c| f64::from(c) / 255.0)
        } else {
            frame.room.backdrop_color.to_array().map(f64::from)
        };
        // Only the resolved pictures are read afterwards. With a wireframe
        // the colour is resolved after the wireframe pass instead, which
        // draws over it and needs the scene's depth.
        let wireframe = self.wireframe;
        let keep = if wireframe {
            wgpu::StoreOp::Store
        } else {
            wgpu::StoreOp::Discard
        };
        // Surfaces and ambient occlusion first: the scene pass reads them.
        if needs_surfaces(&frame.shading) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("houdon_surface_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // No surface: the farthest distance.
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.surface_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            self.draw_scene(&mut pass, true);
        }
        self.composite.draw_occlusion(encoder);
        let color = target.color_attachment();
        {
            let (view, resolve_target) = color;
            let resolve_target = if wireframe { None } else { resolve_target };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("houdon_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: if resolve_target.is_some() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: keep,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            self.draw_scene(&mut pass, false);
        }
        if wireframe {
            // Its own pass: WebGL2 cannot blend one target and not another.
            let (view, resolve_target) = color;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("houdon_wireframe_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: if resolve_target.is_some() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            self.draw_wireframe(&mut pass);
        }
    }

    /// Remakes the globals bind group, for a new matcap or new pictures.
    fn rebind_globals(&mut self, device: &wgpu::Device) {
        let occlusion = self
            .composite
            .target
            .as_ref()
            .map_or(&self.no_occlusion, |t| &t.smooth_occlusion_view);
        self.globals_bind_group = globals_bind_group(
            device,
            &self.globals_layout,
            &self.globals_buffer,
            &self.masks,
            &self.matcap,
            occlusion,
        );
        self.bound_generation = self.composite.generation;
    }

    /// How `frame`'s surfaces are lit, uploading its matcap if it uses one.
    fn shade_code(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame) -> f32 {
        let shading = &frame.shading;
        let matcap = shading.matcap.as_ref().map(|m| &m.image);
        match (shading.mode, shading.solid, matcap) {
            (ShadingMode::Lit, ..) => SHADE_LIT,
            (ShadingMode::Solid, SolidLighting::Flat, _) => SHADE_FLAT,
            (ShadingMode::Solid, SolidLighting::Matcap, Some(image))
                if image.width.max(image.height) <= device.limits().max_texture_dimension_2d =>
            {
                if self.matcap.prepare(device, queue, image) {
                    self.rebind_globals(device);
                }
                SHADE_MATCAP
            }
            // An absent or oversized matcap uses the Studio light.
            (ShadingMode::Solid, ..) => SHADE_STUDIO,
        }
    }

    /// Puts the last prepared picture into `render_pass`, as a value study
    /// if one is on.
    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        self.composite.draw(render_pass);
    }

    /// Draws every part: its colour, or with `surfaces` the picture of
    /// surfaces for the screen-space overlays.
    fn draw_scene(&self, render_pass: &mut wgpu::RenderPass<'_>, surfaces: bool) {
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        render_pass.set_bind_group(2, &self.shadows.bind_group, &[]);
        for (i, entry) in self.draw_list.iter().enumerate() {
            let Some(mesh) = self.meshes.get(&entry.model_id) else {
                continue;
            };
            if surfaces && entry.see_through {
                // The overlays see the solid surfaces behind it.
                continue;
            }
            render_pass.set_pipeline(match (surfaces, entry.one_sided, entry.see_through) {
                (false, _, true) => &self.pipelines.see_through,
                (false, false, false) => &self.pipelines.two_sided,
                (false, true, false) => &self.pipelines.one_sided,
                (true, false, _) => &self.surface_pipelines.two_sided,
                (true, true, _) => &self.surface_pipelines.one_sided,
            });
            let offset = (i as u64 * self.object_stride) as u32;
            render_pass.set_bind_group(1, &self.object_bind_group, &[offset]);
            render_pass.set_bind_group(3, self.materials.bind_group(entry.material), &[]);
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, mesh.curvature_buffer.slice(..));
            render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(part_range(&entry.part), 0, 0..1);
        }
    }

    /// The edges of every object but the room, over the drawn scene.
    fn draw_wireframe(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipelines.wireframe);
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        render_pass.set_bind_group(2, &self.shadows.bind_group, &[]);
        for (i, entry) in self.draw_list.iter().enumerate() {
            let Some(mesh) = self.meshes.get(&entry.model_id) else {
                continue;
            };
            if !entry.wireframe {
                continue;
            }
            let offset = (i as u64 * self.object_stride) as u32;
            render_pass.set_bind_group(1, &self.object_bind_group, &[offset]);
            render_pass.set_bind_group(3, self.materials.bind_group(entry.material), &[]);
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.edge_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..mesh.edge_index_count, 0, 0..1);
        }
    }
}

/// The index range of a part.
fn part_range(part: &MeshPart) -> std::ops::Range<u32> {
    part.first_index..part.first_index + part.index_count
}

/// The scene's pipelines for one multisampling.
struct ScenePipelines {
    samples: u32,
    /// Imported meshes are often open or single-sided: drawn from both
    /// sides, the shader flips the normal.
    two_sided: wgpu::RenderPipeline,
    /// The room is drawn from inside only, so it can be seen through from
    /// outside.
    one_sided: wgpu::RenderPipeline,
    /// The meshes' edges as lines over their surfaces.
    wireframe: wgpu::RenderPipeline,
    /// See-through objects, blended over the solid ones: their front faces
    /// only, without writing depth.
    see_through: wgpu::RenderPipeline,
}

/// The surfaces pass's pipelines: never multisampled (see `fs_surface`).
struct SurfacePipelines {
    two_sided: wgpu::RenderPipeline,
    one_sided: wgpu::RenderPipeline,
}

impl SurfacePipelines {
    fn new(
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
    ) -> Self {
        let output = SceneOutput {
            entry_point: "fs_surface",
            format: NORMAL_FORMAT,
            blended: false,
        };
        let pipeline = |cull_mode, label| {
            scene_pipeline(device, pipeline_layout, shader, output, cull_mode, 1, label)
        };
        Self {
            two_sided: pipeline(None, "houdon_surface_pipeline"),
            one_sided: pipeline(Some(wgpu::Face::Back), "houdon_one_sided_surface_pipeline"),
        }
    }
}

impl ScenePipelines {
    fn new(
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        target_format: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let output = SceneOutput {
            entry_point: "fs_main",
            format: target_format,
            blended: false,
        };
        let pipeline = |cull_mode, label| {
            scene_pipeline(
                device,
                pipeline_layout,
                shader,
                output,
                cull_mode,
                samples,
                label,
            )
        };
        let see_through = SceneOutput {
            blended: true,
            ..output
        };
        Self {
            samples,
            two_sided: pipeline(None, "houdon_scene_pipeline"),
            one_sided: pipeline(Some(wgpu::Face::Back), "houdon_one_sided_pipeline"),
            see_through: scene_pipeline(
                device,
                pipeline_layout,
                shader,
                see_through,
                Some(wgpu::Face::Back),
                samples,
                "houdon_see_through_pipeline",
            ),
            wireframe: wireframe_pipeline(device, pipeline_layout, shader, target_format, samples),
        }
    }
}

fn wireframe_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    samples: u32,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("houdon_wireframe_pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_wire"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            ..Default::default()
        },
        // Seen where the surface is; the shader pulls lines a little
        // towards the eye so they win over their own faces.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SCENE_DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_wire"),
            // Colour only: the surfaces under the lines stay as they are
            // for the screen-space overlays.
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// What a scene pipeline draws: the colour (`fs_main`) or the surfaces for
/// the screen-space overlays (`fs_surface`).
#[derive(Clone, Copy)]
struct SceneOutput {
    entry_point: &'static str,
    format: wgpu::TextureFormat,
    /// Blends over the picture and leaves the depth as it is.
    blended: bool,
}

fn scene_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    output: SceneOutput,
    cull_mode: Option<wgpu::Face>,
    samples: u32,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
            }), Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<f32>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![3 => Float32],
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SCENE_DEPTH_FORMAT,
            depth_write_enabled: Some(!output.blended),
            depth_compare: Some(if output.blended {
                wgpu::CompareFunction::LessEqual
            } else {
                wgpu::CompareFunction::Less
            }),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(output.entry_point),
            targets: &[Some(wgpu::ColorTargetState {
                format: output.format,
                blend: output.blended.then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Bind group 0: the frame's globals and the light masks.
fn globals_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("houdon_globals_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(size_of::<Globals>() as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

fn globals_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    masks: &MaskTextures,
    matcap: &MatcapTexture,
    occlusion: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("houdon_globals_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&masks.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&masks.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&matcap.view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&matcap.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(occlusion),
            },
        ],
    })
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
    let edges = edge_indices(&model.indices);
    GpuMesh {
        edge_index_count: edges.len() as u32,
        edge_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("houdon_mesh_edges"),
            contents: bytemuck::cast_slice(&edges),
            usage: wgpu::BufferUsages::INDEX,
        }),
        vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("houdon_mesh_vertices"),
            contents: bytemuck::cast_slice(&model.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        curvature_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("houdon_mesh_curvature"),
            contents: bytemuck::cast_slice(&model.vertex_curvature()),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("houdon_mesh_indices"),
            contents: bytemuck::cast_slice(&model.indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
    }
}

/// Each triangle edge once, as pairs of vertex indices.
fn edge_indices(indices: &[u32]) -> Vec<u32> {
    let mut seen = std::collections::HashSet::new();
    let mut edges = Vec::new();
    for tri in indices.as_chunks::<3>().0 {
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            if seen.insert((a.min(b), a.max(b))) {
                edges.extend([a, b]);
            }
        }
    }
    // An index buffer may not be empty.
    if edges.is_empty() {
        edges.extend([0, 0]);
    }
    edges
}

/// The Studio viewport for one egui frame.
pub struct ViewportCallback {
    pub frame: Frame,
    /// The viewport's size in physical pixels.
    pub size: [u32; 2],
    pub anti_aliasing: AntiAliasing,
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        match callback_resources.get_mut::<SceneRenderer>() {
            Some(renderer) => renderer.prepare(
                device,
                queue,
                egui_encoder,
                &self.frame,
                self.size,
                self.anti_aliasing,
            ),
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
    fn the_shader_shadow_views_match_the_renderer() {
        let declaration = format!(
            "const MAX_SHADOW_VIEWS: u32 = {}u;",
            crate::MAX_SHADOW_VIEWS
        );
        assert!(include_str!("shader.wgsl").contains(&declaration));
    }

    #[test]
    fn both_shaders_agree_on_the_object_uniforms() {
        let block = |source: &'static str| {
            let start = source
                .find("struct ObjectUniforms {")
                .expect("the struct is declared");
            let end = start + source[start..].find("};").expect("the struct ends");
            source[start..end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(
            block(include_str!("shader.wgsl")),
            block(include_str!("shadow.wgsl"))
        );
    }

    #[test]
    fn uniforms_are_16_byte_aligned() {
        // WebGL2 uniform blocks use std140 layout.
        assert_eq!(size_of::<Globals>() % 16, 0);
        assert_eq!(size_of::<ObjectUniforms>() % 16, 0);
    }
}
