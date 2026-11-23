use dome_ecs::World;
use dome_gpu::{
    AddressMode, Backend, Filter, GpuContext, GpuRequest, SamplerDesc, Texture, TextureFormat,
    TextureUsages,
};
use dome_math::{Quat, Transform, Vec3};
use dome_render::{
    Camera, Capture, CaptureProbe, DirectionalLight, Environment, MaterialDesc, MeshInstance,
    Renderer, SceneDamage, Target, Vertex,
};

const WALL: [Vertex; 6] = [
    Vertex {
        position: [-1.0, -1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, -1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 0.0],
    },
];

const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[test]
fn a_probe_captures_the_boxes_and_damage_its_box_holds() {
    for backend in dome_gpu::PREFERENCE {
        capture(*backend);
        shadowed(*backend);
        damaged(*backend);
    }
}

fn shadowed(backend: Backend) {
    let lit = lit_probe(backend, false);
    let held = lit_probe(backend, true);
    assert!(
        lit[0].x > held[0].x * 2.0,
        "{backend:?} shades the probe it captures away from the main view: {lit:?} lit {held:?} occluded",
    );
}

fn lit_probe(backend: Backend, occluder: bool) -> [Vec3; 9] {
    let context = GpuContext::open(&GpuRequest {
        backend: Some(backend),
        ..Default::default()
    })
    .unwrap_or_else(|error| panic!("no {backend:?} device captures a probe: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "shadowed probe frame",
        64,
        64,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let quad = renderer.mesh(&WALL);
    let white = renderer.material(MaterialDesc::new(Vec3::ONE));
    let black = renderer.material(MaterialDesc::new(Vec3::ZERO));
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(
        eye,
        Transform::from_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    );
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 20.0,
        },
    );
    let sun = world.spawn();
    world.insert(sun, Transform::IDENTITY);
    world.insert(
        sun,
        DirectionalLight {
            color: Vec3::ONE,
            intensity: 1.0,
            shadow_distance: 100.0,
        },
    );
    let ground = world.spawn();
    world.insert(
        ground,
        Transform::from_scale_rotation_translation(
            Vec3::splat(6.0),
            Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -8.0),
        ),
    );
    world.insert(
        ground,
        MeshInstance {
            mesh: quad,
            material: white,
            tint: Vec3::ONE,
        },
    );
    if occluder {
        let held = world.spawn();
        world.insert(
            held,
            Transform::from_scale_rotation_translation(
                Vec3::splat(6.0),
                Quat::IDENTITY,
                Vec3::new(0.0, 0.0, 2.0),
            ),
        );
        world.insert(
            held,
            MeshInstance {
                mesh: quad,
                material: black,
                tint: Vec3::ONE,
            },
        );
    }
    let probe = world.spawn();
    world.insert(probe, Transform::from_scale(Vec3::splat(12.0)));
    world.insert(probe, placeholder(&mut renderer));
    world.insert(probe, CaptureProbe::default());
    let captures = settled(&mut renderer, &mut world, &target);
    assert_eq!(captures.len(), 1, "{backend:?} captures the lit probe");
    captures[0].environment.irradiance
}

