//! The pass that puts the rendered scene on screen: as it is, or as a
//! value study (squint, grey, tones). The scene is drawn into a picture of
//! its own first, so this pass can read neighbouring pixels.

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use houdon_core::{AmbientOcclusion, Lines, ShadingMode, ValueMode};

use crate::Frame;

/// The scene's second picture: normals and distances as the camera sees
/// them, for the screen-space cavity (see `FragmentOutput` in shader.wgsl).
pub const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// How much of each point's hemisphere is covered, for ambient occlusion.
const OCCLUSION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// Depth buffer of the scene's own picture.
pub const SCENE_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

/// How edges are smoothed. `samples` is the multisampling of the scene
/// pass (1 or 4, both on the base render tier); `supersample` draws the
/// scene at that many times the output size per side and averages it down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AntiAliasing {
    pub samples: u32,
    pub supersample: u32,
}

impl AntiAliasing {
    /// Jagged edges, the cheapest picture.
    pub const OFF: Self = Self {
        samples: 1,
        supersample: 1,
    };
    /// The viewport: 4× MSAA.
    pub const VIEWPORT: Self = Self {
        samples: 4,
        supersample: 1,
    };
    /// Exported pictures: 4× MSAA over a picture twice the size per side.
    pub const EXPORT: Self = Self {
        samples: 4,
        supersample: 2,
    };

    /// The supported settings nearest to these: 1 or 4 samples and a
    /// supersampling of 1 or 2.
    pub fn normalized(self) -> Self {
        Self {
            samples: if self.samples > 1 { 4 } else { 1 },
            supersample: self.supersample.clamp(1, 2),
        }
    }
}

/// The squint radius at full strength, as a share of the picture height.
const SQUINT_SHARE: f32 = 0.03;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ValueUniforms {
    params: [f32; 4],
    blur: [f32; 4],
    cavity: [f32; 4],
    lines: [f32; 4],
    line_color: [f32; 4],
    ambient_occlusion: [f32; 4],
    lens: [f32; 4],
}

/// The scene's picture and depth at the current size.
pub struct SceneTarget {
    /// The picture's size: the output's times the supersampling.
    pub size: [u32; 2],
    pub samples: u32,
    /// The single-sampled picture the composite reads.
    color_view: wgpu::TextureView,
    /// The multisampled picture drawn into and resolved into `color_view`.
    msaa_view: Option<wgpu::TextureView>,
    /// The colour picture's depth, multisampled like it.
    pub depth_view: wgpu::TextureView,
    /// The surfaces for the screen-space overlays (normal and distance,
    /// see `fs_surface`) and their own depth; never multisampled.
    pub surface_view: wgpu::TextureView,
    pub surface_depth_view: wgpu::TextureView,
    /// Ambient occlusion's cover, before it is smoothed.
    occlusion_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    occlusion_bind_group: wgpu::BindGroup,
}

impl SceneTarget {
    /// The view to draw the scene's colour into, and the one it resolves
    /// to when multisampled.
    pub fn color_attachment(&self) -> (&wgpu::TextureView, Option<&wgpu::TextureView>) {
        match &self.msaa_view {
            Some(msaa) => (msaa, Some(&self.color_view)),
            None => (&self.color_view, None),
        }
    }
}

/// Whether the picture of surfaces is needed: for ambient occlusion in the
/// Lit mode, the screen-space cavity or the contour lines.
pub fn needs_surfaces(shading: &houdon_core::Shading) -> bool {
    (shading.ambient_occlusion.enabled && shading.mode == ShadingMode::Lit)
        || shading.cavity.screen
        || shading.lines.contours
}

pub struct Composite {
    pipeline: wgpu::RenderPipeline,
    occlusion_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    occlusion_layout: wgpu::BindGroupLayout,
    /// Whether the frame being prepared has ambient occlusion.
    ambient_occlusion: bool,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    color_format: wgpu::TextureFormat,
    pub target: Option<SceneTarget>,
}

