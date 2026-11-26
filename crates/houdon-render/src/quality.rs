//! Quality levels: how much the renderer spends on shadows and ambient
//! occlusion, chosen from the graphics adapter, stepped down when frames
//! stay slow, and always overridden by the artist's own choice.

use egui_wgpu::wgpu;

/// How much rendering work a frame may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QualityLevel {
    Low,
    Medium,
    High,
}

impl QualityLevel {
    pub const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    /// What this level spends.
    pub fn budget(self) -> RenderQuality {
        match self {
            Self::Low => RenderQuality {
                shadow_atlas_size: 1024,
                ambient_occlusion_samples: 8,
            },
            Self::Medium => RenderQuality {
                shadow_atlas_size: 2048,
                ambient_occlusion_samples: 16,
            },
            Self::High => RenderQuality {
                shadow_atlas_size: 4096,
                ambient_occlusion_samples: 24,
            },
        }
    }

    fn lower(self) -> Option<Self> {
        match self {
            Self::Low => None,
            Self::Medium => Some(Self::Low),
            Self::High => Some(Self::Medium),
        }
    }

    /// The level to start with on `adapter`: low on a software renderer,
    /// high on a discrete GPU with a native API (the full render tier),
    /// medium otherwise.
    pub fn for_adapter(adapter: &wgpu::AdapterInfo) -> Self {
        Self::for_hardware(&adapter.name, adapter.device_type, adapter.backend)
    }

    fn for_hardware(name: &str, device_type: wgpu::DeviceType, backend: wgpu::Backend) -> Self {
        let name = name.to_ascii_lowercase();
        let software = device_type == wgpu::DeviceType::Cpu
            || [
                "swiftshader",
                "llvmpipe",
                "lavapipe",
                "softpipe",
                "software",
                "microsoft basic",
            ]
            .iter()
            .any(|marker| name.contains(marker));
        let native = matches!(
            backend,
            wgpu::Backend::Vulkan | wgpu::Backend::Metal | wgpu::Backend::Dx12
        );
        if software {
            Self::Low
        } else if device_type == wgpu::DeviceType::DiscreteGpu && native {
            Self::High
        } else {
            Self::Medium
        }
    }
}

/// The renderer's budget: the shadow atlas side, texels, and the most
/// ambient occlusion samples per pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderQuality {
    pub shadow_atlas_size: u32,
    pub ambient_occlusion_samples: u32,
}

impl Default for RenderQuality {
    fn default() -> Self {
        QualityLevel::Medium.budget()
    }
}

/// A frame slower than this is slow, seconds (under 24 frames a second).
const SLOW_FRAME: f32 = 1.0 / 24.0;
/// A gap longer than this is the app waiting for input, not a slow frame.
const IDLE_GAP: f32 = 0.5;
/// Frames looked at together, and how many of them must be slow to step down.
const WINDOW: usize = 60;
const SLOW_IN_WINDOW: usize = 48;

/// Chooses the level: the artist's choice if there is one, else the
/// automatic level, which only ever steps down while frames stay slow (up
/// and down again would make the picture flicker between levels).
#[derive(Debug, Clone)]
pub struct QualityGovernor {
    automatic: QualityLevel,
    manual: Option<QualityLevel>,
    recent: std::collections::VecDeque<bool>,
}

impl QualityGovernor {
    pub fn new(start: QualityLevel, manual: Option<QualityLevel>) -> Self {
        Self {
            automatic: start,
            manual,
            recent: std::collections::VecDeque::with_capacity(WINDOW),
        }
    }

    pub fn level(&self) -> QualityLevel {
        self.manual.unwrap_or(self.automatic)
    }

    /// The level the automatic choice is at, whatever the artist chose.
    pub fn automatic(&self) -> QualityLevel {
        self.automatic
    }

    pub fn manual(&self) -> Option<QualityLevel> {
        self.manual
    }

