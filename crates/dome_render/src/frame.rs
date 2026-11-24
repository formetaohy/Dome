use crate::abi::Fvec4;
use crate::abi::{CascadeRecord, InstanceRecord, NO_HIGHLIGHT, ViewRecord};
use crate::camera::Camera;
use crate::environment::Environment;
use crate::hidden::Hidden;
use crate::instance::MeshInstance;
use crate::light::DirectionalLight;
use crate::mesh::{Geometry, Mesh};
use crate::palette::Skinning;
use crate::shadow::Cascade;
use crate::view::View;
use dome_ecs::{Entity, World};
use dome_math::{Mat4, Transform, Vec3, Vec4};

pub(crate) struct Batch {
    pub(crate) mesh: Mesh,
    pub(crate) count: u32,
}

pub(crate) struct Screen {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) vertical: f32,
    pub(crate) hzb_levels: u32,
    pub(crate) cluster_x: u32,
    pub(crate) cluster_y: u32,
    pub(crate) point_lights: u32,
    pub(crate) cluster_offset: u32,
    pub(crate) highlight: u32,
}

pub(crate) fn vertical() -> f32 {
    -0.5
}

pub(crate) fn camera_of(world: &World) -> (Camera, Transform) {
    world
        .iter::<Camera>()
        .find_map(|(entity, camera)| {
            if world.contains::<Hidden>(entity) {
                return None;
            }
            world
                .get::<Transform>(entity)
                .map(|transform| (*camera, *transform))
        })
        .unwrap_or_else(|| panic!("a rendered world holds a camera with a transform"))
}

pub(crate) fn light_of(world: &World) -> (DirectionalLight, Transform) {
    world
        .iter::<DirectionalLight>()
        .find_map(|(entity, light)| {
            if world.contains::<Hidden>(entity) {
                return None;
            }
            world
                .get::<Transform>(entity)
                .map(|transform| (*light, *transform))
        })
        .unwrap_or((
            DirectionalLight {
                color: Vec3::ZERO,
                intensity: 0.0,
                shadow_distance: 1_000.0,
            },
            Transform::IDENTITY,
        ))
}

pub(crate) struct Ambient<'a> {
    pub(crate) global: Option<(&'a Environment, Transform)>,
    pub(crate) local: Vec<(&'a Environment, Transform)>,
}

pub(crate) fn ambient_of(world: &World) -> Ambient<'_> {
    let mut ambient = Ambient {
        global: None,
        local: Vec::new(),
    };
    for (entity, environment) in world.iter::<Environment>() {
        if world.contains::<Hidden>(entity) {
            continue;
        }
        let transform = world
            .get::<Transform>(entity)
            .copied()
            .unwrap_or(Transform::IDENTITY);
        if environment.local {
            ambient.local.push((environment, transform));
        } else if ambient.global.is_none() {
            ambient.global = Some((environment, transform));
        }
    }
    ambient
}

pub(crate) fn record_of_view(
    view: &View,
    instances: u32,
    eye: Vec3,
    forward: Vec3,
    palette_base: u32,
) -> ViewRecord {
    ViewRecord {
        clip0: vector(view.clip.col(0)),
        clip1: vector(view.clip.col(1)),
        clip2: vector(view.clip.col(2)),
        clip3: vector(view.clip.col(3)),
        plane0: vector(view.planes[0]),
        plane1: vector(view.planes[1]),
        plane2: vector(view.planes[2]),
        plane3: vector(view.planes[3]),
        plane4: vector(view.planes[4]),
        plane5: vector(view.planes[5]),
        instances,
        hzb_levels: 0,
        view_width: 0.0,
        view_height: 0.0,
        eye: direction_record(eye),
        forward: direction_record(forward),
        near: 0.0,
        far: 0.0,
        tan_vertical: 0.0,
        aspect: 0.0,
        vertical: 0.0,
        palette_base,
        pad0: 0,
        cluster_x: 0,
        cluster_y: 0,
        point_lights: 0,
        cluster_offset: 0,
        highlight: NO_HIGHLIGHT,
    }
}

