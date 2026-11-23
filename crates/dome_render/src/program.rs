mod capture;
mod compute;
mod environment;
mod forward;
mod hzb;
mod post;

pub(crate) use capture::capture_conversion;
pub(crate) use compute::{clustering, culling};
pub(crate) use environment::{
    atmosphere_scattering_program, atmosphere_sky_program, atmosphere_transmittance_program,
    integrating, irradiance_reduction_program, prefiltering,
};
pub(crate) use forward::{
    debug_lines, depth_prepass, instancing, motion_vectors, picking, shadowing,
};
pub(crate) use hzb::{hzb_base, hzb_reduce};
pub(crate) use post::{
    bloom_down, bloom_extract, bloom_up, sky_program, temporal_resolve, tonemap,
    transparency_resolve,
};

use crate::abi::{
    AtlasRecord, AtmosphereRecord, BakeLevelRecord, BloomRecord, BloomUpRecord, CascadeRecord,
    DebugLineRecord, DrawRecord, HzbLevelRecord, HzbStepRecord, IRRADIANCE_PARTIALS,
    InstanceRecord, IrradianceRecord, IrradianceSetupRecord, JointRecord, LIGHT_CAPACITY,
    LIGHT_SLICES, LIGHT_STRIDE, LIGHT_TILE, LightRecord, MaterialRecord, MotionRecord, ProbeRecord,
    SHADOW_CASCADES, SkinVertexRecord, SkyRecord, TemporalRecord, TonemapRecord,
    TransparentSetupRecord, VERTEX_FLOATS, ViewRecord, WORKGROUP,
};
use dome_shader::AtomicU32;
use dome_shader::{ComputeProgram, Fvec2, Fvec4, GraphicsProgram, Read, ReadWrite, Uvec3};

pub(crate) const CULL_VIEW: u32 = 0;
pub(crate) const CULL_INSTANCES: u32 = 1;
pub(crate) const CULL_DRAWN: u32 = 2;
pub(crate) const CULL_ARGUMENTS: u32 = 3;
pub(crate) const CULL_LEVELS: u32 = 4;
pub(crate) const CULL_DEPTHS: u32 = 5;

pub(crate) const DEPTH_VIEW: u32 = 0;
pub(crate) const DEPTH_PALETTE: u32 = 1;
pub(crate) const DEPTH_SKINS: u32 = 2;
pub(crate) const DEPTH_DRAWN: u32 = 3;
pub(crate) const DEPTH_VERTICES: u32 = 4;
pub(crate) const DEPTH_MATERIALS: u32 = 5;

pub(crate) const HZB_STEP_BINDING: u32 = 0;
pub(crate) const HZB_DEPTHS_BINDING: u32 = 1;
pub(crate) const HZB_SOURCE_BINDING: u32 = 0;
pub(crate) const HZB_SAMPLER_BINDING: u32 = 1;

pub(crate) const DRAW_LIGHT: u32 = 0;
pub(crate) const DRAW_CASCADES: u32 = 1;
pub(crate) const DRAW_DRAWN: u32 = 2;
pub(crate) const DRAW_MATERIALS: u32 = 3;
pub(crate) const DRAW_VIEW: u32 = 4;
pub(crate) const DRAW_PALETTE: u32 = 5;
pub(crate) const DRAW_SKINS: u32 = 6;
pub(crate) const DRAW_FRAGMENTS: u32 = 7;
pub(crate) const DRAW_COLORS: u32 = 8;
pub(crate) const DRAW_DROPPED: u32 = 9;
pub(crate) const DRAW_SETTINGS: u32 = 10;
pub(crate) const DRAW_PROBES: u32 = 11;
pub(crate) const DRAW_VERTICES: u32 = 12;
pub(crate) const DRAW_LIGHTS: u32 = 13;

pub(crate) const PICK_VIEW: u32 = 0;
pub(crate) const PICK_PALETTE: u32 = 1;
pub(crate) const PICK_SKINS: u32 = 2;
pub(crate) const PICK_DRAWN: u32 = 3;
pub(crate) const PICK_VERTICES: u32 = 4;

pub(crate) const CLUSTER_VIEW: u32 = 0;
pub(crate) const CLUSTER_POINTS: u32 = 1;
pub(crate) const CLUSTER_GRID: u32 = 2;

pub(crate) const SHADOW_PALETTE: u32 = 0;
pub(crate) const SHADOW_SKINS: u32 = 1;
pub(crate) const SHADOW_DRAWN: u32 = 2;
pub(crate) const SHADOW_VERTICES: u32 = 3;
pub(crate) const SHADOW_MATERIALS: u32 = 4;

pub(crate) const SHADE_TEXTURE: u32 = 0;
pub(crate) const SHADE_SAMPLER: u32 = 1;
pub(crate) const SHADE_RADIANCE: u32 = 2;
pub(crate) const SHADE_RADIANCE_SAMPLER: u32 = 3;
pub(crate) const SHADE_BRDF: u32 = 4;
pub(crate) const SHADE_BRDF_SAMPLER: u32 = 5;
pub(crate) const POINT_SHADOW_TEXTURE: u32 = 6;
pub(crate) const POINT_SHADOW_SAMPLER: u32 = 7;
pub(crate) const PROBE_TEXTURES: u32 = 8;
pub(crate) const PROBE_SAMPLERS: u32 = 9;

