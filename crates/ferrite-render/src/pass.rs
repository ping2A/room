//! Basic unlit geometry pass (walls + flats).

use crate::camera::Camera;
use crate::device::GpuContext;
use crate::fuzz::FuzzType;
use crate::lightmode::LightMode;
use crate::lights::{
    ClusterAssignment, LightUniforms, bake_1d_shadowmaps, cluster_tex_dims, pack_cluster_rgba,
    pack_shadowmap_r32, shadowmap_tex_dims,
};
use crate::materials::MaterialCatalog;
use crate::mesh::{FlatVertex, ViewUniforms, WallVertex};
use crate::scene::{MaterialRange, SceneLists};
use bytemuck::{Pod, bytes_of, cast_slice};
use ferrite_vfs::Vfs;
use glam::Mat4;

const UNLIT_WGSL: &str = include_str!("shaders/unlit.wgsl");

/// Owns pipelines and per-frame buffers for the unlit pass.
pub struct UnlitPass {
    pipeline: wgpu::RenderPipeline,
    /// Floors/ceilings with back-face cull so elevated floor *undersides* do not
    /// paint over outdoor sky (winding is normalized in `push_flat_tri_oriented`).
    flats_pipeline: wgpu::RenderPipeline,
    /// Alpha-blended pipeline (translucent walls/flats + fuzzy sprites).
    sprite_pipeline: wgpu::RenderPipeline,
    /// Opaque sprite billboards (Doom patches: opaque or discard holes).
    opaque_sprite_pipeline: wgpu::RenderPipeline,
    /// Write stencil = ref (Always/Replace); no color or depth writes — portal/mirror mask.
    stencil_mask_pipeline: wgpu::RenderPipeline,
    /// Draw only where stencil == ref (Equal/keep) — portal/mirror sub-scene.
    stencil_equal_pipeline: wgpu::RenderPipeline,
    view_buf: wgpu::Buffer,
    light_buf: wgpu::Buffer,
    view_bind: wgpu::BindGroup,
    /// Cluster light bitmasks (RGBA8, 64×54).
    cluster_tex: wgpu::Texture,
    /// Kept alive for [`Self::view_bind`].
    #[allow(dead_code)]
    cluster_view: wgpu::TextureView,
    /// 1D shadowmap atlas (R32F, 512 × max lights).
    shadow_tex: wgpu::Texture,
    /// Kept alive for [`Self::view_bind`].
    #[allow(dead_code)]
    shadow_view: wgpu::TextureView,
    /// Lightmap atlas (default 1×1 black); kept alive for [`Self::lightmap_view`].
    #[allow(dead_code)]
    lightmap_tex: wgpu::Texture,
    lightmap_view: wgpu::TextureView,
    lightmap_samp: wgpu::Sampler,
    /// View+lights+cluster+shadow+lightmap bind layout (for rebinds).
    view_bgl: wgpu::BindGroupLayout,
    mat_bgl: wgpu::BindGroupLayout,
    mat_bind: wgpu::BindGroup,
    /// Kept alive for the default material bind group.
    _default_tex: wgpu::Texture,
    _default_view: wgpu::TextureView,
    /// 1×1 black brightmap (no brightmap regions).
    _black_bright_tex: wgpu::Texture,
    _black_bright_view: wgpu::TextureView,
    /// 1×1 flat normal (0.5, 0.5, 1.0).
    _flat_normal_tex: wgpu::Texture,
    _flat_normal_view: wgpu::TextureView,
    /// 1×1 metal-rough default (G=0.5 roughness, B=0 metallic).
    _default_mr_tex: wgpu::Texture,
    _default_mr_view: wgpu::TextureView,
    _default_samp: wgpu::Sampler,
    wall_buf: wgpu::Buffer,
    wall_cap: u64,
    flat_buf: wgpu::Buffer,
    flat_cap: u64,
    /// Vertex offset of translucent flats inside [`Self::flat_buf`].
    translucent_flat_base: u32,
    /// Vertex offset of translucent walls inside [`Self::wall_buf`].
    translucent_wall_base: u32,
    sky_buf: wgpu::Buffer,
    sky_cap: u64,
    sprite_buf: wgpu::Buffer,
    sprite_cap: u64,
    /// Active sector light mode.
    pub light_mode: LightMode,
    /// Software-renderer visibility (`r_visibility` / globVis stand-in).
    pub glob_vis: f32,
    /// Radial fog density (`0` = off).
    pub fog_density: f32,
    /// Spectre fuzz variant (`gl_fuzztype`).
    pub fuzz_type: FuzzType,
    /// Elapsed seconds (fuzz animation).
    pub time: f32,
    /// Optional world clip plane (`n·p + d > 0` discarded); zero xyz = off.
    pub clip_plane: [f32; 4],
    /// Lightmap blend strength (`0` = off).
    pub lightmap_strength: f32,
}

