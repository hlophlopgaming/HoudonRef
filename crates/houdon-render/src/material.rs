//! GPU side of materials: textures with mipmaps and one bind group per
//! material (bind group 3 of the scene pipeline).

use std::collections::HashMap;

use egui_wgpu::wgpu;
use houdon_core::{Material, Model, ModelId, TextureImage};

/// Material textures, cached per model and dropped with the model.
pub struct MaterialResources {
    pub layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// For parts without a material and materials without some texture:
    /// white base colour and metal-roughness, a flat normal.
    default_bind_group: wgpu::BindGroup,
    white: wgpu::TextureView,
    flat_normal: wgpu::TextureView,
    textures: HashMap<(ModelId, usize, bool), wgpu::TextureView>,
    bind_groups: HashMap<(ModelId, usize), wgpu::BindGroup>,
}

impl MaterialResources {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("houdon_material_layout"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                texture_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("houdon_material_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let solid = |rgba: [u8; 4], srgb| {
            let image = TextureImage {
                width: 1,
                height: 1,
                rgba: rgba.to_vec(),
            };
            upload(device, queue, &image, srgb)
        };
        let white = solid([255; 4], false);
        let flat_normal = solid([128, 128, 255, 255], false);
        let default_bind_group =
            bind_group(device, &layout, &white, &white, &flat_normal, &sampler);
        Self {
            layout,
            sampler,
            default_bind_group,
            white,
            flat_normal,
            textures: HashMap::new(),
            bind_groups: HashMap::new(),
        }
    }

    /// Drops the GPU data of models no longer drawn.
    pub fn retain(&mut self, keep: impl Fn(ModelId) -> bool) {
        self.textures.retain(|(model, _, _), _| keep(*model));
        self.bind_groups.retain(|(model, _), _| keep(*model));
    }

    /// Uploads what material `index` of `model` needs, once.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: ModelId,
        model: &Model,
        index: usize,
    ) {
        if self.bind_groups.contains_key(&(id, index)) {
            return;
        }
        let material: &Material = &model.materials[index];
        let mut view = |texture: Option<usize>, srgb: bool| -> Option<wgpu::TextureView> {
            let texture = texture.filter(|&t| t < model.textures.len())?;
            Some(
                self.textures
                    .entry((id, texture, srgb))
                    .or_insert_with(|| upload(device, queue, &model.textures[texture], srgb))
                    .clone(),
            )
        };
        let base = view(material.base_color_texture, true);
        let metal_rough = view(material.metallic_roughness_texture, false);
        let normal = view(material.normal_texture, false);
        let group = bind_group(
            device,
            &self.layout,
            base.as_ref().unwrap_or(&self.white),
            metal_rough.as_ref().unwrap_or(&self.white),
            normal.as_ref().unwrap_or(&self.flat_normal),
            &self.sampler,
        );
        self.bind_groups.insert((id, index), group);
    }

    /// The bind group of a material, or the default one.
    pub fn bind_group(&self, material: Option<(ModelId, usize)>) -> &wgpu::BindGroup {
        material
            .and_then(|key| self.bind_groups.get(&key))
            .unwrap_or(&self.default_bind_group)
    }
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    base: &wgpu::TextureView,
    metal_rough: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("houdon_material_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(base),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(metal_rough),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(normal),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Uploads an image with a full mipmap chain. `srgb` for colour images,
/// so sampling returns linear values; data images stay as they are.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &TextureImage,
    srgb: bool,
) -> wgpu::TextureView {
    let levels = mip_chain(image);
    let format = if srgb {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("houdon_material_texture"),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, mip) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &mip.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mip.width * 4),
                rows_per_image: Some(mip.height),
            },
            wgpu::Extent3d {
                width: mip.width,
                height: mip.height,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The image and each half-size level below it, down to 1 × 1, by
/// averaging 2 × 2 blocks.
fn mip_chain(image: &TextureImage) -> Vec<TextureImage> {
    let mut levels = vec![image.clone()];
    while let Some(last) = levels.last()
        && (last.width > 1 || last.height > 1)
    {
        let (w, h) = ((last.width / 2).max(1), (last.height / 2).max(1));
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(last.width - 1);
                        let sy = (y * 2 + dy).min(last.height - 1);
                        sum += u32::from(last.rgba[((sy * last.width + sx) * 4 + c) as usize]);
                    }
                    rgba.push((sum / 4) as u8);
                }
            }
        }
        levels.push(TextureImage {
            width: w,
            height: h,
            rgba,
        });
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chains_halve_down_to_one_texel() {
        let image = TextureImage {
            width: 4,
            height: 2,
            rgba: vec![200; 4 * 2 * 4],
        };
        let sizes: Vec<_> = mip_chain(&image)
            .iter()
            .map(|m| (m.width, m.height))
            .collect();
        assert_eq!(sizes, [(4, 2), (2, 1), (1, 1)]);
        assert!(
            mip_chain(&image)
                .iter()
                .all(|m| m.rgba.iter().all(|&v| v == 200))
        );
    }
}
