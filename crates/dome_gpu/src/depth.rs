use crate::context::Device;
use crate::native::NativeDepth;

pub struct DepthTexture {
    native: NativeDepth,
    width: u32,
    height: u32,
}

impl Clone for DepthTexture {
    fn clone(&self) -> Self {
        Self {
            native: self.native.clone(),
            width: self.width,
            height: self.height,
        }
    }
}

impl DepthTexture {
    pub fn new(device: &Device, label: &str, width: u32, height: u32) -> Self {
        assert!(
            width > 0 && height > 0,
            "a {width} by {height} depth texture holds no texel",
        );
        Self {
            native: device.native().create_depth_texture(label, width, height),
            width,
            height,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn native(&self) -> &NativeDepth {
        &self.native
    }
}
