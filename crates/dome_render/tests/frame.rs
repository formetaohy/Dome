use dome_ecs::World;
use dome_gpu::{
    AddressMode, BufferUsages, FilterMode, Gpu, GpuBuffer, GpuRequest, Sampler, SamplerDesc,
    Submission, Texture, TextureFormat, TextureUsages,
};
use dome_math::{Mat4, Quat, Transform, Vec2, Vec3, Vec4};
use dome_render::environment::Panorama;
use dome_render::{
    AlphaMode, Camera, DirectionalLight, Environment, Lines, MaterialDesc, MaterialImage,
    MeshInstance, PointLight, Renderer, SamplerDesc as RenderSamplerDesc, Skin, SkinVertexRecord,
    Target, Vertex,
};
use std::sync::Arc;

const QUAD: [Vertex; 6] = [
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

const HALF: [Vertex; 6] = [
    Vertex {
        position: [-1.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 0.5],
    },
    Vertex {
        position: [1.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 0.5],
    },
    Vertex {
        position: [1.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 0.5],
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

const PALETTE: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255,
];

const SECOND: [u8; 16] = [
    0, 0, 255, 255, 255, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
];

const CLEAR: [f32; 4] = [0.25, 0.25, 0.25, 1.0];
const WHITE: Vec3 = Vec3::new(1.0, 1.0, 1.0);
const FADED: Vec3 = Vec3::new(0.5, 0.5, 0.5);
const MASKED: [u8; 16] = [0, 255, 0, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 255, 0, 255];
const RED: Vec3 = Vec3::new(1.0, 0.0, 0.0);
const GREEN: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const BLUE: Vec3 = Vec3::new(0.0, 0.0, 1.0);
const SIDE: u32 = 64;

#[test]
fn a_gpu_driven_frame_culls_orders_textures_and_shadows_instances() {
    paint();
}

fn paint() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device renders a frame: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "frame",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let albedo = Texture::new(
        device,
        "albedo",
        2,
        2,
        1,
        TextureFormat::Rgba8UnormSrgb,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    albedo.write(&context.queue(), &PALETTE);
    let second_albedo = Texture::new(
        device,
        "second albedo",
        2,
        2,
        1,
        TextureFormat::Rgba8UnormSrgb,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    second_albedo.write(&context.queue(), &SECOND);
    let palette = Sampler::new(
        device,
        SamplerDesc {
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            compare: None,
        },
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let material = renderer.material(
        MaterialDesc::new(WHITE).base_color_image(MaterialImage::new(albedo, palette.clone())),
    );
    let second = renderer.material(
        MaterialDesc::new(WHITE).base_color_image(MaterialImage::new(second_albedo, palette)),
    );
    let quad = renderer.mesh(&QUAD);
    let half = renderer.mesh(&HALF);
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
    let culled = world.spawn();
    world.insert(
        culled,
        Transform::from_translation(Vec3::new(100.0, 0.0, 0.0)),
    );
    world.insert(
        culled,
        MeshInstance {
            mesh: quad,
            material,
            tint: BLUE,
        },
    );
    let covered = world.spawn();
    world.insert(covered, Transform::IDENTITY);
    world.insert(
        covered,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    let near = world.spawn();
    world.insert(near, Transform::from_translation(Vec3::new(0.0, 0.0, 0.5)));
    world.insert(
        near,
        MeshInstance {
            mesh: half,
            material: second,
            tint: FADED,
        },
    );

    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    assert_eq!(
        renderer.pick(Vec2::new(16.0, 8.0)),
        Some(near),
        "picks the half in front of the full quad"
    );
    assert_eq!(
        renderer.pick(Vec2::new(16.0, 56.0)),
        Some(covered),
        "picks the full quad where the half does not cover it"
    );
    let pixels = read(&context, &target);
    assert_blue(
        &pixels,
        SIDE,
        16,
        8,
        "the near half samples the blue half of its own material",
    );
    assert_yellow(
        &pixels,
        SIDE,
        48,
        8,
        "the near half samples the yellow half of its own material",
    );
    assert_red(
        &pixels,
        SIDE,
        16,
        56,
        "the full quad samples the left texels below the near half",
    );
    assert_green(
        &pixels,
        SIDE,
        48,
        56,
        "the full quad samples the right texels below the near half",
    );
    assert!(
        pixels.iter().all(|pixel| *pixel != [0, 0, 0, 255]),
        "draws the instance of the culled mesh"
    );

    renderer.render(&mut world, Target::Texture(&target), CLEAR, Some(near));
    let pixels = read(&context, &target);
    assert!(
        (28..38).any(|y| {
            let pixel = pixels[(y * SIDE + 16) as usize];
            pixel[2] > 150 && pixel[1] > 60 && pixel[0] < 180
        }),
        "outlines the highlighted instance along the silhouette it covers",
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, Some(culled));
    let pixels = read(&context, &target);
    assert!(
        (28..38).all(|y| {
            let pixel = pixels[(y * SIDE + 16) as usize];
            !(pixel[2] > 150 && pixel[1] > 60 && pixel[0] < 180)
        }),
        "outlines no instance the view culls",
    );

    world.insert(near, Transform::from_translation(Vec3::new(0.0, 0.0, -0.5)));
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    assert_eq!(
        renderer.pick(Vec2::new(16.0, 8.0)),
        Some(covered),
        "picks the full quad in front of the half behind it"
    );
    let pixels = read(&context, &target);
    assert_red(
        &pixels,
        SIDE,
        16,
        8,
        "the depth test hides the half behind the full quad",
    );
    assert_green(&pixels, SIDE, 48, 56, "the full quad keeps the bottom");

    world.despawn(near);
    world.insert(
        covered,
        Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    );
    world.insert(
        sun,
        Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    );
    let occluder = world.spawn();
    world.insert(
        occluder,
        Transform::from_scale_rotation_translation(
            Vec3::splat(0.2),
            Quat::IDENTITY,
            Vec3::new(-0.5, 0.4, -0.5),
        ),
    );
    world.insert(
        occluder,
        MeshInstance {
            mesh: half,
            material,
            tint: WHITE,
        },
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    let shaded = pixels[(16 * SIDE + 16) as usize];
    let lit = pixels[(48 * SIDE + 48) as usize];
    assert!(
        shaded.iter().take(3).all(|channel| *channel < 32),
        "paints the occluded texels {shaded:?}",
    );
    assert!(
        lit.iter().take(3).any(|channel| *channel > 200),
        "lights the texels the occluder misses {lit:?}",
    );

    let mut far = World::new();
    let eye = far.spawn();
    far.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)));
    far.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let sun = far.spawn();
    far.insert(
        sun,
        Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    );
    far.insert(
        sun,
        DirectionalLight {
            color: Vec3::ONE,
            intensity: std::f32::consts::PI,
            shadow_distance: 100.0,
        },
    );
    let receiver = far.spawn();
    far.insert(
        receiver,
        Transform::from_scale_rotation_translation(
            Vec3::splat(4.0),
            Quat::from_rotation_y(std::f32::consts::PI),
            Vec3::new(12.0, 0.0, -15.0),
        ),
    );
    far.insert(
        receiver,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    let caster = far.spawn();
    far.insert(
        caster,
        Transform::from_scale_rotation_translation(
            Vec3::ONE,
            Quat::from_rotation_y(std::f32::consts::PI),
            Vec3::new(12.0, 0.0, -17.0),
        ),
    );
    far.insert(
        caster,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    renderer.render(&mut far, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    let mut dark = false;
    let mut lit = false;
    for y in 24..40 {
        for x in 50..62 {
            let pixel = pixels[(y * SIDE + x) as usize];
            dark |= pixel.iter().take(3).all(|channel| *channel < 32);
            lit |= pixel.iter().take(3).any(|channel| *channel > 200);
        }
    }
    assert!(
        dark && lit,
        "paints a far cascade shadow where dark {dark} and lit {lit} belong",
    );

    let mut occluded = World::new();
    let eye = occluded.spawn();
    occluded.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)));
    occluded.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let sun = occluded.spawn();
    occluded.insert(sun, Transform::IDENTITY);
    occluded.insert(
        sun,
        DirectionalLight {
            color: Vec3::ONE,
            intensity: std::f32::consts::PI,
            shadow_distance: 100.0,
        },
    );
    let occluder = occluded.spawn();
    occluded.insert(
        occluder,
        Transform::from_scale_rotation_translation(Vec3::splat(10.0), Quat::IDENTITY, Vec3::ZERO),
    );
    occluded.insert(
        occluder,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    let hidden = occluded.spawn();
    occluded.insert(
        hidden,
        Transform::from_scale_rotation_translation(
            Vec3::splat(0.1),
            Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -2.0),
        ),
    );
    occluded.insert(
        hidden,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    renderer.render(&mut occluded, Target::Texture(&target), CLEAR, None);
    let visible = renderer.visible_instances();
    assert_eq!(visible, 1, "culls the instance the occluder hides");
    let pixels = read(&context, &target);
    let center = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    assert!(
        center.iter().take(3).any(|channel| *channel > 0),
        "paints the occluder {center:?}",
    );
}

#[test]
fn a_clustered_frame_assigns_point_lights_to_the_tiles_they_reach() {
    points();
}

fn points() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device lights a clustered frame: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "clustered frame",
        256,
        128,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let albedo = Texture::new(
        device,
        "clustered albedo",
        1,
        1,
        1,
        TextureFormat::Rgba8UnormSrgb,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    albedo.write(&context.queue(), &[255, 255, 255, 255]);
    let sampler = Sampler::new(
        device,
        SamplerDesc {
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            compare: None,
        },
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let material = renderer
        .material(MaterialDesc::new(WHITE).base_color_image(MaterialImage::new(albedo, sampler)));
    let quad = renderer.mesh(&QUAD);
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 6.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let wall = world.spawn();
    world.insert(wall, Transform::from_scale(Vec3::new(12.0, 6.0, 1.0)));
    world.insert(
        wall,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    for (translation, color) in [
        (Vec3::new(-6.0, 0.0, 1.0), RED),
        (Vec3::new(6.0, 0.0, 1.0), BLUE),
    ] {
        let light = world.spawn();
        world.insert(light, Transform::from_translation(translation));
        world.insert(light, PointLight::new(color, 1.0, 2.0));
    }
    let occluder = world.spawn();
    world.insert(
        occluder,
        Transform::from_scale_rotation_translation(
            Vec3::new(0.2, 0.2, 1.0),
            Quat::IDENTITY,
            Vec3::new(-6.0, 0.0, 0.9),
        ),
    );
    world.insert(
        occluder,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    let sliver = world.spawn();
    world.insert(
        sliver,
        Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
    );
    world.insert(sliver, PointLight::new(WHITE, 1.0, 0.04));

    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    assert_red(
        &pixels,
        256,
        52,
        64,
        "the occluder stands lit by the red point light it hides",
    );
    let shadowed = pixels[(64 * 256 + 64) as usize];
    assert!(
        shadowed.iter().take(3).all(|channel| *channel < 32),
        "paints the wall behind the point light occluder {shadowed:?}",
    );
    assert_blue(
        &pixels,
        256,
        192,
        64,
        "the right tile holds the blue point light it reaches",
    );
    let center = pixels[(64 * 256 + 128) as usize];
    assert!(
        center.iter().take(3).all(|channel| *channel < 32),
        "paints the unlit center {center:?}",
    );
}

#[test]
fn a_frame_shadows_the_point_lights_the_device_affords() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device shadows a crowded frame: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "crowded point lights",
        64,
        64,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let material = renderer.material(MaterialDesc::new(WHITE));
    let quad = renderer.mesh(&QUAD);
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 6.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let wall = world.spawn();
    world.insert(wall, Transform::from_scale(Vec3::new(12.0, 6.0, 1.0)));
    world.insert(
        wall,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    let lights = device.limits().max_texture_dimension_2d / dome_render::POINT_SHADOW_SIDE + 1;
    for index in 0..lights {
        let light = world.spawn();
        let at = (index as f32 - lights as f32 * 0.5) * 0.25;
        world.insert(light, Transform::from_translation(Vec3::new(at, 0.0, 1.0)));
        world.insert(light, PointLight::new(WHITE, 0.5, 3.0));
    }
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    assert!(
        pixels
            .iter()
            .any(|pixel| pixel.iter().take(3).all(|channel| *channel > 24)),
        "a frame of {lights} point lights lights the wall the nearest of them shadows",
    );
}

fn read(context: &Gpu, target: &Texture) -> Vec<[u8; 4]> {
    let device = context.device();
    let readback = GpuBuffer::new(
        device,
        "frame readback",
        target.readback_bytes(),
        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
    );
    let mut submission = Submission::new(device, "frame readback");
    submission.copy_texture(target, &readback, 0);
    let queue = context.queue();
    let index = submission.submit(&queue);
    let bytes = readback.read(&queue, index, target.readback_bytes());
    target
        .rows(&bytes)
        .flat_map(|row| row.as_chunks::<4>().0)
        .copied()
        .collect()
}

fn assert_red(pixels: &[[u8; 4]], width: u32, x: u32, y: u32, reason: &str) {
    let held = pixels[(y * width + x) as usize];
    assert!(
        held[0] > 32 && held[0] > held[1] + 16 && held[0] > held[2] + 16,
        "paints ({x}, {y}) {held:?} where red belongs: {reason}",
    );
}

fn assert_green(pixels: &[[u8; 4]], width: u32, x: u32, y: u32, reason: &str) {
    let held = pixels[(y * width + x) as usize];
    assert!(
        held[1] > 32 && held[1] > held[0] + 16 && held[1] > held[2] + 16,
        "paints ({x}, {y}) {held:?} where green belongs: {reason}",
    );
}

fn assert_blue(pixels: &[[u8; 4]], width: u32, x: u32, y: u32, reason: &str) {
    let held = pixels[(y * width + x) as usize];
    assert!(
        held[2] > 32 && held[2] > held[0] + 16 && held[2] > held[1] + 16,
        "paints ({x}, {y}) {held:?} where blue belongs: {reason}",
    );
}

fn assert_yellow(pixels: &[[u8; 4]], width: u32, x: u32, y: u32, reason: &str) {
    let held = pixels[(y * width + x) as usize];
    assert!(
        held[0] > 32 && held[1] > 32 && held[0] > held[2] + 16 && held[1] > held[2] + 16,
        "paints ({x}, {y}) {held:?} where yellow belongs: {reason}",
    );
}

#[test]
fn a_transparent_frame_blends_its_layers_masks_cutouts_and_orders_crossing_fragments() {
    blend();
}

fn blend() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device blends a frame: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "transparent frame",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let white = Texture::new(
        device,
        "transparent white",
        1,
        1,
        1,
        TextureFormat::Rgba8UnormSrgb,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    white.write(&context.queue(), &[255, 255, 255, 255]);
    let masked = Texture::new(
        device,
        "transparent masked",
        2,
        2,
        1,
        TextureFormat::Rgba8UnormSrgb,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    masked.write(&context.queue(), &MASKED);
    let sampler = Sampler::new(
        device,
        SamplerDesc {
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            compare: None,
        },
    );
    layers(&context, &target, white.clone(), masked, sampler.clone());
    crossing(&context, &target, white, sampler);
}

fn layers(context: &Gpu, target: &Texture, white: Texture, masked: Texture, sampler: Sampler) {
    let mut renderer = Renderer::new(context, TextureFormat::Rgba8Unorm);
    let opaque = renderer.material(
        MaterialDesc::new(RED).base_color_image(MaterialImage::new(white.clone(), sampler.clone())),
    );
    let far = renderer.material(
        MaterialDesc::new(GREEN)
            .alpha(0.5)
            .alpha_mode(AlphaMode::Blend)
            .base_color_image(MaterialImage::new(white.clone(), sampler.clone())),
    );
    let near = renderer.material(
        MaterialDesc::new(BLUE)
            .alpha(0.5)
            .alpha_mode(AlphaMode::Blend)
            .base_color_image(MaterialImage::new(white.clone(), sampler.clone())),
    );
    let cutout = renderer.material(
        MaterialDesc::new(WHITE)
            .alpha_mode(AlphaMode::Mask)
            .alpha_cutoff(0.5)
            .base_color_image(MaterialImage::new(masked, sampler.clone())),
    );
    let veil = renderer.material(
        MaterialDesc::new(WHITE)
            .alpha(0.5)
            .alpha_mode(AlphaMode::Blend)
            .base_color_image(MaterialImage::new(white.clone(), sampler.clone())),
    );
    let blush = renderer.material(
        MaterialDesc::new(RED)
            .alpha_mode(AlphaMode::Blend)
            .base_color_image(MaterialImage::new(white, sampler)),
    );
    let quad = renderer.mesh(&QUAD);
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)));
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
    let base = world.spawn();
    world.insert(base, Transform::IDENTITY);
    world.insert(
        base,
        MeshInstance {
            mesh: quad,
            material: opaque,
            tint: WHITE,
        },
    );
    let behind = world.spawn();
    world.insert(
        behind,
        Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)),
    );
    world.insert(
        behind,
        MeshInstance {
            mesh: quad,
            material: far,
            tint: WHITE,
        },
    );
    let front = world.spawn();
    world.insert(front, Transform::from_translation(Vec3::new(0.0, 0.0, 1.5)));
    world.insert(
        front,
        MeshInstance {
            mesh: quad,
            material: near,
            tint: WHITE,
        },
    );

    renderer.render(&mut world, Target::Texture(target), CLEAR, None);
    let pixels = read(context, target);
    let center = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    assert!(
        center[2] > 32 && center[2] > center[1] + 16 && center[2] > center[0] + 16,
        "blends the nearer transparent quad over the farther one {center:?}",
    );

    world.despawn(behind);
    world.despawn(front);
    let mask = world.spawn();
    world.insert(mask, Transform::from_translation(Vec3::new(0.0, 0.0, 0.5)));
    world.insert(
        mask,
        MeshInstance {
            mesh: quad,
            material: cutout,
            tint: WHITE,
        },
    );

    renderer.render(&mut world, Target::Texture(target), CLEAR, None);
    let pixels = read(context, target);
    assert_red(
        &pixels,
        SIDE,
        24,
        32,
        "the masked quad discards its transparent half",
    );
    assert_green(
        &pixels,
        SIDE,
        40,
        32,
        "the masked quad renders its opaque half",
    );

    world.despawn(mask);
    let mut veils = Vec::new();
    for depth in [1.7, 1.75, 1.8, 1.85] {
        let layer = world.spawn();
        world.insert(
            layer,
            Transform::from_translation(Vec3::new(0.0, 0.0, depth)),
        );
        world.insert(
            layer,
            MeshInstance {
                mesh: quad,
                material: veil,
                tint: WHITE,
            },
        );
        veils.push(layer);
    }
    let overflowed = world.spawn();
    world.insert(
        overflowed,
        Transform::from_translation(Vec3::new(0.0, 0.0, 1.9)),
    );
    world.insert(
        overflowed,
        MeshInstance {
            mesh: quad,
            material: blush,
            tint: WHITE,
        },
    );
    renderer.render(&mut world, Target::Texture(target), CLEAR, None);
    let pixels = read(context, target);
    let center = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    assert!(
        center[0] > 32 && center[1] > 32 && center[2] > 32 && center[0] > center[1] + 64,
        "keeps the layer nearest the eye over the four it replaces {center:?}",
    );
    assert_eq!(
        renderer.dropped_transparent_fragments(),
        SIDE * SIDE,
        "counts the layer every covered pixel loses beyond its four",
    );

    for veil in veils {
        world.despawn(veil);
    }
    world.despawn(overflowed);
    let single = world.spawn();
    world.insert(
        single,
        Transform::from_translation(Vec3::new(0.0, 0.0, 1.6)),
    );
    world.insert(
        single,
        MeshInstance {
            mesh: quad,
            material: veil,
            tint: WHITE,
        },
    );
    renderer.render(&mut world, Target::Texture(target), CLEAR, None);
    let pixels = read(context, target);
    let center = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    assert!(
        center[0] > center[1] + 32,
        "composites one layer over the stale layers of the last frame {center:?}",
    );
    assert_eq!(
        renderer.dropped_transparent_fragments(),
        0,
        "drops nothing for a layer inside its capacity",
    );
}

fn crossing(context: &Gpu, target: &Texture, white: Texture, sampler: Sampler) {
    let mut renderer = Renderer::new(context, TextureFormat::Rgba8Unorm);
    let red = renderer.material(
        MaterialDesc::new(RED)
            .alpha(0.5)
            .alpha_mode(AlphaMode::Blend)
            .base_color_image(MaterialImage::new(white.clone(), sampler.clone())),
    );
    let blue = renderer.material(
        MaterialDesc::new(BLUE)
            .alpha(0.5)
            .alpha_mode(AlphaMode::Blend)
            .base_color_image(MaterialImage::new(white, sampler)),
    );
    let quad = renderer.mesh(&QUAD);
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)));
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
    let left = world.spawn();
    world.insert(
        left,
        Transform::from_scale_rotation_translation(
            Vec3::ONE,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            Vec3::new(0.0, 0.0, 1.2),
        ),
    );
    world.insert(
        left,
        MeshInstance {
            mesh: quad,
            material: red,
            tint: WHITE,
        },
    );
    let right = world.spawn();
    world.insert(
        right,
        Transform::from_scale_rotation_translation(
            Vec3::ONE,
            Quat::from_rotation_y(-std::f32::consts::FRAC_PI_4),
            Vec3::new(0.0, 0.0, 1.2),
        ),
    );
    world.insert(
        right,
        MeshInstance {
            mesh: quad,
            material: blue,
            tint: WHITE,
        },
    );

    renderer.render(
        &mut world,
        Target::Texture(target),
        [0.0, 0.0, 0.0, 1.0],
        None,
    );
    let pixels = read(context, target);
    let left = pixels[(SIDE as usize / 2) * SIDE as usize + 20];
    let right = pixels[(SIDE as usize / 2) * SIDE as usize + 44];
    assert!(
        left[0] > left[2] + 16,
        "blends the nearer red fragment over the blue one at {left:?}",
    );
    assert!(
        right[2] > right[0] + 16,
        "blends the nearer blue fragment over the red one at {right:?}",
    );
}

