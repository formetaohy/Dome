use dome_gpu_device::{AddressMode, Backend, Filter, GpuContext, GpuRequest, Queue, Texture};
use dome_math::Vec3;
use dome_render::environment::{LEVELS, Panorama, SPECULAR_WIDTH};
use dome_render::{Renderer, SamplerDesc, TextureFormat};
use half::f16;
use std::f32::consts::PI;

const WIDTH: u32 = 32;
const HEIGHT: u32 = 16;

#[test]
fn a_baked_environment_lights_a_uniform_sky_evenly_and_holds_its_hemispheres_apart() {
    for backend in dome_gpu_device::PREFERENCE {
        baked(*backend);
    }
}

fn baked(backend: Backend) {
    let context = GpuContext::open(&GpuRequest {
        backend: Some(backend),
        ..Default::default()
    })
    .unwrap_or_else(|error| panic!("no {backend:?} device bakes an environment: {error}"));
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let sampler = renderer.sampler(SamplerDesc {
        min_filter: Filter::Linear,
        mag_filter: Filter::Linear,
        mip_filter: Filter::Linear,
        address_u: AddressMode::Repeat,
        address_v: AddressMode::ClampToEdge,
    });
    let uniform = Panorama::new(WIDTH, HEIGHT, &vec![1.0; (WIDTH * HEIGHT * 3) as usize]);
    let texture = uniform.texture(&renderer, "uniform sky");
    let request = renderer.bake(&texture, &sampler);
    let held = renderer.wait_baked(request);
    assert_eq!(held.levels, LEVELS);
    assert_eq!(held.radiance.format(), TextureFormat::Rgba16Float);
    assert_eq!(held.radiance.mip_count(), LEVELS);
    assert_eq!(
        (held.radiance.width(), held.radiance.height()),
        (SPECULAR_WIDTH, SPECULAR_WIDTH / 2),
    );
    let irradiance = held.irradiance;
    assert!(
        (irradiance[0].x * 0.282_095 - PI).abs() < 1e-2,
        "a uniform sky of one bakes the ambient {irradiance:?}",
    );
    assert!((irradiance[0].y - irradiance[0].x).abs() < 1e-2);
    assert!((irradiance[0].z - irradiance[0].x).abs() < 1e-2);
    for coefficient in &irradiance[1..] {
        assert!(
            coefficient.length() < 0.1,
            "{backend:?} bakes the uniform lobe {coefficient:?}",
        );
    }

    let mut rgb = Vec::new();
    for y in 0..HEIGHT {
        let texel = if y < HEIGHT / 2 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        for _ in 0..WIDTH {
            rgb.extend_from_slice(&texel);
        }
    }
    let split = Panorama::new(WIDTH, HEIGHT, &rgb);
    let texture = split.texture(&renderer, "split sky");
    let request = renderer.bake(&texture, &sampler);
    let held = renderer.wait_baked(request);
    assert!(
        held.irradiance[1].x > 0.0 && held.irradiance[1].z < 0.0,
        "{backend:?} bakes a red northern and a blue southern hemisphere into {:?}",
        held.irradiance[1],
    );
}

#[test]
fn an_atmosphere_paints_a_blue_day_and_a_red_sunset() {
    for backend in dome_gpu_device::PREFERENCE {
        painted(*backend);
    }
}

fn painted(backend: Backend) {
    let context = GpuContext::open(&GpuRequest {
        backend: Some(backend),
        ..Default::default()
    })
    .unwrap_or_else(|error| panic!("no {backend:?} device paints an atmosphere: {error}"));
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let queue = context.queue();
    let day = dome_render::environment::Atmosphere::default();
    let held = renderer.atmosphere(&day);
    let radiance = texels(&held.radiance, queue);
    let background = texels(&held.background, queue);
    let zenith = sample(&radiance, 512, 16);
    assert!(
        zenith.z > zenith.y && zenith.y > zenith.x && zenith.z > 0.0,
        "{backend:?} paints a blue day zenith {zenith:?}",
    );
    let mut lit = 0.0f32;
    let mut disk = 0.0f32;
    for texel in &radiance.texels {
        lit = lit.max(texel.x.max(texel.y).max(texel.z));
    }
    for texel in &background.texels {
        disk = disk.max(texel.x.max(texel.y).max(texel.z));
    }
    assert!(
        lit < 10.0,
        "{backend:?} keeps the sun disk out of the indirect sky: {lit}",
    );
    assert!(
        disk > 1000.0,
        "{backend:?} paints the sun disk into the visible sky: {disk}",
    );

    let sunset = dome_render::environment::Atmosphere {
        sun_elevation: 1.0,
        ..dome_render::environment::Atmosphere::default()
    };
    let held = renderer.atmosphere(&sunset);
    let radiance = texels(&held.radiance, queue);
    let horizon = sample(&radiance, 512, 496);
    let zenith = sample(&radiance, 512, 16);
    assert!(
        horizon.x > horizon.z * 4.0,
        "{backend:?} reddens the horizon of a setting sun {horizon:?}",
    );
    assert!(
        zenith.z > zenith.x * 1.5,
        "{backend:?} leaves the zenith of a setting sun blue {zenith:?}",
    );
    assert!(
        radiance
            .texels
            .iter()
            .all(|held| held.x.is_finite() && held.y.is_finite() && held.z.is_finite()),
        "{backend:?} paints a finite atmosphere",
    );
}

struct Texels {
    width: u32,
    texels: Vec<Vec3>,
}

fn texels(texture: &Texture, queue: &Queue) -> Texels {
    let bytes = texture.read(queue);
    let mut texels = Vec::with_capacity((texture.width() * texture.height()) as usize);
    for row in texture.rows(&bytes) {
        for texel in row.as_chunks::<8>().0 {
            let channel = |at: usize| {
                f16::from_bits(u16::from_le_bytes([texel[at * 2], texel[at * 2 + 1]])).to_f32()
            };
            texels.push(Vec3::new(channel(0), channel(1), channel(2)));
        }
    }
    Texels {
        width: texture.width(),
        texels,
    }
}

fn sample(held: &Texels, x: u32, y: u32) -> Vec3 {
    held.texels[(y * held.width + x) as usize]
}
