//! wgpu renderer: 3D scene plus 2D canvas backend.
//!
//! # Safety
//!
//! `unsafe` is allowed in this crate only for `bytemuck` buffer casts
//! (AGENTS.md). Each block needs a `// SAFETY:` comment.

#![allow(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

/// Depth+stencil format for all 3D passes (mirrors / portals need stencil).
pub const DEPTH_STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

/// Max nested `__PORTAL*` canvas refreshes (eye view = depth 0).
pub const MAX_PORTAL_RECURSION: u32 = 2;

pub mod bloom;
pub mod bsp_walk;
pub mod camera;
pub mod canvas;
pub mod canvas_gpu;
pub mod capture;
pub mod clipper;
pub mod decals;
pub mod device;
pub mod faces;
pub mod fonts;
pub mod frame;
pub mod fuzz;
pub mod glsl_translate;
pub mod hardware_shader;
pub mod hq_resize;
pub mod hw_post;
pub mod iqm_anim;
pub mod kvx;
pub mod lightmap;
pub mod lightmode;
pub mod lights;
pub mod materials;
pub mod mesh;
pub mod mod_zones;
pub mod model_catalog;
pub mod models;
pub mod offscreen;
pub mod particle_pass;
pub mod particles;
pub mod pass;
pub mod pbr;
pub mod player_colors;
pub mod portal_view;
pub mod post;
pub mod render_style;
pub mod scene;
pub mod settings;
pub mod skinning;
pub mod sky;
pub mod sprites;
pub mod ssao;
pub mod texanim;
pub mod texture;
pub mod ue1;
pub mod voxel_draw;

