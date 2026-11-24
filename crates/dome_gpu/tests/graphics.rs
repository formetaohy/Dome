use dome_gpu::{
    AddressMode, Binding, BindingSpec, BufferUsages, Draw, FilterMode, Gpu, GpuBuffer, GpuRequest,
    GraphicsPipeline, GraphicsShader, Pass, PipelineState, Sampler, SamplerDesc, SamplerTable,
    Submission, Texture, TextureFormat, TextureTable, TextureUsages,
};

const SHADER: &str = r#"
enable wgpu_binding_array;

@group(0) @binding(0) var<storage, read> vertices: array<f32>;
@group(1) @binding(0) var albedos: binding_array<texture_2d<f32>, TEXTURE_TABLE_EXTENT>;
@group(1) @binding(1) var palettes: binding_array<sampler, SAMPLER_TABLE_EXTENT>;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex: u32) -> Out {
    let base = vertex * 5u;
    let corner = vec4<f32>(vertices[base + 3u], vertices[base + 4u], 0.0, 0.0);
    let scaled = 2.0 * corner * 0.5;
    var out: Out;
    out.position = vec4<f32>(vertices[base], vertices[base + 1u], 0.0, 1.0);
    out.uv = vec2<f32>(scaled.x, scaled.y);
    return out;
}

@fragment
fn fragment(input: Out) -> @location(0) vec4<f32> {
    return textureSample(albedos[0u], palettes[0u], input.uv);
}
"#;

const QUAD: GraphicsShader = GraphicsShader {
    label: "quad",
    source: SHADER,
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, 0),
        BindingSpec::texture_table(1, 0),
        BindingSpec::sampler_table(1, 1),
    ],
    targets: 1,
};

const VERTICES: [f32; 30] = [
    -1.0, -1.0, 0.0, 0.0, 1.0, 1.0, -1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0, -1.0, -1.0, 0.0,
    0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0, -1.0, 1.0, 0.0, 0.0, 0.0,
];

const PALETTE: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255,
];

const SIDE: u32 = 64;

#[test]
fn a_sampled_texture_paints_a_quad() {
    let gpu = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device samples a texture: {error}"));
    for format in [
        TextureFormat::Rgba8Unorm,
        TextureFormat::Bgra8Unorm,
        TextureFormat::Rgba8UnormSrgb,
    ] {
        paint(&gpu, format);
    }
}

fn paint(gpu: &Gpu, format: TextureFormat) {
    let device = gpu.device();
    let queue = gpu.queue();
    let albedo = Texture::new(
        device,
        "albedo",
        2,
        2,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    albedo.write(&queue, &PALETTE);
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
    let pixels = render(gpu, &albedo, &palette, format);
    let left = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 4];
    let right = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize * 3 / 4];
    let red = match format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => [255, 0, 0, 255],
        TextureFormat::Bgra8Unorm => [0, 0, 255, 255],
        _ => unreachable!("a sampled quad writes an eight bit colour target"),
    };
    assert_eq!(left, red, "a {format:?} target samples the left texel");
    assert_eq!(
        right,
        [0, 255, 0, 255],
        "a {format:?} target samples the right texel"
    );
}

#[test]
fn a_mipped_texture_samples_the_level_its_footprint_names() {
    let gpu = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device samples a mip chain: {error}"));
    let device = gpu.device();
    let queue = gpu.queue();
    let texture = Texture::new(
        device,
        "mipped",
        256,
        256,
        2,
        TextureFormat::Rgba8Unorm,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    let mut red = vec![0u8; 256 * 256 * 4];
    for texel in red.as_chunks_mut::<4>().0 {
        texel.copy_from_slice(&[255, 0, 0, 255]);
    }
    let mut green = vec![0u8; 128 * 128 * 4];
    for texel in green.as_chunks_mut::<4>().0 {
        texel.copy_from_slice(&[0, 255, 0, 255]);
    }
    texture.write_levels(&queue, &[&red, &green]);
    let sampler = Sampler::new(
        device,
        SamplerDesc {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            compare: None,
        },
    );
    let pixels = render(&gpu, &texture, &sampler, TextureFormat::Rgba8Unorm);
    let center = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    assert!(
        center[1] > 200 && center[0] < 32 && center[2] < 32,
        "a minified quad samples the mip level {center:?}",
    );
}

fn render(gpu: &Gpu, albedo: &Texture, sampler: &Sampler, format: TextureFormat) -> Vec<[u8; 4]> {
    let device = gpu.device();
    let queue = gpu.queue();
    let target = Texture::new(
        device,
        "quad",
        SIDE,
        SIDE,
        1,
        format,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let pipeline = GraphicsPipeline::new(
        device,
        QUAD.specialized(*device.tables()),
        format,
        PipelineState::SCREEN,
    );
    let vertices = GpuBuffer::new(
        device,
        "quad vertices",
        (VERTICES.len() * 4) as u64,
        BufferUsages::STORAGE | BufferUsages::COPY_DST,
    );
    vertices.write(&queue, bytemuck::cast_slice(&VERTICES));
    let frame = pipeline.bind_group(
        0,
        &[Binding::buffer(0, vertices.binding(0, vertices.size()))],
    );
    let white = Texture::new(
        device,
        "white",
        1,
        1,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    white.write(&queue, &[255, 255, 255, 255]);
    let mut albedos = TextureTable::new(device, device.tables().textures, &white);
    albedos.add(albedo);
    let mut palettes = SamplerTable::new(device, device.tables().samplers, sampler);
    palettes.add(sampler);
    let material = pipeline.bind_group(
        1,
        &[
            Binding::texture_table(0, &albedos),
            Binding::sampler_table(1, &palettes),
        ],
    );
    let groups = [frame, material];
    let mut submission = Submission::new(device, "quad");
    submission.render(
        Pass::new(&target, [0.0, 0.0, 0.0, 1.0]),
        &[Draw::new(&pipeline, &groups, 6)],
    );
    let readback = GpuBuffer::new(
        device,
        "quad readback",
        target.readback_bytes(),
        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
    );
    submission.copy_texture(&target, &readback, 0);
    let index = submission.submit(&queue);
    let bytes = readback.read(&queue, index, target.readback_bytes());
    target
        .rows(&bytes)
        .flat_map(|row| row.as_chunks::<4>().0)
        .copied()
        .collect::<Vec<_>>()
}
