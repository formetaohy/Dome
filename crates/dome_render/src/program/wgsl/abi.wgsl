enable wgpu_binding_array;

struct View {
    clip0: vec4<f32>,
    clip1: vec4<f32>,
    clip2: vec4<f32>,
    clip3: vec4<f32>,
    plane0: vec4<f32>,
    plane1: vec4<f32>,
    plane2: vec4<f32>,
    plane3: vec4<f32>,
    plane4: vec4<f32>,
    plane5: vec4<f32>,
    instances: u32,
    hzb_levels: u32,
    view_width: f32,
    view_height: f32,
    eye: vec4<f32>,
    forward: vec4<f32>,
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
};

struct Instance {
    model0: vec4<f32>,
    model1: vec4<f32>,
    model2: vec4<f32>,
    model3: vec4<f32>,
    previous0: vec4<f32>,
    previous1: vec4<f32>,
    previous2: vec4<f32>,
    previous3: vec4<f32>,
    normal0: vec4<f32>,
    normal1: vec4<f32>,
    normal2: vec4<f32>,
    tint: vec4<f32>,
    bounds: vec4<f32>,
    batch: u32,
    base: u32,
    entity: u32,
    material: u32,
    palette: u32,
    joints: u32,
    hzb: u32,
    pad0: u32,
};

struct SkinVertex {
    joints: vec4<u32>,
    weights: vec4<f32>,
};

struct Joint {
    col0: vec4<f32>,
    col1: vec4<f32>,
    col2: vec4<f32>,
    col3: vec4<f32>,
};

struct DebugLine {
    start: vec4<f32>,
    end: vec4<f32>,
    color: vec4<f32>,
};

struct Material {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
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
};

struct PointLight {
    position: vec4<f32>,
    radiance: vec4<f32>,
    shadow: vec4<f32>,
};

struct Cascade {
    clip0: vec4<f32>,
    clip1: vec4<f32>,
    clip2: vec4<f32>,
    clip3: vec4<f32>,
    atlas: vec4<f32>,
    split: f32,
    texel: f32,
    depth_span: f32,
    atlas_texel: f32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    pad3: u32,
};

struct Motion {
    previous0: vec4<f32>,
    previous1: vec4<f32>,
    previous2: vec4<f32>,
    previous3: vec4<f32>,
    clip0: vec4<f32>,
    clip1: vec4<f32>,
    clip2: vec4<f32>,
    clip3: vec4<f32>,
    eye: vec4<f32>,
    forward: vec4<f32>,
    vertical: f32,
    base: u32,
    previous: u32,
    pad0: u32,
};

struct Temporal {
    inverse0: vec4<f32>,
    inverse1: vec4<f32>,
    inverse2: vec4<f32>,
    inverse3: vec4<f32>,
    previous0: vec4<f32>,
    previous1: vec4<f32>,
    previous2: vec4<f32>,
    previous3: vec4<f32>,
    eye: vec4<f32>,
    forward: vec4<f32>,
    previous_eye: vec4<f32>,
    previous_forward: vec4<f32>,
    near: f32,
    far: f32,
    vertical: f32,
    width: f32,
    height: f32,
    blend: f32,
    tolerance: f32,
    revealage: f32,
};

struct Light {
    direction: vec4<f32>,
    radiance: vec4<f32>,
    irradiance0: vec4<f32>,
    irradiance1: vec4<f32>,
    irradiance2: vec4<f32>,
    irradiance3: vec4<f32>,
    irradiance4: vec4<f32>,
    irradiance5: vec4<f32>,
    irradiance6: vec4<f32>,
    irradiance7: vec4<f32>,
    irradiance8: vec4<f32>,
    environment0: vec4<f32>,
    environment1: vec4<f32>,
    environment2: vec4<f32>,
    rayleigh: vec4<f32>,
    mie: vec4<f32>,
    absorption: vec4<f32>,
    intensity: f32,
    levels: f32,
    vertical: f32,
    probes: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    pad3: u32,
};

struct Probe {
    center: vec4<f32>,
    inverse0: vec4<f32>,
    inverse1: vec4<f32>,
    inverse2: vec4<f32>,
    extent: vec4<f32>,
    levels: f32,
    texture: u32,
    sampler: u32,
    pad0: u32,
    irradiance0: vec4<f32>,
    irradiance1: vec4<f32>,
    irradiance2: vec4<f32>,
    irradiance3: vec4<f32>,
    irradiance4: vec4<f32>,
    irradiance5: vec4<f32>,
    irradiance6: vec4<f32>,
    irradiance7: vec4<f32>,
    irradiance8: vec4<f32>,
};

struct Sky {
    inverse0: vec4<f32>,
    inverse1: vec4<f32>,
    inverse2: vec4<f32>,
    inverse3: vec4<f32>,
    eye: vec4<f32>,
    environment0: vec4<f32>,
    environment1: vec4<f32>,
    environment2: vec4<f32>,
    intensity: f32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

struct BakeLevel {
    roughness: f32,
    vertical: f32,
    pad0: u32,
    pad1: u32,
};

struct IrradianceSetup {
    width: u32,
    height: u32,
    texels: u32,
    partials: u32,
};

struct Irradiance {
    coefficient0: vec4<f32>,
    coefficient1: vec4<f32>,
    coefficient2: vec4<f32>,
    coefficient3: vec4<f32>,
    coefficient4: vec4<f32>,
    coefficient5: vec4<f32>,
    coefficient6: vec4<f32>,
    coefficient7: vec4<f32>,
    coefficient8: vec4<f32>,
};

struct Atlas {
    face: f32,
    vertical: f32,
    texel: f32,
    pad0: f32,
};

struct Atmosphere {
    rayleigh: vec4<f32>,
    mie: vec4<f32>,
    absorption: vec4<f32>,
    ozone: vec4<f32>,
    ground: vec4<f32>,
    sun: vec4<f32>,
    altitude: f32,
    anisotropy: f32,
    vertical: f32,
    width: f32,
    height: f32,
    disk: u32,
    pad0: u32,
    pad1: u32,
};

struct Bloom {
    source_width: f32,
    source_height: f32,
    vertical: f32,
    threshold: f32,
    knee: f32,
};

struct BloomUp {
    source_width: f32,
    source_height: f32,
    vertical: f32,
};

struct Tonemap {
    vertical: f32,
    intensity: f32,
    source_width: f32,
    source_height: f32,
};

struct TransparentSetup {
    layers: u32,
    width: u32,
    height: u32,
    vertical: f32,
};