impl Composite {
    /// `color_format` is both the scene picture's and the output's format;
    /// `output_depth` the depth format of the pass the picture is put into.
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        output_depth: Option<wgpu::TextureFormat>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("houdon_composite_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("houdon_composite_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(
                            size_of::<ValueUniforms>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let occlusion_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("houdon_occlusion_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("houdon_composite_pipeline_layout"),
            bind_group_layouts: &[Some(&layout), Some(&occlusion_layout)],
            immediate_size: 0,
        });
        let occlusion_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("houdon_occlusion_pipeline_layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let occlusion_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("houdon_occlusion_pipeline"),
            layout: Some(&occlusion_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_composite"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_occlusion"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: OCCLUSION_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("houdon_composite_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_composite"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            // The output pass may have a depth buffer (egui's); ignore it.
            depth_stencil: output_depth.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_composite"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("houdon_composite_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("houdon_value_uniforms"),
            size: size_of::<ValueUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            occlusion_pipeline,
            layout,
            occlusion_layout,
            ambient_occlusion: false,
            sampler,
            uniforms,
            color_format,
            target: None,
        }
    }

    /// Makes sure the scene picture fits an output of `output_size` with
    /// `anti_aliasing`, and writes the settings of `frame`'s overlays: the value
    /// study, the screen-space cavity and the contour lines.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_size: [u32; 2],
        anti_aliasing: AntiAliasing,
        frame: &Frame,
    ) {
        let shading = &frame.shading;
        let (value, cavity, lines) = (&shading.value, &shading.cavity, &shading.lines);
        let max = device.limits().max_texture_dimension_2d;
        // Without supersampling when that would pass the device's largest
        // texture: clamping each side apart would change the proportions.
        let output_size = output_size.map(|side| side.clamp(1, max));
        let fits = output_size
            .iter()
            .all(|&side| side * anti_aliasing.supersample <= max);
        let scale = if fits { anti_aliasing.supersample } else { 1 };
        let size = output_size.map(|side| side * scale);
        let samples = anti_aliasing.samples;
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.size != size || t.samples != samples)
        {
            self.target = Some(self.create_target(device, size, samples));
        }
        let (mode, tones) = match value.mode {
            ValueMode::Off => (0.0, 0.0),
            ValueMode::Grey => (1.0, 0.0),
            ValueMode::Tones(n) => (
                2.0,
                f32::from(n.clamp(ValueMode::MIN_TONES, ValueMode::MAX_TONES)),
            ),
        };
        let linear = if self.color_format.is_srgb() {
            1.0
        } else {
            0.0
        };
        let radius = value.squint.clamp(0.0, 1.0) * SQUINT_SHARE * size[1] as f32;
        let uniforms = ValueUniforms {
            params: [mode, tones, value.balance.clamp(-1.0, 1.0), linear],
            blur: [1.0 / size[0] as f32, 1.0 / size[1] as f32, radius, 0.0],
            cavity: if cavity.screen {
                [cavity.screen_ridge, cavity.screen_hollow, scale as f32, 0.0]
            } else {
                [0.0, 0.0, scale as f32, 0.0]
            },
            lines: if lines.contours {
                let thickness = lines.thickness.clamp(1.0, Lines::MAX_THICKNESS);
                let sensitivity = lines.fold_sensitivity.clamp(0.0, 1.0);
                [
                    thickness * scale as f32 * 0.5,
                    1.6 + (0.15 - 1.6) * sensitivity,
                    0.0,
                    0.0,
                ]
            } else {
                [0.0; 4]
            },
            line_color: lines.color.extend(1.0).to_array(),
            ambient_occlusion: {
                let ao = &shading.ambient_occlusion;
                let on = ao.enabled && shading.mode == ShadingMode::Lit;
                self.ambient_occlusion = on && ao.strength > 0.0;
                [
                    if on { ao.strength.clamp(0.0, 1.0) } else { 0.0 },
                    ao.radius.clamp(0.01, AmbientOcclusion::MAX_RADIUS),
                    ao.samples
                        .min(frame.quality.ambient_occlusion_samples)
                        .clamp(AmbientOcclusion::MIN_SAMPLES, AmbientOcclusion::MAX_SAMPLES)
                        as f32,
                    0.0,
                ]
            },
            lens: [frame.tan_half_fov, frame.aspect.max(1e-3), 0.0, 0.0],
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Works out ambient occlusion's cover from the picture of surfaces,
    /// when the prepared frame has it. Call after the surface pass.
    pub fn draw_occlusion(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(target) = self.target.as_ref().filter(|_| self.ambient_occlusion) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("houdon_occlusion_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.occlusion_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.occlusion_pipeline);
        pass.set_bind_group(0, &target.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Draws the scene picture, as a value study if one is on.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(target) = &self.target {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &target.bind_group, &[]);
            pass.set_bind_group(1, &target.occlusion_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    fn create_target(&self, device: &wgpu::Device, size: [u32; 2], samples: u32) -> SceneTarget {
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let texture = |label, format, usage, sample_count| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let color_view = texture(
            "houdon_scene_picture",
            self.color_format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            1,
        );
        let msaa_view = (samples > 1).then(|| {
            texture(
                "houdon_scene_picture_msaa",
                self.color_format,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
                samples,
            )
        });
        let surface_view = texture(
            "houdon_scene_surfaces",
            NORMAL_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            1,
        );
        let surface_depth_view = texture(
            "houdon_scene_surfaces_depth",
            SCENE_DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            1,
        );
        let occlusion_view = texture(
            "houdon_scene_occlusion",
            OCCLUSION_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            1,
        );
        let occlusion_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("houdon_occlusion_bind_group"),
            layout: &self.occlusion_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&occlusion_view),
            }],
        });
        let depth_view = texture(
            "houdon_scene_depth",
            SCENE_DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            samples,
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("houdon_composite_bind_group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&surface_view),
                },
            ],
        });
        SceneTarget {
            size,
            samples,
            color_view,
            msaa_view,
            depth_view,
            surface_view,
            surface_depth_view,
            occlusion_view,
            bind_group,
            occlusion_bind_group,
        }
    }
}
