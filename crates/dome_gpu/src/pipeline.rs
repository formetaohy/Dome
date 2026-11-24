use crate::binding::{BindGroup, BindGroupLayout, Binding, bind_group, pipeline_layout};
use crate::context::Device;
use crate::identity::ResourceId;
use crate::shader::{ComputeShader, GraphicsShader};
use std::sync::Arc;
use wgpu::{CompareFunction, TextureFormat};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthState {
    TestWrite,
    TestCovered,
    Test,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendState {
    Opaque,
    Premultiplied,
    Preserve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineState {
    pub depth: DepthState,
    pub blend: BlendState,
    pub alpha: bool,
}

impl PipelineState {
    pub const DEPTH_WRITE: Self = Self {
        depth: DepthState::TestWrite,
        blend: BlendState::Opaque,
        alpha: true,
    };

    pub const SCREEN: Self = Self {
        depth: DepthState::Off,
        blend: BlendState::Opaque,
        alpha: true,
    };

    pub const OVERLAY: Self = Self {
        depth: DepthState::Off,
        blend: BlendState::Premultiplied,
        alpha: true,
    };

    pub const DEBUG: Self = Self {
        depth: DepthState::Off,
        blend: BlendState::Premultiplied,
        alpha: false,
    };

    pub const TRANSPARENT: Self = Self {
        depth: DepthState::Test,
        blend: BlendState::Preserve,
        alpha: false,
    };

    pub const TESTED: Self = Self {
        depth: DepthState::Test,
        blend: BlendState::Opaque,
        alpha: true,
    };

    pub const COVERED: Self = Self {
        depth: DepthState::TestCovered,
        blend: BlendState::Opaque,
        alpha: true,
    };
}

pub(crate) struct PipelineLayout {
    pub(crate) native: wgpu::PipelineLayout,
    pub(crate) groups: Vec<Arc<BindGroupLayout>>,
}

#[derive(Clone)]
pub struct GraphicsPipeline {
    device: Device,
    native: wgpu::RenderPipeline,
    layout: Arc<PipelineLayout>,
    shader: &'static GraphicsShader,
    format: Option<TextureFormat>,
    depth: bool,
}

#[derive(Clone)]
pub struct ComputePipeline {
    device: Device,
    native: wgpu::ComputePipeline,
    layout: Arc<PipelineLayout>,
    shader: &'static ComputeShader,
}

impl GraphicsPipeline {
    pub fn new(
        device: &Device,
        shader: &'static GraphicsShader,
        format: TextureFormat,
        state: PipelineState,
    ) -> Self {
        Self::layered(device, shader, Some(format), state, false)
    }

    pub fn new_depth(
        device: &Device,
        shader: &'static GraphicsShader,
        format: TextureFormat,
        state: PipelineState,
    ) -> Self {
        Self::layered(device, shader, Some(format), state, true)
    }

    pub fn new_depth_only(
        device: &Device,
        shader: &'static GraphicsShader,
        state: PipelineState,
    ) -> Self {
        Self::layered(device, shader, None, state, true)
    }

    fn layered(
        device: &Device,
        shader: &'static GraphicsShader,
        format: Option<TextureFormat>,
        state: PipelineState,
        depth: bool,
    ) -> Self {
        assert_eq!(
            shader.targets,
            usize::from(format.is_some()),
            "the graphics program {} writes {} fragment targets into {} colour attachment",
            shader.label,
            shader.targets,
            if format.is_some() { "one" } else { "no" },
        );
        let layout = Arc::new(pipeline_layout(device, shader.bindings, shader.label, true));
        let module = device
            .cache()
            .shader(device.native(), shader.label, shader.source);
        let targets = format
            .map(|format| {
                vec![Some(wgpu::ColorTargetState {
                    format,
                    blend: match state.blend {
                        BlendState::Opaque => None,
                        other => Some(blend(other)),
                    },
                    write_mask: if state.alpha {
                        wgpu::ColorWrites::ALL
                    } else {
                        wgpu::ColorWrites::COLOR
                    },
                })]
            })
            .unwrap_or_default();
        let depth_stencil = depth.then(|| wgpu::DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: Some(matches!(
                state.depth,
                DepthState::TestWrite | DepthState::TestCovered
            )),
            depth_compare: Some(match state.depth {
                DepthState::Off => CompareFunction::Always,
                DepthState::TestCovered => CompareFunction::LessEqual,
                _ => CompareFunction::Less,
            }),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        let native = device
            .native()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(shader.label),
                layout: Some(&layout.native),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(shader.vertex),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(shader.fragment),
                    targets: &targets,
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        Self {
            device: device.clone(),
            native,
            layout,
            shader,
            format,
            depth,
        }
    }

    pub fn format(&self) -> Option<TextureFormat> {
        self.format
    }

    pub fn label(&self) -> &'static str {
        self.shader.label
    }

    pub fn bind_group(&self, group: u32, entries: &[Binding<'_>]) -> BindGroup {
        let layout = self.layout.groups.get(group as usize).unwrap_or_else(|| {
            panic!(
                "the graphics program {} declares {} bind groups where {group} is requested",
                self.shader.label,
                self.layout.groups.len(),
            )
        });
        bind_group(
            &self.device,
            self.device.cache(),
            layout,
            group,
            entries,
            self.shader.label,
        )
    }

    pub(crate) fn native(&self) -> &wgpu::RenderPipeline {
        &self.native
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    pub(crate) fn depth(&self) -> bool {
        self.depth
    }

    pub(crate) fn group_layout(&self, group: u32) -> ResourceId {
        self.layout.groups[group as usize].id
    }

    pub(crate) fn group_count(&self) -> u32 {
        self.layout.groups.len() as u32
    }
}

impl ComputePipeline {
    pub fn new(device: &Device, shader: &'static ComputeShader) -> Self {
        crate::shader::group_count(shader.bindings, shader.label, true);
        assert!(
            shader.workgroup <= device.limits().max_compute_workgroup_size_x
                && shader.workgroup <= device.limits().max_compute_invocations_per_workgroup,
            "the kernel {} dispatches {} invocations the device cannot run",
            shader.label,
            shader.workgroup,
        );
        let layout = Arc::new(pipeline_layout(
            device,
            shader.bindings,
            shader.label,
            false,
        ));
        let module = device
            .cache()
            .shader(device.native(), shader.label, shader.source);
        let native = device
            .native()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(shader.label),
                layout: Some(&layout.native),
                module: &module,
                entry_point: Some(shader.entry),
                compilation_options: Default::default(),
                cache: None,
            });
        Self {
            device: device.clone(),
            native,
            layout,
            shader,
        }
    }

    pub fn label(&self) -> &'static str {
        self.shader.label
    }

    pub fn bind_group(&self, group: u32, entries: &[Binding<'_>]) -> BindGroup {
        let layout = self.layout.groups.get(group as usize).unwrap_or_else(|| {
            panic!(
                "the kernel {} declares {} bind groups where {group} is requested",
                self.shader.label,
                self.layout.groups.len(),
            )
        });
        bind_group(
            &self.device,
            self.device.cache(),
            layout,
            group,
            entries,
            self.shader.label,
        )
    }

    pub(crate) fn native(&self) -> &wgpu::ComputePipeline {
        &self.native
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    pub(crate) fn group_layout(&self, group: u32) -> ResourceId {
        self.layout.groups[group as usize].id
    }

    pub(crate) fn group_count(&self) -> u32 {
        self.layout.groups.len() as u32
    }
}

fn blend(state: BlendState) -> wgpu::BlendState {
    match state {
        BlendState::Opaque => wgpu::BlendState::REPLACE,
        BlendState::Premultiplied => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },
        BlendState::Preserve => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },
    }
}
