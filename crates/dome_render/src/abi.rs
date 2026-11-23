use dome_shader::record;
use dome_shader::{Fvec4, Uvec4};
use std::mem::size_of;

pub const WORKGROUP: u32 = 64;
pub const ARGUMENT_WORDS: u32 = 4;
pub const VERTEX_FLOATS: u32 = 8;
pub const SHADOW_CASCADES: u32 = 4;
pub const SHADOW_ATLAS_COLUMNS: u32 = 2;
pub const SHADOW_SIDE: u32 = 1024;
pub const SHADOW_ATLAS: u32 = SHADOW_SIDE * SHADOW_ATLAS_COLUMNS;
pub const HZB_SCALE: u32 = 4;
pub const LIGHT_TILE: u32 = 64;
pub const LIGHT_SLICES: u32 = 24;
pub const LIGHT_CAPACITY: u32 = 32;
pub const LIGHT_STRIDE: u32 = LIGHT_CAPACITY + 1;
pub const NO_HIGHLIGHT: u32 = u32::MAX;
pub const ALPHA_OPAQUE: u32 = 0;
pub const ALPHA_MASK: u32 = 1;
pub const ALPHA_BLEND: u32 = 2;
pub const IRRADIANCE_PARTIALS: u32 = 1024;
pub const IRRADIANCE_WORDS: u32 = 36;
pub const IRRADIANCE_TOTAL_BYTES: u64 = 108;
pub const ATMOSPHERE_RECORDS: u32 = 4;
pub const TRANSPARENT_LAYERS: u32 = 4;

const _: () = assert!(
    size_of::<AtmosphereRecord>() * ATMOSPHERE_RECORDS as usize == 512,
    "an atmosphere writes its four pass records into one setup buffer",
);

const _: () = assert!(
    SHADOW_CASCADES == SHADOW_ATLAS_COLUMNS * SHADOW_ATLAS_COLUMNS,
    "a square atlas holds every cascade",
);
const _: () = assert!(
    IRRADIANCE_WORDS as usize * 4 == size_of::<IrradianceRecord>(),
    "the irradiance reduction strides the record its shader reads",
);
const _: () = assert!(
    IRRADIANCE_TOTAL_BYTES == 27 * 4,
    "an irradiance total holds nine coefficient triples",
);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

record!(ViewRecord, VIEW, "View" {
    clip0: Fvec4 => F32x4,
    clip1: Fvec4 => F32x4,
    clip2: Fvec4 => F32x4,
    clip3: Fvec4 => F32x4,
    plane0: Fvec4 => F32x4,
    plane1: Fvec4 => F32x4,
    plane2: Fvec4 => F32x4,
    plane3: Fvec4 => F32x4,
    plane4: Fvec4 => F32x4,
    plane5: Fvec4 => F32x4,
    instances: u32 => U32,
    hzb_levels: u32 => U32,
    view_width: f32 => F32,
    view_height: f32 => F32,
    eye: Fvec4 => F32x4,
    forward: Fvec4 => F32x4,
    near: f32 => F32,
    far: f32 => F32,
    tan_vertical: f32 => F32,
    aspect: f32 => F32,
    vertical: f32 => F32,
    palette_base: u32 => U32,
    pad0: u32 => U32,
    cluster_x: u32 => U32,
    cluster_y: u32 => U32,
    point_lights: u32 => U32,
    cluster_offset: u32 => U32,
    highlight: u32 => U32,
});

record!(DrawRecord, DRAW, "Draw" {
    clip0: Fvec4 => F32x4,
    clip1: Fvec4 => F32x4,
    clip2: Fvec4 => F32x4,
    clip3: Fvec4 => F32x4,
    model0: Fvec4 => F32x4,
    model1: Fvec4 => F32x4,
    model2: Fvec4 => F32x4,
    model3: Fvec4 => F32x4,
    previous0: Fvec4 => F32x4,
    previous1: Fvec4 => F32x4,
    previous2: Fvec4 => F32x4,
    previous3: Fvec4 => F32x4,
    normal0: Fvec4 => F32x4,
    normal1: Fvec4 => F32x4,
    normal2: Fvec4 => F32x4,
    tint: Fvec4 => F32x4,
    material: u32 => U32,
    pick: u32 => U32,
    palette: u32 => U32,
    joints: u32 => U32,
    pad0: u32 => U32,
    pad1: u32 => U32,
    pad2: u32 => U32,
    pad3: u32 => U32,
});

record!(InstanceRecord, INSTANCE, "Instance" {
    model0: Fvec4 => F32x4,
    model1: Fvec4 => F32x4,
    model2: Fvec4 => F32x4,
    model3: Fvec4 => F32x4,
    previous0: Fvec4 => F32x4,
    previous1: Fvec4 => F32x4,
    previous2: Fvec4 => F32x4,
    previous3: Fvec4 => F32x4,
    normal0: Fvec4 => F32x4,
    normal1: Fvec4 => F32x4,
    normal2: Fvec4 => F32x4,
    tint: Fvec4 => F32x4,
    bounds: Fvec4 => F32x4,
    batch: u32 => U32,
    base: u32 => U32,
    entity: u32 => U32,
    material: u32 => U32,
    palette: u32 => U32,
    joints: u32 => U32,
    hzb: u32 => U32,
    pad0: u32 => U32,
});

