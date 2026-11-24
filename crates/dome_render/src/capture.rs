use crate::abi::{
    ARGUMENT_WORDS, AtlasRecord, CascadeRecord, LIGHT_SLICES, LIGHT_STRIDE, LIGHT_TILE,
    NO_HIGHLIGHT, PointLightRecord, SHADOW_CASCADES, SHADOW_SIDE, SkyRecord, ViewRecord,
};
use crate::camera::Camera;
use crate::environment::{BakeRequest, Baked, Environment};
use crate::frame::{
    Batch, Screen, direction_record, record_of_cascade, record_of_main_view, record_of_view, vector,
};
use crate::growing::Growing;
use crate::program;
use crate::shadow::Cascade;
use crate::view::View;
use dome_ecs::{Entity, World};
use dome_gpu::{
    AddressMode, BufferBinding, BufferUsages, CompareFunction, Device, FilterMode, Gpu, GpuBuffer,
    GraphicsPipeline, PipelineState, Queue, Sampler, SamplerDesc, Texture, TextureFormat,
    TextureUsages,
};
use dome_math::{Mat4, Quat, Transform, Vec3};
use std::mem::size_of;

pub const PROBE_RESOLUTION: u32 = 128;
pub(crate) const FACES: u32 = 6;
const VIEWS: u32 = FACES + 1;
pub(crate) const SHADOW_VIEW: u32 = FACES;
const PROBE_WIDTH: u32 = PROBE_RESOLUTION * FACES;
pub(crate) const PANORAMA_WIDTH: u32 = PROBE_RESOLUTION * 4;
pub(crate) const PANORAMA_HEIGHT: u32 = PROBE_RESOLUTION * 2;
const VIEW_BYTES: u64 = size_of::<ViewRecord>() as u64;
const SKY_BYTES: u64 = size_of::<SkyRecord>() as u64;
const INDEX_BYTES: u64 = size_of::<u32>() as u64;
const CASCADE_BYTES: u64 = size_of::<CascadeRecord>() as u64;
const SETUP_BYTES: u64 = size_of::<AtlasRecord>() as u64;
const POINT_LIGHT_BYTES: u64 = size_of::<PointLightRecord>() as u64;
const ARGUMENT_BYTES: u64 = (ARGUMENT_WORDS * 4) as u64;
const WORD_BYTES: u64 = 4;
const CLUSTER_BYTES: u64 = LIGHT_STRIDE as u64 * WORD_BYTES;
const CLUSTER_COLUMNS: u32 = PROBE_RESOLUTION.div_ceil(LIGHT_TILE);
const CLUSTER_ROWS: u32 = CLUSTER_COLUMNS;
const FACE_CLUSTERS: u32 = CLUSTER_COLUMNS * CLUSTER_ROWS * LIGHT_SLICES;
const FACE_CLUSTER_BYTES: u64 = FACE_CLUSTERS as u64 * CLUSTER_BYTES;
const NEAR: f32 = 0.05;
const FAR: f32 = 1_000.0;
const WIDE: f32 = std::f32::consts::FRAC_PI_2;

const FACES_OF: [(Vec3, Vec3); FACES as usize] = [
    (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
    (Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)),
    (Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, -1.0, 0.0)),
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CaptureProbe {
    captured: Option<Transform>,
}

impl CaptureProbe {
    pub fn stale(&self, transform: Transform) -> bool {
        self.captured != Some(transform)
    }

    pub fn captured(&mut self, transform: Transform) {
        self.captured = Some(transform);
    }
}

pub struct Capture {
    pub entity: Entity,
    pub probe: CaptureProbe,
    pub environment: Environment,
}

pub fn install_captures(world: &mut World, captures: impl IntoIterator<Item = Capture>) {
    for capture in captures {
        assert!(
            world.is_alive(capture.entity),
            "a capture names the live probe it renders",
        );
        world.insert(capture.entity, capture.probe);
        world.insert(capture.entity, capture.environment);
    }
}

pub(crate) struct Probe {
    pub(crate) entity: Entity,
    pub(crate) transform: Transform,
    pub(crate) probe: CaptureProbe,
    pub(crate) environment: Environment,
}

pub(crate) struct CaptureFrame<'a> {
    pub(crate) device: Device,
    pub(crate) queue: Queue,
    pub(crate) eye: Vec3,
    pub(crate) basis: Quat,
    pub(crate) rotation: Mat4,
    pub(crate) intensity: f32,
    pub(crate) shadow: Cascade,
    pub(crate) palette_base: u32,
    pub(crate) batches: &'a [Batch],
    pub(crate) arguments: &'a [u32],
    pub(crate) slots: u32,
    pub(crate) instances: u32,
    pub(crate) points: &'a [PointLightRecord],
}

