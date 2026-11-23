use crate::abi::{TRANSPARENT_LAYERS, TransparentSetupRecord};
use crate::program;
use dome_gpu_device::{
    AddressMode, Binding, BufferBinding, BufferUsages, Device, Draw, Filter, GpuBuffer,
    GraphicsPipeline, Pass, PipelineState, Queue, Sampler, SamplerDesc, Submission, Texture,
    TextureFormat,
};
use std::mem::size_of;

const SETTINGS_BYTES: u64 = size_of::<TransparentSetupRecord>() as u64;
const WORD_BYTES: u64 = 4;
const RESOLVE_VERTICES: u32 = 3;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const LIT_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

struct Screen {
    keys: GpuBuffer,
    colors: GpuBuffer,
    dropped: GpuBuffer,
    settings: GpuBuffer,
    width: u32,
    height: u32,
    layers: u32,
}

pub(crate) struct Transparency {
    resolve: GraphicsPipeline,
    sampler: Sampler,
    screen: Option<Screen>,
}

impl Transparency {
    pub(crate) fn new(device: &Device) -> Self {
        Self {
            resolve: GraphicsPipeline::new(
                device,
                program::transparency_resolve(),
                LIT_FORMAT,
                PipelineState::SCREEN,
            ),
            sampler: Sampler::new(
                device,
                SamplerDesc {
                    min_filter: Filter::Nearest,
                    mag_filter: Filter::Nearest,
                    mip_filter: Filter::Nearest,
                    address_u: AddressMode::ClampToEdge,
                    address_v: AddressMode::ClampToEdge,
                },
            ),
            screen: None,
        }
    }

    pub(crate) fn ensure(
        &mut self,
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        vertical: f32,
    ) {
        if self
            .screen
            .as_ref()
            .is_some_and(|screen| screen.width == width && screen.height == height)
        {
            return;
        }
        let pixels = u64::from(width) * u64::from(height);
        let layer_bytes = pixels * u64::from(TRANSPARENT_LAYERS) * WORD_BYTES;
        assert!(
            layer_bytes <= device.limits().max_storage_buffer_binding_size,
            "a {width} by {height} frame slots {layer_bytes} transparent layers beyond the device storage binding",
        );
        let storage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let keys = GpuBuffer::new(device, "dome transparent keys", layer_bytes, storage);
        let colors = GpuBuffer::new(device, "dome transparent colors", layer_bytes, storage);
        let dropped = GpuBuffer::new(
            device,
            "dome transparent dropped",
            WORD_BYTES,
            storage | BufferUsages::COPY_SRC,
        );
        let settings = GpuBuffer::new(device, "dome transparent settings", SETTINGS_BYTES, storage);
        settings.write(
            queue,
            bytemuck::bytes_of(&TransparentSetupRecord {
                layers: TRANSPARENT_LAYERS,
                width,
                height,
                vertical,
            }),
        );
        self.screen = Some(Screen {
            keys,
            colors,
            dropped,
            settings,
            width,
            height,
            layers: TRANSPARENT_LAYERS,
        });
    }

    pub(crate) fn clear_layers(&self, submission: &mut Submission) {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its layers");
        let layers = u64::from(screen.width) * u64::from(screen.height) * u64::from(screen.layers);
        submission.clear(&screen.keys, 0, layers * WORD_BYTES);
        submission.clear(&screen.dropped, 0, WORD_BYTES);
    }

    pub(crate) fn clear_dropped(&self, submission: &mut Submission) {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its layers");
        submission.clear(&screen.dropped, 0, WORD_BYTES);
    }

    pub(crate) fn keys(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its layers");
        screen.keys.binding(0, screen.keys.size())
    }

    pub(crate) fn colors(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its layers");
        screen.colors.binding(0, screen.colors.size())
    }

    pub(crate) fn dropped(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its layers");
        screen.dropped.binding(0, WORD_BYTES)
    }

    pub(crate) fn dropped_buffer(&self) -> Option<&GpuBuffer> {
        self.screen.as_ref().map(|screen| &screen.dropped)
    }

    pub(crate) fn settings(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its layers");
        screen.settings.binding(0, SETTINGS_BYTES)
    }

    pub(crate) fn resolve(&self, submission: &mut Submission, source: &Texture, lit: &Texture) {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its layers");
        let frame = self.resolve.bind_group(
            0,
            &[
                Binding::buffer(
                    program::RESOLVE_SETTINGS,
                    screen.settings.binding(0, SETTINGS_BYTES),
                ),
                Binding::buffer(
                    program::RESOLVE_KEYS,
                    screen.keys.binding(0, screen.keys.size()),
                ),
                Binding::buffer(
                    program::RESOLVE_COLORS,
                    screen.colors.binding(0, screen.colors.size()),
                ),
            ],
        );
        let image = self.resolve.bind_group(
            1,
            &[
                Binding::texture(program::RESOLVE_SOURCE, source),
                Binding::sampler(program::RESOLVE_FILTER, &self.sampler),
            ],
        );
        submission.render(
            Pass::new(lit, CLEAR),
            &[Draw::new(&self.resolve, &[frame, image], RESOLVE_VERTICES)],
        );
    }
}

const _: () = assert!(
    SETTINGS_BYTES == 16,
    "the transparent settings pack the layers, viewport and vertical direction",
);