record!(SkinVertexRecord, SKIN_VERTEX, "SkinVertex" {
    joints: Uvec4 => U32x4,
    weights: Fvec4 => F32x4,
});

impl SkinVertexRecord {
    pub const fn new(joints: [u32; 4], weights: [f32; 4]) -> Self {
        Self {
            joints: Uvec4 {
                x: joints[0],
                y: joints[1],
                z: joints[2],
                w: joints[3],
            },
            weights: Fvec4 {
                x: weights[0],
                y: weights[1],
                z: weights[2],
                w: weights[3],
            },
        }
    }
}

record!(JointRecord, JOINT, "Joint" {
    col0: Fvec4 => F32x4,
    col1: Fvec4 => F32x4,
    col2: Fvec4 => F32x4,
    col3: Fvec4 => F32x4,
});

record!(DebugLineRecord, DEBUG_LINE, "DebugLine" {
    start: Fvec4 => F32x4,
    end: Fvec4 => F32x4,
    color: Fvec4 => F32x4,
});

record!(MaterialRecord, MATERIAL, "Material" {
    base_color: Fvec4 => F32x4,
    emissive: Fvec4 => F32x4,
    base_color_image: u32 => U32,
    base_color_sampler: u32 => U32,
    metallic_roughness_image: u32 => U32,
    metallic_roughness_sampler: u32 => U32,
    normal_image: u32 => U32,
    normal_sampler: u32 => U32,
    occlusion_image: u32 => U32,
    occlusion_sampler: u32 => U32,
    emissive_image: u32 => U32,
    emissive_sampler: u32 => U32,
    metallic: f32 => F32,
    roughness: f32 => F32,
    normal_scale: f32 => F32,
    occlusion_strength: f32 => F32,
    alpha_mode: u32 => U32,
    alpha_cutoff: f32 => F32,
});

record!(PointLightRecord, POINT_LIGHT, "PointLight" {
    position: Fvec4 => F32x4,
    radiance: Fvec4 => F32x4,
    shadow: Fvec4 => F32x4,
});

record!(CascadeRecord, CASCADE, "Cascade" {
    clip0: Fvec4 => F32x4,
    clip1: Fvec4 => F32x4,
    clip2: Fvec4 => F32x4,
    clip3: Fvec4 => F32x4,
    atlas: Fvec4 => F32x4,
    split: f32 => F32,
    texel: f32 => F32,
    depth_span: f32 => F32,
    atlas_texel: f32 => F32,
    pad0: u32 => U32,
    pad1: u32 => U32,
    pad2: u32 => U32,
    pad3: u32 => U32,
});

record!(MotionRecord, MOTION, "Motion" {
    previous0: Fvec4 => F32x4,
    previous1: Fvec4 => F32x4,
    previous2: Fvec4 => F32x4,
    previous3: Fvec4 => F32x4,
    eye: Fvec4 => F32x4,
    forward: Fvec4 => F32x4,
    vertical: f32 => F32,
    base: u32 => U32,
    previous: u32 => U32,
    pad0: u32 => U32,
});

record!(TemporalRecord, TEMPORAL, "Temporal" {
    inverse0: Fvec4 => F32x4,
    inverse1: Fvec4 => F32x4,
    inverse2: Fvec4 => F32x4,
    inverse3: Fvec4 => F32x4,
    previous0: Fvec4 => F32x4,
    previous1: Fvec4 => F32x4,
    previous2: Fvec4 => F32x4,
    previous3: Fvec4 => F32x4,
    eye: Fvec4 => F32x4,
    forward: Fvec4 => F32x4,
    previous_eye: Fvec4 => F32x4,
    previous_forward: Fvec4 => F32x4,
    near: f32 => F32,
    far: f32 => F32,
    vertical: f32 => F32,
    width: f32 => F32,
    height: f32 => F32,
    blend: f32 => F32,
    tolerance: f32 => F32,
    pad0: u32 => U32,
});

record!(HzbLevelRecord, HZB_LEVEL, "HzbLevel" {
    offset: u32 => U32,
    width: u32 => U32,
    height: u32 => U32,
    stride: u32 => U32,
});

record!(HzbStepRecord, HZB_STEP, "HzbStep" {
    source_width: f32 => F32,
    source_height: f32 => F32,
    target_width: f32 => F32,
    target_height: f32 => F32,
    vertical: f32 => F32,
    source_offset: u32 => U32,
    source_stride: u32 => U32,
    target_offset: u32 => U32,
    target_stride: u32 => U32,
});

