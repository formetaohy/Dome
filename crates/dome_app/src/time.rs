use std::time::Duration;

const MAX_FRAME: Duration = Duration::from_millis(250);

pub struct Time {
    delta: Duration,
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
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            fixed,
            accumulator: Duration::ZERO,
            alpha: 0.0,
            frame: 0,
        }
    }

    pub fn delta(&self) -> Duration {
        self.delta
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
        let real = real.min(MAX_FRAME);
        self.delta = real;
        self.elapsed += real;
        self.accumulator += real;
        let steps =
            (self.accumulator.as_nanos() / self.fixed.as_nanos()).min(u128::from(max_steps));
        if steps == u128::from(max_steps) {
            self.accumulator = self
                .fixed
                .checked_mul(max_steps)
                .expect("a fixed step budget spans the clock");
        }
        steps as u32
    }

    pub(crate) fn step(&mut self) {
        self.accumulator = self.accumulator.saturating_sub(self.fixed);
    }

    pub(crate) fn settle(&mut self) {
        self.alpha = self.accumulator.as_secs_f32() / self.fixed.as_secs_f32();
        self.frame += 1;
    }
}
