use crate::binding::BindGroup;
use crate::buffer::{BufferBinding, BufferUsages, GpuBuffer};
use crate::context::{Device, Queue};
use crate::pipeline::{ComputePipeline, GraphicsPipeline};
use crate::surface::SurfaceFrame;
use crate::texture::{Texture, TextureFormatExt, TextureRegion, TextureUsages};

pub use wgpu::SubmissionIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DepthLoad {
    Clear,
    Keep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DepthOps {
    pub load: DepthLoad,
    pub store: bool,
}

impl DepthOps {
    pub const CLEAR: Self = Self {
        load: DepthLoad::Clear,
        store: false,
    };

    pub const CLEAR_STORED: Self = Self {
        load: DepthLoad::Clear,
        store: true,
    };

    pub const KEEP: Self = Self {
        load: DepthLoad::Keep,
        store: true,
    };

    pub const KEEP_DISCARDED: Self = Self {
        load: DepthLoad::Keep,
        store: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        assert!(
            x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0,
            "a viewport at ({x}, {y}) spans {width} by {height}"
        );
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn full(width: u32, height: u32) -> Self {
        Self::new(0.0, 0.0, width as f32, height as f32)
    }

    fn assert_within(self, width: u32, height: u32) {
        assert!(
            self.x + self.width <= width as f32 && self.y + self.height <= height as f32,
            "a viewport of {} by {} at ({}, {}) outruns a {width} by {height} target",
            self.width,
            self.height,
            self.x,
            self.y,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scissor {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Scissor {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        assert!(
            width > 0 && height > 0,
            "a scissor at ({x}, {y}) spans {width} by {height}"
        );
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn full(width: u32, height: u32) -> Self {
        Self::new(0, 0, width, height)
    }

    fn assert_within(self, width: u32, height: u32) {
        assert!(
            self.x
                .checked_add(self.width)
                .is_some_and(|right| right <= width)
                && self
                    .y
                    .checked_add(self.height)
                    .is_some_and(|bottom| bottom <= height),
            "a scissor of {} by {} at ({}, {}) outruns a {width} by {height} target",
            self.width,
            self.height,
            self.x,
            self.y,
        );
    }
}

pub struct Pass<'a> {
    pub color: Option<&'a Texture>,
    pub level: u32,
    pub clear: [f32; 4],
    pub depth: Option<(&'a Texture, DepthOps)>,
}

impl<'a> Pass<'a> {
    pub fn new(color: &'a Texture, clear: [f32; 4]) -> Self {
        assert!(
            !color.format().is_depth(),
            "a render pass paints into a colour attachment, not a depth one",
        );
        Self {
            color: Some(color),
            level: 0,
            clear,
            depth: None,
        }
    }

    pub fn depth_only(depth: &'a Texture) -> Self {
        assert!(
            depth.format().is_depth(),
            "a depth only pass writes into a depth attachment",
        );
        Self {
            color: None,
            level: 0,
            clear: [0.0; 4],
            depth: Some((depth, DepthOps::CLEAR_STORED)),
        }
    }

    pub fn depth_keep(depth: &'a Texture) -> Self {
        assert!(
            depth.format().is_depth(),
            "a depth only pass writes into a depth attachment",
        );
        Self {
            color: None,
            level: 0,
            clear: [0.0; 4],
            depth: Some((depth, DepthOps::KEEP)),
        }
    }

    pub fn level(self, level: u32) -> Self {
        Self { level, ..self }
    }

    pub fn depth(self, texture: &'a Texture, ops: DepthOps) -> Self {
        assert!(
            texture.format().is_depth(),
            "a render pass tests against a depth attachment",
        );
        Self {
            depth: Some((texture, ops)),
            ..self
        }
    }
}

#[derive(Clone, Copy)]
pub enum Count<'a> {
    Direct { vertices: u32, instances: u32 },
    Indirect(BufferBinding<'a>),
}

#[derive(Clone, Copy)]
pub struct Draw<'a> {
    pub pipeline: &'a GraphicsPipeline,
    pub groups: &'a [BindGroup],
    pub count: Count<'a>,
    pub viewport: Option<Viewport>,
    pub scissor: Option<Scissor>,
}

impl<'a> Draw<'a> {
    pub fn new(pipeline: &'a GraphicsPipeline, groups: &'a [BindGroup], vertices: u32) -> Self {
        Self {
            pipeline,
            groups,
            count: Count::Direct {
                vertices,
                instances: 1,
            },
            viewport: None,
            scissor: None,
        }
    }

    pub fn viewport(self, viewport: Viewport) -> Self {
        Self {
            viewport: Some(viewport),
            ..self
        }
    }

    pub fn scissor(self, scissor: Scissor) -> Self {
        Self {
            scissor: Some(scissor),
            ..self
        }
    }

    pub fn instances(self, instances: u32) -> Self {
        match self.count {
            Count::Direct { vertices, .. } => Self {
                count: Count::Direct {
                    vertices,
                    instances,
                },
                ..self
            },
            Count::Indirect(_) => panic!("an indirect draw counts its instances on the device"),
        }
    }

    pub fn indirect(
        pipeline: &'a GraphicsPipeline,
        groups: &'a [BindGroup],
        arguments: BufferBinding<'a>,
    ) -> Self {
        Self {
            pipeline,
            groups,
            count: Count::Indirect(arguments),
            viewport: None,
            scissor: None,
        }
    }
}

pub struct Submission {
    device: Device,
    encoder: wgpu::CommandEncoder,
    surface: Option<wgpu::SurfaceTexture>,
}

impl Submission {
    pub fn new(device: &Device, label: &str) -> Self {
        Self {
            device: device.clone(),
            encoder: device
                .native()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) }),
            surface: None,
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
            source.identity() != target.identity()
                || source_offset + bytes <= target_offset
                || target_offset + bytes <= source_offset,
            "a copy cannot overlap itself"
        );
        self.encoder.copy_buffer_to_buffer(
            source.native(),
            source_offset,
            target.native(),
            target_offset,
            bytes,
        );
    }

    pub fn clear(&mut self, buffer: &GpuBuffer, offset: u64, bytes: u64) {
        assert!(
            buffer.device().same(&self.device),
            "a clear crosses compute devices"
        );
        assert!(
            buffer.usage().contains(BufferUsages::COPY_DST),
            "a clear requires a copy destination"
        );
        assert!(offset.is_multiple_of(4) && bytes > 0 && bytes.is_multiple_of(4));
        assert!(
            offset
                .checked_add(bytes)
                .is_some_and(|end| end <= buffer.size())
        );
        self.encoder
            .clear_buffer(buffer.native(), offset, Some(bytes));
    }

    pub fn dispatch(&mut self, pipeline: &ComputePipeline, group: &BindGroup, groups: [u32; 3]) {
        assert!(
            pipeline.device().same(&self.device),
            "a pipeline crosses compute devices"
        );
        assert_eq!(
            pipeline.group_count(),
            1,
            "a compute dispatch binds one group"
        );
        assert!(group.group() == 0, "a compute dispatch binds group zero");
        assert_eq!(
            group.layout(),
            pipeline.group_layout(0),
            "a group belongs to another pipeline"
        );
        let limits = self.device.limits();
        assert!(
            groups
                .iter()
                .all(|count| *count > 0 && *count <= limits.max_compute_workgroups_per_dimension),
            "a dispatch exceeds the workgroup grid"
        );
        let mut pass = self
            .encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
        pass.set_pipeline(pipeline.native());
        pass.set_bind_group(0, &group.native, &[]);
        pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
    }

    pub fn render(&mut self, pass: Pass<'_>, draws: &[Draw<'_>]) {
        let Pass {
            color: target,
            level,
            clear,
            depth,
        } = pass;
        assert!(
            target.is_some() || depth.is_some(),
            "a render pass paints a colour or a depth attachment"
        );
        for held in target.iter() {
            assert!(
                held.device().same(&self.device),
                "a render pass crosses devices"
            );
            assert!(
                held.usages().contains(TextureUsages::RENDER_ATTACHMENT),
                "a render pass draws into a render attachment"
            );
            assert!(
                level < held.mip_count(),
                "a render pass draws into the level {level} of a {} level texture",
                held.mip_count(),
            );
        }
        if let Some((held, _)) = &depth {
            assert!(
                held.device().same(&self.device),
                "a render pass crosses devices"
            );
            assert!(
                held.usages().contains(TextureUsages::RENDER_ATTACHMENT),
                "a render pass tests against a render attachment"
            );
            assert!(
                held.format() == wgpu::TextureFormat::Depth32Float,
                "a render pass tests against a {:?} depth attachment",
                held.format(),
            );
        }
        let (width, height) = match (target, &depth) {
            (Some(held), _) => held.level_extent(level),
            (None, Some((held, _))) => (held.width(), held.height()),
            (None, None) => unreachable!("a render pass names an attachment"),
        };
        if let Some((held, _)) = &depth {
            assert!(
                held.level_extent(0) == (width, height),
                "a {width} by {height} pass loads a {} by {} depth texture",
                held.width(),
                held.height(),
            );
        }
        for draw in draws {
            let pipeline = draw.pipeline;
            assert!(
                pipeline.device().same(&self.device),
                "a graphics pipeline crosses devices"
            );
            assert_eq!(
                pipeline.format(),
                target.map(|held| held.format()),
                "a draw writes the {:?} format of its pipeline into a {:?} target",
                pipeline.format(),
                target.map(|held| held.format()),
            );
            assert_eq!(
                pipeline.depth(),
                depth.is_some(),
                "a draw of {} declares {} depth attachment where the pass holds {}",
                pipeline.label(),
                if pipeline.depth() { "a" } else { "no" },
                if depth.is_some() { "one" } else { "none" },
            );
            assert_eq!(
                draw.groups.len() as u32,
                pipeline.group_count(),
                "a draw binds every group of its pipeline"
            );
            for (index, group) in draw.groups.iter().enumerate() {
                let index = index as u32;
                assert_eq!(group.group(), index, "a draw binds its groups in order");
                assert_eq!(
                    group.layout(),
                    pipeline.group_layout(index),
                    "a bind group belongs to another pipeline"
                );
            }
            match draw.count {
                Count::Direct {
                    vertices,
                    instances,
                } => assert!(
                    vertices > 0 && instances > 0,
                    "a draw of no vertex or instance renders nothing"
                ),
                Count::Indirect(arguments) => {
                    assert!(
                        arguments.buffer.device().same(&self.device),
                        "an indirect draw crosses devices"
                    );
                    assert!(
                        arguments.buffer.usage().contains(BufferUsages::INDIRECT),
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
                }
            }
            let viewport = draw
                .viewport
                .unwrap_or_else(|| Viewport::full(width, height));
            viewport.assert_within(width, height);
            let scissor = draw.scissor.unwrap_or_else(|| Scissor::full(width, height));
            scissor.assert_within(width, height);
        }
        let color_view = target.map(|held| held.level_view(level));
        let depth_view = depth.map(|(held, _)| held.level_view(0));
        let color_attachments = color_view
            .as_ref()
            .map(|view| {
                vec![Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(clear[0]),
                            g: f64::from(clear[1]),
                            b: f64::from(clear[2]),
                            a: f64::from(clear[3]),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })]
            })
            .unwrap_or_default();
        let depth_attachment = depth_view.as_ref().zip(depth).map(|(view, (_, ops))| {
            wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: match ops.load {
                        DepthLoad::Clear => wgpu::LoadOp::Clear(1.0),
                        DepthLoad::Keep => wgpu::LoadOp::Load,
                    },
                    store: if ops.store {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                }),
                stencil_ops: None,
            }
        });
        let mut render = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &color_attachments,
            depth_stencil_attachment: depth_attachment,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for draw in draws {
            let pipeline = draw.pipeline;
            let viewport = draw
                .viewport
                .unwrap_or_else(|| Viewport::full(width, height));
            let scissor = draw.scissor.unwrap_or_else(|| Scissor::full(width, height));
            render.set_pipeline(pipeline.native());
            for (index, group) in draw.groups.iter().enumerate() {
                render.set_bind_group(index as u32, &group.native, &[]);
            }
            render.set_viewport(
                viewport.x,
                viewport.y,
                viewport.width,
                viewport.height,
                0.0,
                1.0,
            );
            render.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
            match draw.count {
                Count::Direct {
                    vertices,
                    instances,
                } => render.draw(0..vertices, 0..instances),
                Count::Indirect(arguments) => {
                    render.draw_indirect(arguments.buffer.native(), arguments.offset)
                }
            }
        }
    }

    pub fn write_texture(&mut self, texture: &Texture, level: u32, pixels: &[u8]) {
        let (width, height) = texture.level_extent(level);
        let region = TextureRegion::new(0, 0, width, height);
        let row = texture.level_row_bytes(level);
        self.write_texture_region(texture, level, region, row, pixels);
    }

    pub fn write_texture_region(
        &mut self,
        texture: &Texture,
        level: u32,
        region: TextureRegion,
        row_pitch: u64,
        pixels: &[u8],
    ) {
        assert!(
            texture.device().same(&self.device),
            "a texture upload crosses devices"
        );
        assert!(
            texture.usages().contains(TextureUsages::COPY_DST),
            "a texture upload requires a copy destination"
        );
        let (width, height) = texture.level_extent(level);
        region.assert_within(width, height);
        let texel = u64::from(texture.format().bytes_per_texel());
        let row = u64::from(region.width) * texel;
        assert!(
            row_pitch >= row && row_pitch.is_multiple_of(texel),
            "a region of {row} byte rows reads them {row_pitch} apart",
        );
        assert!(
            pixels.len() as u64 >= row_pitch * u64::from(region.height - 1) + row,
            "a region {region:?} upload writes {} bytes where {} come in at a {row_pitch} byte pitch",
            row_pitch * u64::from(region.height - 1) + row,
            pixels.len(),
        );
        self.device.native_queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: texture.native(),
                mip_level: level,
                origin: wgpu::Origin3d {
                    x: region.x,
                    y: region.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_pitch as u32),
                rows_per_image: Some(region.height),
            },
            wgpu::Extent3d {
                width: region.width,
                height: region.height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn copy_texture(&mut self, texture: &Texture, target: &GpuBuffer, target_offset: u64) {
        assert!(
            texture.device().same(&self.device) && target.device().same(&self.device),
            "a texture readback crosses devices"
        );
        assert!(
            target.usage().contains(BufferUsages::COPY_DST),
            "a texture readback requires a copy destination"
        );
        assert!(
            texture.usages().contains(TextureUsages::COPY_SRC),
            "a texture readback requires a copy source"
        );
        assert!(
            target_offset.is_multiple_of(Texture::READBACK_ALIGNMENT),
            "a texture readback starts on a 256 byte boundary"
        );
        assert!(
            target_offset
                .checked_add(texture.readback_bytes())
                .is_some_and(|end| end <= target.size()),
            "a texture of {} bytes outruns its readback buffer",
            texture.readback_bytes()
        );
        let (width, height) = texture.level_extent(0);
        self.encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: texture.native(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: target.native(),
                layout: wgpu::TexelCopyBufferLayout {
                    offset: target_offset,
                    bytes_per_row: Some(texture.level_row_stride(0) as u32),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn present(&mut self, mut frame: SurfaceFrame) {
        assert!(
            self.surface.is_none(),
            "a submission presents one surface frame"
        );
        assert!(
            frame.device().same(&self.device),
            "a submission presents another device's surface frame"
        );
        assert!(
            frame.token.valid(),
            "a surface frame outlived the swapchain it was acquired from",
        );
        self.surface = frame.surface.take();
    }

    pub fn submit(self, queue: &Queue) -> SubmissionIndex {
        assert!(
            self.device.same(queue.device()),
            "a submission crosses compute devices"
        );
        let Self {
            device,
            encoder,
            surface,
        } = self;
        let index = queue.native().submit([encoder.finish()]);
        if let Some(surface) = surface {
            queue.native().present(surface);
        }
        device.cache().begin_frame();
        index
    }
}
