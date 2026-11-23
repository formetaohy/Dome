use crate::abi::UiScreenRecord;
use crate::element::Layer;
use crate::paint::{Instance, LayerBuffer};
use crate::shader;
use crate::texture::Textures;
use dome_gpu::{
    BindGroup, Binding, BufferUsages, ClipSpace, Device, Draw, GpuBuffer, GraphicsPipeline, Pass,
    PipelineState, Queue, Submission, Texture, TextureFormat, TextureUsages,
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
    generation: u64,
}

impl Slot {
    fn new(device: &Device, label: &str) -> Self {
        Self {
            buffer: storage(device, label, MINIMUM_INSTANCES),
            capacity: MINIMUM_INSTANCES,
            revision: None,
            generation: 0,
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
            self.generation = self.generation.wrapping_add(1);
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

struct Cached {
    key: u64,
    groups: [BindGroup; 2],
}

pub struct Renderer {
    device: Device,
    queue: Queue,
    format: TextureFormat,
    paint: GraphicsPipeline,
    screen: GpuBuffer,
    fallback: Texture,
    slots: [Slot; SLOTS],
    groups: [Option<Cached>; SLOTS],
    vertical: f32,
    written: Option<(u32, u32, u32)>,
}

impl Renderer {
    pub fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self {
        assert!(
            !format.is_srgb(),
            "a user interface paints display space bytes into a linear target"
        );
        let vertical = match device.clip_space() {
            ClipSpace::YUp => -0.5,
            ClipSpace::YDown => 0.5,
        };
        let fallback = Texture::new(
            device,
            "dome ui fallback",
            1,
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        fallback.write(queue, &FALLBACK);
        Self {
            device: device.clone(),
            queue: queue.clone(),
            format,
            paint: GraphicsPipeline::new(device, shader::program(), format, PipelineState::OVERLAY),
            screen: GpuBuffer::new(
                device,
                "dome ui screen",
                SCREEN_BYTES,
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            fallback,
            slots: std::array::from_fn(|index| Slot::new(device, labels()[index])),
            groups: std::array::from_fn(|_| None),
            vertical,
            written: None,
        }
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
        for (index, layer) in layers.iter().enumerate() {
            let key = group_key(&self.slots[index], layer.revision, textures);
            let stale = self.groups[index]
                .as_ref()
                .is_none_or(|cached| cached.key != key);
            if stale {
                let groups = self.groups_of(&self.slots[index], textures);
                self.groups[index] = Some(Cached { key, groups });
            }
        }
        let mut draws = Vec::with_capacity(SLOTS);
        for (index, layer) in layers.iter().enumerate() {
            if layer.material.is_empty() {
                continue;
            }
            let cached = self.groups[index]
                .as_ref()
                .expect("a painted layer holds its groups");
            draws.push(
                Draw::new(&self.paint, &cached.groups, VERTICES)
                    .instances(layer.material.len() as u32),
            );
        }
        submission.render(Pass::new(target, CLEAR), &draws);
    }

    fn groups_of(&self, slot: &Slot, textures: &Textures) -> [BindGroup; 2] {
        let settings = self.paint.bind_group(
            0,
            &[
                Binding::buffer(0, self.screen.binding(0, SCREEN_BYTES)),
                Binding::buffer(
                    1,
                    slot.buffer
                        .binding(0, slot.capacity as u64 * size_of::<Instance>() as u64),
                ),
            ],
        );
        let resources = self.paint.bind_group(
            1,
            &[
                Binding::texture_table(0, textures.table()),
                Binding::sampler_table(1, textures.samplers()),
            ],
        );
        [settings, resources]
    }
}

fn group_key(slot: &Slot, revision: u64, textures: &Textures) -> u64 {
    let mut key = 0xcbf2_9ce4_8422_2325u64;
    for value in [
        slot.generation,
        slot.capacity as u64,
        revision,
        textures.generation(),
    ] {
        key = (key ^ value).wrapping_mul(0x0000_0100_0000_01b3);
    }
    key
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
