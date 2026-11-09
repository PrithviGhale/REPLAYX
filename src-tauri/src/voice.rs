//! Voice Enhancer: REPLAYX's own real-time microphone processing.
//!
//! Runs natively on each 10 ms chunk (480 frames at 48 kHz) between the
//! WASAPI capture timeline and the encoder, so recordings, the live
//! "Test mic" monitor and the original/enhanced comparison all hear exactly
//! the same processing. Order (see `STAGES`):
//!
//! high-pass → noise suppression (RNNoise) → expander → EQ → compressor →
//! de-esser → gain → look-ahead limiter
//!
//! * Noise suppression first, so EQ and compression never lift the noise
//!   before it is removed; the expander after it, keyed by RNNoise's voice
//!   probability so quiet speech is never gated away.
//! * RNNoise (Xiph/Mozilla, BSD-3, via the pure-Rust `nnnoiseless` port)
//!   delays its output by `RNNOISE_DELAY` samples; strengths below Strong mix
//!   a *delay-aligned* dry signal back in (no comb filtering).
//! * Built from documented techniques (RBJ biquads, feed-forward
//!   compressor, split-band de-esser, look-ahead peak limiter). No code,
//!   models or assets from other voice software.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const RATE: f32 = 48_000.0;
/// Samples per processing frame (RNNoise's frame, REPLAYX's audio chunk).
pub const FRAME: usize = 480;
/// RNNoise output delay in samples (measured: `rnnoise_delay_is_one_frame`).
pub const RNNOISE_DELAY: usize = 480;
/// Limiter look-ahead (1 ms) and ceiling (−1 dBFS sample peak).
const LOOKAHEAD: usize = 48;
const CEILING: f32 = 0.891;

/// Processing stages in order (documentation and a regression test).
pub const STAGES: [&str; 8] = [
    "high-pass",
    "noise suppression",
    "expander",
    "equalizer",
    "compressor",
    "de-esser",
    "gain",
    "limiter",
];

/// Microphone processing settings. Presets fill these fields; the processor
/// only reads fields, so a preset is never a cosmetic label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    /// "raw", "natural", "clear", "broadcast", "noise_free" or "custom".
    pub preset: String,
    pub high_pass: bool,
    /// "off", "light", "balanced" or "strong".
    pub suppression: String,
    pub gate: bool,
    pub gate_threshold_db: f32,
    pub compressor: bool,
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    /// Low shelf at 140 Hz (fullness), dB.
    pub warmth_db: f32,
    /// Peaking cut at 320 Hz (boxiness), dB (≤ 0).
    pub mud_db: f32,
    /// Peaking at 3.2 kHz (intelligibility), dB.
    pub presence_db: f32,
    /// High shelf at 10 kHz (air), dB.
    pub air_db: f32,
    pub deesser: bool,
}
impl Default for VoiceConfig {
    fn default() -> Self {
        preset("clear").unwrap()
    }
}
pub const PRESETS: [&str; 5] = ["raw", "natural", "clear", "broadcast", "noise_free"];
pub const SUPPRESSION: [&str; 4] = ["off", "light", "balanced", "strong"];

