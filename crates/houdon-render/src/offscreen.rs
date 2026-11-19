//! Pictures of a scene without a window: PNG export and render tests.

use egui_wgpu::wgpu;

use crate::{Frame, SceneRenderer};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

/// An RGBA picture, 8 bits per channel, sRGB, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    pub fn pixels(&self) -> impl Iterator<Item = [u8; 4]> + '_ {
        self.rgba.as_chunks::<4>().0.iter().copied()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OffscreenError {
    #[error("no GPU adapter available: {0}")]
    NoAdapter(#[from] wgpu::RequestAdapterError),
    #[error("could not open the GPU device: {0}")]
    NoDevice(#[from] wgpu::RequestDeviceError),
}

/// Renders frames into images on its own device.
pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: SceneRenderer,
}

impl OffscreenRenderer {
    /// Opens a GPU device of its own, limited to the WebGL2 feature level so
    /// what passes here also runs on the base render tier.
    pub fn new_headless() -> Result<Self, OffscreenError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("houdon_offscreen_device"),
                required_limits:
                    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            }))?;
        Ok(Self::from_device(device, queue))
    }

    /// Uses an existing device, for example the app's own.
    pub fn from_device(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let renderer = SceneRenderer::new(&device, COLOR_FORMAT, Some(DEPTH_FORMAT));
        Self {
            device,
            queue,
            renderer,
        }
    }

    /// Renders `frame` into a `width` × `height` picture.
    pub fn render(&mut self, frame: &Frame, width: u32, height: u32) -> Image {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("houdon_offscreen_color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("houdon_offscreen_depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer.prepare(&self.device, &self.queue, frame);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("houdon_offscreen"),
            });
        {
            let [r, g, b] = frame.clear_color().map(|c| f64::from(c) / 255.0);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("houdon_offscreen_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            self.renderer.draw(&mut pass);
        }

        // Rows in a buffer copy must be 256-byte aligned.
        let unpadded_row = width * 4;
        let padded_row = unpadded_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("houdon_offscreen_readback"),
            size: u64::from(padded_row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            if let Err(error) = result {
                log::error!("reading back the offscreen picture failed: {error}");
            }
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU device lost while rendering offscreen");

        let mapped = slice
            .get_mapped_range()
            .expect("the readback buffer was mapped above");
        let mut rgba = Vec::with_capacity((unpadded_row * height) as usize);
        for row in mapped.chunks(padded_row as usize) {
            rgba.extend_from_slice(&row[..unpadded_row as usize]);
        }
        drop(mapped);
        readback.unmap();
        Image {
            width,
            height,
            rgba,
        }
    }
}
