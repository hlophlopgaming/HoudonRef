//! Pictures of a scene without a window: PNG export and render tests.

use std::sync::{Arc, Mutex};

use egui_wgpu::wgpu;

use crate::{AntiAliasing, Frame, SceneRenderer};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

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

/// The sizes a picture can be exported at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportSize {
    /// The viewport's own size.
    Window,
    /// Twice the viewport's size per side.
    Double,
    /// 4096 pixels on the long side.
    Large,
}

impl ExportSize {
    pub const ALL: [Self; 3] = [Self::Window, Self::Double, Self::Large];
    /// The long side of [`ExportSize::Large`], pixels.
    pub const LARGE_SIDE: u32 = 4096;

    /// The picture's size for a viewport of `window` pixels, keeping its
    /// proportions and staying within `max_side`.
    pub fn pixels(self, window: [u32; 2], max_side: u32) -> [u32; 2] {
        let [w, h] = window.map(|side| side.max(1) as f32);
        let long = w.max(h);
        let wanted = match self {
            Self::Window => long,
            Self::Double => long * 2.0,
            Self::Large => Self::LARGE_SIDE as f32,
        };
        let scale = wanted.min(max_side as f32) / long;
        [w, h].map(|side| ((side * scale).round() as u32).max(1))
    }
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
    #[cfg(not(target_arch = "wasm32"))]
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
        let renderer = SceneRenderer::new(
            &device,
            &queue,
            COLOR_FORMAT,
            None,
            crate::DEFAULT_ATLAS_SIZE,
        );
        Self {
            device,
            queue,
            renderer,
        }
    }

    /// The side of the shadow atlas the last picture used, texels.
    pub fn shadow_atlas_size(&self) -> u32 {
        self.renderer.shadow_atlas_size()
    }

    /// How many times the shadow maps were redrawn so far.
    pub fn shadow_map_builds(&self) -> u64 {
        self.renderer.shadow_map_builds()
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Renders `frame` into a `width` × `height` picture with the
    /// anti-aliasing of exported pictures.
    pub fn render(&mut self, frame: &Frame, width: u32, height: u32) -> Image {
        self.render_with(frame, width, height, AntiAliasing::EXPORT)
    }

    /// Renders `frame` into a `width` × `height` picture, waiting for it.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_with(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        anti_aliasing: AntiAliasing,
    ) -> Image {
        let pending = self.start(frame, width, height, anti_aliasing);
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU device lost while rendering offscreen");
        pending
            .take()
            .expect("the picture was read back after waiting")
            .expect("reading back the offscreen picture")
    }

    /// The largest picture side this device can render, pixels.
    pub fn max_side(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    /// Checks whether `pending` has arrived, without waiting: on the web the
    /// GPU cannot be waited for, so the app asks again on later frames.
    pub fn poll(&self, pending: &PendingImage) -> Option<Result<Image, String>> {
        // Polling only moves finished work along; an error shows in the map.
        let _ = self.device.poll(wgpu::PollType::Poll);
        pending.take()
    }

    /// Starts rendering `frame` into a `width` × `height` picture and
    /// reading it back; [`OffscreenRenderer::poll`] returns it once there.
    pub fn start(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        anti_aliasing: AntiAliasing,
    ) -> PendingImage {
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
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("houdon_offscreen"),
            });
        self.renderer.prepare(
            &self.device,
            &self.queue,
            &mut encoder,
            frame,
            [width, height],
            anti_aliasing,
        );
        {
            // The composite covers every pixel; the clear only sets a start.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("houdon_offscreen_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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

        let state = Arc::new(Mutex::new(MapState::Waiting));
        let done = Arc::clone(&state);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                *done.lock().unwrap() = match result {
                    Ok(()) => MapState::Mapped,
                    Err(error) => MapState::Failed(error.to_string()),
                };
            });
        PendingImage {
            readback,
            state,
            width,
            height,
            padded_row,
        }
    }
}

enum MapState {
    Waiting,
    Mapped,
    Failed(String),
}

/// A picture on its way back from the GPU.
pub struct PendingImage {
    readback: wgpu::Buffer,
    state: Arc<Mutex<MapState>>,
    width: u32,
    height: u32,
    padded_row: u32,
}

impl PendingImage {
    /// The picture once read back, or why it could not be; `None` while
    /// it is still on its way.
    fn take(&self) -> Option<Result<Image, String>> {
        match &*self.state.lock().unwrap() {
            MapState::Waiting => None,
            MapState::Failed(reason) => Some(Err(reason.clone())),
            MapState::Mapped => {
                let unpadded_row = (self.width * 4) as usize;
                let mapped = self.readback.slice(..).get_mapped_range().ok()?;
                let mut rgba = Vec::with_capacity(unpadded_row * self.height as usize);
                for row in mapped.chunks(self.padded_row as usize) {
                    rgba.extend_from_slice(&row[..unpadded_row]);
                }
                drop(mapped);
                self.readback.unmap();
                Some(Ok(Image {
                    width: self.width,
                    height: self.height,
                    rgba,
                }))
            }
        }
    }
}