/// The settings for a built-in preset.
pub fn preset(name: &str) -> Option<VoiceConfig> {
    let base = VoiceConfig {
        preset: name.into(),
        high_pass: true,
        suppression: "balanced".into(),
        gate: false,
        gate_threshold_db: -50.0,
        compressor: false,
        comp_threshold_db: -24.0,
        comp_ratio: 2.5,
        warmth_db: 0.0,
        mud_db: 0.0,
        presence_db: 0.0,
        air_db: 0.0,
        deesser: false,
    };
    Some(match name {
        "raw" => VoiceConfig {
            high_pass: false,
            suppression: "off".into(),
            ..base
        },
        "natural" => VoiceConfig {
            suppression: "light".into(),
            ..base
        },
        "clear" => VoiceConfig {
            compressor: true,
            mud_db: -1.5,
            presence_db: 2.5,
            air_db: 1.0,
            ..base
        },
        "broadcast" => VoiceConfig {
            compressor: true,
            comp_threshold_db: -28.0,
            comp_ratio: 3.5,
            warmth_db: 1.5,
            mud_db: -2.5,
            presence_db: 3.0,
            air_db: 2.0,
            deesser: true,
            ..base
        },
        "noise_free" => VoiceConfig {
            suppression: "strong".into(),
            gate: true,
            gate_threshold_db: -48.0,
            compressor: true,
            presence_db: 1.5,
            ..base
        },
        _ => return None,
    })
}
impl VoiceConfig {
    /// Whether anything at all is applied (raw passes samples through).
    pub fn active(&self) -> bool {
        self.high_pass
            || self.suppression != "off"
            || self.gate
            || self.compressor
            || self.deesser
            || [self.warmth_db, self.mud_db, self.presence_db, self.air_db]
                .iter()
                .any(|d| d.abs() > 0.05)
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        let ok = (PRESETS.contains(&self.preset.as_str()) || self.preset == "custom")
            && SUPPRESSION.contains(&self.suppression.as_str())
            && (-70.0..=-20.0).contains(&self.gate_threshold_db)
            && (-50.0..=-6.0).contains(&self.comp_threshold_db)
            && (1.0..=10.0).contains(&self.comp_ratio)
            && (0.0..=6.0).contains(&self.warmth_db)
            && (-8.0..=0.0).contains(&self.mud_db)
            && (0.0..=8.0).contains(&self.presence_db)
            && (0.0..=6.0).contains(&self.air_db);
        if !ok {
            anyhow::bail!("Invalid voice settings")
        }
        Ok(())
    }
    /// Dry/wet mix for the noise-suppression strength (1 = full RNNoise).
    fn suppression_mix(&self) -> Option<f32> {
        match self.suppression.as_str() {
            "light" => Some(0.70),
            "balanced" => Some(0.90),
            "strong" => Some(1.0),
            _ => None,
        }
    }
}

fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}
fn to_db(v: f32) -> f32 {
    20.0 * v.max(1e-9).log10()
}
/// One-pole smoothing coefficient for a time constant.
fn coef(ms: f32) -> f32 {
    (-1.0 / (ms * 0.001 * RATE)).exp()
}

/// RBJ biquad, transposed direct form II.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}
enum Shape {
    HighPass,
    LowShelf,
    HighShelf,
    Peak,
}
impl Biquad {
    fn new(shape: Shape, freq: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * std::f64::consts::PI * freq / RATE as f64;
        let (s, c) = (w.sin(), w.cos());
        let alpha = s / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::HighPass => (
                (1.0 + c) / 2.0,
                -(1.0 + c),
                (1.0 + c) / 2.0,
                1.0 + alpha,
                -2.0 * c,
                1.0 - alpha,
            ),
            Shape::Peak => (
                1.0 + alpha * a,
                -2.0 * c,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * c,
                1.0 - alpha / a,
            ),
            Shape::LowShelf => {
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * c + k),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                    a * ((a + 1.0) - (a - 1.0) * c - k),
                    (a + 1.0) + (a - 1.0) * c + k,
                    -2.0 * ((a - 1.0) + (a + 1.0) * c),
                    (a + 1.0) + (a - 1.0) * c - k,
                )
            }
            Shape::HighShelf => {
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * c + k),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                    a * ((a + 1.0) + (a - 1.0) * c - k),
                    (a + 1.0) - (a - 1.0) * c + k,
                    2.0 * ((a - 1.0) - (a + 1.0) * c),
                    (a + 1.0) - (a - 1.0) * c - k,
                )
            }
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
    fn run(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y as f32
    }
}

