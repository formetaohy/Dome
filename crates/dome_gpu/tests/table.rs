use dome_gpu::{
    AddressMode, Backend, Device, Filter, GpuContext, GpuRequest, Sampler, SamplerDesc,
    SamplerTable, Texture, TextureFormat, TextureTable, TextureUsages,
};

#[test]
fn a_reset_table_hands_back_every_slot_it_held() {
    for backend in dome_gpu::PREFERENCE {
        let context = GpuContext::open(&GpuRequest {
            backend: Some(*backend),
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("no {backend:?} device resets a table: {error}"));
        reset(context.device(), *backend);
    }
}

fn reset(device: &Device, backend: Backend) {
    let usages = TextureUsages::SAMPLED;
    let fallback = Texture::new(
        device,
        "fallback",
        1,
        1,
        1,
        TextureFormat::Rgba8Unorm,
        usages,
    );
    let red = Texture::new(device, "red", 1, 1, 1, TextureFormat::Rgba8Unorm, usages);
    let green = Texture::new(device, "green", 1, 1, 1, TextureFormat::Rgba8Unorm, usages);
    let mut textures = TextureTable::new(device, 2, &fallback);
    assert_eq!(
        textures.add(&red),
        0,
        "{backend:?} fills the first texture slot",
    );
    assert_eq!(
        textures.add(&green),
        1,
        "{backend:?} fills the second texture slot",
    );
    textures.reset(&fallback);
    assert_eq!(
        textures.filled(),
        0,
        "{backend:?} resets every texture slot it filled",
    );
    assert_eq!(
        textures.position(&fallback),
        None,
        "{backend:?} resets the ownership of every texture slot",
    );
    assert_eq!(
        textures.add(&red),
        0,
        "{backend:?} hands back the first texture slot after a reset",
    );
    assert_eq!(
        textures.position(&fallback),
        None,
        "{backend:?} fills the texture slot it reset instead of holding its fallback",
    );

    let nearest = SamplerDesc {
        min_filter: Filter::Nearest,
        mag_filter: Filter::Nearest,
        mip_filter: Filter::Nearest,
        address_u: AddressMode::ClampToEdge,
        address_v: AddressMode::ClampToEdge,
        compare: None,
    };
    let linear = SamplerDesc {
        min_filter: Filter::Linear,
        mag_filter: Filter::Linear,
        mip_filter: Filter::Linear,
        ..nearest
    };
    let fallback = Sampler::new(device, nearest);
    let filtered = Sampler::new(device, linear);
    let mut samplers = SamplerTable::new(device, 2, &fallback);
    assert_eq!(
        samplers.add(&filtered),
        0,
        "{backend:?} fills the first sampler slot",
    );
    assert_eq!(
        samplers.add(&filtered),
        1,
        "{backend:?} fills the second sampler slot",
    );
    samplers.reset(&fallback);
    assert_eq!(
        samplers.filled(),
        0,
        "{backend:?} resets every sampler slot it filled",
    );
    assert_eq!(
        samplers.position(nearest),
        None,
        "{backend:?} resets the ownership of every sampler slot",
    );
    assert_eq!(
        samplers.add(&filtered),
        0,
        "{backend:?} hands back the first sampler slot after a reset",
    );
    assert_eq!(
        samplers.position(nearest),
        None,
        "{backend:?} fills the sampler slot it reset instead of holding its fallback",
    );
}