impl UnlitPass {
    /// Create pipelines against an existing device.
    pub fn new(gpu: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("unlit"),
            source: wgpu::ShaderSource::Wgsl(UNLIT_WGSL.into()),
        });
        let view_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("view-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let mat_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mat-bgl"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("unlit-layout"),
            bind_group_layouts: &[&view_bgl, &mat_bgl],
            push_constant_ranges: &[],
        });
        // Walls: no cull (one-sided content, winding not always outward).
        // Slight depth bias reduces floor/wall z-fight shimmer.
        let pipeline = make_unlit_pipeline_bias(
            device,
            &shader,
            &layout,
            target_format,
            "unlit-pipe",
            wgpu::BlendState::REPLACE,
            true,
            None,
            wgpu::ColorWrites::ALL,
            wgpu::StencilState::default(),
            wgpu::CompareFunction::LessEqual,
            crate::DEPTH_STENCIL_FORMAT,
            wgpu::DepthBiasState {
                constant: 1,
                slope_scale: 1.0,
                clamp: 0.0,
            },
        );
        // Flats: cull back faces so you never see the underside of a high floor
        // stretched across the sky (E1M1 courtyard).
        let flats_pipeline = make_unlit_pipeline_bias(
            device,
            &shader,
            &layout,
            target_format,
            "unlit-flats-pipe",
            wgpu::BlendState::REPLACE,
            true,
            Some(wgpu::Face::Back),
            wgpu::ColorWrites::ALL,
            wgpu::StencilState::default(),
            wgpu::CompareFunction::LessEqual,
            crate::DEPTH_STENCIL_FORMAT,
            wgpu::DepthBiasState {
                constant: 1,
                slope_scale: 1.0,
                clamp: 0.0,
            },
        );
        // Translucent walls/flats share this alpha pipeline.
        let sprite_pipeline = make_unlit_pipeline_bias(
            device,
            &shader,
            &layout,
            target_format,
            "sprite-pipe",
            wgpu::BlendState::ALPHA_BLENDING,
            false,
            None,
            wgpu::ColorWrites::ALL,
            wgpu::StencilState::default(),
            wgpu::CompareFunction::LessEqual,
            crate::DEPTH_STENCIL_FORMAT,
            wgpu::DepthBiasState {
                constant: -8,
                slope_scale: -2.0,
                clamp: 0.0,
            },
        );
        // Monster/item billboards: REPLACE + depth write + strong camera bias so
        // they cannot wash out or lose to coplanar walls (E1M1 "invisible killer").
        let opaque_sprite_pipeline = make_unlit_pipeline_bias(
            device,
            &shader,
            &layout,
            target_format,
            "opaque-sprite-pipe",
            wgpu::BlendState::REPLACE,
            true,
            None,
            wgpu::ColorWrites::ALL,
            wgpu::StencilState::default(),
            wgpu::CompareFunction::LessEqual,
            crate::DEPTH_STENCIL_FORMAT,
            wgpu::DepthBiasState {
                constant: -48,
                slope_scale: -8.0,
                clamp: 0.0,
            },
        );
        let stencil_write = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Replace,
        };
        // Dual MRT (scene RGBA + dummy R8). Metal ties stencil Replace to
        // colour-target-0 write enable — empty slot0 skips stencil even if aux
        // writes. Mask writes slot0 (Equal overwrites); aux kept for pass shape.
        let stencil_mask_pipeline = make_mrt_pipeline(
            device,
            &shader,
            &layout,
            target_format,
            "fs_stencil_mask",
            "stencil-mask-pipe",
            false,
            None,
            wgpu::ColorWrites::ALL,
            wgpu::ColorWrites::ALL,
            wgpu::StencilState {
                front: stencil_write,
                back: stencil_write,
                read_mask: 0xff,
                write_mask: 0xff,
            },
            wgpu::CompareFunction::Always,
        );
        let stencil_eq = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Equal,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Keep,
        };
        let stencil_equal_pipeline = make_mrt_pipeline(
            device,
            &shader,
            &layout,
            target_format,
            "fs_main_mrt",
            "stencil-equal-pipe",
            true,
            Some(wgpu::Face::Back),
            wgpu::ColorWrites::ALL,
            wgpu::ColorWrites::empty(),
            wgpu::StencilState {
                front: stencil_eq,
                back: stencil_eq,
                read_mask: 0xff,
                write_mask: 0x00,
            },
            wgpu::CompareFunction::LessEqual,
        );
        let view_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("view-ubo"),
            size: std::mem::size_of::<ViewUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let light_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dyn-lights-ubo"),
            size: std::mem::size_of::<LightUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (cw, ch) = cluster_tex_dims();
        let cluster_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cluster-masks"),
            size: wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Non-sRGB so bitmasks round-trip via textureLoad.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let cluster_view = cluster_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let (sw, sh) = shadowmap_tex_dims();
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadowmaps-1d"),
            size: wgpu::Extent3d {
                width: sw,
                height: sh,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let lightmap_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("default-lightmap"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &lightmap_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8, 0, 0, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let lightmap_view = lightmap_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let lightmap_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("lightmap-samp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("view-bg"),
            layout: &view_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: view_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&cluster_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&lightmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&lightmap_samp),
                },
            ],
        });

        let default_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("default-white"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &default_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0x8C, 0x8C, 0x99, 0xFF],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let default_view = default_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let black_bright_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("default-black-brightmap"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &black_bright_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0, 0, 0, 0xFF],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let black_bright_view =
            black_bright_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let flat_normal_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("default-flat-normal"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &flat_normal_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[128, 128, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let flat_normal_view = flat_normal_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let default_mr_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("default-metal-rough"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &default_mr_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8, 128, 0, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let default_mr_view = default_mr_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let default_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default-samp"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let mat_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mat-bg"),
            layout: &mat_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&default_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&default_samp),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&black_bright_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&flat_normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&default_mr_view),
                },
            ],
        });

        let wall_buf = empty_vb(device, "walls", 64 * 32);
        let flat_buf = empty_vb(device, "flats", 64 * 32);
        let sky_buf = empty_vb(device, "sky", 64 * 32);
        let sprite_buf = empty_vb(device, "sprites", 64 * 32);
        Self {
            pipeline,
            flats_pipeline,
            sprite_pipeline,
            opaque_sprite_pipeline,
            stencil_mask_pipeline,
            stencil_equal_pipeline,
            view_buf,
            light_buf,
            view_bind,
            cluster_tex,
            cluster_view,
            shadow_tex,
            shadow_view,
            lightmap_tex,
            lightmap_view,
            lightmap_samp,
            view_bgl,
            mat_bgl,
            mat_bind,
            _default_tex: default_tex,
            _default_view: default_view,
            _black_bright_tex: black_bright_tex,
            _black_bright_view: black_bright_view,
            _flat_normal_tex: flat_normal_tex,
            _flat_normal_view: flat_normal_view,
            _default_mr_tex: default_mr_tex,
            _default_mr_view: default_mr_view,
            _default_samp: default_samp,
            wall_buf,
            wall_cap: 64 * 32,
            flat_buf,
            flat_cap: 64 * 32,
            translucent_flat_base: 0,
            translucent_wall_base: 0,
            sky_buf,
            sky_cap: 64 * 32,
            sprite_buf,
            sprite_cap: 64 * 32,
            light_mode: LightMode::Standard,
            // GZDoom `GlobVis` is derived from `r_visibility` × view size and is
            // typically hundreds; 8 made every wall/sprite beyond ~15mu sit at the
            // DoomSoftware floor so monsters vanished into dark sectors.
            glob_vis: 512.0,
            fog_density: 0.0,
            fuzz_type: FuzzType::Shadow,
            time: 0.0,
            clip_plane: [0.0; 4],
            lightmap_strength: 0.0,
        }
    }

    /// ACS / MAPINFO fog density (0 = off).
    pub fn set_fog_density(&mut self, density: f32) {
        self.fog_density = density.max(0.0);
    }

    /// Enable / clear the world-space clip plane (mirror / portal stand-in).
    pub fn set_clip_plane(&mut self, plane: [f32; 4]) {
        self.clip_plane = plane;
    }

    /// Disable clip plane.
    pub fn clear_clip_plane(&mut self) {
        self.clip_plane = [0.0; 4];
    }

    /// Bind a ZDRay / FLMP lightmap atlas view (replaces default black).
    pub fn set_lightmap(&mut self, device: &wgpu::Device, view: &wgpu::TextureView, strength: f32) {
        self.lightmap_strength = strength.max(0.0);
        // Keep owned default tex; rebind view_bind to the caller's atlas view.
        self.view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("view-bg"),
            layout: &self.view_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.view_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.light_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.cluster_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.lightmap_samp),
                },
            ],
        });
    }

    /// Clear lightmap (1×1 black, strength 0).
    pub fn clear_lightmap(&mut self, device: &wgpu::Device) {
        self.lightmap_strength = 0.0;
        self.view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("view-bg"),
            layout: &self.view_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.view_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.light_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.cluster_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.lightmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.lightmap_samp),
                },
            ],
        });
    }

    /// Replace the bound albedo (+ optional brightmap; `None` = black / no effect).
    pub fn set_albedo(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        samp: &wgpu::Sampler,
    ) {
        self.set_albedo_bright(device, view, samp, None);
    }

    /// Bind albedo and brightmap views for the next draw.
    pub fn set_albedo_bright(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        samp: &wgpu::Sampler,
        bright: Option<&wgpu::TextureView>,
    ) {
        self.set_albedo_bright_normal(device, view, samp, bright, None);
    }

    /// Bind albedo, brightmap, and optional normal map (GLDEFS PBR bootstrap).
    pub fn set_albedo_bright_normal(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        samp: &wgpu::Sampler,
        bright: Option<&wgpu::TextureView>,
        normal: Option<&wgpu::TextureView>,
    ) {
        self.set_albedo_maps(device, view, samp, bright, normal, None);
    }

    /// Bind albedo + bright + normal + optional glTF metal-rough map.
    pub fn set_albedo_maps(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        samp: &wgpu::Sampler,
        bright: Option<&wgpu::TextureView>,
        normal: Option<&wgpu::TextureView>,
        metal_rough: Option<&wgpu::TextureView>,
    ) {
        let bright_view = bright.unwrap_or(&self._black_bright_view);
        let normal_view = normal.unwrap_or(&self._flat_normal_view);
        let mr_view = metal_rough.unwrap_or(&self._default_mr_view);
        self.mat_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mat-bg"),
            layout: &self.mat_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(samp),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bright_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(mr_view),
                },
            ],
        });
    }

    /// Upload scene geometry and view uniforms.
    pub fn prepare(&mut self, gpu: &GpuContext, camera: &Camera, scene: &SceneLists) {
        let uniforms = ViewUniforms {
            view_proj: mat4_cols(camera.view_proj()),
            eye: [camera.eye.x, camera.eye.y, camera.eye.z, self.time],
            light_params: [
                f32::from(self.light_mode as u8),
                self.glob_vis,
                self.fog_density,
                f32::from(self.fuzz_type as u8),
            ],
            clip_plane: self.clip_plane,
            lightmap_params: [self.lightmap_strength, 0.0, 0.0, 0.0],
        };
        gpu.queue
            .write_buffer(&self.view_buf, 0, bytes_of(&uniforms));
        let mut assigned = ClusterAssignment::build(camera.eye, &scene.dyn_lights);
        let atlas = bake_1d_shadowmaps(&scene.dyn_lights, &scene.shadow_occluders);
        if assigned.uniforms.count[0] > 0 && !scene.shadow_occluders.is_empty() {
            assigned.uniforms.count[2] = 1;
        }
        gpu.queue
            .write_buffer(&self.light_buf, 0, bytes_of(&assigned.uniforms));
        let (cw, ch) = cluster_tex_dims();
        let rgba = pack_cluster_rgba(&assigned.masks);
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.cluster_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(cw * 4),
                rows_per_image: Some(ch),
            },
            wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
        );
        let (sw, sh) = shadowmap_tex_dims();
        let shadow_bytes = pack_shadowmap_r32(&atlas);
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.shadow_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &shadow_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(sw * 4),
                rows_per_image: Some(sh),
            },
            wgpu::Extent3d {
                width: sw,
                height: sh,
                depth_or_array_layers: 1,
            },
        );
        #[allow(clippy::cast_possible_truncation)]
        {
            self.translucent_wall_base = scene.opaque_walls.len() as u32;
        }
        let mut wall_verts: Vec<WallVertex> =
            Vec::with_capacity(scene.opaque_walls.len() + scene.translucent_walls.len());
        wall_verts.extend_from_slice(&scene.opaque_walls);
        wall_verts.extend_from_slice(&scene.translucent_walls);
        self.wall_buf = ensure_vb(
            &gpu.device,
            &gpu.queue,
            &self.wall_buf,
            &mut self.wall_cap,
            "walls",
            &wall_verts,
        );
        #[allow(clippy::cast_possible_truncation)]
        {
            self.translucent_flat_base = scene.flats.len() as u32;
        }
        let mut flat_verts: Vec<FlatVertex> =
            Vec::with_capacity(scene.flats.len() + scene.translucent_flats.len());
        flat_verts.extend_from_slice(&scene.flats);
        flat_verts.extend_from_slice(&scene.translucent_flats);
        self.flat_buf = ensure_vb(
            &gpu.device,
            &gpu.queue,
            &self.flat_buf,
            &mut self.flat_cap,
            "flats",
            &flat_verts,
        );
        self.sky_buf = ensure_vb(
            &gpu.device,
            &gpu.queue,
            &self.sky_buf,
            &mut self.sky_cap,
            "sky",
            &scene.sky_parts,
        );
        self.sprite_buf = ensure_vb(
            &gpu.device,
            &gpu.queue,
            &self.sprite_buf,
            &mut self.sprite_cap,
            "sprites",
            &scene.sprites,
        );
    }

    /// Write `stencil_ref` into the stencil buffer for `mask` wall ranges.
    ///
    /// Requires a dual-MRT pass (scene colour + stencil aux). Metal needs
    /// colour-target-0 writes for stencil Replace; Equal overwrites those pixels.
    pub fn draw_stencil_mask(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        scene: &SceneLists,
        mask: &[MaterialRange],
        stencil_ref: u32,
    ) {
        if mask.is_empty() || scene.opaque_walls.is_empty() {
            return;
        }
        pass.set_pipeline(&self.stencil_mask_pipeline);
        pass.set_stencil_reference(stencil_ref);
        pass.set_bind_group(0, &self.view_bind, &[]);
        pass.set_bind_group(1, &self.mat_bind, &[]);
        pass.set_vertex_buffer(0, self.wall_buf.slice(..));
        for r in mask {
            if r.count == 0 {
                continue;
            }
            pass.draw(r.start..r.start + r.count, 0..1);
        }
    }

    /// Draw opaque walls/flats only where stencil equals `stencil_ref`.
    pub fn draw_stencil_equal(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        scene: &SceneLists,
        stencil_ref: u32,
    ) {
        pass.set_pipeline(&self.stencil_equal_pipeline);
        pass.set_stencil_reference(stencil_ref);
        pass.set_bind_group(0, &self.view_bind, &[]);
        pass.set_bind_group(1, &self.mat_bind, &[]);
        if !scene.opaque_walls.is_empty() {
            pass.set_vertex_buffer(0, self.wall_buf.slice(..));
            pass.draw(0..scene.opaque_walls.len() as u32, 0..1);
        }
        if !scene.flats.is_empty() {
            pass.set_vertex_buffer(0, self.flat_buf.slice(..));
            pass.draw(0..scene.flats.len() as u32, 0..1);
        }
    }

    /// Record draws into an existing render pass (color + depth already set).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, scene: &SceneLists) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.view_bind, &[]);
        pass.set_bind_group(1, &self.mat_bind, &[]);
        if !scene.opaque_walls.is_empty() {
            pass.set_vertex_buffer(0, self.wall_buf.slice(..));
            #[allow(clippy::cast_possible_truncation)]
            let n = scene.opaque_walls.len() as u32;
            pass.draw(0..n, 0..1);
        }
        if !scene.flats.is_empty() {
            pass.set_pipeline(&self.flats_pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_bind_group(1, &self.mat_bind, &[]);
            pass.set_vertex_buffer(0, self.flat_buf.slice(..));
            #[allow(clippy::cast_possible_truncation)]
            let n = scene.flats.len() as u32;
            pass.draw(0..n, 0..1);
            pass.set_pipeline(&self.pipeline);
        }
        if !scene.sky_parts.is_empty() {
            pass.set_vertex_buffer(0, self.sky_buf.slice(..));
            #[allow(clippy::cast_possible_truncation)]
            let n = scene.sky_parts.len() as u32;
            pass.draw(0..n, 0..1);
        }
        if !scene.sprites.is_empty() {
            pass.set_pipeline(&self.opaque_sprite_pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_bind_group(1, &self.mat_bind, &[]);
            pass.set_vertex_buffer(0, self.sprite_buf.slice(..));
            #[allow(clippy::cast_possible_truncation)]
            let n = scene.sprites.len() as u32;
            pass.draw(0..n, 0..1);
        }
        if !scene.translucent_walls.is_empty() {
            pass.set_pipeline(&self.sprite_pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_bind_group(1, &self.mat_bind, &[]);
            pass.set_vertex_buffer(0, self.wall_buf.slice(..));
            let base = self.translucent_wall_base;
            #[allow(clippy::cast_possible_truncation)]
            let n = scene.translucent_walls.len() as u32;
            pass.draw(base..base + n, 0..1);
        }
        if !scene.translucent_flats.is_empty() {
            pass.set_pipeline(&self.sprite_pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_bind_group(1, &self.mat_bind, &[]);
            pass.set_vertex_buffer(0, self.flat_buf.slice(..));
            let base = self.translucent_flat_base;
            #[allow(clippy::cast_possible_truncation)]
            let n = scene.translucent_flats.len() as u32;
            pass.draw(base..base + n, 0..1);
        }
    }

    /// Draw walls/flats with per-range materials from `catalog` (uploads on demand).
    ///
    /// `__MIRROR*` / `__PORTAL*` wall ranges are stencil-masked then Equal-drawn so
    /// canvas textures only land on the portal aperture (Metal MRT main pass).
    pub fn draw_materials(
        &mut self,
        gpu: &GpuContext,
        pass: &mut wgpu::RenderPass<'_>,
        scene: &SceneLists,
        catalog: &mut MaterialCatalog,
        vfs: &Vfs,
    ) {
        catalog.ensure_sampler(&gpu.device);
        let (portal_walls, solid_walls): (Vec<_>, Vec<_>) = scene
            .wall_ranges
            .iter()
            .cloned()
            .partition(|r| r.name.starts_with("__MIRROR") || r.name.starts_with("__PORTAL"));
        // Floors first, then walls: step risers must win z-order against any
        // convex-hull floor ears that poke past a linedef (transparent steps).
        pass.set_bind_group(0, &self.view_bind, &[]);
        if !scene.flats.is_empty() {
            pass.set_pipeline(&self.flats_pipeline);
            pass.set_vertex_buffer(0, self.flat_buf.slice(..));
            self.draw_ranges(gpu, pass, &scene.flat_ranges, catalog, vfs);
        }
        if !scene.opaque_walls.is_empty() {
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.wall_buf.slice(..));
            self.draw_ranges(gpu, pass, &solid_walls, catalog, vfs);
        }
        if !scene.sky_parts.is_empty() {
            pass.set_vertex_buffer(0, self.sky_buf.slice(..));
            if !scene.sky_ranges.is_empty() {
                self.draw_ranges(gpu, pass, &scene.sky_ranges, catalog, vfs);
            } else if scene.sky_texture.is_empty() {
                pass.set_bind_group(1, &self.mat_bind, &[]);
                #[allow(clippy::cast_possible_truncation)]
                let n = scene.sky_parts.len() as u32;
                pass.draw(0..n, 0..1);
            } else {
                let range = [crate::scene::MaterialRange {
                    name: scene.sky_texture.clone(),
                    start: 0,
                    count: scene.sky_parts.len() as u32,
                }];
                self.draw_ranges(gpu, pass, &range, catalog, vfs);
            }
        }
        if !scene.sprites.is_empty() {
            pass.set_pipeline(&self.opaque_sprite_pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_vertex_buffer(0, self.sprite_buf.slice(..));
            if scene.sprite_ranges.is_empty() {
                pass.set_bind_group(1, &self.mat_bind, &[]);
                #[allow(clippy::cast_possible_truncation)]
                let n = scene.sprites.len() as u32;
                pass.draw(0..n, 0..1);
            } else {
                self.draw_ranges(gpu, pass, &scene.sprite_ranges, catalog, vfs);
            }
        }
        if !scene.translucent_walls.is_empty() {
            pass.set_pipeline(&self.sprite_pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_vertex_buffer(0, self.wall_buf.slice(..));
            let base = self.translucent_wall_base;
            let ranges: Vec<MaterialRange> = scene
                .translucent_wall_ranges
                .iter()
                .map(|r| MaterialRange {
                    name: r.name.clone(),
                    start: r.start + base,
                    count: r.count,
                })
                .collect();
            self.draw_ranges(gpu, pass, &ranges, catalog, vfs);
        }
        if !scene.translucent_flats.is_empty() {
            pass.set_pipeline(&self.sprite_pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_vertex_buffer(0, self.flat_buf.slice(..));
            let base = self.translucent_flat_base;
            let ranges: Vec<MaterialRange> = scene
                .translucent_flat_ranges
                .iter()
                .map(|r| MaterialRange {
                    name: r.name.clone(),
                    start: r.start + base,
                    count: r.count,
                })
                .collect();
            self.draw_ranges(gpu, pass, &ranges, catalog, vfs);
        }
        // Portal / mirror canvases: mark stencil, then Equal-draw the textured quads.
        if !portal_walls.is_empty() && !scene.opaque_walls.is_empty() {
            for (i, r) in portal_walls.iter().enumerate() {
                let ref_v = 1 + (i as u32 % 200);
                self.draw_stencil_mask(pass, scene, std::slice::from_ref(r), ref_v);
                pass.set_pipeline(&self.stencil_equal_pipeline);
                pass.set_stencil_reference(ref_v);
                pass.set_bind_group(0, &self.view_bind, &[]);
                pass.set_vertex_buffer(0, self.wall_buf.slice(..));
                self.draw_ranges(gpu, pass, std::slice::from_ref(r), catalog, vfs);
            }
        }
    }

    fn draw_ranges(
        &mut self,
        gpu: &GpuContext,
        pass: &mut wgpu::RenderPass<'_>,
        ranges: &[MaterialRange],
        catalog: &mut MaterialCatalog,
        vfs: &Vfs,
    ) {
        if ranges.is_empty() {
            pass.set_bind_group(1, &self.mat_bind, &[]);
            return;
        }
        for r in ranges {
            // Missing lumps must not reuse the previous sprite's bind — that made
            // bullet impact decals draw as whatever actor/puff was bound last.
            let bm_name = catalog.brightmap_name(&r.name).map(str::to_owned);
            let Some(id) = catalog.get_or_upload(&gpu.device, &gpu.queue, vfs, &r.name) else {
                // Still draw with the pass default albedo (solid white) so missing
                // monster/item sprites stay visible instead of silently vanishing.
                pass.set_bind_group(1, &self.mat_bind, &[]);
                pass.draw(r.start..r.start + r.count, 0..1);
                continue;
            };
            let bright_id = bm_name
                .as_deref()
                .and_then(|bm| catalog.get_or_upload(&gpu.device, &gpu.queue, vfs, bm));
            let Some(samp) = catalog.sampler.as_ref() else {
                pass.set_bind_group(1, &self.mat_bind, &[]);
                pass.draw(r.start..r.start + r.count, 0..1);
                continue;
            };
            let Some(albedo_gpu) = catalog.textures.get(id).and_then(|e| e.gpu.as_ref()) else {
                pass.set_bind_group(1, &self.mat_bind, &[]);
                pass.draw(r.start..r.start + r.count, 0..1);
                continue;
            };
            let albedo_view = albedo_gpu.create_view(&wgpu::TextureViewDescriptor::default());
            let bright_owned = bright_id.and_then(|bid| {
                catalog
                    .textures
                    .get(bid)
                    .and_then(|e| e.gpu.as_ref())
                    .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
            });
            self.set_albedo_bright(&gpu.device, &albedo_view, samp, bright_owned.as_ref());
            pass.set_bind_group(1, &self.mat_bind, &[]);
            pass.draw(r.start..r.start + r.count, 0..1);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn make_mrt_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    fs_entry: &str,
    label: &str,
    depth_write: bool,
    cull_mode: Option<wgpu::Face>,
    color0_writes: wgpu::ColorWrites,
    color1_writes: wgpu::ColorWrites,
    stencil: wgpu::StencilState,
    depth_compare: wgpu::CompareFunction,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<WallVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x3,
                    1 => Float32x2,
                    2 => Float32,
                    3 => Float32x2,
                    4 => Float32x2,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[
                Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: color0_writes,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: color1_writes,
                }),
            ],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DEPTH_STENCIL_FORMAT,
            depth_write_enabled: depth_write,
            depth_compare,
            stencil,
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // thin wrapper kept for call sites that want zero bias
fn make_unlit_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    label: &str,
    blend: wgpu::BlendState,
    depth_write: bool,
    cull_mode: Option<wgpu::Face>,
    write_mask: wgpu::ColorWrites,
    stencil: wgpu::StencilState,
    depth_compare: wgpu::CompareFunction,
    ds_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    make_unlit_pipeline_bias(
        device,
        shader,
        layout,
        target_format,
        label,
        blend,
        depth_write,
        cull_mode,
        write_mask,
        stencil,
        depth_compare,
        ds_format,
        wgpu::DepthBiasState::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn make_unlit_pipeline_bias(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    label: &str,
    blend: wgpu::BlendState,
    depth_write: bool,
    cull_mode: Option<wgpu::Face>,
    write_mask: wgpu::ColorWrites,
    stencil: wgpu::StencilState,
    depth_compare: wgpu::CompareFunction,
    ds_format: wgpu::TextureFormat,
    bias: wgpu::DepthBiasState,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<WallVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x3,
                    1 => Float32x2,
                    2 => Float32,
                    3 => Float32x2,
                    4 => Float32x2,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            // MRT: colour + dummy R8 so the main pass matches stencil pipelines.
            entry_point: Some("fs_main_mrt"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[
                Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(blend),
                    write_mask,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::empty(),
                }),
            ],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: ds_format,
            depth_write_enabled: depth_write,
            depth_compare,
            stencil,
            bias,
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn empty_vb(device: &wgpu::Device, label: &str, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.max(64),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn ensure_vb<T: Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    old: &wgpu::Buffer,
    cap: &mut u64,
    label: &str,
    data: &[T],
) -> wgpu::Buffer {
    if data.is_empty() {
        return old.clone();
    }
    let need = std::mem::size_of_val(data) as u64;
    let buf = if need > *cap {
        *cap = need.next_power_of_two();
        empty_vb(device, label, *cap)
    } else {
        old.clone()
    };
    queue.write_buffer(&buf, 0, cast_slice(data));
    buf
}

fn mat4_cols(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}
