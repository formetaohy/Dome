use crate::abi::{MOTION, MotionRecord, TEMPORAL, TemporalRecord};
use crate::program;
use dome_gpu::{
    AddressMode, Binding, BufferBinding, BufferUsages, Device, Draw, Filter, GpuBuffer,
    GraphicsPipeline, Pass, PipelineState, Queue, Sampler, SamplerDesc, Submission, Texture,
    TextureFormat, TextureUsages,
};
use dome_math::{Mat4, Vec2, Vec3};
use dome_shader::Fvec4;
use std::mem::size_of;

const FULLSCREEN_VERTICES: u32 = 3;
const FORMAT: TextureFormat = TextureFormat::Rgba16Float;
const MOTION_BYTES: u64 = size_of::<MotionRecord>() as u64;
const TEMPORAL_BYTES: u64 = size_of::<TemporalRecord>() as u64;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
const SAMPLE_COUNT: u32 = 8;
const HISTORY_WEIGHT: f32 = 0.9;
const DEPTH_TOLERANCE: f32 = 0.02;

#[derive(Clone, Copy)]
pub(crate) struct Pose {
    pub(crate) clip: Mat4,
    pub(crate) eye: Vec3,
    pub(crate) forward: Vec3,
}

pub(crate) struct Frame<'a> {
    pub(crate) source: &'a Texture,
    pub(crate) pose: Pose,
    pub(crate) near: f32,
    pub(crate) far: f32,
    pub(crate) vertical: f32,
}

struct Screen {
    history: [Texture; 2],
    motion: [Texture; 2],
    history_at: usize,
    motion_at: usize,
    width: u32,
    height: u32,
    weight: f32,
}

pub(crate) struct Temporality {
    motion_pipeline: GraphicsPipeline,
    motion_blended_pipeline: GraphicsPipeline,
    temporal_pipeline: GraphicsPipeline,
    filter: Sampler,
    nearest: Sampler,
    motion_record: GpuBuffer,
    temporal_record: GpuBuffer,
    screen: Option<Screen>,
    frame: u32,
    previous: Option<Pose>,
}

