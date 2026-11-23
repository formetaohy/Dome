use crate::ClipSource;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
struct Fade {
    from: f32,
    to: f32,
    duration: f32,
    elapsed: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct AnimationLayer {
    source: Arc<ClipSource>,
    time: f32,
    speed: f32,
    looped: bool,
    playing: bool,
    weight: f32,
    fade: Option<Fade>,
    finished: bool,
}

impl AnimationLayer {
    pub fn new(
        source: Arc<ClipSource>,
        time: f32,
        speed: f32,
        looped: bool,
        playing: bool,
        weight: f32,
    ) -> Self {
        assert!(time.is_finite(), "an animation time of {time} is finite");
        assert!(speed.is_finite(), "an animation speed of {speed} is finite");
        assert!(
            weight.is_finite() && weight >= 0.0,
            "an animation weight of {weight} is no finite non-negative number",
        );
        let mut layer = Self {
            source,
            time,
            speed,
            looped,
            playing,
            weight,
            fade: None,
            finished: false,
        };
        layer.set_time(time);
        layer
    }

    pub fn source(&self) -> &ClipSource {
        &self.source
    }

    pub(crate) fn rebase(&mut self, source: Arc<ClipSource>, speed: f32, looped: bool) {
        assert!(speed.is_finite(), "an animation speed of {speed} is finite");
        let time = self.time;
        self.source = source;
        self.speed = speed;
        self.looped = looped;
        self.set_time(time);
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn set_time(&mut self, time: f32) {
        assert!(time.is_finite(), "an animation time of {time} is finite");
        let duration = self.source.clip().duration();
        self.time = if self.looped && duration > 0.0 {
            time.rem_euclid(duration)
        } else {
            time
        };
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    pub fn weight(&self) -> f32 {
        self.weight
    }

    pub fn fade_to(&mut self, weight: f32, duration: f32) {
        assert!(
            weight.is_finite() && weight >= 0.0,
            "an animation weight of {weight} is no finite non-negative number",
        );
        assert!(
            duration.is_finite() && duration >= 0.0,
            "an animation fade of {duration} is no finite non-negative span",
        );
        if duration == 0.0 {
            self.weight = weight;
            self.fade = None;
            return;
        }
        self.fade = Some(Fade {
            from: self.weight,
            to: weight,
            duration,
            elapsed: 0.0,
        });
    }

    pub(crate) fn advance(&mut self, delta: f32) -> bool {
        self.finished = false;
        let mut changed = false;
        if let Some(mut fade) = self.fade {
            let step = delta.max(0.0);
            if step > 0.0 {
                changed = true;
                fade.elapsed = (fade.elapsed + step).min(fade.duration);
                let phase = fade.elapsed / fade.duration;
                self.weight = fade.from + (fade.to - fade.from) * phase;
                if fade.elapsed >= fade.duration {
                    self.weight = fade.to;
                    self.fade = None;
                } else {
                    self.fade = Some(fade);
                }
            }
        }
        if !self.playing || delta == 0.0 {
            return changed;
        }
        let time = self.time + delta * self.speed;
        let duration = self.source.clip().duration();
        if duration <= 0.0 {
            self.time = 0.0;
            self.finished = true;
            return true;
        }
        if self.looped {
            self.finished = time >= duration || time < 0.0;
            self.time = time.rem_euclid(duration);
            changed |= self.speed != 0.0;
        } else if time >= duration {
            self.time = duration;
            self.playing = false;
            self.finished = true;
            changed = true;
        } else if time < 0.0 {
            self.time = 0.0;
            self.playing = false;
            self.finished = true;
            changed = true;
        } else {
            self.time = time;
            changed |= self.speed != 0.0;
        }
        changed
    }

    pub(crate) fn finished(&self) -> bool {
        self.finished
    }

    pub(crate) fn retiring(&self) -> bool {
        self.weight <= 0.0 && self.fade.is_none_or(|fade| fade.to <= 0.0)
    }
}