#[test]
fn a_linear_frame_resolves_its_highlights_into_a_glow() {
    glow();
}

fn glow() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device resolves a linear frame: {error}"));
    let device = context.device();
    let side = 256;
    let target = Texture::new(
        device,
        "linear frame",
        side,
        side,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let albedo = Texture::new(
        device,
        "linear albedo",
        1,
        1,
        1,
        TextureFormat::Rgba8UnormSrgb,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    albedo.write(&context.queue(), &[255, 255, 255, 255]);
    let sampler = Sampler::new(
        device,
        SamplerDesc {
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            compare: None,
        },
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let material = renderer
        .material(MaterialDesc::new(WHITE).base_color_image(MaterialImage::new(albedo, sampler)));
    let quad = renderer.mesh(&QUAD);
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let sun = world.spawn();
    world.insert(
        sun,
        Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    );
    world.insert(
        sun,
        DirectionalLight {
            color: Vec3::ONE,
            intensity: 40.0 * std::f32::consts::PI,
            shadow_distance: 100.0,
        },
    );
    let bright = world.spawn();
    world.insert(
        bright,
        Transform::from_scale_rotation_translation(
            Vec3::splat(0.25),
            Quat::from_rotation_y(std::f32::consts::PI),
            Vec3::new(0.0, 0.0, -1.0),
        ),
    );
    world.insert(
        bright,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );

    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    let core = pixels[((side / 2) * side + side / 2) as usize];
    let near = pixels[((side / 2) * side + side * 3 / 8) as usize];
    let far = pixels[((side / 2) * side + side / 16) as usize];
    assert!(
        core.iter().take(3).all(|channel| *channel > 240),
        "resolves the high dynamic range core to {core:?}",
    );
    assert!(
        near[0] > far[0] + 8,
        "glows at {near:?} beyond the background {far:?}",
    );
}

#[test]
fn a_lit_frame_samples_every_material_map() {
    maps();
}

fn maps() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device samples a map: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "material frame",
        32,
        32,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let albedo = Texture::new(
        device,
        "material albedo",
        1,
        1,
        1,
        TextureFormat::Rgba8UnormSrgb,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    albedo.write(&context.queue(), &[255, 255, 255, 255]);
    let properties = Texture::new(
        device,
        "material properties",
        1,
        1,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    properties.write(&context.queue(), &[255, 0, 0, 255]);
    let normal = Texture::new(
        device,
        "material normal",
        1,
        1,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    normal.write(&context.queue(), &[128, 128, 255, 255]);
    let glow = Texture::new(
        device,
        "material glow",
        1,
        1,
        1,
        TextureFormat::Rgba8UnormSrgb,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    glow.write(&context.queue(), &[255, 0, 0, 255]);
    let sampler = Sampler::new(
        device,
        SamplerDesc {
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            compare: None,
        },
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let material = renderer.material(
        MaterialDesc::new(WHITE)
            .base_color_image(MaterialImage::new(albedo, sampler.clone()))
            .metallic_roughness_image(MaterialImage::new(properties.clone(), sampler.clone()))
            .normal_image(MaterialImage::new(normal, sampler.clone()))
            .occlusion_image(MaterialImage::new(properties, sampler.clone()))
            .emissive_image(MaterialImage::new(glow, sampler))
            .emissive(Vec3::ONE),
    );
    let quad = renderer.mesh(&QUAD);
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let surface = world.spawn();
    world.insert(surface, Transform::IDENTITY);
    world.insert(
        surface,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );

    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    assert_red(
        &pixels,
        32,
        16,
        16,
        "the emissive map colors the surface it covers",
    );
}

#[test]
fn a_gpu_driven_frame_skins_its_meshes_and_picks_the_deformed_surface() {
    skinned();
}

fn skinned() {
    const SKIN: [SkinVertexRecord; 6] =
        [SkinVertexRecord::new([0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]); 6];
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device skins a frame: {error}"));
    let target = Texture::new(
        context.device(),
        "skinned frame",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let material = renderer.material(MaterialDesc::new(WHITE));
    let mesh = renderer.skinned_mesh(&QUAD, &SKIN);
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
    let joint = world.spawn();
    world.insert(joint, Transform::from_translation(Vec3::new(0.5, 0.0, 0.0)));
    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(
        entity,
        MeshInstance {
            mesh,
            material,
            tint: WHITE,
        },
    );
    world.insert(
        entity,
        Skin::new("joint", vec![joint], Arc::from([Mat4::IDENTITY])),
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    assert_eq!(renderer.visible_instances(), 1);
    assert_eq!(
        renderer.pick(Vec2::new(24.0, 32.0)),
        Some(entity),
        "picks the joint of the skinned quad",
    );
    assert_eq!(
        renderer.pick(Vec2::new(8.0, 32.0)),
        None,
        "picks no rest surface beside the skinned quad",
    );
    let pixels = read(&context, &target);
    assert!(
        pixels[(32 * SIDE + 24) as usize][0] > 16,
        "paints the skinned quad",
    );
}

#[test]
fn a_debug_line_frame_draws_colored_segments() {
    line();
}

fn line() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device draws a line: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "line frame",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 4.0)));
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
    let mut lines = Lines::new();
    lines.push(
        Vec3::new(-2.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec4::new(1.0, 0.0, 0.0, 1.0),
    );
    lines.push(
        Vec3::new(0.5, -0.5, 0.0),
        Vec3::new(0.5, 0.5, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
    );
    world.insert_resource(lines);
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    assert_red(
        &pixels,
        SIDE,
        20,
        32,
        "the horizontal line spans the left of the frame",
    );
    assert_green(
        &pixels,
        SIDE,
        36,
        34,
        "the vertical line spans the right of the frame",
    );

    let material = renderer.material(MaterialDesc::new(WHITE));
    let quad = renderer.mesh(&QUAD);
    let covered = world.spawn();
    world.insert(covered, Transform::IDENTITY);
    world.insert(
        covered,
        MeshInstance {
            mesh: quad,
            material,
            tint: BLUE,
        },
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    assert_green(
        &pixels,
        SIDE,
        36,
        34,
        "the debug line draws over the mesh it crosses",
    );
}

#[test]
fn an_environment_lights_its_scene_and_paints_its_sky() {
    ambient();
}

fn ambient() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device renders a frame: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "ambient",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut rgb = Vec::new();
    for _ in 0..4 {
        for x in 0..8 {
            if !(2..6).contains(&x) {
                rgb.extend_from_slice(&[4.0, 0.0, 0.0]);
            } else {
                rgb.extend_from_slice(&[0.0, 0.0, 1.0]);
            }
        }
    }
    let panorama = Panorama::new(8, 4, &rgb);
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let sampler = renderer.sampler(RenderSamplerDesc {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::ClampToEdge,
        compare: None,
    });
    let panorama_texture = panorama.texture(&renderer, "ambient panorama");
    let request = renderer.bake(&panorama_texture, &sampler);
    let baked = renderer.wait_baked(request);
    let environment = Environment::new(
        baked.radiance,
        panorama_texture,
        sampler,
        baked.levels,
        baked.irradiance,
    );
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let sky_entity = world.spawn();
    world.insert(sky_entity, Transform::IDENTITY);
    world.insert(sky_entity, environment);

    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    let sky = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    println!("sky {sky:?}");
    assert!(
        i32::from(sky[2]) > 200 && i32::from(sky[2]) > i32::from(sky[0]) + 60,
        "paints the blue side of its environment as its sky at {sky:?}",
    );

    let turned = Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI));
    world.insert(sky_entity, turned);
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    let sky = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    println!("turned sky {sky:?}");
    assert!(
        i32::from(sky[0]) > 200 && i32::from(sky[0]) > i32::from(sky[2]) + 40,
        "turns the red side of its environment into its sky at {sky:?}",
    );
    world.insert(sky_entity, Transform::IDENTITY);

    let quad = renderer.mesh(&QUAD);
    let sphere = world.spawn();
    world.insert(sphere, Transform::IDENTITY);
    let materials = [
        (
            "diffuse",
            renderer.material(MaterialDesc::new(WHITE).roughness(0.95)),
        ),
        (
            "smooth metal",
            renderer.material(MaterialDesc::new(WHITE).metallic(1.0).roughness(0.05)),
        ),
        (
            "rough metal",
            renderer.material(MaterialDesc::new(WHITE).metallic(1.0).roughness(0.95)),
        ),
    ];
    for (title, material) in materials {
        world.insert(
            sphere,
            MeshInstance {
                mesh: quad,
                material,
                tint: WHITE,
            },
        );
        renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
        let pixels = read(&context, &target);
        let center = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
        println!("{title} {center:?}");
        if title == "rough metal" {
            assert!(
                i32::from(center[0]) > 100 && i32::from(center[2]) > 100,
                "lights the {title} surface from its environment at {center:?}",
            );
        } else {
            assert!(
                i32::from(center[0]) > 100 && i32::from(center[0]) > i32::from(center[2]) + 30,
                "lights the {title} surface from its red environment at {center:?}",
            );
        }
    }
    world.insert(sky_entity, turned);
    for (title, material) in materials {
        world.insert(
            sphere,
            MeshInstance {
                mesh: quad,
                material,
                tint: WHITE,
            },
        );
        renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
        let pixels = read(&context, &target);
        let center = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
        println!("turned {title} {center:?}");
        if title == "rough metal" {
            assert!(
                i32::from(center[0]) > 100 && i32::from(center[2]) > 100,
                "lights the turned {title} surface from its environment at {center:?}",
            );
        } else {
            assert!(
                i32::from(center[2]) > 100 && i32::from(center[2]) > i32::from(center[0]) + 30,
                "lights the turned {title} surface from its blue environment at {center:?}",
            );
        }
    }
    world.despawn(sphere);
    world.despawn(sphere);
    world.insert(sky_entity, Transform::IDENTITY);
    let held = world.spawn();
    let diffuse = renderer.material(MaterialDesc::new(WHITE).roughness(0.95));
    world.insert(
        held,
        Transform::from_scale_rotation_translation(
            Vec3::splat(20.0),
            Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -8.0),
        ),
    );
    world.insert(
        held,
        MeshInstance {
            mesh: quad,
            material: diffuse,
            tint: WHITE,
        },
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let plain = read(&context, &target)[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    let mut hazy = world
        .get::<Environment>(sky_entity)
        .expect("a lit world holds the environment it lights with")
        .clone();
    hazy.atmosphere = Some(dome_render::environment::Atmosphere {
        rayleigh: 1_300.0,
        mie: 1_300.0,
        ..dome_render::environment::Atmosphere::default()
    });
    world.insert(sky_entity, hazy);
    world.insert(
        held,
        Transform::from_scale_rotation_translation(
            Vec3::splat(60.0),
            Quat::IDENTITY,
            Vec3::new(3.0, 0.0, -30.0),
        ),
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let near = read(&context, &target)[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    world.insert(
        held,
        Transform::from_scale_rotation_translation(
            Vec3::splat(400.0),
            Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -90.0),
        ),
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let far = read(&context, &target)[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    println!("plain {plain:?} near {near:?} far {far:?}");
    assert!(
        i32::from(near[2]) > i32::from(plain[2]) + 10,
        "fades a surface into the sky it stands under: {plain:?} became {near:?}",
    );
    assert!(
        i32::from(far[2]) > i32::from(near[2]) + 15 && i32::from(far[0]) < i32::from(near[0]),
        "fades a distant surface further than a near one: {near:?} against {far:?}",
    );
}

#[test]
fn a_local_environment_lights_the_box_it_bounds() {
    local();
}

fn local() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device renders a frame: {error}"));
    let device = context.device();
    let target = Texture::new(
        device,
        "local environment",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let global = environment(&mut renderer, RED);
    let contained = environment(&mut renderer, BLUE).local(0.2);
    let quad = renderer.mesh(&QUAD);
    let material = renderer.material(MaterialDesc::new(WHITE).metallic(1.0).roughness(0.2));
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 4.0)));
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
    let sky = world.spawn();
    world.insert(sky, Transform::IDENTITY);
    world.insert(sky, global);
    let probe = world.spawn();
    world.insert(
        probe,
        Transform {
            translation: Vec3::new(-0.8, 0.0, 0.0),
            rotation: dome_math::Quat::IDENTITY,
            scale: Vec3::splat(0.6),
        },
    );
    world.insert(probe, contained);
    let inside = world.spawn();
    world.insert(
        inside,
        Transform {
            translation: Vec3::new(-0.8, 0.0, 0.0),
            rotation: dome_math::Quat::IDENTITY,
            scale: Vec3::splat(0.35),
        },
    );
    world.insert(
        inside,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    let outside = world.spawn();
    world.insert(
        outside,
        Transform {
            translation: Vec3::new(0.8, 0.0, 0.0),
            rotation: dome_math::Quat::IDENTITY,
            scale: Vec3::splat(0.35),
        },
    );
    world.insert(
        outside,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);

    assert_blue(
        &pixels,
        SIDE,
        26,
        32,
        "the surface inside the local environment samples its radiance",
    );
    assert_red(
        &pixels,
        SIDE,
        38,
        32,
        "the surface outside the local environment samples the global one",
    );
}

fn environment(renderer: &mut Renderer, radiance: Vec3) -> Environment {
    let panorama = Panorama::new(2, 1, &radiance.to_array().repeat(2));
    let sampler = renderer.sampler(RenderSamplerDesc {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::ClampToEdge,
        compare: None,
    });
    let texture = panorama.texture(renderer, "local panorama");
    let request = renderer.bake(&texture, &sampler);
    let baked = renderer.wait_baked(request);
    Environment::new(
        baked.radiance,
        texture,
        sampler,
        baked.levels,
        baked.irradiance,
    )
}

#[test]
fn a_temporal_frame_accumulates_subpixel_coverage_and_rejects_the_history_of_a_moved_instance() {
    temporal();
}

fn temporal() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device renders a frame: {error}"));
    let target = Texture::new(
        context.device(),
        "temporal frame",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    accumulation(&context, &target);
    movement(&context, &target);
}

fn accumulation(context: &Gpu, target: &Texture) {
    let mut renderer = Renderer::new(context, TextureFormat::Rgba8Unorm);
    let quad = renderer.mesh(&QUAD);
    let material = renderer.material(MaterialDesc::new(WHITE).emissive(WHITE));
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 4.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let turned = world.spawn();
    world.insert(
        turned,
        Transform {
            translation: Vec3::ZERO,
            rotation: Quat::from_rotation_z(0.25),
            scale: Vec3::ONE,
        },
    );
    world.insert(
        turned,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    for _ in 0..8 {
        renderer.render(
            &mut world,
            Target::Texture(target),
            [0.0, 0.0, 0.0, 1.0],
            None,
        );
    }
    let pixels = read(context, target);
    let held = pixels[(SIDE * 32 + 32) as usize];
    assert!(
        held[0] > 200,
        "paints the quad {held:?} where its emission belongs",
    );
    let covered = pixels
        .iter()
        .filter(|pixel| pixel[0] > 40 && pixel[0] < 200)
        .count();
    assert!(
        covered > 4,
        "leaves {covered} pixels of fractional edge coverage where a temporal frame accumulates its jitter",
    );
}

fn movement(context: &Gpu, target: &Texture) {
    let mut renderer = Renderer::new(context, TextureFormat::Rgba8Unorm);
    let quad = renderer.mesh(&QUAD);
    let material = renderer.material(MaterialDesc::new(WHITE).emissive(WHITE));
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 4.0)));
    world.insert(
        eye,
        Camera {
            vertical_fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            far: 100.0,
        },
    );
    let mover = world.spawn();
    world.insert(
        mover,
        Transform {
            translation: Vec3::new(-0.6, 0.0, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::splat(0.3),
        },
    );
    world.insert(
        mover,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    renderer.render(
        &mut world,
        Target::Texture(target),
        [0.0, 0.0, 0.0, 1.0],
        None,
    );
    world.insert(
        mover,
        Transform {
            translation: Vec3::new(0.6, 0.0, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::splat(0.3),
        },
    );
    renderer.render(
        &mut world,
        Target::Texture(target),
        [0.0, 0.0, 0.0, 1.0],
        None,
    );
    let pixels = read(context, target);
    let held = pixels[(SIDE * 32 + 37) as usize];
    assert!(
        held[0] > 200,
        "paints (37, 32) {held:?} where the moved instance stands",
    );
    let left = pixels[(SIDE * 32 + 27) as usize];
    assert!(
        left[0] < 40,
        "leaves {left:?} where the instance no longer stands: a temporal frame rejects the history of a disoccluded surface",
    );
}

#[test]
fn a_frame_of_an_odd_extent_culls_against_the_depth_pyramid_it_builds() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device renders a frame: {error}"));
    let device = context.device();
    let (width, height) = (500, 320);
    let target = Texture::new(
        device,
        "odd frame",
        width,
        height,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let quad = renderer.mesh(&QUAD);
    let material = renderer.material(MaterialDesc::new(WHITE));
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::from_translation(Vec3::new(0.0, 0.0, 4.0)));
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
    let covered = world.spawn();
    world.insert(covered, Transform::IDENTITY);
    world.insert(
        covered,
        MeshInstance {
            mesh: quad,
            material,
            tint: WHITE,
        },
    );
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    let pixels = read(&context, &target);
    let center = pixels[(height / 2 * width + width / 2) as usize];
    let corner = pixels[0];
    assert!(
        center != corner,
        "draws the quad at the center of an odd frame the depth pyramid sizes: {center:?}",
    );
}
