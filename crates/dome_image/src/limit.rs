#[derive(Clone, Copy)]
pub struct DecodeLimit {
    texels: usize,
    extent: u32,
}

impl DecodeLimit {
    pub const DEFAULT: Self = Self::of(16_777_216, 16_384);

    pub const fn of(texels: usize, extent: u32) -> Self {
        Self { texels, extent }
    }

    pub(crate) fn guard(self, width: u32, height: u32, what: &str) -> Result<usize, String> {
        if width == 0 || height == 0 {
            return Err(format!(
                "{what} spans no texel of its {width} by {height} picture",
            ));
        }
        if width > self.extent || height > self.extent {
            return Err(format!(
                "{what} spans a {width} by {height} picture where a texture holds {} texels a side",
                self.extent,
            ));
        }
        let texels = u64::from(width) * u64::from(height);
        if texels > self.texels as u64 {
            return Err(format!(
                "{what} spans {texels} texels where a decode holds at most {}",
                self.texels,
            ));
        }
        Ok(texels as usize)
    }
}
