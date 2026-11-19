//! Light masks on the GPU: one layer of a texture array per light, so each
//! of the 16 lights can shine through its own mask.

use std::sync::Arc;

use egui_wgpu::wgpu;
use houdon_core::{Light, MAX_LIGHTS, MaskImage};

/// Side of every layer, texels: the size of the built-in masks, so they are
/// uploaded as they are. Other images are resampled for the upload only.
pub const MASK_SIZE: u32 = 512;

pub struct MaskTextures {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    /// The image in each layer, kept so a layer is uploaded again only when
    /// its light gets a different mask.
    layers: [Option<Arc<MaskImage>>; MAX_LIGHTS],
    /// How many layers were uploaded so far.
    pub uploads: u64,
}

impl MaskTextures {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("houdon_light_masks"),
            size: wgpu::Extent3d {
                width: MASK_SIZE,
                height: MASK_SIZE,
                depth_or_array_layers: MAX_LIGHTS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        // Repeats for directional lights; spot lights clamp in the shader.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("houdon_light_mask_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            texture,
            view,
            sampler,
            layers: Default::default(),
            uploads: 0,
        }
    }

    /// Uploads the masks of `lights` (light `i` into layer `i`) that changed
    /// and returns each light's layer, or `None` without a mask.
    pub fn prepare(&mut self, queue: &wgpu::Queue, lights: &[Light]) -> Vec<Option<u32>> {
        lights
            .iter()
            .take(MAX_LIGHTS)
            .enumerate()
            .map(|(i, light)| {
                let mask = light.mask.as_ref().filter(|_| light.kind.takes_mask())?;
                let loaded = &mut self.layers[i];
                if !loaded
                    .as_ref()
                    .is_some_and(|image| Arc::ptr_eq(image, &mask.image))
                {
                    self.upload(queue, i as u32, &mask.image);
                    self.layers[i] = Some(mask.image.clone());
                }
                Some(i as u32)
            })
            .collect()
    }

    fn upload(&mut self, queue: &wgpu::Queue, layer: u32, image: &MaskImage) {
        let texels = image.resampled(MASK_SIZE);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &texels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(MASK_SIZE),
                rows_per_image: Some(MASK_SIZE),
            },
            wgpu::Extent3d {
                width: MASK_SIZE,
                height: MASK_SIZE,
                depth_or_array_layers: 1,
            },
        );
        self.uploads += 1;
    }
}
