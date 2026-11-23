#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReferenceKind {
    Node,
    Mesh,
    Material,
    Image,
    Model,
    Skin,
    Animation,
    Sound,
    Graph,
    AudioBus,
    Environment,
}

impl ReferenceKind {
    pub const ALL: [Self; 11] = [
        Self::Node,
        Self::Mesh,
        Self::Material,
        Self::Image,
        Self::Model,
        Self::Skin,
        Self::Animation,
        Self::Sound,
        Self::Graph,
        Self::AudioBus,
        Self::Environment,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Mesh => "mesh",
            Self::Material => "material",
            Self::Image => "image",
            Self::Model => "model",
            Self::Skin => "skin",
            Self::Animation => "animation",
            Self::Sound => "sound",
            Self::Graph => "graph",
            Self::AudioBus => "audio_bus",
            Self::Environment => "environment",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldKind {
    Number { range: [f32; 2], speed: f32 },
    Bool,
    Text,
    Choice(&'static [&'static str]),
    Vector { range: [f32; 2], speed: f32 },
    Rotation { range: [f32; 2], speed: f32 },
    Color,
    Tint,
    Bits { count: u8 },
    Reference(ReferenceKind),
    OptionalReference(ReferenceKind),
}

impl FieldKind {
    pub const fn with_range(self, range: [f32; 2]) -> Self {
        match self {
            Self::Number { speed, .. } => Self::Number { range, speed },
            Self::Vector { speed, .. } => Self::Vector { range, speed },
            Self::Rotation { speed, .. } => Self::Rotation { range, speed },
            _ => panic!("a scene range applies to a number, vector or rotation"),
        }
    }

    pub const fn with_speed(self, speed: f32) -> Self {
        match self {
            Self::Number { range, .. } => Self::Number { range, speed },
            Self::Vector { range, .. } => Self::Vector { range, speed },
            Self::Rotation { range, .. } => Self::Rotation { range, speed },
            _ => panic!("a scene speed applies to a number, vector or rotation"),
        }
    }

    pub const fn with_choices(self, choices: &'static [&'static str]) -> Self {
        match self {
            Self::Text | Self::Choice(_) => Self::Choice(choices),
            _ => panic!("scene choices apply to text"),
        }
    }

    pub const fn with_color(self) -> Self {
        match self {
            Self::Vector { .. } | Self::Color => Self::Color,
            _ => panic!("a scene color applies to a vector"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Number { .. } => "number",
            Self::Bool => "boolean",
            Self::Text => "text",
            Self::Choice(_) => "choice",
            Self::Vector { .. } => "vector",
            Self::Rotation { .. } => "rotation",
            Self::Color => "color",
            Self::Tint => "tint",
            Self::Bits { .. } => "bits",
            Self::Reference(_) => "reference",
            Self::OptionalReference(_) => "reference",
        }
    }
}

pub const SOUND_FIELDS: &[Field] = &[
    Field {
        name: "path",
        label: "Path",
        kind: FieldKind::Text,
    },
    Field {
        name: "stream",
        label: "Stream",
        kind: FieldKind::Bool,
    },
];

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
}

impl Field {
    pub fn units(self) -> Option<&'static str> {
        match self.name {
            "rotation" | "vertical_fov" | "cone" | "swing" | "sun_elevation" | "sun_azimuth" => {
                Some("degrees")
            }
            "gain_db" | "send_db" | "threshold_db" | "makeup_db" => Some("dB"),
            "frequency" => Some("Hz"),
            "attack_ms" | "release_ms" => Some("ms"),
            "altitude" => Some("m"),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    None,
    Node,
    World,
}

pub struct ComponentSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub required: bool,
    pub fields: &'static [Field],
    pub runtime_fields: &'static [&'static str],
    pub reach: Reach,
}

pub const NODE_FIELDS: &[Field] = &[Field {
    name: "parent",
    label: "Parent",
    kind: FieldKind::Reference(ReferenceKind::Node),
}];

