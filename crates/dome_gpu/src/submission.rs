use crate::buffer::GpuBuffer;
use crate::capability::{BufferUsages, TextureUsages};
use crate::context::{Device, Queue};
use crate::depth::DepthTexture;
use crate::graphics::{Count, DepthOps, Draw, Pass, Scissor, Viewport};
use crate::native::NativeBuffer;
use crate::pipeline::{BindGroup, GraphicsSlot, Owner, PipelineHandle};
use crate::surface::SurfaceFrame;
use crate::texture::Texture;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubmissionIndex(pub(crate) u64);

pub(crate) struct Write {
    pub(crate) buffer: NativeBuffer,
    pub(crate) offset: u64,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone)]
pub(crate) enum RecordedCount {
    Direct { vertices: u32, instances: u32 },
    Indirect { arguments: GpuBuffer, offset: u64 },
}

#[derive(Clone)]
pub(crate) struct RecordedDraw {
    pub(crate) pipeline: Arc<GraphicsSlot>,
    pub(crate) groups: Vec<BindGroup>,
    pub(crate) count: RecordedCount,
    pub(crate) viewport: Viewport,
    pub(crate) scissor: Scissor,
}

#[derive(Clone)]
pub(crate) enum Command {
    Copy {
        source: GpuBuffer,
        source_offset: u64,
        target: GpuBuffer,
        target_offset: u64,
        bytes: u64,
    },
    Clear {
        buffer: GpuBuffer,
        offset: u64,
        bytes: u64,
    },
    Dispatch {
        pipeline: PipelineHandle,
        group: BindGroup,
        groups: [u32; 3],
    },
    Render {
        target: Texture,
        level: u32,
        clear: [f32; 4],
        depth: Option<(DepthTexture, DepthOps)>,
        draws: Vec<RecordedDraw>,
    },
    CopyTexture {
        texture: Texture,
        target: GpuBuffer,
        target_offset: u64,
    },
    WriteTexture {
        texture: Texture,
        level: u32,
        bytes: Vec<u8>,
    },
    Present {
        texture: Texture,
        native: crate::native::NativePresent,
        token: crate::surface::PresentToken,
    },
}

pub struct Submission {
    device: Device,
    commands: Vec<Command>,
}

impl Submission {
    pub fn new(device: &Device, _label: &str) -> Self {
        Self {
            device: device.clone(),
            commands: Vec::new(),
        }
    }

    pub fn copy(
        &mut self,
        source: &GpuBuffer,
        source_offset: u64,
        target: &GpuBuffer,
        target_offset: u64,
        bytes: u64,
    ) {
        assert!(
            source.device().same(&self.device) && target.device().same(&self.device),
            "a transfer crosses compute devices"
        );
        assert!(
            source.usage().contains(BufferUsages::COPY_SRC)
                && target.usage().contains(BufferUsages::COPY_DST),
            "a transfer requires a copy source and destination"
        );
        assert!(
            bytes > 0
                && bytes.is_multiple_of(4)
                && source_offset.is_multiple_of(4)
                && target_offset.is_multiple_of(4)
        );
        assert!(
            source_offset
                .checked_add(bytes)
                .is_some_and(|end| end <= source.size())
        );
        assert!(
            target_offset
                .checked_add(bytes)
                .is_some_and(|end| end <= target.size())
        );
        assert!(
            source.allocation() != target.allocation()
                || source_offset + bytes <= target_offset
                || target_offset + bytes <= source_offset,
            "a copy cannot overlap itself"
        );
        self.commands.push(Command::Copy {
            source: source.clone(),
            source_offset,
            target: target.clone(),
            target_offset,
            bytes,
        });
    }

    pub fn clear(&mut self, buffer: &GpuBuffer, offset: u64, bytes: u64) {
        assert!(
            buffer.device().same(&self.device),
            "a clear crosses compute devices"
        );
        assert!(
            buffer.usage().contains(crate::BufferUsages::COPY_DST),
            "a clear requires a copy destination"
        );
        assert!(offset.is_multiple_of(4) && bytes > 0 && bytes.is_multiple_of(4));
        assert!(
            offset
                .checked_add(bytes)
                .is_some_and(|end| end <= buffer.size())
        );
        self.commands.push(Command::Clear {
            buffer: buffer.clone(),
            offset,
            bytes,
        });
    }