/// Real-time processor for one microphone (mono, 48 kHz, 480-sample frames).
pub struct Processor {
    cfg: VoiceConfig,
    gain: f32,
    hpf: [Biquad; 2],
    eq: Vec<Biquad>,
    denoise: Option<Box<nnnoiseless::DenoiseState<'static>>>,
    dry: VecDeque<f32>,
    vad: f32,
    // Expander.
    gate_env: f32,
    gate_gain: f32,
    hold: usize,
    // Compressor.
    comp_env: f32,
    comp_gain_db: f32,
    makeup: f32,
    // De-esser.
    ess_hp: Biquad,
    ess_env: f32,
    // Limiter.
    delay: VecDeque<f32>,
    lim_gain: f32,
    // Level matching for original/enhanced comparisons.
    in_energy: f64,
    out_energy: f64,
    /// Frames whose output was not finite (processor reset, raw passed).
    pub faults: u64,
}
impl Processor {
    pub fn new(cfg: &VoiceConfig, gain_db: f32) -> Self {
        let mut p = Self {
            cfg: cfg.clone(),
            gain: db(gain_db.clamp(0.0, 24.0)),
            hpf: [Biquad::default(); 2],
            eq: Vec::new(),
            denoise: None,
            dry: VecDeque::new(),
            vad: 0.0,
            gate_env: 0.0,
            gate_gain: 1.0,
            hold: 0,
            comp_env: 0.0,
            comp_gain_db: 0.0,
            makeup: 1.0,
            ess_hp: Biquad::new(Shape::HighPass, 5500.0, 0.707, 0.0),
            ess_env: 0.0,
            delay: VecDeque::from(vec![0.0; LOOKAHEAD]),
            lim_gain: 1.0,
            in_energy: 0.0,
            out_energy: 0.0,
            faults: 0,
        };
        p.configure(cfg, gain_db);
        p
    }
    /// Apply new settings without losing filter state (live adjustments).
    pub fn configure(&mut self, cfg: &VoiceConfig, gain_db: f32) {
        // 4th-order Butterworth high-pass at 80 Hz (two cascaded sections).
        self.hpf = [
            Biquad::new(Shape::HighPass, 80.0, 0.541, 0.0),
            Biquad::new(Shape::HighPass, 80.0, 1.307, 0.0),
        ];
        let mut eq = Vec::new();
        if cfg.warmth_db.abs() > 0.05 {
            eq.push(Biquad::new(
                Shape::LowShelf,
                140.0,
                0.707,
                cfg.warmth_db as f64,
            ));
        }
        if cfg.mud_db.abs() > 0.05 {
            eq.push(Biquad::new(Shape::Peak, 320.0, 1.0, cfg.mud_db as f64));
        }
        if cfg.presence_db.abs() > 0.05 {
            eq.push(Biquad::new(
                Shape::Peak,
                3200.0,
                0.9,
                cfg.presence_db as f64,
            ));
        }
        if cfg.air_db.abs() > 0.05 {
            eq.push(Biquad::new(
                Shape::HighShelf,
                10_000.0,
                0.707,
                cfg.air_db as f64,
            ));
        }
        self.eq = eq;
        if cfg.suppression_mix().is_some() && self.denoise.is_none() {
            self.denoise = Some(nnnoiseless::DenoiseState::new());
            self.dry = VecDeque::from(vec![0.0; RNNOISE_DELAY]);
        }
        if cfg.suppression_mix().is_none() {
            self.denoise = None;
            self.dry.clear();
        }
        // Make-up gain restores part of what the compressor takes from speech.
        self.makeup = if cfg.compressor {
            db((-cfg.comp_threshold_db) * (1.0 - 1.0 / cfg.comp_ratio.max(1.0)) * 0.35)
        } else {
            1.0
        };
        self.gain = db(gain_db.clamp(0.0, 24.0));
        self.cfg = cfg.clone();
    }
    /// Samples of delay this configuration adds (RNNoise + limiter).
    pub fn latency(&self) -> usize {
        LOOKAHEAD
            + if self.denoise.is_some() {
                RNNOISE_DELAY
            } else {
                0
            }
    }
    /// Gain that makes the original signal as loud as the processed one
    /// (speech energy so far), for fair comparisons.
    pub fn match_gain(&self) -> f32 {
        if self.in_energy <= 0.0 || self.out_energy <= 0.0 {
            return 1.0;
        }
        ((self.out_energy / self.in_energy).sqrt() as f32).clamp(db(-12.0), db(18.0))
    }
    /// Process one frame in place. `frame` is mono, ±1.0 full scale.
    pub fn process(&mut self, frame: &mut [f32]) {
        // Never feed non-finite or absurd samples to the filters or RNNoise
        // (its FFT has undefined behaviour on NaN).
        if frame.iter().any(|v| !v.is_finite() || v.abs() > 8.0) {
            self.faults += 1;
            for v in frame.iter_mut() {
                *v = if v.is_finite() {
                    v.clamp(-8.0, 8.0)
                } else {
                    0.0
                };
            }
        }
        let original: Vec<f32> = frame.to_vec();
        self.run(frame);
        if frame.iter().any(|v| !v.is_finite()) {
            // Never pass garbage on: reset and use the raw frame.
            self.faults += 1;
            let (cfg, gain) = (self.cfg.clone(), to_db(self.gain));
            *self = Self {
                faults: self.faults,
                ..Self::new(&cfg, gain)
            };
            for (o, v) in frame.iter_mut().zip(&original) {
                *o = if v.is_finite() {
                    v.clamp(-CEILING, CEILING)
                } else {
                    0.0
                };
            }
        }
    }
    fn run(&mut self, frame: &mut [f32]) {
        let speech_in: f64 = frame.iter().map(|v| (*v as f64).powi(2)).sum();
        if self.cfg.high_pass {
            for s in frame.iter_mut() {
                let first = self.hpf[0].run(*s);
                *s = self.hpf[1].run(first);
            }
        }
        if let (Some(state), Some(mix)) = (self.denoise.as_mut(), self.cfg.suppression_mix()) {
            if frame.len() == FRAME {
                let input: Vec<f32> = frame.iter().map(|v| v * 32768.0).collect();
                let mut out = [0f32; FRAME];
                self.vad = state.process_frame(&mut out, &input);
                for (i, s) in frame.iter_mut().enumerate() {
                    // Dry path delayed to line up with RNNoise's output.
                    self.dry.push_back(*s);
                    let dry = self.dry.pop_front().unwrap_or(0.0);
                    let wet = out[i] / 32768.0;
                    *s = wet * mix + dry * (1.0 - mix);
                }
            }
        }
        if self.cfg.gate {
            let thr = self.cfg.gate_threshold_db;
            let (att, rel) = (coef(2.0), coef(100.0));
            let (open, close) = (coef(3.0), coef(150.0));
            for s in frame.iter_mut() {
                let e = s.abs();
                self.gate_env = if e > self.gate_env {
                    att * self.gate_env + (1.0 - att) * e
                } else {
                    rel * self.gate_env + (1.0 - rel) * e
                };
                let level = to_db(self.gate_env * 1.414);
                // Voice detected by RNNoise, or above threshold: open, and hold.
                if level > thr || self.vad > 0.6 {
                    self.hold = (0.2 * RATE) as usize;
                }
                let target = if self.hold > 0 {
                    self.hold -= 1;
                    1.0
                } else {
                    // 1:2 downward expansion, at most 12 dB.
                    db(((level - thr) * 1.0).max(-12.0))
                };
                let k = if target > self.gate_gain { open } else { close };
                self.gate_gain = k * self.gate_gain + (1.0 - k) * target;
                *s *= self.gate_gain;
            }
        }
        for f in self.eq.iter_mut() {
            for s in frame.iter_mut() {
                *s = f.run(*s);
            }
        }
        if self.cfg.compressor {
            let (thr, ratio, knee) = (
                self.cfg.comp_threshold_db,
                self.cfg.comp_ratio.max(1.0),
                6.0,
            );
            let det = coef(10.0);
            let (att, rel) = (coef(8.0), coef(150.0));
            for s in frame.iter_mut() {
                self.comp_env = det * self.comp_env + (1.0 - det) * *s * *s;
                let level = 10.0 * self.comp_env.max(1e-12).log10() + 3.0;
                let over = level - thr;
                let reduce = if over <= -knee / 2.0 {
                    0.0
                } else if over >= knee / 2.0 {
                    over * (1.0 - 1.0 / ratio)
                } else {
                    (1.0 - 1.0 / ratio) * (over + knee / 2.0).powi(2) / (2.0 * knee)
                };
                let k = if -reduce < self.comp_gain_db {
                    att
                } else {
                    rel
                };
                self.comp_gain_db = k * self.comp_gain_db + (1.0 - k) * -reduce;
                *s *= db(self.comp_gain_db) * self.makeup;
            }
        }
        if self.cfg.deesser {
            let (att, rel) = (coef(1.0), coef(60.0));
            for s in frame.iter_mut() {
                let hp = self.ess_hp.run(*s);
                let e = hp.abs();
                self.ess_env = if e > self.ess_env {
                    att * self.ess_env + (1.0 - att) * e
                } else {
                    rel * self.ess_env + (1.0 - rel) * e
                };
                let over = to_db(self.ess_env) - (-30.0);
                let g = if over > 0.0 {
                    db(-(over * 0.6).min(8.0))
                } else {
                    1.0
                };
                // y = low + g·high, with low = x − high: exact when g = 1.
                *s -= (1.0 - g) * hp;
            }
        }
        if (self.gain - 1.0).abs() > 1e-4 {
            for s in frame.iter_mut() {
                *s *= self.gain;
            }
        }
        // Look-ahead peak limiter: the gain is already down when a peak
        // leaves the delay line; a final clamp guarantees the ceiling.
        let (att, rel) = (coef(0.3), coef(60.0));
        for s in frame.iter_mut() {
            self.delay.push_back(*s);
            let peak = self.delay.iter().fold(0f32, |m, v| m.max(v.abs()));
            let target = if peak > CEILING { CEILING / peak } else { 1.0 };
            let k = if target < self.lim_gain { att } else { rel };
            self.lim_gain = (k * self.lim_gain + (1.0 - k) * target).min(1.0);
            if target < self.lim_gain {
                self.lim_gain = self.lim_gain.min(target.max(self.lim_gain * 0.9));
            }
            let out = self.delay.pop_front().unwrap_or(0.0) * self.lim_gain;
            *s = out.clamp(-CEILING, CEILING);
        }
        // Speech-weighted energies for level matching (ignore near silence).
        let speech_out: f64 = frame.iter().map(|v| (*v as f64).powi(2)).sum();
        if speech_in / FRAME as f64 > 1e-6 {
            self.in_energy += speech_in;
            self.out_energy += speech_out;
        }
    }
}

