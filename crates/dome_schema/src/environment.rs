use dome_scene::EnvironmentId;

#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentRef {
    pub environment: EnvironmentId,
    pub intensity: f32,
    pub sky: bool,
    pub local: bool,
    pub blend: f32,
}

impl EnvironmentRef {
    pub fn new(environment: EnvironmentId) -> Self {
        Self {
            environment,
            intensity: 1.0,
            sky: true,
            local: false,
            blend: 1.0,
        }
    }

    pub fn probe(mut self, blend: f32) -> Self {
        self.local = true;
        self.blend = blend;
        self
    }
}
