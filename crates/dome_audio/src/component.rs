use crate::source::Source;

#[derive(Clone)]
pub struct AudioSource {
    name: String,
    source: Source,
    bus: String,
    gain: f32,
    looping: bool,
    spatial: bool,
    min_distance: f32,
    max_distance: f32,
    pitch: f32,
    priority: u8,
    playing: bool,
    generation: u64,
}

impl AudioSource {
    pub fn new(name: impl Into<String>, source: Source) -> Self {
        Self {
            name: name.into(),
            source,
            bus: String::new(),
            gain: 1.0,
            looping: false,
            spatial: true,
            min_distance: 1.0,
            max_distance: 50.0,
            pitch: 1.0,
            priority: 0,
            playing: false,
            generation: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn set_source(&mut self, source: Source) {
        self.source = source;
    }

    pub fn priority(&self) -> u8 {
        self.priority
    }

    pub fn set_priority(&mut self, priority: u8) {
        self.priority = priority;
    }

    pub fn bus(&self) -> &str {
        &self.bus
    }

    pub fn set_bus(&mut self, bus: impl Into<String>) {
        self.bus = bus.into();
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn set_gain(&mut self, gain: f32) {
        assert!(
            gain.is_finite() && gain >= 0.0,
            "an audio source of gain {gain} plays silence or less",
        );
        self.gain = gain;
    }

    pub fn looping(&self) -> bool {
        self.looping
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    pub fn spatial(&self) -> bool {
        self.spatial
    }

    pub fn set_spatial(&mut self, spatial: bool) {
        self.spatial = spatial;
    }

    pub fn min_distance(&self) -> f32 {
        self.min_distance
    }

    pub fn max_distance(&self) -> f32 {
        self.max_distance
    }

    pub fn set_distances(&mut self, min_distance: f32, max_distance: f32) {
        assert!(
            min_distance.is_finite() && min_distance > 0.0,
            "an audio source of {min_distance} units starts its falloff nowhere",
        );
        assert!(
            max_distance.is_finite() && max_distance > min_distance,
            "an audio source of {min_distance} to {max_distance} units falls off nowhere",
        );
        self.min_distance = min_distance;
        self.max_distance = max_distance;
    }

    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        assert!(
            pitch.is_finite() && pitch > 0.0,
            "an audio source of pitch {pitch} holds no frequency",
        );
        self.pitch = pitch;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn play(&mut self) {
        self.playing = true;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn finished(&mut self) {
        self.playing = false;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioListener;