pub(crate) struct Pending {
    pub(crate) entity: Entity,
    pub(crate) probe: CaptureProbe,
    pub(crate) intensity: f32,
    pub(crate) background: bool,
    pub(crate) blend: f32,
    pub(crate) panorama: Texture,
    pub(crate) request: BakeRequest,
}

pub(crate) struct Captures {
    pub(crate) atlas: Texture,
    pub(crate) depth: Texture,
    pub(crate) shadow: Texture,
    pub(crate) shadow_sampler: Sampler,
    pub(crate) views: GpuBuffer,
    pub(crate) skies: GpuBuffer,
    pub(crate) cascades: GpuBuffer,
    pub(crate) setup: GpuBuffer,
    pub(crate) visible: Growing,
    pub(crate) arguments: Growing,
    pub(crate) point_light: Growing,
    pub(crate) lighting: Growing,
    pub(crate) convert: GraphicsPipeline,
    pub(crate) sampler: Sampler,
    slots: u32,
    batches: u32,
    point_bytes: u64,
    sky_span: u64,
    cluster_span: u64,
    visible_span: u64,
    argument_span: u64,
    pending: Option<Pending>,
    results: Vec<Capture>,
}

impl Captures {
    pub(crate) fn new(context: &Gpu) -> Self {
        let device = context.device();
        let queue = Queue::of(device);
        let attachment = TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
        let depth_attachment = TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
        let storage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let sky_span = crate::span::padded(device.binding_alignment().max(WORD_BYTES), SKY_BYTES);
        let setup = GpuBuffer::new(device, "dome capture atlas", SETUP_BYTES, storage);
        setup.write(
            &queue,
            bytemuck::bytes_of(&AtlasRecord {
                face: 1.0 / FACES as f32,
                vertical: crate::frame::vertical(),
                texel: 0.5 / PROBE_RESOLUTION as f32,
                pad0: 0.0,
            }),
        );
        Self {
            atlas: Texture::new(
                device,
                "dome capture faces",
                PROBE_WIDTH,
                PROBE_RESOLUTION,
                1,
                TextureFormat::Rgba16Float,
                attachment,
            ),
            depth: Texture::new(
                device,
                "dome capture depth",
                PROBE_WIDTH,
                PROBE_RESOLUTION,
                1,
                TextureFormat::Depth32Float,
                depth_attachment,
            ),
            shadow: Texture::new(
                device,
                "dome capture shadow",
                SHADOW_SIDE,
                SHADOW_SIDE,
                1,
                TextureFormat::Depth32Float,
                depth_attachment,
            ),
            shadow_sampler: Sampler::new(
                device,
                SamplerDesc {
                    min_filter: FilterMode::Nearest,
                    mag_filter: FilterMode::Nearest,
                    mipmap_filter: FilterMode::Nearest,
                    address_mode_u: AddressMode::ClampToEdge,
                    address_mode_v: AddressMode::ClampToEdge,
                    compare: Some(CompareFunction::LessEqual),
                },
            ),
            views: GpuBuffer::new(
                device,
                "dome capture views",
                u64::from(VIEWS) * VIEW_BYTES,
                storage,
            ),
            skies: GpuBuffer::new(
                device,
                "dome capture skies",
                u64::from(FACES) * sky_span,
                storage,
            ),
            cascades: GpuBuffer::new(
                device,
                "dome capture cascades",
                u64::from(SHADOW_CASCADES) * CASCADE_BYTES,
                storage,
            ),
            setup,
            visible: Growing::new(device, "dome capture visible", storage),
            arguments: Growing::new(
                device,
                "dome capture arguments",
                storage | BufferUsages::INDIRECT,
            ),
            point_light: Growing::new(device, "dome capture point lights", storage),
            lighting: Growing::new(device, "dome capture lighting", storage),
            convert: GraphicsPipeline::new(
                device,
                program::capture_conversion(),
                TextureFormat::Rgba16Float,
                PipelineState::SCREEN,
            ),
            slots: 0,
            batches: 0,
            point_bytes: 0,
            sky_span,
            cluster_span: FACE_CLUSTER_BYTES,
            visible_span: INDEX_BYTES,
            argument_span: ARGUMENT_BYTES,
            sampler: Sampler::new(
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
            results: Vec::new(),
            pending: None,
        }
    }

    pub(crate) fn take(&mut self) -> Vec<Capture> {
        std::mem::take(&mut self.results)
    }

    pub(crate) fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    pub(crate) fn begin(&mut self, pending: Pending) {
        assert!(
            self.pending.is_none(),
            "a capture submits one bake before its probe settles",
        );
        self.pending = Some(pending);
    }

    pub(crate) fn settle(&mut self, baked: Baked, sampler: &Sampler) {
        let pending = self
            .pending
            .take()
            .expect("a settled capture answers the bake it submitted");
        let mut environment = Environment::new(
            baked.radiance,
            pending.panorama,
            sampler.clone(),
            baked.levels,
            baked.irradiance,
        );
        environment.intensity = pending.intensity;
        environment.background = pending.background;
        environment.local = true;
        environment.blend = pending.blend;
        self.results.push(Capture {
            entity: pending.entity,
            probe: pending.probe,
            environment,
        });
    }

    pub(crate) fn panorama(&self, device: &Device) -> Texture {
        Texture::new(
            device,
            "dome capture panorama",
            PANORAMA_WIDTH,
            PANORAMA_HEIGHT,
            1,
            TextureFormat::Rgba16Float,
            TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
        )
    }

    pub(crate) fn setup(&self) -> BufferBinding<'_> {
        self.setup.binding(0, SETUP_BYTES)
    }