/// Process a whole mono recording (offline comparisons and evaluation).
/// Output is aligned with the input (processing latency removed).
pub fn process_buffer(cfg: &VoiceConfig, gain_db: f32, input: &[f32]) -> (Vec<f32>, f32) {
    let mut p = Processor::new(cfg, gain_db);
    let latency = if cfg.active() || gain_db > 0.05 {
        p.latency()
    } else {
        0
    };
    let mut out = Vec::with_capacity(input.len() + latency + FRAME);
    let mut padded = input.to_vec();
    padded.resize(input.len() + latency + FRAME, 0.0);
    for chunk in padded.chunks(FRAME) {
        let mut f = chunk.to_vec();
        f.resize(FRAME, 0.0);
        if cfg.active() || gain_db > 0.05 {
            p.process(&mut f);
        }
        out.extend_from_slice(&f);
    }
    let out: Vec<f32> = out.into_iter().skip(latency).take(input.len()).collect();
    (out, p.match_gain())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rms_db(x: &[f32]) -> f32 {
        to_db((x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt())
    }
    fn noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
        // Pinkish noise (fan-like): filtered white noise.
        let mut s = seed;
        let mut b = [0f32; 3];
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                let w = (s as f64 / u64::MAX as f64 * 2.0 - 1.0) as f32;
                b[0] = 0.997 * b[0] + 0.029 * w;
                b[1] = 0.985 * b[1] + 0.032 * w;
                b[2] = 0.950 * b[2] + 0.048 * w;
                (b[0] + b[1] + b[2] + 0.02 * w) * amp
            })
            .collect()
    }
    /// Voiced, speech-like test signal: a glottal-pulse train at a moving
    /// pitch through formant resonances, in syllables.
    fn voice(n: usize, level: f32) -> Vec<f32> {
        let mut f1 = Biquad::new(Shape::Peak, 700.0, 4.0, 18.0);
        let mut f2 = Biquad::new(Shape::Peak, 1200.0, 5.0, 14.0);
        let mut f3 = Biquad::new(Shape::Peak, 2600.0, 6.0, 10.0);
        let mut phase = 0.0f32;
        let raw: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / RATE;
                let pitch = 130.0 + 25.0 * (t * 2.1).sin();
                phase += pitch / RATE;
                let pulse = if phase.fract() < 0.08 { 1.0 } else { -0.087 };
                let syllable = ((t * 4.0 * std::f32::consts::PI).sin()).max(0.0).powf(0.7);
                f3.run(f2.run(f1.run(pulse))) * syllable
            })
            .collect();
        let k = db(level) / db(rms_db(&raw));
        raw.into_iter().map(|v| v * k).collect()
    }
    #[test]
    fn stage_order_is_documented() {
        assert_eq!(STAGES[0], "high-pass");
        assert_eq!(STAGES[1], "noise suppression");
        assert_eq!(*STAGES.last().unwrap(), "limiter");
    }
    #[test]
    fn presets_are_real_and_valid() {
        for p in PRESETS {
            let c = preset(p).unwrap();
            c.validate().unwrap();
            assert_eq!(c.preset, p);
        }
        assert!(!preset("raw").unwrap().active());
        assert_ne!(preset("clear"), preset("broadcast"));
        assert_eq!(VoiceConfig::default().preset, "clear");
        assert!(preset("nope").is_none());
    }
    #[test]
    fn raw_passes_samples_through_unchanged() {
        let x = voice(48_000, -25.0);
        let (y, _) = process_buffer(&preset("raw").unwrap(), 0.0, &x);
        assert_eq!(x, y);
    }
    #[test]
    fn rnnoise_delay_is_one_frame() {
        // Delay measured by cross-correlating a voiced signal with RNNoise's
        // output; the dry path in `Processor` relies on it.
        let x = voice(48_000, -20.0);
        let mut st = nnnoiseless::DenoiseState::new();
        let mut y = Vec::new();
        for c in x.chunks(FRAME) {
            let input: Vec<f32> = c.iter().map(|v| v * 32768.0).collect();
            let mut out = [0f32; FRAME];
            st.process_frame(&mut out, &input);
            y.extend(out.iter().map(|v| v / 32768.0));
        }
        let corr = |lag: usize| {
            (0..30_000)
                .map(|i| x[10_000 + i] * y[10_000 + i + lag])
                .sum::<f32>()
        };
        let best = (0..1200)
            .max_by(|a, b| corr(*a).total_cmp(&corr(*b)))
            .unwrap();
        assert_eq!(best, RNNOISE_DELAY);
    }
    #[test]
    fn suppression_removes_steady_noise_and_off_does_not() {
        let fan = noise(48_000 * 4, 0.02, 7);
        let base = rms_db(&fan[96_000..]);
        let at = |s: &str| {
            let cfg = VoiceConfig {
                suppression: s.into(),
                ..preset("raw").unwrap()
            };
            let (y, _) = process_buffer(&cfg, 0.0, &fan);
            base - rms_db(&y[96_000..])
        };
        let (off, light, balanced, strong) = (at("off"), at("light"), at("balanced"), at("strong"));
        assert!(off.abs() < 0.01, "off changes nothing: {off}");
        assert!(
            light > 6.0 && balanced > light && strong > balanced,
            "{light} {balanced} {strong}"
        );
        assert!(
            balanced > 12.0,
            "balanced removes at least 12 dB of fan noise: {balanced}"
        );
    }
    #[test]
    fn quiet_speech_survives_the_expander() {
        // Quiet voice (−42 dBFS RMS) with the Noise-Free gate.
        let x = voice(48_000 * 3, -42.0);
        let cfg = VoiceConfig {
            suppression: "off".into(),
            compressor: false,
            presence_db: 0.0,
            ..preset("noise_free").unwrap()
        };
        let (y, _) = process_buffer(&cfg, 0.0, &x);
        let loss = rms_db(&x[48_000..]) - rms_db(&y[48_000..]);
        assert!(loss < 3.0, "quiet speech lost {loss} dB");
    }
    #[test]
    fn boost_never_clips() {
        let x = voice(48_000, -6.0);
        for p in PRESETS {
            let (y, _) = process_buffer(&preset(p).unwrap(), 24.0, &x);
            let peak = y.iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!(peak <= CEILING + 1e-6, "{p}: {peak}");
        }
    }
    #[test]
    fn compressor_evens_out_loud_and_quiet_speech() {
        let mut x = voice(48_000 * 2, -36.0);
        x.extend(voice(48_000 * 2, -12.0));
        let cfg = preset("broadcast").unwrap();
        let (y, _) = process_buffer(
            &VoiceConfig {
                suppression: "off".into(),
                ..cfg
            },
            0.0,
            &x,
        );
        let spread_in = rms_db(&x[120_000..192_000]) - rms_db(&x[24_000..96_000]);
        let spread_out = rms_db(&y[120_000..192_000]) - rms_db(&y[24_000..96_000]);
        assert!(spread_out < spread_in - 8.0, "{spread_in} -> {spread_out}");
    }
    #[test]
    fn level_match_gain_tracks_the_processing() {
        let x = voice(48_000 * 2, -30.0);
        let (y, g) = process_buffer(&preset("broadcast").unwrap(), 6.0, &x);
        let diff = rms_db(&y[24_000..]) - (rms_db(&x[24_000..]) + to_db(g));
        assert!(diff.abs() < 1.5, "matched within 1.5 dB: {diff}");
    }
    #[test]
    fn a_frame_is_processed_far_inside_its_10_ms_budget() {
        // Even in an unoptimised test build (about 2.5 ms; release is ~0.06 ms).
        // Measured as this thread's CPU time, best of three: the test suite
        // runs in parallel, and wall-clock time would also count the time
        // spent waiting for a free core.
        fn thread_cpu_ms() -> f64 {
            use windows::Win32::{
                Foundation::FILETIME,
                System::Threading::{GetCurrentThread, GetThreadTimes},
            };
            let mut t = [FILETIME::default(); 4];
            let [created, exited, kernel, user] = &mut t;
            unsafe { GetThreadTimes(GetCurrentThread(), created, exited, kernel, user) }.unwrap();
            let ms = |f: &FILETIME| {
                (((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64) as f64 / 10_000.0
            };
            ms(&t[2]) + ms(&t[3])
        }
        let x = voice(48_000, -24.0);
        let mut p = Processor::new(&preset("broadcast").unwrap(), 6.0);
        let per_frame = (0..3)
            .map(|_| {
                let start = thread_cpu_ms();
                for c in x.chunks(FRAME) {
                    let mut f = c.to_vec();
                    p.process(&mut f);
                }
                (thread_cpu_ms() - start) / 100.0
            })
            .fold(f64::INFINITY, f64::min);
        assert!(per_frame < 5.0, "{per_frame:.3} ms of CPU per 10 ms frame");
        assert_eq!(
            p.latency(),
            RNNOISE_DELAY + LOOKAHEAD,
            "about 11 ms added for the microphone track"
        );
    }
    #[test]
    fn processor_survives_garbage_input() {
        let mut p = Processor::new(&preset("broadcast").unwrap(), 6.0);
        let mut f = vec![f32::NAN; FRAME];
        p.process(&mut f);
        assert!(f.iter().all(|v| v.is_finite()));
        let mut ok = voice(FRAME, -20.0);
        p.process(&mut ok);
        assert!(ok.iter().all(|v| v.is_finite()));
    }
}