    pub fn dispatch(&mut self, pipeline: &PipelineHandle, group: &BindGroup, groups: [u32; 3]) {
        assert!(
            pipeline.slot.device.same(&self.device),
            "a pipeline crosses compute devices"
        );
        assert!(
            matches!(&group.owner, Owner::Compute(slot) if Arc::ptr_eq(slot, &pipeline.slot)),
            "a group belongs to another pipeline"
        );
        assert!(
            pipeline
                .slot
                .program
                .bindings()
                .iter()
                .all(|binding| binding.group == 0),
            "a compute dispatch binds one group"
        );
        let limits = self.device.limits();
        assert!(
            groups
                .iter()
                .all(|count| *count > 0 && *count <= limits.max_compute_workgroups_per_dimension),
            "a dispatch exceeds the workgroup grid"
        );
        pipeline.compile();
        self.commands.push(Command::Dispatch {
            pipeline: pipeline.clone(),
            group: group.clone(),
            groups,
        });
    }

    pub fn render(&mut self, pass: Pass<'_>, draws: &[Draw<'_>]) {
        let Pass {
            color: target,
            level,
            clear,
            depth,
        } = pass;
        assert!(
            target.device().same(&self.device),
            "a render pass crosses devices"
        );
        assert!(
            target.usages().contains(TextureUsages::RENDER_ATTACHMENT),
            "a render pass draws into a render attachment"
        );
        assert!(
            level < target.mip_count(),
            "a render pass draws into the level {level} of a {} level texture",
            target.mip_count(),
        );
        let (width, height) = target.level_extent(level);
        if let Some((held, _)) = &depth {
            assert!(
                held.width() == width && held.height() == height,
                "a {width} by {height} pass loads a {} by {} depth texture",
                held.width(),
                held.height(),
            );
        }
        let draws = draws
            .iter()
            .map(|draw| {
                let pipeline = &draw.pipeline.slot;
                assert!(
                    pipeline.device.same(&self.device),
                    "a graphics pipeline crosses devices"
                );
                assert_eq!(
                    pipeline.format,
                    target.format(),
                    "a draw writes the {:?} format of its pipeline into a {:?} target",
                    pipeline.format,
                    target.format(),
                );
                assert_eq!(
                    pipeline.depth,
                    depth.is_some(),
                    "a draw of {} declares {} depth attachment where the pass holds {}",
                    pipeline.program.label(),
                    if pipeline.depth { "a" } else { "no" },
                    if depth.is_some() { "one" } else { "none" },
                );
                let declared = pipeline
                    .program
                    .bindings()
                    .iter()
                    .map(|binding| binding.group)
                    .max()
                    .map_or(0, |group| group + 1);
                assert_eq!(
                    draw.groups.len() as u32,
                    declared,
                    "a draw binds every group of its pipeline"
                );
                for (index, group) in draw.groups.iter().enumerate() {
                    assert_eq!(
                        group.group, index as u32,
                        "a draw binds its groups in order"
                    );
                    assert!(
                        group.owner.same(&Owner::Graphics(pipeline.clone())),
                        "a bind group belongs to another pipeline"
                    );
                }
                draw.pipeline.compile();
                let count = match draw.count {
                    Count::Direct {
                        vertices,
                        instances,
                    } => {
                        assert!(
                            vertices > 0 && instances > 0,
                            "a draw of no vertex or instance renders nothing"
                        );
                        RecordedCount::Direct {
                            vertices,
                            instances,
                        }
                    }
                    Count::Indirect(arguments) => {
                        let buffer = arguments.buffer;
                        assert!(
                            buffer.device().same(&self.device),
                            "an indirect draw crosses devices"
                        );
                        assert!(
                            buffer.usage().contains(BufferUsages::INDIRECT),
                            "an indirect draw reads a buffer declared for indirect arguments"
                        );
                        assert!(
                            arguments.offset.is_multiple_of(4),
                            "an indirect draw reads its arguments on a word boundary"
                        );
                        assert!(
                            arguments.size >= 16,
                            "an indirect draw reads {} bytes where its arguments span 16",
                            arguments.size
                        );
                        RecordedCount::Indirect {
                            arguments: buffer.clone(),
                            offset: arguments.offset,
                        }
                    }
                };
                let viewport = draw
                    .viewport
                    .unwrap_or_else(|| Viewport::full(width, height));
                viewport.assert_within(width, height);
                let scissor = draw.scissor.unwrap_or_else(|| Scissor::full(width, height));
                scissor.assert_within(width, height);
                RecordedDraw {
                    pipeline: pipeline.clone(),
                    groups: draw.groups.to_vec(),
                    count,
                    viewport,
                    scissor,
                }
            })
            .collect();
        self.commands.push(Command::Render {
            target: target.clone(),
            level,
            clear,
            depth: depth.map(|(held, ops)| (held.clone(), ops)),
            draws,
        });
    }

    pub fn write_texture(&mut self, texture: &Texture, level: u32, pixels: &[u8]) {
        assert!(
            texture.device().same(&self.device),
            "a texture upload crosses devices"
        );
        assert!(
            texture.usages().contains(TextureUsages::COPY_DST),
            "a texture upload requires a copy destination"
        );
        let (width, height) = texture.level_extent(level);
        let row = texture.level_row_bytes(level) as usize;
        let stride = texture.level_row_stride(level) as usize;
        assert_eq!(
            pixels.len(),
            row * height as usize,
            "a level {level} upload writes {} bytes where {width} texels of {row} bytes span {}",
            pixels.len(),
            row * height as usize,
        );
        let mut bytes = vec![0u8; stride * height as usize];
        for (line, target) in pixels.chunks_exact(row).zip(bytes.chunks_exact_mut(stride)) {
            target[..row].copy_from_slice(line);
        }
        self.commands.push(Command::WriteTexture {
            texture: texture.clone(),
            level,
            bytes,
        });
    }

    pub fn copy_texture(&mut self, texture: &Texture, target: &GpuBuffer, target_offset: u64) {
        assert!(
            texture.device().same(&self.device) && target.device().same(&self.device),
            "a texture readback crosses devices"
        );
        assert!(
            target.usage().contains(crate::BufferUsages::COPY_DST),
            "a texture readback requires a copy destination"
        );
        assert!(
            target_offset.is_multiple_of(512),
            "a texture readback starts on a 512 byte boundary"
        );
        assert!(
            target_offset
                .checked_add(texture.readback_bytes())
                .is_some_and(|end| end <= target.size()),
            "a texture of {} bytes outruns its readback buffer",
            texture.readback_bytes()
        );
        self.commands.push(Command::CopyTexture {
            texture: texture.clone(),
            target: target.clone(),
            target_offset,
        });
    }

    pub fn present(&mut self, mut frame: SurfaceFrame) {
        assert!(
            !self
                .commands
                .iter()
                .any(|command| matches!(command, Command::Present { .. })),
            "a submission presents one surface frame"
        );
        let native = frame.native.take().expect("a surface frame presents once");
        self.commands.push(Command::Present {
            texture: frame.texture.clone(),
            native,
            token: frame.token.clone(),
        });
    }

    pub fn submit(self, queue: &Queue) -> SubmissionIndex {
        assert!(
            self.device.same(queue.device()),
            "a submission crosses compute devices"
        );
        for command in &self.commands {
            if let Command::Present { texture, token, .. } = command {
                assert!(
                    token.valid(),
                    "a surface frame outlived the {} by {} swapchain it was acquired from",
                    texture.width(),
                    texture.height(),
                );
            }
        }
        queue.submit(&self.commands)
    }
}