    pub(crate) fn view(&self, view: u32) -> BufferBinding<'_> {
        self.views.binding(u64::from(view) * VIEW_BYTES, VIEW_BYTES)
    }

    pub(crate) fn sky(&self, face: u32) -> BufferBinding<'_> {
        self.skies
            .binding(u64::from(face) * self.sky_span, SKY_BYTES)
    }

    pub(crate) fn cascades(&self) -> BufferBinding<'_> {
        self.cascades
            .binding(0, u64::from(SHADOW_CASCADES) * CASCADE_BYTES)
    }

    pub(crate) fn visible(&self, view: u32, offset: u32, count: u32) -> BufferBinding<'_> {
        let base = u64::from(view) * self.visible_span + u64::from(offset) * INDEX_BYTES;
        self.visible
            .buffer()
            .binding(base, u64::from(count) * INDEX_BYTES)
    }

    pub(crate) fn visible_view(&self, view: u32) -> BufferBinding<'_> {
        let offset = u64::from(view) * self.visible_span;
        self.visible
            .buffer()
            .binding(offset, u64::from(self.slots) * INDEX_BYTES)
    }

    pub(crate) fn argument_view(&self, view: u32) -> BufferBinding<'_> {
        let offset = u64::from(view) * self.argument_span;
        self.arguments
            .buffer()
            .binding(offset, u64::from(self.batches) * ARGUMENT_BYTES)
    }

    pub(crate) fn arguments(&self, view: u32, batch: u32) -> BufferBinding<'_> {
        let base = u64::from(view) * self.argument_span + u64::from(batch) * ARGUMENT_BYTES;
        self.arguments.buffer().binding(base, ARGUMENT_BYTES)
    }

    pub(crate) fn point_lights(&self) -> BufferBinding<'_> {
        self.point_light
            .buffer()
            .binding(0, self.point_bytes.max(POINT_LIGHT_BYTES))
    }

    pub(crate) fn clusters(&self, face: u32) -> BufferBinding<'_> {
        self.lighting
            .buffer()
            .binding(u64::from(face) * self.cluster_span, FACE_CLUSTER_BYTES)
    }

    pub(crate) fn cluster_region(&self) -> (u64, u64) {
        (0, u64::from(FACES) * self.cluster_span)
    }

    pub(crate) fn lighting(&self) -> BufferBinding<'_> {
        self.lighting
            .buffer()
            .binding(0, u64::from(FACES) * self.cluster_span)
    }

    pub(crate) fn cluster_columns(&self) -> u32 {
        CLUSTER_COLUMNS
    }

    pub(crate) fn cluster_rows(&self) -> u32 {
        CLUSTER_ROWS
    }

    pub(crate) fn prepare(&mut self, frame: &CaptureFrame<'_>) {
        let alignment = frame.device.binding_alignment().max(WORD_BYTES);
        self.sky_span = crate::span::padded(alignment, SKY_BYTES);
        self.cluster_span = crate::span::padded(alignment, FACE_CLUSTER_BYTES);
        self.point_bytes = (frame.points.len() as u64 * POINT_LIGHT_BYTES).max(POINT_LIGHT_BYTES);
        self.slots = frame.slots.max(1);
        self.batches = frame.batches.len() as u32;
        self.visible_span = crate::span::padded(alignment, u64::from(self.slots) * INDEX_BYTES);
        self.argument_span =
            crate::span::padded(alignment, u64::from(self.batches) * ARGUMENT_BYTES);
        self.point_light.fit(&frame.device, self.point_bytes);
        self.lighting
            .fit(&frame.device, u64::from(FACES) * self.cluster_span);
        if !frame.points.is_empty() {
            self.point_light
                .buffer()
                .write(&frame.queue, bytemuck::cast_slice(frame.points));
        }
        let camera = Camera::new(WIDE, NEAR, FAR);
        let instances = frame.instances;
        let axes = FACES_OF.map(|(forward, up)| {
            (
                frame.basis.rotate_vec3(forward),
                frame.basis.rotate_vec3(up),
            )
        });
        let views = axes.map(|(forward, up)| face_view(frame, forward, up));
        let mut records = Vec::with_capacity(VIEWS as usize);
        for (face, view) in views.iter().enumerate() {
            let screen = Screen {
                width: PROBE_RESOLUTION,
                height: PROBE_RESOLUTION,
                vertical: crate::frame::vertical(),
                hzb_levels: 0,
                cluster_x: CLUSTER_COLUMNS,
                cluster_y: CLUSTER_ROWS,
                point_lights: frame.points.len() as u32,
                cluster_offset: face as u32 * (self.cluster_span / WORD_BYTES) as u32,
                highlight: NO_HIGHLIGHT,
            };
            records.push(record_of_main_view(
                view,
                instances,
                frame.eye,
                axes[face].0,
                frame.palette_base,
                camera,
                screen,
            ));
        }
        let shadow = View::of(frame.shadow.clip);
        records.push(record_of_view(
            &shadow,
            instances,
            Vec3::ZERO,
            Vec3::Z,
            frame.palette_base,
        ));
        self.views
            .write(&frame.queue, bytemuck::cast_slice(records.as_slice()));
        let skies = views
            .iter()
            .map(|view| sky_record(view, frame))
            .collect::<Vec<_>>();
        let mut sky_bytes = vec![0u8; (u64::from(FACES) * self.sky_span) as usize];
        for (face, record) in skies.iter().enumerate() {
            let start = face * self.sky_span as usize;
            sky_bytes[start..start + SKY_BYTES as usize]
                .copy_from_slice(bytemuck::bytes_of(record));
        }
        self.skies.write(&frame.queue, &sky_bytes);
        self.cascades.write(
            &frame.queue,
            bytemuck::cast_slice(&[record_of_cascade(&frame.shadow); SHADOW_CASCADES as usize]),
        );
        self.arguments
            .fit(&frame.device, u64::from(VIEWS) * self.argument_span);
        if self.batches > 0 {
            let mut arguments = vec![0u8; (u64::from(VIEWS) * self.argument_span) as usize];
            let source = bytemuck::cast_slice(frame.arguments);
            for view in 0..VIEWS {
                let start = (u64::from(view) * self.argument_span) as usize;
                arguments[start..start + source.len()].copy_from_slice(source);
            }
            self.arguments.buffer().write(&frame.queue, &arguments);
        }
        self.visible
            .fit(&frame.device, u64::from(VIEWS) * self.visible_span);
    }

    pub(crate) fn shadow_visible(&self, offset: u32, count: u32) -> BufferBinding<'_> {
        self.visible(SHADOW_VIEW, offset, count)
    }

    pub(crate) fn shadow_arguments(&self, batch: u32) -> BufferBinding<'_> {
        self.arguments(SHADOW_VIEW, batch)
    }

    pub(crate) fn view_count(&self) -> u32 {
        VIEWS
    }
}

fn face_view(frame: &CaptureFrame<'_>, forward: Vec3, up: Vec3) -> View {
    let projection = Mat4::perspective(WIDE, 1.0, NEAR, FAR);

    View::of(projection * Mat4::look_at(frame.eye, frame.eye + forward.normalize(), up.normalize()))
}

fn sky_record(view: &View, frame: &CaptureFrame<'_>) -> SkyRecord {
    let inverse = view.clip.inverse();
    SkyRecord {
        inverse0: vector(inverse.col(0)),
        inverse1: vector(inverse.col(1)),
        inverse2: vector(inverse.col(2)),
        inverse3: vector(inverse.col(3)),
        eye: direction_record(frame.eye),
        environment0: vector(frame.rotation.col(0)),
        environment1: vector(frame.rotation.col(1)),
        environment2: vector(frame.rotation.col(2)),
        intensity: frame.intensity,
        pad0: 0,
        pad1: 0,
        pad2: 0,
    }
}

const _: () = assert!(
    CASCADE_BYTES == size_of::<CascadeRecord>() as u64,
    "the capture cascade record keeps the layout its shader reads",
);