pub(crate) fn record_of_main_view(
    view: &View,
    instances: u32,
    eye: Vec3,
    forward: Vec3,
    palette_base: u32,
    camera: Camera,
    screen: Screen,
) -> ViewRecord {
    ViewRecord {
        hzb_levels: screen.hzb_levels,
        view_width: screen.width as f32,
        view_height: screen.height as f32,
        near: camera.near,
        far: camera.far,
        tan_vertical: (camera.vertical_fov * 0.5).tan(),
        aspect: screen.width as f32 / screen.height as f32,
        vertical: screen.vertical,
        cluster_x: screen.cluster_x,
        cluster_y: screen.cluster_y,
        point_lights: screen.point_lights,
        cluster_offset: screen.cluster_offset,
        highlight: screen.highlight,
        ..record_of_view(view, instances, eye, forward, palette_base)
    }
}

pub(crate) fn record_of_cascade(cascade: &Cascade) -> CascadeRecord {
    CascadeRecord {
        clip0: vector(cascade.clip.col(0)),
        clip1: vector(cascade.clip.col(1)),
        clip2: vector(cascade.clip.col(2)),
        clip3: vector(cascade.clip.col(3)),
        atlas: Fvec4 {
            x: cascade.atlas_origin.x,
            y: cascade.atlas_origin.y,
            z: cascade.atlas_scale,
            w: 0.0,
        },
        split: cascade.split,
        texel: cascade.texel,
        depth_span: cascade.depth_span,
        atlas_texel: cascade.atlas_texel,
        pad0: 0,
        pad1: 0,
        pad2: 0,
        pad3: 0,
    }
}

pub(crate) struct Placement {
    pub(crate) batch: u32,
    pub(crate) base: u32,
    pub(crate) skinning: Skinning,
    pub(crate) hzb: bool,
}

pub(crate) fn record_of(
    entity: Entity,
    transform: Transform,
    previous: Mat4,
    instance: &MeshInstance,
    bounds: (Vec3, f32),
    placement: Placement,
) -> InstanceRecord {
    assert!(
        transform.scale.x != 0.0 && transform.scale.y != 0.0 && transform.scale.z != 0.0,
        "a rendered instance scales a normal by zero"
    );
    let model = transform.matrix();
    let rotation = transform.rotation;
    let normal = |axis, scale| rotation.rotate_vec3(axis) / scale;
    let scale = transform.scale;
    let center = bounds.0;
    InstanceRecord {
        model0: vector(model.col(0)),
        model1: vector(model.col(1)),
        model2: vector(model.col(2)),
        model3: vector(model.col(3)),
        previous0: vector(previous.col(0)),
        previous1: vector(previous.col(1)),
        previous2: vector(previous.col(2)),
        previous3: vector(previous.col(3)),
        normal0: direction_record(normal(Vec3::X, scale.x)),
        normal1: direction_record(normal(Vec3::Y, scale.y)),
        normal2: direction_record(normal(Vec3::Z, scale.z)),
        tint: vector(instance.tint.extend(0.0)),
        bounds: Fvec4 {
            x: center.x,
            y: center.y,
            z: center.z,
            w: bounds.1,
        },
        batch: placement.batch,
        base: placement.base,
        entity: entity.index(),
        material: instance.material.index(),
        palette: placement.skinning.palette,
        joints: placement.skinning.joints,
        hzb: u32::from(placement.hzb),
        pad0: 0,
    }
}

pub(crate) fn bases_of(counts: &[u32], blocks: u32) -> (Vec<u32>, u32) {
    let mut slot = 0;
    let bases = counts
        .iter()
        .map(|count| {
            let held = slot;
            slot = (slot + count).div_ceil(blocks) * blocks;
            held
        })
        .collect();
    (bases, slot)
}

pub(crate) fn arguments_of(geometry: &Geometry) -> [u32; 4] {
    [geometry.count, 0, 0, 0]
}

pub(crate) fn direction_record(value: Vec3) -> Fvec4 {
    Fvec4 {
        x: value.x,
        y: value.y,
        z: value.z,
        w: 0.0,
    }
}

pub(crate) fn vector(value: Vec4) -> Fvec4 {
    Fvec4 {
        x: value.x,
        y: value.y,
        z: value.z,
        w: value.w,
    }
}
