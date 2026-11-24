use crate::abi::{BloomRecord, BloomUpRecord, TonemapRecord};
use crate::program;
use dome_gpu::{
    AddressMode, Binding, BufferUsages, Device, Draw, FilterMode, GpuBuffer, GraphicsPipeline,
    Pass, PipelineState, Queue, Sampler, SamplerDesc, Submission, Texture, TextureFormat,
    TextureUsages,
};
use std::mem::size_of;

const BLOOM_LEVELS: usize = 5;
const BLOOM_THRESHOLD: f32 = 1.0;
const BLOOM_KNEE: f32 = 0.5;
const BLOOM_INTENSITY: f32 = 0.08;
const FULLSCREEN_VERTICES: u32 = 3;
const HDR_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
const BLOOM_BYTES: u64 = size_of::<BloomRecord>() as u64;
const BLOOM_UP_BYTES: u64 = size_of::<BloomUpRecord>() as u64;
const TONEMAP_BYTES: u64 = size_of::<TonemapRecord>() as u64;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

const _: () = assert!(
    BLOOM_LEVELS >= 2,
    "a bloom chain upsamples the levels it downsamples",
);

struct Settings {
    buffer: GpuBuffer,
    extract: u64,
    down: Vec<u64>,
    up: Vec<u64>,
    tonemap: u64,
}

struct Screen {
    hdr: Texture,
    lit: Texture,
    down: Vec<Texture>,
    up: Vec<Texture>,
    settings: Settings,
}

struct Layout {
    bytes: Vec<u8>,
    alignment: u64,
}

impl Layout {
    fn push(&mut self, record: &[u8]) -> u64 {
        let offset = (self.bytes.len() as u64).next_multiple_of(self.alignment);
        self.bytes.resize(offset as usize, 0);
        self.bytes.extend_from_slice(record);
        offset
    }
}

pub(crate) struct Post {
    extract: GraphicsPipeline,
    down: GraphicsPipeline,
    up: GraphicsPipeline,
    tonemap: GraphicsPipeline,
    filter: Sampler,
    vertical: f32,
    screen: Option<Screen>,
}

impl Post {
    pub(crate) fn new(device: &Device, format: TextureFormat) -> Self {
        Self {
            extract: GraphicsPipeline::new(
                device,
                program::bloom_extract(),
                HDR_FORMAT,
                PipelineState::SCREEN,
            ),
            down: GraphicsPipeline::new(
                device,
                program::bloom_down(),
                HDR_FORMAT,
                PipelineState::SCREEN,
            ),
            up: GraphicsPipeline::new(
                device,
                program::bloom_up(),
                HDR_FORMAT,
                PipelineState::SCREEN,
            ),
            tonemap: GraphicsPipeline::new(
                device,
                program::tonemap(),
                format,
                PipelineState::SCREEN,
            ),
            filter: Sampler::new(
                device,
                SamplerDesc {
                    min_filter: FilterMode::Linear,
                    mag_filter: FilterMode::Linear,
                    mipmap_filter: FilterMode::Nearest,
                    address_mode_u: AddressMode::ClampToEdge,
                    address_mode_v: AddressMode::ClampToEdge,
                    compare: None,
                },
            ),
            vertical: crate::frame::vertical(),
            screen: None,
        }
    }