    /// The artist's choice; `None` lets the level follow the hardware.
    pub fn set_manual(&mut self, manual: Option<QualityLevel>) {
        self.manual = manual;
        self.recent.clear();
    }

    /// Notes how long the last frame took, seconds. Returns whether the
    /// automatic level stepped down.
    pub fn frame(&mut self, seconds: f32) -> bool {
        if self.manual.is_some() || !seconds.is_finite() || seconds > IDLE_GAP {
            return false;
        }
        if self.recent.len() == WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back(seconds > SLOW_FRAME);
        let slow = self.recent.iter().filter(|&&slow| slow).count();
        if self.recent.len() == WINDOW
            && slow >= SLOW_IN_WINDOW
            && let Some(lower) = self.automatic.lower()
        {
            self.automatic = lower;
            self.recent.clear();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hardware_sets_the_starting_level() {
        use wgpu::{Backend, DeviceType};
        let start = QualityLevel::for_hardware;
        assert_eq!(
            start("llvmpipe (LLVM 17)", DeviceType::Cpu, Backend::Vulkan),
            QualityLevel::Low
        );
        assert_eq!(
            start(
                "ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device))",
                DeviceType::Other,
                Backend::Gl
            ),
            QualityLevel::Low,
            "a software WebGL2 adapter"
        );
        assert_eq!(
            start("GeForce RTX 3060", DeviceType::DiscreteGpu, Backend::Vulkan),
            QualityLevel::High
        );
        assert_eq!(
            start("Intel Iris Xe", DeviceType::IntegratedGpu, Backend::Vulkan),
            QualityLevel::Medium
        );
        assert_eq!(
            start(
                "ANGLE (NVIDIA GeForce RTX 3060)",
                DeviceType::DiscreteGpu,
                Backend::Gl
            ),
            QualityLevel::Medium,
            "WebGL2 is the base tier even on a strong GPU"
        );
    }

    #[test]
    fn sustained_slow_frames_step_down_one_level() {
        let mut governor = QualityGovernor::new(QualityLevel::High, None);
        for _ in 0..WINDOW - 1 {
            assert!(!governor.frame(0.1));
        }
        assert!(governor.frame(0.1), "a whole window of slow frames");
        assert_eq!(governor.level(), QualityLevel::Medium);
        // Fast frames never step back up.
        for _ in 0..WINDOW * 3 {
            governor.frame(0.005);
        }
        assert_eq!(governor.level(), QualityLevel::Medium);
    }

    #[test]
    fn a_few_slow_frames_or_idle_gaps_do_not_step_down() {
        let mut governor = QualityGovernor::new(QualityLevel::High, None);
        for i in 0..WINDOW * 4 {
            // Every third frame slow, and long idle gaps in between.
            governor.frame(if i % 3 == 0 { 0.1 } else { 0.01 });
            governor.frame(5.0);
        }
        assert_eq!(governor.level(), QualityLevel::High);
    }

    #[test]
    fn the_artists_choice_is_never_changed_automatically() {
        let mut governor = QualityGovernor::new(QualityLevel::High, Some(QualityLevel::High));
        for _ in 0..WINDOW * 3 {
            assert!(!governor.frame(0.2));
        }
        assert_eq!(governor.level(), QualityLevel::High);
        assert_eq!(governor.automatic(), QualityLevel::High);
        governor.set_manual(None);
        assert_eq!(governor.level(), QualityLevel::High);
    }

    #[test]
    fn higher_levels_spend_more() {
        let budgets: Vec<_> = QualityLevel::ALL.iter().map(|l| l.budget()).collect();
        assert!(
            budgets
                .windows(2)
                .all(|w| w[0].shadow_atlas_size < w[1].shadow_atlas_size
                    && w[0].ambient_occlusion_samples < w[1].ambient_occlusion_samples)
        );
        assert_eq!(
            RenderQuality::default().shadow_atlas_size,
            crate::DEFAULT_ATLAS_SIZE
        );
    }
}
