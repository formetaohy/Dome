use crate::abi::{DrawRecord, PointLightRecord, ViewRecord};
use crate::frame::record_of_view;
use crate::growing::Growing;
use crate::view::View;
use dome_gpu::{
    AddressMode, BufferBinding, BufferUsages, ClipSpace, DepthTexture, Device, Filter, GpuBuffer,
    Queue, Sampler, SamplerDesc, Texture, TextureFormat, TextureUsages, Viewport,
};
use dome_math::{Mat4, Vec3};
use std::mem::size_of;

pub(crate) const POINT_SHADOW_SIDE: u32 = 256;
pub(crate) const POINT_SHADOW_FACES: u32 = 6;
pub(crate) const POINT_SHADOW_NEAR: f32 = 0.05;

const VIEW_BYTES: u64 = size_of::<ViewRecord>() as u64;
const DRAW_BYTES: u64 = size_of::<DrawRecord>() as u64;
const ARGUMENT_BYTES: u64 = 16;
const WORD_BYTES: u64 = 4;
const WIDE: f32 = std::f32::consts::FRAC_PI_2;
const CLEAR: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

const FACES: [(Vec3, Vec3); POINT_SHADOW_FACES as usize] = [
    (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
    (Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)),
    (Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, -1.0, 0.0)),
];

pub(crate) struct PointShadowFrame<'a> {
    pub(crate) clip_space: ClipSpace,
    pub(crate) lights: &'a [PointLightRecord],
    pub(crate) instances: u32,
    pub(crate) slots: u32,
    pub(crate) palette_base: u32,
    pub(crate) arguments: &'a [u32],
}

pub(crate) struct PointShadows {
    atlas: Texture,
    depth: DepthTexture,
    sampler: Sampler,
    views: GpuBuffer,
    drawn: Growing,
    arguments: Growing,
    rows: u32,
    slots: u32,
    batches: u32,
    drawn_span: u64,
    argument_span: u64,
}

impl PointShadows {
    pub(crate) fn new(device: &Device) -> Self {
        let (atlas, depth) = atlas(device, 1);
        Self {
            atlas,
            depth,
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
            views: GpuBuffer::new(
                device,
                "dome point shadow views",
                u64::from(POINT_SHADOW_FACES) * VIEW_BYTES,
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            drawn: Growing::new(device, "dome point shadow drawn", BufferUsages::STORAGE),
            arguments: Growing::new(
                device,
                "dome point shadow arguments",
                BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
            ),
            rows: 1,
            slots: 0,
            batches: 0,
            drawn_span: 0,
            argument_span: 0,
        }
    }

    pub(crate) fn ensure(&mut self, device: &Device, lights: usize) {
        let rows = (lights as u32).max(1);
        if self.rows == rows {
            return;
        }
        (self.atlas, self.depth) = atlas(device, rows);
        self.views = GpuBuffer::new(
            device,
            "dome point shadow views",
            u64::from(rows) * u64::from(POINT_SHADOW_FACES) * VIEW_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        self.rows = rows;
    }

    pub(crate) fn prepare(&mut self, device: &Device, queue: &Queue, frame: &PointShadowFrame<'_>) {
        let PointShadowFrame {
            clip_space,
            lights,
            instances,
            slots,
            palette_base,
            arguments,
        } = *frame;
        assert!(
            arguments.len().is_multiple_of(4),
            "a frame writes {} argument words, not whole draws",
            arguments.len(),
        );
        assert_eq!(
            lights.len().max(1) as u32,
            self.rows,
            "a frame prepares the point shadow rows its lights span",
        );
        if lights.is_empty() {
            self.slots = slots;
            self.batches = (arguments.len() / 4) as u32;
            return;
        }
        let rows = self.rows;
        self.slots = slots;
        self.batches = (arguments.len() / 4) as u32;
        let alignment = device.limits().min_storage_buffer_offset_alignment.max(4);
        self.drawn_span = (u64::from(slots) * DRAW_BYTES).next_multiple_of(alignment);
        self.argument_span = (arguments.len() as u64 * WORD_BYTES).next_multiple_of(alignment);
        let cells = u64::from(rows) * u64::from(POINT_SHADOW_FACES);
        self.drawn.fit(device, cells * self.drawn_span);
        self.arguments.fit(device, cells * self.argument_span);
        let mut views = Vec::with_capacity(cells as usize);
        for light in lights {
            let far = light.position.w;
            assert!(
                far > POINT_SHADOW_NEAR && far.is_finite(),
                "a point light of range {far} shadows no depth",
            );
            let eye = Vec3::new(light.position.x, light.position.y, light.position.z);
            let projection = match clip_space {
                ClipSpace::YUp => Mat4::perspective(WIDE, 1.0, POINT_SHADOW_NEAR, far),
                ClipSpace::YDown => {
                    Mat4::perspective(WIDE, 1.0, POINT_SHADOW_NEAR, far).flip_clip_y()
                }
            };
            for (forward, up) in FACES {
                let clip = projection * Mat4::look_at(eye, eye + forward, up);
                views.push(record_of_view(
                    &View::of(clip),
                    instances,
                    Vec3::ZERO,
                    Vec3::Z,
                    palette_base,
                ));
            }
        }
        if views.is_empty() {
            views.push(record_of_view(
                &View::of(Mat4::IDENTITY),
                0,
                Vec3::ZERO,
                Vec3::Z,
                palette_base,
            ));
        }
        self.views.write(queue, bytemuck::cast_slice(&views));
        let mut held = vec![0u8; (cells * self.argument_span) as usize];
        let source = bytemuck::cast_slice(arguments);
        for cell in 0..cells {
            let start = (cell * self.argument_span) as usize;
            if source.is_empty() {
                break;
            }
            held[start..start + source.len()].copy_from_slice(source);
        }
        self.arguments.buffer().write(queue, &held);
    }

    pub(crate) fn atlas(&self) -> &Texture {
        &self.atlas
    }

    pub(crate) fn depth(&self) -> &DepthTexture {
        &self.depth
    }

    pub(crate) fn sampler(&self) -> &Sampler {
        &self.sampler
    }

    pub(crate) fn clear(&self) -> [f32; 4] {
        CLEAR
    }

    pub(crate) fn cells(&self) -> u32 {
        self.rows * POINT_SHADOW_FACES
    }

    pub(crate) fn view(&self, cell: u32) -> BufferBinding<'_> {
        self.views.binding(u64::from(cell) * VIEW_BYTES, VIEW_BYTES)
    }

    pub(crate) fn drawn_view(&self, cell: u32) -> BufferBinding<'_> {
        self.drawn.buffer().binding(
            u64::from(cell) * self.drawn_span,
            u64::from(self.slots).max(1) * DRAW_BYTES,
        )
    }

