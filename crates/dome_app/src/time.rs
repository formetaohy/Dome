use std::time::Duration;

const MAX_FRAME: Duration = Duration::from_millis(250);

pub struct Time {
    frame_delta: Duration,
    running_delta: Duration,
    elapsed: Duration,
    fixed: Duration,
    accumulator: Duration,
    alpha: f32,
    frame: u64,
}

impl Time {
    pub fn new(fixed_hz: f64) -> Self {
        assert!(
            fixed_hz.is_finite() && fixed_hz > 0.0,
            "a fixed timestep of {fixed_hz} hertz steps no time"
        );
        let fixed = Duration::from_secs_f64(1.0 / fixed_hz);
        assert!(
            fixed > Duration::ZERO,
            "a fixed timestep of {fixed_hz} hertz rounds to no time"
        );
        Self {
            frame_delta: Duration::ZERO,
            running_delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            fixed,
            accumulator: Duration::ZERO,
            alpha: 0.0,
            frame: 0,
        }
    }

    pub fn delta(&self) -> Duration {
        self.running_delta
    }

    pub fn frame_delta(&self) -> Duration {
        self.frame_delta
    }

    pub fn fixed_delta(&self) -> Duration {
        self.fixed
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub(crate) fn advance(&mut self, real: Duration, max_steps: u32) -> u32 {
        self.elapsed += real;
        self.frame_delta = real.min(MAX_FRAME);
        self.running_delta = self.frame_delta;
        let budget = self
            .fixed
            .checked_mul(max_steps)
            .expect("a fixed step budget spans the clock");
        self.accumulator = (self.accumulator + self.frame_delta).min(budget);
        let steps = self.accumulator.as_nanos() / self.fixed.as_nanos();
        u32::try_from(steps).expect("a clamped accumulator spans its step budget")
    }

    pub(crate) fn enter_fixed(&mut self) {
        self.running_delta = self.fixed;
    }

    pub(crate) fn enter_update(&mut self) {
        self.running_delta = self.frame_delta;
    }

    pub(crate) fn consume(&mut self) {
        self.accumulator = self
            .accumulator
            .checked_sub(self.fixed)
            .expect("a stepped step draws on the accumulator it counted");
    }

    pub(crate) fn settle(&mut self) {
        self.alpha = self.accumulator.as_secs_f32() / self.fixed.as_secs_f32();
        self.frame += 1;
    }
}
