use crate::buffer::BufferBinding;
use crate::depth::DepthTexture;
use crate::pipeline::{BindGroup, GraphicsPipeline};
use crate::texture::Texture;

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

    pub(crate) fn assert_within(self, width: u32, height: u32) {
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

    pub(crate) fn assert_within(self, width: u32, height: u32) {
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

    pub const COVERED: Self = Self {
        depth: DepthState::TestCovered,
        blend: BlendState::Opaque,
        alpha: true,
    };
}

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
        store: false,
    };
}

pub struct Pass<'a> {
    pub color: &'a Texture,
    pub level: u32,
    pub clear: [f32; 4],
    pub depth: Option<(&'a DepthTexture, DepthOps)>,
}

impl<'a> Pass<'a> {
    pub fn new(color: &'a Texture, clear: [f32; 4]) -> Self {
        Self {
            color,
            level: 0,
            clear,
            depth: None,
        }
    }

    pub fn level(self, level: u32) -> Self {
        Self { level, ..self }
    }

    pub fn depth(self, texture: &'a DepthTexture, ops: DepthOps) -> Self {
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