impl Temporality {
    pub(crate) fn new(device: &Device) -> Self {
        Self {
            motion_pipeline: GraphicsPipeline::new_depth(
                device,
                program::motion_vectors(),
                FORMAT,
                PipelineState::COVERED,
            ),
            motion_blended_pipeline: GraphicsPipeline::new_depth(
                device,
                program::motion_vectors(),
                FORMAT,
                PipelineState::TRANSPARENT,
            ),
            temporal_pipeline: GraphicsPipeline::new(
                device,
                program::temporal_resolve(),
                FORMAT,
                PipelineState::SCREEN,
            ),
            filter: Sampler::new(
                device,
                SamplerDesc {
                    min_filter: Filter::Linear,
                    mag_filter: Filter::Linear,
                    mip_filter: Filter::Nearest,
                    address_u: AddressMode::ClampToEdge,
                    address_v: AddressMode::ClampToEdge,
                },
            ),
            nearest: Sampler::new(
                device,
                SamplerDesc {
                    min_filter: Filter::Nearest,
                    mag_filter: Filter::Nearest,
                    mip_filter: Filter::Nearest,
                    address_u: AddressMode::ClampToEdge,
                    address_v: AddressMode::ClampToEdge,
                },
            ),
            motion_record: GpuBuffer::new(
                device,
                "dome motion",
                MOTION_BYTES,
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            temporal_record: GpuBuffer::new(
                device,
                "dome temporal",
                TEMPORAL_BYTES,
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            screen: None,
            frame: 0,
            previous: None,
        }
    }

    pub(crate) fn jitter(&mut self) -> Vec2 {
        let index = self.frame % SAMPLE_COUNT + 1;
        self.frame = self.frame.wrapping_add(1);
        Vec2::new(halton(index, 2) - 0.5, halton(index, 3) - 0.5)
    }

    pub(crate) fn prepare(&mut self, device: &Device, queue: &Queue, width: u32, height: u32) {
        if self
            .screen
            .as_ref()
            .is_some_and(|screen| screen.width == width && screen.height == height)
        {
            return;
        }
        let usage = TextureUsages::RENDER_ATTACHMENT | TextureUsages::SAMPLED;
        let history = [
            Texture::new(device, "dome history", width, height, 1, FORMAT, usage),
            Texture::new(device, "dome history", width, height, 1, FORMAT, usage),
        ];
        let motion = [
            Texture::new(device, "dome motion", width, height, 1, FORMAT, usage),
            Texture::new(device, "dome motion", width, height, 1, FORMAT, usage),
        ];
        let mut submission = Submission::new(device, "dome history clear");
        for texture in history.iter().chain(motion.iter()) {
            submission.render(Pass::new(texture, CLEAR), &[]);
        }
        submission.submit(queue);
        self.screen = Some(Screen {
            history,
            motion,
            history_at: 0,
            motion_at: 0,
            width,
            height,
            weight: 0.0,
        });
        self.previous = None;
        self.frame = 0;
    }

    pub(crate) fn motion(&self) -> &GraphicsPipeline {
        &self.motion_pipeline
    }

    pub(crate) fn motion_blended(&self) -> &GraphicsPipeline {
        &self.motion_blended_pipeline
    }

    pub(crate) fn view(&self) -> BufferBinding<'_> {
        self.motion_record.binding(0, MOTION_BYTES)
    }

    pub(crate) fn target(&self) -> &Texture {
        let screen = self
            .screen
            .as_ref()
            .expect("a temporal frame holds its motion target");
        &screen.motion[screen.motion_at]
    }

    pub(crate) fn begin(&self, queue: &Queue, pose: Pose, vertical: f32, base: u32, previous: u32) {
        let held = self.previous.unwrap_or(pose);
        self.motion_record.write(
            queue,
            bytemuck::bytes_of(&MotionRecord {
                previous0: column(held.clip, 0),
                previous1: column(held.clip, 1),
                previous2: column(held.clip, 2),
                previous3: column(held.clip, 3),
                eye: direction(pose.eye),
                forward: direction(pose.forward),
                vertical,
                base,
                previous,
                pad0: 0,
            }),
        );
    }

    pub(crate) fn resolve(&self, queue: &Queue, submission: &mut Submission, frame: Frame<'_>) {
        let screen = self
            .screen
            .as_ref()
            .expect("a temporal frame holds its history");
        let pose = frame.pose;
        let previous = self.previous.unwrap_or(pose);
        let inverse = pose.clip.inverse();
        self.temporal_record.write(
            queue,
            bytemuck::bytes_of(&TemporalRecord {
                inverse0: column(inverse, 0),
                inverse1: column(inverse, 1),
                inverse2: column(inverse, 2),
                inverse3: column(inverse, 3),
                previous0: column(previous.clip, 0),
                previous1: column(previous.clip, 1),
                previous2: column(previous.clip, 2),
                previous3: column(previous.clip, 3),
                eye: direction(pose.eye),
                forward: direction(pose.forward),
                previous_eye: direction(previous.eye),
                previous_forward: direction(previous.forward),
                near: frame.near,
                far: frame.far,
                vertical: frame.vertical,
                width: screen.width as f32,
                height: screen.height as f32,
                blend: screen.weight,
                tolerance: DEPTH_TOLERANCE,
                pad0: 0,
            }),
        );
        let record = self.temporal_pipeline.bind_group(
            0,
            &[Binding::buffer(
                program::TEMPORAL_FRAME,
                self.temporal_record.binding(0, TEMPORAL_BYTES),
            )],
        );
        let image = self.temporal_pipeline.bind_group(
            1,
            &[
                Binding::texture(program::TEMPORAL_SOURCE, frame.source),
                Binding::texture(
                    program::TEMPORAL_HISTORY,
                    &screen.history[screen.history_at],
                ),
                Binding::sampler(program::TEMPORAL_FILTER, &self.filter),
            ],
        );
        let gaze = self.temporal_pipeline.bind_group(
            2,
            &[
                Binding::texture(program::TEMPORAL_MOTION, &screen.motion[screen.motion_at]),
                Binding::texture(
                    program::TEMPORAL_PREVIOUS,
                    &screen.motion[1 - screen.motion_at],
                ),
                Binding::sampler(program::TEMPORAL_NEAREST, &self.nearest),
            ],
        );
        submission.render(
            Pass::new(&screen.history[1 - screen.history_at], CLEAR),
            &[Draw::new(
                &self.temporal_pipeline,
                &[record, image, gaze],
                FULLSCREEN_VERTICES,
            )],
        );
    }

    pub(crate) fn resolved(&self) -> &Texture {
        let screen = self
            .screen
            .as_ref()
            .expect("a temporal frame holds its history");
        &screen.history[1 - screen.history_at]
    }

    pub(crate) fn finish(&mut self, pose: Pose) {
        let screen = self
            .screen
            .as_mut()
            .expect("a temporal frame holds its history");
        screen.history_at = 1 - screen.history_at;
        screen.motion_at = 1 - screen.motion_at;
        screen.weight = HISTORY_WEIGHT;
        self.previous = Some(pose);
    }
}

fn halton(index: u32, base: u32) -> f32 {
    let mut index = index;
    let mut fraction = 1.0;
    let mut result = 0.0;
    while index > 0 {
        fraction /= base as f32;
        result += fraction * (index % base) as f32;
        index /= base;
    }
    result
}

fn column(matrix: Mat4, index: usize) -> Fvec4 {
    let held = matrix.col(index);
    Fvec4 {
        x: held.x,
        y: held.y,
        z: held.z,
        w: held.w,
    }
}

fn direction(value: Vec3) -> Fvec4 {
    Fvec4 {
        x: value.x,
        y: value.y,
        z: value.z,
        w: 0.0,
    }
}

const _: () = assert!(
    MOTION_BYTES == MOTION.size as u64,
    "the motion record keeps the layout its shader reads",
);
const _: () = assert!(
    TEMPORAL_BYTES == TEMPORAL.size as u64,
    "the temporal record keeps the layout its shader reads",
);
