use crate::sampler::Sampler;

pub struct Image {
    held: dome_image::Image,
    sampler: Sampler,
}

impl Image {
    pub(crate) fn decode(bytes: &[u8], what: &str, sampler: Sampler) -> Self {
        Self {
            held: dome_image::Image::decode(bytes, what).unwrap_or_else(|error| panic!("{error}")),
            sampler,
        }
    }

    pub fn width(&self) -> u32 {
        self.held.width()
    }

    pub fn height(&self) -> u32 {
        self.held.height()
    }

    pub fn rgba(&self) -> &[u8] {
        self.held.rgba()
    }

    pub fn sampler(&self) -> Sampler {
        self.sampler
    }
}