fn capture(backend: Backend) {
    let context = GpuContext::open(&GpuRequest {
        backend: Some(backend),
        ..Default::default()
    })
    .unwrap_or_else(|error| panic!("no {backend:?} device captures a probe: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "capture frame",
        64,
        64,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let quad = renderer.mesh(&WALL);
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let sun = world.spawn();
    world.insert(sun, Transform::IDENTITY);
    world.insert(
        sun,
        DirectionalLight {
            color: Vec3::ONE,
            intensity: 0.0,
            shadow_distance: 100.0,
        },
    );
    let walls = [
        (
            Vec3::X,
            Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
            Vec3::X,
        ),
        (
            -Vec3::X,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Vec3::Y,
        ),
        (
            Vec3::Y,
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            Vec3::Z,
        ),
        (
            -Vec3::Y,
            Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            Vec3::ONE,
        ),
        (Vec3::Z, Quat::IDENTITY, Vec3::new(1.0, 1.0, 0.0)),
        (
            -Vec3::Z,
            Quat::from_rotation_y(std::f32::consts::PI),
            Vec3::new(1.0, 0.0, 1.0),
        ),
    ];
    for (at, rotation, color) in walls {
        let wall = world.spawn();
        world.insert(
            wall,
            Transform::from_scale_rotation_translation(Vec3::splat(6.0), rotation, at * 8.0),
        );
        world.insert(
            wall,
            MeshInstance {
                mesh: quad,
                material: renderer.material(MaterialDesc::new(Vec3::ONE).emissive(color)),
                tint: Vec3::ONE,
            },
        );
    }
    let probe = world.spawn();
    world.insert(probe, Transform::from_scale(Vec3::splat(4.0)));
    world.insert(probe, placeholder(&mut renderer));
    world.insert(probe, CaptureProbe::default());

    let captures = settled(&mut renderer, &mut world, &target);
    assert_eq!(
        captures.len(),
        1,
        "{backend:?} captures the probe its box holds",
    );
    let held = &captures[0];
    assert_eq!(held.entity, probe);
    assert!(held.environment.local, "{backend:?} keeps the probe local");
    let irradiance = held.environment.irradiance;
    assert!(
        irradiance[3].x > 0.0 && irradiance[3].y < 0.0,
        "{backend:?} lights the probe red on +x and green on -x: {irradiance:?}",
    );
    assert!(
        irradiance[1].z > 0.0,
        "{backend:?} lights the probe blue above it: {irradiance:?}",
    );
    assert!(
        irradiance[2].x > 0.0,
        "{backend:?} lights the probe yellow in front of it: {irradiance:?}",
    );
    assert!(
        irradiance
            .iter()
            .all(|held| held.x.is_finite() && held.y.is_finite() && held.z.is_finite()),
        "{backend:?} captures a finite environment: {irradiance:?}",
    );
    dome_render::install_captures(&mut world, captures);

    assert!(
        settled(&mut renderer, &mut world, &target).is_empty(),
        "{backend:?} captures a probe once for the box it holds",
    );
    world.insert(
        probe,
        Transform::from_scale_rotation_translation(
            Vec3::splat(4.0),
            Quat::IDENTITY,
            Vec3::new(1.0, 0.0, 0.0),
        ),
    );
    assert_eq!(
        settled(&mut renderer, &mut world, &target).len(),
        1,
        "{backend:?} captures a probe whose box moved",
    );
}

fn damaged(backend: Backend) {
    let context = GpuContext::open(&GpuRequest {
        backend: Some(backend),
        ..Default::default()
    })
    .unwrap_or_else(|error| panic!("no {backend:?} device recaptures a probe: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "damaged probe frame",
        64,
        64,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let quad = renderer.mesh(&WALL);
    let white = renderer.material(MaterialDesc::new(Vec3::ONE));
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let sun = world.spawn();
    world.insert(sun, Transform::IDENTITY);
    world.insert(
        sun,
        DirectionalLight {
            color: Vec3::ONE,
            intensity: std::f32::consts::PI,
            shadow_distance: 100.0,
        },
    );
    let mut instance = |at: Vec3| {
        let held = world.spawn();
        world.insert(held, Transform::from_translation(at));
        world.insert(
            held,
            MeshInstance {
                mesh: quad,
                material: white,
                tint: Vec3::ONE,
            },
        );
        held
    };
    let near = instance(Vec3::new(-8.0, 0.0, -2.0));
    instance(Vec3::new(8.0, 0.0, -2.0));
    let left = world.spawn();
    world.insert(
        left,
        Transform::from_scale_rotation_translation(
            Vec3::splat(4.0),
            Quat::IDENTITY,
            Vec3::new(-8.0, 0.0, 0.0),
        ),
    );
    world.insert(left, placeholder(&mut renderer));
    world.insert(left, CaptureProbe::default());
    let right = world.spawn();
    world.insert(
        right,
        Transform::from_scale_rotation_translation(
            Vec3::splat(4.0),
            Quat::IDENTITY,
            Vec3::new(8.0, 0.0, 0.0),
        ),
    );
    world.insert(right, placeholder(&mut renderer));
    world.insert(right, CaptureProbe::default());
    let mut captured = Vec::new();
    while captured.len() < 2 {
        let captures = settled(&mut renderer, &mut world, &target);
        assert!(
            !captures.is_empty(),
            "{backend:?} captures both probes: {captured:?}",
        );
        captured.extend(captures.iter().map(|held| held.entity));
        dome_render::install_captures(&mut world, captures);
    }
    assert!(
        settled(&mut renderer, &mut world, &target).is_empty(),
        "{backend:?} captures a probe once for the box it holds",
    );
    world.insert_resource(SceneDamage::new(1, vec![near], false));
    let captures = settled(&mut renderer, &mut world, &target);
    assert_eq!(
        captures.iter().map(|held| held.entity).collect::<Vec<_>>(),
        [left],
        "{backend:?} rebakes the probe whose box holds the damaged mesh",
    );
    dome_render::install_captures(&mut world, captures);
    assert!(
        settled(&mut renderer, &mut world, &target).is_empty(),
        "{backend:?} leaves the probe the damage misses baked",
    );
    world.insert_resource(SceneDamage::new(2, Vec::new(), true));
    let mut again = Vec::new();
    while again.len() < 2 {
        let captures = settled(&mut renderer, &mut world, &target);
        assert!(
            !captures.is_empty(),
            "{backend:?} rebakes every probe the sky reaches: {again:?}",
        );
        again.extend(captures.iter().map(|held| held.entity));
        dome_render::install_captures(&mut world, captures);
    }
    again.sort();
    let mut both = vec![left, right];
    both.sort();
    assert_eq!(
        again, both,
        "{backend:?} rebakes one probe per frame the sky changed",
    );
}

fn settled(renderer: &mut Renderer, world: &mut World, target: &Texture) -> Vec<Capture> {
    for _ in 0..8 {
        renderer.render(world, Target::Texture(target), CLEAR, None);
        let captures = renderer.take_captures();
        if !captures.is_empty() {
            return captures;
        }
    }
    Vec::new()
}

fn placeholder(renderer: &mut Renderer) -> Environment {
    let texel = [0u16, 0u16, 0u16, 0x3C00];
    let bytes = bytemuck::cast_slice(&texel);
    let texture = |label: &str| {
        let texture = renderer.texture(
            label,
            1,
            1,
            1,
            TextureFormat::Rgba16Float,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        renderer.write_texture(&texture, &[bytes]);
        texture
    };
    let radiance = texture("capture placeholder radiance");
    let panorama = texture("capture placeholder panorama");
    let sampler = renderer.sampler(SamplerDesc {
        min_filter: Filter::Linear,
        mag_filter: Filter::Linear,
        mip_filter: Filter::Nearest,
        address_u: AddressMode::Repeat,
        address_v: AddressMode::ClampToEdge,
    });
    Environment::new(radiance, panorama, sampler, 1, [Vec3::ZERO; 9]).local(0.5)
}