pub const MATERIAL_FIELDS: &[Field] = &[
    Field {
        name: "base_color",
        label: "Base Color",
        kind: FieldKind::Color,
    },
    Field {
        name: "alpha",
        label: "Alpha",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "alpha_mode",
        label: "Alpha Mode",
        kind: FieldKind::Choice(&["opaque", "mask", "blend"]),
    },
    Field {
        name: "alpha_cutoff",
        label: "Alpha Cutoff",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "base_color_image",
        label: "Base Color Image",
        kind: FieldKind::OptionalReference(ReferenceKind::Image),
    },
    Field {
        name: "metallic",
        label: "Metallic",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "roughness",
        label: "Roughness",
        kind: FieldKind::Number {
            range: [0.0001, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "metallic_roughness_image",
        label: "Metallic Roughness Image",
        kind: FieldKind::OptionalReference(ReferenceKind::Image),
    },
    Field {
        name: "normal_image",
        label: "Normal Image",
        kind: FieldKind::OptionalReference(ReferenceKind::Image),
    },
    Field {
        name: "normal_scale",
        label: "Normal Scale",
        kind: FieldKind::Number {
            range: [0.0, 4.0],
            speed: 0.01,
        },
    },
    Field {
        name: "occlusion_image",
        label: "Occlusion Image",
        kind: FieldKind::OptionalReference(ReferenceKind::Image),
    },
    Field {
        name: "occlusion_strength",
        label: "Occlusion Strength",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "emissive",
        label: "Emissive",
        kind: FieldKind::Color,
    },
    Field {
        name: "emissive_image",
        label: "Emissive Image",
        kind: FieldKind::OptionalReference(ReferenceKind::Image),
    },
];

pub const MESH_FIELDS: &[Field] = &[Field {
    name: "shape",
    label: "Shape",
    kind: FieldKind::Choice(&["plane", "cube", "sphere"]),
}];

pub const ENVIRONMENT_PANORAMA_FIELDS: &[Field] = &[Field {
    name: "path",
    label: "Path",
    kind: FieldKind::Text,
}];

pub const ENVIRONMENT_ATMOSPHERE_FIELDS: &[Field] = &[
    Field {
        name: "sun_elevation",
        label: "Sun Elevation",
        kind: FieldKind::Number {
            range: [-90.0, 90.0],
            speed: 0.5,
        },
    },
    Field {
        name: "sun_azimuth",
        label: "Sun Azimuth",
        kind: FieldKind::Number {
            range: [0.0, 360.0],
            speed: 1.0,
        },
    },
    Field {
        name: "sun_intensity",
        label: "Sun Intensity",
        kind: FieldKind::Number {
            range: [0.01, 20.0],
            speed: 0.05,
        },
    },
    Field {
        name: "rayleigh",
        label: "Rayleigh",
        kind: FieldKind::Number {
            range: [0.0, 4.0],
            speed: 0.01,
        },
    },
    Field {
        name: "mie",
        label: "Mie",
        kind: FieldKind::Number {
            range: [0.0, 4.0],
            speed: 0.01,
        },
    },
    Field {
        name: "mie_anisotropy",
        label: "Mie Anisotropy",
        kind: FieldKind::Number {
            range: [-0.99, 0.99],
            speed: 0.005,
        },
    },
    Field {
        name: "ozone",
        label: "Ozone",
        kind: FieldKind::Number {
            range: [0.0, 4.0],
            speed: 0.01,
        },
    },
    Field {
        name: "ground_albedo",
        label: "Ground Albedo",
        kind: FieldKind::Color,
    },
    Field {
        name: "altitude",
        label: "Altitude",
        kind: FieldKind::Number {
            range: [0.0, 20000.0],
            speed: 10.0,
        },
    },
];

pub const IMAGE_FIELDS: &[Field] = &[
    Field {
        name: "path",
        label: "Path",
        kind: FieldKind::Text,
    },
    Field {
        name: "min_filter",
        label: "Min Filter",
        kind: FieldKind::Choice(&["linear", "nearest"]),
    },
    Field {
        name: "mag_filter",
        label: "Mag Filter",
        kind: FieldKind::Choice(&["linear", "nearest"]),
    },
    Field {
        name: "mip_filter",
        label: "Mip Filter",
        kind: FieldKind::Choice(&["linear", "nearest"]),
    },
    Field {
        name: "wrap_u",
        label: "Wrap U",
        kind: FieldKind::Choice(&["repeat", "mirror", "clamp"]),
    },
    Field {
        name: "wrap_v",
        label: "Wrap V",
        kind: FieldKind::Choice(&["repeat", "mirror", "clamp"]),
    },
];

pub const MODEL_FIELDS: &[Field] = &[Field {
    name: "path",
    label: "Path",
    kind: FieldKind::Text,
}];
