//! The Solid mode's matcap on the GPU, as half floats so Blender's EXR
//! matcaps keep their range.

use std::sync::Arc;

use egui_wgpu::wgpu;
use houdon_core::MatcapImage;

pub struct MatcapTexture {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    /// The image uploaded last, so it is uploaded once.
    loaded: Option<Arc<MatcapImage>>,
}

impl MatcapTexture {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = create(device, 1, 1);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("houdon_matcap_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            texture,
            view,
            sampler,
            loaded: None,
        }
    }

    /// Makes `image` the matcap. Returns true when the texture was replaced
    /// by one of another size, so bind groups using it must be rebuilt.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &Arc<MatcapImage>,
    ) -> bool {
        if self
            .loaded
            .as_ref()
            .is_some_and(|loaded| Arc::ptr_eq(loaded, image))
        {
            return false;
        }
        let size = self.texture.size();
        let resized = size.width != image.width || size.height != image.height;
        if resized {
            self.texture = create(device, image.width, image.height);
            self.view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
        }
        let halves: Vec<u16> = image.rgba.iter().map(|&c| f16_bits(c)).collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&halves),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 8),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        self.loaded = Some(image.clone());
        resized
    }
}

fn create(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("houdon_matcap"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// An f32 as the bits of an IEEE half float, rounded to nearest; values
/// beyond the half range become the largest half.
fn f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exponent == 0xff {
        // Infinity or NaN.
        return sign | 0x7c00 | if mantissa != 0 { 0x200 } else { 0 };
    }
    let half_exponent = exponent - 127 + 15;
    if half_exponent >= 0x1f {
        return sign | 0x7bff;
    }
    if half_exponent <= 0 {
        if half_exponent < -10 {
            return sign;
        }
        // A subnormal half.
        let mantissa = mantissa | 0x80_0000;
        let shift = (14 - half_exponent) as u32;
        let rounded = (mantissa + (1 << (shift - 1))) >> shift;
        return sign | rounded as u16;
    }
    let rounded = ((half_exponent as u32) << 10) + ((mantissa + 0x1000) >> 13);
    // Rounding may carry into the exponent; past the range, clamp.
    sign | rounded.min(0x7bff) as u16
}

#[cfg(test)]
mod tests {
    use super::f16_bits;

    #[test]
    fn half_floats_match_known_values() {
        assert_eq!(f16_bits(0.0), 0x0000);
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(-2.0), 0xc000);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(65504.0), 0x7bff);
        assert_eq!(f16_bits(1e9), 0x7bff, "too large clamps");
        assert_eq!(f16_bits(0.333_333_34), 0x3555);
        assert_eq!(f16_bits(6.0e-8), 0x0001, "smallest subnormal");
    }
}
