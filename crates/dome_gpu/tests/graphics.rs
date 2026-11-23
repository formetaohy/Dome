use dome_gpu::{
    AddressMode, Backend, Binding, BufferUsages, Draw, Filter, GpuBuffer, GpuContext, GpuRequest,
    GraphicsPipeline, Pass, PipelineState, Sampler, SamplerDesc, SamplerTable, Submission, Texture,
    TextureFormat, TextureTable, TextureUsages,
};
use dome_shader::{Fvec2, Fvec4};

#[dome_shader::graphics]
mod quad {
    #[vertex]
    fn vertex(vertex: u32, vertices: Read<f32>, position: Out<Position>, uv: Out<Fvec2>) {
        let base = vertex * 5u32;
        let corner = fvec4(vertices[base + 3u32], vertices[base + 4u32], 0.0, 0.0);
        let scaled = 2.0 * corner * 0.5;
        position = fvec4(vertices[base], vertices[base + 1u32], 0.0, 1.0);
        uv = fvec2(scaled.x, scaled.y);
    }

    #[fragment]
    fn fragment(
        albedos: TextureTable,
        palettes: SamplerTable,
        sampled: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        target = shade(albedos[0u32], palettes[0u32], sampled);
    }

    fn shade(albedo: Texture2D, palette: Sampler, coordinates: Fvec2) -> Fvec4 {
        sample(albedo, palette, coordinates)
    }
}

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
    for backend in dome_gpu::PREFERENCE {
        let context = GpuContext::open(&GpuRequest {
            backend: Some(*backend),
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("no {backend:?} device samples a texture: {error}"));
        for format in [
            TextureFormat::Rgba8Unorm,
            TextureFormat::Bgra8Unorm,
            TextureFormat::Rgba8UnormSrgb,
        ] {
            paint(&context, *backend, format);
        }
    }
}

fn paint(context: &GpuContext, backend: Backend, format: TextureFormat) {
    let device = context.device();
    let queue = context.queue();
    let albedo = Texture::new(
        device,
        "albedo",
        2,
        2,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::SAMPLED | TextureUsages::COPY_DST,
    );
    albedo.write(queue, &PALETTE);
    let palette = Sampler::new(
        device,
        SamplerDesc {
            min_filter: Filter::Nearest,
            mag_filter: Filter::Nearest,
            mip_filter: Filter::Nearest,
            address_u: AddressMode::ClampToEdge,
            address_v: AddressMode::ClampToEdge,
        },
    );
    let pixels = render(context, &albedo, &palette, format);
    let left = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 4];
    let right = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize * 3 / 4];
    let red = match format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => [255, 0, 0, 255],
        TextureFormat::Bgra8Unorm => [0, 0, 255, 255],
        TextureFormat::Rgba16Float | TextureFormat::R32Float => {
            unreachable!("a sampled quad writes an eight bit colour target")
        }
    };
    assert_eq!(
        left, red,
        "{backend:?} samples the left texels of a {format:?} texture"
    );
    assert_eq!(
        right,
        [0, 255, 0, 255],
        "{backend:?} samples the right texels of a {format:?} texture"
    );
}

#[test]
fn a_mipped_texture_samples_the_level_its_footprint_names() {
    for backend in dome_gpu::PREFERENCE {
        mipped(*backend);
    }
}

fn mipped(backend: Backend) {
    let context = GpuContext::open(&GpuRequest {
        backend: Some(backend),
        ..Default::default()
    })
    .unwrap_or_else(|error| panic!("no {backend:?} device samples a mip chain: {error}"));
    let device = context.device();
    let texture = Texture::new(
        device,
        "mipped",
        256,
        256,
        2,
        TextureFormat::Rgba8Unorm,
        TextureUsages::SAMPLED | TextureUsages::COPY_DST,
    );
    let mut red = vec![0u8; 256 * 256 * 4];
    for texel in red.as_chunks_mut::<4>().0 {
        texel.copy_from_slice(&[255, 0, 0, 255]);
    }
    let mut green = vec![0u8; 128 * 128 * 4];
    for texel in green.as_chunks_mut::<4>().0 {
        texel.copy_from_slice(&[0, 255, 0, 255]);
    }
    texture.write_levels(context.queue(), &[&red, &green]);
    let sampler = Sampler::new(
        device,
        SamplerDesc {
            min_filter: Filter::Linear,
            mag_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_u: AddressMode::ClampToEdge,
            address_v: AddressMode::ClampToEdge,
        },
    );
    let pixels = render(&context, &texture, &sampler, TextureFormat::Rgba8Unorm);
    let center = pixels[(SIDE as usize / 2) * SIDE as usize + SIDE as usize / 2];
    assert!(
        center[1] > 200 && center[0] < 32 && center[2] < 32,
        "{backend:?} samples the minified mip level {center:?}",
    );
}

fn render(
    context: &GpuContext,
    albedo: &Texture,
    sampler: &Sampler,
    format: TextureFormat,
) -> Vec<[u8; 4]> {
    let device = context.device();
    let queue = context.queue();
    let target = Texture::new(
        device,
        "quad",
        SIDE,
        SIDE,
        1,
        format,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let pipeline = GraphicsPipeline::new(device, quad(), format, PipelineState::SCREEN);
    let vertices = GpuBuffer::new(
        device,
        "quad vertices",
        (VERTICES.len() * 4) as u64,
        BufferUsages::STORAGE | BufferUsages::COPY_DST,
    );
    vertices.write(queue, bytemuck::cast_slice(&VERTICES));
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
        TextureUsages::SAMPLED | TextureUsages::COPY_DST,
    );
    white.write(queue, &[255, 255, 255, 255]);
    let mut albedos = TextureTable::new(device, dome_shader::TEXTURE_TABLE_CAPACITY, &white);
    albedos.add(albedo);
    let mut palettes = SamplerTable::new(device, dome_shader::SAMPLER_TABLE_CAPACITY, sampler);
    palettes.add(sampler);
    let material = pipeline.bind_group(
        1,
        &[
            Binding::texture_table(0, &albedos),
            Binding::sampler_table(1, &palettes),
        ],
    );
    let mut submission = Submission::new(device, "quad");
    submission.render(
        Pass::new(&target, [0.0, 0.0, 0.0, 1.0]),
        &[Draw::new(&pipeline, &[frame, material], 6)],
    );
    let readback = GpuBuffer::new(
        device,
        "quad readback",
        target.readback_bytes(),
        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
    );
    submission.copy_texture(&target, &readback, 0);
    let index = submission.submit(queue);
    let bytes = readback.read(queue, index, target.readback_bytes());
    target
        .rows(&bytes)
        .flat_map(|row| row.as_chunks::<4>().0)
        .copied()
        .collect::<Vec<_>>()
}