pub use bloom::{BLOOM_LEVELS, BloomPass};
pub use bsp_walk::{VisibleSet, walk_bsp};
pub use camera::Camera;
pub use canvas::{Canvas, CanvasCmd};
pub use canvas_gpu::CanvasGpu;
pub use capture::{
    CaptureError, encode_gif, encode_png, read_rgba, read_texture_rgba, scale_rgba_nn,
    swizzle_bgra_to_rgba,
};
pub use clipper::{Clipper, pseudo_angle};
pub use decals::{
    DecalIngestAnim, DecalInstance, DecalStore, MAX_DECALS, append_decals, emit_decal_quad,
};
pub use device::{DeviceError, GpuContext, GpuTier};
pub use faces::{FaceTextureKey, upsert_face_rgba};
pub use fonts::{Fon1Face, TextDraw};
pub use frame::{FrameTargets, PostChain, PostChainInputs};
pub use fuzz::{FuzzType, fuzz_modulate};
pub use glsl_translate::{GlslError, glsl_to_wgsl};
pub use hardware_shader::{
    CompiledHardwareShader, HARDWARE_POST_HOST_WGSL, HardwareShaderCatalog, HardwareShaderError,
    compile_material_glsl, compose_hardware_post_wgsl, create_hardware_post_pipeline,
};
pub use hw_post::{
    CompiledPostShader, HardwarePostRunner, POST_VS_WGSL, PostTimes, PostUniformField,
    compile_post_glsl, post_prolog, post_uniform_layout,
};
pub use iqm_anim::{
    BoneComponent, BoneKind, BoneOverride, IqmAnim, IqmAnimData, ModelAnim, Trs, absolute_matrices,
    anim_frames, apply_overrides, blend_poses, frame_pose, interpolate_bone, interpolate_quat,
    parse_iqm_anim_data,
};
pub use kvx::{
    KvxHeader, KvxModel, KvxVoxel, kvx_to_mesh, kvx_to_mesh_greedy, kvx_to_mesh_naive, parse_kvx,
};
pub use lightmap::{
    LIGHTMAP_MAGIC, LightmapAtlas, LightmapBindings, LightmapError, LightmapHeader, LightmapPass,
    LightmapSurface, lightmap_atlas_texel, parse_lightmap_header, parse_lightmap_lump, surface_uv,
    world_xy_to_surface_uv,
};
pub use lightmode::LightMode;
pub use lights::{
    CLUSTER_COUNT, CLUSTER_X, CLUSTER_Y, CLUSTER_Z, ClusterAssignment, GpuLight, LightUniforms,
    MAX_GPU_LIGHTS, SHADOWMAP_RES, ShadowOccluder, bake_1d_shadowmaps, cluster_tex_dims,
    pack_cluster_rgba, pack_shadowmap_r32, shadowmap_tex_dims,
};
pub use materials::{MaterialCatalog, MaterialMaps, lump_preview_rgba};
pub use mesh::{FlatVertex, ViewUniforms, WallVertex};
pub use mod_zones::{
    ModZonePass, ZoneDraw, ZoneMeshes, ZoneVertex, build_zone_meshes, flags as mod_zone_flags,
};
pub use model_catalog::{
    AnimRequest, FramedModel, ModelActorPose, ModelCatalog, actor_has_model, actor_pose,
    actor_pose_lerp, append_actor_models, join_model_path, model_matrix, posed_mesh, resolve_anim,
};
pub use models::{
    IqmJoint, IqmModel, Md2Model, Md3Model, ModelError, ModelMesh, ModelVertex, append_model,
    parse_iqm, parse_iqm_full, parse_md2, parse_md3, parse_model_bytes, parse_obj,
};
pub use offscreen::{
    OffscreenTarget, render_offscreen, render_offscreen_stenciled, render_offscreen_with_post,
};
pub use particle_pass::ParticleInstancedPass;
pub use particles::{
    MAX_PARTICLES, PARTICLE_TEX, PARTICLE_TEX_ADD, ParticleGpuInstance, ParticleInstance,
    ParticleStore, ParticleStyle, append_particles, append_particles_animated, append_particles_ex,
    pack_particle_instances, particle_tint_tex_name, soft_disc_rgba,
};
pub use pass::UnlitPass;
pub use pbr::{MetalRough, fresnel_schlick, ggx_d, pbr_specular, specular_f0};
pub use portal_view::{PortalCamera, PortalRenderer, portal_camera};
pub use post::{
    PALETTE_LUT_SIZE, PostPass, PostUniforms, TonemapMode, build_palette_lut, quantize_playpal,
    tonemap_rgb, tonemap_rgb_pal,
};
pub use render_style::RenderStyle;
pub use scene::{
    DynLightInstance, LinkedPortalSurface, MaterialRange, MirrorSurface, PORTAL_STYLE,
    PORTAL_STYLE_FLIP_X, PlaneReflectSurface, PortalView, PortalViewKind, SceneLists,
    append_sprites, apply_dyn_lights, apply_glow_fullbright, build_scene, build_scene_from_visible,
    build_scene_from_visible_ex, build_scene_with_sky, build_scene_with_sky_ex,
    build_scene_with_sky_ex_sized, clip_scene_to_mirror_line, collect_shadow_occluders,
    horizon_scene, is_sky_flat, mirror_clip_plane, mirror_tex_name, plane_reflect_tex_name,
    portal_tex_name, portal_view_strips_portals, request_portal_view,
};
pub use settings::{AaMode, CrtPreset, GoreLevel, RenderSettings, SHADER_CVAR_NAMES};
pub use skinning::{
    BoneUniforms, MAX_BONES, SkinnedPass, SkinnedVertex, iqm_to_skinned_vertices, skin_matrices,
};
pub use sky::{CubemapFaces, cubemap_sky, cylinder_sky, scroll_sky_u};
pub use sprites::{
    SpriteFx, SpriteInstance, SpriteOrient, emit_sprite_quad, mirror_rotation,
    sprite_lump_candidates, sprite_lump_candidates_rot, sprite_offset_delta, sprite_rotation_digit,
};
pub use ssao::{
    SsaoPass, SsaoUniforms, depth_only_view, ssao_bilateral_weight, ssao_edge_occlusion,
};
pub use texanim::TextureAnimator;
pub use texture::{TextureEntry, TextureFormatKind, TextureManager};
pub use ue1::{Ue1Model, Ue1Tri, decompress_ue1_vertex, parse_ue1};
pub use voxel_draw::{
    MAX_VOXEL_WALL_VERTS_PER_FRAME, VoxelActorPose, VoxelCatalog, voxel_spin_degrees,
    voxel_view_angles,
};
