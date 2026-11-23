#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    LowPass,
    HighPass,
    BandPass,
    Peaking,
    LowShelf,
    HighShelf,
}

impl FilterKind {
    pub const ALL: [Self; 6] = [
        Self::LowPass,
        Self::HighPass,
        Self::BandPass,
        Self::Peaking,
        Self::LowShelf,
        Self::HighShelf,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::LowPass => "low_pass",
            Self::HighPass => "high_pass",
            Self::BandPass => "band_pass",
            Self::Peaking => "peaking",
            Self::LowShelf => "low_shelf",
            Self::HighShelf => "high_shelf",
        }
    }

    pub fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioBus {
    id: String,
    parent: Option<String>,
    send: Option<String>,
    gain_db: f32,
    mute: bool,
    send_db: f32,
}

impl AudioBus {
    pub fn new(
        id: impl Into<String>,
        parent: Option<String>,
        send: Option<String>,
        gain_db: f32,
        mute: bool,
        send_db: f32,
    ) -> Self {
        let bus = Self {
            id: id.into(),
            parent,
            send,
            gain_db,
            mute,
            send_db,
        };
        assert!(!bus.id.is_empty(), "an audio bus holds a name");
        assert!(
            bus.parent.as_ref().is_none_or(|parent| !parent.is_empty()),
            "the audio bus {} parents to no name",
            bus.id,
        );
        assert!(
            bus.send.as_ref().is_none_or(|send| !send.is_empty()),
            "the audio bus {} sends to no name",
            bus.id,
        );
        bus.check_gain(gain_db);
        bus.check_send(send_db);
        bus
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    pub fn send(&self) -> Option<&str> {
        self.send.as_deref()
    }

    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    pub fn mute(&self) -> bool {
        self.mute
    }

    pub fn send_db(&self) -> f32 {
        self.send_db
    }

    pub fn set_gain_db(&mut self, gain_db: f32) {
        self.check_gain(gain_db);
        self.gain_db = gain_db;
    }

    pub fn set_mute(&mut self, mute: bool) {
        self.mute = mute;
    }

    pub fn set_send_db(&mut self, send_db: f32) {
        self.check_send(send_db);
        self.send_db = send_db;
    }

    fn check_gain(&self, gain_db: f32) {
        assert!(
            gain_db.is_finite() && (-60.0..=12.0).contains(&gain_db),
            "the audio bus {} holds the gain {gain_db} decibels",
            self.id,
        );
    }

    fn check_send(&self, send_db: f32) {
        assert!(
            send_db.is_finite() && (-60.0..=0.0).contains(&send_db),
            "the audio bus {} holds the send {send_db} decibels",
            self.id,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioFilter {
    enabled: bool,
    kind: FilterKind,
    frequency: f32,
    q: f32,
    gain_db: f32,
}

impl Default for AudioFilter {
    fn default() -> Self {
        Self {
            enabled: true,
            kind: FilterKind::LowPass,
            frequency: 20_000.0,
            q: 0.707,
            gain_db: 0.0,
        }
    }
}

impl AudioFilter {
    pub fn new(enabled: bool, kind: FilterKind, frequency: f32, q: f32, gain_db: f32) -> Self {
        let filter = Self {
            enabled,
            kind,
            frequency,
            q,
            gain_db,
        };
        filter.check();
        filter
    }

    pub(crate) fn bypassed() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn kind(&self) -> FilterKind {
        self.kind
    }

    pub fn frequency(&self) -> f32 {
        self.frequency
    }

    pub fn q(&self) -> f32 {
        self.q
    }

    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_kind(&mut self, kind: FilterKind) {
        self.kind = kind;
    }

    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
        self.check();
    }

    pub fn set_q(&mut self, q: f32) {
        self.q = q;
        self.check();
    }

    pub fn set_gain_db(&mut self, gain_db: f32) {
        self.gain_db = gain_db;
        self.check();
    }

    fn check(&self) {
        assert!(
            self.frequency.is_finite() && (20.0..=20_000.0).contains(&self.frequency),
            "the audio filter holds the frequency {}",
            self.frequency,
        );
        assert!(
            self.q.is_finite() && (0.1..=10.0).contains(&self.q),
            "the audio filter holds the q {}",
            self.q,
        );
        assert!(
            self.gain_db.is_finite() && (-24.0..=24.0).contains(&self.gain_db),
            "the audio filter holds the gain {} decibels",
            self.gain_db,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioCompressor {
    enabled: bool,
    threshold_db: f32,
    ratio: f32,
    attack_ms: f32,
    release_ms: f32,
    makeup_db: f32,
}

impl Default for AudioCompressor {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_db: -12.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            makeup_db: 0.0,
        }
    }
}

impl AudioCompressor {
    pub fn new(
        enabled: bool,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_db: f32,
    ) -> Self {
        let compressor = Self {
            enabled,
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_db,
        };
        compressor.check();
        compressor
    }

    pub(crate) fn bypassed() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn threshold_db(&self) -> f32 {
        self.threshold_db
    }

    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    pub fn attack_ms(&self) -> f32 {
        self.attack_ms
    }

    pub fn release_ms(&self) -> f32 {
        self.release_ms
    }

    pub fn makeup_db(&self) -> f32 {
        self.makeup_db
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_threshold_db(&mut self, threshold_db: f32) {
        self.threshold_db = threshold_db;
        self.check();
    }

    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio;
        self.check();
    }

    pub fn set_attack_ms(&mut self, attack_ms: f32) {
        self.attack_ms = attack_ms;
        self.check();
    }

    pub fn set_release_ms(&mut self, release_ms: f32) {
        self.release_ms = release_ms;
        self.check();
    }

    pub fn set_makeup_db(&mut self, makeup_db: f32) {
        self.makeup_db = makeup_db;
        self.check();
    }

    fn check(&self) {
        assert!(
            self.threshold_db.is_finite() && (-60.0..=0.0).contains(&self.threshold_db),
            "the audio compressor holds the threshold {} decibels",
            self.threshold_db,
        );
        assert!(
            self.ratio.is_finite() && (1.0..=20.0).contains(&self.ratio),
            "the audio compressor holds the ratio {}",
            self.ratio,
        );
        assert!(
            self.attack_ms.is_finite() && (0.1..=1_000.0).contains(&self.attack_ms),
            "the audio compressor holds the attack {} milliseconds",
            self.attack_ms,
        );
        assert!(
            self.release_ms.is_finite() && (1.0..=5_000.0).contains(&self.release_ms),
            "the audio compressor holds the release {} milliseconds",
            self.release_ms,
        );
        assert!(
            self.makeup_db.is_finite() && (0.0..=24.0).contains(&self.makeup_db),
            "the audio compressor holds the makeup {} decibels",
            self.makeup_db,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioReverb {
    enabled: bool,
    room: f32,
    damping: f32,
    mix: f32,
}

impl Default for AudioReverb {
    fn default() -> Self {
        Self {
            enabled: true,
            room: 0.8,
            damping: 0.3,
            mix: 0.3,
        }
    }
}

impl AudioReverb {
    pub fn new(enabled: bool, room: f32, damping: f32, mix: f32) -> Self {
        let reverb = Self {
            enabled,
            room,
            damping,
            mix,
        };
        reverb.check();
        reverb
    }

    pub(crate) fn bypassed() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn room(&self) -> f32 {
        self.room
    }

    pub fn damping(&self) -> f32 {
        self.damping
    }

    pub fn mix(&self) -> f32 {
        self.mix
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_room(&mut self, room: f32) {
        self.room = room;
        self.check();
    }

    pub fn set_damping(&mut self, damping: f32) {
        self.damping = damping;
        self.check();
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix;
        self.check();
    }

    fn check(&self) {
        assert!(
            self.room.is_finite() && (0.0..=1.0).contains(&self.room),
            "the audio reverb holds the room {}",
            self.room,
        );
        assert!(
            self.damping.is_finite() && (0.0..=1.0).contains(&self.damping),
            "the audio reverb holds the damping {}",
            self.damping,
        );
        assert!(
            self.mix.is_finite() && (0.0..=1.0).contains(&self.mix),
            "the audio reverb holds the mix {}",
            self.mix,
        );
    }
}

pub(crate) fn decibels(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BusState<'a> {
    pub id: &'a str,
    pub gain_db: f32,
    pub mute: bool,
    pub send_db: f32,
}
