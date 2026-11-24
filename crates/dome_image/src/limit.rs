#[derive(Clone, Copy)]
pub struct DecodeLimit {
    texels: usize,
}

impl DecodeLimit {
    pub const DEFAULT: Self = Self::texels(16_777_216);

    pub const fn texels(texels: usize) -> Self {
        Self { texels }
    }

    pub(crate) fn guard(self, width: u32, height: u32, what: &str) -> Result<usize, String> {
        if width == 0 || height == 0 {
            return Err(format!(
                "{what} spans no texel of its {width} by {height} picture",
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
