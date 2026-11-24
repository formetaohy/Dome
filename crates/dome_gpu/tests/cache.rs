use dome_gpu::{
    AddressMode, Binding, BindingSpec, FilterMode, Gpu, GpuRequest, GraphicsPipeline,
    GraphicsShader, Pass, PipelineState, Sampler, SamplerDesc, Submission, Texture, TextureFormat,
    TextureUsages,
};

const SHADER: GraphicsShader = GraphicsShader {
    label: "cached image",
    source: r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_filter: sampler;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let at = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(at * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return textureSample(image, image_filter, at.xy / 64.0);
}
"#,
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::sampled_texture(0, 0),
        BindingSpec::filtering_sampler(0, 1),
    ],
    targets: 1,
};

#[test]
fn a_dropped_image_takes_the_bind_group_it_was_cached_in() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device caches bind groups: {error}"));
    let device = context.device();
    let queue = context.queue();
    let format = TextureFormat::Rgba8Unorm;
    let pipeline = GraphicsPipeline::new(device, &SHADER, format, PipelineState::SCREEN);
    let filter = Sampler::new(
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
    let target = Texture::new(
        device,
        "cached target",
        64,
        64,
        1,
        format,
        TextureUsages::RENDER_ATTACHMENT,
    );
    let before = device.cached_bind_groups();
    let mut textures = Vec::new();
    for index in 0..4u8 {
        let texture = Texture::new(
            device,
            "cached image",
            64,
            64,
            1,
            format,
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        );
        texture.write(&queue, &[index, index, index, 255].repeat(64 * 64));
        let group = pipeline.bind_group(
            0,
            &[Binding::texture(0, &texture), Binding::sampler(1, &filter)],
        );
        let mut submission = Submission::new(device, "cached image");
        submission.render(
            Pass::new(&target, [0.0, 0.0, 0.0, 1.0]),
            &[dome_gpu::Draw::new(&pipeline, &[group], 3)],
        );
        submission.submit(&queue);
        textures.push(texture);
        assert_eq!(
            device.cached_bind_groups(),
            before + 1,
            "a cached bind group holds the image it binds",
        );
        textures.clear();
    }
    Submission::new(device, "cached sweep").submit(&queue);
    assert_eq!(
        device.cached_bind_groups(),
        before,
        "a dropped image takes the bind group it was cached in",
    );
}
