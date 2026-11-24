use std::mem::size_of;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Fvec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uvec4 {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
}

macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
        pub struct $name {
            $(pub $field: $ty),*
        }
    };
}

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

record!(ViewRecord {
    clip0: Fvec4,
    clip1: Fvec4,
    clip2: Fvec4,
    clip3: Fvec4,
    plane0: Fvec4,
    plane1: Fvec4,
    plane2: Fvec4,
    plane3: Fvec4,
    plane4: Fvec4,
    plane5: Fvec4,
    instances: u32,
    hzb_levels: u32,
    view_width: f32,
    view_height: f32,
    eye: Fvec4,
    forward: Fvec4,
    near: f32,
    far: f32,
    tan_vertical: f32,
    aspect: f32,
    vertical: f32,
    palette_base: u32,
    pad0: u32,
    cluster_x: u32,
    cluster_y: u32,
    point_lights: u32,
    cluster_offset: u32,
    highlight: u32,
});

record!(InstanceRecord {
    model0: Fvec4,
    model1: Fvec4,
    model2: Fvec4,
    model3: Fvec4,
    previous0: Fvec4,
    previous1: Fvec4,
    previous2: Fvec4,
    previous3: Fvec4,
    normal0: Fvec4,
    normal1: Fvec4,
    normal2: Fvec4,
    tint: Fvec4,
    bounds: Fvec4,
    batch: u32,
    base: u32,
    entity: u32,
    material: u32,
    palette: u32,
    joints: u32,
    hzb: u32,
    pad0: u32,
});

record!(SkinVertexRecord {
    joints: Uvec4,
    weights: Fvec4,
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

record!(JointRecord {
    col0: Fvec4,
    col1: Fvec4,
    col2: Fvec4,
    col3: Fvec4,
});

record!(DebugLineRecord {
    start: Fvec4,
    end: Fvec4,
    color: Fvec4,
});

record!(MaterialRecord {
    base_color: Fvec4,
    emissive: Fvec4,
    base_color_image: u32,
    base_color_sampler: u32,
    metallic_roughness_image: u32,
    metallic_roughness_sampler: u32,
    normal_image: u32,
    normal_sampler: u32,
    occlusion_image: u32,
    occlusion_sampler: u32,
    emissive_image: u32,
    emissive_sampler: u32,
    metallic: f32,
    roughness: f32,
    normal_scale: f32,
    occlusion_strength: f32,
    alpha_mode: u32,
    alpha_cutoff: f32,
});

record!(PointLightRecord {
    position: Fvec4,
    radiance: Fvec4,
    shadow: Fvec4,
});

record!(CascadeRecord {
    clip0: Fvec4,
    clip1: Fvec4,
    clip2: Fvec4,
    clip3: Fvec4,
    atlas: Fvec4,
    split: f32,
    texel: f32,
    depth_span: f32,
    atlas_texel: f32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    pad3: u32,
});

record!(MotionRecord {
    previous0: Fvec4,
    previous1: Fvec4,
    previous2: Fvec4,
    previous3: Fvec4,
    clip0: Fvec4,
    clip1: Fvec4,
    clip2: Fvec4,
    clip3: Fvec4,
    eye: Fvec4,
    forward: Fvec4,
    vertical: f32,
    base: u32,
    previous: u32,
    pad0: u32,
});

record!(TemporalRecord {
    inverse0: Fvec4,
    inverse1: Fvec4,
    inverse2: Fvec4,
    inverse3: Fvec4,
    previous0: Fvec4,
    previous1: Fvec4,
    previous2: Fvec4,
    previous3: Fvec4,
    eye: Fvec4,
    forward: Fvec4,
    previous_eye: Fvec4,
    previous_forward: Fvec4,
    near: f32,
    far: f32,
    vertical: f32,
    width: f32,
    height: f32,
    blend: f32,
    tolerance: f32,
    revealage: f32,
});

record!(LightRecord {
    direction: Fvec4,
    radiance: Fvec4,
    irradiance0: Fvec4,
    irradiance1: Fvec4,
    irradiance2: Fvec4,
    irradiance3: Fvec4,
    irradiance4: Fvec4,
    irradiance5: Fvec4,
    irradiance6: Fvec4,
    irradiance7: Fvec4,
    irradiance8: Fvec4,
    environment0: Fvec4,
    environment1: Fvec4,
    environment2: Fvec4,
    rayleigh: Fvec4,
    mie: Fvec4,
    absorption: Fvec4,
    intensity: f32,
    levels: f32,
    vertical: f32,
    probes: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    pad3: u32,
});

record!(ProbeRecord {
    center: Fvec4,
    inverse0: Fvec4,
    inverse1: Fvec4,
    inverse2: Fvec4,
    extent: Fvec4,
    levels: f32,
    texture: u32,
    sampler: u32,
    pad0: u32,
    irradiance0: Fvec4,
    irradiance1: Fvec4,
    irradiance2: Fvec4,
    irradiance3: Fvec4,
    irradiance4: Fvec4,
    irradiance5: Fvec4,
    irradiance6: Fvec4,
    irradiance7: Fvec4,
    irradiance8: Fvec4,
});

record!(SkyRecord {
    inverse0: Fvec4,
    inverse1: Fvec4,
    inverse2: Fvec4,
    inverse3: Fvec4,
    eye: Fvec4,
    environment0: Fvec4,
    environment1: Fvec4,
    environment2: Fvec4,
    intensity: f32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
});

record!(BakeLevelRecord {
    roughness: f32,
    vertical: f32,
    pad0: u32,
    pad1: u32,
});

record!(IrradianceSetupRecord {
    width: u32,
    height: u32,
    texels: u32,
    partials: u32,
});

record!(IrradianceRecord {
    coefficient0: Fvec4,
    coefficient1: Fvec4,
    coefficient2: Fvec4,
    coefficient3: Fvec4,
    coefficient4: Fvec4,
    coefficient5: Fvec4,
    coefficient6: Fvec4,
    coefficient7: Fvec4,
    coefficient8: Fvec4,
});

record!(AtlasRecord {
    face: f32,
    vertical: f32,
    texel: f32,
    pad0: f32,
});

record!(AtmosphereRecord {
    rayleigh: Fvec4,
    mie: Fvec4,
    absorption: Fvec4,
    ozone: Fvec4,
    ground: Fvec4,
    sun: Fvec4,
    altitude: f32,
    anisotropy: f32,
    vertical: f32,
    width: f32,
    height: f32,
    disk: u32,
    pad0: u32,
    pad1: u32,
});

record!(BloomRecord {
    source_width: f32,
    source_height: f32,
    vertical: f32,
    threshold: f32,
    knee: f32,
});

record!(BloomUpRecord {
    source_width: f32,
    source_height: f32,
    vertical: f32,
});

record!(TonemapRecord {
    vertical: f32,
    intensity: f32,
    source_width: f32,
    source_height: f32,
});

record!(TransparentSetupRecord {
    layers: u32,
    width: u32,
    height: u32,
    vertical: f32,
});
