//! wgpu device setup and quality tiers.

use thiserror::Error;

/// Renderer quality tier selected from adapter limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuTier {
    /// Vulkan/Metal/DX12/WebGPU with compute + storage buffers.
    Full,
    /// WebGL2 / downlevel: no compute, CPU light lists.
    Downlevel,
}

/// Errors from GPU init.
#[derive(Debug, Error)]
pub enum DeviceError {
    /// No suitable adapter.
    #[error("no wgpu adapter")]
    NoAdapter,
    /// Device request failed.
    #[error("wgpu device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    /// Surface / texture create failed.
    #[error("{0}")]
    Message(String),
}

/// GPU handle owned by the renderer (library-first: no window required).
pub struct GpuContext {
    /// Instance.
    pub instance: wgpu::Instance,
    /// Adapter.
    pub adapter: wgpu::Adapter,
    /// Device.
    pub device: wgpu::Device,
    /// Queue.
    pub queue: wgpu::Queue,
    /// Selected tier.
    pub tier: GpuTier,
}

impl GpuContext {
    /// Create a headless-capable context (no surface).
    pub async fn headless() -> Result<Self, DeviceError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| DeviceError::NoAdapter)?;
        Self::from_adapter(instance, adapter).await
    }

    /// Finish device creation from an existing instance + adapter (surface-aware shells).
    pub async fn from_adapter(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
    ) -> Result<Self, DeviceError> {
        let info = adapter.get_info();
        let downlevel = adapter.get_downlevel_capabilities();
        let tier = if info.backend == wgpu::Backend::Gl
            || !downlevel
                .flags
                .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        {
            GpuTier::Downlevel
        } else {
            GpuTier::Full
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("room"),
                required_features: wgpu::Features::empty(),
                required_limits: if tier == GpuTier::Downlevel {
                    wgpu::Limits::downlevel_webgl2_defaults()
                } else {
                    wgpu::Limits::default()
                },
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;
        let _ = info;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            tier,
        })
    }
}