    pub(crate) fn argument_view(&self, cell: u32) -> BufferBinding<'_> {
        self.arguments.buffer().binding(
            u64::from(cell) * self.argument_span,
            (u64::from(self.batches).max(1) * ARGUMENT_BYTES).min(self.argument_span),
        )
    }

    pub(crate) fn drawn(&self, cell: u32, base: u32, count: u32) -> BufferBinding<'_> {
        self.drawn.buffer().binding(
            u64::from(cell) * self.drawn_span + u64::from(base) * DRAW_BYTES,
            u64::from(count) * DRAW_BYTES,
        )
    }

    pub(crate) fn arguments(&self, cell: u32, batch: u32) -> BufferBinding<'_> {
        self.arguments.buffer().binding(
            u64::from(cell) * self.argument_span + u64::from(batch) * ARGUMENT_BYTES,
            ARGUMENT_BYTES,
        )
    }

    pub(crate) fn viewport(&self, cell: u32) -> Viewport {
        let light = cell / POINT_SHADOW_FACES;
        let face = cell % POINT_SHADOW_FACES;
        Viewport::new(
            (face * POINT_SHADOW_SIDE) as f32,
            (light * POINT_SHADOW_SIDE) as f32,
            POINT_SHADOW_SIDE as f32,
            POINT_SHADOW_SIDE as f32,
        )
    }
}

fn atlas(device: &Device, rows: u32) -> (Texture, DepthTexture) {
    let width = POINT_SHADOW_FACES * POINT_SHADOW_SIDE;
    let height = rows * POINT_SHADOW_SIDE;
    (
        Texture::new(
            device,
            "dome point shadows",
            width,
            height,
            1,
            TextureFormat::R32Float,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::SAMPLED,
        ),
        DepthTexture::new(device, "dome point shadow depth", width, height),
    )
}

const _: () = assert!(
    POINT_SHADOW_SIDE.is_multiple_of(2),
    "a point shadow face holds whole texel pairs",
);
