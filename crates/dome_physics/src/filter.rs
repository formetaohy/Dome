pub const LAYER_COUNT: u16 = 16;
pub const ALL_LAYERS: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionFilter {
    layer: u16,
    mask: u16,
}

impl CollisionFilter {
    pub const DEFAULT: Self = Self {
        layer: 0,
        mask: ALL_LAYERS,
    };

    pub fn new(layer: u16, mask: u16) -> Self {
        assert!(
            layer < LAYER_COUNT,
            "a collision layer {layer} outruns the {LAYER_COUNT} layers",
        );
        Self { layer, mask }
    }

    pub fn layer(self) -> u16 {
        self.layer
    }

    pub fn mask(self) -> u16 {
        self.mask
    }

    pub fn set_layer(&mut self, layer: u16) {
        assert!(
            layer < LAYER_COUNT,
            "a collision layer {layer} outruns the {LAYER_COUNT} layers",
        );
        self.layer = layer;
    }

    pub fn set_mask(&mut self, mask: u16) {
        self.mask = mask;
    }

    pub fn bit(self) -> u16 {
        1 << self.layer
    }

    pub fn accepts(self, other: Self) -> bool {
        self.bit() & other.mask != 0 && other.bit() & self.mask != 0
    }
}

impl Default for CollisionFilter {
    fn default() -> Self {
        Self::DEFAULT
    }
}
