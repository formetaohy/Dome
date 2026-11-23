use crate::abi::{DrawRecord, VERTEX_FLOATS, ViewRecord};
use crate::frame::record_of_view;
use crate::mesh::{Mesh, MeshLibrary};
use crate::program;
use crate::view::View;
use dome_ecs::Entity;
use dome_gpu_device::{
    Binding, BufferBinding, BufferUsages, ClipSpace, DepthOps, DepthTexture, Device, Draw,
    GpuBuffer, GraphicsPipeline, Pass, PipelineState, Queue, Submission, Texture, TextureFormat,
    TextureUsages,
};
use dome_math::{Mat4, Vec2, Vec3, Vec4};
use std::mem::size_of;

const VIEW_BYTES: u64 = size_of::<ViewRecord>() as u64;
const DRAW_BYTES: u64 = size_of::<DrawRecord>() as u64;
const VERTEX_BYTES: u64 = (VERTEX_FLOATS * 4) as u64;
const ARGUMENT_BYTES: u64 = 16;
const TARGET_SIDE: u32 = 1;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
const ID_LIMIT: usize = 1 << 24;

pub(crate) struct PickBatch {
    pub(crate) mesh: Mesh,
    pub(crate) base: u32,
    pub(crate) count: u32,
}

pub(crate) struct PickFrame {
    pub(crate) clip: Mat4,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) batches: Vec<PickBatch>,
    pub(crate) entities: Vec<Entity>,
}

pub(crate) struct PickResources<'a> {
    pub(crate) drawn: &'a GpuBuffer,
    pub(crate) arguments: &'a GpuBuffer,
    pub(crate) meshes: &'a MeshLibrary,
    pub(crate) palette: BufferBinding<'a>,
}

pub(crate) struct Picker {
    pipeline: GraphicsPipeline,
    view: GpuBuffer,
    target: Texture,
    depth: DepthTexture,
}

impl Picker {
    pub(crate) fn new(device: &Device) -> Self {
        Self {
            pipeline: GraphicsPipeline::new_depth(
                device,
                program::picking(),
                TextureFormat::Rgba8Unorm,
                PipelineState::DEPTH_WRITE,
            ),
            view: GpuBuffer::new(
                device,
                "dome pick view",
                VIEW_BYTES,
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            target: Texture::new(
                device,
                "dome pick target",
                TARGET_SIDE,
                TARGET_SIDE,
                1,
                TextureFormat::Rgba8Unorm,
                TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            ),
            depth: DepthTexture::new(device, "dome pick depth", TARGET_SIDE, TARGET_SIDE),
        }
    }

    pub(crate) fn pick(
        &mut self,
        device: &Device,
        queue: &Queue,
        clip_space: ClipSpace,
        frame: &PickFrame,
        resources: &PickResources<'_>,
        point: Vec2,
    ) -> Option<Entity> {
        assert!(
            point.x >= 0.0
                && point.y >= 0.0
                && point.x < frame.width as f32
                && point.y < frame.height as f32,
            "a pick of {point:?} leaves the {} by {} viewport",
            frame.width,
            frame.height,
        );
        assert!(
            frame.entities.len() < ID_LIMIT,
            "a pick names {} instances where its channels hold {ID_LIMIT}",
            frame.entities.len(),
        );
        let clip = zoomed(frame.clip, frame.width, frame.height, point, clip_space);
        let record = record_of_view(
            &View::of(clip),
            frame.entities.len() as u32,
            Vec3::ZERO,
            Vec3::Z,
            0,
        );
        self.view.write(queue, bytemuck::bytes_of(&record));
        let groups = frame
            .batches
            .iter()
            .map(|batch| {
                let geometry = resources.meshes.geometry(batch.mesh);
                self.pipeline.bind_group(
                    0,
                    &[
                        Binding::buffer(program::PICK_VIEW, self.view.binding(0, VIEW_BYTES)),
                        Binding::buffer(program::PICK_PALETTE, resources.palette),
                        Binding::buffer(
                            program::PICK_SKINS,
                            resources.meshes.skin_binding(geometry),
                        ),
                        Binding::buffer(
                            program::PICK_DRAWN,
                            resources.drawn.binding(
                                u64::from(batch.base) * DRAW_BYTES,
                                u64::from(batch.count) * DRAW_BYTES,
                            ),
                        ),
                        Binding::buffer(
                            program::PICK_VERTICES,
                            resources.meshes.buffer().binding(
                                u64::from(geometry.first) * VERTEX_BYTES,
                                u64::from(geometry.count) * VERTEX_BYTES,
                            ),
                        ),
                    ],
                )
            })
            .collect::<Vec<_>>();
        let draws = groups
            .iter()
            .enumerate()
            .map(|(index, group)| {
                Draw::indirect(
                    &self.pipeline,
                    std::slice::from_ref(group),
                    resources
                        .arguments
                        .binding(index as u64 * ARGUMENT_BYTES, ARGUMENT_BYTES),
                )
            })
            .collect::<Vec<_>>();
        let mut submission = Submission::new(device, "dome pick");
        submission.render(
            Pass::new(&self.target, CLEAR).depth(&self.depth, DepthOps::CLEAR),
            &draws,
        );
        submission.submit(queue);
        let bytes = self.target.read(queue);
        let pixel = &bytes[..4];
        if pixel[3] == 0 {
            return None;
        }
        let order = u32::from(pixel[0]) | (u32::from(pixel[1]) << 8) | (u32::from(pixel[2]) << 16);
        frame.entities.get(order as usize).copied()
    }
}

fn zoomed(clip: Mat4, width: u32, height: u32, point: Vec2, clip_space: ClipSpace) -> Mat4 {
    let width = width as f32;
    let height = height as f32;
    let ndc_x = (point.x + 0.5) / width * 2.0 - 1.0;
    let ndc_y = match clip_space {
        ClipSpace::YUp => 1.0 - (point.y + 0.5) / height * 2.0,
        ClipSpace::YDown => (point.y + 0.5) / height * 2.0 - 1.0,
    };
    let column = |held: Vec4| {
        Vec4::new(
            (held.x - held.w * ndc_x) * width,
            (held.y - held.w * ndc_y) * height,
            held.z,
            held.w,
        )
    };
    Mat4::from_cols(
        column(clip.col(0)),
        column(clip.col(1)),
        column(clip.col(2)),
        column(clip.col(3)),
    )
}