pub(crate) const SKY_VIEW: u32 = 0;
pub(crate) const SKY_RADIANCE: u32 = 0;
pub(crate) const SKY_SAMPLER: u32 = 1;

pub(crate) const PREFILTER_LEVEL: u32 = 0;
pub(crate) const PREFILTER_PANORAMA: u32 = 0;
pub(crate) const PREFILTER_SAMPLER: u32 = 1;
pub(crate) const CAPTURE_SETUP: u32 = 0;
pub(crate) const CAPTURE_ATLAS: u32 = 0;
pub(crate) const CAPTURE_SAMPLER: u32 = 1;
pub(crate) const INTEGRATION_SETUP: u32 = 0;
pub(crate) const INTEGRATION_PARTIALS: u32 = 1;
pub(crate) const INTEGRATION_PANORAMA: u32 = 0;
pub(crate) const INTEGRATION_SAMPLER: u32 = 1;
pub(crate) const REDUCTION_PARTIALS: u32 = 0;
pub(crate) const REDUCTION_TOTAL: u32 = 1;

pub(crate) const ATMOSPHERE_SETUP: u32 = 0;
pub(crate) const ATMOSPHERE_TRANSMITTANCE: u32 = 0;
pub(crate) const ATMOSPHERE_SCATTERING: u32 = 1;
pub(crate) const ATMOSPHERE_LUT_SAMPLER: u32 = 1;
pub(crate) const ATMOSPHERE_SKY_SAMPLER: u32 = 2;

pub(crate) const BLOOM_SETTINGS: u32 = 0;
pub(crate) const EXTRACT_SOURCE: u32 = 0;
pub(crate) const EXTRACT_FILTER: u32 = 1;
pub(crate) const DOWN_SOURCE: u32 = 0;
pub(crate) const DOWN_FILTER: u32 = 1;
pub(crate) const UP_LOW: u32 = 0;
pub(crate) const UP_HIGH: u32 = 1;
pub(crate) const UP_FILTER: u32 = 2;
pub(crate) const TONEMAP_SETTINGS: u32 = 0;
pub(crate) const TONEMAP_LIT: u32 = 0;
pub(crate) const TONEMAP_BLOOM: u32 = 1;
pub(crate) const TONEMAP_FILTER: u32 = 2;

pub(crate) const SHADOW_SHADOW: u32 = 0;
pub(crate) const SHADOW_SAMPLER: u32 = 1;

pub(crate) const RESOLVE_SETTINGS: u32 = 0;
pub(crate) const RESOLVE_KEYS: u32 = 1;
pub(crate) const RESOLVE_COLORS: u32 = 2;
pub(crate) const RESOLVE_SOURCE: u32 = 0;
pub(crate) const RESOLVE_FILTER: u32 = 1;

pub(crate) const LINE_VIEW: u32 = 0;
pub(crate) const LINE_SEGMENTS: u32 = 1;

pub(crate) const MOTION_VIEW: u32 = 0;
pub(crate) const MOTION_DRAWN: u32 = 1;
pub(crate) const MOTION_PALETTE: u32 = 2;
pub(crate) const MOTION_SKINS: u32 = 3;
pub(crate) const MOTION_MATERIALS: u32 = 4;
pub(crate) const MOTION_VERTICES: u32 = 5;
pub(crate) const MOTION_TEXTURE: u32 = 0;
pub(crate) const MOTION_SAMPLER: u32 = 1;

pub(crate) const TEMPORAL_FRAME: u32 = 0;
pub(crate) const TEMPORAL_SOURCE: u32 = 0;
pub(crate) const TEMPORAL_HISTORY: u32 = 1;
pub(crate) const TEMPORAL_FILTER: u32 = 2;
pub(crate) const TEMPORAL_MOTION: u32 = 0;
pub(crate) const TEMPORAL_PREVIOUS: u32 = 1;
pub(crate) const TEMPORAL_NEAREST: u32 = 2;

const _: () = assert!(
    WORKGROUP == 64,
    "the culling kernel runs the workgroup its attribute declares",
);
const _: () = assert!(
    VERTEX_FLOATS == 8,
    "the instancing stage reads the vertex layout of a render vertex",
);
const _: () = assert!(
    SHADOW_CASCADES == 4,
    "the shadow receiver selects one of four cascades",
);
const _: () = assert!(
    crate::abi::HZB_SCALE == 4,
    "the base reduction samples the block the scale names",
);
const _: () = assert!(
    LIGHT_STRIDE == LIGHT_CAPACITY + 1,
    "a light cluster holds its count beside every light it names",
);
const _: () = assert!(
    LIGHT_STRIDE == 33 && LIGHT_TILE == 64 && LIGHT_SLICES == 24 && LIGHT_CAPACITY == 32,
    "the clustered lighting program walks the tile, slice and light counts its records declare",
);
const _: () = assert!(
    LIGHT_TILE > 0 && LIGHT_SLICES > 0,
    "a light cluster covers a tile of at least one pixel and one depth slice",
);
const _: () = assert!(
    crate::point_shadow::POINT_SHADOW_SIDE == 256,
    "the point shadow shader walks the face texels its atlas holds",
);
const _: () = assert!(
    IRRADIANCE_PARTIALS == 1024 && crate::abi::IRRADIANCE_WORDS == 36,
    "the irradiance reduction walks the partials and record words its shader counts",
);
const _: () = assert!(
    crate::environment::SPECULAR_SAMPLES == 128,
    "the prefilter samples the half vectors its shader counts",
);
