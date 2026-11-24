use crate::abi::UiScreenRecord;
use crate::element::Layer;
use crate::paint::{Instance, LayerBuffer};
use crate::shader;
use crate::texture::Textures;
use dome_gpu::{
    Binding, BufferUsages, Device, Draw, GpuBuffer, GraphicsPipeline, Pass, PipelineState, Queue,
    Submission, Texture, TextureFormat, TextureUsages,
};
use std::mem::size_of;

pub(crate) const SLOTS: usize = Layer::ALL.len() + 1;
const SCREEN_BYTES: u64 = size_of::<UiScreenRecord>() as u64;
const MINIMUM_INSTANCES: usize = 512;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
const VERTICES: u32 = 6;
const FALLBACK: [u8; 4] = [0, 0, 0, 255];

struct Slot {
    buffer: GpuBuffer,
    capacity: usize,
    revision: Option<u64>,
}

impl Slot {
    fn new(device: &Device, label: &str) -> Self {
        Self {
            buffer: storage(device, label, MINIMUM_INSTANCES),
            capacity: MINIMUM_INSTANCES,
            revision: None,
        }
    }

    fn upload(&mut self, device: &Device, queue: &Queue, layer: &LayerBuffer) {
        if Some(layer.revision) == self.revision {
            return;
        }
        let count = layer.material.len();
        self.revision = Some(layer.revision);
        if count == 0 {
            return;
        }
        let mut grown = false;
        if self.capacity < count {
            self.capacity = count.next_power_of_two().max(MINIMUM_INSTANCES);
            self.buffer = storage(device, "dome ui instances", self.capacity);
            grown = true;
        }
        match layer.dirty {
            Some((start, end)) if !grown && end > start && end - start < count => {
                self.buffer.write_at(
                    queue,
                    (start * size_of::<Instance>()) as u64,
                    bytemuck::cast_slice(&layer.material[start..end]),
                );
            }
            _ => self
                .buffer
                .write(queue, bytemuck::cast_slice(&layer.material)),
        }
    }
}

pub struct Renderer {
    device: Device,
    queue: Queue,
    format: TextureFormat,
    paint: GraphicsPipeline,
    screen: GpuBuffer,
    fallback: Texture,
    slots: [Slot; SLOTS],
    vertical: f32,
    written: Option<(u32, u32, u32)>,
}

impl Renderer {
    pub fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self {
        assert!(
            !format.is_srgb(),
            "a user interface paints display space bytes into a linear target"
        );
        let vertical = -0.5;
        let fallback = Texture::new(
            device,
            "dome ui fallback",
            1,
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        );
        fallback.write(queue, &FALLBACK);
        Self {
            device: device.clone(),
            queue: queue.clone(),
            format,
            paint: GraphicsPipeline::new(
                device,
                shader::program(*device.tables()),
                format,
                PipelineState::OVERLAY,
            ),
            screen: GpuBuffer::new(
                device,
                "dome ui screen",
                SCREEN_BYTES,
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            fallback,
            slots: std::array::from_fn(|index| Slot::new(device, labels()[index])),
            vertical,
            written: None,
        }
    }

    pub fn format(&self) -> TextureFormat {
        self.format
    }

    pub(crate) fn paint(
        &mut self,
        submission: &mut Submission,
        target: &Texture,
        scene: Option<&Texture>,
        scale: f32,
        textures: &mut Textures,
        layers: &[&LayerBuffer; SLOTS],
    ) {
        assert_eq!(
            target.format(),
            self.format,
            "a user interface target keeps its pipeline format"
        );
        let width = target.width();
        let height = target.height();
        let written = (width, height, scale.to_bits());
        if self.written != Some(written) {
            self.screen.write(
                &self.queue,
                bytemuck::bytes_of(&UiScreenRecord {
                    width: width as f32,
                    height: height as f32,
                    scale,
                    vertical: self.vertical,
                }),
            );
            self.written = Some(written);
        }
        for (index, layer) in layers.iter().enumerate() {
            self.slots[index].upload(&self.device, &self.queue, layer);
        }
        textures.set_scene(scene.unwrap_or(&self.fallback));
        let mut cached = Vec::with_capacity(SLOTS);
        for slot in &self.slots {
            cached.push(groups_of(&self.paint, &self.screen, slot, textures));
        }
        let mut draws = Vec::with_capacity(SLOTS);
        for (index, layer) in layers.iter().enumerate() {
            if layer.material.is_empty() {
                continue;
            }
            draws.push(
                Draw::new(&self.paint, &cached[index], VERTICES)
                    .instances(layer.material.len() as u32),
            );
        }
        submission.render(Pass::new(target, CLEAR), &draws);
    }
}

fn groups_of(
    paint: &GraphicsPipeline,
    screen: &GpuBuffer,
    slot: &Slot,
    textures: &Textures,
) -> [dome_gpu::BindGroup; 2] {
    let settings = paint.bind_group(
        0,
        &[
            Binding::buffer(0, screen.binding(0, SCREEN_BYTES)),
            Binding::buffer(
                1,
                slot.buffer
                    .binding(0, slot.capacity as u64 * size_of::<Instance>() as u64),
            ),
        ],
    );
    let resources = paint.bind_group(
        1,
        &[
            Binding::texture_table(0, textures.table()),
            Binding::sampler_table(1, textures.samplers()),
        ],
    );
    [settings, resources]
}

fn storage(device: &Device, label: &str, capacity: usize) -> GpuBuffer {
    GpuBuffer::new(
        device,
        label,
        (capacity * size_of::<Instance>()) as u64,
        BufferUsages::STORAGE | BufferUsages::COPY_DST,
    )
}

fn labels() -> [&'static str; SLOTS] {
    [
        "dome ui instances content",
        "dome ui instances chrome",
        "dome ui instances popup",
        "dome ui instances hud",
        "dome ui instances overlay",
    ]
}