    pub(crate) fn ensure(&mut self, device: &Device, queue: &Queue, width: u32, height: u32) {
        if self
            .screen
            .as_ref()
            .is_some_and(|screen| screen.hdr.width() == width && screen.hdr.height() == height)
        {
            return;
        }
        let usage = TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING;
        let hdr = Texture::new(device, "dome hdr", width, height, 1, HDR_FORMAT, usage);
        let lit = Texture::new(device, "dome lit", width, height, 1, HDR_FORMAT, usage);
        let dimensions = dimensions_of(width, height);
        let levels = dimensions
            .iter()
            .map(|(level_width, level_height)| {
                Texture::new(
                    device,
                    "dome bloom",
                    *level_width,
                    *level_height,
                    1,
                    HDR_FORMAT,
                    usage,
                )
            })
            .collect::<Vec<_>>();
        let ups = dimensions[..dimensions.len() - 1]
            .iter()
            .map(|(level_width, level_height)| {
                Texture::new(
                    device,
                    "dome bloom up",
                    *level_width,
                    *level_height,
                    1,
                    HDR_FORMAT,
                    usage,
                )
            })
            .collect::<Vec<_>>();
        let mut layout = Layout {
            bytes: Vec::new(),
            alignment: device.binding_alignment().max(4),
        };
        let extract = layout.push(bytemuck::bytes_of(&BloomRecord {
            source_width: width as f32,
            source_height: height as f32,
            vertical: self.vertical,
            threshold: BLOOM_THRESHOLD,
            knee: BLOOM_KNEE,
        }));
        let down = (1..levels.len())
            .map(|index| {
                let source = &levels[index - 1];
                layout.push(bytemuck::bytes_of(&BloomRecord {
                    source_width: source.width() as f32,
                    source_height: source.height() as f32,
                    vertical: self.vertical,
                    threshold: 0.0,
                    knee: 0.0,
                }))
            })
            .collect();
        let up = (0..levels.len() - 1)
            .map(|index| {
                let source = &levels[index + 1];
                layout.push(bytemuck::bytes_of(&BloomUpRecord {
                    source_width: source.width() as f32,
                    source_height: source.height() as f32,
                    vertical: self.vertical,
                }))
            })
            .collect();
        let tonemap = layout.push(bytemuck::bytes_of(&TonemapRecord {
            vertical: self.vertical,
            intensity: BLOOM_INTENSITY,
            source_width: width as f32,
            source_height: height as f32,
        }));
        let buffer = GpuBuffer::new(
            device,
            "dome post settings",
            layout.bytes.len() as u64,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        buffer.write(queue, &layout.bytes);
        self.screen = Some(Screen {
            hdr,
            lit,
            down: levels,
            up: ups,
            settings: Settings {
                buffer,
                extract,
                down,
                up,
                tonemap,
            },
        });
    }

    pub(crate) fn hdr(&self) -> &Texture {
        &self
            .screen
            .as_ref()
            .expect("a post pipeline holds the frame it renders")
            .hdr
    }

    pub(crate) fn lit(&self) -> &Texture {
        &self
            .screen
            .as_ref()
            .expect("a post pipeline holds the frame it renders")
            .lit
    }

    pub(crate) fn resolve(&self, submission: &mut Submission, source: &Texture) {
        let screen = self
            .screen
            .as_ref()
            .expect("a post pipeline holds the frame it renders");
        let settings = &screen.settings;
        let used = |offset: u64, bytes: u64| settings.buffer.binding(offset, bytes);
        let extract = self.extract.bind_group(
            0,
            &[Binding::buffer(
                program::BLOOM_SETTINGS,
                used(settings.extract, BLOOM_BYTES),
            )],
        );
        let source = self.extract.bind_group(
            1,
            &[
                Binding::texture(program::EXTRACT_SOURCE, source),
                Binding::sampler(program::EXTRACT_FILTER, &self.filter),
            ],
        );
        submission.render(
            Pass::new(&screen.down[0], CLEAR),
            &[Draw::new(
                &self.extract,
                &[extract, source],
                FULLSCREEN_VERTICES,
            )],
        );
        for index in 1..screen.down.len() {
            let frame = self.down.bind_group(
                0,
                &[Binding::buffer(
                    program::BLOOM_SETTINGS,
                    used(settings.down[index - 1], BLOOM_BYTES),
                )],
            );
            let source = self.down.bind_group(
                1,
                &[
                    Binding::texture(program::DOWN_SOURCE, &screen.down[index - 1]),
                    Binding::sampler(program::DOWN_FILTER, &self.filter),
                ],
            );
            submission.render(
                Pass::new(&screen.down[index], CLEAR),
                &[Draw::new(&self.down, &[frame, source], FULLSCREEN_VERTICES)],
            );
        }
        for index in (0..screen.up.len()).rev() {
            let low = screen
                .up
                .get(index + 1)
                .unwrap_or(&screen.down[screen.down.len() - 1]);
            let frame = self.up.bind_group(
                0,
                &[Binding::buffer(
                    program::BLOOM_SETTINGS,
                    used(settings.up[index], BLOOM_UP_BYTES),
                )],
            );
            let source = self.up.bind_group(
                1,
                &[
                    Binding::texture(program::UP_LOW, low),
                    Binding::texture(program::UP_HIGH, &screen.down[index]),
                    Binding::sampler(program::UP_FILTER, &self.filter),
                ],
            );
            submission.render(
                Pass::new(&screen.up[index], CLEAR),
                &[Draw::new(&self.up, &[frame, source], FULLSCREEN_VERTICES)],
            );
        }
    }

    pub(crate) fn tonemap(
        &self,
        submission: &mut Submission,
        target: &Texture,
        source: &Texture,
        cover: &Texture,
    ) {
        let screen = self
            .screen
            .as_ref()
            .expect("a post pipeline holds the frame it renders");
        let settings = &screen.settings;
        let frame = self.tonemap.bind_group(
            0,
            &[Binding::buffer(
                program::TONEMAP_SETTINGS,
                settings.buffer.binding(settings.tonemap, TONEMAP_BYTES),
            )],
        );
        let source = self.tonemap.bind_group(
            1,
            &[
                Binding::texture(program::TONEMAP_LIT, source),
                Binding::texture(program::TONEMAP_BLOOM, &screen.up[0]),
                Binding::sampler(program::TONEMAP_FILTER, &self.filter),
                Binding::texture(program::TONEMAP_COVER, cover),
            ],
        );
        submission.render(
            Pass::new(target, CLEAR),
            &[Draw::new(
                &self.tonemap,
                &[frame, source],
                FULLSCREEN_VERTICES,
            )],
        );
    }
}

fn dimensions_of(width: u32, height: u32) -> Vec<(u32, u32)> {
    let mut dimensions = Vec::with_capacity(BLOOM_LEVELS);
    let mut current = (width, height);
    for _ in 0..BLOOM_LEVELS {
        current = ((current.0 / 2).max(1), (current.1 / 2).max(1));
        dimensions.push(current);
    }
    dimensions
}

const _: () = assert!(
    BLOOM_BYTES == size_of::<BloomRecord>() as u64,
    "the bloom record keeps the layout its shader reads",
);
const _: () = assert!(
    BLOOM_UP_BYTES == size_of::<BloomUpRecord>() as u64,
    "the bloom up record keeps the layout its shader reads",
);
const _: () = assert!(
    TONEMAP_BYTES == size_of::<TonemapRecord>() as u64,
    "the tonemap record keeps the layout its shader reads",
);
