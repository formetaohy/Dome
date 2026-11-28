use dome_math::{Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Property {
    Translation,
    Rotation,
    Scale,
}

impl Property {
    pub fn components(self) -> usize {
        match self {
            Self::Rotation => 4,
            Self::Translation | Self::Scale => 3,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Translation => "translation",
            Self::Rotation => "rotation",
            Self::Scale => "scale",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Step,
    Linear,
    CubicSpline,
}

impl Interpolation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Step => "step",
            Self::Linear => "linear",
            Self::CubicSpline => "cubic_spline",
        }
    }

    fn stride(self) -> usize {
        match self {
            Self::CubicSpline => 3,
            Self::Step | Self::Linear => 1,
        }
    }

    fn value_slot(self, key: usize) -> usize {
        match self {
            Self::CubicSpline => key * 3 + 1,
            Self::Step | Self::Linear => key,
        }
    }

    fn out_slot(self, key: usize) -> usize {
        key * 3 + 2
    }

    fn in_slot(self, key: usize) -> usize {
        key * 3
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    target: String,
    property: Property,
    interpolation: Interpolation,
    times: Vec<f32>,
    values: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sample {
    Vector(Vec3),
    Rotation(Quat),
}

impl Track {
    pub fn new(
        target: impl Into<String>,
        property: Property,
        interpolation: Interpolation,
        times: Vec<f32>,
        values: Vec<f32>,
    ) -> Result<Self, String> {
        let target = target.into();
        if target.is_empty() {
            return Err(format!(
                "an animation track of {property:?} targets no node"
            ));
        }
        if times.is_empty() {
            return Err(format!("the track of {target} holds no key"));
        }
        if !times.iter().all(|time| time.is_finite()) {
            return Err(format!(
                "the track of {target} holds a time that is no finite number"
            ));
        }
        if !times.windows(2).all(|window| window[0] < window[1]) {
            return Err(format!("the track of {target} holds times out of order"));
        }
        let components = property.components();
        let expected = times.len() * components * interpolation.stride();
        if values.len() != expected {
            return Err(format!(
                "the track of {target} holds {} values where {expected} stand",
                values.len(),
            ));
        }
        if !values.iter().all(|value| value.is_finite()) {
            return Err(format!(
                "the track of {target} holds a value that is no finite number"
            ));
        }
        let mut track = Self {
            target,
            property,
            interpolation,
            times,
            values,
        };
        if property == Property::Rotation {
            track.normalize_rotations();
        }
        Ok(track)
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn property(&self) -> Property {
        self.property
    }

    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }

    pub fn sample(&self, time: f32) -> Sample {
        assert!(
            time.is_finite(),
            "the track of {} samples no finite time",
            self.target,
        );
        if time <= self.times[0] {
            return self.sample_at(0);
        }
        let last = self.times.len() - 1;
        if time >= self.times[last] {
            return self.sample_at(last);
        }
        let next = self.times.partition_point(|held| *held <= time);
        self.segment(next - 1, next, time)
    }

    fn sample_at(&self, key: usize) -> Sample {
        let components = self.property.components();
        let slot = self.interpolation.value_slot(key);
        self.decode(slot * components)
    }

    fn segment(&self, first: usize, second: usize, time: f32) -> Sample {
        let components = self.property.components();
        let start = self.times[first];
        let end = self.times[second];
        let span = end - start;
        assert!(
            span > 0.0,
            "the track of {} holds two keys at one time",
            self.target,
        );
        let phase = (time - start) / span;
        match self.interpolation {
            Interpolation::Step => self.sample_at(first),
            Interpolation::Linear => {
                let a = self.interpolation.value_slot(first) * components;
                let b = self.interpolation.value_slot(second) * components;
                self.linear(a, b, phase)
            }
            Interpolation::CubicSpline => {
                let a = self.interpolation.value_slot(first) * components;
                let b = self.interpolation.value_slot(second) * components;
                let out = self.interpolation.out_slot(first) * components;
                let incoming = self.interpolation.in_slot(second) * components;
                self.cubic(a, b, out, incoming, phase, span)
            }
        }
    }

    fn linear(&self, a: usize, b: usize, phase: f32) -> Sample {
        if self.property == Property::Rotation {
            let first = self.quaternion(a);
            let second = self.quaternion(b);
            return Sample::Rotation(first.slerp(second, phase));
        }
        let first = self.vector(a);
        let second = self.vector(b);
        Sample::Vector(first * (1.0 - phase) + second * phase)
    }

    fn cubic(
        &self,
        a: usize,
        b: usize,
        out: usize,
        incoming: usize,
        phase: f32,
        span: f32,
    ) -> Sample {
        let phase2 = phase * phase;
        let phase3 = phase2 * phase;
        let h00 = 2.0 * phase3 - 3.0 * phase2 + 1.0;
        let h10 = phase3 - 2.0 * phase2 + phase;
        let h01 = -2.0 * phase3 + 3.0 * phase2;
        let h11 = phase3 - phase2;
        let mix = |index: usize| {
            h00 * self.values[a + index]
                + h10 * span * self.values[out + index]
                + h01 * self.values[b + index]
                + h11 * span * self.values[incoming + index]
        };
        if self.property == Property::Rotation {
            let values = [mix(0), mix(1), mix(2), mix(3)];
            let rotation = Quat::new(values[0], values[1], values[2], values[3]);
            return Sample::Rotation(rotation.normalize());
        }
        let values = [mix(0), mix(1), mix(2)];
        Sample::Vector(Vec3::new(values[0], values[1], values[2]))
    }

    fn decode(&self, at: usize) -> Sample {
        if self.property == Property::Rotation {
            Sample::Rotation(self.quaternion(at))
        } else {
            Sample::Vector(self.vector(at))
        }
    }

    fn vector(&self, at: usize) -> Vec3 {
        Vec3::new(self.values[at], self.values[at + 1], self.values[at + 2])
    }

    fn quaternion(&self, at: usize) -> Quat {
        Quat::new(
            self.values[at],
            self.values[at + 1],
            self.values[at + 2],
            self.values[at + 3],
        )
    }

    fn normalize_rotations(&mut self) {
        let components = self.property.components();
        for key in 0..self.times.len() {
            let at = self.interpolation.value_slot(key) * components;
            let rotation = self.quaternion(at);
            let normalized = rotation.normalize();
            self.values[at] = normalized.x;
            self.values[at + 1] = normalized.y;
            self.values[at + 2] = normalized.z;
            self.values[at + 3] = normalized.w;
        }
    }

    fn duration(&self) -> f32 {
        self.times[self.times.len() - 1]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    tracks: Vec<Track>,
    duration: f32,
}

impl Clip {
    pub fn new(tracks: Vec<Track>) -> Self {
        let duration = tracks.iter().map(Track::duration).fold(0.0, f32::max);
        Self { tracks, duration }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}