record!(LightRecord, LIGHT, "Light" {
    direction: Fvec4 => F32x4,
    radiance: Fvec4 => F32x4,
    irradiance0: Fvec4 => F32x4,
    irradiance1: Fvec4 => F32x4,
    irradiance2: Fvec4 => F32x4,
    irradiance3: Fvec4 => F32x4,
    irradiance4: Fvec4 => F32x4,
    irradiance5: Fvec4 => F32x4,
    irradiance6: Fvec4 => F32x4,
    irradiance7: Fvec4 => F32x4,
    irradiance8: Fvec4 => F32x4,
    environment0: Fvec4 => F32x4,
    environment1: Fvec4 => F32x4,
    environment2: Fvec4 => F32x4,
    rayleigh: Fvec4 => F32x4,
    mie: Fvec4 => F32x4,
    absorption: Fvec4 => F32x4,
    intensity: f32 => F32,
    levels: f32 => F32,
    vertical: f32 => F32,
    probes: u32 => U32,
    pad0: u32 => U32,
    pad1: u32 => U32,
    pad2: u32 => U32,
    pad3: u32 => U32,
});

record!(ProbeRecord, PROBE, "Probe" {
    center: Fvec4 => F32x4,
    inverse0: Fvec4 => F32x4,
    inverse1: Fvec4 => F32x4,
    inverse2: Fvec4 => F32x4,
    extent: Fvec4 => F32x4,
    levels: f32 => F32,
    texture: u32 => U32,
    sampler: u32 => U32,
    pad0: u32 => U32,
    irradiance0: Fvec4 => F32x4,
    irradiance1: Fvec4 => F32x4,
    irradiance2: Fvec4 => F32x4,
    irradiance3: Fvec4 => F32x4,
    irradiance4: Fvec4 => F32x4,
    irradiance5: Fvec4 => F32x4,
    irradiance6: Fvec4 => F32x4,
    irradiance7: Fvec4 => F32x4,
    irradiance8: Fvec4 => F32x4,
});

record!(SkyRecord, SKY, "Sky" {
    inverse0: Fvec4 => F32x4,
    inverse1: Fvec4 => F32x4,
    inverse2: Fvec4 => F32x4,
    inverse3: Fvec4 => F32x4,
    eye: Fvec4 => F32x4,
    environment0: Fvec4 => F32x4,
    environment1: Fvec4 => F32x4,
    environment2: Fvec4 => F32x4,
    intensity: f32 => F32,
    pad0: u32 => U32,
    pad1: u32 => U32,
    pad2: u32 => U32,
});

record!(BakeLevelRecord, BAKE_LEVEL, "BakeLevel" {
    roughness: f32 => F32,
    vertical: f32 => F32,
    pad0: u32 => U32,
    pad1: u32 => U32,
});

record!(IrradianceSetupRecord, IRRADIANCE_SETUP, "IrradianceSetup" {
    width: u32 => U32,
    height: u32 => U32,
    texels: u32 => U32,
    partials: u32 => U32,
});

record!(IrradianceRecord, IRRADIANCE, "Irradiance" {
    coefficient0: Fvec4 => F32x4,
    coefficient1: Fvec4 => F32x4,
    coefficient2: Fvec4 => F32x4,
    coefficient3: Fvec4 => F32x4,
    coefficient4: Fvec4 => F32x4,
    coefficient5: Fvec4 => F32x4,
    coefficient6: Fvec4 => F32x4,
    coefficient7: Fvec4 => F32x4,
    coefficient8: Fvec4 => F32x4,
});

record!(AtlasRecord, ATLAS, "Atlas" {
    face: f32 => F32,
    vertical: f32 => F32,
    texel: f32 => F32,
    pad0: f32 => F32,
});

record!(AtmosphereRecord, ATMOSPHERE, "Atmosphere" {
    rayleigh: Fvec4 => F32x4,
    mie: Fvec4 => F32x4,
    absorption: Fvec4 => F32x4,
    ozone: Fvec4 => F32x4,
    ground: Fvec4 => F32x4,
    sun: Fvec4 => F32x4,
    altitude: f32 => F32,
    anisotropy: f32 => F32,
    vertical: f32 => F32,
    width: f32 => F32,
    height: f32 => F32,
    disk: u32 => U32,
    pad0: u32 => U32,
    pad1: u32 => U32,
});

record!(BloomRecord, BLOOM, "Bloom" {
    source_width: f32 => F32,
    source_height: f32 => F32,
    vertical: f32 => F32,
    threshold: f32 => F32,
    knee: f32 => F32,
});

record!(BloomUpRecord, BLOOM_UP, "BloomUp" {
    source_width: f32 => F32,
    source_height: f32 => F32,
    vertical: f32 => F32,
});

record!(TonemapRecord, TONEMAP, "Tonemap" {
    vertical: f32 => F32,
    intensity: f32 => F32,
    source_width: f32 => F32,
    source_height: f32 => F32,
});

record!(TransparentSetupRecord, TRANSPARENT_SETTINGS, "TransparentSetup" {
    layers: u32 => U32,
    width: u32 => U32,
    height: u32 => U32,
    vertical: f32 => F32,
});
