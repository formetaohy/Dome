use dome_gpu::{
    AddressMode, Device, FilterMode, Gpu, GpuRequest, Sampler, SamplerDesc, SamplerTable, Texture,
    TextureFormat, TextureTable, TextureUsages,
};

#[test]
fn a_reset_table_hands_back_every_slot_it_held() {
    let gpu = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device resets a table: {error}"));
    reset(gpu.device());
}

fn reset(device: &Device) {
    let usages = TextureUsages::TEXTURE_BINDING;
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
        "a table fills the first texture slot"
    );
    assert_eq!(
        textures.add(&green),
        1,
        "a table fills the second texture slot"
    );
    textures.reset(&fallback);
    assert_eq!(
        textures.filled(),
        0,
        "a table resets every texture slot it filled"
    );
    assert_eq!(
        textures.position(&fallback),
        None,
        "a table resets the ownership of every texture slot"
    );
    assert_eq!(
        textures.add(&red),
        0,
        "a table hands back the first texture slot after a reset"
    );
    assert_eq!(
        textures.position(&fallback),
        None,
        "a table fills the texture slot it reset instead of holding its fallback"
    );

    let nearest = SamplerDesc {
        min_filter: FilterMode::Nearest,
        mag_filter: FilterMode::Nearest,
        mipmap_filter: FilterMode::Nearest,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        compare: None,
    };
    let linear = SamplerDesc {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        ..nearest
    };
    let fallback = Sampler::new(device, nearest);
    let filtered = Sampler::new(device, linear);
    let mut samplers = SamplerTable::new(device, 2, &fallback);
    assert_eq!(
        samplers.add(&filtered),
        0,
        "a table fills the first sampler slot"
    );
    assert_eq!(
        samplers.add(&filtered),
        1,
        "a table fills the second sampler slot"
    );
    samplers.reset(&fallback);
    assert_eq!(
        samplers.filled(),
        0,
        "a table resets every sampler slot it filled"
    );
    assert_eq!(
        samplers.position(nearest),
        None,
        "a table resets the ownership of every sampler slot"
    );
    assert_eq!(
        samplers.add(&filtered),
        0,
        "a table hands back the first sampler slot after a reset"
    );
    assert_eq!(
        samplers.position(nearest),
        None,
        "a table fills the sampler slot it reset instead of holding its fallback"
    );
}
